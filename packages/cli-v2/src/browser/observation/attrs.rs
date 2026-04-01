use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element::TabContext, navigation};
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Read all attributes on an element.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser attrs \"#logo\" --session s1 --tab t1
  actionbook browser attrs @e5 --session s1 --tab t1

Accepts a CSS selector, XPath, or snapshot ref (@eN from snapshot output).
Returns all HTML attributes as key-value pairs.
Use attr to read a single attribute by name.")]
pub struct Cmd {
    /// Selector (CSS, XPath, or @ref)
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

pub const COMMAND_NAME: &str = "browser attrs";

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
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let (_, object_id) = match ctx.resolve_object(&cmd.selector).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let value = match get_attributes(&ctx, &object_id).await {
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

async fn get_attributes(ctx: &TabContext, object_id: &str) -> Result<Value, ActionResult> {
    let resp = ctx
        .execute_on_element(
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

    if resp.pointer("/result/exceptionDetails").is_some() {
        let description = resp
            .pointer("/result/exceptionDetails/exception/description")
            .and_then(|v| v.as_str())
            .unwrap_or("JS exception during attribute read");
        return Err(ActionResult::fatal("JS_EXCEPTION", description.to_string()));
    }

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
