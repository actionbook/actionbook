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
    fn browser_backend_display_and_default() {
        assert_eq!(BrowserBackend::default(), BrowserBackend::Cdp);
        assert_eq!(BrowserBackend::Cdp.to_string(), "cdp");
        assert_eq!(BrowserBackend::Camofox.to_string(), "camofox");
    }

    #[test]
    fn browser_backend_from_str_is_case_insensitive() {
        assert_eq!(
            "CDP".parse::<BrowserBackend>().unwrap(),
            BrowserBackend::Cdp
        );
        assert_eq!(
            "CamoFox".parse::<BrowserBackend>().unwrap(),
            BrowserBackend::Camofox
        );
        assert!("unknown".parse::<BrowserBackend>().is_err());
    }

    #[test]
    fn browser_backend_serde_uses_lowercase() {
        let json = serde_json::to_string(&BrowserBackend::Camofox).unwrap();
        assert_eq!(json, "\"camofox\"");
        let decoded: BrowserBackend = serde_json::from_str("\"cdp\"").unwrap();
        assert_eq!(decoded, BrowserBackend::Cdp);
    }
}
