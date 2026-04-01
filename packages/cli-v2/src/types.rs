use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Semantic session identifier.
/// Auto-generated format: `sN` (e.g. `s1`, `s2`, `s3`). Global counter, mode-agnostic.
/// Manual format (--set-session-id): `^[a-z][a-z0-9-]{1,63}$`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ParseIdError> {
        let id = id.into();
        if !Self::is_valid(&id) {
            return Err(ParseIdError::InvalidSessionId(id));
        }
        Ok(SessionId(id))
    }

    fn is_valid(id: &str) -> bool {
        if id.len() < 2 || id.len() > 64 {
            return false;
        }
        // All session IDs: lowercase start, then lowercase/digit/hyphen.
        // Auto-generated `sN` (e.g. s1, s42) satisfies this naturally.
        let bytes = id.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return false;
        }
        bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }

    pub fn new_unchecked(id: impl Into<String>) -> Self {
        SessionId(id.into())
    }

    /// Generate `sN` (e.g. `s1`, `s2`) — global counter, mode-agnostic.
    /// `n` is the 1-based counter value (already computed by the registry).
    pub fn auto_generate(n: u32) -> Self {
        SessionId(format!("s{n}"))
    }

    pub fn from_profile(profile: &str, suffix: u32) -> Self {
        let sanitized = Self::sanitize_profile(profile);
        let max_base = if suffix == 0 {
            64
        } else {
            let suffix_str = format!("-{}", suffix + 1);
            64 - suffix_str.len()
        };
        let mut base = if sanitized.len() > max_base {
            let mut s = sanitized[..max_base].to_string();
            while s.ends_with('-') {
                s.pop();
            }
            s
        } else {
            sanitized
        };
        if suffix > 0 {
            base = format!("{}-{}", base, suffix + 1);
        }
        if Self::is_valid(&base) {
            SessionId(base)
        } else {
            Self::auto_generate(suffix + 1)
        }
    }

    fn sanitize_profile(profile: &str) -> String {
        let lowered = profile.to_lowercase();
        let mapped: String = lowered
            .chars()
            .map(|c| {
                if c.is_ascii_lowercase() || c.is_ascii_digit() {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let mut result = String::with_capacity(mapped.len());
        let mut prev_hyphen = true;
        for c in mapped.chars() {
            if c == '-' {
                if !prev_hyphen {
                    result.push('-');
                }
                prev_hyphen = true;
            } else {
                result.push(c);
                prev_hyphen = false;
            }
        }
        if result.ends_with('-') {
            result.pop();
        }
        result
    }

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

/// Tab ID — Chrome's native CDP target ID (opaque string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub String);

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TabId {
    type Err = ParseIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(ParseIdError::InvalidSessionId("empty tab id".to_string()));
        }
        Ok(TabId(s.to_string()))
    }
}

/// Window ID (w0, w1, ...).
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Local,
    Extension,
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

impl FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(Mode::Local),
            "extension" => Ok(Mode::Extension),
            "cloud" => Ok(Mode::Cloud),
            _ => Err(format!("unknown mode: {s}")),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParseIdError {
    MissingPrefix(char),
    InvalidNumber(std::num::ParseIntError),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_accepts_auto_generated_format() {
        assert_eq!(
            SessionId::new("s1").expect("s1 should be valid"),
            SessionId::new_unchecked("s1")
        );
        assert_eq!(
            SessionId::new("s42").expect("s42 should be valid"),
            SessionId::new_unchecked("s42")
        );
        assert_eq!(
            SessionId::new("s9999").expect("s9999 should be valid"),
            SessionId::new_unchecked("s9999")
        );
    }

    #[test]
    fn session_id_rejects_old_uppercase_prefixes() {
        assert!(SessionId::new("SLOCAL-1").is_err());
        assert!(SessionId::new("SCLOUD-42").is_err());
        assert!(SessionId::new("SEXT-9999").is_err());
    }
}
