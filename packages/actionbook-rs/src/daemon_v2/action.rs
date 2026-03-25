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

use super::types::{Mode, SessionId, TabId, WindowId};

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
    },

    /// Close an existing session and its browser.
    CloseSession {
        session: SessionId,
    },

    /// List all active sessions.
    ListSessions,

    /// Get detailed status of a session.
    SessionStatus {
        session: SessionId,
    },

    // =======================================================================
    // Session-level commands — require session
    // =======================================================================
    /// List all tabs in a session.
    ListTabs {
        session: SessionId,
    },

    /// List all windows in a session.
    ListWindows {
        session: SessionId,
    },

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
    CloseTab {
        session: SessionId,
        tab: TabId,
    },

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
    Back {
        session: SessionId,
        tab: TabId,
    },

    /// Navigate forward in history.
    Forward {
        session: SessionId,
        tab: TabId,
    },

    /// Reload the current page.
    Reload {
        session: SessionId,
        tab: TabId,
    },

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
    Close {
        session: SessionId,
    },

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
    },
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
            | Action::Text { session, .. } => Some(*session),
        }
    }
}

fn default_mode() -> Mode {
    Mode::Local
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
            session: SessionId(0),
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
                assert_eq!(session, SessionId(0));
                assert_eq!(tab, TabId(1));
                assert_eq!(url, "https://example.com");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn click_round_trip() {
        let action = Action::Click {
            session: SessionId(2),
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
        let json = r#"{"type":"Snapshot","session":0,"tab":0}"#;
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
            session: SessionId(0),
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
            session: SessionId(1),
            tab: TabId(2),
            selector: "input[name=q]".into(),
            text: "hello world".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        match decoded {
            Action::Type {
                selector, text, ..
            } => {
                assert_eq!(selector, "input[name=q]");
                assert_eq!(text, "hello world");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn wait_element_with_timeout() {
        let action = Action::WaitElement {
            session: SessionId(0),
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
