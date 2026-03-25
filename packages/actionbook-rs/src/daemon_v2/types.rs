//! Core newtypes and enums for the v2 daemon protocol.
//!
//! Provides [`SessionId`], [`TabId`], [`WindowId`] newtypes with short-format
//! Display impls (s0, t0, w0), and the [`Mode`] enum for backend selection.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SessionId
// ---------------------------------------------------------------------------

/// Daemon-assigned short alias for a session (s0, s1, ...).
///
/// Monotonically increasing within a daemon instance. Used in wire protocol
/// for addressing; internally backed by a UUID for crash recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u32);

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "s{}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let num = s
            .strip_prefix('s')
            .ok_or(ParseIdError::MissingPrefix('s'))?
            .parse::<u32>()
            .map_err(ParseIdError::InvalidNumber)?;
        Ok(SessionId(num))
    }
}

// ---------------------------------------------------------------------------
// TabId
// ---------------------------------------------------------------------------

/// Daemon-assigned short alias for a tab within a session (t0, t1, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u32);

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "t{}", self.0)
    }
}

impl FromStr for TabId {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let num = s
            .strip_prefix('t')
            .ok_or(ParseIdError::MissingPrefix('t'))?
            .parse::<u32>()
            .map_err(ParseIdError::InvalidNumber)?;
        Ok(TabId(num))
    }
}

// ---------------------------------------------------------------------------
// WindowId
// ---------------------------------------------------------------------------

/// Daemon-assigned short alias for a browser window (w0, w1, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u32);

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "w{}", self.0)
    }
}

impl FromStr for WindowId {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let num = s
            .strip_prefix('w')
            .ok_or(ParseIdError::MissingPrefix('w'))?
            .parse::<u32>()
            .map_err(ParseIdError::InvalidNumber)?;
        Ok(WindowId(num))
    }
}

// ---------------------------------------------------------------------------
// Mode
// ---------------------------------------------------------------------------

/// Browser connection mode, determining which [`BrowserBackend`] to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Launch and control a local Chrome process via CDP over `ws://127.0.0.1`.
    Local,
    /// Connect to user's existing Chrome via the browser extension bridge.
    Extension,
    /// Connect to a remote browser via WSS endpoint.
    Cloud,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Local => write!(f, "local"),
            Mode::Extension => write!(f, "extension"),
            Mode::Cloud => write!(f, "cloud"),
        }
    }
}

// ---------------------------------------------------------------------------
// QueryMode
// ---------------------------------------------------------------------------

/// Query mode for element search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMode {
    /// CSS selector query.
    Css,
    /// XPath query.
    Xpath,
    /// Text content search.
    Text,
}

impl fmt::Display for QueryMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryMode::Css => write!(f, "css"),
            QueryMode::Xpath => write!(f, "xpath"),
            QueryMode::Text => write!(f, "text"),
        }
    }
}

// ---------------------------------------------------------------------------
// StorageKind
// ---------------------------------------------------------------------------

/// Web storage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageKind {
    /// `window.localStorage`
    Local,
    /// `window.sessionStorage`
    Session,
}

impl fmt::Display for StorageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageKind::Local => write!(f, "local"),
            StorageKind::Session => write!(f, "session"),
        }
    }
}

// ---------------------------------------------------------------------------
// SameSite
// ---------------------------------------------------------------------------

/// Cookie SameSite attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SameSite::Strict => write!(f, "Strict"),
            SameSite::Lax => write!(f, "Lax"),
            SameSite::None => write!(f, "None"),
        }
    }
}

// ---------------------------------------------------------------------------
// ParseIdError
// ---------------------------------------------------------------------------

/// Error returned when parsing a short ID string (e.g. "s0", "t1", "w2").
#[derive(Debug, Clone)]
pub enum ParseIdError {
    /// The string did not start with the expected prefix character.
    MissingPrefix(char),
    /// The numeric suffix could not be parsed.
    InvalidNumber(std::num::ParseIntError),
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseIdError::MissingPrefix(c) => write!(f, "expected prefix '{c}'"),
            ParseIdError::InvalidNumber(e) => write!(f, "invalid number: {e}"),
        }
    }
}

impl std::error::Error for ParseIdError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_display() {
        assert_eq!(SessionId(0).to_string(), "s0");
        assert_eq!(SessionId(42).to_string(), "s42");
    }

    #[test]
    fn session_id_parse() {
        assert_eq!("s0".parse::<SessionId>().unwrap(), SessionId(0));
        assert_eq!("s99".parse::<SessionId>().unwrap(), SessionId(99));
        assert!("t0".parse::<SessionId>().is_err());
        assert!("sABC".parse::<SessionId>().is_err());
    }

    #[test]
    fn tab_id_display_and_parse() {
        assert_eq!(TabId(3).to_string(), "t3");
        assert_eq!("t3".parse::<TabId>().unwrap(), TabId(3));
    }

    #[test]
    fn window_id_display_and_parse() {
        assert_eq!(WindowId(1).to_string(), "w1");
        assert_eq!("w1".parse::<WindowId>().unwrap(), WindowId(1));
    }

    #[test]
    fn session_id_serde_round_trip() {
        let id = SessionId(7);
        let json = serde_json::to_string(&id).unwrap();
        let decoded: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, decoded);
    }

    #[test]
    fn tab_id_serde_round_trip() {
        let id = TabId(12);
        let json = serde_json::to_string(&id).unwrap();
        let decoded: TabId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, decoded);
    }

    #[test]
    fn window_id_serde_round_trip() {
        let id = WindowId(0);
        let json = serde_json::to_string(&id).unwrap();
        let decoded: WindowId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, decoded);
    }

    #[test]
    fn mode_serde_round_trip() {
        for mode in [Mode::Local, Mode::Extension, Mode::Cloud] {
            let json = serde_json::to_string(&mode).unwrap();
            let decoded: Mode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, decoded);
        }
    }

    #[test]
    fn mode_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&Mode::Local).unwrap(), "\"local\"");
        assert_eq!(
            serde_json::to_string(&Mode::Extension).unwrap(),
            "\"extension\""
        );
        assert_eq!(serde_json::to_string(&Mode::Cloud).unwrap(), "\"cloud\"");
    }
}
