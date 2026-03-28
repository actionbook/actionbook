use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::ensure_scheme;
use crate::daemon::registry::{SharedRegistry, TabEntry};
use crate::output::ResponseContext;
use crate::types::TabId;

/// Open a new tab
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// URL to open
    pub url: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Open in new window
    #[arg(long)]
    pub new_window: bool,
    /// Window ID
    #[arg(long)]
    pub window: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.new-tab";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    // Special case like browser start: returns context with newly created tab_id
    if let ActionResult::Ok { data } = result {
        Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: data["tab"]["tab_id"].as_str().map(|s| s.to_string()),
            window_id: None,
            url: data["tab"]["url"].as_str().map(|s| s.to_string()),
            title: data["tab"]["title"].as_str().map(|s| s.to_string()),
        })
    } else {
        None
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let final_url = ensure_scheme(&cmd.url);

    let cdp_port = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(e) => e.cdp_port,
            None => {
                return ActionResult::fatal(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                );
            }
        }
    };

    let create_url = format!(
        "http://127.0.0.1:{}/json/new?{}",
        cdp_port,
        urlencoding::encode(&final_url)
    );
    let (target_id, title) = match reqwest::get(&create_url).await {
        Ok(resp) => {
            let v = resp
                .json::<serde_json::Value>()
                .await
                .unwrap_or(serde_json::Value::Null);
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
            let t = v
                .get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
            (id, t)
        }
        Err(_) => (String::new(), String::new()),
    };

    let tab_id = {
        let mut reg = registry.lock().await;
        let entry = match reg.get_mut(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                );
            }
        };
        let tid = TabId(entry.next_tab_id);
        entry.next_tab_id += 1;
        entry.tabs.push(TabEntry {
            id: tid,
            target_id: target_id.clone(),
            url: final_url.clone(),
            title: title.clone(),
        });
        tid
    };

    let native_tab_id: serde_json::Value = if target_id.is_empty() {
        serde_json::Value::Null
    } else {
        json!(target_id)
    };

    ActionResult::ok(json!({
        "tab": {
            "tab_id": tab_id.to_string(),
            "url": final_url,
            "title": title,
            "native_tab_id": native_tab_id,
        },
        "created": true,
        "new_window": cmd.new_window,
    }))
}
