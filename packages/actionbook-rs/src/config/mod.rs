mod profile;

pub use profile::ProfileConfig;

use std::collections::HashMap;
use std::path::PathBuf;

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};

use crate::error::{ActionbookError, Result};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// API configuration
    #[serde(default)]
    pub api: ApiConfig,

    /// Browser configuration
    #[serde(default)]
    pub browser: BrowserConfig,

    /// Named profiles
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API base URL
    #[serde(default = "default_api_url")]
    pub base_url: String,

    /// API key
    pub api_key: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: default_api_url(),
            api_key: None,
        }
    }
}

fn default_api_url() -> String {
    "https://api.actionbook.dev".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// Browser executable path (overrides auto-discovery)
    pub executable: Option<String>,

    /// Default profile name
    #[serde(default = "default_profile_name")]
    pub default_profile: String,

    /// Default headless mode
    #[serde(default)]
    pub headless: bool,

    /// Use an isolated Chrome profile for extension bridge
    #[serde(default)]
    pub extension_isolated_profile: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            executable: None,
            default_profile: default_profile_name(),
            headless: false,
            extension_isolated_profile: false,
        }
    }
}

fn default_profile_name() -> String {
    "actionbook".to_string()
}

fn normalize_default_profile_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        default_profile_name()
    } else {
        trimmed.to_string()
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert(default_profile_name(), ProfileConfig::default());

        Self {
            api: ApiConfig::default(),
            browser: BrowserConfig::default(),
            profiles,
        }
    }
}

impl Config {
    pub fn effective_default_profile_name(&self) -> String {
        normalize_default_profile_name(&self.browser.default_profile)
    }

    /// Load configuration from all sources (file, env, defaults)
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        let config: Config = Figment::new()
            // Start with defaults
            .merge(Serialized::defaults(Config::default()))
            // Merge config file if exists
            .merge(Toml::file(&config_path))
            // Merge environment variables (ACTIONBOOK_*)
            .merge(Env::prefixed("ACTIONBOOK_").split("_"))
            .extract()
            .map_err(|e| ActionbookError::ConfigError(e.to_string()))?;

        Ok(config)
    }

    /// Get the configuration file path
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("actionbook")
            .join("config.toml")
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| ActionbookError::ConfigError(e.to_string()))?;

        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get a profile by name, falling back to default
    pub fn get_profile(&self, name: &str) -> Result<ProfileConfig> {
        let normalized_name = name.trim();

        // First check named profiles
        if let Some(profile) = self.profiles.get(normalized_name) {
            return Ok(profile.clone());
        }

        // If asking for configured default profile and it doesn't exist, create an implicit one.
        if normalized_name == self.effective_default_profile_name() {
            let mut profile = ProfileConfig::default();

            // Apply browser config defaults
            if let Some(ref exe) = self.browser.executable {
                profile.browser_path = Some(exe.clone());
            }
            profile.headless = self.browser.headless;

            return Ok(profile);
        }

        Err(ActionbookError::ProfileNotFound(
            normalized_name.to_string(),
        ))
    }

    /// Add or update a profile
    pub fn set_profile(&mut self, name: &str, profile: ProfileConfig) {
        self.profiles.insert(name.to_string(), profile);
    }

    /// Remove a profile
    pub fn remove_profile(&mut self, name: &str) -> Result<()> {
        let normalized_name = name.trim();

        if normalized_name == self.effective_default_profile_name() {
            return Err(ActionbookError::ConfigError(
                "Cannot remove the default profile".to_string(),
            ));
        }

        self.profiles
            .remove(normalized_name)
            .ok_or_else(|| ActionbookError::ProfileNotFound(normalized_name.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_actionbook_profile() {
        let config = Config::default();

        assert_eq!(config.browser.default_profile, "actionbook");
        assert!(config.profiles.contains_key("actionbook"));
    }

    #[test]
    fn get_profile_returns_implicit_configured_default_profile() {
        let config = Config {
            api: ApiConfig::default(),
            browser: BrowserConfig {
                executable: Some("/Applications/Google Chrome.app".to_string()),
                default_profile: "team".to_string(),
                headless: true,
                extension_isolated_profile: false,
            },
            profiles: HashMap::new(),
        };

        let profile = config.get_profile("team").unwrap();
        assert_eq!(
            profile.browser_path.as_deref(),
            Some("/Applications/Google Chrome.app")
        );
        assert!(profile.headless);
    }

    #[test]
    fn get_profile_returns_error_for_non_default_missing_profile() {
        let config = Config::default();
        let result = config.get_profile("missing-profile");

        assert!(matches!(
            result,
            Err(ActionbookError::ProfileNotFound(name)) if name == "missing-profile"
        ));
    }

    #[test]
    fn get_profile_uses_actionbook_when_config_default_is_blank() {
        let config = Config {
            api: ApiConfig::default(),
            browser: BrowserConfig {
                executable: None,
                default_profile: "   ".to_string(),
                headless: false,
                extension_isolated_profile: false,
            },
            profiles: HashMap::new(),
        };

        assert!(config.get_profile("actionbook").is_ok());
    }

    #[test]
    fn remove_profile_blocks_configured_default_profile() {
        let mut config = Config::default();
        config.browser.default_profile = "team".to_string();
        config.set_profile("team", ProfileConfig::default());

        let result = config.remove_profile("team");
        assert!(matches!(result, Err(ActionbookError::ConfigError(_))));
    }

    #[test]
    fn remove_profile_blocks_actionbook_when_config_default_is_blank() {
        let mut config = Config::default();
        config.browser.default_profile = "  ".to_string();
        config.set_profile("actionbook", ProfileConfig::default());

        let result = config.remove_profile("actionbook");
        assert!(matches!(result, Err(ActionbookError::ConfigError(_))));
    }

    #[test]
    fn extension_isolated_profile_defaults_to_false() {
        let toml_str = r#"
default_profile = "actionbook"
headless = false
"#;
        let browser: BrowserConfig = toml::from_str(toml_str).unwrap();
        assert!(!browser.extension_isolated_profile);
    }

    #[test]
    fn extension_isolated_profile_round_trip() {
        let browser = BrowserConfig {
            executable: None,
            default_profile: "actionbook".to_string(),
            headless: false,
            extension_isolated_profile: true,
        };
        let serialized = toml::to_string(&browser).unwrap();
        let deserialized: BrowserConfig = toml::from_str(&serialized).unwrap();
        assert!(deserialized.extension_isolated_profile);
    }
}
