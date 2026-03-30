use super::*;
use crate::browser::readability::READABILITY_JS;
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
    "listbox",
    "menuitem",
    "tab",
    "treeitem",
    "switch",
    "slider",
    "spinbutton",
    "searchbox",
    "option",
    "menuitemcheckbox",
    "menuitemradio",
];

const ENSURE_LOG_CAPTURE_JS: &str = r#"(function() {
    if (typeof window.__ab_log_seq === 'undefined') { window.__ab_log_seq = 0; }
    if (typeof window.__ab_err_seq === 'undefined') { window.__ab_err_seq = 0; }
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
                    id: 'log-' + (++window.__ab_log_seq),
                    level,
                    text: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '),
                    source: '',
                    timestamp_ms: Date.now()
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
                id: 'err-' + (++window.__ab_err_seq),
                level: 'error',
                text: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '),
                source: '',
                timestamp_ms: Date.now()
            });
            origError.apply(console, args);
        };
        window.addEventListener('error', function(e) {
            window.__ab_error_logs.push({
                id: 'err-' + (++window.__ab_err_seq),
                level: 'error',
                text: e.message,
                source: e.filename || '',
                timestamp_ms: Date.now()
            });
        });
        window.addEventListener('unhandledrejection', function(e) {
            window.__ab_error_logs.push({
                id: 'err-' + (++window.__ab_err_seq),
                level: 'error',
                text: 'Unhandled rejection: ' + String(e.reason),
                source: '',
                timestamp_ms: Date.now()
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

fn fallback_tab_context(regs: &Registries, tab: TabId) -> (Option<String>, Option<String>) {
    regs.find_tab(tab)
        .map(|entry| (Some(entry.url.clone()), Some(entry.title.clone())))
        .unwrap_or((None, None))
}

async fn live_tab_context(
    backend: &mut dyn BackendSession,
    target_id: &str,
    regs: &Registries,
    tab: TabId,
) -> (Option<String>, Option<String>) {
    let fallback = fallback_tab_context(regs, tab);
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: "(() => ({ url: location.href, title: document.title || '' }))()".to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            let url = val
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or(fallback.0);
            let title = match val.get("title").and_then(|v| v.as_str()) {
                Some("") => fallback.1,
                Some(title) => Some(title.to_string()),
                None => fallback.1,
            };
            (url, title)
        }
        Err(_) => fallback,
    }
}

async fn ok_with_tab_context(
    backend: &mut dyn BackendSession,
    target_id: &str,
    regs: &Registries,
    tab: TabId,
    data: Value,
) -> ActionResult {
    let (url, title) = live_tab_context(backend, target_id, regs, tab).await;
    let mut object = match data {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };
    object.insert(
        "__ctx_url".to_string(),
        url.map(Value::String).unwrap_or(Value::Null),
    );
    object.insert(
        "__ctx_title".to_string(),
        title.map(Value::String).unwrap_or(Value::Null),
    );
    ActionResult::ok(Value::Object(object))
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

async fn query_context_fields(
    backend: &mut dyn BackendSession,
    regs: &Registries,
    target_id: &str,
    tab: TabId,
) -> (String, String) {
    let registry_fallback = regs
        .tabs
        .get(&tab)
        .map(|entry| (entry.url.clone(), entry.title.clone()))
        .unwrap_or_default();

    let context_result = backend
        .exec(BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression:
                r#"(function(){ return { url: window.location.href, title: document.title }; })()"#
                    .to_string(),
            return_by_value: true,
        })
        .await;

    let Ok(result) = context_result else {
        return registry_fallback;
    };

    let live_context = extract_eval_value(&result.value);

    let live_url = live_context
        .get("url")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|value| !value.is_empty());
    let live_title = live_context
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    (
        live_url.unwrap_or(registry_fallback.0),
        live_title.unwrap_or(registry_fallback.1),
    )
}

fn with_query_context(
    ctx_url: &str,
    ctx_title: &str,
    value: serde_json::Value,
) -> serde_json::Value {
    let mut object = value.as_object().cloned().unwrap_or_default();
    object.insert("__ctx_url".to_string(), json!(ctx_url));
    object.insert("__ctx_title".to_string(), json!(ctx_title));
    serde_json::Value::Object(object)
}

fn css_query_script(selector_json: &str) -> String {
    format!(
        r#"(function() {{
            const raw = {selector_json};

            function unquote(input) {{
                const trimmed = String(input || '').trim();
                if ((trimmed.startsWith('"') && trimmed.endsWith('"')) || (trimmed.startsWith("'") && trimmed.endsWith("'"))) {{
                    return trimmed.slice(1, -1);
                }}
                return trimmed;
            }}

            function extractCalls(input, pseudo) {{
                const marker = ':' + pseudo + '(';
                const values = [];
                let remaining = String(input || '').trim();
                while (true) {{
                    const start = remaining.indexOf(marker);
                    if (start === -1) {{
                        break;
                    }}
                    let depth = 0;
                    let end = -1;
                    for (let i = start + marker.length - 1; i < remaining.length; i++) {{
                        const ch = remaining[i];
                        if (ch === '(') {{
                            depth += 1;
                        }} else if (ch === ')') {{
                            depth -= 1;
                            if (depth === 0) {{
                                end = i;
                                break;
                            }}
                        }}
                    }}
                    if (end === -1) {{
                        break;
                    }}
                    values.push(remaining.slice(start + marker.length, end));
                    remaining = (remaining.slice(0, start) + remaining.slice(end + 1)).trim();
                }}
                return {{ remaining, values }};
            }}

            function extractTrailingFlag(input, suffix) {{
                let remaining = String(input || '').trim();
                let enabled = false;
                while (remaining.endsWith(suffix)) {{
                    enabled = true;
                    remaining = remaining.slice(0, -suffix.length).trim();
                }}
                return {{ remaining, enabled }};
            }}

            function isVisible(el) {{
                const rect = el.getBoundingClientRect();
                const cs = window.getComputedStyle(el);
                return cs.display !== 'none' && cs.visibility !== 'hidden' && rect.width > 0 && rect.height > 0;
            }}

            let working = String(raw || '').trim();
            const containsInfo = extractCalls(working, 'contains');
            working = containsInfo.remaining;
            const hasInfo = extractCalls(working, 'has');
            working = hasInfo.remaining;
            const visibleInfo = extractTrailingFlag(working, ':visible');
            working = visibleInfo.remaining;
            const enabledInfo = extractTrailingFlag(working, ':enabled');
            working = enabledInfo.remaining;
            const disabledInfo = extractTrailingFlag(working, ':disabled');
            working = disabledInfo.remaining;
            const checkedInfo = extractTrailingFlag(working, ':checked');
            working = checkedInfo.remaining;

            const baseSelector = working || '*';
            const containsTexts = containsInfo.values.map(unquote).filter(Boolean);
            const hasSelectors = hasInfo.values.map(unquote).filter(Boolean);

            let elements = [];
            try {{
                elements = Array.from(document.querySelectorAll(baseSelector));
            }} catch (_error) {{
                return [];
            }}

            return elements
                .filter((el) => {{
                    if (visibleInfo.enabled && !isVisible(el)) {{
                        return false;
                    }}
                    if (enabledInfo.enabled && !!el.disabled) {{
                        return false;
                    }}
                    if (disabledInfo.enabled && !el.disabled) {{
                        return false;
                    }}
                    if (checkedInfo.enabled && !el.checked) {{
                        return false;
                    }}

                    const text = (el.innerText || el.textContent || '').trim();
                    if (containsTexts.some((needle) => !text.includes(needle))) {{
                        return false;
                    }}
                    for (const selector of hasSelectors) {{
                        try {{
                            if (!el.querySelector(selector)) {{
                                return false;
                            }}
                        }} catch (_error) {{
                            return false;
                        }}
                    }}
                    return true;
                }})
                .slice(0, 500)
                .map((el, i) => {{
                    const rect = el.getBoundingClientRect();
                    const cs = window.getComputedStyle(el);
                    return {{
                        selector: raw + ':nth-of-type(' + (i + 1) + ')',
                        tag: el.tagName.toLowerCase(),
                        text: isVisible(el) ? (el.innerText || el.textContent || '').trim().substring(0, 80) : '',
                        visible: cs.display !== 'none' && cs.visibility !== 'hidden' && rect.width > 0 && rect.height > 0,
                        enabled: !el.disabled
                    }};
                }});
        }})()"#
    )
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
    let (ctx_url, ctx_title) = query_context_fields(backend, regs, target_id, tab).await;
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    // Query all matching elements with metadata (PRD §10.7).
    let js = match mode {
        QueryMode::Css => css_query_script(&selector_json),
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
            let sample_selectors: Vec<serde_json::Value> = items
                .iter()
                .filter_map(|item| item.get("selector").and_then(|value| value.as_str()))
                .take(3)
                .map(|selector| json!(selector))
                .collect();

            match cardinality {
                QueryCardinality::One => {
                    if count == 0 {
                        return ActionResult::fatal_with_details(
                            "ELEMENT_NOT_FOUND",
                            format!("no elements match selector '{selector}'"),
                            "check the selector or wait for the element to appear",
                            json!({
                                "query": selector,
                                "count": 0
                            }),
                        );
                    }
                    if count > 1 {
                        return ActionResult::fatal_with_details(
                            "MULTIPLE_MATCHES",
                            format!("Query mode 'one' requires exactly 1 match, found {count}"),
                            "use 'query all' or narrow your selector",
                            json!({
                                "query": selector,
                                "count": count,
                                "sample_selectors": sample_selectors.clone(),
                            }),
                        );
                    }
                    ActionResult::ok(with_query_context(
                        &ctx_url,
                        &ctx_title,
                        json!({
                            "mode": "one",
                            "query": selector,
                            "count": 1,
                            "item": items[0],
                        }),
                    ))
                }
                QueryCardinality::All => ActionResult::ok(with_query_context(
                    &ctx_url,
                    &ctx_title,
                    json!({
                        "mode": "all",
                        "query": selector,
                        "count": count,
                        "items": items,
                    }),
                )),
                QueryCardinality::Count => ActionResult::ok(with_query_context(
                    &ctx_url,
                    &ctx_title,
                    json!({
                        "mode": "count",
                        "query": selector,
                        "count": count,
                    }),
                )),
                QueryCardinality::Nth => {
                    let n = nth_index.unwrap_or(1) as usize;
                    if n == 0 || n > count {
                        return ActionResult::fatal_with_details(
                            "INDEX_OUT_OF_RANGE",
                            format!("index {n} out of range (found {count} matches)"),
                            "use 'query count' to check the number of matches first",
                            json!({
                                "query": selector,
                                "count": count,
                                "index": n,
                                "sample_selectors": sample_selectors.clone(),
                            }),
                        );
                    }
                    ActionResult::ok(with_query_context(
                        &ctx_url,
                        &ctx_title,
                        json!({
                            "mode": "nth",
                            "query": selector,
                            "index": n,
                            "count": count,
                            "item": items[n - 1],
                        }),
                    ))
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
                ok_with_tab_context(backend, target_id, regs, tab, json!({"html": val})).await
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
    mode: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let js = match (selector, mode) {
        (Some(sel), Some("readability")) => {
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
if (!el) return null;
const root = el.cloneNode(true);
const removeSelectors = ['nav','footer','aside','header','script','style','noscript','template','svg','[hidden]','[aria-hidden="true"]','[style*="display:none"]','[style*="display: none"]'];
for (const selector of removeSelectors) {{
  root.querySelectorAll(selector).forEach(node => node.remove());
}}
const text = root.innerText || root.textContent || '';
return text.replace(/\n{{3,}}/g, '\n\n').replace(/[ \t]+/g, ' ').trim();
}})()"#
            )
        }
        (None, Some("readability")) => READABILITY_JS.to_string(),
        (Some(sel), _) => {
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
        (None, _) => "document.body.innerText".to_string(),
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
                ok_with_tab_context(backend, target_id, regs, tab, json!({"text": val})).await
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
                ok_with_tab_context(
                    backend,
                    target_id,
                    regs,
                    tab,
                    json!({"value": val, "selector": selector}),
                )
                .await
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
                ok_with_tab_context(
                    backend,
                    target_id,
                    regs,
                    tab,
                    json!({"attr": attr_name, "value": val, "selector": selector}),
                )
                .await
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
                ok_with_tab_context(
                    backend,
                    target_id,
                    regs,
                    tab,
                    json!({"attributes": val, "selector": selector}),
                )
                .await
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
    nearby: bool,
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
    let nearby_js = if nearby { "true" } else { "false" };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
function gr(e) {{ const r=e.getAttribute('role'); if(r) return r; const t=e.tagName.toLowerCase(); if(t==='a') return'link'; if(t==='button') return'button'; if(t==='input'){{ const tp=(e.type||'text').toLowerCase(); if(tp==='checkbox') return'checkbox'; if(tp==='radio') return'radio'; if(tp==='submit'||tp==='button'||tp==='reset') return'button'; return'textbox'; }} if(t==='select') return'combobox'; if(t==='textarea') return'textbox'; return t; }}
function gn(e) {{ const l=e.getAttribute('aria-label'); if(l) return l.trim(); const lb=e.getAttribute('aria-labelledby'); if(lb){{ const le=document.getElementById(lb); if(le) return(le.innerText||'').trim(); }} if(e.placeholder) return e.placeholder; if(e.title) return e.title; return(e.innerText||'').trim().substring(0,50); }}
function ga(e) {{ const a={{}}; if(e.type) a.type=e.type; if(e.href) a.href=e.href; return a; }}
function gst(e) {{ const r=e.getBoundingClientRect(); const s=window.getComputedStyle(e); return{{visible:r.width>0&&r.height>0&&s.visibility!=='hidden'&&s.display!=='none',enabled:!e.disabled}}; }}
function sm(e) {{ const r=gr(e); const n=gn(e); return n?r+' "'+n.replace(/"/g,'\\"')+'"':r; }}
const res={{role:gr(el),name:gn(el),tag:el.tagName.toLowerCase(),attributes:ga(el),state:gst(el),nearby:null}};
if({nearby_js}){{ const par=el.parentElement; const prv=el.previousElementSibling; const nxt=el.nextElementSibling; const chl=Array.from(el.children).slice(0,3); res.nearby={{parent:par?sm(par):null,previous_sibling:prv?sm(prv):null,next_sibling:nxt?sm(nxt):null,children:chl.map(sm)}}; }}
return res;
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
                ActionResult::ok(json!({"__describe": val, "selector": selector}))
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
const editable = !el.disabled && !el.readOnly && (typeof el.value !== 'undefined' || el.isContentEditable);
return {{ visible: rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none', enabled: !el.disabled, checked: !!el.checked, focused: document.activeElement === el, editable, selected: !!el.selected }};
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
                ok_with_tab_context(
                    backend,
                    target_id,
                    regs,
                    tab,
                    json!({"state": val, "selector": selector}),
                )
                .await
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
                ok_with_tab_context(
                    backend,
                    target_id,
                    regs,
                    tab,
                    json!({"box": val, "selector": selector}),
                )
                .await
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
                ok_with_tab_context(
                    backend,
                    target_id,
                    regs,
                    tab,
                    json!({"styles": val, "selector": selector}),
                )
                .await
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
    _parent_depth: Option<u32>, // TODO(follow-up): walk DOM ancestors up to depth N and populate __parents
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let js = format!(
        r#"(function() {{
const el = document.elementFromPoint({x}, {y});
if (!el) return null;
function gr(e) {{ const r=e.getAttribute('role'); if(r) return r; const t=e.tagName.toLowerCase(); if(t==='a') return'link'; if(t==='button') return'button'; if(t==='input'){{ const tp=(e.type||'text').toLowerCase(); if(tp==='checkbox') return'checkbox'; if(tp==='radio') return'radio'; if(tp==='submit'||tp==='button'||tp==='reset') return'button'; return'textbox'; }} if(t==='select') return'combobox'; if(t==='textarea') return'textbox'; return t; }}
function gn(e) {{ const l=e.getAttribute('aria-label'); if(l) return l.trim(); if(e.title) return e.title; return(e.innerText||'').trim().substring(0,50); }}
function gsel(e) {{ if(e.id) return'#'+e.id; let s=e.tagName.toLowerCase(); const cls=e.className&&typeof e.className==='string'?e.className.trim():''; if(cls) s+='.'+cls.split(/\s+/).join('.'); return s; }}
return{{ role:gr(el), name:gn(el), selector:gsel(el) }};
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
            ActionResult::ok(
                json!({"__element": val, "x": x, "y": y, "__parents": [], "__screenshot_path": null}),
            )
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
        Some(id) => {
            let id_json = serde_json::to_string(id).unwrap_or_else(|_| format!("\"{id}\""));
            format!(".filter(l => parseInt((l.id||'').split('-')[1]||'0') > parseInt(({id_json}).split('-')[1]||'0'))")
        }
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
        Some(id) => {
            let id_json = serde_json::to_string(id).unwrap_or_else(|_| format!("\"{id}\""));
            format!(".filter(e => parseInt((e.id||'').split('-')[1]||'0') > parseInt(({id_json}).split('-')[1]||'0'))")
        }
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
        assert!(INTERACTIVE_ROLES.contains(&"listbox"));
        assert!(INTERACTIVE_ROLES.contains(&"menuitemradio"));
        assert!(INTERACTIVE_ROLES.contains(&"treeitem"));
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
