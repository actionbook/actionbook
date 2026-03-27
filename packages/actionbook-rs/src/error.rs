use thiserror::Error;

#[allow(dead_code)]
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

    #[error("Daemon error: {0}")]
    DaemonError(String),

    #[error("Daemon not running: {0}")]
    DaemonNotRunning(String),

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
            ActionbookError::DaemonError(_) => "daemon_error",
            ActionbookError::DaemonNotRunning(_) => "daemon_not_running",
            ActionbookError::IoError(_) => "io_error",
            ActionbookError::NetworkError(_) => "network_error",
            ActionbookError::JsonError(_) => "json_error",
            ActionbookError::Other(_) => "unknown_error",
        }
    }
}

pub type Result<T> = std::result::Result<T, ActionbookError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_correct() {
        assert_eq!(
            ActionbookError::BrowserNotFound("chrome".into()).error_code(),
            "browser_not_found"
        );
        assert_eq!(
            ActionbookError::BrowserLaunchFailed("timeout".into()).error_code(),
            "browser_launch_failed"
        );
        assert_eq!(
            ActionbookError::CdpConnectionFailed("refused".into()).error_code(),
            "cdp_connection_failed"
        );
        assert_eq!(
            ActionbookError::BrowserConnectionFailed("lost".into()).error_code(),
            "browser_connection_failed"
        );
        assert_eq!(
            ActionbookError::NavigationFailed("https://x.com".into(), "timeout".into())
                .error_code(),
            "navigation_failed"
        );
        assert_eq!(
            ActionbookError::ScreenshotFailed("io".into()).error_code(),
            "screenshot_failed"
        );
        assert_eq!(
            ActionbookError::ElementActionFailed("btn".into(), "click".into(), "err".into())
                .error_code(),
            "element_action_failed"
        );
        assert_eq!(
            ActionbookError::ContentRetrievalFailed("timeout".into()).error_code(),
            "content_retrieval_failed"
        );
        assert_eq!(
            ActionbookError::BrowserNotRunning.error_code(),
            "browser_not_running"
        );
        assert_eq!(
            ActionbookError::ElementNotFound("#btn".into()).error_code(),
            "element_not_found"
        );
        assert_eq!(
            ActionbookError::JavaScriptError("TypeError".into()).error_code(),
            "javascript_error"
        );
        assert_eq!(
            ActionbookError::ConfigError("missing field".into()).error_code(),
            "config_error"
        );
        assert_eq!(
            ActionbookError::ProfileNotFound("dev".into()).error_code(),
            "profile_not_found"
        );
        assert_eq!(
            ActionbookError::ProfileExists("dev".into()).error_code(),
            "profile_exists"
        );
        assert_eq!(
            ActionbookError::ApiError("401".into()).error_code(),
            "api_error"
        );
        assert_eq!(
            ActionbookError::SetupError("step failed".into()).error_code(),
            "setup_error"
        );
        assert_eq!(
            ActionbookError::ExtensionError("not found".into()).error_code(),
            "extension_error"
        );
        assert_eq!(
            ActionbookError::ExtensionAlreadyUpToDate {
                current: "1.0.0".into(),
                latest: "1.0.0".into()
            }
            .error_code(),
            "extension_already_up_to_date"
        );
        assert_eq!(
            ActionbookError::Timeout("30s".into()).error_code(),
            "timeout"
        );
        assert_eq!(
            ActionbookError::CamofoxServerUnreachable("127.0.0.1:9377".into()).error_code(),
            "camofox_server_unreachable"
        );
        assert_eq!(
            ActionbookError::ElementRefResolution("#x".into(), "not found".into()).error_code(),
            "element_ref_resolution"
        );
        assert_eq!(
            ActionbookError::TabNotFound("t5".into()).error_code(),
            "tab_not_found"
        );
        assert_eq!(
            ActionbookError::BrowserOperation("failed".into()).error_code(),
            "browser_operation"
        );
        assert_eq!(
            ActionbookError::FeatureNotEnabled("stealth".into(), "compile flag".into())
                .error_code(),
            "feature_not_enabled"
        );
        assert_eq!(
            ActionbookError::FeatureNotSupported("screenshot".into()).error_code(),
            "feature_not_supported"
        );
        assert_eq!(
            ActionbookError::PageNotFound("about:blank".into()).error_code(),
            "page_not_found"
        );
        assert_eq!(
            ActionbookError::InvalidOperation("read-only".into()).error_code(),
            "invalid_operation"
        );
        assert_eq!(
            ActionbookError::CdpError("send failed".into()).error_code(),
            "cdp_error"
        );
        assert_eq!(
            ActionbookError::InvalidArgument("--tab".into()).error_code(),
            "invalid_argument"
        );
        assert_eq!(
            ActionbookError::DaemonError("crashed".into()).error_code(),
            "daemon_error"
        );
        assert_eq!(
            ActionbookError::DaemonNotRunning("no socket".into()).error_code(),
            "daemon_not_running"
        );
        assert_eq!(
            ActionbookError::Other("misc".into()).error_code(),
            "unknown_error"
        );
    }

    #[test]
    fn error_codes_for_from_impls() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ab_err: ActionbookError = io_err.into();
        assert_eq!(ab_err.error_code(), "io_error");

        let json_err: ActionbookError = serde_json::from_str::<serde_json::Value>("{bad json")
            .unwrap_err()
            .into();
        assert_eq!(json_err.error_code(), "json_error");
    }

    #[test]
    fn error_display_messages_include_context() {
        let err = ActionbookError::BrowserNotFound("chromium".into());
        assert!(err.to_string().contains("chromium"));

        let err = ActionbookError::NavigationFailed("https://foo.com".into(), "timeout".into());
        assert!(err.to_string().contains("https://foo.com"));
        assert!(err.to_string().contains("timeout"));

        let err = ActionbookError::ExtensionAlreadyUpToDate {
            current: "1.2.3".into(),
            latest: "1.2.3".into(),
        };
        assert!(err.to_string().contains("1.2.3"));

        assert_eq!(
            ActionbookError::BrowserNotRunning.to_string(),
            "Browser not running. Use 'actionbook browser open <url>' first."
        );
    }
}
