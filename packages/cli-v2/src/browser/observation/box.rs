use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Read an element's bounding client rect.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Target element selector
    pub selector: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.box";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let url = match result {
        ActionResult::Ok { data } => data
            .get("__ctx_url")
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
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

    let (node_id, object_id) =
        match element::resolve_selector_object(&cdp, &target_id, &cmd.selector).await {
            Ok(v) => v,
            Err(e) => return e,
        };

    if let Err(e) = element::scroll_into_view(&cdp, &target_id, node_id).await {
        return e;
    }

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let value = match get_box(&cdp, &target_id, &object_id).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    ActionResult::ok(json!({
        "target": { "selector": cmd.selector },
        "value": value,
        "__ctx_url": url,
    }))
}

async fn get_box(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
    object_id: &str,
) -> Result<Value, ActionResult> {
    let resp = cdp
        .execute_on_tab(
            target_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": r#"function() {
                    const rect = this.getBoundingClientRect();
                    return {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                        right: rect.right,
                        bottom: rect.bottom
                    };
                }"#,
                "returnByValue": true,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    Ok(resp
        .pointer("/result/result/value")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new())))
}

pub const BOX_FIELDS: [&str; 6] = ["x", "y", "width", "height", "right", "bottom"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_fields_match_contract_order() {
        assert_eq!(BOX_FIELDS, ["x", "y", "width", "height", "right", "bottom"]);
    }
}
