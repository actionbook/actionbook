use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::TabContext;
use crate::browser::navigation;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Directly set the value of an input field
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser fill \"#email\" \"user@example.com\" --session s1 --tab t1
  actionbook browser fill @e4 \"search query\" --session s1 --tab t1

Accepts a CSS selector, XPath, or snapshot ref (@eN from snapshot output).
Sets the value instantly (no per-character events). Use for standard inputs.
For fields that need keystroke events (autocomplete, validation), use type instead.")]
pub struct Cmd {
    /// Selector (CSS, XPath, or @ref)
    pub selector: String,
    /// Value to fill
    pub value: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser fill";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("post_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            data.get("post_title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
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

    // Resolve the target element
    let node_id = match ctx.resolve_node(&cmd.selector).await {
        Ok(id) => id,
        Err(e) => return e,
    };

    // Focus the element
    if let Err(e) = ctx
        .execute_on_element("DOM.focus", json!({ "nodeId": node_id }))
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Resolve to objectId so we can use callFunctionOn (works across frames,
    // unlike document.activeElement which stays in the top-level context).
    let object_id = match ctx.resolve_object_id(node_id).await {
        Ok(oid) => oid,
        Err(e) => return e,
    };

    // Set value directly via JS and dispatch an input event (no key events)
    let value_json = serde_json::to_string(&cmd.value).unwrap_or_default();
    let fill_fn = format!(
        r#"function() {{
            const proto = this instanceof HTMLTextAreaElement
                ? HTMLTextAreaElement.prototype
                : HTMLInputElement.prototype;
            const nativeSet = Object.getOwnPropertyDescriptor(proto, 'value');
            if (nativeSet && nativeSet.set) {{
                nativeSet.set.call(this, {value_json});
            }} else {{
                this.value = {value_json};
            }}
            this.dispatchEvent(new Event('input', {{ bubbles: true }}));
            return 'ok';
        }}"#
    );

    let resp = match ctx
        .execute_on_element(
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": fill_fn,
                "returnByValue": true,
            }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return cdp_error_to_result(e, "CDP_ERROR"),
    };

    let result_str = resp
        .pointer("/result/result/value")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if result_str != "ok" {
        return ActionResult::fatal("CDP_ERROR", format!("fill failed: {result_str}"));
    }

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "action": "fill",
        "target": { "selector": cmd.selector },
        "value_summary": { "text_length": cmd.value.chars().count() },
        "post_url": url,
        "post_title": title,
    }))
}
