//! Browser driver router for multi-backend support
//!
//! Routes commands to either CDP (Chrome/Edge/Brave) or Camoufox backend based on configuration.

use super::{
    session::SessionManager,
    BrowserBackend,
};

#[cfg(feature = "camoufox")]
use super::{
    camofox::CamofoxSession,
    camofox_webdriver::CamofoxDriver,
};
#[allow(unused_imports)]
use crate::{
    cli::Cli,
    config::{Config, ProfileConfig},
    error::{ActionbookError, Result},
};

/// Unified browser driver that routes commands to the appropriate backend
pub enum BrowserDriver {
    /// Chrome DevTools Protocol backend
    Cdp(SessionManager),
    #[cfg(feature = "camoufox")]
    /// Camoufox browser backend (REST API via Python server)
    Camofox(CamofoxSession),
    #[cfg(feature = "camoufox")]
    /// Camoufox WebDriver backend (direct Rust control)
    CamofoxWebDriver(CamofoxDriver),
}

impl BrowserDriver {
    /// Create a browser driver from CLI flags (convenience wrapper)
    ///
    /// Respects `--profile` and `--cdp` CLI flags to override config defaults.
    #[allow(dead_code)]
    pub async fn from_cli(cli: &Cli) -> Result<Self> {
        let config = Config::load()?;
        let profile_name = match cli.profile.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(name) => name.to_string(),
            None => config.effective_default_profile_name(),
        };
        let mut profile = config.get_profile(&profile_name)?;
        // Apply CLI overrides
        if let Some(ref cdp) = cli.cdp {
            profile.cdp_url = Some(cdp.clone());
        }
        if cli.headless {
            profile.headless = true;
        }
        Self::from_config(&config, &profile, cli).await
    }

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
            #[cfg(feature = "camoufox")]
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
            #[cfg(not(feature = "camoufox"))]
            BrowserBackend::Camofox => {
                Err(crate::error::ActionbookError::FeatureNotEnabled(
                    "camoufox".to_string(),
                    "Compile with --features camoufox to use Camoufox backend".to_string(),
                ))
            }
        }
    }

    /// Navigate to a URL
    pub async fn goto(&mut self, url: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.goto(None, url).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(session) => {
                // If no active tab, create one
                if session.active_tab().is_err() {
                    session.create_tab(url).await?;
                    Ok(())
                } else {
                    session.navigate(url).await
                }
            }
                                    #[cfg(feature = "camoufox")]
            Self::CamofoxWebDriver(driver) => driver.goto(url).await,
        }
    }

    /// Ensure Camoufox session has an active tab (creates blank tab if needed)
    #[cfg(feature = "camoufox")]
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
            #[cfg(feature = "camoufox")]
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.click(selector).await
            }
                        #[cfg(feature = "camoufox")]
            Self::CamofoxWebDriver(driver) => driver.click(selector).await,
        }
    }

    /// Fill (clear + type) text into an element
    pub async fn fill(&mut self, selector: &str, text: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.fill_on_page(None, selector, text).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(crate::error::ActionbookError::FeatureNotSupported(
                    "fill is not yet supported for Camoufox backend".to_string(),
                ))
            }
        }
    }

    /// Select an option from a dropdown
    pub async fn select(&mut self, selector: &str, value: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.select_on_page(None, selector, value).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(crate::error::ActionbookError::FeatureNotSupported(
                    "select is not yet supported for Camoufox backend".to_string(),
                ))
            }
        }
    }

    /// Hover over an element
    #[allow(dead_code)]
    pub async fn hover(&mut self, selector: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.hover_on_page(None, selector).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(crate::error::ActionbookError::FeatureNotSupported(
                    "hover is not yet supported for Camoufox backend".to_string(),
                ))
            }
        }
    }

    /// Focus on an element
    pub async fn focus(&mut self, selector: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.focus_on_page(None, selector).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(crate::error::ActionbookError::FeatureNotSupported(
                    "focus is not yet supported for Camoufox backend".to_string(),
                ))
            }
        }
    }

    /// Type text into an element
    pub async fn type_text(&mut self, selector: &str, text: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.type_on_page(None, selector, text).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.type_text(selector, text).await
            }
                        #[cfg(feature = "camoufox")]
            Self::CamofoxWebDriver(driver) => driver.type_text(selector, text).await,
        }
    }

    /// Take a screenshot
    pub async fn screenshot(&mut self) -> Result<Vec<u8>> {
        match self {
            Self::Cdp(mgr) => mgr.screenshot_page(None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.screenshot().await
            }
                        #[cfg(feature = "camoufox")]
            Self::CamofoxWebDriver(driver) => driver.screenshot().await,
        }
    }

    /// Get page content as string
    ///
    /// For CDP: Returns HTML
    /// For Camoufox REST: Returns accessibility tree JSON
    /// For Camoufox WebDriver: Returns HTML
    pub async fn get_content(&mut self) -> Result<String> {
        match self {
            Self::Cdp(mgr) => mgr.get_html(None, None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(session) => {
                Self::ensure_camofox_tab(session).await?;
                session.get_content().await
            }
            #[cfg(feature = "camoufox")]
            Self::CamofoxWebDriver(driver) => driver.get_html().await,
        }
    }

    // ========== G5: Fingerprint Rotation ==========

    /// Rotate the browser fingerprint dynamically
    pub async fn rotate_fingerprint(
        &mut self,
        fingerprint: &super::stealth_enhanced::EnhancedStealthProfile,
    ) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.rotate_fingerprint(None, fingerprint).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Fingerprint rotation is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== G2: Global Animation Disabling ==========

    /// Disable CSS animations and transitions on all pages
    pub async fn disable_animations(&mut self) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.disable_animations(None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Animation disabling is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== F3: Resource Blocking ==========

    /// Block resource loading at network level
    pub async fn set_resource_blocking(
        &mut self,
        level: super::session::ResourceBlockLevel,
    ) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.set_resource_blocking(None, level).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Resource blocking is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== F4: Readability Text Extraction ==========

    /// Get readable text content from the page
    pub async fn get_readable_text(
        &mut self,
        mode: super::session::TextExtractionMode,
    ) -> Result<String> {
        match self {
            Self::Cdp(mgr) => mgr.get_readable_text(None, mode).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                // Fallback: use eval for raw mode
                Err(ActionbookError::FeatureNotSupported(
                    "Readability extraction is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== F1: CDP Accessibility Tree ==========

    /// Get the accessibility tree via CDP (returns raw JSON)
    pub async fn get_accessibility_tree_raw(&mut self) -> Result<serde_json::Value> {
        match self {
            Self::Cdp(mgr) => mgr.get_accessibility_tree(None).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "CDP Accessibility Tree is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Get backend node ID for a CSS selector (for snapshot scoping)
    pub async fn get_backend_node_id(&mut self, selector: &str) -> Result<Option<i64>> {
        match self {
            Self::Cdp(mgr) => mgr.get_backend_node_id(None, selector).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => Ok(None),
        }
    }

    // ========== F2: Ref-based actions ==========

    /// Click an element by backendNodeId
    pub async fn click_by_node_id(&mut self, backend_node_id: i64) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.click_by_node_id(None, backend_node_id).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Node ID actions are only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Type text into an element by backendNodeId
    pub async fn type_by_node_id(&mut self, backend_node_id: i64, text: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.type_by_node_id(None, backend_node_id, text).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Node ID actions are only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Fill an element by backendNodeId
    pub async fn fill_by_node_id(&mut self, backend_node_id: i64, text: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.fill_by_node_id(None, backend_node_id, text).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Node ID actions are only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Focus an element by backendNodeId
    pub async fn focus_by_node_id(&mut self, backend_node_id: i64) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.focus_by_node_id(None, backend_node_id).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Node ID actions are only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Get the center coordinates of an element by backendNodeId
    pub async fn get_element_center_by_node_id(&mut self, backend_node_id: i64) -> Result<(f64, f64)> {
        match self {
            Self::Cdp(mgr) => mgr.get_element_center_by_node_id(None, backend_node_id).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Node ID actions are only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Get the center coordinates of an element by CSS selector
    pub async fn get_element_center(&mut self, selector: &str) -> Result<(f64, f64)> {
        match self {
            Self::Cdp(mgr) => mgr.get_element_center(None, selector).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Element center lookup is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== File Upload ==========

    /// Set files on a file input element by CSS selector
    pub async fn set_file_input_files(&mut self, selector: &str, files: &[String]) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.set_file_input_files(None, selector, files).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "File upload is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Set files on a file input element by backendNodeId
    pub async fn set_file_input_files_by_node_id(
        &mut self,
        backend_node_id: i64,
        files: &[String],
    ) -> Result<()> {
        match self {
            Self::Cdp(mgr) => {
                mgr.set_file_input_files_by_node_id(None, backend_node_id, files)
                    .await
            }
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "File upload is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== F5: Human-like input ==========

    /// Dispatch mouse move events along a path
    pub async fn dispatch_mouse_moves(&mut self, points: &[(f64, f64)]) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.dispatch_mouse_moves(None, points).await,
                        #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Human mouse simulation is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== H1: Console Log Capture ==========

    /// Install console interceptor and capture logs
    pub async fn install_console_interceptor(&mut self) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.install_console_interceptor(None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Console capture is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Capture console logs from the page
    pub async fn capture_console_logs(&mut self) -> Result<Vec<serde_json::Value>> {
        match self {
            Self::Cdp(mgr) => mgr.capture_console_logs(None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Console capture is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== H2: Network Idle Wait ==========

    /// Wait for network to become idle
    pub async fn wait_for_network_idle(&mut self, timeout_ms: u64, idle_ms: u64) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.wait_for_network_idle(None, timeout_ms, idle_ms).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Network idle wait is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== H3: Dialog Auto-Handling ==========

    /// Enable auto-dismissal of JavaScript dialogs
    pub async fn enable_dialog_auto_dismiss(&mut self) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.enable_dialog_auto_dismiss(None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Dialog auto-dismiss is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== H4: Element Info ==========

    /// Get detailed info about an element
    pub async fn get_element_info(&mut self, selector: &str) -> Result<serde_json::Value> {
        match self {
            Self::Cdp(mgr) => mgr.get_element_info(None, selector).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Element info is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== H6: Device Emulation ==========

    /// Emulate a device
    pub async fn emulate_device(
        &mut self,
        width: u32,
        height: u32,
        device_scale_factor: f64,
        mobile: bool,
        user_agent: Option<&str>,
    ) -> Result<()> {
        match self {
            Self::Cdp(mgr) => {
                mgr.emulate_device(None, width, height, device_scale_factor, mobile, user_agent)
                    .await
            }
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Device emulation is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    // ========== H7: Wait for JS Condition ==========

    /// Wait for a JavaScript expression to return a truthy value
    pub async fn wait_for_function(
        &mut self,
        expression: &str,
        timeout_ms: u64,
        interval_ms: u64,
    ) -> Result<serde_json::Value> {
        match self {
            Self::Cdp(mgr) => {
                mgr.wait_for_function(None, expression, timeout_ms, interval_ms)
                    .await
            }
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Wait for function is only supported on CDP backend".to_string(),
                ))
            }
        }
    }

    /// Execute JavaScript and return result as string (convenience alias for execute_js)
    pub async fn eval(&mut self, script: &str) -> Result<String> {
        self.execute_js(script).await
    }

    /// Execute JavaScript (CDP only)
    ///
    /// For Camoufox, returns an error as it doesn't support arbitrary JS execution
    #[allow(dead_code)]
    pub async fn execute_js(&mut self, script: &str) -> Result<String> {
        match self {
            Self::Cdp(mgr) => {
                let result = mgr.eval_on_page(None, script).await?;
                Ok(serde_json::to_string(&result).unwrap_or_default())
            }
            #[cfg(feature = "camoufox")]
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
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => BrowserBackend::Camofox,
        }
    }

    /// Check if the driver is using Camoufox (either REST or WebDriver)
    pub fn is_camofox(&self) -> bool {
        #[cfg(feature = "camoufox")]
        {
            matches!(self, Self::Camofox(_) | Self::CamofoxWebDriver(_))
        }
        #[cfg(not(feature = "camoufox"))]
        {
            false
        }
    }

    /// Check if the driver is using CDP
    pub fn is_cdp(&self) -> bool {
        matches!(self, Self::Cdp(_))
    }

    /// Check if using WebDriver mode (direct Rust control)
    #[allow(dead_code)]
    pub fn is_webdriver(&self) -> bool {
        #[cfg(feature = "camoufox")]
        {
            matches!(self, Self::CamofoxWebDriver(_))
        }
        #[cfg(not(feature = "camoufox"))]
        {
            false
        }
    }

    /// List all open pages/tabs
    pub async fn list_pages(&self) -> Result<Vec<super::session::PageInfo>> {
        match self {
            Self::Cdp(mgr) => mgr.get_pages(None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Page listing not yet supported for Camoufox backend".to_string()
                ))
            }
        }
    }

    /// Switch to a specific page by ID
    pub async fn switch_to_page(&mut self, page_id: &str) -> Result<super::session::PageInfo> {
        match self {
            Self::Cdp(mgr) => mgr.switch_to_page(None, page_id).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Page switching not supported for Camoufox backend".to_string()
                ))
            }
        }
    }

    /// Create a new tab/page
    pub async fn new_page(&mut self, url: Option<&str>) -> Result<super::session::PageInfo> {
        match self {
            Self::Cdp(mgr) => mgr.new_page(None, url).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(session) => {
                let page_url = url.unwrap_or("about:blank");
                session.create_tab(page_url).await?;
                // Return a mock PageInfo since Camoufox doesn't expose full page details
                Ok(super::session::PageInfo {
                    id: String::new(),
                    title: String::new(),
                    url: page_url.to_string(),
                    page_type: "page".to_string(),
                    web_socket_debugger_url: None,
                })
            }
            #[cfg(feature = "camoufox")]
            Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "New tab creation not supported for Camoufox WebDriver backend".to_string()
                ))
            }
        }
    }

    /// Close a specific page/tab
    pub async fn close_page(&mut self, page_id: &str) -> Result<()> {
        match self {
            Self::Cdp(mgr) => mgr.close_page(None, page_id).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Page closing not supported for Camoufox backend".to_string()
                ))
            }
        }
    }

    /// Get currently active page info
    pub async fn get_active_page(&self) -> Result<super::session::PageInfo> {
        match self {
            Self::Cdp(mgr) => mgr.get_active_page_info(None).await,
            #[cfg(feature = "camoufox")]
            Self::Camofox(session) => {
                let tab_id = session.active_tab()?;
                Ok(super::session::PageInfo {
                    id: tab_id.to_string(),
                    title: String::new(),
                    url: String::new(),
                    page_type: "page".to_string(),
                    web_socket_debugger_url: None,
                })
            }
            #[cfg(feature = "camoufox")]
            Self::CamofoxWebDriver(_) => {
                Err(ActionbookError::FeatureNotSupported(
                    "Get active page not supported for Camoufox WebDriver backend".to_string()
                ))
            }
        }
    }

    /// Get CDP session manager (if using CDP backend)
    #[allow(dead_code)]
    pub fn as_cdp(&self) -> Option<&SessionManager> {
        match self {
            Self::Cdp(mgr) => Some(mgr),
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => None,
        }
    }

    /// Get CDP session manager mutably (if using CDP backend)
    pub fn as_cdp_mut(&mut self) -> Option<&mut SessionManager> {
        match self {
            Self::Cdp(mgr) => Some(mgr),
            #[cfg(feature = "camoufox")]
            Self::Camofox(_) | Self::CamofoxWebDriver(_) => None,
        }
    }

    /// Get Camoufox session (if using Camoufox REST backend)
    #[cfg(feature = "camoufox")]
    #[allow(dead_code)]
    pub fn as_camofox(&self) -> Option<&CamofoxSession> {
        match self {
            Self::Cdp(_) | Self::CamofoxWebDriver(_) => None,
            Self::Camofox(session) => Some(session),
        }
    }

    /// Get Camoufox session mutably (if using Camoufox REST backend)
    #[cfg(feature = "camoufox")]
    #[allow(dead_code)]
    pub fn as_camofox_mut(&mut self) -> Option<&mut CamofoxSession> {
        match self {
            Self::Cdp(_) | Self::CamofoxWebDriver(_) => None,
            Self::Camofox(session) => Some(session),
        }
    }

    /// Get Camoufox WebDriver (if using WebDriver backend)
    #[cfg(feature = "camoufox")]
    #[allow(dead_code)]
    pub fn as_webdriver(&self) -> Option<&CamofoxDriver> {
        match self {
            Self::CamofoxWebDriver(driver) => Some(driver),
            Self::Cdp(_) | Self::Camofox(_) => None,
        }
    }

    /// Get Camoufox WebDriver mutably (if using WebDriver backend)
    #[cfg(feature = "camoufox")]
    #[allow(dead_code)]
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
        #[cfg(feature = "camoufox")]
        assert!(driver.as_camofox().is_none());
    }
}
