use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Read a named attribute from an element
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser attr \"a.link\" href --session s1 --tab t1
  actionbook browser attr \"#email\" aria-label --session s1 --tab t1
  actionbook browser attr \"img\" src --session s1 --tab t1")]
pub struct Cmd {
    /// Target element selector
    pub selector: String,
    /// Attribute name to read
    pub name: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.attr";

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
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let value = match get_attr(
        &cdp,
        &target_id,
        &cmd.selector,
        &cmd.name,
        registry,
        &cmd.session,
        &cmd.tab,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return e,
    };
    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "target": { "selector": cmd.selector },
        "value": value,
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}

async fn get_attr(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
    selector: &str,
    attr_name: &str,
    registry: &SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<Value, ActionResult> {
    let (_, object_id) =
        element::resolve_selector_object(cdp, target_id, selector, registry, session_id, tab_id)
            .await?;
    let attr_json = serde_json::to_string(attr_name).map_err(|e| {
        ActionResult::fatal("INTERNAL_ERROR", format!("serialize attribute name: {e}"))
    })?;
    let function = format!(r#"function() {{ return this.getAttribute({attr_json}); }}"#);

    let resp = cdp
        .execute_on_tab(
            target_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": function,
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
