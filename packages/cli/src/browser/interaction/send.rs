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
    /// URL to send the request to
    pub url: String,
    /// HTTP method (default: GET, or POST if -d is used)
    #[arg(short = 'X', long = "method")]
    pub method: Option<String>,
    /// HTTP headers (Key: Value or JSON format), can be repeated
    #[arg(short = 'H', long = "header")]
    pub header: Vec<String>,
    /// Request body data
    #[arg(short = 'd', long = "data")]
    pub data: Option<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser send";

/// Parse a single header string into key-value pairs.
///
/// Supports two formats:
/// - `"Key: Value"` — splits on the first colon
/// - `{"Key": "Value", ...}` — parses as JSON object (supports multiple keys)
pub fn parse_header(raw: &str) -> HashMap<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{')
        && let Ok(map) = serde_json::from_str::<HashMap<String, String>>(trimmed)
    {
        return map;
    }
    let mut result = HashMap::new();
    if let Some(colon_idx) = trimmed.find(':') {
        let key = trimmed[..colon_idx].trim().to_string();
        let value = trimmed[colon_idx + 1..].trim().to_string();
        result.insert(key, value);
    }
    result
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
    // Use serde_json::to_string for safe JS string embedding
    let url_json = serde_json::to_string(url).unwrap_or_else(|_| format!("\"{}\"", url));
    let method_upper = method.to_uppercase();

    let mut options = format!("method: \"{}\"", method_upper);

    if !headers.is_empty() {
        let header_entries: Vec<String> = headers
            .iter()
            .map(|(k, v)| {
                let k_json = serde_json::to_string(k).unwrap_or_else(|_| format!("\"{}\"", k));
                let v_json = serde_json::to_string(v).unwrap_or_else(|_| format!("\"{}\"", v));
                format!("    {}: {}", k_json, v_json)
            })
            .collect();
        options.push_str(&format!(
            ",\n    headers: {{\n{}\n  }}",
            header_entries.join(",\n")
        ));
    }

    if let Some(b) = body {
        let b_json = serde_json::to_string(b).unwrap_or_else(|_| format!("\"{}\"", b));
        options.push_str(&format!(",\n    body: {}", b_json));
    }

    format!(
        r#"(async () => {{
  const r = await fetch({url}, {{
    {options}
  }});
  const hs = {{}}; r.headers.forEach((v, k) => {{ hs[k] = v; }});
  const body = await r.text();
  return {{ status: r.status, statusText: r.statusText, headers: hs, body }};
}})()"#,
        url = url_json,
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
    for raw in &cmd.header {
        match resolve_tokens(raw) {
            Ok(resolved) => {
                for (k, v) in parse_header(&resolved) {
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
    fn parse_header_json_format() {
        let input = r#"{"content-type": "application/json"}"#;
        let result = parse_header(input);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn parse_header_json_multiple_keys() {
        let input = r#"{"Authorization": "Bearer token", "Accept": "text/html"}"#;
        let result = parse_header(input);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
        assert_eq!(result.get("Accept"), Some(&"text/html".to_string()));
    }

    #[test]
    fn parse_header_key_value_format() {
        let input = "Content-Type: application/json";
        let result = parse_header(input);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn parse_header_key_value_with_extra_whitespace() {
        let input = "  Authorization :  Bearer my-token  ";
        let result = parse_header(input);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("Authorization"),
            Some(&"Bearer my-token".to_string())
        );
    }

    #[test]
    fn parse_header_key_value_with_colon_in_value() {
        let input = "Authorization: Bearer abc:def";
        let result = parse_header(input);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("Authorization"),
            Some(&"Bearer abc:def".to_string())
        );
    }

    #[test]
    fn parse_header_invalid_input_returns_empty() {
        let result = parse_header("no-colon-here");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_header_empty_string_returns_empty() {
        let result = parse_header("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_header_invalid_json_falls_back_to_key_value() {
        // Starts with '{' but is not valid JSON
        let input = "{broken json";
        let result = parse_header(input);
        // Falls through JSON parsing, then tries Key:Value — no colon after key, so empty
        assert!(result.is_empty());
    }

    #[test]
    fn parse_header_invalid_json_with_colon_fallback() {
        // Starts with '{' but invalid JSON, but has a colon so Key:Value fallback works
        let input = "{broken: json}";
        let result = parse_header(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("{broken"), Some(&"json}".to_string()));
    }

    // ── method inference ─────────────────────────────────────────────────────

    #[test]
    fn method_inference_defaults_to_get_without_body() {
        assert_eq!(infer_method(None, false), "GET");
    }

    #[test]
    fn method_inference_defaults_to_post_with_body() {
        assert_eq!(infer_method(None, true), "POST");
    }

    #[test]
    fn method_inference_explicit_method_overrides() {
        assert_eq!(infer_method(Some("PUT"), true), "PUT");
    }

    #[test]
    fn method_inference_explicit_delete_without_body() {
        assert_eq!(infer_method(Some("DELETE"), false), "DELETE");
    }

    // ── build_fetch_js ────────────────────────────────────────────────────────

    #[test]
    fn build_fetch_js_basic() {
        let js = build_fetch_js("https://example.com/api", "GET", &HashMap::new(), None);
        assert!(js.contains("fetch(\"https://example.com/api\""));
        assert!(js.contains("method: \"GET\""));
        assert!(!js.contains("headers: {"));
        assert!(!js.contains(",\n    body:"));
    }

    #[test]
    fn build_fetch_js_with_headers_and_body() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        let js = build_fetch_js(
            "https://api.example.com",
            "POST",
            &headers,
            Some(r#"{"key":"value"}"#),
        );
        assert!(js.contains("method: \"POST\""));
        assert!(js.contains("headers: {"));
        assert!(js.contains("\"Authorization\""));
        assert!(js.contains("\"Bearer token123\""));
        assert!(js.contains("body:"));
    }

    #[test]
    fn build_fetch_js_returns_structured_object() {
        let js = build_fetch_js("https://example.com", "GET", &HashMap::new(), None);
        assert!(js.contains("r.status"));
        assert!(js.contains("r.statusText"));
        assert!(js.contains("r.headers"));
        assert!(js.contains("r.text()"));
    }

    // ── CLI parser tests ─────────────────────────────────────────────────────

    #[test]
    fn parse_browser_send_basic() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            cmd: Cmd,
        }

        let cli = TestCli::parse_from([
            "test",
            "https://example.com/api",
            "--session",
            "s1",
            "--tab",
            "t1",
        ]);
        assert_eq!(cli.cmd.url, "https://example.com/api");
        assert_eq!(cli.cmd.session, "s1");
        assert_eq!(cli.cmd.tab, "t1");
        assert!(cli.cmd.method.is_none());
        assert!(cli.cmd.header.is_empty());
        assert!(cli.cmd.data.is_none());
    }

    #[test]
    fn parse_browser_send_with_options() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            cmd: Cmd,
        }

        let cli = TestCli::parse_from([
            "test",
            "https://api.example.com/users",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-H",
            "Authorization: Bearer token",
            "-d",
            r#"{"name":"alice"}"#,
            "--session",
            "s1",
            "--tab",
            "t1",
        ]);
        assert_eq!(cli.cmd.url, "https://api.example.com/users");
        assert_eq!(cli.cmd.method, Some("POST".to_string()));
        assert_eq!(cli.cmd.header.len(), 2);
        assert_eq!(cli.cmd.header[0], "Content-Type: application/json");
        assert_eq!(cli.cmd.header[1], "Authorization: Bearer token");
        assert_eq!(cli.cmd.data, Some(r#"{"name":"alice"}"#.to_string()));
    }

    // ── resolve_tokens ────────────────────────────────────────────────────────

    #[test]
    fn resolve_tokens_no_placeholders_unchanged() {
        let input = "https://api.example.com/v1/data";
        assert_eq!(resolve_tokens(input), Ok(input.to_string()));
    }

    #[test]
    fn resolve_tokens_env_var() {
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
        let result = resolve_tokens("$ACTIONBOOK.NONEXISTENT_SITE_XYZ_12345.API_KEY");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("ACTIONBOOK_NONEXISTENT_SITE_XYZ_12345_API_KEY"));
    }

    #[test]
    fn resolve_tokens_file_based() {
        // Use env var override to avoid filesystem dependencies
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
