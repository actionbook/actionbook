use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// List tabs in a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser list-tabs --session my-session
  actionbook browser list-tabs --session my-session --json

Returns each tab's ID (t1, t2, ...), URL, and title.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser list-tabs";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    match result {
        ActionResult::Ok { .. } => Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: None,
            window_id: None,
            url: None,
            title: None,
        }),
        _ => None,
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Get CdpSession from registry
    let cdp = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(e) => match e.cdp.clone() {
                Some(c) => c,
                None => {
                    return ActionResult::fatal_with_hint(
                        "INTERNAL_ERROR",
                        format!("no CDP connection for session '{}'", cmd.session),
                        "try restarting the session",
                    );
                }
            },
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        }
    };

    // Real-time fetch via CDP Target.getTargets (works for both local and cloud)
    let resp = match cdp.execute_browser("Target.getTargets", json!({})).await {
        Ok(r) => r,
        Err(e) => return cdp_error_to_result(e, "CDP_CONNECTION_FAILED"),
    };

    let target_infos = resp
        .pointer("/result/targetInfos")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let live_pages: Vec<(&str, &str, &str)> = target_infos
        .iter()
        .filter(|tgt| tgt.get("type").and_then(|v| v.as_str()) == Some("page"))
        .map(|tgt| {
            let native_id = tgt.get("targetId").and_then(|v| v.as_str()).unwrap_or("");
            let url = tgt.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let title = tgt.get("title").and_then(|v| v.as_str()).unwrap_or("");
            (native_id, url, title)
        })
        .collect();

    // Sync registry with live CDP state:
    // - Matching native_id → keep short tab ID, update url/title
    // - Stale registry tabs (not in CDP) → remove
    // - New CDP tabs (not in registry) → assign new short ID
    let (tabs, to_attach): (Vec<serde_json::Value>, Vec<String>) = {
        let mut reg = registry.lock().await;
        let entry = match reg.get_mut(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };

        // Remove stale tabs whose native_id no longer exists in CDP
        entry
            .tabs
            .retain(|t| live_pages.iter().any(|(nid, _, _)| *nid == t.native_id));

        let mut result = Vec::new();
        let mut to_attach = Vec::new();
        for (native_id, url, title) in &live_pages {
            // Find existing short ID or assign a new one
            if let Some(existing) = entry.tabs.iter_mut().find(|t| t.native_id == *native_id) {
                // Update url/title from live CDP data
                existing.url = url.to_string();
                existing.title = title.to_string();
                result.push(json!({
                    "tab_id": existing.id.0,
                    "native_tab_id": native_id,
                    "url": url,
                    "title": title,
                }));
            } else {
                // New tab — assign next short ID and mark for CDP attach
                entry.push_tab(native_id.to_string(), url.to_string(), title.to_string());
                let new_tab = entry.tabs.last().unwrap();
                to_attach.push(native_id.to_string());
                result.push(json!({
                    "tab_id": new_tab.id.0,
                    "native_tab_id": native_id,
                    "url": url,
                    "title": title,
                }));
            }
        }
        (result, to_attach)
    };

    // Attach newly discovered tabs outside the registry lock
    for native_id in &to_attach {
        if let Err(e) = cdp.attach(native_id, None).await {
            tracing::warn!("failed to attach discovered tab {native_id}: {e}");
        }
    }

    ActionResult::ok(json!({
        "total_tabs": tabs.len(),
        "tabs": tabs,
    }))
}
