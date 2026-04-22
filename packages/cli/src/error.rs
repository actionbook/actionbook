use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("daemon not running")]
    DaemonNotRunning,
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("profile '{profile}' is already in use by session '{existing_session}'")]
    SessionAlreadyExists {
        profile: String,
        existing_session: String,
    },
    #[error("session id '{0}' is already in use")]
    SessionIdAlreadyExists(String),
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
    #[error("session closed: {0}")]
    SessionClosed(String),
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
    #[error("api error: {0}")]
    ApiError(String),
    #[error("api unauthorized: {0}")]
    ApiUnauthorized(String),
    #[error("api rate limited: {0}")]
    ApiRateLimited(String),
    #[error("api server error: {0}")]
    ApiServerError(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl CliError {
    pub fn error_code(&self) -> &str {
        match self {
            CliError::DaemonNotRunning => "DAEMON_NOT_RUNNING",
            CliError::ConnectionFailed(_) => "CONNECTION_FAILED",
            CliError::SessionNotFound(_) => "SESSION_NOT_FOUND",
            CliError::SessionAlreadyExists { .. } | CliError::SessionIdAlreadyExists(_) => {
                "SESSION_ALREADY_EXISTS"
            }
            CliError::TabNotFound(_) => "TAB_NOT_FOUND",
            CliError::InvalidArgument(_) => "INVALID_ARGUMENT",
            CliError::InvalidSessionId(_) => "INVALID_SESSION_ID",
            CliError::BrowserNotFound => "BROWSER_NOT_FOUND",
            CliError::BrowserLaunchFailed(_) => "BROWSER_LAUNCH_FAILED",
            CliError::CdpConnectionFailed(_) => "CDP_CONNECTION_FAILED",
            CliError::CdpError(_) => "CDP_ERROR",
            CliError::SessionClosed(_) => "SESSION_CLOSED",
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
            CliError::ApiError(_) => "API_ERROR",
            CliError::ApiUnauthorized(_) => "API_UNAUTHORIZED",
            CliError::ApiRateLimited(_) => "API_RATE_LIMITED",
            CliError::ApiServerError(_) => "API_SERVER_ERROR",
            CliError::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn hint(&self) -> String {
        match self {
            CliError::VersionMismatch { .. } => {
                "daemon is outdated. Kill the daemon process and retry".to_string()
            }
            CliError::SessionAlreadyExists {
                existing_session, ..
            } => {
                format!(
                    "each Chrome profile can only be used by one session at a time. Use --session {existing_session} to reuse it, close it with `actionbook browser close --session {existing_session}`, or use a different --profile"
                )
            }
            CliError::SessionIdAlreadyExists(existing_session) => {
                format!(
                    "choose a different --session / --set-session-id, or close the existing session with `actionbook browser close --session {existing_session}`"
                )
            }
            CliError::DaemonNotRunning => {
                "run a browser command to auto-start the daemon".to_string()
            }
            CliError::SessionClosed(_) => {
                "the session was closed while a command was still in flight — start a new session"
                    .to_string()
            }
            CliError::ApiUnauthorized(_) => {
                "check the provider API key environment variable (e.g. HYPERBROWSER_API_KEY, DRIVER_API_KEY, BROWSER_USE_API_KEY) and rotate it if revoked"
                    .to_string()
            }
            CliError::ApiRateLimited(_) => {
                "the provider rejected the request due to rate limiting — back off and retry later"
                    .to_string()
            }
            CliError::ApiServerError(_) => {
                "the provider service returned a 5xx error — retry after a short delay or check the provider's status page"
                    .to_string()
            }
            _ => String::new(),
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
                | CliError::ApiRateLimited(_)
                | CliError::ApiServerError(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::CliError;

    #[test]
    fn cdp_error_code_is_structured_for_stale_node_messages() {
        let err = CliError::cdp_classified("No node with given id found".to_string(), None);
        assert_eq!(err.error_code(), "CDP_NODE_NOT_FOUND");
    }

    #[test]
    fn cdp_error_hint_is_actionable_for_not_interactable_messages() {
        let err = CliError::cdp_classified("Could not compute box model.".to_string(), None);
        assert!(
            err.hint().contains("scroll"),
            "expected actionable not-interactable hint, got {:?}",
            err.hint()
        );
    }

    #[test]
    fn cdp_error_nav_timeout_is_retryable() {
        let err = CliError::cdp_classified("Navigation timeout of 100 ms exceeded".to_string(), None);
        assert_eq!(err.error_code(), "CDP_NAV_TIMEOUT");
        assert!(err.is_retryable(), "navigation timeout should be retryable");
    }

    #[test]
    fn cdp_error_target_closed_is_retryable() {
        let err = CliError::cdp_classified("response channel dropped".to_string(), None);
        assert_eq!(err.error_code(), "CDP_TARGET_CLOSED");
        assert!(err.is_retryable(), "target closed should be retryable");
    }

    #[test]
    fn cdp_error_protocol_errors_get_structured_code() {
        let err = CliError::cdp_classified("CDP error -32602: invalid params".to_string(), Some(-32602));
        assert_eq!(err.error_code(), "CDP_PROTOCOL_ERROR");
    }

    #[test]
    fn cdp_error_generic_transport_failures_get_generic_code() {
        let err = CliError::cdp_classified("socket parse exploded".to_string(), None);
        assert_eq!(err.error_code(), "CDP_GENERIC");
    }

    #[test]
    fn cdp_with_code_factory_bypasses_classifier() {
        panic!(
            "not yet implemented: add CliError::cdp_with_code factory and assert site override semantics"
        );
    }
}
