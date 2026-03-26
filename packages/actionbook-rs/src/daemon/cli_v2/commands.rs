//! Subcommand enums and CLI-facing helper types.
//!
//! Extracted from `cli_v2` to keep the main module focused on top-level
//! dispatch, action construction, and daemon lifecycle.

use clap::Subcommand;

use super::super::types::{Mode, QueryMode, SameSite, SessionId, StorageKind, TabId};

// ---------------------------------------------------------------------------
// CLI-facing enums (map to protocol types)
// ---------------------------------------------------------------------------

/// CLI-facing mode enum (maps to protocol [`Mode`]).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CliMode {
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

/// CLI-facing query mode enum.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CliQueryMode {
    Css,
    Xpath,
    Text,
}

impl From<CliQueryMode> for QueryMode {
    fn from(m: CliQueryMode) -> QueryMode {
        match m {
            CliQueryMode::Css => QueryMode::Css,
            CliQueryMode::Xpath => QueryMode::Xpath,
            CliQueryMode::Text => QueryMode::Text,
        }
    }
}

/// CLI-facing storage kind enum.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CliStorageKind {
    Local,
    Session,
}

impl From<CliStorageKind> for StorageKind {
    fn from(k: CliStorageKind) -> StorageKind {
        match k {
            CliStorageKind::Local => StorageKind::Local,
            CliStorageKind::Session => StorageKind::Session,
        }
    }
}

/// CLI-facing SameSite enum.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CliSameSite {
    Strict,
    Lax,
    None,
}

impl From<CliSameSite> for SameSite {
    fn from(s: CliSameSite) -> SameSite {
        match s {
            CliSameSite::Strict => SameSite::Strict,
            CliSameSite::Lax => SameSite::Lax,
            CliSameSite::None => SameSite::None,
        }
    }
}

// ---------------------------------------------------------------------------
// Subcommand enums
// ---------------------------------------------------------------------------

/// Query subcommands: one, all, count, nth (PRD §10.7).
#[derive(Subcommand, Debug)]
pub(crate) enum QueryCmd {
    /// Query exactly one element
    One {
        /// CSS selector
        selector: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Query mode: css, xpath, or text
        #[arg(long, value_enum, default_value = "css")]
        mode: CliQueryMode,
    },
    /// Query all matching elements
    All {
        /// CSS selector
        selector: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Query mode: css, xpath, or text
        #[arg(long, value_enum, default_value = "css")]
        mode: CliQueryMode,
    },
    /// Count matching elements
    Count {
        /// CSS selector
        selector: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Query mode: css, xpath, or text
        #[arg(long, value_enum, default_value = "css")]
        mode: CliQueryMode,
    },
    /// Query the nth matching element (1-based)
    Nth {
        /// 1-based index
        n: u32,
        /// CSS selector
        selector: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Query mode: css, xpath, or text
        #[arg(long, value_enum, default_value = "css")]
        mode: CliQueryMode,
    },
}

/// Scroll subcommands: up, down, left, right, top, bottom, into-view.
#[derive(Subcommand, Debug)]
pub(crate) enum ScrollCmd {
    /// Scroll up
    Up {
        /// Amount in pixels (default: 300)
        amount: Option<i32>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Scroll down
    Down {
        /// Amount in pixels (default: 300)
        amount: Option<i32>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Scroll left
    Left {
        /// Amount in pixels (default: 300)
        amount: Option<i32>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Scroll right
    Right {
        /// Amount in pixels (default: 300)
        amount: Option<i32>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Scroll to top of page
    Top {
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Scroll to bottom of page
    Bottom {
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Scroll an element into view
    IntoView {
        /// CSS selector
        selector: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
}

/// Cookies subcommands: list, get, set, delete, clear.
#[derive(Subcommand, Debug)]
pub(crate) enum CookiesCmd {
    /// List all cookies
    List {
        #[arg(short = 's', long)]
        session: SessionId,
        /// Filter by domain
        #[arg(long)]
        domain: Option<String>,
    },
    /// Get a specific cookie by name
    Get {
        /// Cookie name
        name: String,
        #[arg(short = 's', long)]
        session: SessionId,
    },
    /// Set a cookie
    Set {
        /// Cookie name
        name: String,
        /// Cookie value
        value: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        secure: bool,
        #[arg(long)]
        http_only: bool,
        #[arg(long, value_enum)]
        same_site: Option<CliSameSite>,
        #[arg(long)]
        expires: Option<f64>,
    },
    /// Delete a cookie by name
    Delete {
        /// Cookie name
        name: String,
        #[arg(short = 's', long)]
        session: SessionId,
    },
    /// Clear all cookies
    Clear {
        #[arg(short = 's', long)]
        session: SessionId,
        /// Filter by domain
        #[arg(long)]
        domain: Option<String>,
    },
}

/// Storage subcommands: list, get, set, delete, clear.
#[derive(Subcommand, Debug)]
pub(crate) enum StorageSubCmd {
    /// List all keys
    List {
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Get a value by key
    Get {
        /// Storage key
        key: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Set a key-value pair
    Set {
        /// Storage key
        key: String,
        /// Storage value
        value: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Delete a key
    Delete {
        /// Storage key
        key: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Clear all keys
    Clear {
        /// Optional key (ignored, clears all)
        key: Option<String>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
}

/// Wrapper — local-storage uses StorageSubCmd with Local kind.
pub(crate) type LocalStorageCmd = StorageSubCmd;
/// Wrapper — session-storage uses StorageSubCmd with Session kind.
pub(crate) type SessionStorageCmd = StorageSubCmd;

/// Wait subcommands: element, navigation, network-idle, condition.
#[derive(Subcommand, Debug)]
pub(crate) enum WaitCmd {
    /// Wait for an element to appear in the DOM
    Element {
        /// CSS selector
        selector: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Timeout in milliseconds (default: 30000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Wait for a navigation to complete
    Navigation {
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Timeout in milliseconds (default: 30000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Wait for network to become idle
    NetworkIdle {
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Timeout in milliseconds (default: 30000)
        #[arg(long)]
        timeout: Option<u64>,
        /// Idle time in milliseconds (default: 500)
        #[arg(long)]
        idle_time: Option<u64>,
    },
    /// Wait for a JS expression to become truthy
    Condition {
        /// JavaScript expression that should return a truthy value
        expression: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Timeout in milliseconds (default: 30000)
        #[arg(long)]
        timeout: Option<u64>,
    },
}
