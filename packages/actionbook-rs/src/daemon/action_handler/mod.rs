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
        Action::Snapshot {
            tab,
            interactive,
            compact,
            cursor,
            depth,
            selector,
            ..
        } => {
            observation::handle_snapshot(
                session_id,
                backend,
                regs,
                tab,
                interactive,
                compact,
                cursor,
                depth,
                selector.as_deref(),
            )
            .await
        }
        Action::Screenshot { tab, full_page, .. } => {
            observation::handle_screenshot(session_id, backend, regs, tab, full_page).await
        }
        Action::Click {
            tab,
            selector,
            button,
            count,
            new_tab,
            coordinates,
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
                new_tab,
                coordinates,
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
        Action::Text {
            tab,
            selector,
            mode,
            ..
        } => {
            observation::handle_text(
                session_id,
                backend,
                regs,
                tab,
                selector.as_deref(),
                mode.as_deref(),
            )
            .await
        }

        // -- Session-level commands --
        Action::ListTabs { .. } => session::handle_list_tabs(backend, regs).await,
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
        Action::Styles {
            tab,
            selector,
            names,
            ..
        } => observation::handle_styles(session_id, backend, regs, tab, &selector, &names).await,
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
        Action::LogsConsole {
            tab,
            level,
            tail,
            since,
            clear,
            ..
        } => {
            observation::handle_logs_console(
                session_id,
                backend,
                regs,
                tab,
                level.as_deref(),
                tail,
                since.as_deref(),
                clear,
            )
            .await
        }
        Action::LogsErrors {
            tab,
            source,
            tail,
            since,
            clear,
            ..
        } => {
            observation::handle_logs_errors(
                session_id,
                backend,
                regs,
                tab,
                source.as_deref(),
                tail,
                since.as_deref(),
                clear,
            )
            .await
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
            button,
            to_coordinates,
            ..
        } => {
            interaction::handle_drag(
                session_id,
                backend,
                regs,
                tab,
                &from_selector,
                &to_selector,
                button.as_deref(),
                to_coordinates,
            )
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
            container,
            align,
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
                container.as_deref(),
                align.as_deref(),
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
        targets: Vec<TargetInfo>,
    }

    impl MockBackendSession {
        fn new(responses: Vec<Result<OpResult, ActionbookError>>) -> Self {
            Self {
                ops: Vec::new(),
                responses: responses.into(),
                targets: Vec::new(),
            }
        }

        fn with_targets(mut self, targets: Vec<TargetInfo>) -> Self {
            self.targets = targets;
            self
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
            Ok(self.targets.clone())
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
            Ok(OpResult::new(json!({}))), // Navigate
            Ok(OpResult::new(
                json!({"url": "https://rust-lang.org", "ready": "complete"}),
            )), // wait_for_page_load
            Ok(OpResult::new(json!("Rust"))), // document.title
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
        assert_eq!(backend.ops().len(), 3);
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
        // Provide a valid CDP Accessibility.getFullAXTree response structure
        let tree = json!({"nodes": [
            {
                "nodeId": "1",
                "role": {"type": "role", "value": "RootWebArea"},
                "name": {"type": "computedString", "value": "Test"},
                "childIds": ["2"],
                "properties": []
            },
            {
                "nodeId": "2",
                "backendDOMNodeId": 10,
                "role": {"type": "role", "value": "button"},
                "name": {"type": "computedString", "value": "Submit"},
                "childIds": [],
                "properties": []
            }
        ]});
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
                cursor: false,
                depth: None,
                selector: None,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 1);
        assert!(matches!(
            &backend.ops()[0],
            BackendOp::GetAccessibilityTree { .. }
        ));

        // Verify PRD 10.1 shape in response
        if let ActionResult::Ok { data } = &result {
            assert_eq!(data["format"], "snapshot");
            assert!(data["content"].as_str().unwrap().contains("button"));
            assert!(!data["nodes"].as_array().unwrap().is_empty());
            assert!(data["stats"]["node_count"].as_u64().unwrap() > 0);
        }
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
                new_tab: false,
                coordinates: None,
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
                new_tab: false,
                coordinates: None,
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
        let mut backend = MockBackendSession::new(vec![]).with_targets(vec![TargetInfo {
            target_id: "TARGET_0".into(),
            target_type: "page".into(),
            title: "Example Title".into(),
            url: "https://example.com/updated".into(),
            attached: true,
        }]);
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
                assert_eq!(tabs[0]["url"], "https://example.com/updated");
                assert_eq!(tabs[0]["title"], "Example Title");
                assert_eq!(tabs[0]["native_tab_id"], "TARGET_0");
                assert_eq!(data["total_tabs"], 1);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn new_tab_creates_target_and_registers() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({"targetId": "NEW_TARGET_1"}))),
            Ok(OpResult::null()),
            Ok(OpResult::new(json!("complete"))),
            Ok(OpResult::new(json!("https://new-page.com"))),
            Ok(OpResult::new(json!("New Page"))),
        ]);
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
        assert_eq!(new_tab.title, "New Page");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["tab"]["tab_id"], "t1");
                assert_eq!(data["tab"]["url"], "https://new-page.com");
                assert_eq!(data["tab"]["title"], "New Page");
                assert_eq!(data["tab"]["native_tab_id"], "NEW_TARGET_1");
                assert_eq!(data["created"], true);
                assert_eq!(data["new_window"], false);
            }
            _ => panic!("expected Ok"),
        }
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
                mode: None,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "element_not_found"),
            _ => panic!("expected Fatal for missing element"),
        }
    }

    // -----------------------------------------------------------------------
    // Session handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn list_tabs_empty_registry() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = Registries::new();
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
                assert_eq!(data["tabs"].as_array().unwrap().len(), 0);
                assert_eq!(data["total_tabs"], 0);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn list_windows_returns_registry_content() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::ListWindows { session: sid },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                let windows = data["windows"].as_array().unwrap();
                assert_eq!(windows.len(), 1);
                assert_eq!(windows[0]["id"], "w0");
                let tabs = windows[0]["tabs"].as_array().unwrap();
                assert_eq!(tabs.len(), 1);
                assert_eq!(tabs[0], "t0");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn list_windows_empty_registry() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = Registries::new();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::ListWindows { session: sid },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["windows"].as_array().unwrap().len(), 0);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn new_tab_with_new_window_creates_window() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({"targetId": "NEW_T"}))),
            Ok(OpResult::null()),
            Ok(OpResult::new(json!("complete"))),
            Ok(OpResult::new(json!("https://new.com"))),
            Ok(OpResult::new(json!("New Window Tab"))),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        assert_eq!(regs.windows.len(), 1);

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::NewTab {
                session: sid,
                url: "https://new.com".into(),
                new_window: true,
                window: None,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(regs.windows.len(), 2);
        assert_eq!(regs.tabs.len(), 2);
        let new_tab = regs.tabs.get(&TabId(1)).unwrap();
        assert_eq!(new_tab.window, WindowId(1));
        match &backend.ops()[0] {
            BackendOp::CreateTarget { new_window, .. } => assert!(new_window),
            other => panic!("expected CreateTarget, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn new_tab_with_explicit_window() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({"targetId": "NEW_T2"}))),
            Ok(OpResult::null()),
            Ok(OpResult::new(json!("complete"))),
            Ok(OpResult::new(json!("https://tab-in-w0.com"))),
            Ok(OpResult::new(json!("Window 0 Tab"))),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::NewTab {
                session: sid,
                url: "https://tab-in-w0.com".into(),
                new_window: false,
                window: Some(WindowId(0)),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(regs.windows.len(), 1);
        assert_eq!(regs.tabs.len(), 2);
        let new_tab = regs.tabs.get(&TabId(1)).unwrap();
        assert_eq!(new_tab.window, WindowId(0));
        assert_eq!(regs.windows.get(&WindowId(0)).unwrap().tabs.len(), 2);
        match &backend.ops()[0] {
            BackendOp::CreateTarget { window_id, .. } => {
                assert_eq!(*window_id, Some(0));
            }
            other => panic!("expected CreateTarget, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn new_tab_backend_missing_target_id() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"other": "data"})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::NewTab {
                session: sid,
                url: "https://fail.com".into(),
                new_window: false,
                window: None,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "create_target_failed"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn new_tab_backend_error() {
        let mut backend = MockBackendSession::new(vec![Err(ActionbookError::CdpError(
            "cannot create target".into(),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::NewTab {
                session: sid,
                url: "https://error.com".into(),
                new_window: false,
                window: None,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "cdp_error"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn close_tab_not_found() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::CloseTab {
                session: sid,
                tab: TabId(99),
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "tab_not_found"),
            _ => panic!("expected Fatal"),
        }
    }

    // -----------------------------------------------------------------------
    // Navigation handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn back_sends_history_back() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({}))), // history.back()
            Ok(OpResult::new(
                json!({"url": "https://prev.com", "ready": "complete"}),
            )), // wait_for_page_load
            Ok(OpResult::new(json!("Previous Page"))), // document.title
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Back {
                session: sid,
                tab: TabId(0),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["kind"], "back");
                assert_eq!(data["to_url"], "https://prev.com");
                assert_eq!(data["title"], "Previous Page");
            }
            _ => panic!("expected Ok"),
        }
        // Should have 3 ops: history.back(), wait_for_page_load eval, document.title
        assert_eq!(backend.ops().len(), 3);
    }

    #[tokio::test]
    async fn forward_sends_history_forward() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({}))), // history.forward()
            Ok(OpResult::new(
                json!({"url": "https://next.com", "ready": "complete"}),
            )), // wait_for_page_load
            Ok(OpResult::new(json!("Next Page"))), // document.title
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Forward {
                session: sid,
                tab: TabId(0),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["kind"], "forward");
                assert_eq!(data["to_url"], "https://next.com");
                assert_eq!(data["title"], "Next Page");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn reload_sends_location_reload() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({}))), // location.reload()
            Ok(OpResult::new(
                json!({"url": "https://example.com", "ready": "complete"}),
            )), // wait_for_page_load
            Ok(OpResult::new(json!("Reloaded"))), // document.title
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Reload {
                session: sid,
                tab: TabId(0),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["kind"], "reload");
                assert_eq!(data["title"], "Reloaded");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn goto_updates_tab_url_and_title() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({}))), // Navigate
            Ok(OpResult::new(
                json!({"url": "https://new-page.com", "ready": "complete"}),
            )), // wait_for_page_load
            Ok(OpResult::new(json!("New Title"))), // document.title
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
                url: "https://new-page.com".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        let tab = regs.tabs.get(&TabId(0)).unwrap();
        assert_eq!(tab.url, "https://new-page.com");
        assert_eq!(tab.title, "New Title");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["title"], "New Title");
                assert_eq!(data["from_url"], "https://example.com");
                assert_eq!(data["to_url"], "https://new-page.com");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn goto_title_fetch_failure_graceful() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({}))), // Navigate
            Ok(OpResult::new(
                json!({"url": "https://new-page.com", "ready": "complete"}),
            )), // wait_for_page_load
            Err(ActionbookError::CdpError("eval failed".into())), // document.title fails
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
                url: "https://new-page.com".into(),
            },
        )
        .await;

        // Should still succeed — title fetch is best-effort
        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["title"], "");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn back_tab_not_found() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Back {
                session: sid,
                tab: TabId(99),
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "tab_not_found"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn reload_tab_not_found() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Reload {
                session: sid,
                tab: TabId(99),
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "tab_not_found"),
            _ => panic!("expected Fatal"),
        }
    }

    // -----------------------------------------------------------------------
    // Interaction handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn hover_sends_mouse_moved() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({
                "result": {"type": "object", "value": {"x": 50.0, "y": 75.0}}
            }))),
            Ok(OpResult::null()),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Hover {
                session: sid,
                tab: TabId(0),
                selector: "#menu".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["hovered"], "#menu");
                assert_eq!(data["x"], 50.0);
                assert_eq!(data["y"], 75.0);
            }
            _ => panic!("expected Ok"),
        }
        assert!(
            matches!(&backend.ops()[1], BackendOp::DispatchMouseEvent { event_type, button, .. } if event_type == "mouseMoved" && button == "none")
        );
    }

    #[tokio::test]
    async fn focus_sends_evaluate() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": true}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Focus {
                session: sid,
                tab: TabId(0),
                selector: "input#email".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["focused"], "input#email");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn press_single_key() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::null()), // keyDown
            Ok(OpResult::null()), // keyUp
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Press {
                session: sid,
                tab: TabId(0),
                key_or_chord: "enter".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 2);
        match &backend.ops()[0] {
            BackendOp::DispatchKeyEvent {
                event_type, key, ..
            } => {
                assert_eq!(event_type, "keyDown");
                assert_eq!(key, "Enter");
            }
            other => panic!("expected DispatchKeyEvent, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn press_chord_with_modifier() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::null()), // Control keyDown
            Ok(OpResult::null()), // A keyDown
            Ok(OpResult::null()), // A keyUp
            Ok(OpResult::null()), // Control keyUp
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Press {
                session: sid,
                tab: TabId(0),
                key_or_chord: "Control+A".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 4);
        // First op: Control down
        match &backend.ops()[0] {
            BackendOp::DispatchKeyEvent { key, .. } => assert_eq!(key, "Control"),
            other => panic!("expected DispatchKeyEvent, got {other:?}"),
        }
        // Last op: Control up
        match &backend.ops()[3] {
            BackendOp::DispatchKeyEvent {
                event_type, key, ..
            } => {
                assert_eq!(event_type, "keyUp");
                assert_eq!(key, "Control");
            }
            other => panic!("expected DispatchKeyEvent, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn select_by_value() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": "opt2"}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Select {
                session: sid,
                tab: TabId(0),
                selector: "select#country".into(),
                value: "opt2".into(),
                by_text: false,
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["selected"], "opt2");
                assert_eq!(data["selector"], "select#country");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn select_element_not_found() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": null}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Select {
                session: sid,
                tab: TabId(0),
                selector: "#missing-select".into(),
                value: "val".into(),
                by_text: false,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "element_not_found"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn mouse_move_dispatches_event() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::null())]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::MouseMove {
                session: sid,
                tab: TabId(0),
                x: 120.0,
                y: 240.0,
            },
        )
        .await;

        assert!(result.is_ok());
        match &backend.ops()[0] {
            BackendOp::DispatchMouseEvent {
                event_type,
                x,
                y,
                button,
                ..
            } => {
                assert_eq!(event_type, "mouseMoved");
                assert_eq!(*x, 120.0);
                assert_eq!(*y, 240.0);
                assert_eq!(button, "none");
            }
            other => panic!("expected DispatchMouseEvent, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cursor_position_returns_coordinates() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": {"x": 10, "y": 20}}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::CursorPosition {
                session: sid,
                tab: TabId(0),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["cursor"]["x"], 10);
                assert_eq!(data["cursor"]["y"], 20);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn drag_moves_between_elements() {
        let mut backend = MockBackendSession::new(vec![
            // resolve from element
            Ok(OpResult::new(json!({
                "result": {"type": "object", "value": {"x": 10.0, "y": 20.0}}
            }))),
            // resolve to element
            Ok(OpResult::new(json!({
                "result": {"type": "object", "value": {"x": 100.0, "y": 200.0}}
            }))),
            Ok(OpResult::null()), // mouseMoved to source
            Ok(OpResult::null()), // mousePressed
            Ok(OpResult::null()), // mouseMoved to target
            Ok(OpResult::null()), // mouseReleased
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Drag {
                session: sid,
                tab: TabId(0),
                from_selector: "#source".into(),
                to_selector: "#target".into(),
                button: None,
                to_coordinates: None,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(backend.ops().len(), 6);
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["from"]["x"], 10.0);
                assert_eq!(data["to"]["x"], 100.0);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn scroll_down_sends_evaluate() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": true}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Scroll {
                session: sid,
                tab: TabId(0),
                direction: "down".into(),
                amount: Some(500),
                selector: None,
                container: None,
                align: None,
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["scrolled"], "down");
                assert_eq!(data["amount"], 500);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn scroll_invalid_direction() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Scroll {
                session: sid,
                tab: TabId(0),
                direction: "diagonal".into(),
                amount: None,
                selector: None,
                container: None,
                align: None,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "invalid_direction"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn scroll_into_view_missing_selector() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Scroll {
                session: sid,
                tab: TabId(0),
                direction: "into-view".into(),
                amount: None,
                selector: None,
                container: None,
                align: None,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "missing_selector"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn scroll_with_container_field_builds_correctly() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": true}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Scroll {
                session: sid,
                tab: TabId(0),
                direction: "down".into(),
                amount: Some(200),
                selector: None,
                container: Some("#scroll-container".into()),
                align: None,
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["scrolled"], "down");
                assert_eq!(data["amount"], 200);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn scroll_into_view_with_align_field() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": true}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Scroll {
                session: sid,
                tab: TabId(0),
                direction: "into-view".into(),
                amount: None,
                selector: Some("#hero".into()),
                container: None,
                align: Some("start".into()),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["scrolled"], "into-view");
                assert_eq!(data["selector"], "#hero");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn upload_sends_set_file_input_files() {
        let mut backend = MockBackendSession::new(vec![
            // GetDocument
            Ok(OpResult::new(json!({"root": {"nodeId": 1}}))),
            // QuerySelector
            Ok(OpResult::new(json!({"nodeId": 42}))),
            // SetFileInputFiles
            Ok(OpResult::null()),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Upload {
                session: sid,
                tab: TabId(0),
                selector: "input[type=file]".into(),
                files: vec!["/tmp/test.txt".into()],
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["uploaded"], 1);
            }
            _ => panic!("expected Ok"),
        }
        assert_eq!(backend.ops().len(), 3);
        assert!(matches!(
            &backend.ops()[2],
            BackendOp::SetFileInputFiles { files, .. } if files.len() == 1
        ));
    }

    #[tokio::test]
    async fn upload_node_not_found() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(json!({"root": {"nodeId": 1}}))),
            Ok(OpResult::new(json!({"nodeId": 0}))), // node not found
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Upload {
                session: sid,
                tab: TabId(0),
                selector: "input#missing".into(),
                files: vec!["/tmp/test.txt".into()],
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "element_not_found"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn wait_navigation_returns_when_url_changes() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(
                json!({"result": {"value": "https://before.test"}}),
            )),
            Ok(OpResult::new(json!({
                "result": {"value": {"url": "https://after.test", "ready": "loading"}}
            }))),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::WaitNavigation {
                session: sid,
                tab: TabId(0),
                timeout_ms: Some(50),
            },
        )
        .await;

        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["navigated"], true);
                assert_eq!(data["url"], "https://after.test");
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_navigation_returns_when_ready_state_is_complete() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(
                json!({"result": {"value": "https://same.test"}}),
            )),
            Ok(OpResult::new(json!({
                "result": {"value": {"url": "https://same.test", "ready": "complete"}}
            }))),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::WaitNavigation {
                session: sid,
                tab: TabId(0),
                timeout_ms: Some(50),
            },
        )
        .await;

        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["readyState"], "complete");
                assert_eq!(data["url"], "https://same.test");
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_navigation_times_out_when_url_never_changes() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(
                json!({"result": {"value": "https://same.test"}}),
            )),
            Ok(OpResult::new(json!({
                "result": {"value": {"url": "https://same.test", "ready": "loading"}}
            }))),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::WaitNavigation {
                session: sid,
                tab: TabId(0),
                timeout_ms: Some(0),
            },
        )
        .await;

        match result {
            ActionResult::Retryable { reason, hint } => {
                assert_eq!(reason, "navigation_timeout");
                assert!(hint.contains("0ms"));
            }
            other => panic!("expected Retryable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_network_idle_returns_when_pending_requests_drop_to_zero() {
        let mut backend = MockBackendSession::new(vec![
            Ok(OpResult::new(
                json!({"result": {"value": {"pending": 2, "now": 10}}}),
            )),
            Ok(OpResult::new(
                json!({"result": {"value": {"pending": 0, "now": 20}}}),
            )),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::WaitNetworkIdle {
                session: sid,
                tab: TabId(0),
                timeout_ms: Some(250),
                idle_time_ms: Some(100),
            },
        )
        .await;

        match result {
            ActionResult::Ok { data } => assert_eq!(data["network_idle"], true),
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_network_idle_times_out_when_pending_requests_remain() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": {"pending": 1, "now": 10}}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::WaitNetworkIdle {
                session: sid,
                tab: TabId(0),
                timeout_ms: Some(0),
                idle_time_ms: Some(100),
            },
        )
        .await;

        match result {
            ActionResult::Retryable { reason, hint } => {
                assert_eq!(reason, "network_idle_timeout");
                assert!(hint.contains("0ms"));
            }
            other => panic!("expected Retryable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_condition_returns_value_for_truthy_expression() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": {"ready": true}}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::WaitCondition {
                session: sid,
                tab: TabId(0),
                expression: "window.__ready".into(),
                timeout_ms: Some(50),
            },
        )
        .await;

        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["condition_met"], true);
                assert_eq!(data["value"]["ready"], true);
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_condition_times_out_for_falsy_value() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": ""}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::WaitCondition {
                session: sid,
                tab: TabId(0),
                expression: "window.__ready".into(),
                timeout_ms: Some(0),
            },
        )
        .await;

        match result {
            ActionResult::Retryable { reason, hint } => {
                assert_eq!(reason, "condition_timeout");
                assert!(hint.contains("window.__ready"));
            }
            other => panic!("expected Retryable, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Observation handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn title_returns_document_title() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": "My Page Title"}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Title {
                session: sid,
                tab: TabId(0),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => assert_eq!(data["title"], "My Page Title"),
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn url_returns_location_href() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": "https://current.url/page"}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Url {
                session: sid,
                tab: TabId(0),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => assert_eq!(data["url"], "https://current.url/page"),
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn eval_with_exception() {
        let mut backend = MockBackendSession::new(vec![
            // log capture init
            Ok(OpResult::new(json!({"result": {"value": true}}))),
            // eval with exception
            Ok(OpResult::new(json!({
                "exceptionDetails": {
                    "exception": {"description": "ReferenceError: foo is not defined"},
                    "text": "Uncaught"
                }
            }))),
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
                expression: "foo.bar".into(),
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "eval_error"),
            _ => panic!("expected Fatal"),
        }
    }

    // -----------------------------------------------------------------------
    // Data handler tests — storage
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn storage_list_returns_items() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": [
                {"key": "key1", "value": "value1"},
                {"key": "key2", "value": "value2"},
            ]}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::StorageList {
                session: sid,
                tab: TabId(0),
                kind: StorageKind::Local,
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                let items = data["items"].as_array().unwrap();
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], json!({"key": "key1", "value": "value1"}));
                assert_eq!(data["storage"], "local");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn storage_get_returns_value() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::new(
            json!({"result": {"value": "stored_val"}}),
        ))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::StorageGet {
                session: sid,
                tab: TabId(0),
                kind: StorageKind::Session,
                key: "mykey".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(
                    data["item"],
                    json!({
                        "key": "mykey",
                        "value": "stored_val",
                    })
                );
                assert_eq!(data["storage"], "session");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn storage_set_sends_evaluate() {
        let mut backend = MockBackendSession::new(vec![Ok(OpResult::null())]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::StorageSet {
                session: sid,
                tab: TabId(0),
                kind: StorageKind::Local,
                key: "token".into(),
                value: "abc123".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["storage"], "local");
                assert_eq!(data["action"], "set");
                assert_eq!(data["affected"], 1);
            }
            _ => panic!("expected Ok"),
        }
        assert!(matches!(&backend.ops()[0], BackendOp::Evaluate { .. }));
    }

    #[tokio::test]
    async fn storage_delete_sends_evaluate() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": 1}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::StorageDelete {
                session: sid,
                tab: TabId(0),
                kind: StorageKind::Local,
                key: "token".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["storage"], "local");
                assert_eq!(data["action"], "delete");
                assert_eq!(data["affected"], 1);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn storage_clear_sends_evaluate() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"result": {"value": 2}})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::StorageClear {
                session: sid,
                tab: TabId(0),
                kind: StorageKind::Session,
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["storage"], "session");
                assert_eq!(data["action"], "clear");
                assert_eq!(data["affected"], 2);
            }
            _ => panic!("expected Ok"),
        }
    }

    // -----------------------------------------------------------------------
    // Data handler tests — cookies additional
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn cookies_set_sends_set_cookie_op() {
        let mut backend = MockBackendSession::new(vec![
            // hostname eval
            Ok(OpResult::new(json!({"result": {"value": "example.com"}}))),
            // SetCookie
            Ok(OpResult::null()),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::CookiesSet {
                session: sid,
                name: "session".into(),
                value: "xyz".into(),
                domain: None,
                path: None,
                secure: Some(true),
                http_only: None,
                same_site: None,
                expires: None,
            },
        )
        .await;

        assert!(result.is_ok());
        match &backend.ops()[1] {
            BackendOp::SetCookie {
                name,
                value,
                domain,
                secure,
                ..
            } => {
                assert_eq!(name, "session");
                assert_eq!(value, "xyz");
                assert_eq!(domain, ".example.com");
                assert_eq!(*secure, Some(true));
            }
            other => panic!("expected SetCookie, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cookies_delete_sends_delete_cookies_op() {
        let mut backend = MockBackendSession::new(vec![
            // hostname eval
            Ok(OpResult::new(json!({"result": {"value": "site.com"}}))),
            // DeleteCookies
            Ok(OpResult::null()),
        ]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::CookiesDelete {
                session: sid,
                name: "tracking".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["action"], "delete");
                assert_eq!(data["affected"], 1);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn cookies_no_tabs_returns_error() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = Registries::new(); // empty — no tabs
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::CookiesList {
                session: sid,
                domain: None,
            },
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "no_tabs"),
            _ => panic!("expected Fatal"),
        }
    }

    // -----------------------------------------------------------------------
    // Dispatch edge cases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn close_session_returns_ok() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = Registries::new();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Close { session: sid },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn restart_session_returns_ok() {
        let mut backend = MockBackendSession::new(vec![]);
        let mut regs = Registries::new();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::RestartSession { session: sid },
        )
        .await;

        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => assert_eq!(data["restarting"], true),
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn open_alias_dispatches_to_new_tab() {
        let mut backend =
            MockBackendSession::new(vec![Ok(OpResult::new(json!({"targetId": "ALIAS_T"})))]);
        let mut regs = make_regs_with_tab();
        let sid = SessionId::new_unchecked("local-1");

        let result = handle_action(
            sid.clone(),
            &mut backend,
            &mut regs,
            Action::Open {
                session: sid,
                tab: TabId(0),
                url: "https://via-open.com".into(),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(regs.tabs.len(), 2);
    }

    // -- extract_eval_value tests --

    #[test]
    fn extract_eval_value_standard_cdp_format() {
        let cdp = json!({"result": {"type": "string", "value": "hello"}});
        assert_eq!(extract_eval_value(&cdp), json!("hello"));
    }

    #[test]
    fn extract_eval_value_number() {
        let cdp = json!({"result": {"type": "number", "value": 42}});
        assert_eq!(extract_eval_value(&cdp), json!(42));
    }

    #[test]
    fn extract_eval_value_null() {
        let cdp = json!({"result": {"type": "object", "subtype": "null", "value": null}});
        assert_eq!(extract_eval_value(&cdp), json!(null));
    }

    #[test]
    fn extract_eval_value_boolean() {
        let cdp = json!({"result": {"type": "boolean", "value": true}});
        assert_eq!(extract_eval_value(&cdp), json!(true));
    }

    #[test]
    fn extract_eval_value_missing_result_returns_whole() {
        let cdp = json!({"something": "else"});
        assert_eq!(extract_eval_value(&cdp), cdp);
    }

    #[test]
    fn extract_eval_value_missing_value_returns_whole() {
        let cdp = json!({"result": {"type": "undefined"}});
        assert_eq!(extract_eval_value(&cdp), cdp);
    }

    #[test]
    fn extract_eval_value_object_value() {
        let cdp = json!({"result": {"type": "object", "value": {"key": "val"}}});
        assert_eq!(extract_eval_value(&cdp), json!({"key": "val"}));
    }

    // -- map_key_name tests --

    #[test]
    fn map_key_name_enter() {
        let (key, text) = map_key_name("enter");
        assert_eq!(key, "Enter");
        assert_eq!(text, "\r");
    }

    #[test]
    fn map_key_name_return_alias() {
        let (key, _) = map_key_name("Return");
        assert_eq!(key, "Enter");
    }

    #[test]
    fn map_key_name_tab() {
        let (key, text) = map_key_name("Tab");
        assert_eq!(key, "Tab");
        assert_eq!(text, "\t");
    }

    #[test]
    fn map_key_name_escape_aliases() {
        assert_eq!(map_key_name("escape").0, "Escape");
        assert_eq!(map_key_name("esc").0, "Escape");
        assert_eq!(map_key_name("ESC").0, "Escape");
    }

    #[test]
    fn map_key_name_backspace() {
        let (key, text) = map_key_name("Backspace");
        assert_eq!(key, "Backspace");
        assert_eq!(text, "");
    }

    #[test]
    fn map_key_name_arrows() {
        assert_eq!(map_key_name("up").0, "ArrowUp");
        assert_eq!(map_key_name("ArrowUp").0, "ArrowUp");
        assert_eq!(map_key_name("down").0, "ArrowDown");
        assert_eq!(map_key_name("left").0, "ArrowLeft");
        assert_eq!(map_key_name("right").0, "ArrowRight");
    }

    #[test]
    fn map_key_name_space() {
        let (key, text) = map_key_name("space");
        assert_eq!(key, " ");
        assert_eq!(text, " ");
    }

    #[test]
    fn map_key_name_home_end() {
        assert_eq!(map_key_name("home").0, "Home");
        assert_eq!(map_key_name("end").0, "End");
    }

    #[test]
    fn map_key_name_page_keys() {
        assert_eq!(map_key_name("pageup").0, "PageUp");
        assert_eq!(map_key_name("pagedown").0, "PageDown");
    }

    #[test]
    fn map_key_name_delete() {
        assert_eq!(map_key_name("delete").0, "Delete");
    }

    #[test]
    fn map_key_name_unknown_passthrough() {
        let (key, text) = map_key_name("a");
        assert_eq!(key, "a");
        assert_eq!(text, "a");
    }

    #[test]
    fn map_key_name_case_insensitive() {
        assert_eq!(map_key_name("ENTER").0, "Enter");
        assert_eq!(map_key_name("Tab").0, "Tab");
        assert_eq!(map_key_name("BACKSPACE").0, "Backspace");
    }

    // -- element_not_found tests --

    #[test]
    fn element_not_found_includes_selector() {
        let result = element_not_found("#missing");
        assert!(!result.is_ok());
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["code"], "element_not_found");
        assert!(json["message"].as_str().unwrap().contains("#missing"));
    }

    #[test]
    fn element_not_found_includes_hint() {
        let result = element_not_found("div.test");
        let json = serde_json::to_value(&result).unwrap();
        assert!(json["hint"].as_str().unwrap().contains("snapshot"));
    }
}
