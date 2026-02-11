//! Browser backend selection

use serde::{Deserialize, Serialize};

/// Browser backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserBackend {
    /// Chrome DevTools Protocol (Chrome, Brave, Edge)
    Cdp,
    /// Camoufox browser with anti-bot capabilities
    Camofox,
}

impl Default for BrowserBackend {
    fn default() -> Self {
        Self::Cdp
    }
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
