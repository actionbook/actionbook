use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element, element::TabContext, navigation};
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Read element or page HTML
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser html --session s1 --tab t1
  actionbook browser html \"#main\" --session s1 --tab t1
  actionbook browser html @e3 --session s1 --tab t1

Accepts a CSS selector, XPath, or snapshot ref (@eN from snapshot output).
Without a selector, returns the full page HTML.
With a selector, returns the outer HTML of the matched element.")]
pub struct Cmd {
    /// Selector (CSS, XPath, or @ref). Omit to read the full page HTML.
    pub selector: Option<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser html";

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
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("__ctx_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("__ctx_title")
                .and_then(|v| v.as_str())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let value = match get_html(&mut ctx, cmd.selector.as_deref()).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "target": { "selector": cmd.selector },
        "value": value,
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}

async fn get_html(ctx: &mut TabContext, selector: Option<&str>) -> Result<Value, ActionResult> {
    match selector {
        Some(selector) => {
            let (_, object_id) = ctx.resolve_object(selector).await?;
            let resp = ctx
                .execute_on_element(
                    "Runtime.callFunctionOn",
                    json!({
                        "objectId": object_id,
                        "functionDeclaration": r#"function() { return this.outerHTML; }"#,
                        "returnByValue": true,
                    }),
                )
                .await
                .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

            let value = resp
                .pointer("/result/result/value")
                .cloned()
                .unwrap_or(Value::Null);
            if value.is_null() {
                Err(element::element_not_found(selector))
            } else {
                Ok(value)
            }
        }
        None => {
            let resp = ctx
                .cdp
                .execute_on_tab(
                    &ctx.target_id,
                    "Runtime.evaluate",
                    json!({
                        "expression": "document.documentElement.outerHTML",
                        "returnByValue": true,
                    }),
                )
                .await
                .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

            Ok(resp
                .pointer("/result/result/value")
                .cloned()
                .unwrap_or(Value::Null))
        }
    }
}
