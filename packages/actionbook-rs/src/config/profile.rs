use serde::{Deserialize, Serialize};

/// Profile configuration for a browser session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// CDP port for this profile
    #[serde(default = "default_cdp_port")]
    pub cdp_port: u16,

    /// User data directory for this profile
    pub user_data_dir: Option<String>,

    /// Browser executable path (profile-specific override)
    pub browser_path: Option<String>,

    /// Headless mode
    #[serde(default)]
    pub headless: bool,

    /// CDP WebSocket URL (for remote connections)
    pub cdp_url: Option<String>,

    /// Extra browser arguments
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn default_cdp_port() -> u16 {
    9222
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            cdp_port: default_cdp_port(),
            user_data_dir: None,
            browser_path: None,
            headless: false,
            cdp_url: None,
            extra_args: Vec::new(),
        }
    }
}

impl ProfileConfig {
    /// Create a new profile with a specific CDP port
    pub fn with_cdp_port(port: u16) -> Self {
        Self {
            cdp_port: port,
            ..Default::default()
        }
    }

    /// Create a profile for remote connection
    #[allow(dead_code)]
    pub fn remote(cdp_url: String) -> Self {
        Self {
            cdp_url: Some(cdp_url),
            ..Default::default()
        }
    }

    /// Check if this is a remote profile
    pub fn is_remote(&self) -> bool {
        self.cdp_url.is_some()
    }
}
