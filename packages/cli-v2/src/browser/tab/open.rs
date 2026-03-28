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
    let client = reqwest::Client::new();
    let resp = client
        .put(&create_url)
        .send()
        .await
        .map_err(|e| {
            ActionResult::fatal(
                "CDP_ERROR",
                format!("failed to create tab via /json/new: {e}"),
            )
        });
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return e,
    };
    let body = resp.text().await.unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    let target_id = match v.get("id").and_then(|i| i.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return ActionResult::fatal(
                "CDP_ERROR",
                format!("Chrome /json/new did not return target id, body: {body}"),
            );
        }
    };
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let cdp = {
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
        entry.tabs.push(TabEntry {
            id: TabId(target_id.clone()),
            url: final_url.clone(),
            title: title.clone(),
        });
        entry.cdp.clone()
    };

    // Attach the new tab to the persistent CDP session
    if let Some(ref cdp) = cdp {
        if let Err(e) = cdp.attach(&target_id).await {
            return ActionResult::fatal(
                "CDP_ERROR",
                format!("failed to attach tab to CDP session: {e}"),
            );
        }
    }

    ActionResult::ok(json!({
        "tab": {
            "tab_id": target_id,
            "url": final_url,
            "title": title,
        },
        "created": true,
        "new_window": cmd.new_window,
    }))
}
