//! Action handler — compiles high-level Actions into BackendOp sequences.
//!
//! Each handler method takes an `&mut dyn BackendSession`, the session's
//! tab/window registries, and the Action-specific parameters. It returns an
//! [`ActionResult`].
//!
//! The session actor calls [`handle_action`] which dispatches to the correct
//! handler based on the Action variant.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::action::Action;
use super::action_result::ActionResult;
use super::backend::BackendSession;
use super::backend_op::BackendOp;
use super::types::{QueryMode, SameSite, SessionId, StorageKind, TabId, WindowId};
use crate::error::ActionbookError;

// ---------------------------------------------------------------------------
// Tab / Window entries (owned by the session actor)
// ---------------------------------------------------------------------------

/// A tab tracked by the session actor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabEntry {
    /// Short alias (t0, t1, ...).
    pub id: TabId,
    /// CDP target ID.
    pub target_id: String,
    /// Owning window.
    pub window: WindowId,
    /// Last known URL.
    pub url: String,
    /// Last known title.
    pub title: String,
}

/// A window tracked by the session actor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowEntry {
    /// Short alias (w0, w1, ...).
    pub id: WindowId,
    /// Tabs in this window.
    pub tabs: Vec<TabId>,
}

/// Mutable registries passed into action handlers.
pub struct Registries {
    /// Open tabs keyed by tab ID.
    pub tabs: std::collections::HashMap<TabId, TabEntry>,
    /// Open windows keyed by window ID.
    pub windows: std::collections::HashMap<WindowId, WindowEntry>,
    /// Counter for allocating the next tab ID.
    pub next_tab_id: u32,
    /// Counter for allocating the next window ID.
    pub next_window_id: u32,
}

impl Registries {
    /// Create empty registries.
    pub fn new() -> Self {
        Self {
            tabs: std::collections::HashMap::new(),
            windows: std::collections::HashMap::new(),
            next_tab_id: 0,
            next_window_id: 0,
        }
    }

    /// Allocate the next [`TabId`].
    pub fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    /// Allocate the next [`WindowId`].
    pub fn alloc_window_id(&mut self) -> WindowId {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;
        id
    }

    fn find_tab(&self, tab: TabId) -> Option<&TabEntry> {
        self.tabs.get(&tab)
    }
}

impl Default for Registries {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Main dispatch
// ---------------------------------------------------------------------------

/// Dispatch an Action to the appropriate handler, returning an ActionResult.
///
/// `session_id` is the owning session's ID (used in error hints).
/// `backend` is the live BackendSession.
/// `regs` are the tab/window registries.
pub async fn handle_action(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    action: Action,
) -> ActionResult {
    match action {
        // -- Tab-level commands --
        Action::Goto { tab, url, .. } => handle_goto(session_id, backend, regs, tab, &url).await,
        Action::Back { tab, .. } => handle_history(backend, regs, session_id, tab, "back").await,
        Action::Forward { tab, .. } => {
            handle_history(backend, regs, session_id, tab, "forward").await
        }
        Action::Reload { tab, .. } => handle_reload(session_id, backend, regs, tab).await,
        Action::Open { url, .. } => {
            handle_new_tab(session_id, backend, regs, &url, false, None).await
        }
        Action::Snapshot { tab, .. } => handle_snapshot(session_id, backend, regs, tab).await,
        Action::Screenshot { tab, full_page, .. } => {
            handle_screenshot(session_id, backend, regs, tab, full_page).await
        }
        Action::Click {
            tab,
            selector,
            button,
            count,
            ..
        } => {
            handle_click(
                session_id,
                backend,
                regs,
                tab,
                &selector,
                button.as_deref(),
                count,
            )
            .await
        }
        Action::Type {
            tab,
            selector,
            text,
            ..
        } => handle_type(session_id, backend, regs, tab, &selector, &text).await,
        Action::Fill {
            tab,
            selector,
            value,
            ..
        } => handle_fill(session_id, backend, regs, tab, &selector, &value).await,
        Action::Eval {
            tab, expression, ..
        } => handle_eval(session_id, backend, regs, tab, &expression).await,
        Action::WaitElement {
            tab,
            selector,
            timeout_ms,
            ..
        } => handle_wait_element(session_id, backend, regs, tab, &selector, timeout_ms).await,
        Action::Html { tab, selector, .. } => {
            handle_html(session_id, backend, regs, tab, selector.as_deref()).await
        }
        Action::Text { tab, selector, .. } => {
            handle_text(session_id, backend, regs, tab, selector.as_deref()).await
        }

        // -- Session-level commands --
        Action::ListTabs { .. } => handle_list_tabs(regs),
        Action::ListWindows { .. } => handle_list_windows(regs),
        Action::NewTab {
            url,
            new_window,
            window,
            ..
        } => handle_new_tab(session_id, backend, regs, &url, new_window, window).await,
        Action::CloseTab { tab, .. } => handle_close_tab(session_id, backend, regs, tab).await,
        Action::Close { .. } | Action::CloseSession { .. } => {
            // Handled at the session actor level, not here.
            ActionResult::ok(json!({"closed": true}))
        }

        // -- Observation commands (tab-level) --
        Action::Pdf { tab, path, .. } => handle_pdf(session_id, backend, regs, tab, &path).await,
        Action::Title { tab, .. } => handle_title(session_id, backend, regs, tab).await,
        Action::Url { tab, .. } => handle_url(session_id, backend, regs, tab).await,
        Action::Value { tab, selector, .. } => {
            handle_value(session_id, backend, regs, tab, &selector).await
        }
        Action::Attr {
            tab,
            selector,
            name,
            ..
        } => handle_attr(session_id, backend, regs, tab, &selector, &name).await,
        Action::Attrs { tab, selector, .. } => {
            handle_attrs(session_id, backend, regs, tab, &selector).await
        }
        Action::Describe { tab, selector, .. } => {
            handle_describe(session_id, backend, regs, tab, &selector).await
        }
        Action::State { tab, selector, .. } => {
            handle_state(session_id, backend, regs, tab, &selector).await
        }
        Action::Box_ { tab, selector, .. } => {
            handle_box(session_id, backend, regs, tab, &selector).await
        }
        Action::Styles { tab, selector, .. } => {
            handle_styles(session_id, backend, regs, tab, &selector).await
        }
        Action::Viewport { tab, .. } => handle_viewport(session_id, backend, regs, tab).await,
        Action::Query {
            tab,
            selector,
            mode,
            ..
        } => handle_query(session_id, backend, regs, tab, &selector, mode).await,
        Action::InspectPoint { tab, x, y, .. } => {
            handle_inspect_point(session_id, backend, regs, tab, x, y).await
        }
        Action::LogsConsole { tab, .. } => {
            handle_logs_console(session_id, backend, regs, tab).await
        }
        Action::LogsErrors { tab, .. } => handle_logs_errors(session_id, backend, regs, tab).await,

        // -- Data commands (session-level cookies) --
        Action::CookiesList { ref domain, .. } => {
            handle_cookies_list(session_id, backend, regs, domain.as_deref()).await
        }
        Action::CookiesGet { name, .. } => {
            handle_cookies_get(session_id, backend, regs, &name).await
        }
        Action::CookiesSet {
            name,
            value,
            domain,
            path,
            secure,
            http_only,
            same_site,
            expires,
            ..
        } => {
            handle_cookies_set(
                session_id,
                backend,
                regs,
                &name,
                &value,
                domain.as_deref(),
                path.as_deref(),
                secure,
                http_only,
                same_site,
                expires,
            )
            .await
        }
        Action::CookiesDelete { name, .. } => {
            handle_cookies_delete(session_id, backend, regs, &name).await
        }
        Action::CookiesClear { ref domain, .. } => {
            handle_cookies_clear(session_id, backend, regs, domain.as_deref()).await
        }

        // -- Data commands (tab-level storage) --
        Action::StorageList { tab, kind, .. } => {
            handle_storage_list(session_id, backend, regs, tab, kind).await
        }
        Action::StorageGet { tab, kind, key, .. } => {
            handle_storage_get(session_id, backend, regs, tab, kind, &key).await
        }
        Action::StorageSet {
            tab,
            kind,
            key,
            value,
            ..
        } => handle_storage_set(session_id, backend, regs, tab, kind, &key, &value).await,
        Action::StorageDelete { tab, kind, key, .. } => {
            handle_storage_delete(session_id, backend, regs, tab, kind, &key).await
        }
        Action::StorageClear { tab, kind, .. } => {
            handle_storage_clear(session_id, backend, regs, tab, kind).await
        }

        // -- Interaction commands --
        Action::Select {
            tab,
            selector,
            value,
            by_text,
            ..
        } => handle_select(session_id, backend, regs, tab, &selector, &value, by_text).await,
        Action::Hover { tab, selector, .. } => {
            handle_hover(session_id, backend, regs, tab, &selector).await
        }
        Action::Focus { tab, selector, .. } => {
            handle_focus(session_id, backend, regs, tab, &selector).await
        }
        Action::Press {
            tab, key_or_chord, ..
        } => handle_press(session_id, backend, regs, tab, &key_or_chord).await,
        Action::Drag {
            tab,
            from_selector,
            to_selector,
            ..
        } => handle_drag(session_id, backend, regs, tab, &from_selector, &to_selector).await,
        Action::Upload {
            tab,
            selector,
            files,
            ..
        } => handle_upload(session_id, backend, regs, tab, &selector, &files).await,
        Action::Scroll {
            tab,
            direction,
            amount,
            selector,
            ..
        } => {
            handle_scroll(
                session_id,
                backend,
                regs,
                tab,
                &direction,
                amount,
                selector.as_deref(),
            )
            .await
        }
        Action::MouseMove { tab, x, y, .. } => {
            handle_mouse_move(session_id, backend, regs, tab, x, y).await
        }
        Action::CursorPosition { tab, .. } => {
            handle_cursor_position(session_id, backend, regs, tab).await
        }

        // -- Waiting commands --
        Action::WaitNavigation {
            tab, timeout_ms, ..
        } => handle_wait_navigation(session_id, backend, regs, tab, timeout_ms).await,
        Action::WaitNetworkIdle {
            tab,
            timeout_ms,
            idle_time_ms,
            ..
        } => {
            handle_wait_network_idle(session_id, backend, regs, tab, timeout_ms, idle_time_ms).await
        }
        Action::WaitCondition {
            tab,
            expression,
            timeout_ms,
            ..
        } => handle_wait_condition(session_id, backend, regs, tab, &expression, timeout_ms).await,

        // -- Session management --
        Action::RestartSession { .. } => {
            // RestartSession is handled at the session actor level (like Close).
            ActionResult::ok(json!({"restarting": true}))
        }

        // -- Global commands (should not reach the action handler) --
        Action::StartSession { .. } | Action::ListSessions | Action::SessionStatus { .. } => {
            ActionResult::fatal(
                "invalid_dispatch",
                "global action dispatched to session handler",
                "this is a bug — global actions should be handled by the router",
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Tab-level handlers
// ---------------------------------------------------------------------------

async fn handle_goto(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    url: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::Navigate {
        target_id: target_id.to_string(),
        url: url.to_string(),
    };

    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"navigated": url})),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_history(
    backend: &mut dyn BackendSession,
    regs: &Registries,
    session_id: SessionId,
    tab: TabId,
    direction: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: format!("history.{direction}()"),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"navigated": direction})),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_reload(
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
        expression: "location.reload()".to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"reloaded": true})),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_snapshot(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let op = BackendOp::GetAccessibilityTree {
        target_id: target_id.to_string(),
    };

    match backend.exec(op).await {
        Ok(result) => ActionResult::ok(result.value),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_screenshot(
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

async fn handle_click(
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

async fn handle_type(
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

async fn handle_fill(
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

async fn handle_eval(
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

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: expression.to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(val)
        }
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_wait_element(
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

async fn handle_html(
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

async fn handle_text(
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

// ---------------------------------------------------------------------------
// Observation handlers (tab-level)
// ---------------------------------------------------------------------------

async fn handle_pdf(
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

async fn handle_title(
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

async fn handle_url(
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

async fn handle_value(
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

async fn handle_attr(
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

async fn handle_attrs(
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

async fn handle_describe(
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

async fn handle_state(
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

async fn handle_box(
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

async fn handle_styles(
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
const cs = window.getComputedStyle(el);
const props = ['display','visibility','opacity','color','backgroundColor','fontSize','fontWeight','fontFamily','margin','padding','border','position','zIndex','overflow','cursor','width','height'];
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

async fn handle_viewport(
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

async fn handle_query(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    selector: &str,
    mode: QueryMode,
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

    let js = match mode {
        QueryMode::Css => format!(
            r#"(function() {{ const els = document.querySelectorAll({selector_json}); return Array.from(els).slice(0, 100).map((el, i) => {{ const rect = el.getBoundingClientRect(); return {{ index: i, tag: el.tagName.toLowerCase(), id: el.id || '', text: (el.innerText || '').substring(0, 80), x: rect.left, y: rect.top, width: rect.width, height: rect.height }}; }}); }})()"#
        ),
        QueryMode::Xpath => format!(
            r#"(function() {{ const result = document.evaluate({selector_json}, document, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null); const items = []; for (let i = 0; i < Math.min(result.snapshotLength, 100); i++) {{ const el = result.snapshotItem(i); if (el.nodeType === 1) {{ const rect = el.getBoundingClientRect(); items.push({{ index: i, tag: el.tagName.toLowerCase(), id: el.id || '', text: (el.innerText || '').substring(0, 80), x: rect.left, y: rect.top, width: rect.width, height: rect.height }}); }} }} return items; }})()"#
        ),
        QueryMode::Text => format!(
            r#"(function() {{ const text = {selector_json}; const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null); const results = []; while (walker.nextNode()) {{ if (walker.currentNode.textContent.includes(text) && results.length < 100) {{ const el = walker.currentNode.parentElement; if (el) {{ const rect = el.getBoundingClientRect(); results.push({{ index: results.length, tag: el.tagName.toLowerCase(), id: el.id || '', text: (el.innerText || '').substring(0, 80), x: rect.left, y: rect.top, width: rect.width, height: rect.height }}); }} }} }} return results; }})()"#
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
            ActionResult::ok(
                json!({"results": val, "selector": selector, "mode": mode.to_string()}),
            )
        }
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_inspect_point(
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

async fn handle_logs_console(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let js = r#"(function() { if (!window.__ab_console_logs) { window.__ab_console_logs = []; const orig = { log: console.log, warn: console.warn, info: console.info, debug: console.debug, error: console.error }; for (const [level, fn] of Object.entries(orig)) { console[level] = function(...args) { window.__ab_console_logs.push({ level, message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '), timestamp: Date.now() }); fn.apply(console, args); }; } } const logs = window.__ab_console_logs.slice(-200); window.__ab_console_logs = []; return logs; })()"#;

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js.to_string(),
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

async fn handle_logs_errors(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let js = r#"(function() { if (!window.__ab_error_logs) { window.__ab_error_logs = []; const origError = console.error; console.error = function(...args) { window.__ab_error_logs.push({ message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '), timestamp: Date.now() }); origError.apply(console, args); }; window.addEventListener('error', function(e) { window.__ab_error_logs.push({ message: e.message, source: e.filename, line: e.lineno, col: e.colno, timestamp: Date.now() }); }); window.addEventListener('unhandledrejection', function(e) { window.__ab_error_logs.push({ message: 'Unhandled rejection: ' + String(e.reason), timestamp: Date.now() }); }); } const errors = window.__ab_error_logs.slice(-200); window.__ab_error_logs = []; return errors; })()"#;

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js.to_string(),
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

// ---------------------------------------------------------------------------
// Data handlers — Cookies (session-level, use first tab as proxy target)
// ---------------------------------------------------------------------------

fn resolve_any_tab(session_id: SessionId, regs: &Registries) -> Result<&str, ActionResult> {
    regs.tabs
        .values()
        .next()
        .map(|t| t.target_id.as_str())
        .ok_or_else(|| {
            ActionResult::fatal(
                "no_tabs",
                format!("session {session_id} has no open tabs for cookie operations"),
                format!("open a tab first with `actionbook browser open -s {session_id} <url>`"),
            )
        })
}

async fn handle_cookies_list(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    domain: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let op = BackendOp::GetCookies {
        target_id: target_id.to_string(),
    };
    match backend.exec(op).await {
        Ok(result) => {
            let cookies = result
                .value
                .get("cookies")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let items = filter_cookies_by_domain(cookies, domain);
            ActionResult::ok(json!({"items": items}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_cookies_get(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    name: &str,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let op = BackendOp::GetCookies {
        target_id: target_id.to_string(),
    };
    match backend.exec(op).await {
        Ok(result) => {
            let cookies = result
                .value
                .get("cookies")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let found: Vec<_> = cookies
                .into_iter()
                .filter(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
                .collect();
            let item = found.into_iter().next().unwrap_or(json!(null));
            ActionResult::ok(json!({"item": item}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_cookies_set(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    name: &str,
    value: &str,
    domain: Option<&str>,
    path: Option<&str>,
    secure: Option<bool>,
    http_only: Option<bool>,
    same_site: Option<SameSite>,
    expires: Option<f64>,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let op = BackendOp::SetCookie {
        target_id: target_id.to_string(),
        name: name.to_string(),
        value: value.to_string(),
        domain: domain.unwrap_or("").to_string(),
        path: path.unwrap_or("/").to_string(),
        secure,
        http_only,
        same_site: same_site.map(|s| s.to_string()),
        expires,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({
            "action": "set",
            "affected": 1,
            "domain": domain
        })),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_cookies_delete(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    name: &str,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let op = BackendOp::DeleteCookies {
        target_id: target_id.to_string(),
        name: name.to_string(),
        domain: None,
        path: None,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({
            "action": "delete",
            "affected": 1
        })),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_cookies_clear(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    domain: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let get_op = BackendOp::GetCookies {
        target_id: target_id.to_string(),
    };
    let cookies = match backend.exec(get_op).await {
        Ok(result) => result
            .value
            .get("cookies")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        Err(e) => return cdp_error_to_result(e),
    };
    let cookies = filter_cookies_by_domain(cookies, domain);
    let mut deleted = 0;
    for cookie in &cookies {
        let cname = cookie.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let cdomain = cookie.get("domain").and_then(|d| d.as_str());
        let cpath = cookie.get("path").and_then(|p| p.as_str());
        let op = BackendOp::DeleteCookies {
            target_id: target_id.to_string(),
            name: cname.to_string(),
            domain: cdomain.map(|s| s.to_string()),
            path: cpath.map(|s| s.to_string()),
        };
        if let Err(e) = backend.exec(op).await {
            return cdp_error_to_result(e);
        }
        deleted += 1;
    }
    ActionResult::ok(json!({
        "action": "clear",
        "affected": deleted,
        "domain": domain
    }))
}

fn normalize_cookie_domain(domain: &str) -> String {
    domain.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn cookie_matches_domain(cookie: &Value, domain: Option<&str>) -> bool {
    let Some(domain) = domain else {
        return true;
    };
    let Some(cookie_domain) = cookie.get("domain").and_then(|v| v.as_str()) else {
        return false;
    };
    normalize_cookie_domain(cookie_domain) == normalize_cookie_domain(domain)
}

fn filter_cookies_by_domain(cookies: Vec<Value>, domain: Option<&str>) -> Vec<Value> {
    cookies
        .into_iter()
        .filter(|cookie| cookie_matches_domain(cookie, domain))
        .collect()
}

// ---------------------------------------------------------------------------
// Data handlers — Storage (tab-level)
// ---------------------------------------------------------------------------

fn storage_js_name(kind: StorageKind) -> &'static str {
    match kind {
        StorageKind::Local => "localStorage",
        StorageKind::Session => "sessionStorage",
    }
}

async fn handle_storage_list(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let js = format!(
        r#"(function() {{ const keys = []; for (let i = 0; i < {store}.length; i++) {{ keys.push({store}.key(i)); }} return keys; }})()"#
    );
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"keys": val, "kind": kind.to_string()}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_storage_get(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
    key: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let key_json = match serde_json::to_string(key) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_key", e.to_string(), "check key"),
    };
    let js = format!("{store}.getItem({key_json})");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"key": key, "value": val, "kind": kind.to_string()}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_storage_set(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
    key: &str,
    value: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let key_json = match serde_json::to_string(key) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_key", e.to_string(), "check key"),
    };
    let value_json = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_value", e.to_string(), "check value"),
    };
    let js = format!("{store}.setItem({key_json}, {value_json})");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"set": key, "kind": kind.to_string()})),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_storage_delete(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
    key: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let key_json = match serde_json::to_string(key) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_key", e.to_string(), "check key"),
    };
    let js = format!("{store}.removeItem({key_json})");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"deleted": key, "kind": kind.to_string()})),
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_storage_clear(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let js = format!("{store}.clear()");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"cleared": kind.to_string()})),
        Err(e) => cdp_error_to_result(e),
    }
}

// ---------------------------------------------------------------------------
// Session-level handlers
// ---------------------------------------------------------------------------

fn handle_list_tabs(regs: &Registries) -> ActionResult {
    let mut tabs: Vec<serde_json::Value> = regs
        .tabs
        .values()
        .map(|t| {
            json!({
                "id": t.id.to_string(),
                "target_id": t.target_id,
                "window": t.window.to_string(),
                "url": t.url,
                "title": t.title,
            })
        })
        .collect();
    tabs.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    ActionResult::ok(json!({"tabs": tabs}))
}

fn handle_list_windows(regs: &Registries) -> ActionResult {
    let mut windows: Vec<serde_json::Value> = regs
        .windows
        .values()
        .map(|w| {
            json!({
                "id": w.id.to_string(),
                "tabs": w.tabs.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            })
        })
        .collect();
    windows.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    ActionResult::ok(json!({"windows": windows}))
}

async fn handle_new_tab(
    _session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    url: &str,
    new_window: bool,
    window: Option<WindowId>,
) -> ActionResult {
    let op = BackendOp::CreateTarget {
        url: url.to_string(),
        window_id: None,
        new_window,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let target_id = result
                .value
                .get("targetId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if target_id.is_empty() {
                return ActionResult::fatal(
                    "create_target_failed",
                    "Target.createTarget did not return a targetId",
                    "check browser logs",
                );
            }

            let win_id = if new_window {
                let wid = regs.alloc_window_id();
                regs.windows.insert(
                    wid,
                    WindowEntry {
                        id: wid,
                        tabs: Vec::new(),
                    },
                );
                wid
            } else if let Some(w) = window {
                regs.windows.entry(w).or_insert_with(|| WindowEntry {
                    id: w,
                    tabs: Vec::new(),
                });
                w
            } else {
                regs.windows
                    .keys()
                    .min_by_key(|w| w.0)
                    .copied()
                    .unwrap_or_else(|| {
                        let wid = regs.alloc_window_id();
                        regs.windows.insert(
                            wid,
                            WindowEntry {
                                id: wid,
                                tabs: Vec::new(),
                            },
                        );
                        wid
                    })
            };

            let tab_id = regs.alloc_tab_id();
            regs.tabs.insert(
                tab_id,
                TabEntry {
                    id: tab_id,
                    target_id: target_id.clone(),
                    window: win_id,
                    url: url.to_string(),
                    title: String::new(),
                },
            );
            if let Some(win) = regs.windows.get_mut(&win_id) {
                win.tabs.push(tab_id);
            }

            ActionResult::ok(json!({
                "tab": tab_id.to_string(),
                "target_id": target_id,
                "window": win_id.to_string(),
            }))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

async fn handle_close_tab(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t.to_string(),
        Err(r) => return r,
    };

    let op = BackendOp::CloseTarget { target_id };

    match backend.exec(op).await {
        Ok(_) => {
            if let Some(entry) = regs.tabs.remove(&tab) {
                if let Some(win) = regs.windows.get_mut(&entry.window) {
                    win.tabs.retain(|t| *t != tab);
                }
            }
            ActionResult::ok(json!({"closed_tab": tab.to_string()}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

// ---------------------------------------------------------------------------
// Interaction handlers
// ---------------------------------------------------------------------------

async fn handle_select(
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

async fn handle_hover(
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

async fn handle_focus(
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

async fn handle_press(
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

async fn handle_drag(
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

async fn handle_upload(
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

async fn handle_scroll(
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

    let px = amount.unwrap_or(300);
    let (dx, dy) = match direction.to_lowercase().as_str() {
        "up" => (0, -px),
        "down" => (0, px),
        "left" => (-px, 0),
        "right" => (px, 0),
        _ => {
            return ActionResult::fatal(
                "invalid_direction",
                format!("unknown scroll direction '{direction}'"),
                "use: up, down, left, right",
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

async fn handle_mouse_move(
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

async fn handle_cursor_position(
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

// ---------------------------------------------------------------------------
// Waiting handlers
// ---------------------------------------------------------------------------

async fn handle_wait_navigation(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    timeout_ms: Option<u64>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
    let poll_interval = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + timeout;

    // Get the current URL first
    let get_url_js = "window.location.href".to_string();
    let initial_url = {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: get_url_js.clone(),
            return_by_value: true,
        };
        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                val.as_str().unwrap_or("").to_string()
            }
            Err(e) => return cdp_error_to_result(e),
        }
    };

    // Poll until URL changes or document.readyState is complete
    loop {
        let check_js = r#"(function() {
                const url = window.location.href;
                const ready = document.readyState;
                return { url: url, ready: ready };
            })()"#
            .to_string();
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: check_js,
            return_by_value: true,
        };

        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                let current_url = val.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let ready = val.get("ready").and_then(|v| v.as_str()).unwrap_or("");
                if current_url != initial_url || ready == "complete" {
                    return ActionResult::ok(json!({
                        "navigated": true,
                        "url": current_url,
                        "readyState": ready,
                    }));
                }
            }
            Err(e) => return cdp_error_to_result(e),
        }

        if tokio::time::Instant::now() >= deadline {
            return ActionResult::retryable(
                "navigation_timeout",
                format!(
                    "navigation did not complete within {}ms",
                    timeout.as_millis()
                ),
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

async fn handle_wait_network_idle(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    timeout_ms: Option<u64>,
    idle_time_ms: Option<u64>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
    let idle_time = idle_time_ms.unwrap_or(500);
    let poll_interval = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + timeout;

    // Use JS Performance API to detect ongoing requests
    let check_js = format!(
        r#"(function() {{
            const entries = performance.getEntriesByType('resource');
            const now = performance.now();
            const recent = entries.filter(e => now - e.responseEnd < {idle_time});
            return {{ pending: recent.length, now: now }};
        }})()"#
    );

    loop {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: check_js.clone(),
            return_by_value: true,
        };

        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                let pending = val.get("pending").and_then(|v| v.as_i64()).unwrap_or(1);
                if pending == 0 {
                    return ActionResult::ok(json!({"network_idle": true}));
                }
            }
            Err(e) => return cdp_error_to_result(e),
        }

        if tokio::time::Instant::now() >= deadline {
            return ActionResult::retryable(
                "network_idle_timeout",
                format!(
                    "network did not become idle within {}ms",
                    timeout.as_millis()
                ),
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

async fn handle_wait_condition(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    expression: &str,
    timeout_ms: Option<u64>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
    let poll_interval = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: expression.to_string(),
            return_by_value: true,
        };

        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                // Check for truthiness
                let truthy = match &val {
                    serde_json::Value::Bool(b) => *b,
                    serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                    serde_json::Value::String(s) => !s.is_empty(),
                    serde_json::Value::Null => false,
                    _ => true, // objects and arrays are truthy
                };
                if truthy {
                    return ActionResult::ok(json!({"condition_met": true, "value": val}));
                }
            }
            Err(e) => return cdp_error_to_result(e),
        }

        if tokio::time::Instant::now() >= deadline {
            return ActionResult::retryable(
                "condition_timeout",
                format!(
                    "condition '{}' not met within {}ms",
                    expression,
                    timeout.as_millis()
                ),
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve an element's center coordinates using JS.
async fn resolve_element_center(
    backend: &mut dyn BackendSession,
    target_id: &str,
    selector: &str,
) -> Result<(f64, f64), ActionResult> {
    let selector_json = serde_json::to_string(selector).map_err(|e| {
        ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
    })?;

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return null;
el.scrollIntoView({{ behavior: 'instant', block: 'center', inline: 'center' }});
const rect = el.getBoundingClientRect();
return {{ x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }};
}})()"#
    );

    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };

    let coords = match backend.exec(op).await {
        Ok(r) => extract_eval_value(&r.value),
        Err(e) => return Err(cdp_error_to_result(e)),
    };

    if coords.is_null() {
        return Err(element_not_found(selector));
    }

    let x = coords.get("x").and_then(|v| v.as_f64()).ok_or_else(|| {
        ActionResult::fatal(
            "invalid_coordinates",
            "element returned no x coordinate",
            "check selector",
        )
    })?;
    let y = coords.get("y").and_then(|v| v.as_f64()).ok_or_else(|| {
        ActionResult::fatal(
            "invalid_coordinates",
            "element returned no y coordinate",
            "check selector",
        )
    })?;

    Ok((x, y))
}

/// Map common key names to CDP key values and text.
fn map_key_name(key: &str) -> (&str, &str) {
    match key.to_lowercase().as_str() {
        "enter" | "return" => ("Enter", "\r"),
        "tab" => ("Tab", "\t"),
        "escape" | "esc" => ("Escape", ""),
        "backspace" => ("Backspace", ""),
        "delete" => ("Delete", ""),
        "arrowup" | "up" => ("ArrowUp", ""),
        "arrowdown" | "down" => ("ArrowDown", ""),
        "arrowleft" | "left" => ("ArrowLeft", ""),
        "arrowright" | "right" => ("ArrowRight", ""),
        "home" => ("Home", ""),
        "end" => ("End", ""),
        "pageup" => ("PageUp", ""),
        "pagedown" => ("PageDown", ""),
        "space" => (" ", " "),
        _ => (key, key),
    }
}

/// Look up a tab's CDP target_id, or return a Fatal ActionResult.
fn resolve_tab(session_id: SessionId, regs: &Registries, tab: TabId) -> Result<&str, ActionResult> {
    match regs.find_tab(tab) {
        Some(entry) => Ok(&entry.target_id),
        None => Err(ActionResult::fatal(
            "tab_not_found",
            format!("tab {tab} does not exist in session {session_id}"),
            format!("run `actionbook browser list-tabs -s {session_id}`"),
        )),
    }
}

/// Focus an element by selector using JS, returning an error ActionResult on failure.
async fn focus_element(
    backend: &mut dyn BackendSession,
    target_id: &str,
    selector: &str,
) -> Result<(), ActionResult> {
    let selector_json = serde_json::to_string(selector).map_err(|e| {
        ActionResult::fatal("invalid_selector", e.to_string(), "check selector syntax")
    })?;

    let js = format!(
        r#"(function() {{
{FIND_ELEMENT_JS}
const el = __findElement({selector_json});
if (!el) return false;
el.focus();
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
                Ok(())
            } else {
                Err(element_not_found(selector))
            }
        }
        Err(e) => Err(cdp_error_to_result(e)),
    }
}

/// Extract the actual return value from a Runtime.evaluate result.
///
/// CDP wraps the value as `{ "result": { "type": "...", "value": <actual> } }`.
fn extract_eval_value(cdp_result: &serde_json::Value) -> serde_json::Value {
    cdp_result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or_else(|| cdp_result.clone())
}

fn element_not_found(selector: &str) -> ActionResult {
    ActionResult::fatal(
        "element_not_found",
        format!("element '{}' not found", selector),
        "check selector or use `actionbook browser snapshot` to see available elements",
    )
}

fn cdp_error_to_result(err: ActionbookError) -> ActionResult {
    match &err {
        ActionbookError::CdpConnectionFailed(_) => ActionResult::retryable(
            "backend_disconnected",
            "session may be recovering, retry in a moment",
        ),
        ActionbookError::CdpError(msg) => ActionResult::fatal(
            "cdp_error",
            msg.clone(),
            "check the CDP command and parameters",
        ),
        _ => ActionResult::fatal("backend_error", err.to_string(), "check browser logs"),
    }
}

// ---------------------------------------------------------------------------
// find_element_js — injected into Evaluate expressions
// ---------------------------------------------------------------------------

/// Minimal __findElement JS function that supports CSS, XPath, @eN refs.
///
/// Streamlined from session.rs — keeps element resolution logic only.
const FIND_ELEMENT_JS: &str = r#"
function __findElement(selector) {
    const refMatch = selector.match(/^\[ref=(e\d+)\]$/);
    if (refMatch) selector = '@' + refMatch[1];
    if (/^@e\d+$/.test(selector)) {
        const targetNum = parseInt(selector.slice(2));
        const SKIP_TAGS = new Set(['script','style','noscript','template','svg','path','defs','clippath','lineargradient','stop','meta','link','br','wbr']);
        const INLINE_TAGS = new Set(['strong','b','em','i','code','span','small','sup','sub','abbr','mark','u','s','del','ins','time','q','cite','dfn','var','samp','kbd']);
        const INTERACTIVE_ROLES = new Set(['button','link','textbox','checkbox','radio','combobox','listbox','menuitem','menuitemcheckbox','menuitemradio','option','searchbox','slider','spinbutton','switch','tab','treeitem']);
        const CONTENT_ROLES = new Set(['heading','cell','gridcell','columnheader','rowheader','listitem','article','region','main','navigation','img']);
        function getRole(el) {
            const explicit = el.getAttribute('role');
            if (explicit) return explicit.toLowerCase();
            const tag = el.tagName.toLowerCase();
            if (INLINE_TAGS.has(tag)) return tag;
            const roleMap = { 'a': el.hasAttribute('href') ? 'link' : 'generic', 'button': 'button', 'input': getInputRole(el), 'select': 'combobox', 'textarea': 'textbox', 'img': 'img', 'h1':'heading','h2':'heading','h3':'heading','h4':'heading','h5':'heading','h6':'heading', 'nav':'navigation','main':'main','header':'banner','footer':'contentinfo','aside':'complementary', 'form':'form','table':'table','ul':'list','ol':'list','li':'listitem', 'details':'group','summary':'button','dialog':'dialog', 'section': el.hasAttribute('aria-label') || el.hasAttribute('aria-labelledby') ? 'region' : 'generic', 'article':'article' };
            return roleMap[tag] || 'generic';
        }
        function getInputRole(el) {
            const type = (el.getAttribute('type') || 'text').toLowerCase();
            const map = {'text':'textbox','email':'textbox','password':'textbox','search':'searchbox','tel':'textbox','url':'textbox','number':'spinbutton','checkbox':'checkbox','radio':'radio','submit':'button','reset':'button','button':'button','range':'slider'};
            return map[type] || 'textbox';
        }
        function getAccessibleName(el) {
            const ariaLabel = el.getAttribute('aria-label');
            if (ariaLabel) return ariaLabel.trim();
            const tag = el.tagName.toLowerCase();
            if (tag === 'img') return el.getAttribute('alt') || '';
            if (tag === 'input' || tag === 'textarea' || tag === 'select') {
                if (el.id) { const label = document.querySelector('label[for="' + el.id + '"]'); if (label) return label.textContent?.trim()?.substring(0, 100) || ''; }
                return el.getAttribute('placeholder') || el.getAttribute('title') || '';
            }
            return '';
        }
        function isHidden(el) {
            if (el.hidden || el.getAttribute('aria-hidden') === 'true') return true;
            const style = el.style;
            return style.display === 'none' || style.visibility === 'hidden';
        }
        let refCounter = 0;
        function walkFind(el, depth) {
            if (depth > 15) return null;
            const tag = el.tagName.toLowerCase();
            if (SKIP_TAGS.has(tag) || isHidden(el)) return null;
            const role = getRole(el);
            const name = getAccessibleName(el);
            if (INTERACTIVE_ROLES.has(role) || (CONTENT_ROLES.has(role) && name)) {
                refCounter++;
                if (refCounter === targetNum) return el;
            }
            for (const child of el.children) {
                const found = walkFind(child, depth + 1);
                if (found) return found;
            }
            return null;
        }
        return walkFind(document.body, 0);
    }
    if (selector.startsWith('//') || selector.startsWith('(//')) {
        const result = document.evaluate(selector, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null);
        return result.singleNodeValue;
    }
    return document.querySelector(selector);
}
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::backend::{
        BackendEvent, BackendKind, Checkpoint, Health, OpResult, ShutdownPolicy, TargetInfo,
    };
    use async_trait::async_trait;
    use futures::stream::BoxStream;
    use futures::StreamExt;

    // -----------------------------------------------------------------------
    // MockBackendSession
    // -----------------------------------------------------------------------

    struct MockBackendSession {
        ops: Vec<BackendOp>,
        responses: std::collections::VecDeque<Result<OpResult, ActionbookError>>,
    }

    impl MockBackendSession {
        fn new(responses: Vec<Result<OpResult, ActionbookError>>) -> Self {
            Self {
                ops: Vec::new(),
                responses: responses.into(),
            }
        }

        fn ops(&self) -> &[BackendOp] {
            &self.ops
        }
    }

    #[async_trait]
    impl BackendSession for MockBackendSession {
        fn events(&mut self) -> BoxStream<'static, BackendEvent> {
            futures::stream::empty().boxed()
        }

        async fn exec(&mut self, op: BackendOp) -> crate::error::Result<OpResult> {
            self.ops.push(op);
            self.responses.pop_front().unwrap_or(Ok(OpResult::null()))
        }

        async fn list_targets(&self) -> crate::error::Result<Vec<TargetInfo>> {
            Ok(vec![])
        }

        async fn checkpoint(&self) -> crate::error::Result<Checkpoint> {
            Ok(Checkpoint {
                kind: BackendKind::Local,
                pid: None,
                ws_url: "ws://mock".into(),
                cdp_port: None,
                user_data_dir: None,
                headers: None,
            })
        }

        async fn health(&self) -> crate::error::Result<Health> {
            Ok(Health {
                connected: true,
                browser_version: None,
                uptime_secs: None,
            })
        }

        async fn shutdown(&mut self, _policy: ShutdownPolicy) -> crate::error::Result<()> {
            Ok(())
        }
    }

    fn make_regs_with_tab() -> Registries {
        let mut regs = Registries::new();
        let tab_id = regs.alloc_tab_id();
        let win_id = regs.alloc_window_id();
        regs.tabs.insert(
            tab_id,
            TabEntry {
                id: tab_id,
                target_id: "TARGET_0".into(),
                window: win_id,
                url: "https://example.com".into(),
                title: "Example".into(),
            },
        );
        regs.windows.insert(
            win_id,
            WindowEntry {
                id: win_id,
                tabs: vec![tab_id],
            },
        );
        regs
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn goto_sends_navigate_op() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(json!({})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Goto {
                session: sid,
                tab: TabId(0),
                url: "https://rust-lang.org".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 1);
        match &backend.ops()[0] {
            BackendOp::Navigate { target_id, url } => {
                assert_eq!(target_id, "TARGET_0");
                assert_eq!(url, "https://rust-lang.org");
            }
            other => panic!("expected Navigate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn goto_tab_not_found() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Goto {
                session: sid,
                tab: TabId(99),
                url: "https://example.com".into(),
            },
        )
        .await;

        assert!(!result.is_ok());
        match result {
            ActionResult::Fatal { code, hint, .. } => {
                assert_eq!(code, "tab_not_found");
                assert!(hint.contains("list-tabs"));
            }
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn snapshot_sends_get_accessibility_tree() {
        let tree = json!({"nodes": [{"role": "button", "name": "Submit"}]});
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(tree.clone()))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Snapshot {
                session: sid,
                tab: TabId(0),
                interactive: false,
                compact: false,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 1);
        assert!(matches!(
            &backend.ops()[0],
            BackendOp::GetAccessibilityTree { .. }
        ));
    }

    #[tokio::test]
    async fn screenshot_sends_capture() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"data": "base64data"})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Screenshot {
                session: sid,
                tab: TabId(0),
                full_page: true,
            },
        )
        .await;

        assert!(result.is_ok());
        match &backend.ops()[0] {
            BackendOp::CaptureScreenshot { full_page, .. } => assert!(full_page),
            other => panic!("expected CaptureScreenshot, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn click_sends_eval_then_mouse_events() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({
                "result": {"type": "object", "value": {"x": 100.0, "y": 200.0}}
            }))),
            Ok(OpResult::null()),
            Ok(OpResult::null()),
            Ok(OpResult::null()),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Click {
                session: sid,
                tab: TabId(0),
                selector: "#btn".into(),
                button: None,
                count: None,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 4);
        assert!(matches!(&backend.ops()[0], BackendOp::Evaluate { .. }));
        assert!(
            matches!(&backend.ops()[1], BackendOp::DispatchMouseEvent { event_type, .. } if event_type == "mouseMoved")
        );
        assert!(
            matches!(&backend.ops()[2], BackendOp::DispatchMouseEvent { event_type, .. } if event_type == "mousePressed")
        );
        assert!(
            matches!(&backend.ops()[3], BackendOp::DispatchMouseEvent { event_type, .. } if event_type == "mouseReleased")
        );
    }

    #[tokio::test]
    async fn click_element_not_found() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"type": "object", "value": null}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Click {
                session: sid,
                tab: TabId(0),
                selector: "#nonexistent".into(),
                button: None,
                count: None,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "element_not_found"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn type_focuses_then_dispatches_keys() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({"result": {"value": true}}))),
            Ok(OpResult::null()),
            Ok(OpResult::null()),
            Ok(OpResult::null()),
            Ok(OpResult::null()),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Type {
                session: sid,
                tab: TabId(0),
                selector: "input".into(),
                text: "hi".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 5);
        assert!(matches!(&backend.ops()[0], BackendOp::Evaluate { .. }));
        assert!(
            matches!(&backend.ops()[1], BackendOp::DispatchKeyEvent { event_type, .. } if event_type == "keyDown")
        );
    }

    #[tokio::test]
    async fn fill_uses_js_value_setter() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": true}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Fill {
                session: sid,
                tab: TabId(0),
                selector: "input".into(),
                value: "hello".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 1);
        assert!(matches!(&backend.ops()[0], BackendOp::Evaluate { .. }));
    }

    #[tokio::test]
    async fn eval_returns_value() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"type": "string", "value": "Example Title"}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Eval {
                session: sid,
                tab: TabId(0),
                expression: "document.title".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => assert_eq!(data, "Example Title"),
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn list_tabs_returns_registry_content() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::ListTabs { session: sid },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                let tabs = data["tabs"].as_array().unwrap();
                assert_eq!(tabs.len(), 1);
                assert_eq!(tabs[0]["id"], "t0");
                assert_eq!(tabs[0]["url"], "https://example.com");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn new_tab_creates_target_and_registers() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"targetId": "NEW_TARGET_1"})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::NewTab {
                session: sid,
                url: "https://new-page.com".into(),
                new_window: false,
                window: None,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(regs.tabs.len(), 2);
        let new_tab = regs.tabs.get(&TabId(1)).unwrap();
        assert_eq!(new_tab.target_id, "NEW_TARGET_1");
        assert_eq!(new_tab.url, "https://new-page.com");
    }

    #[tokio::test]
    async fn close_tab_removes_from_registry() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(json!(true)))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        assert_eq!(regs.tabs.len(), 1);
        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::CloseTab {
                session: sid,
                tab: TabId(0),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(regs.tabs.len(), 0);
        assert!(regs.windows.get(&WindowId(0)).unwrap().tabs.is_empty());
    }

    #[tokio::test]
    async fn backend_disconnect_returns_retryable() {
        let mut backend = MockBackendSession::new(vec![Err(ActionbookError::CdpConnectionFailed(
            "WS closed".into(),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Goto {
                session: sid,
                tab: TabId(0),
                url: "https://example.com".into(),
            },
        )
        .await;

        match result {
            ActionResult::Retryable { reason, .. } => {
                assert_eq!(reason, "backend_disconnected");
            }
            _ => panic!("expected Retryable, got {result:?}"),
        }
    }

    #[tokio::test]
    async fn cdp_error_returns_fatal() {
        let mut backend = MockBackendSession::new(vec![Err(ActionbookError::CdpError(
            "CDP error: method not found".into(),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Goto {
                session: sid,
                tab: TabId(0),
                url: "https://example.com".into(),
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "cdp_error"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn global_action_returns_fatal() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = Registries::new();
        let sid = SessionId(0);

        let result = handle_action(sid, &mut backend, &mut regs, Action::ListSessions).await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "invalid_dispatch"),
            _ => panic!("expected Fatal for global action"),
        }
    }

    #[tokio::test]
    async fn cookies_list_filters_by_domain_and_returns_items() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(json!({
            "cookies": [
                {"name": "keep", "domain": ".example.com", "path": "/"},
                {"name": "drop", "domain": ".other.com", "path": "/"}
            ]
        })))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::CookiesList {
                session: sid,
                domain: Some("example.com".into()),
            },
        )
        .await;

        match result {
            ActionResult::Ok { data } => {
                let items = data["items"].as_array().unwrap();
                assert_eq!(items.len(), 1);
                assert_eq!(items[0]["name"], "keep");
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cookies_get_returns_item_key() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(json!({
            "cookies": [
                {"name": "session", "value": "abc123", "domain": ".example.com", "path": "/"}
            ]
        })))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::CookiesGet {
                session: sid,
                name: "session".into(),
            },
        )
        .await;

        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["item"]["name"], "session");
                assert_eq!(data["item"]["value"], "abc123");
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cookies_clear_filters_by_domain_and_reports_affected() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({
                "cookies": [
                    {"name": "keep", "domain": ".other.com", "path": "/"},
                    {"name": "clear", "domain": ".example.com", "path": "/"}
                ]
            }))),
            Ok(OpResult::null()),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::CookiesClear {
                session: sid,
                domain: Some("example.com".into()),
            },
        )
        .await;

        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["action"], "clear");
                assert_eq!(data["affected"], 1);
                assert_eq!(data["domain"], "example.com");
            }
            other => panic!("expected Ok, got {other:?}"),
        }

        assert_eq!(backend.ops().len(), 2);
        match &backend.ops()[1] {
            BackendOp::DeleteCookies { name, domain, .. } => {
                assert_eq!(name, "clear");
                assert_eq!(domain.as_deref(), Some(".example.com"));
            }
            other => panic!("expected DeleteCookies, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn html_full_page() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": "<html><body>Hello</body></html>"}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Html {
                session: sid,
                tab: TabId(0),
                selector: None,
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn text_with_selector_not_found() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": null}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId(0);

        let result = handle_action(
            sid,
            &mut backend,
            &mut regs,
            Action::Text {
                session: sid,
                tab: TabId(0),
                selector: Some("#missing".into()),
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "element_not_found"),
            _ => panic!("expected Fatal for missing element"),
        }
    }
}
