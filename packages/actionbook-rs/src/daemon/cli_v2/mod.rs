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
use super::types::{Mode, QueryCardinality, SessionId, StorageKind, TabId};

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
    },

    /// List all active sessions
    ListSessions,

    // =======================================================================
    // Session-level commands — require -s
    // =======================================================================
    /// Show session status
    Status {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// List tabs in a session
    ListTabs {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// List windows in a session
    ListWindows {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// Open a URL in a new tab
    Open {
        /// URL to open
        url: String,
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Open in a new window
        #[arg(long)]
        new_window: bool,
    },

    /// Close a session and its browser
    Close {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
    },

    /// Close a specific tab
    CloseTab {
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Navigate back in history
    Back {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Navigate forward in history
    Forward {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Reload the current page
    Reload {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Capture an accessibility-tree snapshot
    Snapshot {
        /// Session ID (e.g. s0)
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
    },

    /// Take a screenshot (saves PNG to path)
    Screenshot {
        /// Output file path
        path: PathBuf,
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the page title
    Title {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the current page URL
    Url {
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the value of an input element
    Value {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the interactive state of an element
    State {
        /// CSS selector
        selector: String,
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the viewport dimensions
    Viewport {
        /// Session ID (e.g. s0)
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
        /// X coordinate
        x: f64,
        /// Y coordinate
        y: f64,
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get console log messages
    LogsConsole {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get error log messages
    LogsErrors {
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
        /// Tab ID (e.g. t0)
        #[arg(short = 't', long)]
        tab: TabId,
    },

    /// Get the current cursor position
    CursorPosition {
        /// Session ID (e.g. s0)
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
        /// Session ID (e.g. s0)
        #[arg(short = 's', long)]
        session: SessionId,
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
            }
        }
        BrowserCmd::ListSessions => Action::ListSessions,

        // Session
        BrowserCmd::Status { session } => Action::SessionStatus { session },
        BrowserCmd::ListTabs { session } => Action::ListTabs { session },
        BrowserCmd::ListWindows { session } => Action::ListWindows { session },
        BrowserCmd::Open {
            url,
            session,
            new_window,
        } => Action::NewTab {
            session,
            url,
            new_window,
            window: None,
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
        } => Action::Snapshot {
            session,
            tab,
            interactive,
            compact,
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
        } => Action::Text {
            session,
            tab,
            selector: Some(selector),
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
        } => Action::Describe {
            session,
            tab,
            selector,
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
        } => Action::Styles {
            session,
            tab,
            selector,
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
        BrowserCmd::InspectPoint { x, y, session, tab } => {
            Action::InspectPoint { session, tab, x, y }
        }
        BrowserCmd::LogsConsole { session, tab } => Action::LogsConsole { session, tab },
        BrowserCmd::LogsErrors { session, tab } => Action::LogsErrors { session, tab },

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
                                    eprintln!("error: failed to write screenshot: {e}");
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
                                eprintln!("error: failed to decode screenshot data: {e}");
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
    use super::*;

    #[test]
    fn build_start_action_local() {
        let (action, _) = build_action(BrowserCmd::Start {
            mode: CliMode::Local,
            profile: Some("test".into()),
            headless: true,
            open_url: Some("https://example.com".into()),
            cdp_endpoint: None,
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
            session: SessionId(0),
            tab: TabId(1),
        })
        .unwrap();
        match action {
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
    fn build_snapshot_action() {
        let (action, _) = build_action(BrowserCmd::Snapshot {
            session: SessionId(0),
            tab: TabId(0),
            interactive: true,
            compact: false,
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
            session: SessionId(3),
        })
        .unwrap();
        match action {
            Action::CloseSession { session } => assert_eq!(session, SessionId(3)),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_click_action() {
        let (action, _) = build_action(BrowserCmd::Click {
            selector: "#btn".into(),
            session: SessionId(0),
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
            session: SessionId(1),
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
            session: SessionId(2),
            domain: Some("example.com".into()),
        }))
        .unwrap();
        match action {
            Action::CookiesList { session, domain } => {
                assert_eq!(session, SessionId(2));
                assert_eq!(domain.as_deref(), Some("example.com"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_cookies_clear_with_domain() {
        let (action, _) = build_action(BrowserCmd::Cookies(CookiesCmd::Clear {
            session: SessionId(3),
            domain: Some(".example.com".into()),
        }))
        .unwrap();
        match action {
            Action::CookiesClear { session, domain } => {
                assert_eq!(session, SessionId(3));
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
}
