use thiserror::Error;

#[derive(Error, Debug)]
pub enum ActionbookError {
    #[error("Browser not found: {0}")]
    BrowserNotFound(String),

    #[error("Browser launch failed: {0}")]
    BrowserLaunchFailed(String),

    #[error("CDP connection failed: {0}")]
    CdpConnectionFailed(String),

    #[error("Browser connection failed: {0}")]
    BrowserConnectionFailed(String),

    #[error("Navigation failed for URL '{0}': {1}")]
    NavigationFailed(String, String),

    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),

    #[error("Element action failed on '{0}' (action: {1}): {2}")]
    ElementActionFailed(String, String, String),

    #[error("Content retrieval failed: {0}")]
    ContentRetrievalFailed(String),

    #[error("Browser not running. Use 'actionbook browser open <url>' first.")]
    BrowserNotRunning,

    #[error("Element not found: {0}")]
    ElementNotFound(String),

    #[error("JavaScript execution failed: {0}")]
    JavaScriptError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Profile already exists: {0}")]
    #[allow(dead_code)]
    ProfileExists(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Setup error: {0}")]
    SetupError(String),

    #[error("Extension error: {0}")]
    ExtensionError(String),

    #[error("Extension v{current} is already up to date (latest: v{latest})")]
    ExtensionAlreadyUpToDate { current: String, latest: String },

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Camoufox server not reachable at {0}")]
    CamofoxServerUnreachable(String),

    #[error("Element ref resolution failed for selector '{0}': {1}")]
    ElementRefResolution(String, String),

    #[error("Tab not found: {0}")]
    TabNotFound(String),

    #[error("Browser operation failed: {0}")]
    BrowserOperation(String),

    #[error("Feature '{0}' not enabled: {1}")]
    FeatureNotEnabled(String, String),

    #[error("Feature not supported: {0}")]
    FeatureNotSupported(String),

    #[error("Page not found: {0}")]
    PageNotFound(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("CDP error: {0}")]
    CdpError(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl ActionbookError {
    /// Return a machine-readable error code for structured output
    pub fn error_code(&self) -> &'static str {
        match self {
            ActionbookError::BrowserNotFound(_) => "browser_not_found",
            ActionbookError::BrowserLaunchFailed(_) => "browser_launch_failed",
            ActionbookError::CdpConnectionFailed(_) => "cdp_connection_failed",
            ActionbookError::BrowserConnectionFailed(_) => "browser_connection_failed",
            ActionbookError::NavigationFailed(_, _) => "navigation_failed",
            ActionbookError::ScreenshotFailed(_) => "screenshot_failed",
            ActionbookError::ElementActionFailed(_, _, _) => "element_action_failed",
            ActionbookError::ContentRetrievalFailed(_) => "content_retrieval_failed",
            ActionbookError::BrowserNotRunning => "browser_not_running",
            ActionbookError::ElementNotFound(_) => "element_not_found",
            ActionbookError::JavaScriptError(_) => "javascript_error",
            ActionbookError::ConfigError(_) => "config_error",
            ActionbookError::ProfileNotFound(_) => "profile_not_found",
            ActionbookError::ProfileExists(_) => "profile_exists",
            ActionbookError::ApiError(_) => "api_error",
            ActionbookError::SetupError(_) => "setup_error",
            ActionbookError::ExtensionError(_) => "extension_error",
            ActionbookError::ExtensionAlreadyUpToDate { .. } => "extension_already_up_to_date",
            ActionbookError::Timeout(_) => "timeout",
            ActionbookError::CamofoxServerUnreachable(_) => "camofox_server_unreachable",
            ActionbookError::ElementRefResolution(_, _) => "element_ref_resolution",
            ActionbookError::TabNotFound(_) => "tab_not_found",
            ActionbookError::BrowserOperation(_) => "browser_operation",
            ActionbookError::FeatureNotEnabled(_, _) => "feature_not_enabled",
            ActionbookError::FeatureNotSupported(_) => "feature_not_supported",
            ActionbookError::PageNotFound(_) => "page_not_found",
            ActionbookError::InvalidOperation(_) => "invalid_operation",
            ActionbookError::CdpError(_) => "cdp_error",
            ActionbookError::InvalidArgument(_) => "invalid_argument",
            ActionbookError::IoError(_) => "io_error",
            ActionbookError::NetworkError(_) => "network_error",
            ActionbookError::JsonError(_) => "json_error",
            ActionbookError::Other(_) => "unknown_error",
        }
    }
}

pub type Result<T> = std::result::Result<T, ActionbookError>;
