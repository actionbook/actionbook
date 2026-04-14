use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::TabContext;
use crate::browser::navigation;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Select a value from a dropdown list
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser select \"#country\" \"us\" --session s1 --tab t1
  actionbook browser select @e7 \"United States\" --by-text --session s1 --tab t1
  actionbook browser select \"#country\" @e12 --by-ref --session s1 --tab t1

Accepts a CSS selector, XPath, or snapshot ref (@eN from snapshot output).
Selects an option in a <select> element by its value attribute.
Use --by-text to match the visible display text instead.
Use --by-ref to select an option by its snapshot ref (@eN).")]
pub struct Cmd {
    /// Selector for <select> element (CSS, XPath, or @ref)
    pub selector: String,
    /// Value to select (option value, display text with --by-text, or @ref with --by-ref)
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
    /// Match by snapshot ref (@eN) instead of value attribute
    #[arg(long)]
    #[serde(default)]
    pub by_ref: bool,
}

pub const COMMAND_NAME: &str = "browser select";

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
    if cmd.by_text && cmd.by_ref {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            "--by-text and --by-ref are mutually exclusive",
        );
    }

    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve the target element via shared resolver (CSS, XPath, @eN)
    let (node_id, object_id) = match ctx.resolve_object(&cmd.selector).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Scroll element to viewport center before operating
    if let Err(e) = ctx.scroll_into_view(node_id).await {
        return e;
    }

    // Build JS function + arguments based on mode
    let (fn_decl, call_args) = if cmd.by_ref {
        // Resolve option ref → pass the element directly via CDP arguments
        let (_opt_node_id, opt_object_id) = match ctx.resolve_object(&cmd.value).await {
            Ok(v) => v,
            Err(e) => return e,
        };
        (
            r#"function(optEl) {
                if (this.tagName !== 'SELECT') return 'not a select element';
                if (!optEl || optEl.tagName !== 'OPTION') return 'not an option element';
                if (!Array.from(this.options).includes(optEl)) return 'option not in this select';
                this.value = optEl.value;
                this.dispatchEvent(new Event('input', { bubbles: true }));
                this.dispatchEvent(new Event('change', { bubbles: true }));
                return 'ok';
            }"#
            .to_string(),
            json!([{ "objectId": opt_object_id }]),
        )
    } else {
        let value_json = serde_json::to_string(&cmd.value).unwrap_or_default();
        let by_text = cmd.by_text;
        (
            format!(
                r#"function() {{
                    if (this.tagName !== 'SELECT') return 'not a select element';
                    const opts = Array.from(this.options);
                    const opt = {by_text}
                        ? opts.find(o => o.textContent.trim() === {value_json})
                        : opts.find(o => o.value === {value_json});
                    if (!opt) {{
                        const MAX = 20;
                        const values = opts.slice(0, MAX).map(o => o.value);
                        const texts = opts.slice(0, MAX).map(o => o.textContent.trim());
                        return JSON.stringify({{ status: 'option not found', mode: {by_text} ? 'by-text' : 'by-value', total: opts.length, values, texts }});
                    }}
                    this.value = opt.value;
                    this.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    this.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    return 'ok';
                }}"#
            ),
            json!([]),
        )
    };

    let resp = match ctx
        .execute_on_element(
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": fn_decl,
                "arguments": call_args,
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
            // Fallback: JS now returns JSON; this arm kept as defensive safety net.
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                format!("option not found: '{}'", cmd.value),
            );
        }
        other if other.starts_with('{') => {
            if let Ok(diag) = serde_json::from_str::<serde_json::Value>(other)
                && diag["status"].as_str() == Some("option not found")
            {
                let mode = diag["mode"].as_str().unwrap_or("by-value").to_string();
                let total = diag["total"].as_u64().unwrap_or(0);
                let values = diag["values"].clone();
                let texts = diag["texts"].clone();
                let message = format!(
                    "option not found: '{}'. Mode: {}. Total options: {}. Values: {}. Texts: {}",
                    cmd.value, mode, total, values, texts,
                );
                return ActionResult::fatal_with_details(
                    "INVALID_ARGUMENT",
                    message,
                    "check the available values and texts above, or use --by-text to match display text",
                    json!({
                        "status": "option not found",
                        "mode": mode,
                        "total": total,
                        "values": values,
                        "texts": texts,
                    }),
                );
            }
            return ActionResult::fatal("CDP_ERROR", format!("select failed: {other}"));
        }
        "not an option element" => {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                format!("ref '{}' does not point to an <option> element", cmd.value),
            );
        }
        "option not in this select" => {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                format!(
                    "option '{}' is not in the target <select> element",
                    cmd.value
                ),
            );
        }
        other => {
            return ActionResult::fatal("CDP_ERROR", format!("select failed: {other}"));
        }
    }

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "action": "select",
        "target": { "selector": cmd.selector },
        "value_summary": {
            "value": cmd.value,
            "by_text": cmd.by_text,
            "by_ref": cmd.by_ref,
        },
        "post_url": url,
        "post_title": title,
    }))
}
