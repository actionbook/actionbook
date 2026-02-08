use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::sleep;

use super::discovery::{discover_browser, BrowserInfo};
use crate::config::ProfileConfig;
use crate::error::{ActionbookError, Result};

/// Browser launcher that starts a browser with CDP enabled
pub struct BrowserLauncher {
    browser_info: BrowserInfo,
    profile_name: String,
    cdp_port: u16,
    headless: bool,
    stealth: bool,
    user_data_dir: PathBuf,
    extra_args: Vec<String>,
}

impl BrowserLauncher {
    const ACTIONBOOK_PROFILE_NAME: &'static str = "actionbook";
    const DEFAULT_CHROME_PROFILE_NAME: &'static str = "Your Chrome";

    fn default_user_data_dir(profile_name: &str) -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("actionbook")
            .join("profiles")
            .join(profile_name)
    }

    fn resolve_user_data_dir(profile_name: &str, configured_dir: Option<&str>) -> PathBuf {
        configured_dir
            .map(|dir| PathBuf::from(shellexpand::tilde(dir).to_string()))
            .unwrap_or_else(|| Self::default_user_data_dir(profile_name))
    }

    /// Create a new launcher with default settings
    pub fn new() -> Result<Self> {
        let browser_info = discover_browser()?;
        let data_dir = Self::default_user_data_dir(Self::ACTIONBOOK_PROFILE_NAME);

        Ok(Self {
            browser_info,
            profile_name: Self::ACTIONBOOK_PROFILE_NAME.to_string(),
            cdp_port: 9222,
            headless: false,
            stealth: false,
            user_data_dir: data_dir,
            extra_args: Vec::new(),
        })
    }

    /// Create a launcher with a specific browser path
    pub fn with_browser_path(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Err(ActionbookError::BrowserLaunchFailed(format!(
                "Browser not found at: {:?}",
                path
            )));
        }

        let browser_info = BrowserInfo::new(
            super::discovery::BrowserType::Chrome, // Assume Chrome-compatible
            path,
        );

        let data_dir = Self::default_user_data_dir(Self::ACTIONBOOK_PROFILE_NAME);

        Ok(Self {
            browser_info,
            profile_name: Self::ACTIONBOOK_PROFILE_NAME.to_string(),
            cdp_port: 9222,
            headless: false,
            stealth: false,
            user_data_dir: data_dir,
            extra_args: Vec::new(),
        })
    }

    /// Create a launcher from profile configuration
    pub fn from_profile(profile_name: &str, profile: &ProfileConfig) -> Result<Self> {
        let mut launcher = if let Some(ref path) = profile.browser_path {
            Self::with_browser_path(PathBuf::from(path))?
        } else {
            Self::new()?
        };

        launcher.profile_name = profile_name.to_string();
        launcher.cdp_port = profile.cdp_port;
        launcher.headless = profile.headless;
        launcher.user_data_dir =
            Self::resolve_user_data_dir(profile_name, profile.user_data_dir.as_deref());

        Ok(launcher)
    }

    /// Enable stealth mode (anti-detection Chrome flags)
    pub fn with_stealth(mut self, stealth: bool) -> Self {
        self.stealth = stealth;
        self
    }

    /// Set CDP port
    #[allow(dead_code)]
    pub fn cdp_port(mut self, port: u16) -> Self {
        self.cdp_port = port;
        self
    }

    /// Set headless mode
    #[allow(dead_code)]
    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// Set user data directory
    #[allow(dead_code)]
    pub fn user_data_dir(mut self, dir: PathBuf) -> Self {
        self.user_data_dir = dir;
        self
    }

    /// Add extra browser arguments
    #[allow(dead_code)]
    pub fn extra_args(mut self, args: Vec<String>) -> Self {
        self.extra_args = args;
        self
    }

    /// Build the browser launch arguments
    fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            format!("--remote-debugging-port={}", self.cdp_port),
            format!("--user-data-dir={}", self.user_data_dir.display()),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
        ];

        // Always apply anti-detection flags
        args.push("--disable-blink-features=AutomationControlled".to_string());
        args.push("--disable-infobars".to_string());
        args.push("--window-size=1920,1080".to_string());
        args.push("--disable-save-password-bubble".to_string());
        args.push("--disable-translate".to_string());

        if self.headless {
            args.push("--headless=new".to_string());
        }

        // Add extra args
        args.extend(self.extra_args.clone());

        args
    }

    fn read_json_or_default(path: &std::path::Path) -> Result<Value> {
        if !path.exists() {
            return Ok(json!({}));
        }

        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| {
            ActionbookError::Other(format!(
                "Failed to parse JSON file {}: {}",
                path.display(),
                e
            ))
        })
    }

    fn write_json(path: &std::path::Path, value: &Value) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| ActionbookError::Other(format!("Failed to serialize JSON: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn is_customized_profile_name(name: &str) -> bool {
        let normalized = name.trim();
        !normalized.is_empty()
            && normalized != Self::ACTIONBOOK_PROFILE_NAME
            && normalized != Self::DEFAULT_CHROME_PROFILE_NAME
    }

    fn extract_local_state_profile_name(local_state: &Value) -> Option<&str> {
        local_state
            .get("profile")
            .and_then(|p| p.get("info_cache"))
            .and_then(|c| c.get("Default"))
            .and_then(|d| d.get("name"))
            .and_then(Value::as_str)
    }

    fn extract_preferences_profile_name(preferences: &Value) -> Option<&str> {
        preferences
            .get("profile")
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str)
    }

    fn should_preserve_existing_profile_name(local_state: &Value, preferences: &Value) -> bool {
        let local_name = Self::extract_local_state_profile_name(local_state);
        let prefs_name = Self::extract_preferences_profile_name(preferences);

        [local_name, prefs_name]
            .into_iter()
            .flatten()
            .any(Self::is_customized_profile_name)
    }

    fn ensure_object(value: &mut Value) -> &mut serde_json::Map<String, Value> {
        if !value.is_object() {
            *value = json!({});
        }
        value.as_object_mut().expect("object ensured")
    }

    fn apply_actionbook_profile_name(local_state: &mut Value, preferences: &mut Value) {
        let root = Self::ensure_object(local_state);
        let profile = root
            .entry("profile".to_string())
            .or_insert_with(|| json!({}));
        let profile_obj = Self::ensure_object(profile);
        let info_cache = profile_obj
            .entry("info_cache".to_string())
            .or_insert_with(|| json!({}));
        let info_cache_obj = Self::ensure_object(info_cache);
        let default_profile = info_cache_obj
            .entry("Default".to_string())
            .or_insert_with(|| json!({}));
        let default_profile_obj = Self::ensure_object(default_profile);
        default_profile_obj.insert("name".to_string(), json!(Self::ACTIONBOOK_PROFILE_NAME));
        default_profile_obj.insert("is_using_default_name".to_string(), json!(false));

        let prefs_root = Self::ensure_object(preferences);
        let prefs_profile = prefs_root
            .entry("profile".to_string())
            .or_insert_with(|| json!({}));
        let prefs_profile_obj = Self::ensure_object(prefs_profile);
        prefs_profile_obj.insert("name".to_string(), json!(Self::ACTIONBOOK_PROFILE_NAME));
    }

    fn ensure_actionbook_profile_display_name(&self) -> Result<()> {
        if self.profile_name != Self::ACTIONBOOK_PROFILE_NAME {
            return Ok(());
        }

        let local_state_path = self.user_data_dir.join("Local State");
        let preferences_path = self.user_data_dir.join("Default").join("Preferences");

        let mut local_state = Self::read_json_or_default(&local_state_path)?;
        let mut preferences = Self::read_json_or_default(&preferences_path)?;

        if Self::should_preserve_existing_profile_name(&local_state, &preferences) {
            return Ok(());
        }

        Self::apply_actionbook_profile_name(&mut local_state, &mut preferences);
        Self::write_json(&local_state_path, &local_state)?;
        Self::write_json(&preferences_path, &preferences)?;
        Ok(())
    }

    /// Launch the browser and return the process handle
    pub fn launch(&self) -> Result<Child> {
        // Ensure user data directory exists
        std::fs::create_dir_all(&self.user_data_dir)?;
        if let Err(e) = self.ensure_actionbook_profile_display_name() {
            tracing::warn!("Failed to set actionbook profile display name: {}", e);
        }

        let args = self.build_args();

        tracing::debug!(
            "Launching browser: {:?} with args: {:?}",
            self.browser_info.path,
            args
        );

        let child = Command::new(&self.browser_info.path)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                ActionbookError::BrowserLaunchFailed(format!(
                    "Failed to launch {}: {}",
                    self.browser_info.browser_type.name(),
                    e
                ))
            })?;

        Ok(child)
    }

    /// Launch the browser and wait for CDP to be ready
    pub async fn launch_and_wait(&self) -> Result<(Child, String)> {
        let child = self.launch()?;

        // Wait for CDP to be ready
        let cdp_url = self.wait_for_cdp().await?;

        Ok((child, cdp_url))
    }

    /// Wait for CDP endpoint to be ready
    async fn wait_for_cdp(&self) -> Result<String> {
        let url = format!("http://127.0.0.1:{}/json/version", self.cdp_port);

        // Build client with NO_PROXY for localhost
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        // Try for up to 10 seconds
        for i in 0..20 {
            sleep(Duration::from_millis(500)).await;

            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    let json: serde_json::Value = response.json().await.map_err(|e| {
                        ActionbookError::CdpConnectionFailed(format!(
                            "Failed to parse CDP response: {}",
                            e
                        ))
                    })?;

                    if let Some(ws_url) = json.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
                    {
                        tracing::info!("CDP ready at: {}", ws_url);
                        return Ok(ws_url.to_string());
                    }
                }
                Ok(_) => {
                    tracing::debug!("CDP not ready yet (attempt {})", i + 1);
                }
                Err(e) => {
                    tracing::debug!("CDP connection attempt {} failed: {}", i + 1, e);
                }
            }
        }

        Err(ActionbookError::CdpConnectionFailed(
            "Timeout waiting for CDP to be ready".to_string(),
        ))
    }

    /// Get the CDP WebSocket URL for an already running browser
    #[allow(dead_code)]
    pub async fn get_cdp_url(&self) -> Result<String> {
        let url = format!("http://127.0.0.1:{}/json/version", self.cdp_port);

        // Build client with NO_PROXY for localhost
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let response = client.get(&url).send().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Failed to connect to CDP: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(ActionbookError::BrowserNotRunning);
        }

        let json: serde_json::Value = response.json().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Failed to parse CDP response: {}", e))
        })?;

        json.get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                ActionbookError::CdpConnectionFailed("No WebSocket URL in CDP response".to_string())
            })
    }

    /// Get browser info
    #[allow(dead_code)]
    pub fn browser_info(&self) -> &BrowserInfo {
        &self.browser_info
    }

    /// Get CDP port
    pub fn get_cdp_port(&self) -> u16 {
        self.cdp_port
    }
}

impl Default for BrowserLauncher {
    fn default() -> Self {
        Self::new().expect("Failed to create default browser launcher")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::BrowserType;
    use std::path::PathBuf;

    fn test_launcher_with_user_data_dir(dir: PathBuf) -> BrowserLauncher {
        BrowserLauncher {
            browser_info: BrowserInfo::new(BrowserType::Chrome, PathBuf::new()),
            profile_name: BrowserLauncher::ACTIONBOOK_PROFILE_NAME.to_string(),
            cdp_port: 9222,
            headless: false,
            stealth: false,
            user_data_dir: dir,
            extra_args: Vec::new(),
        }
    }

    #[test]
    fn default_profile_user_data_dir_uses_profile_name() {
        let dir = BrowserLauncher::resolve_user_data_dir("work", None);
        let components: Vec<String> = dir
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        let tail = &components[components.len() - 3..];
        assert_eq!(tail, ["actionbook", "profiles", "work"]);
    }

    #[test]
    fn configured_user_data_dir_takes_precedence() {
        let dir = BrowserLauncher::resolve_user_data_dir("work", Some(".custom-profile"));
        let launcher = test_launcher_with_user_data_dir(dir.clone());
        let args = launcher.build_args();

        assert_eq!(dir, PathBuf::from(".custom-profile"));
        assert!(args.contains(&format!("--user-data-dir={}", dir.display())));
    }

    #[test]
    fn ensure_actionbook_profile_display_name_sets_name_for_default_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let launcher = test_launcher_with_user_data_dir(tmp.path().to_path_buf());
        launcher.ensure_actionbook_profile_display_name().unwrap();

        let local_state_path = tmp.path().join("Local State");
        let preferences_path = tmp.path().join("Default").join("Preferences");

        let local_state: Value =
            serde_json::from_str(&std::fs::read_to_string(local_state_path).unwrap()).unwrap();
        let preferences: Value =
            serde_json::from_str(&std::fs::read_to_string(preferences_path).unwrap()).unwrap();

        assert_eq!(
            BrowserLauncher::extract_local_state_profile_name(&local_state),
            Some("actionbook")
        );
        assert_eq!(
            BrowserLauncher::extract_preferences_profile_name(&preferences),
            Some("actionbook")
        );
    }

    #[test]
    fn ensure_actionbook_profile_display_name_preserves_customized_name() {
        let tmp = tempfile::tempdir().unwrap();
        let launcher = test_launcher_with_user_data_dir(tmp.path().to_path_buf());

        let local_state_path = tmp.path().join("Local State");
        let preferences_path = tmp.path().join("Default").join("Preferences");
        std::fs::create_dir_all(preferences_path.parent().unwrap()).unwrap();
        std::fs::write(
            &local_state_path,
            serde_json::to_string_pretty(&json!({
                "profile": {
                    "info_cache": {
                        "Default": {
                            "name": "My Browser"
                        }
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            &preferences_path,
            serde_json::to_string_pretty(&json!({
                "profile": {
                    "name": "My Browser"
                }
            }))
            .unwrap(),
        )
        .unwrap();

        launcher.ensure_actionbook_profile_display_name().unwrap();

        let local_state_after: Value =
            serde_json::from_str(&std::fs::read_to_string(local_state_path).unwrap()).unwrap();
        let preferences_after: Value =
            serde_json::from_str(&std::fs::read_to_string(preferences_path).unwrap()).unwrap();
        assert_eq!(
            BrowserLauncher::extract_local_state_profile_name(&local_state_after),
            Some("My Browser")
        );
        assert_eq!(
            BrowserLauncher::extract_preferences_profile_name(&preferences_after),
            Some("My Browser")
        );
    }

    #[test]
    fn ensure_actionbook_profile_display_name_does_not_change_non_actionbook_profiles() {
        let tmp = tempfile::tempdir().unwrap();
        let mut launcher = test_launcher_with_user_data_dir(tmp.path().to_path_buf());
        launcher.profile_name = "work".to_string();
        launcher.ensure_actionbook_profile_display_name().unwrap();

        assert!(!tmp.path().join("Local State").exists());
        assert!(!tmp.path().join("Default").join("Preferences").exists());
    }
}
