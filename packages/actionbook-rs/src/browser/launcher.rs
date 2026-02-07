use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tokio::time::sleep;

use super::discovery::{discover_browser, BrowserInfo};
use crate::config::ProfileConfig;
use crate::error::{ActionbookError, Result};

/// Browser launcher that starts a browser with CDP enabled
pub struct BrowserLauncher {
    browser_info: BrowserInfo,
    cdp_port: u16,
    headless: bool,
    stealth: bool,
    user_data_dir: PathBuf,
    extra_args: Vec<String>,
}

impl BrowserLauncher {
    /// Create a new launcher with default settings
    pub fn new() -> Result<Self> {
        let browser_info = discover_browser()?;
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("actionbook")
            .join("profiles")
            .join("default");

        Ok(Self {
            browser_info,
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

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("actionbook")
            .join("profiles")
            .join("default");

        Ok(Self {
            browser_info,
            cdp_port: 9222,
            headless: false,
            stealth: false,
            user_data_dir: data_dir,
            extra_args: Vec::new(),
        })
    }

    /// Create a launcher from profile configuration
    pub fn from_profile(profile: &ProfileConfig) -> Result<Self> {
        let mut launcher = if let Some(ref path) = profile.browser_path {
            Self::with_browser_path(PathBuf::from(path))?
        } else {
            Self::new()?
        };

        launcher.cdp_port = profile.cdp_port;
        launcher.headless = profile.headless;

        if let Some(ref dir) = profile.user_data_dir {
            launcher.user_data_dir = PathBuf::from(shellexpand::tilde(dir).to_string());
        }

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

    /// Launch the browser and return the process handle
    pub fn launch(&self) -> Result<Child> {
        // Ensure user data directory exists
        std::fs::create_dir_all(&self.user_data_dir)?;

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
