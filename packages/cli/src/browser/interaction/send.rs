use clap::Args;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Send an HTTP request via the browser's fetch API
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser send https://api.example.com/data --session s1 --tab t1
  actionbook browser send https://api.example.com/users -X POST -d '{\"name\":\"alice\"}' --session s1 --tab t1
  actionbook browser send https://api.example.com/v1 -H 'Authorization: Bearer $ACTIONBOOK.GITHUB.API_KEY' --session s1 --tab t1

Sends an HTTP request from the browser context using the page's fetch API.
Supports token substitution with $ACTIONBOOK.<SITE>.API_KEY patterns.")]
pub struct Cmd {
    /// Target URL
    pub url: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// HTTP method (default: GET, or POST if --data is provided)
    #[arg(short = 'X', long)]
    pub method: Option<String>,
    /// Request header in \"Key: Value\" or JSON format (repeatable)
    #[arg(short = 'H', long = "header")]
    pub headers: Vec<String>,
    /// Request body data
    #[arg(short = 'd', long)]
    pub data: Option<String>,
}

pub const COMMAND_NAME: &str = "browser send";

/// Parse a single header string into a (key, value) pair.
///
/// Supports two formats:
/// - `"Key: Value"` — splits on the first colon
/// - `{"Key": "Value"}` — parses as JSON and returns the first entry
pub fn parse_header(h: &str) -> Option<(String, String)> {
    let trimmed = h.trim();
    if trimmed.starts_with('{')
        && let Ok(map) = serde_json::from_str::<HashMap<String, String>>(trimmed)
    {
        return map.into_iter().next();
    }
    if let Some(colon_idx) = trimmed.find(':') {
        let key = trimmed[..colon_idx].trim().to_string();
        let value = trimmed[colon_idx + 1..].trim().to_string();
        return Some((key, value));
    }
    None
}

/// Infer the HTTP method from explicit flag and body presence.
///
/// - Explicit `-X` flag takes priority
/// - POST if `--data` is present
/// - GET otherwise
pub fn infer_method(explicit: Option<&str>, has_body: bool) -> &str {
    if let Some(m) = explicit {
        return m;
    }
    if has_body { "POST" } else { "GET" }
}

/// Build the JavaScript fetch expression that runs in the browser context.
pub fn build_fetch_js(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
) -> String {
    // Escape the URL for safe JS string embedding
    let url_escaped = url.replace('\\', "\\\\").replace('"', "\\\"");
    let method_upper = method.to_uppercase();

    let mut options = format!("method: \"{}\"", method_upper);

    if !headers.is_empty() {
        let header_entries: Vec<String> = headers
            .iter()
            .map(|(k, v)| {
                let k_esc = k.replace('\\', "\\\\").replace('"', "\\\"");
                let v_esc = v.replace('\\', "\\\\").replace('"', "\\\"");
                format!("    \"{}\": \"{}\"", k_esc, v_esc)
            })
            .collect();
        options.push_str(&format!(
            ",\n    headers: {{\n{}\n  }}",
            header_entries.join(",\n")
        ));
    }

    if let Some(b) = body {
        // Escape for JS template literal — use JSON stringify to safely embed
        let b_json = serde_json::to_string(b).unwrap_or_else(|_| format!("\"{}\"", b));
        options.push_str(&format!(",\n    body: {}", b_json));
    }

    format!(
        r#"(async () => {{
  const r = await fetch("{url}", {{
    {options}
  }});
  const text = await r.text();
  return {{ status: r.status, statusText: r.statusText, headers: Object.fromEntries([...r.headers]), body: text }};
}})()"#,
        url = url_escaped,
        options = options,
    )
}

/// Resolve `$ACTIONBOOK.<SITE>.API_KEY` template variables in a string.
///
/// Lookup order:
/// 1. Environment variable `ACTIONBOOK_<SITE>_API_KEY` (uppercased site)
/// 2. File `~/.actionbook/tokens/<site>` (lowercased site)
///
/// Returns an error string if a placeholder cannot be resolved.
pub fn resolve_tokens(input: &str) -> Result<String, String> {
    let re = Regex::new(r"\$ACTIONBOOK\.([A-Za-z0-9_]+)\.API_KEY").unwrap();
    let mut result = input.to_string();
    for cap in re.captures_iter(input) {
        let site = &cap[1];
        let token = resolve_single_token(site).ok_or_else(|| {
            format!(
                "Token not found for site \"{}\". Set env ACTIONBOOK_{}_API_KEY or create ~/.actionbook/tokens/{}",
                site.to_lowercase(),
                site.to_uppercase(),
                site.to_lowercase(),
            )
        })?;
        result = result.replace(&cap[0], &token);
    }
    Ok(result)
}

/// Resolve a single site's API key token.
fn resolve_single_token(site: &str) -> Option<String> {
    // 1. Environment variable takes priority
    let env_key = format!("ACTIONBOOK_{}_API_KEY", site.to_uppercase());
    if let Ok(val) = std::env::var(&env_key)
        && !val.is_empty()
    {
        return Some(val);
    }
    // 2. Token file: ~/.actionbook/tokens/<site_lower>
    let home = dirs::home_dir()?;
    let token_file = home
        .join(".actionbook")
        .join("tokens")
        .join(site.to_lowercase());
    std::fs::read_to_string(token_file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

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

    // 1. Parse headers
    let mut parsed_headers: HashMap<String, String> = HashMap::new();
    for raw in &cmd.headers {
        match resolve_tokens(raw) {
            Ok(resolved) => {
                if let Some((k, v)) = parse_header(&resolved) {
                    parsed_headers.insert(k, v);
                }
            }
            Err(e) => return ActionResult::fatal("TOKEN_ERROR", e),
        }
    }

    // 2. Resolve tokens in URL
    let resolved_url = match resolve_tokens(&cmd.url) {
        Ok(u) => u,
        Err(e) => return ActionResult::fatal("TOKEN_ERROR", e),
    };

    // 3. Resolve tokens in body
    let resolved_body = match &cmd.data {
        Some(d) => match resolve_tokens(d) {
            Ok(r) => Some(r),
            Err(e) => return ActionResult::fatal("TOKEN_ERROR", e),
        },
        None => None,
    };

    // 4. Infer method
    let method_owned;
    let method = if let Some(ref m) = cmd.method {
        method_owned = m.to_uppercase();
        method_owned.as_str()
    } else {
        infer_method(None, resolved_body.is_some())
    };

    // 5. Build fetch JS and execute via CDP
    let js = build_fetch_js(
        &resolved_url,
        method,
        &parsed_headers,
        resolved_body.as_deref(),
    );

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": js, "returnByValue": true, "awaitPromise": true }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return crate::daemon::cdp_session::cdp_error_to_result(e, "SEND_FAILED"),
    };

    // 6. Check for JS exceptions
    if let Some(exc) = resp.get("result").and_then(|r| r.get("exceptionDetails")) {
        let emsg = exc
            .pointer("/exception/description")
            .and_then(|v| v.as_str())
            .or_else(|| exc.get("text").and_then(|v| v.as_str()))
            .unwrap_or("fetch error");
        return ActionResult::fatal("SEND_FAILED", emsg.to_string());
    }

    // 7. Extract the returned object from CDP result
    let result_value = match resp
        .get("result")
        .and_then(|r| r.get("result"))
        .and_then(|r| r.get("value"))
    {
        Some(v) => v.clone(),
        None => return ActionResult::fatal("SEND_FAILED", "no result in CDP response"),
    };

    // 8. Parse status, statusText, headers, body from the JS return value
    let status = result_value
        .get("status")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u16;
    let status_text = result_value
        .get("statusText")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let response_headers = result_value.get("headers").cloned().unwrap_or(json!({}));
    let body = result_value
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let post_url = navigation::get_tab_url(&cdp, &target_id).await;
    let post_title = navigation::get_tab_title(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "status": status,
        "statusText": status_text,
        "headers": response_headers,
        "body": body,
        "post_url": post_url,
        "post_title": post_title,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_header ──────────────────────────────────────────────────────────

    #[test]
    fn parse_header_key_value_format() {
        let result = parse_header("Content-Type: application/json");
        assert_eq!(
            result,
            Some(("Content-Type".to_string(), "application/json".to_string()))
        );
    }

    #[test]
    fn parse_header_key_value_with_colon_in_value() {
        let result = parse_header("Authorization: Bearer abc:def");
        assert_eq!(
            result,
            Some(("Authorization".to_string(), "Bearer abc:def".to_string()))
        );
    }

    #[test]
    fn parse_header_key_value_with_whitespace() {
        let result = parse_header("  X-Custom :  hello world  ");
        assert_eq!(
            result,
            Some(("X-Custom".to_string(), "hello world".to_string()))
        );
    }

    #[test]
    fn parse_header_json_format() {
        let input = r#"{"content-type": "application/json"}"#;
        let result = parse_header(input);
        assert_eq!(
            result,
            Some(("content-type".to_string(), "application/json".to_string()))
        );
    }

    #[test]
    fn parse_header_no_colon_returns_none() {
        assert_eq!(parse_header("no-colon-here"), None);
    }

    #[test]
    fn parse_header_empty_returns_none() {
        assert_eq!(parse_header(""), None);
    }

    #[test]
    fn parse_header_invalid_json_falls_back_to_key_value() {
        // Starts with '{' but invalid JSON, has colon so Key:Value fallback works
        let result = parse_header("{broken: json}");
        assert_eq!(result, Some(("{broken".to_string(), "json}".to_string())));
    }

    #[test]
    fn parse_header_invalid_json_no_colon_returns_none() {
        // Starts with '{' but invalid JSON, no colon in the fallback sense
        assert_eq!(parse_header("{broken json}"), None);
    }

    // ── infer_method ─────────────────────────────────────────────────────────

    #[test]
    fn infer_method_defaults_to_get_without_body() {
        assert_eq!(infer_method(None, false), "GET");
    }

    #[test]
    fn infer_method_defaults_to_post_with_body() {
        assert_eq!(infer_method(None, true), "POST");
    }

    #[test]
    fn infer_method_explicit_overrides() {
        assert_eq!(infer_method(Some("put"), false), "put");
        assert_eq!(infer_method(Some("DELETE"), true), "DELETE");
    }

    // ── build_fetch_js ────────────────────────────────────────────────────────

    #[test]
    fn build_fetch_js_get_no_body_no_headers() {
        let js = build_fetch_js("https://example.com/api", "GET", &HashMap::new(), None);
        assert!(js.contains("fetch(\"https://example.com/api\""));
        assert!(js.contains("method: \"GET\""));
        // The fetch options block should not include a body field.
        // Note: the return object template has "body: text" so we check the options-specific form.
        // With no headers, there should be no "headers: {" in the options block.
        assert!(!js.contains("headers: {"));
        // With no data, there should be no body option (body option is always ",\n    body: ...")
        assert!(!js.contains(",\n    body:"));
    }

    #[test]
    fn build_fetch_js_post_with_body() {
        let js = build_fetch_js(
            "https://api.example.com",
            "POST",
            &HashMap::new(),
            Some(r#"{"key":"value"}"#),
        );
        assert!(js.contains("method: \"POST\""));
        // body option should be present
        assert!(js.contains(",\n    body:"));
    }

    #[test]
    fn build_fetch_js_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());
        let js = build_fetch_js("https://example.com", "GET", &headers, None);
        assert!(js.contains("headers: {"));
        assert!(js.contains("\"Authorization\""));
        assert!(js.contains("\"Bearer token123\""));
    }

    #[test]
    fn build_fetch_js_url_with_double_quotes_escaped() {
        let js = build_fetch_js(
            "https://example.com/path?q=\"test\"",
            "GET",
            &HashMap::new(),
            None,
        );
        // The URL should have its double quotes escaped
        assert!(js.contains("\\\"test\\\""));
    }

    #[test]
    fn build_fetch_js_returns_structured_object() {
        let js = build_fetch_js("https://example.com", "GET", &HashMap::new(), None);
        assert!(js.contains("r.status"));
        assert!(js.contains("r.statusText"));
        assert!(js.contains("r.headers"));
        assert!(js.contains("r.text()"));
    }

    // ── resolve_tokens ────────────────────────────────────────────────────────

    #[test]
    fn resolve_tokens_no_placeholders_unchanged() {
        let input = "https://api.example.com/v1/data";
        assert_eq!(resolve_tokens(input), Ok(input.to_string()));
    }

    #[test]
    fn resolve_tokens_env_var() {
        // Set environment variable and check resolution
        // SAFETY: test-only, single-threaded context
        unsafe {
            std::env::set_var("ACTIONBOOK_TESTSITE_API_KEY", "test-token-xyz");
        }
        let result = resolve_tokens("Bearer $ACTIONBOOK.TESTSITE.API_KEY");
        unsafe {
            std::env::remove_var("ACTIONBOOK_TESTSITE_API_KEY");
        }
        assert_eq!(result, Ok("Bearer test-token-xyz".to_string()));
    }

    #[test]
    fn resolve_tokens_missing_returns_error() {
        // Use a site name that almost certainly has no env or file
        let result = resolve_tokens("$ACTIONBOOK.NONEXISTENT_SITE_XYZ_12345.API_KEY");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("ACTIONBOOK_NONEXISTENT_SITE_XYZ_12345_API_KEY"));
    }

    #[test]
    fn resolve_tokens_env_var_priority_over_file() {
        // Env var takes priority over file; use env override to avoid filesystem dependencies
        // SAFETY: test-only, single-threaded context
        unsafe {
            std::env::set_var("ACTIONBOOK_FILETEST_API_KEY", "file-token-abc");
        }
        let result = resolve_tokens("$ACTIONBOOK.FILETEST.API_KEY");
        unsafe {
            std::env::remove_var("ACTIONBOOK_FILETEST_API_KEY");
        }
        assert_eq!(result, Ok("file-token-abc".to_string()));
    }
}
