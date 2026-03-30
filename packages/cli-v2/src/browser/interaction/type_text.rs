use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Type text character by character
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser type \"#search\" \"hello world\" --session s1 --tab t1
  actionbook browser type \"textarea\" \"line one\\nline two\" --session s1 --tab t1

Types each character individually, firing keydown/keypress/keyup events.
Use for fields with autocomplete, live validation, or input listeners.
For simple value setting without events, use fill instead.")]
pub struct Cmd {
    /// Target element selector
    pub selector: String,
    /// Text to type
    pub text: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.type";

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
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve and focus the target element
    let node_id = match element::resolve_node(
        &cdp,
        &target_id,
        &cmd.selector,
        registry,
        &cmd.session,
        &cmd.tab,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => return e,
    };

    if let Err(e) = cdp
        .execute_on_tab(&target_id, "DOM.focus", json!({ "nodeId": node_id }))
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Move cursor to end of existing value so typed text appends
    let _ = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({
                "expression": "(() => { const el = document.activeElement; if (el && el.setSelectionRange) el.setSelectionRange(el.value.length, el.value.length); })()",
            }),
        )
        .await;

    // Type each character: keyDown (with text insertion) + keyUp
    for ch in cmd.text.chars() {
        let key = ch.to_string();

        if let Err(e) = cdp
            .execute_on_tab(
                &target_id,
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyDown",
                    "key": key,
                    "text": key,
                }),
            )
            .await
        {
            return cdp_error_to_result(e, "CDP_ERROR");
        }

        if let Err(e) = cdp
            .execute_on_tab(
                &target_id,
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyUp",
                    "key": key,
                }),
            )
            .await
        {
            return cdp_error_to_result(e, "CDP_ERROR");
        }
    }

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "action": "type",
        "target": { "selector": cmd.selector },
        "value_summary": { "text_length": cmd.text.chars().count() },
        "post_url": url,
        "post_title": title,
    }))
}
