use super::*;

pub(super) async fn handle_click(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    button: Option<&str>,
    count: Option<u32>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    // Use JS to find element, scroll into view, and get center coordinates.
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => {
            return ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
        }
    };

    let find_js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
el.scrollIntoView({{ behavior: 'instant', block: 'center', inline: 'center' }});
const rect = el.getBoundingClientRect();
return {{ x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }};
}})()"#
    );

    let eval_op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: find_js,
        return_by_value: true,
    };

    let coords = match backend.exec(eval_op).await {
        Ok(r) => r.value,
        Err(e) => return cdp_error_to_result(e),
    };

    let coords = extract_eval_value(&coords);

    if coords.is_null() {
        return element_not_found(selector);
    }

    let x = match coords.get("x").and_then(|v| v.as_f64()) {
        Some(v) => v,
        None => {
            return ActionResult::fatal(
                "invalid_coordinates",
                "element returned no x coordinate",
                "check selector",
            )
        }
    };
    let y = match coords.get("y").and_then(|v| v.as_f64()) {
        Some(v) => v,
        None => {
            return ActionResult::fatal(
                "invalid_coordinates",
                "element returned no y coordinate",
                "check selector",
            )
        }
    };

    let btn = button.unwrap_or("left").to_string();
    let click_count = count.unwrap_or(1) as i32;

    // mouseMoved -> mousePressed -> mouseReleased
    for (event_type, cc) in [
        ("mouseMoved", 0),
        ("mousePressed", click_count),
        ("mouseReleased", click_count),
    ] {
        let op = BackendOp::DispatchMouseEvent {
            target_id: target_id.to_string(),
            event_type: event_type.to_string(),
            x,
            y,
            button: btn.clone(),
            click_count: cc,
        };
        if let Err(e) = backend.exec(op).await {
            return cdp_error_to_result(e);
        }
    }

    ActionResult::ok(json!({"clicked": selector, "x": x, "y": y}))
}

pub(super) async fn handle_type(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    text: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    // Focus the element first
    if let Err(r) = focus_element(backend, target_id, selector).await {
        return r;
    }

    // Type each character as keyDown + keyUp
    for c in text.chars() {
        let char_str = c.to_string();
        let down = BackendOp::DispatchKeyEvent {
            target_id: target_id.to_string(),
            event_type: "keyDown".to_string(),
            key: char_str.clone(),
            text: char_str.clone(),
        };
        if let Err(e) = backend.exec(down).await {
            return cdp_error_to_result(e);
        }

        let up = BackendOp::DispatchKeyEvent {
            target_id: target_id.to_string(),
            event_type: "keyUp".to_string(),
            key: char_str.clone(),
            text: char_str,
        };
        if let Err(e) = backend.exec(up).await {
            return cdp_error_to_result(e);
        }
    }

    ActionResult::ok(json!({"typed": text, "selector": selector}))
}

pub(super) async fn handle_fill(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    value: &str,
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
    let value_json = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_value", e.to_string(), "check value"),
    };

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return false;
el.focus();
el.value = {value_json};
el.dispatchEvent(new Event('input', {{ bubbles: true }}));
el.dispatchEvent(new Event('change', {{ bubbles: true }}));
return true;
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
            if val.as_bool() == Some(true) {
                ActionResult::ok(json!({"filled": selector, "value": value}))
            } else {
                element_not_found(selector)
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_select(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    value: &str,
    by_text: bool,
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
    let value_json = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_value", e.to_string(), "check value"),
    };

    let js = if by_text {
        format!(
            r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el || el.tagName !== 'SELECT') return null;
const text = {value_json};
for (const opt of el.options) {{
    if (opt.text.trim() === text || opt.textContent.trim() === text) {{
        el.value = opt.value;
        el.dispatchEvent(new Event('change', {{ bubbles: true }}));
        return opt.value;
    }}
}}
return null;
}})()"#
        )
    } else {
        format!(
            r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el || el.tagName !== 'SELECT') return null;
el.value = {value_json};
el.dispatchEvent(new Event('change', {{ bubbles: true }}));
return el.value;
}})()"#
        )
    };

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
                ActionResult::ok(json!({"selected": value, "selector": selector}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_hover(
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

    let (x, y) = match resolve_element_center(backend, target_id, selector).await {
        Ok(coords) => coords,
        Err(r) => return r,
    };

    let op = BackendOp::DispatchMouseEvent {
        target_id: target_id.to_string(),
        event_type: "mouseMoved".to_string(),
        x,
        y,
        button: "none".to_string(),
        click_count: 0,
    };

    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"hovered": selector, "x": x, "y": y})),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_focus(
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

    match focus_element(backend, target_id, selector).await {
        Ok(()) => ActionResult::ok(json!({"focused": selector})),
        Err(r) => r,
    }
}

pub(super) async fn handle_press(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    key_or_chord: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    // Parse chord: "Control+A" -> ["Control", "A"]
    let parts: Vec<&str> = key_or_chord.split('+').collect();

    let get_modifier_info = |key: &str| -> Option<(&str, i32)> {
        match key.to_lowercase().as_str() {
            "control" | "ctrl" => Some(("Control", 2)),
            "shift" => Some(("Shift", 8)),
            "alt" => Some(("Alt", 1)),
            "meta" | "command" | "cmd" => Some(("Meta", 4)),
            _ => None,
        }
    };

    // Press modifier keys down
    for part in &parts[..parts.len().saturating_sub(1)] {
        if let Some((key_value, _)) = get_modifier_info(part) {
            let op = BackendOp::DispatchKeyEvent {
                target_id: target_id.to_string(),
                event_type: "keyDown".to_string(),
                key: key_value.to_string(),
                text: String::new(),
            };
            if let Err(e) = backend.exec(op).await {
                return cdp_error_to_result(e);
            }
        }
    }

    // Press and release the main key
    let main_key = parts.last().unwrap_or(&key_or_chord);
    let (key_value, text) = map_key_name(main_key);

    let down = BackendOp::DispatchKeyEvent {
        target_id: target_id.to_string(),
        event_type: "keyDown".to_string(),
        key: key_value.to_string(),
        text: text.to_string(),
    };
    if let Err(e) = backend.exec(down).await {
        return cdp_error_to_result(e);
    }

    let up = BackendOp::DispatchKeyEvent {
        target_id: target_id.to_string(),
        event_type: "keyUp".to_string(),
        key: key_value.to_string(),
        text: String::new(),
    };
    if let Err(e) = backend.exec(up).await {
        return cdp_error_to_result(e);
    }

    // Release modifier keys (reverse order)
    for part in parts[..parts.len().saturating_sub(1)].iter().rev() {
        if let Some((key_value, _)) = get_modifier_info(part) {
            let op = BackendOp::DispatchKeyEvent {
                target_id: target_id.to_string(),
                event_type: "keyUp".to_string(),
                key: key_value.to_string(),
                text: String::new(),
            };
            if let Err(e) = backend.exec(op).await {
                return cdp_error_to_result(e);
            }
        }
    }

    ActionResult::ok(json!({"pressed": key_or_chord}))
}

pub(super) async fn handle_drag(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    from_selector: &str,
    to_selector: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let (from_x, from_y) = match resolve_element_center(backend, target_id, from_selector).await {
        Ok(coords) => coords,
        Err(r) => return r,
    };

    let (to_x, to_y) = match resolve_element_center(backend, target_id, to_selector).await {
        Ok(coords) => coords,
        Err(r) => return r,
    };

    // Move to source, press, move to target, release
    for (event_type, x, y, button, cc) in [
        ("mouseMoved", from_x, from_y, "left", 0),
        ("mousePressed", from_x, from_y, "left", 1),
        ("mouseMoved", to_x, to_y, "left", 0),
        ("mouseReleased", to_x, to_y, "left", 1),
    ] {
        let op = BackendOp::DispatchMouseEvent {
            target_id: target_id.to_string(),
            event_type: event_type.to_string(),
            x,
            y,
            button: button.to_string(),
            click_count: cc,
        };
        if let Err(e) = backend.exec(op).await {
            return cdp_error_to_result(e);
        }
    }

    ActionResult::ok(json!({
        "dragged": {"from": from_selector, "to": to_selector},
        "from": {"x": from_x, "y": from_y},
        "to": {"x": to_x, "y": to_y},
    }))
}

pub(super) async fn handle_upload(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    files: &[String],
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    // Get document root
    let doc_op = BackendOp::GetDocument {
        target_id: target_id.to_string(),
    };
    let doc_result = match backend.exec(doc_op).await {
        Ok(r) => r,
        Err(e) => return cdp_error_to_result(e),
    };
    let root_node_id = doc_result
        .value
        .get("root")
        .and_then(|r| r.get("nodeId"))
        .and_then(|n| n.as_i64())
        .unwrap_or(1);

    // Query selector to get the file input node
    let qs_op = BackendOp::QuerySelector {
        target_id: target_id.to_string(),
        node_id: root_node_id,
        selector: selector.to_string(),
    };
    let qs_result = match backend.exec(qs_op).await {
        Ok(r) => r,
        Err(e) => return cdp_error_to_result(e),
    };
    let node_id = qs_result
        .value
        .get("nodeId")
        .and_then(|n| n.as_i64())
        .unwrap_or(0);
    if node_id == 0 {
        return element_not_found(selector);
    }

    // Set files on the input
    let upload_op = BackendOp::SetFileInputFiles {
        target_id: target_id.to_string(),
        node_id,
        files: files.to_vec(),
    };

    match backend.exec(upload_op).await {
        Ok(_) => ActionResult::ok(json!({"uploaded": files.len(), "selector": selector})),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_scroll(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    direction: &str,
    amount: Option<i32>,
    selector: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let dir = direction.to_lowercase();

    // Handle special directions: top, bottom, into-view
    match dir.as_str() {
        "top" | "bottom" => {
            let scroll_y = if dir == "top" {
                "0"
            } else {
                "document.body.scrollHeight"
            };
            let js = format!("(function() {{ window.scrollTo(0, {scroll_y}); return true; }})()");
            let op = BackendOp::Evaluate {
                target_id: target_id.to_string(),
                expression: js,
                return_by_value: true,
            };
            return match backend.exec(op).await {
                Ok(_) => ActionResult::ok(json!({"scrolled": direction})),
                Err(e) => cdp_error_to_result(e),
            };
        }
        "into-view" => {
            let sel = match selector {
                Some(s) => s,
                None => {
                    return ActionResult::fatal(
                        "missing_selector",
                        "into-view requires a selector",
                        "provide a CSS selector",
                    )
                }
            };
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
            let js = format!(
                r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({sel_json});
if (!el) return false;
el.scrollIntoView({{ behavior: 'instant', block: 'center' }});
return true;
}})()"#
            );
            let op = BackendOp::Evaluate {
                target_id: target_id.to_string(),
                expression: js,
                return_by_value: true,
            };
            return match backend.exec(op).await {
                Ok(result) => {
                    let val = extract_eval_value(&result.value);
                    if val.as_bool() == Some(true) {
                        ActionResult::ok(json!({"scrolled": "into-view", "selector": sel}))
                    } else {
                        element_not_found(sel)
                    }
                }
                Err(e) => cdp_error_to_result(e),
            };
        }
        _ => {}
    }

    let px = amount.unwrap_or(300);
    let (dx, dy) = match dir.as_str() {
        "up" => (0, -px),
        "down" => (0, px),
        "left" => (-px, 0),
        "right" => (px, 0),
        _ => {
            return ActionResult::fatal(
                "invalid_direction",
                format!("unknown scroll direction '{direction}'"),
                "use: up, down, left, right, top, bottom, into-view",
            )
        }
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
if (!el) return false;
el.scrollBy({dx}, {dy});
return true;
}})()"#
            )
        }
        None => format!("(function() {{ window.scrollBy({dx}, {dy}); return true; }})()"),
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            if val.as_bool() == Some(true) {
                ActionResult::ok(json!({"scrolled": direction, "amount": px}))
            } else if let Some(sel) = selector {
                element_not_found(sel)
            } else {
                ActionResult::ok(json!({"scrolled": direction, "amount": px}))
            }
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_mouse_move(
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

    let op = BackendOp::DispatchMouseEvent {
        target_id: target_id.to_string(),
        event_type: "mouseMoved".to_string(),
        x,
        y,
        button: "none".to_string(),
        click_count: 0,
    };

    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"moved": {"x": x, "y": y}})),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_cursor_position(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    // There is no direct CDP method to get cursor position, so we use JS.
    let js = r#"(function() {
        let x = 0, y = 0;
        document.addEventListener('mousemove', function handler(e) {
            x = e.clientX; y = e.clientY;
            document.removeEventListener('mousemove', handler);
        }, { once: true });
        return { x: window.__abCursorX || 0, y: window.__abCursorY || 0 };
    })()"#;

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js.to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"cursor": val}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}
