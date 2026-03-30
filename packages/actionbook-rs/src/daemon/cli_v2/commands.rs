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
        /// CSS selector of the element to scroll within (defaults to page)
        #[arg(long)]
        container: Option<String>,
    },
    /// Scroll down
    Down {
        /// Amount in pixels (default: 300)
        amount: Option<i32>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// CSS selector of the element to scroll within (defaults to page)
        #[arg(long)]
        container: Option<String>,
    },
    /// Scroll left
    Left {
        /// Amount in pixels (default: 300)
        amount: Option<i32>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// CSS selector of the element to scroll within (defaults to page)
        #[arg(long)]
        container: Option<String>,
    },
    /// Scroll right
    Right {
        /// Amount in pixels (default: 300)
        amount: Option<i32>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// CSS selector of the element to scroll within (defaults to page)
        #[arg(long)]
        container: Option<String>,
    },
    /// Scroll to top of page
    Top {
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// CSS selector of the element to scroll within (defaults to page)
        #[arg(long)]
        container: Option<String>,
    },
    /// Scroll to bottom of page
    Bottom {
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// CSS selector of the element to scroll within (defaults to page)
        #[arg(long)]
        container: Option<String>,
    },
    /// Scroll an element into view
    IntoView {
        /// CSS selector
        selector: String,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
        /// Alignment for scrollIntoView: start, center, end, nearest
        #[arg(long, value_parser = ["start", "center", "end", "nearest"])]
        align: Option<String>,
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
        /// Timeout in milliseconds (overrides global --timeout)
        #[arg(long = "timeout")]
        timeout_ms: Option<u64>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Wait for a navigation to complete
    Navigation {
        /// Timeout in milliseconds (overrides global --timeout)
        #[arg(long = "timeout")]
        timeout_ms: Option<u64>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Wait for network to become idle
    NetworkIdle {
        /// Timeout in milliseconds (overrides global --timeout)
        #[arg(long = "timeout")]
        timeout_ms: Option<u64>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
    /// Wait for a JS expression to become truthy
    Condition {
        /// JavaScript expression that should return a truthy value
        expression: String,
        /// Timeout in milliseconds (overrides global --timeout)
        #[arg(long = "timeout")]
        timeout_ms: Option<u64>,
        #[arg(short = 's', long)]
        session: SessionId,
        #[arg(short = 't', long)]
        tab: TabId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_mode_maps_to_protocol_mode() {
        assert_eq!(Mode::from(CliMode::Local), Mode::Local);
        assert_eq!(Mode::from(CliMode::Extension), Mode::Extension);
        assert_eq!(Mode::from(CliMode::Cloud), Mode::Cloud);
    }

    #[test]
    fn cli_query_mode_maps_to_protocol_query_mode() {
        assert_eq!(QueryMode::from(CliQueryMode::Css), QueryMode::Css);
        assert_eq!(QueryMode::from(CliQueryMode::Xpath), QueryMode::Xpath);
        assert_eq!(QueryMode::from(CliQueryMode::Text), QueryMode::Text);
    }

    #[test]
    fn cli_storage_kind_maps_to_protocol_storage_kind() {
        assert_eq!(StorageKind::from(CliStorageKind::Local), StorageKind::Local);
        assert_eq!(
            StorageKind::from(CliStorageKind::Session),
            StorageKind::Session
        );
    }

    #[test]
    fn cli_same_site_maps_to_protocol_same_site() {
        assert_eq!(SameSite::from(CliSameSite::Strict), SameSite::Strict);
        assert_eq!(SameSite::from(CliSameSite::Lax), SameSite::Lax);
        assert_eq!(SameSite::from(CliSameSite::None), SameSite::None);
    }
}
