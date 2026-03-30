use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Select a value from a dropdown list
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser select \"#country\" \"us\" --session s1 --tab t1
  actionbook browser select \"#country\" \"United States\" --by-text --session s1 --tab t1

Selects an option in a <select> element by its value attribute.
Use --by-text to match the visible display text instead.")]
pub struct Cmd {
    /// Target `<select>` element selector
    pub selector: String,
    /// Value to select
    pub value: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Match by display text instead of value attribute
    #[arg(long)]
    #[serde(default)]
    pub by_text: bool,
}

pub const COMMAND_NAME: &str = "browser.select";

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

    // Resolve the target element via shared resolver (CSS, XPath, future @eN)
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

    // Convert nodeId to a remote JS object for callFunctionOn
    let resolve_resp = match cdp
        .execute_on_tab(&target_id, "DOM.resolveNode", json!({ "nodeId": node_id }))
        .await
    {
        Ok(v) => v,
        Err(e) => return cdp_error_to_result(e, "CDP_ERROR"),
    };

    let object_id = match resolve_resp
        .pointer("/result/object/objectId")
        .and_then(|v| v.as_str())
    {
        Some(id) => id.to_string(),
        None => return ActionResult::fatal("CDP_ERROR", "could not resolve element to JS object"),
    };

    // Select the option by value or by visible text
    let value_json = serde_json::to_string(&cmd.value).unwrap_or_default();
    let by_text = cmd.by_text;

    let fn_decl = format!(
        r#"function() {{
            if (this.tagName !== 'SELECT') return 'not a select element';
            const opts = Array.from(this.options);
            const opt = {by_text}
                ? opts.find(o => o.textContent.trim() === {value_json})
                : opts.find(o => o.value === {value_json});
            if (!opt) return 'option not found';
            this.value = opt.value;
            this.dispatchEvent(new Event('input', {{ bubbles: true }}));
            this.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'ok';
        }}"#
    );

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": fn_decl,
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

    match result_str {
        "ok" => {}
        "option not found" => {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                format!("option not found: '{}'", cmd.value),
            );
        }
        other => {
            return ActionResult::fatal("CDP_ERROR", format!("select failed: {other}"));
        }
    }

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "action": "select",
        "target": { "selector": cmd.selector },
        "value_summary": {
            "value": cmd.value,
            "by_text": cmd.by_text,
        },
        "post_url": url,
        "post_title": title,
    }))
}
