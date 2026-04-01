use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::StorageKind;

/// Set a key-value entry in a Web Storage object
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Storage key
    #[arg()]
    pub key: String,
    /// Storage value
    #[arg()]
    pub value: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Storage kind (injected by CLI router, not a CLI flag)
    #[arg(skip)]
    pub kind: StorageKind,
}

pub fn command_name(kind: StorageKind) -> &'static str {
    match kind {
        StorageKind::Local => "browser local-storage set",
        StorageKind::Session => "browser session-storage set",
    }
}

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result {
        if code == "SESSION_NOT_FOUND" {
            return None;
        }
        if code == "TAB_NOT_FOUND" {
            return Some(ResponseContext {
                session_id: cmd.session.clone(),
                tab_id: None,
                window_id: None,
                url: None,
                title: None,
            });
        }
    }
    let url = if let ActionResult::Ok { data } = result {
        data.get("__url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
    } else {
        None
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let key_json = serde_json::to_string(&cmd.key).unwrap_or_default();
    let val_json = serde_json::to_string(&cmd.value).unwrap_or_default();
    let js = format!(
        "{}.setItem({}, {}); 1",
        cmd.kind.js_object(),
        key_json,
        val_json
    );

    match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": js, "returnByValue": true }),
        )
        .await
    {
        Ok(_) => {}
        Err(e) => return ActionResult::fatal("CDP_ERROR", e.to_string()),
    }

    let url = crate::browser::navigation::get_tab_url(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "storage": cmd.kind.data_name(),
        "action": "set",
        "affected": 1,
        "__url": url,
    }))
}
