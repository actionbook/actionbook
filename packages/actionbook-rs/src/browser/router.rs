//! Browser driver router for multi-backend support
//!
//! Routes commands to either CDP (Chrome/Edge/Brave) or Camoufox backend based on configuration.

use super::{
    camofox::CamofoxSession,
    camofox_webdriver::CamofoxDriver,
    session::SessionManager,
    BrowserBackend,
};
use crate::{
    cli::Cli,
    config::{Config, ProfileConfig},
    error::Result,
};

/// Unified browser driver that routes commands to the appropriate backend
pub enum BrowserDriver {
    /// Chrome DevTools Protocol backend
    Cdp(SessionManager),
    /// Camoufox browser backend (REST API via Python server)
    Camofox(CamofoxSession),
    /// Camoufox WebDriver backend (direct Rust control)
    CamofoxWebDriver(CamofoxDriver),
}

impl BrowserDriver {
    /// Create a browser driver from configuration
    ///
    /// Backend selection hierarchy:
    /// 1. CLI flag: `--camofox`
    /// 2. Profile config: `profiles.{name}.backend`
    /// 3. Global config: `browser.backend`
    /// 4. Default: CDP
    pub async fn from_config(
        config: &Config,
        profile: &ProfileConfig,
        cli: &Cli,
    ) -> Result<Self> {
        // Determine backend
        let backend = if cli.camofox {
            BrowserBackend::Camofox
        } else {
            profile
                .backend
                .or(Some(config.browser.backend))
                .unwrap_or_default()
        };

        match backend {
            BrowserBackend::Cdp => {
                let session_mgr = SessionManager::new(config.clone());
                Ok(Self::Cdp(session_mgr))
            }
            BrowserBackend::Camofox => {
                // Check if using WebDriver mode
                if config.browser.camofox.use_webdriver {
                    // Use Rust WebDriver implementation
                    let headless = config.browser.camofox.headless;
                    let driver = CamofoxDriver::new(headless).await?;
                    Ok(Self::CamofoxWebDriver(driver))
                } else {
                    // Use REST API (Python server)
                    let port = cli
                        .camofox_port
                        .or(profile.camofox_port)
                        .unwrap_or(config.browser.camofox.port);

                    let user_id = config
                        .browser
                        .camofox
                        .user_id
                        .clone()
                        .unwrap_or_else(|| "actionbook-user".to_string());

                    let session_key = config
                        .browser
                        .camofox
                        .session_key
                        .clone()
                        .unwrap_or_else(|| format!("actionbook-default"));

                    let session = CamofoxSession::connect(port, user_id, session_key).await?;
                    Ok(Self::Camofox(session))
                }
            }
        }
    }

    /// Navigate to a URL
    pub async fn goto(&mut self, url: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.goto(None, url).await,
            Self::Camofox(session) => {
                // If no active tab, create one
                if session.active_tab().is_err() {
                    session.create_tab(url).await?;
                    Ok(())
                } else {
                    session.navigate(url).await
                }
            }
            Self::CamofoxWebDriver(driver) => driver.goto(url).await,
        }
    }

    /// Ensure Camoufox session has an active tab (creates blank tab if needed)
    async fn ensure_camofox_tab(session: &mut CamofoxSession) -> Result<()> {
        if session.active_tab().is_err() {
            // Create a blank tab at about:blank
            session.create_tab("about:blank").await?;
        }
        Ok(())
    }

    /// Click an element by selector
    pub async fn click(&mut self, selector: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.click_on_page(None, selector).await,
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.click(selector).await
            }
            Self::CamofoxWebDriver(driver) => driver.click(selector).await,
        }
    }

    /// Type text into an element
    pub async fn type_text(&mut self, selector: &str, text: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.type_on_page(None, selector, text).await,
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.type_text(selector, text).await
            }
            Self::CamofoxWebDriver(driver) => driver.type_text(selector, text).await,
        }
    }

    /// Take a screenshot
    pub async fn screenshot(&mut self) -> Result<Vec<u8>> {
        match self {
            Self::Cdp(mgr) => mgr.screenshot_page(None).await,
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.screenshot().await
            }
            Self::CamofoxWebDriver(driver) => driver.screenshot().await,
        }
    }

    /// Get page content
    ///
    /// For CDP: Returns HTML
    /// For Camoufox REST: Returns accessibility tree JSON
    /// For Camoufox WebDriver: Returns HTML
    pub async fn get_content(&mut self) -> Result<String> {
        match self {
            Self::Cdp(mgr) => mgr.get_html(None, None).await,
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.get_content().await
            }
            Self::CamofoxWebDriver(driver) => driver.get_html().await,
        }
    }

    /// Execute JavaScript (CDP only)
    ///
    /// For Camoufox, returns an error as it doesn't support arbitrary JS execution
    pub async fn execute_js(&mut self, script: &str) -> Result<String> {
        match self {
            Self::Cdp(mgr) => {
                let result = mgr.eval_on_page(None, script).await?;
                Ok(serde_json::to_string(&result).unwrap_or_default())
            }
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(crate::error::ActionbookError::BrowserOperation(
                    "JavaScript execution not supported in Camoufox backend".to_string(),
                ))
            }
        }
    }

    /// Get the backend type
    pub fn backend(&self) -> BrowserBackend {
        match self {
            Self::Cdp(_) => BrowserBackend::Cdp,
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => BrowserBackend::Camofox,
        }
    }

    /// Check if the driver is using Camoufox (either REST or WebDriver)
    pub fn is_camofox(&self) -> bool {
        matches!(self, Self::Camofox(_) | Self::CamofoxWebDriver(_))
    }

    /// Check if the driver is using CDP
    pub fn is_cdp(&self) -> bool {
        matches!(self, Self::Cdp(_))
    }

    /// Check if using WebDriver mode (direct Rust control)
    pub fn is_webdriver(&self) -> bool {
        matches!(self, Self::CamofoxWebDriver(_))
    }

    /// Get CDP session manager (if using CDP backend)
    pub fn as_cdp(&self) -> Option<&SessionManager> {
        match self {
            Self::Cdp(mgr) => Some(mgr),
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => None,
        }
    }

    /// Get CDP session manager mutably (if using CDP backend)
    pub fn as_cdp_mut(&mut self) -> Option<&mut SessionManager> {
        match self {
            Self::Cdp(mgr) => Some(mgr),
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => None,
        }
    }

    /// Get Camoufox session (if using Camoufox REST backend)
    pub fn as_camofox(&self) -> Option<&CamofoxSession> {
        match self {
            Self::Cdp(_) | Self::CamofoxWebDriver(_) => None,
            Self::Camofox(session) => Some(session),
        }
    }

    /// Get Camoufox session mutably (if using Camoufox REST backend)
    pub fn as_camofox_mut(&mut self) -> Option<&mut CamofoxSession> {
        match self {
            Self::Cdp(_) | Self::CamofoxWebDriver(_) => None,
            Self::Camofox(session) => Some(session),
        }
    }

    /// Get Camoufox WebDriver (if using WebDriver backend)
    pub fn as_webdriver(&self) -> Option<&CamofoxDriver> {
        match self {
            Self::CamofoxWebDriver(driver) => Some(driver),
            Self::Cdp(_) | Self::Camofox(_) => None,
        }
    }

    /// Get Camoufox WebDriver mutably (if using WebDriver backend)
    pub fn as_webdriver_mut(&mut self) -> Option<&mut CamofoxDriver> {
        match self {
            Self::CamofoxWebDriver(driver) => Some(driver),
            Self::Cdp(_) | Self::Camofox(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_checking() {
        let config = Config::default();
        let session_mgr = SessionManager::new(config);
        let driver = BrowserDriver::Cdp(session_mgr);

        assert!(driver.is_cdp());
        assert!(!driver.is_camofox());
        assert_eq!(driver.backend(), BrowserBackend::Cdp);
        assert!(driver.as_cdp().is_some());
        assert!(driver.as_camofox().is_none());
    }
}
