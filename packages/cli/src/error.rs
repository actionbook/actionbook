use serde_json::{Map, Value};
use thiserror::Error;

use crate::daemon::cdp_error_classifier::{CdpErrorCode, classify};

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
    #[error("cdp error: {reason}")]
    CdpError {
        code: CdpErrorCode,
        reason: String,
        cdp_code: Option<i64>,
        details: Map<String, Value>,
    },
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
    /// Construct a CDP error whose code is derived from the raw message and
    /// (optional) CDP numeric code via the classifier.
    pub fn cdp_classified(reason: impl Into<String>, cdp_code: Option<i64>) -> Self {
        let reason = reason.into();
        let code = classify(&reason, cdp_code);
        CliError::CdpError {
            code,
            reason,
            cdp_code,
            details: Map::new(),
        }
    }

    /// Construct a CDP error with a caller-chosen code (bypasses the classifier).
    /// Use this at call sites where the caller knows the code better than
    /// message-pattern inference.
    pub fn cdp_with_code(
        code: CdpErrorCode,
        reason: impl Into<String>,
        cdp_code: Option<i64>,
    ) -> Self {
        CliError::CdpError {
            code,
            reason: reason.into(),
            cdp_code,
            details: Map::new(),
        }
    }

    /// Chainable per-site detail injection; no-op on non-CdpError variants.
    pub fn with_detail(mut self, key: &str, value: Value) -> Self {
        if let CliError::CdpError { details, .. } = &mut self {
            details.insert(key.to_string(), value);
        }
        self
    }

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
            CliError::CdpError { code, .. } => code.code(),
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
            CliError::CdpError { code, .. } => code.default_hint().to_string(),
            _ => String::new(),
        }
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            CliError::DaemonNotRunning
            | CliError::ConnectionFailed(_)
            | CliError::CloudConnectionLost(_)
            | CliError::Timeout
            | CliError::Http(_)
            | CliError::ApiRateLimited(_)
            | CliError::ApiServerError(_) => true,
            CliError::CdpError { code, .. } => code.is_retryable(),
            _ => false,
        }
    }

    /// Extract per-variant details for the JSON envelope.
    ///
    /// For `CdpError`, the returned object always carries `reason` (the raw
    /// CDP message), plus `cdp_code` when present and any site-specific fields
    /// injected via `with_detail`. Other variants return `Value::Null`.
    pub fn envelope_details(&self) -> Value {
        match self {
            CliError::CdpError {
                reason,
                cdp_code,
                details,
                ..
            } => {
                let mut out = details.clone();
                out.insert("reason".to_string(), Value::String(reason.clone()));
                if let Some(n) = cdp_code {
                    out.insert("cdp_code".to_string(), Value::from(*n));
                }
                Value::Object(out)
            }
            _ => Value::Null,
        }
    }
}

/// Wire-layer counterpart of `CliError::is_retryable` — keyed on the `error.code`
/// string so the envelope builder (which only has the code string, not a
/// `CliError` instance) agrees with the method.
pub fn is_retryable_code(code: &str) -> bool {
    matches!(
        code,
        "CLOUD_CONNECTION_LOST"
            | "TIMEOUT"
            | "CDP_NAV_TIMEOUT"
            | "CDP_TARGET_CLOSED"
            | "API_RATE_LIMITED"
            | "API_SERVER_ERROR"
            | "CONNECTION_FAILED"
            | "HTTP_ERROR"
            | "DAEMON_NOT_RUNNING"
    )
}

#[cfg(test)]
mod tests {
    use super::{CdpErrorCode, CliError, is_retryable_code};

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
        let err =
            CliError::cdp_classified("Navigation timeout of 100 ms exceeded".to_string(), None);
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
        let err =
            CliError::cdp_classified("CDP error -32602: invalid params".to_string(), Some(-32602));
        assert_eq!(err.error_code(), "CDP_PROTOCOL_ERROR");
    }

    #[test]
    fn cdp_error_generic_transport_failures_get_generic_code() {
        let err = CliError::cdp_classified("socket parse exploded".to_string(), None);
        assert_eq!(err.error_code(), "CDP_GENERIC");
    }

    #[test]
    fn cdp_with_code_factory_bypasses_classifier() {
        // reason would classify as NodeNotFound, but caller override wins.
        let err = CliError::cdp_with_code(
            CdpErrorCode::ProtocolError,
            "No node with given id found".to_string(),
            Some(-32000),
        );
        assert_eq!(err.error_code(), "CDP_PROTOCOL_ERROR");
        let details = err.envelope_details();
        assert_eq!(
            details["reason"].as_str(),
            Some("No node with given id found")
        );
        assert_eq!(details["cdp_code"].as_i64(), Some(-32000));
    }

    #[test]
    fn with_detail_injects_site_specific_fields() {
        let err = CliError::cdp_with_code(CdpErrorCode::NavTimeout, "timed out".to_string(), None)
            .with_detail("timeout_ms", serde_json::json!(100));
        let details = err.envelope_details();
        assert_eq!(details["reason"].as_str(), Some("timed out"));
        assert_eq!(details["timeout_ms"].as_i64(), Some(100));
    }

    #[test]
    fn envelope_details_is_null_for_non_cdp_variants() {
        assert!(CliError::Timeout.envelope_details().is_null());
        assert!(CliError::DaemonNotRunning.envelope_details().is_null());
    }

    /// Fixture covering every distinct `error_code()` string returned by
    /// `CliError`. The drift-guard test below walks this fixture to verify
    /// `is_retryable()` (method) and `is_retryable_code()` (wire-level fn)
    /// never disagree.
    fn all_variants_fixture() -> Vec<CliError> {
        vec![
            CliError::DaemonNotRunning,
            CliError::ConnectionFailed("x".to_string()),
            CliError::SessionNotFound("x".to_string()),
            CliError::SessionAlreadyExists {
                profile: "p".to_string(),
                existing_session: "s".to_string(),
            },
            CliError::SessionIdAlreadyExists("s".to_string()),
            CliError::TabNotFound("t".to_string()),
            CliError::InvalidArgument("x".to_string()),
            CliError::InvalidSessionId("x".to_string()),
            CliError::BrowserNotFound,
            CliError::BrowserLaunchFailed("x".to_string()),
            CliError::CdpConnectionFailed("x".to_string()),
            CliError::cdp_with_code(CdpErrorCode::NodeNotFound, "r", None),
            CliError::cdp_with_code(CdpErrorCode::NotInteractable, "r", None),
            CliError::cdp_with_code(CdpErrorCode::NavTimeout, "r", None),
            CliError::cdp_with_code(CdpErrorCode::TargetClosed, "r", None),
            CliError::cdp_with_code(CdpErrorCode::ProtocolError, "r", None),
            CliError::cdp_with_code(CdpErrorCode::Generic, "r", None),
            CliError::SessionClosed("x".to_string()),
            CliError::Timeout,
            CliError::NavigationFailed("x".to_string()),
            CliError::ElementNotFound("x".to_string()),
            CliError::EvalFailed("x".to_string()),
            CliError::MissingCdpEndpoint,
            CliError::CloudConnectionLost("x".to_string()),
            CliError::VersionMismatch {
                cli: "1".to_string(),
                daemon: "2".to_string(),
            },
            CliError::ApiError("x".to_string()),
            CliError::ApiUnauthorized("x".to_string()),
            CliError::ApiRateLimited("x".to_string()),
            CliError::ApiServerError("x".to_string()),
            CliError::Internal("x".to_string()),
        ]
    }

    #[test]
    fn is_retryable_code_matches_method_for_every_variant() {
        for err in all_variants_fixture() {
            let code = err.error_code().to_string();
            assert_eq!(
                err.is_retryable(),
                is_retryable_code(&code),
                "drift: {code} differs between method and code-string lookup"
            );
        }
    }
}
