use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

use crate::action_result::ActionResult;

/// §2.4 JSON envelope.
#[derive(Debug, Serialize)]
pub struct JsonEnvelope {
    pub ok: bool,
    pub command: String,
    pub context: Option<ResponseContext>,
    pub data: Value,
    pub error: Value,
    pub meta: ResponseMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseContext {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResponseMeta {
    pub duration_ms: u64,
    pub warnings: Vec<String>,
    pub pagination: Value,
    pub truncated: bool,
}

impl JsonEnvelope {
    pub fn success(
        command: &str,
        context: Option<ResponseContext>,
        mut data: Value,
        duration: Duration,
    ) -> Self {
        // Extract internal fields before stripping
        let truncated = data
            .get("__truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Strip internal __* fields from data (used by context/meta extraction only)
        if let Some(obj) = data.as_object_mut() {
            obj.retain(|k, _| !k.starts_with("__"));
        }
        JsonEnvelope {
            ok: true,
            command: command.to_string(),
            context,
            data,
            error: Value::Null,
            meta: ResponseMeta {
                duration_ms: duration.as_millis() as u64,
                warnings: vec![],
                pagination: Value::Null,
                truncated,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn error(
        command: &str,
        context: Option<ResponseContext>,
        code: &str,
        message: &str,
        retryable: bool,
        details: Value,
        hint: &str,
        duration: Duration,
    ) -> Self {
        let mut err = serde_json::json!({
            "code": code,
            "message": message,
            "retryable": retryable,
            "details": details,
        });
        if !hint.is_empty() {
            err["hint"] = serde_json::json!(hint);
        }
        JsonEnvelope {
            ok: false,
            command: command.to_string(),
            context,
            data: Value::Null,
            error: err,
            meta: ResponseMeta {
                duration_ms: duration.as_millis() as u64,
                warnings: vec![],
                pagination: Value::Null,
                truncated: false,
            },
        }
    }

    pub fn from_result(
        command: &str,
        context: Option<ResponseContext>,
        result: &ActionResult,
        duration: Duration,
    ) -> Self {
        match result {
            ActionResult::Ok { data } => Self::success(command, context, data.clone(), duration),
            ActionResult::Fatal {
                code,
                message,
                hint,
                details,
            } => {
                // CloudConnectionLost is retryable despite being Fatal variant
                let retryable = code == "CLOUD_CONNECTION_LOST";
                Self::error(
                    command,
                    context,
                    code,
                    message,
                    retryable,
                    details.clone().unwrap_or(Value::Null),
                    hint,
                    duration,
                )
            }
            ActionResult::Retryable { reason, hint } => Self::error(
                command,
                context,
                "RETRYABLE",
                reason,
                true,
                Value::Null,
                hint,
                duration,
            ),
            ActionResult::UserAction { action, hint } => Self::error(
                command,
                context,
                "USER_ACTION",
                action,
                false,
                Value::Null,
                hint,
                duration,
            ),
        }
    }
}

/// Format text output per §2.5.
pub fn format_text(
    command: &str,
    context: &Option<ResponseContext>,
    result: &ActionResult,
) -> String {
    let mut lines = Vec::new();

    // Header
    if let Some(ctx) = context {
        if let Some(ref tab_id) = ctx.tab_id {
            if let Some(ref url) = ctx.url {
                lines.push(format!("[{} {}] {}", ctx.session_id, tab_id, url));
            } else {
                lines.push(format!("[{} {}]", ctx.session_id, tab_id));
            }
        } else {
            lines.push(format!("[{}]", ctx.session_id));
        }
    }

    match result {
        ActionResult::Ok { data } => {
            // Action commands: "ok <command>" then fields
            let is_action = matches!(
                command,
                "browser.start"
                    | "browser.close"
                    | "browser.restart"
                    | "browser.goto"
                    | "browser.back"
                    | "browser.forward"
                    | "browser.reload"
                    | "browser.click"
                    | "browser.fill"
                    | "browser.new-tab"
                    | "browser.close-tab"
            );

            if is_action {
                lines.push(format!("ok {command}"));
            }

            // Emit key-value fields from data
            format_data_fields(command, data, &mut lines);
        }
        ActionResult::Fatal { code, message, .. } => {
            lines.push(format!("error {code}: {message}"));
        }
        ActionResult::Retryable { reason, .. } => {
            lines.push(format!("error RETRYABLE: {reason}"));
        }
        ActionResult::UserAction { action, .. } => {
            lines.push(format!("error USER_ACTION: {action}"));
        }
    }

    lines.join("\n")
}

fn format_data_fields(command: &str, data: &Value, lines: &mut Vec<String>) {
    match command {
        "browser.start" => {
            if let Some(mode) = data
                .get("session")
                .and_then(|s| s.get("mode"))
                .and_then(|v| v.as_str())
            {
                lines.push(format!("mode: {mode}"));
            }
            if let Some(status) = data
                .get("session")
                .and_then(|s| s.get("status"))
                .and_then(|v| v.as_str())
            {
                lines.push(format!("status: {status}"));
            }
            if let Some(title) = data
                .get("tab")
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
            {
                lines.push(format!("title: {title}"));
            }
        }
        "browser.list-sessions" => {
            let total = data
                .get("total_sessions")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let label = if total == 1 { "session" } else { "sessions" };
            // Prepend count before header (list-sessions has no header)
            lines.insert(0, format!("{total} {label}"));
            if let Some(sessions) = data.get("sessions").and_then(|v| v.as_array()) {
                for s in sessions {
                    let sid = s.get("session_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let status = s.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                    let tabs = s.get("tabs_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    lines.push(format!("[{sid}]"));
                    lines.push(format!("status: {status}"));
                    lines.push(format!("tabs: {tabs}"));
                }
            }
        }
        "browser.status" => {
            if let Some(s) = data.get("session") {
                if let Some(status) = s.get("status").and_then(|v| v.as_str()) {
                    lines.push(format!("status: {status}"));
                }
                if let Some(mode) = s.get("mode").and_then(|v| v.as_str()) {
                    lines.push(format!("mode: {mode}"));
                }
                if let Some(tabs) = s.get("tabs_count").and_then(|v| v.as_u64()) {
                    lines.push(format!("tabs: {tabs}"));
                }
            }
        }
        "browser.close" => {
            if let Some(tabs) = data.get("closed_tabs").and_then(|v| v.as_u64()) {
                lines.push(format!("closed_tabs: {tabs}"));
            }
        }
        "browser.restart" => {
            if let Some(status) = data
                .get("session")
                .and_then(|s| s.get("status"))
                .and_then(|v| v.as_str())
            {
                lines.push(format!("status: {status}"));
            }
        }
        "browser.list-tabs" => {
            let total = data.get("total_tabs").and_then(|v| v.as_u64()).unwrap_or(0);
            let label = if total == 1 { "tab" } else { "tabs" };
            lines.push(format!("{total} {label}"));
            if let Some(tabs) = data.get("tabs").and_then(|v| v.as_array()) {
                for t in tabs {
                    let tid = t.get("tab_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if title.is_empty() {
                        lines.push(format!("[{tid}]"));
                    } else {
                        lines.push(format!("[{tid}] {title}"));
                    }
                    lines.push(url.to_string());
                }
            }
        }
        "browser.new-tab" => {
            if let Some(title) = data
                .get("tab")
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
            {
                lines.push(format!("title: {title}"));
            }
        }
        "browser.close-tab" => {
            // No additional fields per §8.3 text format
        }
        "browser.goto" | "browser.back" | "browser.forward" | "browser.reload" => {
            if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                lines.push(format!("title: {title}"));
            }
        }
        "browser.snapshot" => {
            // §10.1: text mode outputs content directly (no "ok" prefix)
            if let Some(content) = data.get("content").and_then(|v| v.as_str()) {
                lines.push(content.to_string());
            }
        }
        "browser.eval" => {
            if let Some(val) = data.get("value") {
                lines.push(val.as_str().unwrap_or(&val.to_string()).to_string());
            }
        }
        _ => {
            // Generic: print data as-is
            if let Some(s) = data.as_str() {
                lines.push(s.to_string());
            }
        }
    }
}
