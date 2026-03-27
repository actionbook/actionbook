use serde::{Deserialize, Serialize};

use crate::types::Mode;

/// CLI → Daemon action protocol. Each variant maps 1:1 to a CLI command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    // ── Global (no session required) ────────────────────────────
    StartSession {
        mode: Mode,
        headless: bool,
        profile: Option<String>,
        open_url: Option<String>,
        cdp_endpoint: Option<String>,
        set_session_id: Option<String>,
    },
    ListSessions,

    // ── Session level (--session required) ──────────────────────
    SessionStatus {
        session_id: String,
    },
    Close {
        session_id: String,
    },
    Restart {
        session_id: String,
    },

    // ── Tab level (--session + --tab required) ──────────────────
    Goto {
        session_id: String,
        tab_id: String,
        url: String,
    },
    NewTab {
        session_id: String,
        url: String,
        new_window: bool,
    },
    CloseTab {
        session_id: String,
        tab_id: String,
    },
    ListTabs {
        session_id: String,
    },
    Snapshot {
        session_id: String,
        tab_id: String,
    },
    Eval {
        session_id: String,
        tab_id: String,
        expression: String,
    },
}

impl Action {
    /// Normalized command name for the JSON envelope.
    pub fn command_name(&self) -> &str {
        match self {
            Action::StartSession { .. } => "browser.start",
            Action::ListSessions => "browser.list-sessions",
            Action::SessionStatus { .. } => "browser.status",
            Action::Close { .. } => "browser.close",
            Action::Restart { .. } => "browser.restart",
            Action::Goto { .. } => "browser.goto",
            Action::NewTab { .. } => "browser.new-tab",
            Action::CloseTab { .. } => "browser.close-tab",
            Action::ListTabs { .. } => "browser.list-tabs",
            Action::Snapshot { .. } => "browser.snapshot",
            Action::Eval { .. } => "browser.eval",
        }
    }
}
