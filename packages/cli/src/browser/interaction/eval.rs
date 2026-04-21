use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::{Map, Value};

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::browser::observation::logs_console::ENSURE_LOG_CAPTURE_JS;
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::error::CliError;
use crate::output::ResponseContext;

#[cfg_attr(not(test), allow(dead_code))]
const BODY_HEAD_LIMIT_CHARS: usize = 256;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalErrorCode {
    RuntimeError,
    CrossOrigin,
    ResponseNotJson,
    ResponseNotOk,
    Timeout,
    ArgsConflict,
    FileNotFound,
    StdinTty,
    StdinEmpty,
}

#[cfg_attr(not(test), allow(dead_code))]
impl EvalErrorCode {
    // Client-layer codes (ArgsConflict, FileNotFound, StdinTty, StdinEmpty) are
    // never emitted by CDP, so they are intentionally absent here.
    fn from_wire_code(raw: &str) -> Option<Self> {
        match raw {
            "EVAL_RUNTIME_ERROR" | "RUNTIME_ERROR" => Some(EvalErrorCode::RuntimeError),
            "EVAL_CROSS_ORIGIN" | "CROSS_ORIGIN" => Some(EvalErrorCode::CrossOrigin),
            "EVAL_RESPONSE_NOT_JSON" | "RESPONSE_NOT_JSON" => Some(EvalErrorCode::ResponseNotJson),
            "EVAL_RESPONSE_NOT_OK" | "RESPONSE_NOT_OK" => Some(EvalErrorCode::ResponseNotOk),
            "EVAL_TIMEOUT" | "TIMEOUT" => Some(EvalErrorCode::Timeout),
            _ => None,
        }
    }

    fn code(self) -> &'static str {
        match self {
            EvalErrorCode::RuntimeError => "EVAL_RUNTIME_ERROR",
            EvalErrorCode::CrossOrigin => "EVAL_CROSS_ORIGIN",
            EvalErrorCode::ResponseNotJson => "EVAL_RESPONSE_NOT_JSON",
            EvalErrorCode::ResponseNotOk => "EVAL_RESPONSE_NOT_OK",
            EvalErrorCode::Timeout => "EVAL_TIMEOUT",
            EvalErrorCode::ArgsConflict => "EVAL_ARGS_CONFLICT",
            EvalErrorCode::FileNotFound => "EVAL_FILE_NOT_FOUND",
            EvalErrorCode::StdinTty => "EVAL_STDIN_TTY",
            EvalErrorCode::StdinEmpty => "EVAL_STDIN_EMPTY",
        }
    }

    fn default_hint(self) -> &'static str {
        match self {
            EvalErrorCode::RuntimeError => {
                "Inspect the expression and referenced variables before retrying"
            }
            EvalErrorCode::CrossOrigin => "Use same-origin fetch or proxy the request server-side",
            EvalErrorCode::ResponseNotJson => "Check content-type before parsing JSON",
            EvalErrorCode::ResponseNotOk => "Handle non-2xx responses before decoding the body",
            EvalErrorCode::Timeout => "Reduce work or raise --timeout",
            EvalErrorCode::ArgsConflict => {
                "Provide exactly one of: positional expression, --file, or stdin (`-`)"
            }
            EvalErrorCode::FileNotFound => "Verify --file points to a readable script path",
            EvalErrorCode::StdinTty => {
                "Pipe the expression via stdin, e.g. echo 'expr' | actionbook browser eval -"
            }
            EvalErrorCode::StdinEmpty => {
                "stdin was read but produced no expression; verify the upstream command or pipeline"
            }
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn truncate_body_head(text: &str) -> String {
    text.chars().take(BODY_HEAD_LIMIT_CHARS).collect()
}

fn json_value_to_string(value: Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text),
        other => Some(other.to_string()),
    }
}

fn string_property<'a>(properties: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    properties.get(key).and_then(|value| value.as_str())
}

fn normalize_detail_key(name: &str) -> String {
    match name {
        "contentType" => "content_type".to_string(),
        "bodyHead" => "body_head".to_string(),
        other => other.to_string(),
    }
}

fn remote_object_to_json(remote: &Value) -> Value {
    if let Some(value) = remote.get("value") {
        return value.clone();
    }
    if let Some(value) = remote
        .get("unserializableValue")
        .and_then(|value| value.as_str())
    {
        return Value::String(value.to_string());
    }
    if let Some(value) = remote.get("description").and_then(|value| value.as_str()) {
        return Value::String(value.to_string());
    }
    Value::Null
}

async fn read_exception_properties(
    cdp: &CdpSession,
    target_id: &str,
    object_id: &str,
) -> Map<String, Value> {
    let Ok(resp) = cdp
        .execute_on_tab(
            target_id,
            "Runtime.getProperties",
            json!({
                "objectId": object_id,
                "ownProperties": true,
            }),
        )
        .await
    else {
        return Map::new();
    };

    let mut properties = Map::new();
    let Some(entries) = resp
        .pointer("/result/result")
        .and_then(|value| value.as_array())
    else {
        return properties;
    };

    for entry in entries {
        let Some(name) = entry.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        if entry.get("get").is_some() || entry.get("set").is_some() {
            continue;
        }
        let Some(remote_value) = entry.get("value") else {
            continue;
        };
        properties.insert(
            normalize_detail_key(name),
            remote_object_to_json(remote_value),
        );
    }

    properties
}

fn has_cross_origin_fetch_target(expression: &str, pre_origin: &str) -> bool {
    if pre_origin.is_empty() || pre_origin == "null" {
        return false;
    }

    let expression_lower = expression.to_ascii_lowercase();
    if !expression_lower.contains("fetch(") && !expression_lower.contains("fetch (") {
        return false;
    }

    let Ok(pre_origin_url) = reqwest::Url::parse(pre_origin) else {
        return false;
    };
    let pre_origin = pre_origin_url.origin().ascii_serialization();

    let mut chars = expression.chars().peekable();
    while let Some(ch) = chars.next() {
        if !matches!(ch, '"' | '\'' | '`') {
            continue;
        }

        let quote = ch;
        let mut literal = String::new();
        let mut escaped = false;
        for ch in chars.by_ref() {
            if escaped {
                literal.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                break;
            }
            literal.push(ch);
        }

        if !(literal.starts_with("http://") || literal.starts_with("https://")) {
            continue;
        }

        let Ok(url) = reqwest::Url::parse(&literal) else {
            continue;
        };
        if url.origin().ascii_serialization() != pre_origin {
            return true;
        }
    }

    false
}

fn classify_eval_error(
    description: &str,
    error_type: &str,
    properties: &Map<String, Value>,
    expression: &str,
    pre_origin: &str,
) -> EvalErrorCode {
    if let Some(code) = string_property(properties, "code").and_then(EvalErrorCode::from_wire_code)
    {
        return code;
    }

    let mut haystack = String::new();
    haystack.push_str(description);
    haystack.push(' ');
    haystack.push_str(error_type);
    if let Some(reason) = string_property(properties, "reason") {
        haystack.push(' ');
        haystack.push_str(reason);
    }
    if let Some(message) = string_property(properties, "message") {
        haystack.push(' ');
        haystack.push_str(message);
    }
    let haystack = haystack.to_ascii_lowercase();

    if haystack.contains("securityerror")
        || haystack.contains("cross-origin")
        || haystack.contains("cross origin")
        || haystack.contains("same origin policy")
        || haystack.contains("blocked a frame with origin")
        || haystack.contains("content security policy")
        || (haystack.contains("failed to fetch")
            && has_cross_origin_fetch_target(expression, pre_origin))
    {
        EvalErrorCode::CrossOrigin
    } else {
        EvalErrorCode::RuntimeError
    }
}

struct EvalFailureContext<'a> {
    pre_url: &'a str,
    pre_origin: &'a str,
    pre_ready_state: &'a str,
    error_type: &'a str,
}

fn build_eval_error_result(
    code: EvalErrorCode,
    reason: String,
    hint: Option<String>,
    context: EvalFailureContext<'_>,
    mut properties: Map<String, Value>,
) -> ActionResult {
    properties.remove("code");
    properties.remove("hint");
    let normalized_reason = properties
        .remove("reason")
        .and_then(json_value_to_string)
        .filter(|value| !value.is_empty())
        .unwrap_or(reason);
    let body_head = properties
        .remove("body_head")
        .and_then(json_value_to_string)
        .map(|value| truncate_body_head(&value));
    let message = normalized_reason.clone();
    let hint = hint.unwrap_or_else(|| code.default_hint().to_string());

    let mut details = Map::new();
    details.insert("stage".to_string(), json!("eval"));
    details.insert("pre_url".to_string(), json!(context.pre_url));
    details.insert("pre_origin".to_string(), json!(context.pre_origin));
    details.insert("pre_readyState".to_string(), json!(context.pre_ready_state));
    details.insert("error_type".to_string(), json!(context.error_type));
    details.insert("reason".to_string(), json!(normalized_reason));

    for (key, value) in properties {
        details.insert(key, value);
    }

    if let Some(body_head) = body_head {
        details.insert("body_head".to_string(), json!(body_head));
    }

    ActionResult::fatal_with_details(code.code(), message, hint, Value::Object(details))
}

fn eval_timeout_result(timeout_ms: u64) -> ActionResult {
    let reason = format!("browser eval timed out after {timeout_ms}ms");
    ActionResult::fatal_with_details(
        EvalErrorCode::Timeout.code(),
        reason.clone(),
        EvalErrorCode::Timeout.default_hint(),
        json!({ "reason": reason }),
    )
}

fn build_eval_args_error(code: EvalErrorCode, message: String, reason: &str) -> ActionResult {
    ActionResult::fatal_with_details(
        code.code(),
        message,
        code.default_hint(),
        json!({ "stage": "eval", "reason": reason }),
    )
}

fn build_file_not_found(path: &Path, io_reason: &str) -> ActionResult {
    ActionResult::fatal_with_details(
        EvalErrorCode::FileNotFound.code(),
        format!("eval script file not found: {}", path.display()),
        EvalErrorCode::FileNotFound.default_hint(),
        json!({
            "stage": "eval",
            "reason": io_reason,
            "path": path.display().to_string(),
        }),
    )
}

/// Resolve the eval expression from one of three mutually-exclusive sources:
/// a positional expression, `--file <path>`, or stdin (positional `-`).
/// All failure envelopes are arg-layer — no CDP round-trip has happened yet,
/// so no pre-page context (pre_url, pre_origin, pre_readyState) is attached.
///
/// Must be called on the CLI side before the command is forwarded to the
/// daemon, because stdin and the file's relative path belong to the CLI
/// process, not the daemon's.
pub fn resolve_eval_source(cmd: &Cmd) -> Result<String, ActionResult> {
    let positional_is_stdin = matches!(cmd.expression.as_deref(), Some("-"));
    let has_stdin = positional_is_stdin;
    let has_inline = cmd.expression.is_some() && !positional_is_stdin;
    let has_file = cmd.file.is_some();

    let source_count = u8::from(has_inline) + u8::from(has_file) + u8::from(has_stdin);
    if source_count > 1 {
        let reason = match (has_inline, has_file, has_stdin) {
            (true, true, false) => "positional+file",
            (true, false, true) => "positional+stdin",
            (false, true, true) => "file+stdin",
            (true, true, true) => "positional+file+stdin",
            _ => "multiple_sources",
        };
        return Err(build_eval_args_error(
            EvalErrorCode::ArgsConflict,
            "multiple eval input sources provided".to_string(),
            reason,
        ));
    }

    if has_stdin {
        if std::io::stdin().is_terminal() {
            return Err(build_eval_args_error(
                EvalErrorCode::StdinTty,
                "stdin is a terminal; no expression piped".to_string(),
                "stdin_is_tty",
            ));
        }
        let mut buf = String::new();
        if let Err(err) = std::io::stdin().read_to_string(&mut buf) {
            return Err(build_eval_args_error(
                EvalErrorCode::StdinEmpty,
                format!("failed to read stdin: {err}"),
                "read_error",
            ));
        }
        if buf.trim().is_empty() {
            let reason = if buf.is_empty() {
                "pipe_closed"
            } else {
                "whitespace_only"
            };
            return Err(build_eval_args_error(
                EvalErrorCode::StdinEmpty,
                "stdin expression is empty".to_string(),
                reason,
            ));
        }
        return Ok(buf);
    }

    if let Some(path) = &cmd.file {
        return std::fs::read_to_string(path)
            .map_err(|err| build_file_not_found(path, &err.to_string()));
    }

    match &cmd.expression {
        Some(expr) => Ok(expr.clone()),
        None => Err(build_eval_args_error(
            EvalErrorCode::ArgsConflict,
            "no eval input provided".to_string(),
            "no_source",
        )),
    }
}

pub fn timeout_result(timeout_ms: u64) -> ActionResult {
    eval_timeout_result(timeout_ms)
}

/// Evaluate JavaScript
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser eval \"document.title\" --session s1 --tab t1
  actionbook browser eval \"window.scrollY\" --session s1 --tab t1
  actionbook browser eval \"document.querySelectorAll('a').length\" --session s1 --tab t1

Evaluates a JavaScript expression in the page context and returns the result.
The expression is evaluated via Runtime.evaluate with returnByValue.

By default each eval runs in an isolated scope so that let/const declarations do
not leak across calls on the same tab.  Single-expression await works transparently
(e.g. 'await fetch(url).then(r => r.json())').

Note: Multi-statement expressions that contain 'await' (e.g.
'let x = await Promise.resolve(42); x + 1') are not supported under the default
isolated mode — use --no-isolate or wrap the body in an explicit async arrow:
  actionbook browser eval \"(async () => { let x = await f(); return x + 1; })()\" ...")]
pub struct Cmd {
    /// JavaScript expression, or `-` to read the expression from stdin
    pub expression: Option<String>,
    /// Read the JavaScript expression from a file
    #[arg(long)]
    #[serde(default)]
    pub file: Option<PathBuf>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Disable scope isolation (allow let/const to persist across evals on the same tab)
    #[arg(long)]
    #[serde(default)]
    pub no_isolate: bool,
}

pub const COMMAND_NAME: &str = "browser eval";

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
    let source_expression = match resolve_eval_source(cmd) {
        Ok(s) => s,
        Err(err) => return err,
    };

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Install log capture hook before eval so console.* calls in the expression are captured.
    let _ = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": ENSURE_LOG_CAPTURE_JS, "returnByValue": true }),
        )
        .await;

    // Capture pre-execution page context for diagnostics.
    let pre_url = navigation::get_tab_url(&cdp, &target_id).await;
    let pre_origin = navigation::get_tab_origin(&cdp, &target_id).await;
    let pre_ready_state = navigation::get_tab_ready_state(&cdp, &target_id).await;

    // Build the expression to send to CDP.
    // By default, isolate scope so let/const don't leak across evals:
    //
    // - Expressions without top-level `await`: wrap with a regular function + eval().
    //   eval() preserves the completion value of multi-statement programs and
    //   scopes let/const to the function, preventing leakage.
    //
    // - Expressions with top-level `await`: embed directly in an async function body.
    //   eval() cannot inherit async context in this Chrome version (eval'd strings
    //   are parsed as Scripts, where await is invalid). The async IIFE makes await
    //   syntactically valid while still isolating let/const to the function scope.
    //   awaitPromise: true (already set) resolves the returned Promise.
    //
    // With --no-isolate, pass the expression directly (old behavior).
    let expression = if cmd.no_isolate {
        source_expression.clone()
    } else {
        // Detect top-level `await` anywhere in the expression (not just at start).
        // e.g. `(await Promise.resolve(42)) + 1` has await after `(`.
        // Sync expressions work fine inside async functions too (awaitPromise unwraps).
        let has_await =
            source_expression.contains("await ") || source_expression.contains("await(");
        if has_await {
            format!(
                "(async function(){{ return (\n{}\n); }})()",
                source_expression
            )
        } else {
            let escaped = serde_json::to_string(&source_expression).unwrap_or_default();
            format!("(function(){{ return eval({}); }})()", escaped)
        }
    };

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return match e {
                CliError::Timeout => eval_timeout_result(60_000),
                CliError::CloudConnectionLost(_) | CliError::SessionClosed(_) => {
                    cdp_error_to_result(e, EvalErrorCode::RuntimeError.code())
                }
                other => {
                    let reason = other.to_string();
                    let code = classify_eval_error(
                        &reason,
                        "CdpError",
                        &Map::new(),
                        &source_expression,
                        &pre_origin,
                    );
                    build_eval_error_result(
                        code,
                        reason,
                        None,
                        EvalFailureContext {
                            pre_url: &pre_url,
                            pre_origin: &pre_origin,
                            pre_ready_state: &pre_ready_state,
                            error_type: "CdpError",
                        },
                        Map::new(),
                    )
                }
            };
        }
    };

    // Extract value from CDP response
    if let Some(result) = resp.get("result").and_then(|r| r.get("result")) {
        if let Some(exc) = resp.get("result").and_then(|r| r.get("exceptionDetails")) {
            // Prefer exception.description (e.g. "Error: boom-eval"), fall back to text
            let emsg = exc
                .pointer("/exception/description")
                .and_then(|v| v.as_str())
                .or_else(|| exc.get("text").and_then(|v| v.as_str()))
                .unwrap_or("expression error");

            let error_type = exc
                .pointer("/exception/className")
                .and_then(|v| v.as_str())
                .unwrap_or("Error")
                .to_string();

            let properties = if let Some(object_id) = exc
                .pointer("/exception/objectId")
                .and_then(|value| value.as_str())
            {
                read_exception_properties(&cdp, &target_id, object_id).await
            } else {
                Map::new()
            };
            let code = classify_eval_error(
                emsg,
                &error_type,
                &properties,
                &source_expression,
                &pre_origin,
            );
            let reason = string_property(&properties, "reason")
                .unwrap_or(emsg)
                .to_string();
            let hint = string_property(&properties, "hint").map(ToOwned::to_owned);

            return build_eval_error_result(
                code,
                reason,
                hint,
                EvalFailureContext {
                    pre_url: &pre_url,
                    pre_origin: &pre_origin,
                    pre_ready_state: &pre_ready_state,
                    error_type: &error_type,
                },
                properties,
            );
        }

        let js_type = result
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("undefined")
            .to_string();

        // Return the typed value as-is from CDP (number, bool, string, etc.)
        let value = result.get("value").cloned().unwrap_or(json!(null));

        let preview = result
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                if value.is_string() {
                    value.as_str().unwrap().to_string()
                } else {
                    value.to_string()
                }
            });

        let post_url = navigation::get_tab_url(&cdp, &target_id).await;
        let post_title = navigation::get_tab_title(&cdp, &target_id).await;

        ActionResult::ok(json!({
            "value": value,
            "type": js_type,
            "preview": preview,
            "pre_url": pre_url,
            "pre_origin": pre_origin,
            "pre_readyState": pre_ready_state,
            "post_url": post_url,
            "post_title": post_title,
        }))
    } else {
        build_eval_error_result(
            EvalErrorCode::RuntimeError,
            "no result in CDP response".to_string(),
            None,
            EvalFailureContext {
                pre_url: &pre_url,
                pre_origin: &pre_origin,
                pre_ready_state: &pre_ready_state,
                error_type: "CdpError",
            },
            Map::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BODY_HEAD_LIMIT_CHARS, EvalErrorCode, EvalFailureContext, build_eval_error_result,
        classify_eval_error, truncate_body_head,
    };
    use crate::action_result::ActionResult;
    use serde_json::{Map, json};

    #[test]
    fn eval_error_code_mappings_are_stable() {
        assert_eq!(EvalErrorCode::RuntimeError.code(), "EVAL_RUNTIME_ERROR");
        assert_eq!(EvalErrorCode::CrossOrigin.code(), "EVAL_CROSS_ORIGIN");
        assert_eq!(
            EvalErrorCode::ResponseNotJson.code(),
            "EVAL_RESPONSE_NOT_JSON"
        );
        assert_eq!(EvalErrorCode::ResponseNotOk.code(), "EVAL_RESPONSE_NOT_OK");
        assert_eq!(EvalErrorCode::Timeout.code(), "EVAL_TIMEOUT");
    }

    #[test]
    fn eval_error_default_hints_are_non_empty() {
        let hints = [
            EvalErrorCode::RuntimeError.default_hint(),
            EvalErrorCode::CrossOrigin.default_hint(),
            EvalErrorCode::ResponseNotJson.default_hint(),
            EvalErrorCode::ResponseNotOk.default_hint(),
            EvalErrorCode::Timeout.default_hint(),
        ];

        assert!(hints.iter().all(|hint| !hint.is_empty()));
    }

    #[test]
    fn truncate_body_head_respects_char_boundary() {
        let input = "中".repeat(BODY_HEAD_LIMIT_CHARS + 8);
        let truncated = truncate_body_head(&input);

        assert_eq!(truncated.chars().count(), BODY_HEAD_LIMIT_CHARS);
        assert!(truncated.chars().all(|ch| ch == '中'));
    }

    #[test]
    fn classify_eval_error_uses_wire_code_first() {
        let mut properties = Map::new();
        properties.insert("code".to_string(), json!("EVAL_RESPONSE_NOT_JSON"));

        assert_eq!(
            classify_eval_error(
                "ignored",
                "Object",
                &properties,
                "1 + 1",
                "https://example.com"
            ),
            EvalErrorCode::ResponseNotJson
        );
    }

    #[test]
    fn classify_eval_error_detects_cross_origin_fallbacks() {
        assert_eq!(
            classify_eval_error(
                "TypeError: Failed to fetch",
                "TypeError",
                &Map::new(),
                r#"fetch("http://127.0.0.1:8080/api/data")"#,
                "https://example.com",
            ),
            EvalErrorCode::CrossOrigin
        );
        assert_eq!(
            classify_eval_error(
                "SecurityError: Blocked",
                "DOMException",
                &Map::new(),
                "document.cookie",
                "https://example.com",
            ),
            EvalErrorCode::CrossOrigin
        );
    }

    #[test]
    fn classify_eval_error_does_not_promote_generic_failed_fetch_without_cross_origin_target() {
        assert_eq!(
            classify_eval_error(
                "TypeError: Failed to fetch",
                "TypeError",
                &Map::new(),
                r#"fetch("/api/data")"#,
                "https://example.com",
            ),
            EvalErrorCode::RuntimeError
        );
    }

    #[test]
    fn build_eval_error_result_normalizes_reason_to_string() {
        let mut properties = Map::new();
        properties.insert("reason".to_string(), json!({ "kind": "bad-shape" }));

        let result = build_eval_error_result(
            EvalErrorCode::RuntimeError,
            "fallback".to_string(),
            None,
            EvalFailureContext {
                pre_url: "about:blank",
                pre_origin: "null",
                pre_ready_state: "complete",
                error_type: "Object",
            },
            properties,
        );

        let ActionResult::Fatal {
            message,
            details: Some(serde_json::Value::Object(details)),
            ..
        } = result
        else {
            panic!("expected fatal result with details");
        };

        assert_eq!(message, r#"{"kind":"bad-shape"}"#);
        assert_eq!(details["reason"], json!(r#"{"kind":"bad-shape"}"#));
    }
}
