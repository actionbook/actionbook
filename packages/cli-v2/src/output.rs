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
        let warnings: Vec<String> = data
            .get("__warnings")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
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
                warnings,
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
                let retryable = matches!(code.as_str(), "CLOUD_CONNECTION_LOST" | "TIMEOUT");
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
                    | "browser.hover"
                    | "browser.focus"
                    | "browser.press"
                    | "browser.type"
                    | "browser.fill"
                    | "browser.screenshot"
                    | "browser.select"
                    | "browser.drag"
                    | "browser.upload"
                    | "browser.mouse-move"
                    | "browser.cursor-position"
                    | "browser.scroll"
                    | "browser.new-tab"
                    | "browser.close-tab"
                    | "browser.pdf"
                    | "browser.wait.element"
                    | "browser.wait.navigation"
                    | "browser.wait.network-idle"
                    | "browser.wait.condition"
                    | "browser.cookies.set"
                    | "browser.cookies.delete"
                    | "browser.cookies.clear"
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
        "browser.type" | "browser.fill" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(len) = data
                .pointer("/value_summary/text_length")
                .and_then(|v| v.as_u64())
            {
                lines.push(format!("text_length: {len}"));
            }
        }
        "browser.select" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(val) = data
                .pointer("/value_summary/value")
                .and_then(|v| v.as_str())
            {
                lines.push(format!("value: {val}"));
            }
            if let Some(by_text) = data
                .pointer("/value_summary/by_text")
                .and_then(|v| v.as_bool())
            {
                lines.push(format!("by_text: {by_text}"));
            }
        }
        "browser.click" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            } else if let Some(coords) =
                data.pointer("/target/coordinates").and_then(|v| v.as_str())
            {
                lines.push(format!("target: {coords}"));
            }
        }
        "browser.hover" | "browser.focus" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
        }
        "browser.mouse-move" => {
            if let Some(coords) = data.pointer("/target/coordinates").and_then(|v| v.as_str()) {
                lines.push(format!("target: {coords}"));
            }
        }
        "browser.cursor-position" => {
            if let Some(x) = data.get("x").and_then(|v| v.as_f64()) {
                lines.push(format!("x: {}", x as i64));
            }
            if let Some(y) = data.get("y").and_then(|v| v.as_f64()) {
                lines.push(format!("y: {}", y as i64));
            }
        }
        "browser.scroll" => {
            if let Some(dir) = data.get("direction").and_then(|v| v.as_str()) {
                lines.push(format!("direction: {dir}"));
            }
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(container) = data.get("container").and_then(|v| v.as_str()) {
                lines.push(format!("container: {container}"));
            }
        }
        "browser.drag" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(sel) = data
                .pointer("/destination/selector")
                .and_then(|v| v.as_str())
            {
                lines.push(format!("destination: {sel}"));
            } else if let Some(coords) = data
                .pointer("/destination/coordinates")
                .and_then(|v| v.as_str())
            {
                lines.push(format!("destination: {coords}"));
            }
        }
        "browser.upload" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(count) = data
                .pointer("/value_summary/count")
                .and_then(|v| v.as_u64())
            {
                lines.push(format!("count: {count}"));
            }
        }
        "browser.press" => {
            if let Some(keys) = data.get("keys").and_then(|v| v.as_str()) {
                lines.push(format!("keys: {keys}"));
            }
        }
        "browser.screenshot" => {
            if let Some(path) = data.pointer("/artifact/path").and_then(|v| v.as_str()) {
                lines.push(format!("path: {path}"));
            }
        }
        "browser.snapshot" => {
            // §10.1: text mode outputs content directly (no "ok" prefix)
            if let Some(content) = data.get("content").and_then(|v| v.as_str()) {
                lines.push(content.to_string());
            }
        }
        "browser.html" | "browser.text" | "browser.value" | "browser.attr" => {
            if let Some(val) = data.get("value") {
                lines.push(text_scalar(val));
            }
        }
        "browser.title" | "browser.url" => {
            if let Some(val) = data.get("value").and_then(|v| v.as_str()) {
                lines.push(val.to_string());
            }
        }
        "browser.viewport" => {
            let width = data.get("width").and_then(|v| v.as_u64());
            let height = data.get("height").and_then(|v| v.as_u64());
            if let (Some(w), Some(h)) = (width, height) {
                lines.push(format!("{w}x{h}"));
            }
        }
        "browser.attrs" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(attrs) = data.get("value").and_then(|v| v.as_object()) {
                let mut order: Vec<String> = data
                    .get("__attr_order")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if order.is_empty() {
                    order = attrs.keys().cloned().collect();
                    order.sort();
                }
                for key in order {
                    if let Some(value) = attrs.get(&key) {
                        lines.push(format!("{key}: {}", text_scalar(value)));
                    }
                }
            }
        }
        "browser.box" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(value) = data.get("value") {
                for key in ["x", "y", "width", "height", "right", "bottom"] {
                    if let Some(field) = value.get(key) {
                        lines.push(format!("{key}: {}", text_scalar(field)));
                    }
                }
            }
        }
        "browser.styles" => {
            if let Some(sel) = data.pointer("/target/selector").and_then(|v| v.as_str()) {
                lines.push(format!("target: {sel}"));
            }
            if let Some(styles) = data.get("value").and_then(|v| v.as_object()) {
                let order: Vec<String> = data
                    .get("__prop_order")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_else(|| styles.keys().cloned().collect());
                for key in order {
                    if let Some(value) = styles.get(&key) {
                        lines.push(format!("{key}: {}", text_scalar(value)));
                    }
                }
            }
        }
        "browser.describe" => {
            let summary = {
                let role = data.get("role").and_then(|v| v.as_str()).unwrap_or("");
                let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    role.to_string()
                } else {
                    format!("{role} \"{name}\"")
                }
            };
            lines.push(summary);
            if let Some(nearby) = data.get("nearby").filter(|v| !v.is_null()) {
                if let Some(p) = nearby.get("parent").and_then(|v| v.as_str()) {
                    lines.push(format!("parent: {p}"));
                }
                if let Some(ps) = nearby.get("previous_sibling").and_then(|v| v.as_str()) {
                    lines.push(format!("previous_sibling: {ps}"));
                }
                if let Some(ns) = nearby.get("next_sibling").and_then(|v| v.as_str()) {
                    lines.push(format!("next_sibling: {ns}"));
                }
                if let Some(children) = nearby.get("children").and_then(|v| v.as_array()) {
                    for child in children {
                        if let Some(s) = child.as_str() {
                            lines.push(format!("child: {s}"));
                        }
                    }
                }
            }
        }
        "browser.state" => {
            for key in [
                "visible", "enabled", "checked", "focused", "editable", "selected",
            ] {
                if let Some(val) = data.pointer(&format!("/state/{key}")) {
                    lines.push(format!("{key}: {}", text_scalar(val)));
                }
            }
        }
        "browser.query" => {
            let mode = data.get("mode").and_then(|v| v.as_str()).unwrap_or("");
            let count = data.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            match mode {
                "one" => {
                    lines.push("1 match".to_string());
                    if let Some(item) = data.get("item") {
                        if let Some(sel) = item.get("selector").and_then(|v| v.as_str()) {
                            lines.push(format!("selector: {sel}"));
                        }
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            lines.push(format!("text: {text}"));
                        }
                    }
                }
                "all" => {
                    lines.push(format!("{count} matches"));
                    if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
                        for (i, item) in items.iter().enumerate() {
                            if let Some(sel) = item.get("selector").and_then(|v| v.as_str()) {
                                lines.push(format!("{}. {sel}", i + 1));
                            }
                            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                lines.push(format!("   {text}"));
                            }
                        }
                    }
                }
                "nth" => {
                    let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    lines.push(format!("match {index}/{count}"));
                    if let Some(item) = data.get("item") {
                        if let Some(sel) = item.get("selector").and_then(|v| v.as_str()) {
                            lines.push(format!("selector: {sel}"));
                        }
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            lines.push(format!("text: {text}"));
                        }
                    }
                }
                "count" => {
                    lines.push(format!("{count}"));
                }
                _ => {}
            }
        }
        "browser.inspect-point" => {
            // §10.11: role "name" / selector / point
            if let Some(element) = data.get("element") {
                let role = element.get("role").and_then(|v| v.as_str()).unwrap_or("");
                let name = element.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if !name.is_empty() {
                    lines.push(format!("{role} \"{name}\""));
                } else {
                    lines.push(role.to_string());
                }
                if let Some(sel) = element.get("selector").and_then(|v| v.as_str()) {
                    lines.push(format!("selector: {sel}"));
                }
            }
            if let Some(point) = data.get("point") {
                let x = point.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let y = point.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                // Format as integers if they are whole numbers
                if x.fract() == 0.0 && y.fract() == 0.0 {
                    lines.push(format!("point: {},{}", x as i64, y as i64));
                } else {
                    lines.push(format!("point: {x},{y}"));
                }
            }
        }
        "browser.pdf" => {
            // §10.3: path line
            if let Some(path) = data
                .get("artifact")
                .and_then(|a| a.get("path"))
                .and_then(|v| v.as_str())
            {
                lines.push(format!("path: {path}"));
            }
        }
        "browser.logs.console" | "browser.logs.errors" => {
            // §10.12-§10.13: N log(s) then level timestamp source text per item
            if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
                let count = items.len();
                let label = if count == 1 { "log" } else { "logs" };
                lines.push(format!("{count} {label}"));
                for item in items {
                    let level = item.get("level").and_then(|v| v.as_str()).unwrap_or("log");
                    let ts = item
                        .get("timestamp_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("");
                    let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    lines.push(format!("{level} {ts} {source} {text}"));
                }
            }
        }
        "browser.wait.element" => {
            if let Some(ms) = data.get("elapsed_ms").and_then(|v| v.as_u64()) {
                lines.push(format!("elapsed_ms: {ms}"));
            }
            if let Some(sel) = data
                .pointer("/observed_value/selector")
                .and_then(|v| v.as_str())
            {
                lines.push(format!("target: {sel}"));
            }
        }
        "browser.wait.navigation" | "browser.wait.network-idle" => {
            if let Some(ms) = data.get("elapsed_ms").and_then(|v| v.as_u64()) {
                lines.push(format!("elapsed_ms: {ms}"));
            }
        }
        "browser.wait.condition" => {
            if let Some(ms) = data.get("elapsed_ms").and_then(|v| v.as_u64()) {
                lines.push(format!("elapsed_ms: {ms}"));
            }
            if let Some(val) = data.get("observed_value") {
                lines.push(format!("observed_value: {}", text_scalar(val)));
            }
        }
        "browser.eval" => {
            if let Some(val) = data.get("value") {
                lines.push(val.as_str().unwrap_or(&val.to_string()).to_string());
            }
        }
        "browser.cookies.list" => {
            let items = data.get("items").and_then(|v| v.as_array());
            let count = items.map(|v| v.len()).unwrap_or(0);
            let label = if count == 1 { "cookie" } else { "cookies" };
            lines.push(format!("{count} {label}"));
            if let Some(items) = items {
                for item in items {
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let domain = item.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                    let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    lines.push(format!("{name} {domain} {path}"));
                }
            }
        }
        "browser.cookies.get" => {
            if let Some(item) = data.get("item") {
                if item.is_null() {
                    lines.push("item: null".to_string());
                } else {
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let domain = item.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                    let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    lines.push(format!("{name} {domain} {path}"));
                }
            }
        }
        "browser.cookies.set" | "browser.cookies.delete" | "browser.cookies.clear" => {
            // is_action already emits "ok {command}"; no additional text fields needed
        }
        _ => {
            // Generic: print data as-is
            if let Some(s) = data.as_str() {
                lines.push(s.to_string());
            }
        }
    }
}

fn text_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}
