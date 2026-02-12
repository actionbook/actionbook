use crate::error::{ActionbookError, Result};
use std::path::PathBuf;
use thirtyfour::prelude::*;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

/// Camoufox WebDriver client for direct browser control
///
/// Launches Camoufox with --marionette flag and connects via WebDriver protocol.
/// No Playwright layer, no Python layer - pure Rust control.
pub struct CamofoxDriver {
    /// WebDriver client connected to Camoufox
    webdriver: WebDriver,
    /// Camoufox browser process
    process: Option<Child>,
    /// Path to Camoufox executable
    executable_path: PathBuf,
    /// Marionette port
    port: u16,
}

impl CamofoxDriver {
    /// Create a new Camoufox WebDriver instance
    ///
    /// This will:
    /// 1. Find the Camoufox binary
    /// 2. Launch Camoufox with --marionette flag
    /// 3. Connect via WebDriver protocol
    ///
    /// # Arguments
    ///
    /// * `headless` - Whether to run in headless mode
    pub async fn new(headless: bool) -> Result<Self> {
        Self::new_with_port(headless, 2828).await
    }

    /// Create a new Camoufox WebDriver instance with custom port
    pub async fn new_with_port(headless: bool, port: u16) -> Result<Self> {
        // 1. Find Camoufox binary
        let executable_path = Self::find_camoufox_binary()?;
        tracing::info!("Found Camoufox at: {}", executable_path.display());

        // 2. Find or download geckodriver
        let geckodriver_path = Self::ensure_geckodriver().await?;
        tracing::info!("Using geckodriver at: {}", geckodriver_path.display());

        // 3. Launch geckodriver with Camoufox binary
        let args = vec![
            "--port".to_string(),
            port.to_string(),
            "--binary".to_string(),
            executable_path.display().to_string(),
        ];

        tracing::info!("Launching geckodriver with args: {:?}", args);
        let mut process = Command::new(&geckodriver_path)
            .args(&args)
            .spawn()
            .map_err(|e| {
                ActionbookError::BrowserLaunchFailed(format!(
                    "Failed to launch geckodriver: {}",
                    e
                ))
            })?;

        // 4. Wait for geckodriver to be ready
        tracing::info!("Waiting for geckodriver to be ready on port {}...", port);
        let max_retries = 30; // 30 seconds max

        for i in 0..max_retries {
            sleep(Duration::from_secs(1)).await;

            // Try to connect
            match Self::try_connect(port, headless).await {
                Ok(driver) => {
                    tracing::info!("Successfully connected to Marionette on attempt {}", i + 1);
                    return Ok(Self {
                        webdriver: driver,
                        process: Some(process),
                        executable_path,
                        port,
                    });
                }
                Err(e) => {
                    tracing::debug!("Connection attempt {} failed: {}", i + 1, e);

                    // Check if process is still alive
                    match process.try_wait() {
                        Ok(Some(status)) => {
                            return Err(ActionbookError::BrowserLaunchFailed(format!(
                                "Camoufox process exited with status: {}",
                                status
                            )));
                        }
                        Ok(None) => {
                            // Process still running, continue waiting
                            continue;
                        }
                        Err(e) => {
                            return Err(ActionbookError::BrowserLaunchFailed(format!(
                                "Failed to check process status: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        // If we get here, we failed to connect
        let _ = process.kill().await;
        Err(ActionbookError::BrowserLaunchFailed(
            "Failed to connect to Marionette after 30 seconds".to_string(),
        ))
    }

    /// Try to connect to geckodriver
    /// Note: Anti-detection is primarily handled by Camoufox's C++-level patches,
    /// not Firefox preferences
    async fn try_connect(port: u16, headless: bool) -> Result<WebDriver> {
        let mut caps = DesiredCapabilities::firefox();

        // Set headless mode via capabilities
        if headless {
            caps.set_headless().map_err(|e| {
                ActionbookError::BrowserLaunchFailed(format!(
                    "Failed to set headless mode: {}",
                    e
                ))
            })?;
        }

        // Note: Camoufox already handles most anti-detection at C++ level
        // Firefox preferences have limited effect since Camoufox patches
        // happen before JavaScript execution

        let driver = WebDriver::new(&format!("http://localhost:{}", port), caps)
            .await
            .map_err(|e| {
                ActionbookError::BrowserConnectionFailed(format!(
                    "Failed to connect to WebDriver: {}",
                    e
                ))
            })?;

        Ok(driver)
    }

    /// Ensure geckodriver is available (find or download)
    async fn ensure_geckodriver() -> Result<PathBuf> {
        // First check if geckodriver is in PATH
        if let Ok(path) = which::which("geckodriver") {
            return Ok(path);
        }

        // Check in common locations
        let home = std::env::var("HOME")
            .map_err(|_| ActionbookError::BrowserNotFound("HOME env var not set".to_string()))?;

        let paths = vec![
            PathBuf::from(&home).join(".local/bin/geckodriver"),
            PathBuf::from(&home).join(".cargo/bin/geckodriver"),
            PathBuf::from("/usr/local/bin/geckodriver"),
            PathBuf::from("/usr/bin/geckodriver"),
        ];

        for path in paths {
            if path.exists() {
                return Ok(path);
            }
        }

        // If not found, provide helpful error message
        Err(ActionbookError::BrowserNotFound(
            "geckodriver not found. Please install it:\n\
             macOS: brew install geckodriver\n\
             Linux: Download from https://github.com/mozilla/geckodriver/releases\n\
             Or set it in PATH".to_string(),
        ))
    }

    /// Navigate to a URL
    pub async fn goto(&self, url: &str) -> Result<()> {
        self.webdriver
            .goto(url)
            .await
            .map_err(|e| ActionbookError::NavigationFailed(url.to_string(), e.to_string()))?;

        Ok(())
    }

    /// Take a screenshot
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let screenshot = self
            .webdriver
            .screenshot_as_png()
            .await
            .map_err(|e| ActionbookError::ScreenshotFailed(e.to_string()))?;

        Ok(screenshot)
    }

    /// Find an element by CSS selector
    pub async fn find_element(&self, selector: &str) -> Result<WebElement> {
        let element = self
            .webdriver
            .find(By::Css(selector))
            .await
            .map_err(|_e| ActionbookError::ElementNotFound(selector.to_string()))?;

        Ok(element)
    }

    /// Click an element
    pub async fn click(&self, selector: &str) -> Result<()> {
        let element = self.find_element(selector).await?;
        element
            .click()
            .await
            .map_err(|e| {
                ActionbookError::ElementActionFailed(
                    selector.to_string(),
                    "click".to_string(),
                    e.to_string(),
                )
            })?;

        Ok(())
    }

    /// Type text into an element
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<()> {
        let element = self.find_element(selector).await?;
        element
            .send_keys(text)
            .await
            .map_err(|e| {
                ActionbookError::ElementActionFailed(
                    selector.to_string(),
                    "type".to_string(),
                    e.to_string(),
                )
            })?;

        Ok(())
    }

    /// Get page HTML content
    pub async fn get_html(&self) -> Result<String> {
        let html = self
            .webdriver
            .source()
            .await
            .map_err(|e| ActionbookError::ContentRetrievalFailed(e.to_string()))?;

        Ok(html)
    }

    /// Get current page title
    pub async fn get_title(&self) -> Result<String> {
        let title = self
            .webdriver
            .title()
            .await
            .map_err(|e| ActionbookError::ContentRetrievalFailed(e.to_string()))?;

        Ok(title)
    }

    /// Get current page URL
    pub async fn get_url(&self) -> Result<String> {
        let url = self
            .webdriver
            .current_url()
            .await
            .map_err(|e| ActionbookError::ContentRetrievalFailed(e.to_string()))?;

        Ok(url.to_string())
    }

    /// Wait for an element to appear
    pub async fn wait_for_element(&self, selector: &str, timeout_secs: u64) -> Result<WebElement> {
        use tokio::time::timeout;

        let element: WebElement = timeout(Duration::from_secs(timeout_secs), async {
            loop {
                match self.find_element(selector).await {
                    Ok(element) => return element,
                    Err(_) => {
                        sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        })
        .await
        .map_err(|_| {
            ActionbookError::ElementNotFound(format!(
                "{} (timeout after {} seconds)",
                selector, timeout_secs
            ))
        })?;

        Ok(element)
    }

    /// Find the Camoufox binary on the system
    fn find_camoufox_binary() -> Result<PathBuf> {
        // Check common locations for Camoufox
        let home = std::env::var("HOME")
            .map_err(|_| ActionbookError::BrowserNotFound("HOME env var not set".to_string()))?;

        let paths = vec![
            // macOS
            PathBuf::from(&home).join("Library/Caches/camoufox/Camoufox.app/Contents/MacOS/camoufox"),
            // Linux
            PathBuf::from(&home).join(".cache/camoufox/camoufox"),
            PathBuf::from(&home).join(".local/share/camoufox/camoufox"),
            // Windows (via WSL or Wine)
            PathBuf::from(&home).join("AppData/Local/camoufox/camoufox.exe"),
        ];

        for path in paths {
            if path.exists() {
                tracing::debug!("Found Camoufox at: {}", path.display());
                return Ok(path);
            }
        }

        // Check if CAMOUFOX_PATH env var is set
        if let Ok(custom_path) = std::env::var("CAMOUFOX_PATH") {
            let path = PathBuf::from(custom_path);
            if path.exists() {
                return Ok(path);
            }
        }

        Err(ActionbookError::BrowserNotFound(
            "Camoufox binary not found. Please install Camoufox or set CAMOUFOX_PATH".to_string(),
        ))
    }

    /// Quit the browser and clean up
    pub async fn quit(self) -> Result<()> {
        // Note: We use ManuallyDrop to prevent Drop from running since we're
        // doing cleanup manually here
        let mut manual = std::mem::ManuallyDrop::new(self);

        // Close WebDriver connection
        // SAFETY: We're taking ownership and will prevent Drop from running
        let webdriver = unsafe { std::ptr::read(&manual.webdriver) };
        if let Err(e) = webdriver.quit().await {
            tracing::warn!("Failed to quit WebDriver gracefully: {}", e);
        }

        // Kill the process
        if let Some(mut process) = manual.process.take() {
            if let Err(e) = process.kill().await {
                tracing::warn!("Failed to kill Camoufox process: {}", e);
            }
        }

        Ok(())
    }
}

impl Drop for CamofoxDriver {
    fn drop(&mut self) {
        // Kill Camoufox process on drop
        if let Some(mut process) = self.process.take() {
            // Note: kill() returns a Result but we can't handle it in Drop
            let _ = process.start_kill();
            tracing::debug!("Killed Camoufox process in Drop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Camoufox to be installed
    async fn test_camoufox_launch() {
        let driver = CamofoxDriver::new(false).await.unwrap();
        driver.goto("https://example.com").await.unwrap();
        let screenshot = driver.screenshot().await.unwrap();
        assert!(!screenshot.is_empty());
        driver.quit().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_camoufox_navigation() {
        let driver = CamofoxDriver::new(false).await.unwrap();
        driver.goto("https://example.com").await.unwrap();
        let title = driver.get_title().await.unwrap();
        assert!(title.contains("Example"));
        driver.quit().await.unwrap();
    }
}
