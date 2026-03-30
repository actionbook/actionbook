//! Browser backend selection

use serde::{Deserialize, Serialize};

/// Browser backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum BrowserBackend {
    /// Chrome DevTools Protocol (Chrome, Brave, Edge)
    #[default]
    Cdp,
    /// Camoufox browser with anti-bot capabilities
    Camofox,
}

impl std::fmt::Display for BrowserBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cdp => write!(f, "cdp"),
            Self::Camofox => write!(f, "camofox"),
        }
    }
}

impl std::str::FromStr for BrowserBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cdp" => Ok(Self::Cdp),
            "camofox" => Ok(Self::Camofox),
            _ => Err(format!("Unknown browser backend: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_backend_is_cdp() {
        assert_eq!(BrowserBackend::default(), BrowserBackend::Cdp);
    }

    #[test]
    fn display_formats_correctly() {
        assert_eq!(BrowserBackend::Cdp.to_string(), "cdp");
        assert_eq!(BrowserBackend::Camofox.to_string(), "camofox");
    }

    #[test]
    fn from_str_parses_valid_values() {
        assert_eq!(
            "cdp".parse::<BrowserBackend>().unwrap(),
            BrowserBackend::Cdp
        );
        assert_eq!(
            "camofox".parse::<BrowserBackend>().unwrap(),
            BrowserBackend::Camofox
        );
        // Case-insensitive
        assert_eq!(
            "CDP".parse::<BrowserBackend>().unwrap(),
            BrowserBackend::Cdp
        );
        assert_eq!(
            "CAMOFOX".parse::<BrowserBackend>().unwrap(),
            BrowserBackend::Camofox
        );
    }

    #[test]
    fn from_str_returns_error_for_unknown_value() {
        let err = "playwright".parse::<BrowserBackend>().unwrap_err();
        assert!(err.contains("playwright"));
    }

    #[test]
    fn serde_round_trip() {
        for backend in [BrowserBackend::Cdp, BrowserBackend::Camofox] {
            let json = serde_json::to_string(&backend).unwrap();
            let decoded: BrowserBackend = serde_json::from_str(&json).unwrap();
            assert_eq!(backend, decoded);
        }
    }

    #[test]
    fn serde_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&BrowserBackend::Cdp).unwrap(),
            "\"cdp\""
        );
        assert_eq!(
            serde_json::to_string(&BrowserBackend::Camofox).unwrap(),
            "\"camofox\""
        );
    }
}
