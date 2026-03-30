use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::StorageKind;

/// Get a single entry from a Web Storage object by key
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Storage key
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
        StorageKind::Local => "browser.local-storage.get",
        StorageKind::Session => "browser.session-storage.get",
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
    let js = format!(
        "(function(){{ var v={}.getItem({}); return v; }})()",
        cmd.kind.js_object(),
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

    let url = crate::browser::navigation::get_tab_url(&cdp, &target_id).await;

    // getItem returns null (JS null) when key is missing
    let js_type = resp
        .pointer("/result/result/type")
        .and_then(|v| v.as_str())
        .unwrap_or("undefined");

    let is_null = js_type == "undefined"
        || (js_type == "object"
            && resp
                .pointer("/result/result/subtype")
                .and_then(|v| v.as_str())
                == Some("null"));

    let item: Value = if is_null {
        Value::Null
    } else {
        let val = resp
            .pointer("/result/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        json!({ "key": cmd.key, "value": val })
    };

    ActionResult::ok(json!({
        "storage": cmd.kind.data_name(),
        "item": item,
        "__url": url,
    }))
}
