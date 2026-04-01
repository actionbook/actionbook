use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::StorageKind;

/// Clear the value for a specific key in a Web Storage object
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Storage key to clear
    #[arg()]
    pub key: String,
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
        StorageKind::Local => "browser local-storage clear",
        StorageKind::Session => "browser session-storage clear",
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
    // Same mechanics as delete but returns action: "clear".
    let js = format!(
        "(function(){{ var s={}; var had=s.getItem({})!==null?1:0; s.removeItem({}); return had; }})()",
        cmd.kind.js_object(),
        key_json,
        key_json
    );

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": js, "returnByValue": true }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return ActionResult::fatal("CDP_ERROR", e.to_string()),
    };

    let affected = resp
        .pointer("/result/result/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let url = crate::browser::navigation::get_tab_url(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "storage": cmd.kind.data_name(),
        "action": "clear",
        "affected": affected,
        "__url": url,
    }))
}
