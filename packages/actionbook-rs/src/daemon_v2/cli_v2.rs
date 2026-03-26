//! CLI v2 thin client — arg parsing, Action construction, RPC, formatting.
//!
//! This module defines the Clap subcommands for Phase 1 browser commands.
//! The CLI is stateless: it parses args, constructs an [`Action`], sends it
//! to the daemon via [`DaemonClient`], and formats the [`ActionResult`].

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use super::action::Action;
use super::client::{self, DaemonClient};
use super::formatter;
use super::types::{Mode, SessionId, TabId};

/// Actionbook CLI v2 — browser automation via daemon
#[derive(Parser, Debug)]
#[command(name = "actionbook")]
pub struct CliV2 {
    /// Path to the daemon socket (default: ~/.actionbook/daemons/v2.sock)
    #[arg(long, global = true, env = "ACTIONBOOK_SOCKET")]
    socket: Option<PathBuf>,

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
        /// Value to fill
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
}

/// CLI-facing mode enum (maps to protocol [`Mode`]).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliMode {
    Local,
    Extension,
    Cloud,
}

impl From<CliMode> for Mode {
    fn from(m: CliMode) -> Mode {
        match m {
            CliMode::Local => Mode::Local,
            CliMode::Extension => Mode::Extension,
            CliMode::Cloud => Mode::Cloud,
        }
    }
}

// ---------------------------------------------------------------------------
// Action construction — pure mapping, no logic
// ---------------------------------------------------------------------------

fn build_action(cmd: BrowserCmd) -> Action {
    match cmd {
        // Global
        BrowserCmd::Start {
            mode,
            profile,
            headless,
            open_url,
        } => Action::StartSession {
            mode: mode.into(),
            profile,
            headless,
            open_url,
            cdp_endpoint: None,
        },
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

        // Tab
        BrowserCmd::Goto {
            url,
            session,
            tab,
        } => Action::Goto {
            session,
            tab,
            url,
        },
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
            text,
            session,
            tab,
            selector,
        } => Action::Fill {
            session,
            tab,
            selector: selector.unwrap_or_default(),
            value: text,
        },
        BrowserCmd::Eval {
            code,
            session,
            tab,
        } => Action::Eval {
            session,
            tab,
            expression: code,
        },
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

impl CliV2 {
    /// Run the CLI: parse -> build Action -> send to daemon -> format output.
    pub async fn run(self) -> ! {
        let socket_path = self.socket.unwrap_or_else(client::default_socket_path);

        let TopLevel::Browser { cmd } = self.command;
        let action = build_action(cmd);

        let mut client = match DaemonClient::connect(&socket_path).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                process::exit(1);
            }
        };

        let result = match client.send_action(action).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                process::exit(1);
            }
        };

        let output = formatter::format_result(&result);
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
    fn build_start_action() {
        let action = build_action(BrowserCmd::Start {
            mode: CliMode::Local,
            profile: Some("test".into()),
            headless: true,
            open_url: Some("https://example.com".into()),
        });
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
    fn build_list_sessions() {
        let action = build_action(BrowserCmd::ListSessions);
        assert!(matches!(action, Action::ListSessions));
    }

    #[test]
    fn build_goto_action() {
        let action = build_action(BrowserCmd::Goto {
            url: "https://example.com".into(),
            session: SessionId(0),
            tab: TabId(1),
        });
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
        let action = build_action(BrowserCmd::Snapshot {
            session: SessionId(0),
            tab: TabId(0),
            interactive: true,
            compact: false,
        });
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
        let action = build_action(BrowserCmd::Close {
            session: SessionId(3),
        });
        match action {
            Action::CloseSession { session } => assert_eq!(session, SessionId(3)),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_click_action() {
        let action = build_action(BrowserCmd::Click {
            selector: "#btn".into(),
            session: SessionId(0),
            tab: TabId(0),
        });
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
        let action = build_action(BrowserCmd::Eval {
            code: "document.title".into(),
            session: SessionId(1),
            tab: TabId(2),
        });
        match action {
            Action::Eval { expression, .. } => assert_eq!(expression, "document.title"),
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
