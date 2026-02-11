use thiserror::Error;

#[derive(Error, Debug)]
pub enum ActionbookError {
    #[error("Browser not found. Please install Chrome, Brave, or Edge.")]
    BrowserNotFound,

    #[error("Browser launch failed: {0}")]
    BrowserLaunchFailed(String),

    #[error("CDP connection failed: {0}")]
    CdpConnectionFailed(String),

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

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ActionbookError>;
