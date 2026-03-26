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

    /// Click an element by selector.
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

    /// Drag an element to another element.
    Drag {
        session: SessionId,
        tab: TabId,
        /// Selector of the element to drag from.
        from_selector: String,
        /// Selector of the element to drop onto.
        to_selector: String,
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
            tab: TabId(1),
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
                assert_eq!(tab, TabId(1));
                assert_eq!(url, "https://example.com");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn click_round_trip() {
        let action = Action::Click {
            session: SessionId::new_unchecked("local-3"),
            tab: TabId(0),
            selector: "#submit".into(),
            button: Some("right".into()),
            count: Some(2),
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
            tab: TabId(0),
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
            tab: TabId(2),
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
            tab: TabId(0),
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
}
