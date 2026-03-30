//! The typed Action enum — the CLI-to-daemon protocol (Layer 1).
//!
//! Each variant maps 1:1 to a CLI subcommand. Actions are the *only* way
//! clients communicate intent to the daemon; the daemon compiles them into
//! [`BackendOp`](super::backend_op::BackendOp) sequences internally.
//!
//! Actions are classified by addressing level:
//! - **Global**: no session/tab required (e.g. `StartSession`, `ListSessions`)
//! - **Session**: requires `session` (e.g. `ListTabs`, `Close`)
//! - **Tab**: requires `session` + `tab` (e.g. `Goto`, `Click`, `Snapshot`)

use serde::{Deserialize, Serialize};

use super::types::{
    Mode, QueryCardinality, QueryMode, SameSite, SessionId, StorageKind, TabId, WindowId,
};

/// A typed command sent from CLI (or MCP/AI SDK client) to the daemon.
///
/// Serialized with `#[serde(tag = "type")]` so each variant produces
/// `{ "type": "StartSession", ... }` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    // =======================================================================
    // Global commands — no session/tab required
    // =======================================================================
    /// Create a new browser session.
    StartSession {
        /// Browser connection mode (defaults to Local).
        #[serde(default = "default_mode")]
        mode: Mode,
        /// Optional profile name for configuration lookup.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<String>,
        /// Launch in headless mode (Local only).
        #[serde(default)]
        headless: bool,
        /// URL to open immediately after session start.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        open_url: Option<String>,
        /// CDP endpoint for Cloud mode.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cdp_endpoint: Option<String>,
        /// Optional WS auth headers (Cloud mode).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ws_headers: Option<std::collections::HashMap<String, String>>,
        /// Optional explicit session ID (validated against `^[a-z][a-z0-9-]{0,63}$`).
        /// When provided, the daemon uses this ID instead of auto-generating one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set_session_id: Option<String>,
    },

    /// Close an existing session and its browser.
    CloseSession { session: SessionId },

    /// List all active sessions.
    ListSessions,

    /// Get detailed status of a session.
    SessionStatus { session: SessionId },

    // =======================================================================
    // Session-level commands — require session
    // =======================================================================
    /// List all tabs in a session.
    ListTabs { session: SessionId },

    /// List all windows in a session.
    ListWindows { session: SessionId },

    /// Open a new tab (optionally in a specific or new window).
    NewTab {
        session: SessionId,
        /// URL to navigate the new tab to.
        url: String,
        /// If true, open in a new window.
        #[serde(default)]
        new_window: bool,
        /// Open in a specific existing window.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<WindowId>,
    },

    /// Close a specific tab.
    CloseTab { session: SessionId, tab: TabId },

    // =======================================================================
    // Tab-level commands — require session + tab
    // =======================================================================
    /// Navigate to a URL.
    Goto {
        session: SessionId,
        tab: TabId,
        url: String,
    },

    /// Navigate back in history.
    Back { session: SessionId, tab: TabId },

    /// Navigate forward in history.
    Forward { session: SessionId, tab: TabId },

    /// Reload the current page.
    Reload { session: SessionId, tab: TabId },

    /// Open a URL in a new tab within the same session (convenience action).
    Open {
        session: SessionId,
        tab: TabId,
        url: String,
    },

    /// Capture an accessibility-tree snapshot of the page.
    Snapshot {
        session: SessionId,
        tab: TabId,
        /// Include only interactive elements.
        #[serde(default)]
        interactive: bool,
        /// Use compact output format.
        #[serde(default)]
        compact: bool,
        /// Show cursor overlay in snapshot.
        #[serde(default)]
        cursor: bool,
        /// Maximum depth of the accessibility tree.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        depth: Option<u32>,
        /// Restrict snapshot to elements matching this selector.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selector: Option<String>,
    },

    /// Take a screenshot (PNG).
    Screenshot {
        session: SessionId,
        tab: TabId,
        /// If true, capture the full scrollable page.
        #[serde(default)]
        full_page: bool,
    },

    /// Close the session's browser entirely.
    Close { session: SessionId },

    /// Click an element by selector or at coordinates.
    Click {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Mouse button: "left" (default), "right", "middle".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        button: Option<String>,
        /// Number of clicks (1 = single, 2 = double).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
        /// If true, extract href from element and open in a new tab.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        new_tab: bool,
        /// Direct coordinates (x, y) instead of selector-based targeting.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        coordinates: Option<(f64, f64)>,
    },

    /// Type text character by character (with key events).
    Type {
        session: SessionId,
        tab: TabId,
        /// CSS selector of the target element.
        selector: String,
        /// Text to type.
        text: String,
    },

    /// Fill an input field (sets value directly, then dispatches input event).
    Fill {
        session: SessionId,
        tab: TabId,
        selector: String,
        value: String,
    },

    /// Evaluate a JavaScript expression in the page context.
    Eval {
        session: SessionId,
        tab: TabId,
        /// JavaScript expression to evaluate.
        expression: String,
    },

    /// Wait for an element to appear in the DOM.
    WaitElement {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Timeout in milliseconds (default: 30000).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },

    /// Get the outer HTML of an element (or the full page if no selector).
    Html {
        session: SessionId,
        tab: TabId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selector: Option<String>,
    },

    /// Get the inner text of an element (or the full page if no selector).
    Text {
        session: SessionId,
        tab: TabId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selector: Option<String>,
        /// Text extraction mode: "raw" (default) or "readability".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },

    // =======================================================================
    // Observation actions — Tab-level (require session + tab)
    // =======================================================================
    /// Print the page to PDF and save to a file.
    Pdf {
        session: SessionId,
        tab: TabId,
        /// Output file path for the PDF.
        path: String,
    },

    /// Get the page title.
    Title { session: SessionId, tab: TabId },

    /// Get the current page URL.
    Url { session: SessionId, tab: TabId },

    /// Get the value of an input element.
    Value {
        session: SessionId,
        tab: TabId,
        selector: String,
    },

    /// Get a specific attribute of an element.
    Attr {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Attribute name to retrieve.
        name: String,
    },

    /// Get all attributes of an element.
    Attrs {
        session: SessionId,
        tab: TabId,
        selector: String,
    },

    /// Get a human-readable description of an element (tag, role, text, etc.).
    Describe {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Include nearby elements (parent, siblings) in the description.
        #[serde(default)]
        nearby: bool,
    },

    /// Get the interactive state of an element (visible, enabled, checked, etc.).
    State {
        session: SessionId,
        tab: TabId,
        selector: String,
    },

    /// Get the bounding box of an element.
    #[serde(rename = "Box")]
    Box_ {
        session: SessionId,
        tab: TabId,
        selector: String,
    },

    /// Get computed styles of an element.
    Styles {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Specific CSS property names to retrieve (default: all computed styles).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
    },

    /// Get the viewport dimensions.
    Viewport { session: SessionId, tab: TabId },

    /// Query elements matching a selector with cardinality constraint.
    Query {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Query mode: css, xpath, or text.
        #[serde(default = "default_query_mode")]
        mode: QueryMode,
        /// Cardinality mode: one, all, count, nth.
        #[serde(default = "default_query_cardinality")]
        cardinality: QueryCardinality,
        /// 1-based index for nth mode.
        #[serde(default)]
        nth_index: Option<u32>,
    },

    /// Inspect the element at a specific point on the page.
    InspectPoint {
        session: SessionId,
        tab: TabId,
        x: f64,
        y: f64,
        /// How many parent levels to include.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_depth: Option<u32>,
    },

    /// Get console log messages.
    LogsConsole {
        session: SessionId,
        tab: TabId,
        /// Filter by log level.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<String>,
        /// Return only the last N log entries.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<u32>,
        /// Return entries since this timestamp/marker.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
        /// Clear the log buffer after reading.
        #[serde(default)]
        clear: bool,
    },

    /// Get error log messages.
    LogsErrors {
        session: SessionId,
        tab: TabId,
        /// Filter by error source.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        /// Return only the last N error entries.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<u32>,
        /// Return entries since this timestamp/marker.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
        /// Clear the error buffer after reading.
        #[serde(default)]
        clear: bool,
    },

    // =======================================================================
    // Data actions — Session-level (require session)
    // =======================================================================
    /// List all cookies for the session.
    CookiesList {
        session: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        domain: Option<String>,
    },

    /// Get a specific cookie by name.
    CookiesGet { session: SessionId, name: String },

    /// Set a cookie.
    CookiesSet {
        session: SessionId,
        name: String,
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        domain: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secure: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        http_only: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        same_site: Option<SameSite>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires: Option<f64>,
    },

    /// Delete a cookie by name.
    CookiesDelete { session: SessionId, name: String },

    /// Clear all cookies for the session.
    CookiesClear {
        session: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        domain: Option<String>,
    },

    /// List all keys in web storage.
    StorageList {
        session: SessionId,
        tab: TabId,
        kind: StorageKind,
    },

    /// Get a value from web storage.
    StorageGet {
        session: SessionId,
        tab: TabId,
        kind: StorageKind,
        key: String,
    },

    /// Set a value in web storage.
    StorageSet {
        session: SessionId,
        tab: TabId,
        kind: StorageKind,
        key: String,
        value: String,
    },

    /// Delete a key from web storage.
    StorageDelete {
        session: SessionId,
        tab: TabId,
        kind: StorageKind,
        key: String,
    },

    /// Clear all web storage of the specified kind.
    StorageClear {
        session: SessionId,
        tab: TabId,
        kind: StorageKind,
    },

    // =======================================================================
    // Interaction actions — Tab-level (require session + tab)
    // =======================================================================
    /// Select a value from a dropdown (`<select>`) element.
    Select {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Value to select.
        value: String,
        /// If true, match by visible text instead of value attribute.
        #[serde(default)]
        by_text: bool,
    },

    /// Hover over an element.
    Hover {
        session: SessionId,
        tab: TabId,
        selector: String,
    },

    /// Focus an element.
    Focus {
        session: SessionId,
        tab: TabId,
        selector: String,
    },

    /// Press a keyboard key or chord (e.g. "Enter", "Control+A").
    Press {
        session: SessionId,
        tab: TabId,
        /// Key or chord string (e.g. "Enter", "Control+A", "Shift+Tab").
        key_or_chord: String,
    },

    /// Drag an element to another element or coordinates.
    Drag {
        session: SessionId,
        tab: TabId,
        /// Selector of the element to drag from.
        from_selector: String,
        /// Selector of the drop target (empty if to_coordinates is set).
        to_selector: String,
        /// Mouse button: "left" (default), "right", "middle".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        button: Option<String>,
        /// Direct coordinates (x, y) for the drop target.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_coordinates: Option<(f64, f64)>,
    },

    /// Upload files to a file input element.
    Upload {
        session: SessionId,
        tab: TabId,
        selector: String,
        /// Absolute file paths to upload.
        files: Vec<String>,
    },

    /// Scroll the page or an element.
    Scroll {
        session: SessionId,
        tab: TabId,
        /// Direction: "up", "down", "left", "right".
        direction: String,
        /// Amount to scroll in pixels (default: 300).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount: Option<i32>,
        /// Optional selector to scroll within (defaults to page).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selector: Option<String>,
        /// Optional CSS selector of the container element to scroll within.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Alignment for into-view scrolling: start, center, end, nearest.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<String>,
    },

    /// Move the mouse to absolute coordinates.
    MouseMove {
        session: SessionId,
        tab: TabId,
        x: f64,
        y: f64,
    },

    /// Get the current cursor position.
    CursorPosition { session: SessionId, tab: TabId },

    // =======================================================================
    // Waiting actions — Tab-level (require session + tab)
    // =======================================================================
    /// Wait for a navigation to complete.
    WaitNavigation {
        session: SessionId,
        tab: TabId,
        /// Timeout in milliseconds (default: 30000).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },

    /// Wait for network to become idle.
    WaitNetworkIdle {
        session: SessionId,
        tab: TabId,
        /// Timeout in milliseconds (default: 30000).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
        /// Milliseconds of idle time to consider "idle" (default: 500).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        idle_time_ms: Option<u64>,
    },

    /// Wait for a JS expression to evaluate to truthy.
    WaitCondition {
        session: SessionId,
        tab: TabId,
        /// JavaScript expression that should return a truthy value.
        expression: String,
        /// Timeout in milliseconds (default: 30000).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },

    // =======================================================================
    // Session management
    // =======================================================================
    /// Close and re-start a session with the same profile/mode.
    RestartSession { session: SessionId },
}

impl Action {
    /// Extract the session ID if this action targets a specific session.
    ///
    /// Returns `None` for global commands (`StartSession`, `ListSessions`).
    pub fn session_id(&self) -> Option<SessionId> {
        match self {
            // Global — no session
            Action::StartSession { .. } | Action::ListSessions => None,

            // Session-level
            Action::CloseSession { session, .. }
            | Action::SessionStatus { session, .. }
            | Action::ListTabs { session, .. }
            | Action::ListWindows { session, .. }
            | Action::NewTab { session, .. }
            | Action::CloseTab { session, .. }
            | Action::Close { session, .. }

            // Tab-level
            | Action::Goto { session, .. }
            | Action::Back { session, .. }
            | Action::Forward { session, .. }
            | Action::Reload { session, .. }
            | Action::Open { session, .. }
            | Action::Snapshot { session, .. }
            | Action::Screenshot { session, .. }
            | Action::Click { session, .. }
            | Action::Type { session, .. }
            | Action::Fill { session, .. }
            | Action::Eval { session, .. }
            | Action::WaitElement { session, .. }
            | Action::Html { session, .. }
            | Action::Text { session, .. }

            // Observation (tab-level)
            | Action::Pdf { session, .. }
            | Action::Title { session, .. }
            | Action::Url { session, .. }
            | Action::Value { session, .. }
            | Action::Attr { session, .. }
            | Action::Attrs { session, .. }
            | Action::Describe { session, .. }
            | Action::State { session, .. }
            | Action::Box_ { session, .. }
            | Action::Styles { session, .. }
            | Action::Viewport { session, .. }
            | Action::Query { session, .. }
            | Action::InspectPoint { session, .. }
            | Action::LogsConsole { session, .. }
            | Action::LogsErrors { session, .. }

            // Data (session-level)
            | Action::CookiesList { session, .. }
            | Action::CookiesGet { session, .. }
            | Action::CookiesSet { session, .. }
            | Action::CookiesDelete { session, .. }
            | Action::CookiesClear { session, .. }

            // Data (tab-level storage)
            | Action::StorageList { session, .. }
            | Action::StorageGet { session, .. }
            | Action::StorageSet { session, .. }
            | Action::StorageDelete { session, .. }
            | Action::StorageClear { session, .. }

            // Interaction (tab-level)
            | Action::Select { session, .. }
            | Action::Hover { session, .. }
            | Action::Focus { session, .. }
            | Action::Press { session, .. }
            | Action::Drag { session, .. }
            | Action::Upload { session, .. }
            | Action::Scroll { session, .. }
            | Action::MouseMove { session, .. }
            | Action::CursorPosition { session, .. }

            // Waiting (tab-level)
            | Action::WaitNavigation { session, .. }
            | Action::WaitNetworkIdle { session, .. }
            | Action::WaitCondition { session, .. }

            // Session management
            | Action::RestartSession { session, .. } => Some(session.clone()),
        }
    }
}

fn default_mode() -> Mode {
    Mode::Local
}

fn default_query_mode() -> QueryMode {
    QueryMode::Css
}

fn default_query_cardinality() -> QueryCardinality {
    QueryCardinality::All
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_session_round_trip() {
        let action = Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: true,
            open_url: Some("https://example.com".into()),
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""type":"StartSession""#));
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::StartSession {
                mode,
                headless,
                open_url,
                ..
            } => {
                assert_eq!(mode, Mode::Local);
                assert!(headless);
                assert_eq!(open_url.as_deref(), Some("https://example.com"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn goto_round_trip() {
        let action = Action::Goto {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(2),
            url: "https://example.com".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""type":"Goto""#));
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Goto {
                session, tab, url, ..
            } => {
                assert_eq!(session, SessionId::new_unchecked("local-1"));
                assert_eq!(tab, TabId(2));
                assert_eq!(url, "https://example.com");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn click_round_trip() {
        let action = Action::Click {
            session: SessionId::new_unchecked("local-3"),
            tab: TabId(1),
            selector: "#submit".into(),
            button: Some("right".into()),
            count: Some(2),
            new_tab: false,
            coordinates: None,
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Click {
                selector,
                button,
                count,
                ..
            } => {
                assert_eq!(selector, "#submit");
                assert_eq!(button.as_deref(), Some("right"));
                assert_eq!(count, Some(2));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn list_sessions_round_trip() {
        let action = Action::ListSessions;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, r#"{"type":"ListSessions"}"#);
        let decoded: Action = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, Action::ListSessions));
    }

    #[test]
    fn snapshot_defaults() {
        let json = r#"{"type":"Snapshot","session":"local-1","tab":0}"#;
        let action: Action = serde_json::from_str(json).unwrap();
        match action {
            Action::Snapshot {
                interactive,
                compact,
                ..
            } => {
                assert!(!interactive);
                assert!(!compact);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn eval_round_trip() {
        let action = Action::Eval {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
            expression: "document.title".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Eval { expression, .. } => assert_eq!(expression, "document.title"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn type_action_round_trip() {
        let action = Action::Type {
            session: SessionId::new_unchecked("local-2"),
            tab: TabId(3),
            selector: "input[name=q]".into(),
            text: "hello world".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Type { selector, text, .. } => {
                assert_eq!(selector, "input[name=q]");
                assert_eq!(text, "hello world");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn wait_element_with_timeout() {
        let action = Action::WaitElement {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
            selector: ".loaded".into(),
            timeout_ms: Some(5000),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::WaitElement {
                selector,
                timeout_ms,
                ..
            } => {
                assert_eq!(selector, ".loaded");
                assert_eq!(timeout_ms, Some(5000));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn session_id_returns_none_for_global_actions() {
        assert!(Action::ListSessions.session_id().is_none());
        let start = Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        };
        assert!(start.session_id().is_none());
    }

    #[test]
    fn session_id_returns_some_for_session_actions() {
        let session = SessionId::new_unchecked("local-1");
        let tab = TabId(1);

        let goto = Action::Goto {
            session: session.clone(),
            tab,
            url: "https://example.com".into(),
        };
        assert_eq!(goto.session_id().unwrap().as_str(), "local-1");

        let close = Action::CloseSession {
            session: session.clone(),
        };
        assert_eq!(close.session_id().unwrap().as_str(), "local-1");

        let list_tabs = Action::ListTabs {
            session: session.clone(),
        };
        assert_eq!(list_tabs.session_id().unwrap().as_str(), "local-1");
    }

    #[test]
    fn screenshot_round_trip() {
        let action = Action::Screenshot {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
            full_page: true,
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Screenshot { full_page, .. } => assert!(full_page),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn new_tab_round_trip() {
        let action = Action::NewTab {
            session: SessionId::new_unchecked("local-1"),
            url: "https://example.com".into(),
            new_window: false,
            window: None,
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::NewTab { url, .. } => assert_eq!(url, "https://example.com"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn session_id_for_all_tab_level_actions() {
        let s = SessionId::new_unchecked("local-5");
        let tab = TabId(3);

        let actions: Vec<Action> = vec![
            Action::Back {
                session: s.clone(),
                tab,
            },
            Action::Forward {
                session: s.clone(),
                tab,
            },
            Action::Reload {
                session: s.clone(),
                tab,
            },
            Action::Open {
                session: s.clone(),
                tab,
                url: "https://example.com".into(),
            },
            Action::Html {
                session: s.clone(),
                tab,
                selector: None,
            },
            Action::Text {
                session: s.clone(),
                tab,
                selector: None,
                mode: None,
            },
            Action::Pdf {
                session: s.clone(),
                tab,
                path: "/tmp/out.pdf".into(),
            },
            Action::Title {
                session: s.clone(),
                tab,
            },
            Action::Url {
                session: s.clone(),
                tab,
            },
            Action::Value {
                session: s.clone(),
                tab,
                selector: "#field".into(),
            },
            Action::Attr {
                session: s.clone(),
                tab,
                selector: "#field".into(),
                name: "aria-label".into(),
            },
            Action::Attrs {
                session: s.clone(),
                tab,
                selector: "#field".into(),
            },
            Action::Describe {
                session: s.clone(),
                tab,
                selector: "#field".into(),
                nearby: false,
            },
            Action::State {
                session: s.clone(),
                tab,
                selector: "#field".into(),
            },
            Action::Box_ {
                session: s.clone(),
                tab,
                selector: "#field".into(),
            },
            Action::Styles {
                session: s.clone(),
                tab,
                selector: "#field".into(),
                names: vec![],
            },
            Action::Viewport {
                session: s.clone(),
                tab,
            },
            Action::Query {
                session: s.clone(),
                tab,
                selector: ".item".into(),
                mode: QueryMode::Css,
                cardinality: QueryCardinality::All,
                nth_index: None,
            },
            Action::InspectPoint {
                session: s.clone(),
                tab,
                x: 10.0,
                y: 20.0,
                parent_depth: None,
            },
            Action::LogsConsole {
                session: s.clone(),
                tab,
                level: None,
                tail: None,
                since: None,
                clear: false,
            },
            Action::LogsErrors {
                session: s.clone(),
                tab,
                source: None,
                tail: None,
                since: None,
                clear: false,
            },
            Action::CookiesList {
                session: s.clone(),
                domain: None,
            },
            Action::CookiesGet {
                session: s.clone(),
                name: "test".into(),
            },
            Action::CookiesSet {
                session: s.clone(),
                name: "x".into(),
                value: "y".into(),
                domain: None,
                path: None,
                secure: None,
                http_only: None,
                same_site: None,
                expires: None,
            },
            Action::CookiesDelete {
                session: s.clone(),
                name: "x".into(),
            },
            Action::CookiesClear {
                session: s.clone(),
                domain: None,
            },
            Action::StorageList {
                session: s.clone(),
                tab,
                kind: StorageKind::Local,
            },
            Action::StorageGet {
                session: s.clone(),
                tab,
                kind: StorageKind::Local,
                key: "k".into(),
            },
            Action::StorageSet {
                session: s.clone(),
                tab,
                kind: StorageKind::Local,
                key: "k".into(),
                value: "v".into(),
            },
            Action::StorageDelete {
                session: s.clone(),
                tab,
                kind: StorageKind::Session,
                key: "k".into(),
            },
            Action::StorageClear {
                session: s.clone(),
                tab,
                kind: StorageKind::Session,
            },
            Action::Select {
                session: s.clone(),
                tab,
                selector: "#sel".into(),
                value: "opt".into(),
                by_text: false,
            },
            Action::Hover {
                session: s.clone(),
                tab,
                selector: "#hover".into(),
            },
            Action::Focus {
                session: s.clone(),
                tab,
                selector: "#focus".into(),
            },
            Action::Press {
                session: s.clone(),
                tab,
                key_or_chord: "Enter".into(),
            },
            Action::Drag {
                session: s.clone(),
                tab,
                from_selector: "#from".into(),
                to_selector: "#to".into(),
                button: None,
                to_coordinates: None,
            },
            Action::Upload {
                session: s.clone(),
                tab,
                selector: "#file".into(),
                files: vec![],
            },
            Action::Scroll {
                session: s.clone(),
                tab,
                direction: "down".into(),
                amount: None,
                selector: None,
                container: None,
                align: None,
            },
            Action::MouseMove {
                session: s.clone(),
                tab,
                x: 10.0,
                y: 20.0,
            },
            Action::CursorPosition {
                session: s.clone(),
                tab,
            },
            Action::WaitNavigation {
                session: s.clone(),
                tab,
                timeout_ms: None,
            },
            Action::WaitNetworkIdle {
                session: s.clone(),
                tab,
                timeout_ms: None,
                idle_time_ms: None,
            },
            Action::WaitCondition {
                session: s.clone(),
                tab,
                expression: "true".into(),
                timeout_ms: None,
            },
            Action::RestartSession { session: s.clone() },
            Action::ListWindows { session: s.clone() },
            Action::CloseTab {
                session: s.clone(),
                tab,
            },
            Action::Close { session: s.clone() },
            Action::SessionStatus { session: s.clone() },
        ];

        for action in &actions {
            assert_eq!(
                action.session_id().as_ref().map(|id| id.as_str()),
                Some("local-5"),
                "session_id() failed for {action:?}"
            );
        }
    }

    #[test]
    fn fill_round_trip() {
        let action = Action::Fill {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
            selector: "#email".into(),
            value: "test@example.com".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Fill {
                selector, value, ..
            } => {
                assert_eq!(selector, "#email");
                assert_eq!(value, "test@example.com");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn cookies_set_with_all_fields_round_trip() {
        let action = Action::CookiesSet {
            session: SessionId::new_unchecked("local-1"),
            name: "my_cookie".into(),
            value: "my_value".into(),
            domain: Some(".example.com".into()),
            path: Some("/".into()),
            secure: Some(true),
            http_only: Some(false),
            same_site: Some(SameSite::Lax),
            expires: Some(1234567890.0),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::CookiesSet {
                name,
                value,
                secure,
                ..
            } => {
                assert_eq!(name, "my_cookie");
                assert_eq!(value, "my_value");
                assert_eq!(secure, Some(true));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn scroll_with_container_round_trip() {
        let action = Action::Scroll {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
            direction: "down".into(),
            amount: Some(300),
            selector: None,
            container: Some("#sidebar".into()),
            align: None,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"container\":\"#sidebar\""));
        assert!(!json.contains("align"));
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Scroll {
                direction,
                amount,
                container,
                align,
                ..
            } => {
                assert_eq!(direction, "down");
                assert_eq!(amount, Some(300));
                assert_eq!(container.as_deref(), Some("#sidebar"));
                assert!(align.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn scroll_into_view_with_align_round_trip() {
        let action = Action::Scroll {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
            direction: "into-view".into(),
            amount: None,
            selector: Some("#banner".into()),
            container: None,
            align: Some("start".into()),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""align":"start""#));
        assert!(!json.contains("container"));
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Scroll {
                direction,
                selector,
                align,
                container,
                ..
            } => {
                assert_eq!(direction, "into-view");
                assert_eq!(selector.as_deref(), Some("#banner"));
                assert_eq!(align.as_deref(), Some("start"));
                assert!(container.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }
}
