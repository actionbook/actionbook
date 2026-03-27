use super::*;

/// Interactive ARIA roles used by the snapshot `--interactive` filter.
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "checkbox",
    "radio",
    "combobox",
    "menuitem",
    "tab",
    "switch",
    "slider",
    "spinbutton",
    "searchbox",
    "option",
    "menuitemcheckbox",
    "menuitemradio",
];

/// Filter snapshot text to only interactive-role lines.
fn filter_interactive(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            INTERACTIVE_ROLES.iter().any(|role| lower.contains(role))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip empty lines from snapshot text.
fn filter_compact(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Truncate snapshot tree to a maximum indentation depth.
fn filter_depth(text: &str, max_depth: u32) -> String {
    text.lines()
        .filter(|line| {
            let indent = line.len() - line.trim_start().len();
            indent / 2 <= max_depth as usize
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Wrap filtered snapshot output with optional cursor / selector metadata.
fn wrap_snapshot_data(output: &str, cursor: bool, selector: Option<&str>) -> serde_json::Value {
    let mut data = json!(output);
    if cursor {
        data = json!({"tree": output, "cursor": true});
    }
    if selector.is_some() {
        if let serde_json::Value::Object(ref mut map) = data {
            map.insert("selector".to_string(), json!(selector));
        } else {
            data = json!({"tree": output, "selector": selector});
        }
    }
    data
}

const ENSURE_LOG_CAPTURE_JS: &str = r#"(function() {
    if (!window.__ab_console_logs) {
        window.__ab_console_logs = [];
        const orig = {
            log: console.log,
            warn: console.warn,
            info: console.info,
            debug: console.debug,
            error: console.error
        };
        for (const [level, fn] of Object.entries(orig)) {
            console[level] = function(...args) {
                window.__ab_console_logs.push({
                    level,
                    message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '),
                    timestamp: Date.now()
                });
                fn.apply(console, args);
            };
        }
    }
    if (!window.__ab_error_logs) {
        window.__ab_error_logs = [];
        const origError = console.error;
        console.error = function(...args) {
            window.__ab_error_logs.push({
                message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '),
                timestamp: Date.now()
            });
            origError.apply(console, args);
        };
        window.addEventListener('error', function(e) {
            window.__ab_error_logs.push({
                message: e.message,
                source: e.filename,
                line: e.lineno,
                col: e.colno,
                timestamp: Date.now()
            });
        });
        window.addEventListener('unhandledrejection', function(e) {
            window.__ab_error_logs.push({
                message: 'Unhandled rejection: ' + String(e.reason),
                timestamp: Date.now()
            });
        });
    }
    return true;
})()"#;

async fn ensure_log_capture_initialized(
    backend: &mut dyn BackendSession,
    target_id: &str,
) -> Result<(), ActionResult> {
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: ENSURE_LOG_CAPTURE_JS.to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(_) => Ok(()),
        Err(e) => Err(cdp_error_to_result(e)),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_snapshot(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    interactive: bool,
    compact: bool,
    cursor: bool,
    depth: Option<u32>,
    selector: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::GetAccessibilityTree {
        target_id: target_id.to_string(),
    };

    match backend.exec(op).await {
        Ok(result) => {
            if !interactive && !compact && !cursor && depth.is_none() && selector.is_none() {
                return ActionResult::ok(result.value);
            }

            // Extract the string content from the JSON value for filtering.
            let text = match result.value {
                serde_json::Value::String(ref s) => s.clone(),
                ref other => other.to_string(),
            };

            let mut output = text;

            if interactive {
                output = filter_interactive(&output);
            }
            if compact {
                output = filter_compact(&output);
            }
            if let Some(max_depth) = depth {
                output = filter_depth(&output, max_depth);
            }

            let data = wrap_snapshot_data(&output, cursor, selector);
            ActionResult::ok(data)
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_screenshot(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    full_page: bool,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::CaptureScreenshot {
        target_id: target_id.to_string(),
        full_page,
    };

    match backend.exec(op).await {
        Ok(result) => ActionResult::ok(result.value),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_pdf(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    path: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::PrintToPdf {
        target_id: target_id.to_string(),
    };

    match backend.exec(op).await {
        Ok(result) => {
            let data = result
                .value
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if data.is_empty() {
                return ActionResult::fatal(
                    "pdf_empty",
                    "Page.printToPDF returned no data",
                    "check if the page is loaded",
                );
            }

            use base64::Engine;
            let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
                Ok(b) => b,
                Err(e) => {
                    return ActionResult::fatal(
                        "pdf_decode_error",
                        format!("failed to decode PDF data: {e}"),
                        "this is a bug",
                    )
                }
            };

            match std::fs::write(path, &bytes) {
                Ok(_) => ActionResult::ok(json!({"pdf": path, "bytes": bytes.len()})),
                Err(e) => ActionResult::fatal(
                    "pdf_write_error",
                    format!("failed to write PDF to {path}: {e}"),
                    "check the output path and permissions",
                ),
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_title(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: "document.title".to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"title": val}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_url(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: "window.location.href".to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"url": val}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_eval(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    expression: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    if let Err(result) = ensure_log_capture_initialized(backend, target_id).await {
        return result;
    }

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: expression.to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            // Check for JS exceptions in the result
            if result.value.get("exceptionDetails").is_some() {
                let desc = result
                    .value
                    .get("exceptionDetails")
                    .and_then(|e| e.get("exception"))
                    .and_then(|e| e.get("description"))
                    .and_then(|d| d.as_str())
                    .or_else(|| {
                        result
                            .value
                            .get("exceptionDetails")
                            .and_then(|e| e.get("text"))
                            .and_then(|t| t.as_str())
                    })
                    .unwrap_or("evaluation threw an exception");
                return ActionResult::fatal("eval_error", desc, "check the expression syntax");
            }
            let val = extract_eval_value(&result.value);
            ActionResult::ok(val)
        }
        Err(e) => cdp_error_to_result(e),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_query(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    mode: QueryMode,
    cardinality: QueryCardinality,
    nth_index: Option<u32>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    // Query all matching elements with metadata (PRD §10.7).
    let js = match mode {
        QueryMode::Css => format!(
            r#"(function() {{
                const els = document.querySelectorAll({selector_json});
                return Array.from(els).slice(0, 500).map((el, i) => {{
                    const rect = el.getBoundingClientRect();
                    const cs = window.getComputedStyle(el);
                    return {{
                        selector: {selector_json} + ':nth-of-type(' + (i+1) + ')',
                        tag: el.tagName.toLowerCase(),
                        text: (el.innerText || '').substring(0, 80),
                        visible: cs.display !== 'none' && cs.visibility !== 'hidden' && rect.width > 0,
                        enabled: !el.disabled
                    }};
                }});
            }})()"#
        ),
        QueryMode::Xpath => format!(
            r#"(function() {{ const result = document.evaluate({selector_json}, document, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null); const items = []; for (let i = 0; i < Math.min(result.snapshotLength, 500); i++) {{ const el = result.snapshotItem(i); if (el.nodeType === 1) {{ const rect = el.getBoundingClientRect(); const cs = window.getComputedStyle(el); items.push({{ selector: {selector_json}, tag: el.tagName.toLowerCase(), text: (el.innerText || '').substring(0, 80), visible: cs.display !== 'none' && cs.visibility !== 'hidden' && rect.width > 0, enabled: !el.disabled }}); }} }} return items; }})()"#
        ),
        QueryMode::Text => format!(
            r#"(function() {{ const text = {selector_json}; const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null); const results = []; while (walker.nextNode()) {{ if (walker.currentNode.textContent.includes(text) && results.length < 500) {{ const el = walker.currentNode.parentElement; if (el) {{ const rect = el.getBoundingClientRect(); const cs = window.getComputedStyle(el); results.push({{ selector: el.tagName.toLowerCase(), tag: el.tagName.toLowerCase(), text: (el.innerText || '').substring(0, 80), visible: cs.display !== 'none' && cs.visibility !== 'hidden' && rect.width > 0, enabled: !el.disabled }}); }} }} }} return results; }})()"#
        ),
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            let items = val.as_array().cloned().unwrap_or_default();
            let count = items.len();

            match cardinality {
                QueryCardinality::One => {
                    if count == 0 {
                        return ActionResult::fatal(
                            "ELEMENT_NOT_FOUND",
                            format!("no elements match selector '{selector}'"),
                            "check the selector or wait for the element to appear",
                        );
                    }
                    if count > 1 {
                        return ActionResult::fatal(
                            "MULTIPLE_MATCHES",
                            format!("Query mode 'one' requires exactly 1 match, found {count}"),
                            "use 'query all' or narrow your selector",
                        );
                    }
                    ActionResult::ok(json!({
                        "mode": "one",
                        "query": selector,
                        "count": 1,
                        "item": items[0],
                    }))
                }
                QueryCardinality::All => ActionResult::ok(json!({
                    "mode": "all",
                    "query": selector,
                    "count": count,
                    "items": items,
                })),
                QueryCardinality::Count => ActionResult::ok(json!({
                    "mode": "count",
                    "query": selector,
                    "count": count,
                })),
                QueryCardinality::Nth => {
                    let n = nth_index.unwrap_or(1) as usize;
                    if n == 0 || n > count {
                        return ActionResult::fatal(
                            "INDEX_OUT_OF_RANGE",
                            format!("index {n} out of range (found {count} matches)"),
                            "use 'query count' to check the number of matches first",
                        );
                    }
                    ActionResult::ok(json!({
                        "mode": "nth",
                        "query": selector,
                        "index": n,
                        "count": count,
                        "item": items[n - 1],
                    }))
                }
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_html(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let js = match selector {
        Some(sel) => {
            let sel_json = match serde_json::to_string(sel) {
                Ok(s) => s,
                Err(e) => {
                    return ActionResult::fatal(
                        "invalid_selector",
                        e.to_string(),
                        "check selector syntax",
                    )
                }
            };
            format!(
                r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({sel_json});
return el ? el.outerHTML : null;
}})()"#
            )
        }
        None => "document.documentElement.outerHTML".to_string(),
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if let Some(sel) = selector.filter(|_| val.is_null()) {
                element_not_found(sel)
            } else {
                ActionResult::ok(json!({"html": val}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_text(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let js = match selector {
        Some(sel) => {
            let sel_json = match serde_json::to_string(sel) {
                Ok(s) => s,
                Err(e) => {
                    return ActionResult::fatal(
                        "invalid_selector",
                        e.to_string(),
                        "check selector syntax",
                    )
                }
            };
            format!(
                r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({sel_json});
return el ? el.innerText : null;
}})()"#
            )
        }
        None => "document.body.innerText".to_string(),
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if let Some(sel) = selector.filter(|_| val.is_null()) {
                element_not_found(sel)
            } else {
                ActionResult::ok(json!({"text": val}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_wait_element(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    timeout_ms: Option<u64>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
    let poll_interval = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + timeout;

    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
return __findElement({selector_json}) !== null;
}})()"#
    );

    loop {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: js.clone(),
            return_by_value: true,
        };

        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                if val.as_bool() == Some(true) {
                    return ActionResult::ok(json!({"found": selector}));
                }
            }
            Err(e) => return cdp_error_to_result(e),
        }

        if tokio::time::Instant::now() >= deadline {
            return ActionResult::retryable(
                "element_timeout",
                format!(
                    "element '{}' not found within {}ms — use `actionbook browser snapshot` to see available elements",
                    selector,
                    timeout.as_millis()
                ),
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub(super) async fn handle_value(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
return el.value;
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.is_null() {
                element_not_found(selector)
            } else {
                ActionResult::ok(json!({"value": val, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_attr(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    attr_name: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };
    let attr_json = match serde_json::to_string(attr_name) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_attr_name", e.to_string(), "check attribute name")
        }
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return {{ __notfound: true }};
return el.getAttribute({attr_json});
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.get("__notfound").is_some() {
                element_not_found(selector)
            } else {
                ActionResult::ok(json!({"attr": attr_name, "value": val, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_attrs(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
const attrs = {{}};
for (const a of el.attributes) {{ attrs[a.name] = a.value; }}
return attrs;
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.is_null() {
                element_not_found(selector)
            } else {
                ActionResult::ok(json!({"attributes": val, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_describe(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
const rect = el.getBoundingClientRect();
return {{ tag: el.tagName.toLowerCase(), role: el.getAttribute('role') || '', text: (el.innerText || '').substring(0, 200), id: el.id || '', className: el.className || '', ariaLabel: el.getAttribute('aria-label') || '', href: el.href || '', type: el.type || '', name: el.name || '', value: el.value || '', placeholder: el.placeholder || '', x: rect.left, y: rect.top, width: rect.width, height: rect.height }};
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.is_null() {
                element_not_found(selector)
            } else {
                ActionResult::ok(json!({"description": val, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_state(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
const rect = el.getBoundingClientRect();
const style = window.getComputedStyle(el);
return {{ visible: rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none', enabled: !el.disabled, checked: !!el.checked, selected: !!el.selected, focused: document.activeElement === el, required: !!el.required, readOnly: !!el.readOnly }};
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.is_null() {
                element_not_found(selector)
            } else {
                ActionResult::ok(json!({"state": val, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_box(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
const rect = el.getBoundingClientRect();
return {{ x: rect.left, y: rect.top, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom }};
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.is_null() {
                element_not_found(selector)
            } else {
                ActionResult::ok(json!({"box": val, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_styles(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    names: &[String],
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    // If names are specified, only retrieve those; otherwise use default set.
    let props_js = if names.is_empty() {
        "['display','visibility','opacity','color','backgroundColor','fontSize','fontWeight','fontFamily','margin','padding','border','position','zIndex','overflow','cursor','width','height']".to_string()
    } else {
        serde_json::to_string(names).unwrap_or_else(|_| "[]".to_string())
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
const cs = window.getComputedStyle(el);
const props = {props_js};
const result = {{}};
for (const p of props) {{ result[p] = cs.getPropertyValue(p.replace(/([A-Z])/g, '-$1').toLowerCase()); }}
return result;
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.is_null() {
                element_not_found(selector)
            } else {
                ActionResult::ok(json!({"styles": val, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_viewport(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: "JSON.stringify({width: window.innerWidth, height: window.innerHeight, scrollX: window.scrollX, scrollY: window.scrollY, scrollWidth: document.documentElement.scrollWidth, scrollHeight: document.documentElement.scrollHeight})".to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let raw = extract_eval_value(&result.value);
            let val = if let Some(s) = raw.as_str() {
                serde_json::from_str(s).unwrap_or(raw)
            } else {
                raw
            };
            ActionResult::ok(json!({"viewport": val}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_inspect_point(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    x: f64,
    y: f64,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let js = format!(
        r#"(function() {{ const el = document.elementFromPoint({x}, {y}); if (!el) return null; const rect = el.getBoundingClientRect(); return {{ tag: el.tagName.toLowerCase(), id: el.id || '', className: el.className || '', text: (el.innerText || '').substring(0, 200), role: el.getAttribute('role') || '', ariaLabel: el.getAttribute('aria-label') || '', href: el.href || '', x: rect.left, y: rect.top, width: rect.width, height: rect.height }}; }})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"element": val, "x": x, "y": y}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_logs_console(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    level: Option<&str>,
    tail: Option<u32>,
    since: Option<&str>,
    clear: bool,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    if let Err(result) = ensure_log_capture_initialized(backend, target_id).await {
        return result;
    }

    let limit = tail.unwrap_or(200);
    let clear_after = clear;

    // Build JS to retrieve (and optionally filter) logs
    let level_filter = match level {
        Some(lvl) => format!(
            ".filter(l => l.level === {lvl_json})",
            lvl_json = serde_json::to_string(lvl).unwrap_or_else(|_| format!("\"{lvl}\""))
        ),
        None => String::new(),
    };
    let since_filter = match since {
        Some(ts) => format!(
            ".filter(l => l.timestamp >= {ts_json})",
            ts_json = serde_json::to_string(ts).unwrap_or_else(|_| format!("\"{ts}\""))
        ),
        None => String::new(),
    };
    let clear_stmt = if clear_after {
        "window.__ab_console_logs = [];"
    } else {
        ""
    };

    let js = format!(
        r#"(function() {{ if (!window.__ab_console_logs) {{ return []; }} const logs = window.__ab_console_logs{level_filter}{since_filter}.slice(-{limit}); {clear_stmt} return logs; }})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"logs": val}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_logs_errors(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    source: Option<&str>,
    tail: Option<u32>,
    since: Option<&str>,
    clear: bool,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    if let Err(result) = ensure_log_capture_initialized(backend, target_id).await {
        return result;
    }

    let limit = tail.unwrap_or(200);
    let clear_after = clear;

    let source_filter = match source {
        Some(src) => format!(
            ".filter(e => e.source === {src_json})",
            src_json = serde_json::to_string(src).unwrap_or_else(|_| format!("\"{src}\""))
        ),
        None => String::new(),
    };
    let since_filter = match since {
        Some(ts) => format!(
            ".filter(e => e.timestamp >= {ts_json})",
            ts_json = serde_json::to_string(ts).unwrap_or_else(|_| format!("\"{ts}\""))
        ),
        None => String::new(),
    };
    let clear_stmt = if clear_after {
        "window.__ab_error_logs = [];"
    } else {
        ""
    };

    let js = format!(
        r#"(function() {{ if (!window.__ab_error_logs) {{ return []; }} const errors = window.__ab_error_logs{source_filter}{since_filter}.slice(-{limit}); {clear_stmt} return errors; }})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"errors": val}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- filter_interactive tests --

    #[test]
    fn filter_interactive_keeps_button_lines() {
        let input = "root\n  button Submit\n  div Container\n  link Home";
        let result = filter_interactive(input);
        assert!(result.contains("button Submit"));
        assert!(result.contains("link Home"));
        assert!(!result.contains("div Container"));
        assert!(!result.contains("root"));
    }

    #[test]
    fn filter_interactive_is_case_insensitive() {
        let input = "  Button Login\n  LINK Signup\n  TextBox Email";
        let result = filter_interactive(input);
        assert_eq!(result.lines().count(), 3);
    }

    #[test]
    fn filter_interactive_returns_empty_for_no_matches() {
        let input = "div\n  span\n  img";
        let result = filter_interactive(input);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_interactive_all_roles_recognized() {
        for role in INTERACTIVE_ROLES {
            let line = format!("  {} test", role);
            let result = filter_interactive(&line);
            assert!(
                !result.is_empty(),
                "role '{}' should be recognized as interactive",
                role
            );
        }
    }

    // -- filter_compact tests --

    #[test]
    fn filter_compact_removes_empty_lines() {
        let input = "line1\n\n  \n\nline2\n   \nline3";
        let result = filter_compact(input);
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn filter_compact_preserves_indentation() {
        let input = "  indented\n\n    deeper";
        let result = filter_compact(input);
        assert_eq!(result, "  indented\n    deeper");
    }

    #[test]
    fn filter_compact_handles_already_compact() {
        let input = "line1\nline2";
        let result = filter_compact(input);
        assert_eq!(result, "line1\nline2");
    }

    // -- filter_depth tests --

    #[test]
    fn filter_depth_zero_keeps_root_only() {
        let input = "root\n  child\n    grandchild";
        let result = filter_depth(input, 0);
        assert_eq!(result, "root");
    }

    #[test]
    fn filter_depth_one_keeps_first_level() {
        let input = "root\n  child1\n  child2\n    grandchild";
        let result = filter_depth(input, 1);
        assert_eq!(result, "root\n  child1\n  child2");
    }

    #[test]
    fn filter_depth_large_value_keeps_all() {
        let input = "root\n  child\n    grandchild\n      deep";
        let result = filter_depth(input, 100);
        assert_eq!(result, input);
    }

    // -- wrap_snapshot_data tests --

    #[test]
    fn wrap_snapshot_plain() {
        let data = wrap_snapshot_data("tree text", false, None);
        assert_eq!(data, json!("tree text"));
    }

    #[test]
    fn wrap_snapshot_with_cursor() {
        let data = wrap_snapshot_data("tree text", true, None);
        assert_eq!(data["tree"], "tree text");
        assert_eq!(data["cursor"], true);
    }

    #[test]
    fn wrap_snapshot_with_selector() {
        let data = wrap_snapshot_data("tree text", false, Some("#main"));
        assert_eq!(data["tree"], "tree text");
        assert_eq!(data["selector"], "#main");
    }

    #[test]
    fn wrap_snapshot_with_cursor_and_selector() {
        let data = wrap_snapshot_data("tree text", true, Some("div.content"));
        assert_eq!(data["tree"], "tree text");
        assert_eq!(data["cursor"], true);
        assert_eq!(data["selector"], "div.content");
    }

    // -- combined filter tests --

    #[test]
    fn filter_interactive_then_compact() {
        let input = "root\n\n  button Submit\n\n  div Container\n\n  link Home\n\n";
        let result = filter_compact(&filter_interactive(input));
        assert_eq!(result, "  button Submit\n  link Home");
    }

    #[test]
    fn filter_interactive_then_depth() {
        let input = "root\n  button Submit\n    link Nested\n      checkbox Deep";
        let result = filter_depth(&filter_interactive(input), 1);
        assert_eq!(result, "  button Submit");
    }
}
