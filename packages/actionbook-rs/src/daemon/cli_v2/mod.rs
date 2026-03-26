//! CLI v2 thin client — arg parsing, Action construction, RPC, formatting.
//!
//! This module defines the Clap subcommands for Phase 1 browser commands.
//! The CLI is stateless: it parses args, constructs an [`Action`], sends it
//! to the daemon via [`DaemonClient`], and formats the [`ActionResult`].

mod commands;

use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use tokio::net::UnixStream;
use tracing::{debug, info};

use super::action::Action;
use super::client::{self, DaemonClient};
use super::daemon_main::DaemonConfig;
use super::formatter;
use super::types::{Mode, QueryCardinality, SessionId, StorageKind, TabId, WindowId};

use commands::{
    CliMode, CookiesCmd, LocalStorageCmd, QueryCmd, ScrollCmd, SessionStorageCmd, StorageSubCmd,
    WaitCmd,
};

/// Actionbook CLI v2 — browser automation via daemon
#[derive(Parser, Debug)]
#[command(name = "actionbook")]
pub struct CliV2 {
    /// Path to the daemon socket (default: ~/.actionbook/daemons/v2.sock)
    #[arg(long, global = true, env = "ACTIONBOOK_SOCKET")]
    socket: Option<PathBuf>,

    /// Output in JSON format
    #[arg(long, global = true)]
    pub json: bool,

    /// Profile name
    #[arg(short = 'P', long, global = true, env = "ACTIONBOOK_PROFILE")]
    pub profile: Option<String>,

    #[command(subcommand)]
    command: TopLevel,
}

#[derive(Subcommand, Debug)]
enum TopLevel {
    /// Browser session and tab management
    #[command(alias = "b")]
    Browser {
        #[command(subcommand)]
        cmd: BrowserCmd,
    },
}

#[derive(Subcommand, Debug)]
enum BrowserCmd {
    // =======================================================================
    // Global commands — no session/tab required
    // =======================================================================
    /// Start a new browser session
    Start {
        /// Browser mode
        #[arg(long, value_enum, default_value = "local")]
        mode: CliMode,
        /// Profile name for configuration
        #[arg(long, short = 'p')]
        profile: Option<String>,
        /// Launch in headless mode
        #[arg(long)]
        headless: bool,
        /// URL to open after session start
        #[arg(long)]
        open_url: Option<String>,
        /// CDP WebSocket endpoint for cloud mode (e.g. wss://cloud.example.com/browser)
        #[arg(long)]
        cdp_endpoint: Option<String>,
        /// Explicit session ID (e.g. "research-google"); daemon auto-assigns if omitted
        #[arg(long)]
        set_session_id: Option<String>,
    },

    /// List all active sessions
    ListSessions,

    // =======================================================================
    // Session-level commands — require -s
    // =======================================================================
    /// Show session status
    Status {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// List tabs in a session
    ListTabs {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// List windows in a session
    ListWindows {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// Open a URL in a new tab
    #[command(name = "new-tab", alias = "open")]
    NewTab {
        /// URL to open
        url: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Open in a new window
        #[arg(long, conflicts_with = "window")]
        new_window: bool,
        /// Open in a specific existing window
        #[arg(long, conflicts_with = "new_window")]
        window: Option<WindowId>,
    },

    /// Close a session and its browser
    Close {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// Close a specific tab
    CloseTab {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    // =======================================================================
    // Tab-level commands — require -s and -t
    // =======================================================================
    /// Navigate to a URL
    Goto {
        /// URL to navigate to
        url: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Navigate back in history
    Back {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Navigate forward in history
    Forward {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Reload the current page
    Reload {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Capture an accessibility-tree snapshot
    Snapshot {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// Only interactive elements
        #[arg(short = 'i', long)]
        interactive: bool,
        /// Compact output
        #[arg(short = 'c', long)]
        compact: bool,
        /// Show cursor overlay in snapshot
        #[arg(long)]
        cursor: bool,
        /// Maximum depth of the accessibility tree
        #[arg(long)]
        depth: Option<u32>,
        /// Restrict snapshot to elements matching this selector
        #[arg(long)]
        selector: Option<String>,
    },

    /// Take a screenshot (saves PNG to path)
    Screenshot {
        /// Output file path
        path: PathBuf,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Click an element by selector
    Click {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Type text (character by character with key events)
    Type {
        /// Text to type
        text: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// CSS selector of target element
        selector: Option<String>,
    },

    /// Fill an input field (set value directly)
    Fill {
        /// CSS selector of target element
        selector: String,
        /// Value to fill
        text: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Evaluate JavaScript in the page context
    Eval {
        /// JavaScript expression
        code: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    // =======================================================================
    // Observation commands — require -s and -t
    // =======================================================================
    /// Print page to PDF
    Pdf {
        /// Output file path
        path: PathBuf,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the page title
    Title {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the current page URL
    Url {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the outer HTML of an element
    Html {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the inner text of an element
    #[command(name = "text")]
    TextCmd {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// Text extraction mode: raw (default) or readability
        #[arg(long, value_parser = ["raw", "readability"])]
        mode: Option<String>,
    },

    /// Get the value of an input element
    Value {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get a specific attribute of an element
    Attr {
        /// CSS selector
        selector: String,
        /// Attribute name
        name: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get all attributes of an element
    Attrs {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get a description of an element (tag, role, text, etc.)
    Describe {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// Include nearby elements (parent, siblings) in the description
        #[arg(long)]
        nearby: bool,
    },

    /// Get the interactive state of an element
    State {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the bounding box of an element
    #[command(name = "box")]
    Box_ {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get computed styles of an element
    Styles {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// Specific CSS property names to retrieve (default: all computed styles)
        names: Vec<String>,
    },

    /// Get the viewport dimensions
    Viewport {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Query elements matching a selector with cardinality constraint
    #[command(subcommand)]
    Query(QueryCmd),

    /// Inspect the element at a point on the page
    InspectPoint {
        /// Point to inspect as "x,y" (e.g. "100,200")
        coords: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// How many parent levels to include
        #[arg(long)]
        parent_depth: Option<u32>,
    },

    /// Capture browser logs (console or errors)
    #[command(subcommand)]
    Logs(LogsCmd),

    /// Get console log messages (legacy flat form)
    #[command(hide = true, name = "logs-console")]
    LogsConsole {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// Filter by log level
        #[arg(long)]
        level: Option<String>,
        /// Return only the last N log entries
        #[arg(long)]
        tail: Option<u32>,
        /// Return entries since this timestamp/marker
        #[arg(long)]
        since: Option<String>,
        /// Clear the log buffer after reading
        #[arg(long)]
        clear: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Get error log messages (legacy flat form)
    #[command(hide = true, name = "logs-errors")]
    LogsErrors {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// Filter by error source
        #[arg(long)]
        source: Option<String>,
        /// Return only the last N error entries
        #[arg(long)]
        tail: Option<u32>,
        /// Return entries since this timestamp/marker
        #[arg(long)]
        since: Option<String>,
        /// Clear the error buffer after reading
        #[arg(long)]
        clear: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    // =======================================================================
    // Data commands — Cookies (require -s)
    // =======================================================================
    /// Cookie operations (list, get, set, delete, clear)
    #[command(subcommand)]
    Cookies(CookiesCmd),

    // =======================================================================
    // Data commands — Storage (require -s and -t)
    // =======================================================================
    /// Local storage operations (list, get, set, delete, clear)
    #[command(subcommand)]
    LocalStorage(LocalStorageCmd),

    /// Session storage operations (list, get, set, delete, clear)
    #[command(subcommand)]
    SessionStorage(SessionStorageCmd),

    // =======================================================================
    // Interaction commands — require -s and -t
    // =======================================================================
    /// Select a value from a dropdown element
    Select {
        /// Value to select
        value: String,
        /// CSS selector of the <select> element
        #[arg(long)]
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
        /// Match by visible text instead of value attribute
        #[arg(long)]
        by_text: bool,
    },

    /// Hover over an element
    Hover {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Focus an element
    Focus {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Press a keyboard key or chord (e.g. "Enter", "Control+A")
    Press {
        /// Key or chord (e.g. "Enter", "Control+A", "Shift+Tab")
        key: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Drag an element to another element
    Drag {
        /// Selector of the element to drag
        from: String,
        /// Selector of the drop target
        to: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Upload files to a file input element
    Upload {
        /// Absolute file paths to upload
        files: Vec<String>,
        /// CSS selector of the file input element
        #[arg(long)]
        selector: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Scroll the page or an element
    #[command(subcommand)]
    Scroll(ScrollCmd),

    /// Move the mouse to absolute coordinates (e.g. "200,300")
    MouseMove {
        /// Coordinates as "x,y"
        coords: String,
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the current cursor position
    CursorPosition {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    // =======================================================================
    // Waiting commands — require -s and -t
    // =======================================================================
    /// Wait for an element, navigation, network idle, or condition
    #[command(subcommand)]
    Wait(WaitCmd),

    // =======================================================================
    // Session management
    // =======================================================================
    /// Restart a session (close + re-start with same profile/mode)
    Restart {
        /// Session ID (e.g. local-1)
        #[arg(short = 's', long)]
        session: SessionId,
    },
}

// ---------------------------------------------------------------------------
// LogsCmd subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Subcommand)]
pub enum LogsCmd {
    /// Capture browser console logs
    Console {
        #[arg(short = 's', long)]
        session: String,
        #[arg(short = 't', long)]
        tab: String,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        tail: Option<u32>,
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        clear: bool,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Capture browser error logs
    Errors {
        #[arg(short = 's', long)]
        session: String,
        #[arg(short = 't', long)]
        tab: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        tail: Option<u32>,
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        clear: bool,
        #[arg(long)]
        json: bool,
    },
}

// ---------------------------------------------------------------------------
// Action construction — pure mapping, no logic
// ---------------------------------------------------------------------------

/// Build an Action from a CLI command.
///
/// Returns (Action, Option<PathBuf>) where the optional path is the local file
/// to write when the daemon returns binary data (e.g. screenshot PNG).
fn build_action(cmd: BrowserCmd) -> Result<(Action, Option<PathBuf>), String> {
    // Extract the screenshot output path before converting to Action.
    let screenshot_path = match &cmd {
        BrowserCmd::Screenshot { path, .. } => Some(path.clone()),
        _ => None,
    };

    let action = match cmd {
        // Global
        BrowserCmd::Start {
            mode,
            profile,
            headless,
            open_url,
            cdp_endpoint,
            set_session_id,
        } => {
            let mode: Mode = mode.into();
            // Cloud mode requires --cdp-endpoint.
            if mode == Mode::Cloud && cdp_endpoint.is_none() {
                return Err(
                    "cloud mode requires --cdp-endpoint wss://... to specify the remote browser"
                        .into(),
                );
            }
            Action::StartSession {
                mode,
                profile,
                headless,
                open_url,
                cdp_endpoint,
                ws_headers: None,
                set_session_id,
            }
        }
        BrowserCmd::ListSessions => Action::ListSessions,

        // Session
        BrowserCmd::Status { session } => Action::SessionStatus { session },
        BrowserCmd::ListTabs { session } => Action::ListTabs { session },
        BrowserCmd::ListWindows { session } => Action::ListWindows { session },
        BrowserCmd::NewTab {
            url,
            session,
            new_window,
            window,
        } => Action::NewTab {
            session,
            url,
            new_window,
            window,
        },
        BrowserCmd::Close { session } => Action::CloseSession { session },
        BrowserCmd::CloseTab { session, tab } => Action::CloseTab { session, tab },

        // Tab
        BrowserCmd::Goto { url, session, tab } => Action::Goto { session, tab, url },
        BrowserCmd::Back { session, tab } => Action::Back { session, tab },
        BrowserCmd::Forward { session, tab } => Action::Forward { session, tab },
        BrowserCmd::Reload { session, tab } => Action::Reload { session, tab },
        BrowserCmd::Snapshot {
            session,
            tab,
            interactive,
            compact,
            cursor,
            depth,
            selector,
        } => Action::Snapshot {
            session,
            tab,
            interactive,
            compact,
            cursor,
            depth,
            selector,
        },
        BrowserCmd::Screenshot {
            path: _,
            session,
            tab,
        } => Action::Screenshot {
            session,
            tab,
            full_page: false,
        },
        // NOTE: screenshot_path is extracted above and returned alongside the action.
        BrowserCmd::Click {
            selector,
            session,
            tab,
        } => Action::Click {
            session,
            tab,
            selector,
            button: None,
            count: None,
        },
        BrowserCmd::Type {
            text,
            session,
            tab,
            selector,
        } => Action::Type {
            session,
            tab,
            selector: selector.unwrap_or_default(),
            text,
        },
        BrowserCmd::Fill {
            selector,
            text,
            session,
            tab,
        } => Action::Fill {
            session,
            tab,
            selector,
            value: text,
        },
        BrowserCmd::Eval { code, session, tab } => Action::Eval {
            session,
            tab,
            expression: code,
        },

        // Observation
        BrowserCmd::Pdf { path, session, tab } => Action::Pdf {
            session,
            tab,
            path: path.to_string_lossy().to_string(),
        },
        BrowserCmd::Title { session, tab } => Action::Title { session, tab },
        BrowserCmd::Url { session, tab } => Action::Url { session, tab },
        BrowserCmd::Html {
            selector,
            session,
            tab,
        } => Action::Html {
            session,
            tab,
            selector: Some(selector),
        },
        BrowserCmd::TextCmd {
            selector,
            session,
            tab,
            mode,
        } => Action::Text {
            session,
            tab,
            selector: Some(selector),
            mode,
        },
        BrowserCmd::Value {
            selector,
            session,
            tab,
        } => Action::Value {
            session,
            tab,
            selector,
        },
        BrowserCmd::Attr {
            selector,
            name,
            session,
            tab,
        } => Action::Attr {
            session,
            tab,
            selector,
            name,
        },
        BrowserCmd::Attrs {
            selector,
            session,
            tab,
        } => Action::Attrs {
            session,
            tab,
            selector,
        },
        BrowserCmd::Describe {
            selector,
            session,
            tab,
            nearby,
        } => Action::Describe {
            session,
            tab,
            selector,
            nearby,
        },
        BrowserCmd::State {
            selector,
            session,
            tab,
        } => Action::State {
            session,
            tab,
            selector,
        },
        BrowserCmd::Box_ {
            selector,
            session,
            tab,
        } => Action::Box_ {
            session,
            tab,
            selector,
        },
        BrowserCmd::Styles {
            selector,
            session,
            tab,
            names,
        } => Action::Styles {
            session,
            tab,
            selector,
            names,
        },
        BrowserCmd::Viewport { session, tab } => Action::Viewport { session, tab },
        BrowserCmd::Query(qcmd) => match qcmd {
            QueryCmd::One {
                selector,
                session,
                tab,
                mode,
            } => Action::Query {
                session,
                tab,
                selector,
                mode: mode.into(),
                cardinality: QueryCardinality::One,
                nth_index: None,
            },
            QueryCmd::All {
                selector,
                session,
                tab,
                mode,
            } => Action::Query {
                session,
                tab,
                selector,
                mode: mode.into(),
                cardinality: QueryCardinality::All,
                nth_index: None,
            },
            QueryCmd::Count {
                selector,
                session,
                tab,
                mode,
            } => Action::Query {
                session,
                tab,
                selector,
                mode: mode.into(),
                cardinality: QueryCardinality::Count,
                nth_index: None,
            },
            QueryCmd::Nth {
                n,
                selector,
                session,
                tab,
                mode,
            } => Action::Query {
                session,
                tab,
                selector,
                mode: mode.into(),
                cardinality: QueryCardinality::Nth,
                nth_index: Some(n),
            },
        },
        BrowserCmd::InspectPoint {
            coords,
            session,
            tab,
            parent_depth,
        } => {
            let parts: Vec<&str> = coords.splitn(2, ',').collect();
            if parts.len() != 2 {
                return Err(format!(
                    "invalid coords '{}': expected format 'x,y' (e.g. '100,200')",
                    coords
                ));
            }
            let x = parts[0]
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("invalid x coordinate '{}'", parts[0].trim()))?;
            let y = parts[1]
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("invalid y coordinate '{}'", parts[1].trim()))?;
            Action::InspectPoint {
                session,
                tab,
                x,
                y,
                parent_depth,
            }
        }
        BrowserCmd::Logs(logs_cmd) => match logs_cmd {
            LogsCmd::Console {
                session,
                tab,
                level,
                tail,
                since,
                clear,
                ..
            } => {
                let session = session
                    .parse::<SessionId>()
                    .map_err(|e| format!("invalid session id: {e}"))?;
                let tab = tab
                    .parse::<TabId>()
                    .map_err(|e| format!("invalid tab id: {e}"))?;
                Action::LogsConsole {
                    session,
                    tab,
                    level,
                    tail,
                    since: since.map(|n| n.to_string()),
                    clear,
                }
            }
            LogsCmd::Errors {
                session,
                tab,
                source,
                tail,
                since,
                clear,
                ..
            } => {
                let session = session
                    .parse::<SessionId>()
                    .map_err(|e| format!("invalid session id: {e}"))?;
                let tab = tab
                    .parse::<TabId>()
                    .map_err(|e| format!("invalid tab id: {e}"))?;
                Action::LogsErrors {
                    session,
                    tab,
                    source,
                    tail,
                    since: since.map(|n| n.to_string()),
                    clear,
                }
            }
        },
        BrowserCmd::LogsConsole {
            session,
            tab,
            level,
            tail,
            since,
            clear,
            ..
        } => Action::LogsConsole {
            session,
            tab,
            level,
            tail,
            since,
            clear,
        },
        BrowserCmd::LogsErrors {
            session,
            tab,
            source,
            tail,
            since,
            clear,
            ..
        } => Action::LogsErrors {
            session,
            tab,
            source,
            tail,
            since,
            clear,
        },

        // Cookies
        BrowserCmd::Cookies(cmd) => match cmd {
            CookiesCmd::List { session, domain } => Action::CookiesList { session, domain },
            CookiesCmd::Get { name, session } => Action::CookiesGet { session, name },
            CookiesCmd::Set {
                name,
                value,
                session,
                domain,
                path,
                secure,
                http_only,
                same_site,
                expires,
            } => Action::CookiesSet {
                session,
                name,
                value,
                domain,
                path,
                secure: if secure { Some(true) } else { None },
                http_only: if http_only { Some(true) } else { None },
                same_site: same_site.map(|s| s.into()),
                expires,
            },
            CookiesCmd::Delete { name, session } => Action::CookiesDelete { session, name },
            CookiesCmd::Clear { session, domain } => Action::CookiesClear { session, domain },
        },

        // Local Storage
        BrowserCmd::LocalStorage(cmd) => storage_cmd_to_action(cmd, StorageKind::Local),

        // Session Storage
        BrowserCmd::SessionStorage(cmd) => storage_cmd_to_action(cmd, StorageKind::Session),

        // Interaction
        BrowserCmd::Select {
            value,
            selector,
            session,
            tab,
            by_text,
        } => Action::Select {
            session,
            tab,
            selector,
            value,
            by_text,
        },
        BrowserCmd::Hover {
            selector,
            session,
            tab,
        } => Action::Hover {
            session,
            tab,
            selector,
        },
        BrowserCmd::Focus {
            selector,
            session,
            tab,
        } => Action::Focus {
            session,
            tab,
            selector,
        },
        BrowserCmd::Press { key, session, tab } => Action::Press {
            session,
            tab,
            key_or_chord: key,
        },
        BrowserCmd::Drag {
            from,
            to,
            session,
            tab,
        } => Action::Drag {
            session,
            tab,
            from_selector: from,
            to_selector: to,
        },
        BrowserCmd::Upload {
            files,
            selector,
            session,
            tab,
        } => Action::Upload {
            session,
            tab,
            selector,
            files,
        },
        BrowserCmd::Scroll(scroll_cmd) => match scroll_cmd {
            ScrollCmd::Up {
                amount,
                session,
                tab,
            } => Action::Scroll {
                session,
                tab,
                direction: "up".to_string(),
                amount,
                selector: None,
            },
            ScrollCmd::Down {
                amount,
                session,
                tab,
            } => Action::Scroll {
                session,
                tab,
                direction: "down".to_string(),
                amount,
                selector: None,
            },
            ScrollCmd::Left {
                amount,
                session,
                tab,
            } => Action::Scroll {
                session,
                tab,
                direction: "left".to_string(),
                amount,
                selector: None,
            },
            ScrollCmd::Right {
                amount,
                session,
                tab,
            } => Action::Scroll {
                session,
                tab,
                direction: "right".to_string(),
                amount,
                selector: None,
            },
            ScrollCmd::Top { session, tab } => Action::Scroll {
                session,
                tab,
                direction: "top".to_string(),
                amount: None,
                selector: None,
            },
            ScrollCmd::Bottom { session, tab } => Action::Scroll {
                session,
                tab,
                direction: "bottom".to_string(),
                amount: None,
                selector: None,
            },
            ScrollCmd::IntoView {
                selector,
                session,
                tab,
            } => Action::Scroll {
                session,
                tab,
                direction: "into-view".to_string(),
                amount: None,
                selector: Some(selector),
            },
        },
        BrowserCmd::MouseMove {
            coords,
            session,
            tab,
        } => {
            let parts: Vec<&str> = coords.split(',').collect();
            let x = parts
                .first()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .ok_or_else(|| {
                    format!("invalid coordinates '{}': expected format 'x,y'", coords)
                })?;
            let y = parts
                .get(1)
                .and_then(|s| s.trim().parse::<f64>().ok())
                .ok_or_else(|| {
                    format!("invalid coordinates '{}': expected format 'x,y'", coords)
                })?;
            Action::MouseMove { session, tab, x, y }
        }
        BrowserCmd::CursorPosition { session, tab } => Action::CursorPosition { session, tab },

        // Waiting
        BrowserCmd::Wait(wait_cmd) => match wait_cmd {
            WaitCmd::Element {
                selector,
                session,
                tab,
                timeout,
            } => Action::WaitElement {
                session,
                tab,
                selector,
                timeout_ms: timeout,
            },
            WaitCmd::Navigation {
                session,
                tab,
                timeout,
            } => Action::WaitNavigation {
                session,
                tab,
                timeout_ms: timeout,
            },
            WaitCmd::NetworkIdle {
                session,
                tab,
                timeout,
                idle_time,
            } => Action::WaitNetworkIdle {
                session,
                tab,
                timeout_ms: timeout,
                idle_time_ms: idle_time,
            },
            WaitCmd::Condition {
                expression,
                session,
                tab,
                timeout,
            } => Action::WaitCondition {
                session,
                tab,
                expression,
                timeout_ms: timeout,
            },
        },

        // Session management
        BrowserCmd::Restart { session } => Action::RestartSession { session },
    };

    Ok((action, screenshot_path))
}

/// Convert a storage subcommand + kind into an Action.
fn storage_cmd_to_action(cmd: StorageSubCmd, kind: StorageKind) -> Action {
    match cmd {
        StorageSubCmd::List { session, tab } => Action::StorageList { session, tab, kind },
        StorageSubCmd::Get { key, session, tab } => Action::StorageGet {
            session,
            tab,
            kind,
            key,
        },
        StorageSubCmd::Set {
            key,
            value,
            session,
            tab,
        } => Action::StorageSet {
            session,
            tab,
            kind,
            key,
            value,
        },
        StorageSubCmd::Delete { key, session, tab } => Action::StorageDelete {
            session,
            tab,
            kind,
            key,
        },
        StorageSubCmd::Clear { session, tab, .. } => Action::StorageClear { session, tab, kind },
    }
}

// ---------------------------------------------------------------------------
// Daemon auto-start
// ---------------------------------------------------------------------------

/// Maximum time to wait for the daemon to become ready after forking.
const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Interval between socket connectivity probes.
const DAEMON_PROBE_INTERVAL: Duration = Duration::from_millis(100);

/// Check whether the daemon socket is connectable.
async fn socket_is_ready(path: &Path) -> bool {
    UnixStream::connect(path).await.is_ok()
}

/// Ensure the daemon is running. If the socket is not connectable, fork a
/// daemon child process and wait until the socket becomes available (up to
/// [`DAEMON_READY_TIMEOUT`]).
pub async fn ensure_daemon_running(socket_path: &Path) -> Result<(), String> {
    if socket_is_ready(socket_path).await {
        debug!("daemon already running at {}", socket_path.display());
        return Ok(());
    }

    info!("daemon not running, auto-starting...");

    // Re-exec ourselves with `daemon serve-v2` which runs run_daemon() in foreground.
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot determine own executable: {e}"))?;

    let child = std::process::Command::new(&exe)
        .args(["daemon", "serve-v2"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn daemon: {e}"))?;

    info!("daemon child spawned (pid {})", child.id());

    // Wait for the socket to become connectable.
    let deadline = tokio::time::Instant::now() + DAEMON_READY_TIMEOUT;
    loop {
        if socket_is_ready(socket_path).await {
            info!("daemon ready at {}", socket_path.display());
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "daemon did not become ready within {}s at {}",
                DAEMON_READY_TIMEOUT.as_secs(),
                socket_path.display()
            ));
        }
        tokio::time::sleep(DAEMON_PROBE_INTERVAL).await;
    }
}

/// Run the daemon in the foreground (for `actionbook daemon serve-v2`).
pub async fn run_daemon_foreground() -> Result<(), String> {
    let config = DaemonConfig::default();
    super::daemon_main::run_daemon(config)
        .await
        .map_err(|e| format!("daemon exited with error: {e}"))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

impl CliV2 {
    /// Run the CLI: ensure daemon -> parse -> build Action -> send -> format output.
    pub async fn run(self) -> ! {
        let socket_path = self.socket.unwrap_or_else(client::default_socket_path);

        let TopLevel::Browser { cmd } = self.command;
        let (action, screenshot_path) = match build_action(cmd) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("error: {e}");
                process::exit(1);
            }
        };

        // Auto-start daemon if not running.
        if let Err(e) = ensure_daemon_running(&socket_path).await {
            eprintln!("error: {e}");
            process::exit(1);
        }

        let mut client = match DaemonClient::connect(&socket_path).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                process::exit(1);
            }
        };

        let started_at = Instant::now();
        let result = match client.send_action(action.clone()).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                process::exit(1);
            }
        };
        let duration_ms = started_at.elapsed().as_millis();

        // If this was a screenshot command, decode the base64 PNG and write to disk.
        if let Some(path) = screenshot_path {
            if result.is_ok() {
                if let super::action_result::ActionResult::Ok { ref data } = result {
                    if let Some(b64) = data.get("data").and_then(|v| v.as_str()) {
                        use base64::Engine;
                        match base64::engine::general_purpose::STANDARD.decode(b64) {
                            Ok(bytes) => {
                                if let Err(e) = std::fs::write(&path, &bytes) {
                                    if self.json {
                                        println!(
                                            "{}",
                                            formatter::format_cli_side_error_json(
                                                &action,
                                                "ARTIFACT_WRITE_FAILED",
                                                &format!(
                                                    "failed to write screenshot to {}: {e}",
                                                    path.display()
                                                ),
                                                serde_json::json!({
                                                    "path": path.display().to_string()
                                                }),
                                                duration_ms
                                            )
                                        );
                                    } else {
                                        eprintln!(
                                            "{}",
                                            formatter::format_cli_side_error_text(
                                                &action,
                                                "ARTIFACT_WRITE_FAILED",
                                                &format!(
                                                    "failed to write screenshot to {}: {e}",
                                                    path.display()
                                                )
                                            )
                                        );
                                    }
                                    process::exit(1);
                                }
                                if self.json {
                                    println!(
                                        "{}",
                                        formatter::format_cli_result_json(
                                            &action,
                                            &result,
                                            duration_ms
                                        )
                                    );
                                } else {
                                    println!("screenshot saved to {}", path.display());
                                }
                                process::exit(0);
                            }
                            Err(e) => {
                                if self.json {
                                    println!(
                                        "{}",
                                        formatter::format_cli_side_error_json(
                                            &action,
                                            "INTERNAL_ERROR",
                                            &format!("failed to decode screenshot data: {e}"),
                                            serde_json::json!({
                                                "reason": "screenshot_decode_failed"
                                            }),
                                            duration_ms
                                        )
                                    );
                                } else {
                                    eprintln!(
                                        "{}",
                                        formatter::format_cli_side_error_text(
                                            &action,
                                            "INTERNAL_ERROR",
                                            &format!("failed to decode screenshot data: {e}")
                                        )
                                    );
                                }
                                process::exit(1);
                            }
                        }
                    }
                }
            }
        }

        let output = if self.json {
            formatter::format_cli_result_json(&action, &result, duration_ms)
        } else {
            formatter::format_cli_result(&action, &result)
        };
        if !output.is_empty() {
            println!("{output}");
        }

        if formatter::is_error(&result) {
            process::exit(1);
        }
        process::exit(0);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::commands::CliQueryMode;
    use super::*;
    use crate::daemon::types::QueryMode;

    #[test]
    fn build_start_action_local() {
        let (action, _) = build_action(BrowserCmd::Start {
            mode: CliMode::Local,
            profile: Some("test".into()),
            headless: true,
            open_url: Some("https://example.com".into()),
            cdp_endpoint: None,
            set_session_id: None,
        })
        .unwrap();
        match action {
            Action::StartSession {
                mode,
                profile,
                headless,
                open_url,
                ..
            } => {
                assert_eq!(mode, Mode::Local);
                assert_eq!(profile.as_deref(), Some("test"));
                assert!(headless);
                assert_eq!(open_url.as_deref(), Some("https://example.com"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_start_action_cloud_with_endpoint() {
        let (action, _) = build_action(BrowserCmd::Start {
            mode: CliMode::Cloud,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: Some("wss://cloud.example.com/browser".into()),
            set_session_id: None,
        })
        .unwrap();
        match action {
            Action::StartSession {
                mode, cdp_endpoint, ..
            } => {
                assert_eq!(mode, Mode::Cloud);
                assert_eq!(
                    cdp_endpoint.as_deref(),
                    Some("wss://cloud.example.com/browser")
                );
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_start_action_cloud_without_endpoint_errors() {
        let result = build_action(BrowserCmd::Start {
            mode: CliMode::Cloud,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            set_session_id: None,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("--cdp-endpoint"));
    }

    #[test]
    fn build_start_action_extension() {
        let (action, _) = build_action(BrowserCmd::Start {
            mode: CliMode::Extension,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            set_session_id: None,
        })
        .unwrap();
        match action {
            Action::StartSession { mode, .. } => {
                assert_eq!(mode, Mode::Extension);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_list_sessions() {
        let (action, _) = build_action(BrowserCmd::ListSessions).unwrap();
        assert!(matches!(action, Action::ListSessions));
    }

    #[test]
    fn build_goto_action() {
        let (action, _) = build_action(BrowserCmd::Goto {
            url: "https://example.com".into(),
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
        })
        .unwrap();
        match action {
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
    fn build_snapshot_action() {
        let (action, _) = build_action(BrowserCmd::Snapshot {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            interactive: true,
            compact: false,
            cursor: false,
            depth: None,
            selector: None,
        })
        .unwrap();
        match action {
            Action::Snapshot {
                interactive,
                compact,
                ..
            } => {
                assert!(interactive);
                assert!(!compact);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_close_session() {
        let (action, _) = build_action(BrowserCmd::Close {
            session: SessionId::new_unchecked("local-4"),
        })
        .unwrap();
        match action {
            Action::CloseSession { session } => {
                assert_eq!(session, SessionId::new_unchecked("local-4"))
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_click_action() {
        let (action, _) = build_action(BrowserCmd::Click {
            selector: "#btn".into(),
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        })
        .unwrap();
        match action {
            Action::Click {
                selector,
                button,
                count,
                ..
            } => {
                assert_eq!(selector, "#btn");
                assert!(button.is_none());
                assert!(count.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_eval_action() {
        let (action, _) = build_action(BrowserCmd::Eval {
            code: "document.title".into(),
            session: SessionId::new_unchecked("local-2"),
            tab: TabId(2),
        })
        .unwrap();
        match action {
            Action::Eval { expression, .. } => assert_eq!(expression, "document.title"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_cookies_list_with_domain() {
        let (action, _) = build_action(BrowserCmd::Cookies(CookiesCmd::List {
            session: SessionId::new_unchecked("local-3"),
            domain: Some("example.com".into()),
        }))
        .unwrap();
        match action {
            Action::CookiesList { session, domain } => {
                assert_eq!(session, SessionId::new_unchecked("local-3"));
                assert_eq!(domain.as_deref(), Some("example.com"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_cookies_clear_with_domain() {
        let (action, _) = build_action(BrowserCmd::Cookies(CookiesCmd::Clear {
            session: SessionId::new_unchecked("local-4"),
            domain: Some(".example.com".into()),
        }))
        .unwrap();
        match action {
            Action::CookiesClear { session, domain } => {
                assert_eq!(session, SessionId::new_unchecked("local-4"));
                assert_eq!(domain.as_deref(), Some(".example.com"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn cli_mode_to_mode() {
        assert_eq!(Mode::from(CliMode::Local), Mode::Local);
        assert_eq!(Mode::from(CliMode::Extension), Mode::Extension);
        assert_eq!(Mode::from(CliMode::Cloud), Mode::Cloud);
    }

    #[test]
    fn build_session_and_navigation_variants() {
        let session = SessionId::new_unchecked("local-1");

        let (action, _) = build_action(BrowserCmd::Status {
            session: session.clone(),
        })
        .unwrap();
        assert!(matches!(action, Action::SessionStatus { session: s } if s == session));

        let (action, _) = build_action(BrowserCmd::ListTabs {
            session: session.clone(),
        })
        .unwrap();
        assert!(matches!(action, Action::ListTabs { session: s } if s == session));

        let (action, _) = build_action(BrowserCmd::ListWindows {
            session: session.clone(),
        })
        .unwrap();
        assert!(matches!(action, Action::ListWindows { session: s } if s == session));

        let (action, _) = build_action(BrowserCmd::NewTab {
            url: "https://example.com".into(),
            session: session.clone(),
            new_window: true,
            window: None,
        })
        .unwrap();
        assert!(matches!(
            action,
            Action::NewTab {
                session: s,
                url,
                new_window: true,
                window: None
            } if s == session && url == "https://example.com"
        ));

        let (action, _) = build_action(BrowserCmd::CloseTab {
            session: session.clone(),
            tab: TabId(2),
        })
        .unwrap();
        assert!(matches!(
            action,
            Action::CloseTab {
                session: s,
                tab: TabId(2)
            } if s == session
        ));

        let (action, _) = build_action(BrowserCmd::Back {
            session: session.clone(),
            tab: TabId(0),
        })
        .unwrap();
        assert!(matches!(action, Action::Back { session: s, tab: TabId(0) } if s == session));

        let (action, _) = build_action(BrowserCmd::Forward {
            session: session.clone(),
            tab: TabId(0),
        })
        .unwrap();
        assert!(matches!(action, Action::Forward { session: s, tab: TabId(0) } if s == session));

        let (action, _) = build_action(BrowserCmd::Reload {
            session: session.clone(),
            tab: TabId(0),
        })
        .unwrap();
        assert!(matches!(action, Action::Reload { session: s, tab: TabId(0) } if s == session));

        let (action, _) = build_action(BrowserCmd::Restart { session }).unwrap();
        assert!(matches!(action, Action::RestartSession { .. }));
    }

    #[test]
    fn build_observation_variants() {
        let session = SessionId::new_unchecked("local-1");
        let tab = TabId(3);

        let (action, path) = build_action(BrowserCmd::Screenshot {
            path: PathBuf::from("/tmp/out.png"),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert_eq!(path.as_deref(), Some(Path::new("/tmp/out.png")));
        assert!(matches!(
            action,
            Action::Screenshot {
                session: s,
                tab: TabId(3),
                full_page: false
            } if s == session
        ));

        let (action, _) = build_action(BrowserCmd::Pdf {
            path: PathBuf::from("/tmp/out.pdf"),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(
            action,
            Action::Pdf {
                session: s,
                tab: TabId(3),
                path
            } if s == session && path == "/tmp/out.pdf"
        ));

        for (cmd, expected) in [
            (
                BrowserCmd::Title {
                    session: session.clone(),
                    tab,
                },
                "title",
            ),
            (
                BrowserCmd::Url {
                    session: session.clone(),
                    tab,
                },
                "url",
            ),
            (
                BrowserCmd::Viewport {
                    session: session.clone(),
                    tab,
                },
                "viewport",
            ),
        ] {
            let (action, _) = build_action(cmd).unwrap();
            match (expected, action) {
                (
                    "title",
                    Action::Title {
                        session: s,
                        tab: TabId(3),
                    },
                ) => assert_eq!(s, session),
                (
                    "url",
                    Action::Url {
                        session: s,
                        tab: TabId(3),
                    },
                ) => assert_eq!(s, session),
                (
                    "viewport",
                    Action::Viewport {
                        session: s,
                        tab: TabId(3),
                    },
                ) => {
                    assert_eq!(s, session)
                }
                _ => panic!("unexpected observation mapping"),
            }
        }

        let (action, _) = build_action(BrowserCmd::Html {
            selector: "#root".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::Html { selector: Some(sel), .. } if sel == "#root"));

        let (action, _) = build_action(BrowserCmd::TextCmd {
            selector: "#copy".into(),
            session: session.clone(),
            tab,
            mode: None,
        })
        .unwrap();
        assert!(matches!(action, Action::Text { selector: Some(sel), .. } if sel == "#copy"));

        let (action, _) = build_action(BrowserCmd::Value {
            selector: "#field".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::Value { selector, .. } if selector == "#field"));

        let (action, _) = build_action(BrowserCmd::Attr {
            selector: "#field".into(),
            name: "aria-label".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(
            matches!(action, Action::Attr { selector, name, .. } if selector == "#field" && name == "aria-label")
        );

        let (action, _) = build_action(BrowserCmd::Attrs {
            selector: "#field".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::Attrs { selector, .. } if selector == "#field"));

        let (action, _) = build_action(BrowserCmd::Describe {
            selector: "#field".into(),
            session: session.clone(),
            tab,
            nearby: false,
        })
        .unwrap();
        assert!(matches!(action, Action::Describe { selector, .. } if selector == "#field"));

        let (action, _) = build_action(BrowserCmd::State {
            selector: "#field".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::State { selector, .. } if selector == "#field"));

        let (action, _) = build_action(BrowserCmd::Box_ {
            selector: "#field".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::Box_ { selector, .. } if selector == "#field"));

        let (action, _) = build_action(BrowserCmd::Styles {
            selector: "#field".into(),
            session: session.clone(),
            tab,
            names: vec![],
        })
        .unwrap();
        assert!(matches!(action, Action::Styles { selector, .. } if selector == "#field"));

        let (action, _) = build_action(BrowserCmd::InspectPoint {
            coords: "10.5,20.25".to_string(),
            parent_depth: None,
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::InspectPoint { x, y, .. } if x == 10.5 && y == 20.25));

        let (action, _) = build_action(BrowserCmd::LogsConsole {
            session: session.clone(),
            tab,
            level: None,
            tail: None,
            since: None,
            clear: false,
            json: false,
        })
        .unwrap();
        assert!(matches!(
            action,
            Action::LogsConsole {
                session: s,
                tab: TabId(3),
                ..
            } if s == session
        ));

        let (action, _) = build_action(BrowserCmd::LogsErrors {
            session,
            tab,
            source: None,
            tail: None,
            since: None,
            clear: false,
            json: false,
        })
        .unwrap();
        assert!(matches!(action, Action::LogsErrors { tab: TabId(3), .. }));
    }

    #[test]
    fn build_query_variants() {
        let session = SessionId::new_unchecked("local-1");
        let tab = TabId(0);

        let (action, _) = build_action(BrowserCmd::Query(QueryCmd::One {
            selector: "#one".into(),
            session: session.clone(),
            tab,
            mode: CliQueryMode::Text,
        }))
        .unwrap();
        assert!(matches!(
            action,
            Action::Query {
                selector,
                mode: QueryMode::Text,
                cardinality: QueryCardinality::One,
                nth_index: None,
                ..
            } if selector == "#one"
        ));

        let (action, _) = build_action(BrowserCmd::Query(QueryCmd::All {
            selector: "//button".into(),
            session: session.clone(),
            tab,
            mode: CliQueryMode::Xpath,
        }))
        .unwrap();
        assert!(matches!(
            action,
            Action::Query {
                selector,
                mode: QueryMode::Xpath,
                cardinality: QueryCardinality::All,
                nth_index: None,
                ..
            } if selector == "//button"
        ));

        let (action, _) = build_action(BrowserCmd::Query(QueryCmd::Count {
            selector: ".count".into(),
            session: session.clone(),
            tab,
            mode: CliQueryMode::Css,
        }))
        .unwrap();
        assert!(matches!(
            action,
            Action::Query {
                selector,
                mode: QueryMode::Css,
                cardinality: QueryCardinality::Count,
                nth_index: None,
                ..
            } if selector == ".count"
        ));

        let (action, _) = build_action(BrowserCmd::Query(QueryCmd::Nth {
            n: 4,
            selector: ".item".into(),
            session,
            tab,
            mode: CliQueryMode::Css,
        }))
        .unwrap();
        assert!(matches!(
            action,
            Action::Query {
                selector,
                cardinality: QueryCardinality::Nth,
                nth_index: Some(4),
                ..
            } if selector == ".item"
        ));
    }

    #[test]
    fn storage_subcommands_map_for_both_kinds() {
        let session = SessionId::new_unchecked("local-1");
        let tab = TabId(1);

        let local_list = storage_cmd_to_action(
            StorageSubCmd::List {
                session: session.clone(),
                tab,
            },
            StorageKind::Local,
        );
        assert!(matches!(
            local_list,
            Action::StorageList {
                kind: StorageKind::Local,
                ..
            }
        ));

        let session_get = storage_cmd_to_action(
            StorageSubCmd::Get {
                key: "theme".into(),
                session: session.clone(),
                tab,
            },
            StorageKind::Session,
        );
        assert!(
            matches!(session_get, Action::StorageGet { kind: StorageKind::Session, key, .. } if key == "theme")
        );

        let local_set = storage_cmd_to_action(
            StorageSubCmd::Set {
                key: "theme".into(),
                value: "dark".into(),
                session: session.clone(),
                tab,
            },
            StorageKind::Local,
        );
        assert!(
            matches!(local_set, Action::StorageSet { kind: StorageKind::Local, key, value, .. } if key == "theme" && value == "dark")
        );

        let session_delete = storage_cmd_to_action(
            StorageSubCmd::Delete {
                key: "theme".into(),
                session: session.clone(),
                tab,
            },
            StorageKind::Session,
        );
        assert!(
            matches!(session_delete, Action::StorageDelete { kind: StorageKind::Session, key, .. } if key == "theme")
        );

        let local_clear = storage_cmd_to_action(
            StorageSubCmd::Clear {
                key: None,
                session,
                tab,
            },
            StorageKind::Local,
        );
        assert!(matches!(
            local_clear,
            Action::StorageClear {
                kind: StorageKind::Local,
                ..
            }
        ));
    }

    #[test]
    fn build_interaction_and_wait_variants() {
        let session = SessionId::new_unchecked("local-1");
        let tab = TabId(2);

        let (action, _) = build_action(BrowserCmd::Select {
            value: "opt-1".into(),
            selector: "#select".into(),
            session: session.clone(),
            tab,
            by_text: true,
        })
        .unwrap();
        assert!(
            matches!(action, Action::Select { selector, value, by_text: true, .. } if selector == "#select" && value == "opt-1")
        );

        let (action, _) = build_action(BrowserCmd::Hover {
            selector: "#hover".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::Hover { selector, .. } if selector == "#hover"));

        let (action, _) = build_action(BrowserCmd::Focus {
            selector: "#focus".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::Focus { selector, .. } if selector == "#focus"));

        let (action, _) = build_action(BrowserCmd::Press {
            key: "Shift+Tab".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(
            matches!(action, Action::Press { key_or_chord, .. } if key_or_chord == "Shift+Tab")
        );

        let (action, _) = build_action(BrowserCmd::Drag {
            from: "#from".into(),
            to: "#to".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(
            matches!(action, Action::Drag { from_selector, to_selector, .. } if from_selector == "#from" && to_selector == "#to")
        );

        let (action, _) = build_action(BrowserCmd::Upload {
            files: vec!["/tmp/a.txt".into(), "/tmp/b.txt".into()],
            selector: "#file".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(
            matches!(action, Action::Upload { selector, files, .. } if selector == "#file" && files.len() == 2)
        );

        let (action, _) = build_action(BrowserCmd::Scroll(ScrollCmd::IntoView {
            selector: "#target".into(),
            session: session.clone(),
            tab,
        }))
        .unwrap();
        assert!(
            matches!(action, Action::Scroll { direction, selector: Some(sel), amount: None, .. } if direction == "into-view" && sel == "#target")
        );

        let (action, _) = build_action(BrowserCmd::Scroll(ScrollCmd::Down {
            amount: Some(240),
            session: session.clone(),
            tab,
        }))
        .unwrap();
        assert!(
            matches!(action, Action::Scroll { direction, amount: Some(240), selector: None, .. } if direction == "down")
        );

        let (action, _) = build_action(BrowserCmd::MouseMove {
            coords: "10.5, 20.25".into(),
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::MouseMove { x, y, .. } if x == 10.5 && y == 20.25));

        let (action, _) = build_action(BrowserCmd::CursorPosition {
            session: session.clone(),
            tab,
        })
        .unwrap();
        assert!(matches!(action, Action::CursorPosition { .. }));

        let (action, _) = build_action(BrowserCmd::Wait(WaitCmd::Element {
            selector: "#ready".into(),
            session: session.clone(),
            tab,
            timeout: Some(5000),
        }))
        .unwrap();
        assert!(
            matches!(action, Action::WaitElement { selector, timeout_ms: Some(5000), .. } if selector == "#ready")
        );

        let (action, _) = build_action(BrowserCmd::Wait(WaitCmd::Navigation {
            session: session.clone(),
            tab,
            timeout: Some(4000),
        }))
        .unwrap();
        assert!(matches!(
            action,
            Action::WaitNavigation {
                timeout_ms: Some(4000),
                ..
            }
        ));

        let (action, _) = build_action(BrowserCmd::Wait(WaitCmd::NetworkIdle {
            session: session.clone(),
            tab,
            timeout: Some(9000),
            idle_time: Some(800),
        }))
        .unwrap();
        assert!(matches!(
            action,
            Action::WaitNetworkIdle {
                timeout_ms: Some(9000),
                idle_time_ms: Some(800),
                ..
            }
        ));

        let (action, _) = build_action(BrowserCmd::Wait(WaitCmd::Condition {
            expression: "window.ready".into(),
            session,
            tab,
            timeout: Some(7000),
        }))
        .unwrap();
        assert!(
            matches!(action, Action::WaitCondition { expression, timeout_ms: Some(7000), .. } if expression == "window.ready")
        );
    }

    #[test]
    fn mouse_move_rejects_invalid_coordinates() {
        let err = build_action(BrowserCmd::MouseMove {
            coords: "oops".into(),
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        })
        .unwrap_err();
        assert!(err.contains("expected format 'x,y'"));
    }
}
