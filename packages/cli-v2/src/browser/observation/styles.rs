use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element::TabContext, navigation};
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Read computed styles for an element.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser styles \"#target\" --session s1 --tab t1
  actionbook browser styles @e3 color backgroundColor z-index --session s1 --tab t1

Accepts a CSS selector, XPath, or snapshot ref (@eN from snapshot output).
Without property names, returns the standard set of computed styles.
With names, returns only the specified CSS properties.")]
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
    /// Optional style property names. Defaults to the standard property set.
    pub names: Vec<String>,
}

pub const COMMAND_NAME: &str = "browser styles";

pub const DEFAULT_STYLE_NAMES: [&str; 17] = [
    "display",
    "visibility",
    "opacity",
    "color",
    "backgroundColor",
    "fontSize",
    "fontWeight",
    "fontFamily",
    "margin",
    "padding",
    "border",
    "position",
    "zIndex",
    "overflow",
    "cursor",
    "width",
    "height",
];

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

    let names = requested_style_names(&cmd.names);
    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let value = match get_styles(&ctx, &object_id, &names).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    ActionResult::ok(json!({
        "target": { "selector": cmd.selector },
        "value": value,
        "__prop_order": names,
        "__ctx_url": url,
    }))
}

async fn get_styles(
    ctx: &TabContext,
    object_id: &str,
    names: &[String],
) -> Result<Value, ActionResult> {
    let names_json = serde_json::to_string(names).map_err(|e| {
        ActionResult::fatal("INTERNAL_ERROR", format!("serialize style names: {e}"))
    })?;

    let function = format!(
        r#"function() {{
            const computed = window.getComputedStyle(this);
            const names = {names_json};
            const styles = {{}};
            for (const name of names) {{
                const cssName = name.replace(/[A-Z]/g, m => '-' + m.toLowerCase());
                styles[name] = computed.getPropertyValue(cssName).trim();
            }}
            return styles;
        }}"#
    );

    let resp = ctx
        .execute_on_element(
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": function,
                "returnByValue": true,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    if resp.pointer("/result/exceptionDetails").is_some() {
        let description = resp
            .pointer("/result/exceptionDetails/exception/description")
            .and_then(|v| v.as_str())
            .unwrap_or("JS exception during style read");
        return Err(ActionResult::fatal("JS_EXCEPTION", description.to_string()));
    }

    Ok(resp
        .pointer("/result/result/value")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new())))
}

pub fn requested_style_names(names: &[String]) -> Vec<String> {
    if names.is_empty() {
        DEFAULT_STYLE_NAMES
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    } else {
        names.to_vec()
    }
}

pub fn css_property_name(name: &str) -> String {
    let mut css = String::new();
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            css.push('-');
            css.push(ch.to_ascii_lowercase());
        } else {
            css.push(ch);
        }
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_style_names_uses_defaults_when_empty() {
        let names = requested_style_names(&[]);
        assert_eq!(names.len(), DEFAULT_STYLE_NAMES.len());
        assert_eq!(names[0], "display");
        assert_eq!(names.last().map(String::as_str), Some("height"));
    }

    #[test]
    fn requested_style_names_preserves_explicit_order() {
        let names = requested_style_names(&[
            "backgroundColor".to_string(),
            "width".to_string(),
            "zIndex".to_string(),
        ]);

        assert_eq!(names, vec!["backgroundColor", "width", "zIndex"]);
    }

    #[test]
    fn css_property_name_converts_camel_case() {
        assert_eq!(css_property_name("backgroundColor"), "background-color");
        assert_eq!(css_property_name("fontSize"), "font-size");
        assert_eq!(css_property_name("z-index"), "z-index");
    }
}
