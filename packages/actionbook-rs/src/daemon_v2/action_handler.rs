//! Action handler — compiles high-level Actions into BackendOp sequences.
//!
//! Each handler method takes an `&mut dyn BackendSession`, the session's
//! tab/window registries, and the Action-specific parameters. It returns an
//! [`ActionResult`].
//!
//! The session actor calls [`handle_action`] which dispatches to the correct
//! handler based on the Action variant.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::action::Action;
use super::action_result::ActionResult;
use super::backend::BackendSession;
use super::backend_op::BackendOp;
use super::types::{SessionId, TabId, WindowId};
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
    pub tabs: std::collections::HashMap<TabId, TabEntry>,
    pub windows: std::collections::HashMap<WindowId, WindowEntry>,
    pub next_tab_id: u32,
    pub next_window_id: u32,
}

impl Registries {
    pub fn new() -> Self {
        Self {
            tabs: std::collections::HashMap::new(),
            windows: std::collections::HashMap::new(),
            next_tab_id: 0,
            next_window_id: 0,
        }
    }

    pub fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

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
        Action::Goto { tab, url, .. } => {
            handle_goto(session_id, backend, regs, tab, &url).await
        }
        Action::Back { tab, .. } => {
            handle_history(backend, regs, session_id, tab, "back").await
        }
        Action::Forward { tab, .. } => {
            handle_history(backend, regs, session_id, tab, "forward").await
        }
        Action::Reload { tab, .. } => {
            handle_reload(session_id, backend, regs, tab).await
        }
        Action::Open { url, .. } => {
            handle_new_tab(session_id, backend, regs, &url, false, None).await
        }
        Action::Snapshot { tab, .. } => {
            handle_snapshot(session_id, backend, regs, tab).await
        }
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
            handle_click(session_id, backend, regs, tab, &selector, button.as_deref(), count)
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
        Action::Html {
            tab, selector, ..
        } => handle_html(session_id, backend, regs, tab, selector.as_deref()).await,
        Action::Text {
            tab, selector, ..
        } => handle_text(session_id, backend, regs, tab, selector.as_deref()).await,

        // -- Session-level commands --
        Action::ListTabs { .. } => handle_list_tabs(regs),
        Action::ListWindows { .. } => handle_list_windows(regs),
        Action::NewTab {
            url,
            new_window,
            window,
            ..
        } => handle_new_tab(session_id, backend, regs, &url, new_window, window).await,
        Action::CloseTab { tab, .. } => {
            handle_close_tab(session_id, backend, regs, tab).await
        }
        Action::Close { .. } | Action::CloseSession { .. } => {
            // Handled at the session actor level, not here.
            ActionResult::ok(json!({"closed": true}))
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
            return ActionResult::fatal(
                "invalid_selector",
                e.to_string(),
                "check selector syntax",
            )
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
            return ActionResult::fatal(
                "invalid_selector",
                e.to_string(),
                "check selector syntax",
            )
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
            if val.is_null() && selector.is_some() {
                element_not_found(selector.unwrap())
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
            if val.is_null() && selector.is_some() {
                element_not_found(selector.unwrap())
            } else {
                ActionResult::ok(json!({"text": val}))
            }
        }
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
                if !regs.windows.contains_key(&w) {
                    regs.windows.insert(
                        w,
                        WindowEntry {
                            id: w,
                            tabs: Vec::new(),
                        },
                    );
                }
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
// Helpers
// ---------------------------------------------------------------------------

/// Look up a tab's CDP target_id, or return a Fatal ActionResult.
fn resolve_tab<'a>(
    session_id: SessionId,
    regs: &'a Registries,
    tab: TabId,
) -> Result<&'a str, ActionResult> {
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
        ActionResult::fatal(
            "invalid_selector",
            e.to_string(),
            "check selector syntax",
        )
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
        ActionbookError::CdpError(msg) => {
            ActionResult::fatal("cdp_error", msg.clone(), "check the CDP command and parameters")
        }
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
    use crate::daemon_v2::backend::{
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
            self.responses
                .pop_front()
                .unwrap_or(Ok(OpResult::null()))
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
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"data": "base64data"}),
        ))]);
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
        assert!(matches!(&backend.ops()[1], BackendOp::DispatchMouseEvent { event_type, .. } if event_type == "mouseMoved"));
        assert!(matches!(&backend.ops()[2], BackendOp::DispatchMouseEvent { event_type, .. } if event_type == "mousePressed"));
        assert!(matches!(&backend.ops()[3], BackendOp::DispatchMouseEvent { event_type, .. } if event_type == "mouseReleased"));
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
        assert!(matches!(&backend.ops()[1], BackendOp::DispatchKeyEvent { event_type, .. } if event_type == "keyDown"));
    }

    #[tokio::test]
    async fn fill_uses_js_value_setter() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": true}}),
        ))]);
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
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"targetId": "NEW_TARGET_1"}),
        ))]);
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
        let mut backend = MockBackendSession::new(vec![Err(
            ActionbookError::CdpConnectionFailed("WS closed".into()),
        )]);
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
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": null}}),
        ))]);
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
