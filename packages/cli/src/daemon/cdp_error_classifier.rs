#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdpErrorCode {
    NodeNotFound,
    NotInteractable,
    NavTimeout,
    TargetClosed,
    ProtocolError,
    Generic,
}

impl CdpErrorCode {
    pub fn from_wire_code(raw: &str) -> Option<Self> {
        match raw {
            "CDP_NODE_NOT_FOUND" => Some(Self::NodeNotFound),
            "CDP_NOT_INTERACTABLE" => Some(Self::NotInteractable),
            "CDP_NAV_TIMEOUT" => Some(Self::NavTimeout),
            "CDP_TARGET_CLOSED" => Some(Self::TargetClosed),
            "CDP_PROTOCOL_ERROR" => Some(Self::ProtocolError),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::NodeNotFound => "CDP_NODE_NOT_FOUND",
            Self::NotInteractable => "CDP_NOT_INTERACTABLE",
            Self::NavTimeout => "CDP_NAV_TIMEOUT",
            Self::TargetClosed => "CDP_TARGET_CLOSED",
            Self::ProtocolError => "CDP_PROTOCOL_ERROR",
            Self::Generic => "CDP_GENERIC",
        }
    }

    pub fn default_hint(self) -> &'static str {
        match self {
            Self::NodeNotFound => {
                "the referenced node is stale — call `actionbook browser snapshot` to refresh node references then retry"
            }
            Self::NotInteractable => {
                "the element exists but isn't interactable — scroll it into view, wait for it to become visible, or dismiss any overlay covering it"
            }
            Self::NavTimeout => {
                "navigation exceeded the deadline — increase `--timeout` or verify the target URL is reachable"
            }
            Self::TargetClosed => {
                "the CDP target was closed mid-command (tab navigated away or session torn down) — start a fresh session or re-attach to the tab"
            }
            Self::ProtocolError => {
                "the browser rejected the command — inspect `details.reason` and `details.cdp_code` for the raw protocol error"
            }
            Self::Generic => "",
        }
    }

    pub fn is_retryable(self) -> bool {
        matches!(self, Self::NavTimeout | Self::TargetClosed)
    }
}

/// Classify a CDP error into a structured code.
///
/// Precedence: message pattern > numeric fallback > Generic. The classifier
/// stays pure — no site knowledge (that lives in call-site overrides via
/// `CliError::cdp_with_code`).
pub fn classify(raw: &str, cdp_numeric_code: Option<i64>) -> CdpErrorCode {
    let lower = raw.to_lowercase();

    if lower.contains("no node with given id")
        || lower.contains("cannot find context with specified id")
    {
        return CdpErrorCode::NodeNotFound;
    }
    if lower.contains("could not compute box model") {
        return CdpErrorCode::NotInteractable;
    }
    if lower.contains("navigation timeout") {
        return CdpErrorCode::NavTimeout;
    }
    if lower.contains("target closed") || lower.contains("response channel dropped") {
        return CdpErrorCode::TargetClosed;
    }

    if let Some(n) = cdp_numeric_code
        && (-32700..=-32000).contains(&n)
    {
        return CdpErrorCode::ProtocolError;
    }

    CdpErrorCode::Generic
}

#[cfg(test)]
mod tests {
    use super::{CdpErrorCode, classify};

    #[test]
    fn cdp_error_code_mappings_are_stable() {
        assert_eq!(CdpErrorCode::NodeNotFound.code(), "CDP_NODE_NOT_FOUND");
        assert_eq!(CdpErrorCode::NotInteractable.code(), "CDP_NOT_INTERACTABLE");
        assert_eq!(CdpErrorCode::NavTimeout.code(), "CDP_NAV_TIMEOUT");
        assert_eq!(CdpErrorCode::TargetClosed.code(), "CDP_TARGET_CLOSED");
        assert_eq!(CdpErrorCode::ProtocolError.code(), "CDP_PROTOCOL_ERROR");
        assert_eq!(CdpErrorCode::Generic.code(), "CDP_GENERIC");
    }

    #[test]
    fn cdp_error_default_hints_match_contract() {
        assert_eq!(
            CdpErrorCode::NodeNotFound.default_hint(),
            "the referenced node is stale — call `actionbook browser snapshot` to refresh node references then retry"
        );
        assert_eq!(
            CdpErrorCode::NotInteractable.default_hint(),
            "the element exists but isn't interactable — scroll it into view, wait for it to become visible, or dismiss any overlay covering it"
        );
        assert_eq!(
            CdpErrorCode::NavTimeout.default_hint(),
            "navigation exceeded the deadline — increase `--timeout` or verify the target URL is reachable"
        );
        assert_eq!(
            CdpErrorCode::TargetClosed.default_hint(),
            "the CDP target was closed mid-command (tab navigated away or session torn down) — start a fresh session or re-attach to the tab"
        );
        assert_eq!(
            CdpErrorCode::ProtocolError.default_hint(),
            "the browser rejected the command — inspect `details.reason` and `details.cdp_code` for the raw protocol error"
        );
        assert_eq!(CdpErrorCode::Generic.default_hint(), "");
    }

    #[test]
    fn classify_node_not_found_by_message() {
        assert_eq!(
            classify("No node with given id found", Some(-32000)),
            CdpErrorCode::NodeNotFound
        );
    }

    #[test]
    fn classify_node_not_found_case_insensitively() {
        assert_eq!(
            classify("NO NODE WITH GIVEN ID FOUND", Some(-32000)),
            CdpErrorCode::NodeNotFound
        );
    }

    #[test]
    fn classify_not_interactable_by_message() {
        assert_eq!(
            classify(
                "CDP error -32000: Could not compute box model.",
                Some(-32000)
            ),
            CdpErrorCode::NotInteractable
        );
    }

    #[test]
    fn classify_nav_timeout_by_message() {
        assert_eq!(
            classify("Navigation timeout of 100 ms exceeded", Some(-32000)),
            CdpErrorCode::NavTimeout
        );
    }

    #[test]
    fn classify_target_closed_by_message() {
        assert_eq!(
            classify("Target closed.", Some(-32000)),
            CdpErrorCode::TargetClosed
        );
    }

    #[test]
    fn classify_response_channel_dropped_as_target_closed() {
        assert_eq!(
            classify("response channel dropped", None),
            CdpErrorCode::TargetClosed
        );
    }

    #[test]
    fn classify_message_patterns_before_numeric_fallback() {
        assert_eq!(
            classify("Cannot find context with specified id", Some(-32602)),
            CdpErrorCode::NodeNotFound
        );
    }

    #[test]
    fn classify_protocol_error_by_numeric_code() {
        assert_eq!(
            classify("CDP error -32602: invalid params", Some(-32602)),
            CdpErrorCode::ProtocolError
        );
    }

    #[test]
    fn classify_pre_target_handshake_failure_as_generic() {
        assert_eq!(
            classify("no response from CDP", None),
            CdpErrorCode::Generic
        );
    }

    #[test]
    fn classify_empty_message_as_generic() {
        assert_eq!(classify("", None), CdpErrorCode::Generic);
    }

    #[test]
    fn classify_non_cdp_garbage_as_generic() {
        assert_eq!(
            classify("socket parse exploded", None),
            CdpErrorCode::Generic
        );
    }

    #[test]
    fn from_wire_code_round_trips_structured_codes_only() {
        assert_eq!(
            CdpErrorCode::from_wire_code("CDP_NODE_NOT_FOUND"),
            Some(CdpErrorCode::NodeNotFound)
        );
        assert_eq!(
            CdpErrorCode::from_wire_code("CDP_NOT_INTERACTABLE"),
            Some(CdpErrorCode::NotInteractable)
        );
        assert_eq!(
            CdpErrorCode::from_wire_code("CDP_NAV_TIMEOUT"),
            Some(CdpErrorCode::NavTimeout)
        );
        assert_eq!(
            CdpErrorCode::from_wire_code("CDP_TARGET_CLOSED"),
            Some(CdpErrorCode::TargetClosed)
        );
        assert_eq!(
            CdpErrorCode::from_wire_code("CDP_PROTOCOL_ERROR"),
            Some(CdpErrorCode::ProtocolError)
        );
        assert_eq!(CdpErrorCode::from_wire_code("CDP_GENERIC"), None);
    }

    #[test]
    fn cdp_error_retryability_matches_contract() {
        assert!(!CdpErrorCode::NodeNotFound.is_retryable());
        assert!(!CdpErrorCode::NotInteractable.is_retryable());
        assert!(CdpErrorCode::NavTimeout.is_retryable());
        assert!(CdpErrorCode::TargetClosed.is_retryable());
        assert!(!CdpErrorCode::ProtocolError.is_retryable());
        assert!(!CdpErrorCode::Generic.is_retryable());
    }
}
