use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element::TabContext, element::element_not_found, navigation};
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Get element state (visible, enabled, checked, focused, editable, selected).
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

pub const COMMAND_NAME: &str = "browser.state";

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
    let ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let (_, object_id) = match ctx.resolve_object(&cmd.selector).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    let resp = ctx
        .cdp
        .execute_on_tab(
            &ctx.target_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": r#"function() {
                    var rect = this.getBoundingClientRect();
                    var style = window.getComputedStyle(this);
                    var tag = this.tagName.toLowerCase();
                    var isTextInput = (tag === 'input' && !(/^(checkbox|radio|button|submit|reset|file|image|hidden|range|color)$/i.test(this.type || 'text'))) || tag === 'textarea';
                    var editable = !this.disabled && !this.readOnly && (isTextInput || this.isContentEditable);
                    return {
                        visible: rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none',
                        enabled: !this.disabled,
                        checked: !!this.checked,
                        focused: document.activeElement === this,
                        editable: editable,
                        selected: !!this.selected
                    };
                }"#,
                "returnByValue": true,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"));

    let resp = match resp {
        Ok(v) => v,
        Err(e) => return e,
    };

    if resp.pointer("/result/exceptionDetails").is_some() {
        let description = resp
            .pointer("/result/exceptionDetails/exception/description")
            .and_then(|v| v.as_str())
            .unwrap_or("JS exception during state read");
        return ActionResult::fatal("JS_EXCEPTION", description.to_string());
    }

    let val = resp
        .pointer("/result/result/value")
        .cloned()
        .unwrap_or(Value::Null);

    if val.is_null() {
        return element_not_found(&cmd.selector);
    }

    ActionResult::ok(json!({
        "target": { "selector": cmd.selector },
        "state": val,
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}
