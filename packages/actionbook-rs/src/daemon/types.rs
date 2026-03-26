//! Core newtypes and enums for the v2 daemon protocol.
//!
//! Provides [`SessionId`], [`TabId`], [`WindowId`] newtypes with short-format
//! Display impls, and the [`Mode`] enum for backend selection.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SessionId
// ---------------------------------------------------------------------------

/// Semantic session identifier (e.g. "local-1", "research-google").
///
/// Validated against `^[a-z][a-z0-9-]{1,63}$` — lowercase alphanumeric
/// with hyphens, 2–64 characters, starting with a letter.
/// Auto-generated as "local-1", "local-2", ... when not explicitly set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    /// Create a new SessionId after validation.
    ///
    /// Must match `^[a-z][a-z0-9-]{1,63}$`.
    pub fn new(id: impl Into<String>) -> Result<Self, ParseIdError> {
        let id = id.into();
        if !Self::is_valid(&id) {
            return Err(ParseIdError::InvalidSessionId(id));
        }
        Ok(SessionId(id))
    }

    /// Validate a session ID string.
    fn is_valid(id: &str) -> bool {
        if id.len() < 2 || id.len() > 64 {
            return false;
        }
        let bytes = id.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return false;
        }
        bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }

    /// Create a SessionId without validation (for internal use).
    pub(crate) fn new_unchecked(id: impl Into<String>) -> Self {
        SessionId(id.into())
    }

    /// Generate an auto-incremented session ID: local-1, local-2, ...
    pub fn auto_generate(n: u32) -> Self {
        SessionId(format!("local-{}", n + 1))
    }

    /// Generate a session ID from a profile name.
    /// First attempt uses the profile name directly; collisions add -2, -3, etc.
    pub fn from_profile(profile: &str, suffix: u32) -> Self {
        if suffix == 0 {
            SessionId(profile.to_string())
        } else {
            SessionId(format!("{}-{}", profile, suffix + 1))
        }
    }

    /// Returns the string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SessionId::new(s)
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
// QueryCardinality
// ---------------------------------------------------------------------------

/// Cardinality mode for the `query` command (PRD §10.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryCardinality {
    /// Expect exactly one match.
    One,
    /// Return all matches.
    All,
    /// Return only the match count.
    Count,
    /// Return the nth match (1-based).
    Nth,
}

impl fmt::Display for QueryCardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryCardinality::One => write!(f, "one"),
            QueryCardinality::All => write!(f, "all"),
            QueryCardinality::Count => write!(f, "count"),
            QueryCardinality::Nth => write!(f, "nth"),
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

/// Error returned when parsing a short ID string.
#[derive(Debug, Clone)]
pub enum ParseIdError {
    /// The string did not start with the expected prefix character.
    MissingPrefix(char),
    /// The numeric suffix could not be parsed.
    InvalidNumber(std::num::ParseIntError),
    /// The session ID does not match the required format.
    InvalidSessionId(String),
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseIdError::MissingPrefix(c) => write!(f, "expected prefix '{c}'"),
            ParseIdError::InvalidNumber(e) => write!(f, "invalid number: {e}"),
            ParseIdError::InvalidSessionId(id) => write!(
                f,
                "invalid session id '{id}': must match ^[a-z][a-z0-9-]{{1,63}}$"
            ),
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
        assert_eq!(SessionId::new_unchecked("local-1").to_string(), "local-1");
        assert_eq!(
            SessionId::new_unchecked("research-google").to_string(),
            "research-google"
        );
    }

    #[test]
    fn session_id_parse() {
        assert_eq!(
            "local-1".parse::<SessionId>().unwrap(),
            SessionId::new_unchecked("local-1")
        );
        assert_eq!(
            "research-google".parse::<SessionId>().unwrap(),
            SessionId::new_unchecked("research-google")
        );
        // Invalid: starts with number
        assert!("0local".parse::<SessionId>().is_err());
        // Invalid: uppercase
        assert!("Local-1".parse::<SessionId>().is_err());
        // Invalid: empty
        assert!("".parse::<SessionId>().is_err());
    }

    #[test]
    fn session_id_validation() {
        // Valid
        assert!(SessionId::new("a").is_err());
        assert!(SessionId::new("local-1").is_ok());
        assert!(SessionId::new("research-google").is_ok());
        assert!(SessionId::new("my-session-123").is_ok());

        // Invalid
        assert!(SessionId::new("").is_err());
        assert!(SessionId::new("1abc").is_err());
        assert!(SessionId::new("ABC").is_err());
        assert!(SessionId::new("has_underscore").is_err());
        assert!(SessionId::new("has space").is_err());
        assert!(SessionId::new("-starts-with-dash").is_err());
    }

    #[test]
    fn session_id_auto_generate() {
        assert_eq!(SessionId::auto_generate(0).to_string(), "local-1");
        assert_eq!(SessionId::auto_generate(1).to_string(), "local-2");
        assert_eq!(SessionId::auto_generate(41).to_string(), "local-42");
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
        let id = SessionId::new_unchecked("local-1");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"local-1\"");
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
