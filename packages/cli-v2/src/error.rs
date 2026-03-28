use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("daemon not running")]
    DaemonNotRunning,
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("session already exists: {0}")]
    SessionAlreadyExists(String),
    #[error("tab not found: {0}")]
    TabNotFound(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("invalid session id: {0}")]
    InvalidSessionId(String),
    #[error("browser not found")]
    BrowserNotFound,
    #[error("browser launch failed: {0}")]
    BrowserLaunchFailed(String),
    #[error("cdp connection failed: {0}")]
    CdpConnectionFailed(String),
    #[error("cdp error: {0}")]
    CdpError(String),
    #[error("timeout")]
    Timeout,
    #[error("navigation failed: {0}")]
    NavigationFailed(String),
    #[error("element not found: {0}")]
    ElementNotFound(String),
    #[error("eval failed: {0}")]
    EvalFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("--mode cloud requires --cdp-endpoint")]
    MissingCdpEndpoint,
    #[error("cloud connection lost: {0}")]
    CloudConnectionLost(String),
    #[error("version mismatch: cli={cli}, daemon={daemon}")]
    VersionMismatch { cli: String, daemon: String },
    #[error("internal error: {0}")]
    Internal(String),
}

impl CliError {
    pub fn error_code(&self) -> &str {
        match self {
            CliError::DaemonNotRunning => "DAEMON_NOT_RUNNING",
            CliError::ConnectionFailed(_) => "CONNECTION_FAILED",
            CliError::SessionNotFound(_) => "SESSION_NOT_FOUND",
            CliError::SessionAlreadyExists(_) => "SESSION_ALREADY_EXISTS",
            CliError::TabNotFound(_) => "TAB_NOT_FOUND",
            CliError::InvalidArgument(_) => "INVALID_ARGUMENT",
            CliError::InvalidSessionId(_) => "INVALID_SESSION_ID",
            CliError::BrowserNotFound => "BROWSER_NOT_FOUND",
            CliError::BrowserLaunchFailed(_) => "BROWSER_LAUNCH_FAILED",
            CliError::CdpConnectionFailed(_) => "CDP_CONNECTION_FAILED",
            CliError::CdpError(_) => "CDP_ERROR",
            CliError::Timeout => "TIMEOUT",
            CliError::NavigationFailed(_) => "NAVIGATION_FAILED",
            CliError::ElementNotFound(_) => "ELEMENT_NOT_FOUND",
            CliError::EvalFailed(_) => "EVAL_FAILED",
            CliError::Io(_) => "IO_ERROR",
            CliError::Json(_) => "INTERNAL_ERROR",
            CliError::Http(_) => "HTTP_ERROR",
            CliError::MissingCdpEndpoint => "MISSING_CDP_ENDPOINT",
            CliError::CloudConnectionLost(_) => "CLOUD_CONNECTION_LOST",
            CliError::VersionMismatch { .. } => "VERSION_MISMATCH",
            CliError::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn hint(&self) -> &str {
        match self {
            CliError::VersionMismatch { .. } => {
                "daemon is outdated. Kill the daemon process and retry"
            }
            CliError::DaemonNotRunning => "run a browser command to auto-start the daemon",
            _ => "",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            CliError::DaemonNotRunning
                | CliError::ConnectionFailed(_)
                | CliError::CloudConnectionLost(_)
                | CliError::Timeout
                | CliError::Http(_)
        )
    }
}
