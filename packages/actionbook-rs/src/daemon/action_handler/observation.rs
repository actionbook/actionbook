use super::*;
use crate::browser::snapshot::{
    format_compact, parse_ax_tree, remove_empty_leaves, SnapshotFilter,
};

/// Interactive ARIA roles used for counting interactive nodes in snapshot stats.
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

    // Look up the tab's url/title to embed in the result for context population.
    let (ctx_url, ctx_title) = regs
        .tabs
        .get(&tab)
        .map(|t| (t.url.clone(), t.title.clone()))
        .unwrap_or_default();

    match backend.exec(op).await {
        Ok(result) => {
            let filter = if interactive {
                SnapshotFilter::Interactive
            } else {
                SnapshotFilter::All
            };
            let max_depth_usize = depth.map(|d| d as usize);

            // Parse the raw CDP accessibility tree into structured A11yNodes.
            let (mut nodes, _cache) =
                match parse_ax_tree(result.value, filter, max_depth_usize, None) {
                    Ok(parsed) => parsed,
                    Err(_) => {
                        return ActionResult::fatal(
                            "SNAPSHOT_PARSE_ERROR",
                            "Failed to parse accessibility tree",
                            "Check that the page has loaded and try again",
                        );
                    }
                };

            if compact {
                nodes = remove_empty_leaves(&nodes);
            }

            // Render text content from parsed nodes.
            let content = format_compact(&nodes);

            // Build PRD 10.1 nodes array: only nodes with refs.
            let interactive_set: std::collections::HashSet<&str> =
                INTERACTIVE_ROLES.iter().copied().collect();
            let prd_nodes: Vec<serde_json::Value> = nodes
                .iter()
                .filter(|n| n.ref_id.is_some())
                .map(|n| {
                    json!({
                        "ref": n.ref_id.as_deref().unwrap_or(""),
                        "role": n.role,
                        "name": n.name,
                        "value": n.value.as_deref().unwrap_or("")
                    })
                })
                .collect();

            let interactive_count = prd_nodes
                .iter()
                .filter(|n| {
                    n.get("role")
                        .and_then(|v| v.as_str())
                        .map(|r| interactive_set.contains(r))
                        .unwrap_or(false)
                })
                .count();

            let mut data = json!({
                "__ctx_url": ctx_url,
                "__ctx_title": ctx_title,
                "format": "snapshot",
                "content": content,
                "nodes": prd_nodes,
                "stats": {
                    "node_count": prd_nodes.len(),
                    "interactive_count": interactive_count
                }
            });

            if cursor {
                if let serde_json::Value::Object(ref mut map) = data {
                    map.insert("cursor".to_string(), json!(true));
                }
            }
            if selector.is_some() {
                if let serde_json::Value::Object(ref mut map) = data {
                    map.insert("selector".to_string(), json!(selector));
                }
            }
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

    // -- INTERACTIVE_ROLES tests --

    #[test]
    fn interactive_roles_contains_expected_roles() {
        assert!(INTERACTIVE_ROLES.contains(&"button"));
        assert!(INTERACTIVE_ROLES.contains(&"link"));
        assert!(INTERACTIVE_ROLES.contains(&"textbox"));
        assert!(INTERACTIVE_ROLES.contains(&"checkbox"));
        assert!(INTERACTIVE_ROLES.contains(&"menuitemradio"));
    }

    // -- Snapshot parsing tests using parse_ax_tree --

    #[test]
    fn snapshot_parse_produces_prd_nodes() {
        // Simulate a minimal CDP Accessibility.getFullAXTree response
        let cdp_response = json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "role": {"type": "role", "value": "RootWebArea"},
                    "name": {"type": "computedString", "value": "Test Page"},
                    "childIds": ["2", "3"],
                    "properties": []
                },
                {
                    "nodeId": "2",
                    "backendDOMNodeId": 10,
                    "role": {"type": "role", "value": "button"},
                    "name": {"type": "computedString", "value": "Submit"},
                    "childIds": [],
                    "properties": []
                },
                {
                    "nodeId": "3",
                    "backendDOMNodeId": 11,
                    "role": {"type": "role", "value": "link"},
                    "name": {"type": "computedString", "value": "Home"},
                    "childIds": [],
                    "properties": []
                }
            ]
        });

        let (nodes, _cache) = parse_ax_tree(cdp_response, SnapshotFilter::All, None, None).unwrap();

        // Should have 2 nodes (RootWebArea is unwrapped)
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].role, "button");
        assert_eq!(nodes[0].name, "Submit");
        assert!(nodes[0].ref_id.is_some());
        assert_eq!(nodes[1].role, "link");
        assert_eq!(nodes[1].name, "Home");
        assert!(nodes[1].ref_id.is_some());
    }

    #[test]
    fn snapshot_interactive_filter_via_parse() {
        let cdp_response = json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "role": {"type": "role", "value": "RootWebArea"},
                    "name": {"type": "computedString", "value": ""},
                    "childIds": ["2", "3"],
                    "properties": []
                },
                {
                    "nodeId": "2",
                    "backendDOMNodeId": 10,
                    "role": {"type": "role", "value": "button"},
                    "name": {"type": "computedString", "value": "Click"},
                    "childIds": [],
                    "properties": []
                },
                {
                    "nodeId": "3",
                    "backendDOMNodeId": 11,
                    "role": {"type": "role", "value": "heading"},
                    "name": {"type": "computedString", "value": "Title"},
                    "childIds": [],
                    "properties": [
                        {"name": "level", "value": {"type": "integer", "value": 1}}
                    ]
                }
            ]
        });

        let (nodes, _cache) =
            parse_ax_tree(cdp_response, SnapshotFilter::Interactive, None, None).unwrap();

        // Only the button should remain (heading is content, not interactive)
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].role, "button");
    }

    #[test]
    fn snapshot_prd_node_shape() {
        let cdp_response = json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "role": {"type": "role", "value": "RootWebArea"},
                    "name": {"type": "computedString", "value": ""},
                    "childIds": ["2"],
                    "properties": []
                },
                {
                    "nodeId": "2",
                    "backendDOMNodeId": 10,
                    "role": {"type": "role", "value": "textbox"},
                    "name": {"type": "computedString", "value": "Search"},
                    "value": {"type": "string", "value": "hello"},
                    "childIds": [],
                    "properties": []
                }
            ]
        });

        let (nodes, _cache) = parse_ax_tree(cdp_response, SnapshotFilter::All, None, None).unwrap();

        // Build PRD node shape
        let interactive_set: std::collections::HashSet<&str> =
            INTERACTIVE_ROLES.iter().copied().collect();
        let prd_nodes: Vec<serde_json::Value> = nodes
            .iter()
            .filter(|n| n.ref_id.is_some())
            .map(|n| {
                json!({
                    "ref": n.ref_id.as_deref().unwrap_or(""),
                    "role": n.role,
                    "name": n.name,
                    "value": n.value.as_deref().unwrap_or("")
                })
            })
            .collect();

        assert_eq!(prd_nodes.len(), 1);
        assert_eq!(prd_nodes[0]["ref"], "e0");
        assert_eq!(prd_nodes[0]["role"], "textbox");
        assert_eq!(prd_nodes[0]["name"], "Search");
        assert_eq!(prd_nodes[0]["value"], "hello");

        let interactive_count = prd_nodes
            .iter()
            .filter(|n| {
                n.get("role")
                    .and_then(|v| v.as_str())
                    .map(|r| interactive_set.contains(r))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(interactive_count, 1);
    }
}
