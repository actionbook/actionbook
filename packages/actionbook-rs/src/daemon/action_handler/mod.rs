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
use super::types::{
    QueryCardinality, QueryMode, SameSite, SessionId, StorageKind, TabId, WindowId,
};
use crate::error::ActionbookError;

mod data;
mod interaction;
mod navigation;
mod observation;
mod session;
mod waiting;

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
        Action::Goto { tab, url, .. } => {
            navigation::handle_goto(session_id, backend, regs, tab, &url).await
        }
        Action::Back { tab, .. } => {
            navigation::handle_history(backend, regs, session_id, tab, "back").await
        }
        Action::Forward { tab, .. } => {
            navigation::handle_history(backend, regs, session_id, tab, "forward").await
        }
        Action::Reload { tab, .. } => {
            navigation::handle_reload(session_id, backend, regs, tab).await
        }
        Action::Open { url, .. } => {
            session::handle_new_tab(session_id, backend, regs, &url, false, None).await
        }
        Action::Snapshot { tab, .. } => {
            observation::handle_snapshot(session_id, backend, regs, tab).await
        }
        Action::Screenshot { tab, full_page, .. } => {
            observation::handle_screenshot(session_id, backend, regs, tab, full_page).await
        }
        Action::Click {
            tab,
            selector,
            button,
            count,
            ..
        } => {
            interaction::handle_click(
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
        } => interaction::handle_type(session_id, backend, regs, tab, &selector, &text).await,
        Action::Fill {
            tab,
            selector,
            value,
            ..
        } => interaction::handle_fill(session_id, backend, regs, tab, &selector, &value).await,
        Action::Eval {
            tab, expression, ..
        } => observation::handle_eval(session_id, backend, regs, tab, &expression).await,
        Action::WaitElement {
            tab,
            selector,
            timeout_ms,
            ..
        } => {
            observation::handle_wait_element(session_id, backend, regs, tab, &selector, timeout_ms)
                .await
        }
        Action::Html { tab, selector, .. } => {
            observation::handle_html(session_id, backend, regs, tab, selector.as_deref()).await
        }
        Action::Text { tab, selector, .. } => {
            observation::handle_text(session_id, backend, regs, tab, selector.as_deref()).await
        }

        // -- Session-level commands --
        Action::ListTabs { .. } => session::handle_list_tabs(regs),
        Action::ListWindows { .. } => session::handle_list_windows(regs),
        Action::NewTab {
            url,
            new_window,
            window,
            ..
        } => session::handle_new_tab(session_id, backend, regs, &url, new_window, window).await,
        Action::CloseTab { tab, .. } => {
            session::handle_close_tab(session_id, backend, regs, tab).await
        }
        Action::Close { .. } | Action::CloseSession { .. } => {
            // Handled at the session actor level, not here.
            ActionResult::ok(json!({"closed": true}))
        }

        // -- Observation commands (tab-level) --
        Action::Pdf { tab, path, .. } => {
            observation::handle_pdf(session_id, backend, regs, tab, &path).await
        }
        Action::Title { tab, .. } => {
            observation::handle_title(session_id, backend, regs, tab).await
        }
        Action::Url { tab, .. } => observation::handle_url(session_id, backend, regs, tab).await,
        Action::Value { tab, selector, .. } => {
            observation::handle_value(session_id, backend, regs, tab, &selector).await
        }
        Action::Attr {
            tab,
            selector,
            name,
            ..
        } => observation::handle_attr(session_id, backend, regs, tab, &selector, &name).await,
        Action::Attrs { tab, selector, .. } => {
            observation::handle_attrs(session_id, backend, regs, tab, &selector).await
        }
        Action::Describe { tab, selector, .. } => {
            observation::handle_describe(session_id, backend, regs, tab, &selector).await
        }
        Action::State { tab, selector, .. } => {
            observation::handle_state(session_id, backend, regs, tab, &selector).await
        }
        Action::Box_ { tab, selector, .. } => {
            observation::handle_box(session_id, backend, regs, tab, &selector).await
        }
        Action::Styles { tab, selector, .. } => {
            observation::handle_styles(session_id, backend, regs, tab, &selector).await
        }
        Action::Viewport { tab, .. } => {
            observation::handle_viewport(session_id, backend, regs, tab).await
        }
        Action::Query {
            tab,
            selector,
            mode,
            cardinality,
            nth_index,
            ..
        } => {
            observation::handle_query(
                session_id,
                backend,
                regs,
                tab,
                &selector,
                mode,
                cardinality,
                nth_index,
            )
            .await
        }
        Action::InspectPoint { tab, x, y, .. } => {
            observation::handle_inspect_point(session_id, backend, regs, tab, x, y).await
        }
        Action::LogsConsole { tab, .. } => {
            observation::handle_logs_console(session_id, backend, regs, tab).await
        }
        Action::LogsErrors { tab, .. } => {
            observation::handle_logs_errors(session_id, backend, regs, tab).await
        }

        // -- Data commands (session-level cookies) --
        Action::CookiesList { ref domain, .. } => {
            data::handle_cookies_list(session_id, backend, regs, domain.as_deref()).await
        }
        Action::CookiesGet { name, .. } => {
            data::handle_cookies_get(session_id, backend, regs, &name).await
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
            data::handle_cookies_set(
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
            data::handle_cookies_delete(session_id, backend, regs, &name).await
        }
        Action::CookiesClear { ref domain, .. } => {
            data::handle_cookies_clear(session_id, backend, regs, domain.as_deref()).await
        }

        // -- Data commands (tab-level storage) --
        Action::StorageList { tab, kind, .. } => {
            data::handle_storage_list(session_id, backend, regs, tab, kind).await
        }
        Action::StorageGet { tab, kind, key, .. } => {
            data::handle_storage_get(session_id, backend, regs, tab, kind, &key).await
        }
        Action::StorageSet {
            tab,
            kind,
            key,
            value,
            ..
        } => data::handle_storage_set(session_id, backend, regs, tab, kind, &key, &value).await,
        Action::StorageDelete { tab, kind, key, .. } => {
            data::handle_storage_delete(session_id, backend, regs, tab, kind, &key).await
        }
        Action::StorageClear { tab, kind, .. } => {
            data::handle_storage_clear(session_id, backend, regs, tab, kind).await
        }

        // -- Interaction commands --
        Action::Select {
            tab,
            selector,
            value,
            by_text,
            ..
        } => {
            interaction::handle_select(session_id, backend, regs, tab, &selector, &value, by_text)
                .await
        }
        Action::Hover { tab, selector, .. } => {
            interaction::handle_hover(session_id, backend, regs, tab, &selector).await
        }
        Action::Focus { tab, selector, .. } => {
            interaction::handle_focus(session_id, backend, regs, tab, &selector).await
        }
        Action::Press {
            tab, key_or_chord, ..
        } => interaction::handle_press(session_id, backend, regs, tab, &key_or_chord).await,
        Action::Drag {
            tab,
            from_selector,
            to_selector,
            ..
        } => {
            interaction::handle_drag(session_id, backend, regs, tab, &from_selector, &to_selector)
                .await
        }
        Action::Upload {
            tab,
            selector,
            files,
            ..
        } => interaction::handle_upload(session_id, backend, regs, tab, &selector, &files).await,
        Action::Scroll {
            tab,
            direction,
            amount,
            selector,
            ..
        } => {
            interaction::handle_scroll(
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
            interaction::handle_mouse_move(session_id, backend, regs, tab, x, y).await
        }
        Action::CursorPosition { tab, .. } => {
            interaction::handle_cursor_position(session_id, backend, regs, tab).await
        }

        // -- Waiting commands --
        Action::WaitNavigation {
            tab, timeout_ms, ..
        } => waiting::handle_wait_navigation(session_id, backend, regs, tab, timeout_ms).await,
        Action::WaitNetworkIdle {
            tab,
            timeout_ms,
            idle_time_ms,
            ..
        } => {
            waiting::handle_wait_network_idle(
                session_id,
                backend,
                regs,
                tab,
                timeout_ms,
                idle_time_ms,
            )
            .await
        }
        Action::WaitCondition {
            tab,
            expression,
            timeout_ms,
            ..
        } => {
            waiting::handle_wait_condition(session_id, backend, regs, tab, &expression, timeout_ms)
                .await
        }

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
// Shared helpers
// ---------------------------------------------------------------------------

/// Look up a tab's CDP target_id, or return a Fatal ActionResult.
pub(super) fn resolve_tab(
    session_id: SessionId,
    regs: &Registries,
    tab: TabId,
) -> Result<&str, ActionResult> {
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
pub(super) async fn focus_element(
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

/// Resolve an element's center coordinates using JS.
pub(super) async fn resolve_element_center(
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
pub(super) fn map_key_name(key: &str) -> (&str, &str) {
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

/// Extract the actual return value from a Runtime.evaluate result.
///
/// CDP wraps the value as `{ "result": { "type": "...", "value": <actual> } }`.
pub(super) fn extract_eval_value(cdp_result: &serde_json::Value) -> serde_json::Value {
    cdp_result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or_else(|| cdp_result.clone())
}

pub(super) fn element_not_found(selector: &str) -> ActionResult {
    ActionResult::fatal(
        "element_not_found",
        format!("element '{}' not found", selector),
        "check selector or use `actionbook browser snapshot` to see available elements",
    )
}

pub(super) fn cdp_error_to_result(err: ActionbookError) -> ActionResult {
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
pub(super) const FIND_ELEMENT_JS: &str = r#"
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
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({}))),
            Ok(OpResult::new(json!("Rust"))),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        assert_eq!(backend.ops().len(), 2);
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let mut backend = MockBackendSession::new(vec![
            // First call: log capture initialization
            Ok(OpResult::new(
                json!({"result": {"type": "boolean", "value": true}}),
            )),
            // Second call: actual eval
            Ok(OpResult::new(
                json!({"result": {"type": "string", "value": "Example Title"}}),
            )),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
                assert_eq!(tabs[0]["tab_id"], "t0");
                assert_eq!(tabs[0]["url"], "https://example.com");
                assert_eq!(data["total_tabs"], 1);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn new_tab_creates_target_and_registers() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"targetId": "NEW_TARGET_1"})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        assert_eq!(regs.tabs.len(), 1);
        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
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
