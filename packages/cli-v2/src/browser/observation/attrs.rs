use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Read all attributes on an element.
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

pub const COMMAND_NAME: &str = "browser.attrs";

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

    let (_, object_id) =
        match element::resolve_selector_object(&cdp, &target_id, &cmd.selector).await {
            Ok(v) => v,
            Err(e) => return e,
        };

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let value = match get_attributes(&cdp, &target_id, &object_id).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let attr_order = attribute_order(&value);

    ActionResult::ok(json!({
        "target": { "selector": cmd.selector },
        "value": value,
        "__attr_order": attr_order,
        "__ctx_url": url,
    }))
}

async fn get_attributes(
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
                    const attrs = {};
                    for (const attr of this.attributes) {
                        attrs[attr.name] = attr.value;
                    }
                    return attrs;
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

pub fn attribute_order(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_order_sorts_keys_for_text_output() {
        let attrs = json!({
            "title": "Card",
            "aria-label": "Profile Card",
            "data-testid": "profile-card",
        });

        assert_eq!(
            attribute_order(&attrs),
            vec![
                "aria-label".to_string(),
                "data-testid".to_string(),
                "title".to_string()
            ]
        );
    }
}
