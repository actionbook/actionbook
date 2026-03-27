use crate::browser::BrowserBackend;
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

    /// Browser backend override for this profile
    pub backend: Option<BrowserBackend>,

    /// Camoufox port override for this profile
    pub camofox_port: Option<u16>,
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
            backend: None,
            camofox_port: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_config_has_expected_values() {
        let profile = ProfileConfig::default();
        assert_eq!(profile.cdp_port, 9222);
        assert!(!profile.headless);
        assert!(profile.user_data_dir.is_none());
        assert!(profile.browser_path.is_none());
        assert!(profile.cdp_url.is_none());
        assert!(profile.extra_args.is_empty());
        assert!(profile.backend.is_none());
        assert!(profile.camofox_port.is_none());
    }

    #[test]
    fn with_cdp_port_sets_port_and_keeps_defaults() {
        let profile = ProfileConfig::with_cdp_port(9333);
        assert_eq!(profile.cdp_port, 9333);
        assert!(!profile.headless);
        assert!(profile.user_data_dir.is_none());
    }

    #[test]
    fn remote_profile_sets_cdp_url() {
        let url = "ws://127.0.0.1:9222/json/version".to_string();
        let profile = ProfileConfig::remote(url.clone());
        assert_eq!(profile.cdp_url.as_deref(), Some(url.as_str()));
        assert_eq!(profile.cdp_port, 9222);
    }

    #[test]
    fn is_remote_returns_true_when_cdp_url_set() {
        let profile = ProfileConfig::remote("ws://host:1234".to_string());
        assert!(profile.is_remote());
    }

    #[test]
    fn is_remote_returns_false_for_default_profile() {
        let profile = ProfileConfig::default();
        assert!(!profile.is_remote());
    }

    #[test]
    fn profile_config_serde_round_trip() {
        let profile = ProfileConfig {
            cdp_port: 9300,
            headless: true,
            extra_args: vec!["--no-sandbox".into()],
            ..ProfileConfig::default()
        };
        let json = serde_json::to_string(&profile).unwrap();
        let decoded: ProfileConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cdp_port, 9300);
        assert!(decoded.headless);
        assert_eq!(decoded.extra_args, vec!["--no-sandbox"]);
    }
}
