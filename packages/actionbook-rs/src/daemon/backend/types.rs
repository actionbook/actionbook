//! Types for the BrowserBackend trait layer.
//!
//! These types define the contract between the daemon's session actor and
//! the concrete backend implementations (Local, Extension, Cloud).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// BackendKind
// ---------------------------------------------------------------------------

/// Identifies which backend implementation is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Local,
    Extension,
    Cloud,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendKind::Local => write!(f, "local"),
            BackendKind::Extension => write!(f, "extension"),
            BackendKind::Cloud => write!(f, "cloud"),
        }
    }
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// Declares what a backend can and cannot do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Backend can launch a new browser process.
    pub can_launch: bool,
    /// Backend can attach to an existing browser.
    pub can_attach: bool,
    /// Backend can resume from a checkpoint.
    pub can_resume: bool,
    /// Backend supports headless mode.
    pub supports_headless: bool,
}

// ---------------------------------------------------------------------------
// StartSpec
// ---------------------------------------------------------------------------

/// Parameters for starting a new browser session via a backend.
#[derive(Debug, Clone)]
pub struct StartSpec {
    /// Profile name (determines user-data-dir for local).
    pub profile: String,
    /// Whether to launch headless.
    pub headless: bool,
    /// URL to open immediately after launch.
    pub open_url: Option<String>,
    /// Extra Chrome flags.
    pub extra_args: Vec<String>,
}

// ---------------------------------------------------------------------------
// AttachSpec
// ---------------------------------------------------------------------------

/// Parameters for attaching to an existing browser.
#[derive(Debug, Clone)]
pub struct AttachSpec {
    /// WebSocket URL of the browser's CDP endpoint.
    pub ws_url: String,
    /// Optional HTTP headers for the WS handshake (e.g. auth for cloud).
    pub headers: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Checkpoint
// ---------------------------------------------------------------------------

/// Serializable state for crash recovery / session resume.
///
/// Each backend produces its own checkpoint; the daemon persists it and
/// hands it back via `BackendFactory::resume()` after a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Which backend produced this checkpoint.
    pub kind: BackendKind,
    /// Browser process ID (Local only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// CDP WebSocket URL.
    pub ws_url: String,
    /// CDP port (Local only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdp_port: Option<u16>,
    /// User data directory (Local only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_data_dir: Option<PathBuf>,
    /// Optional WS auth headers (Cloud only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// BackendEvent
// ---------------------------------------------------------------------------

/// Events emitted by a backend session (via the `events()` stream).
#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// The WebSocket connection to the browser was lost.
    Disconnected {
        /// Human-readable reason.
        reason: String,
    },
    /// A new target (tab) was created.
    TargetCreated { target_id: String },
    /// A target (tab) was destroyed.
    TargetDestroyed { target_id: String },
    /// A JavaScript dialog appeared.
    Dialog { message: String },
}

// ---------------------------------------------------------------------------
// OpResult
// ---------------------------------------------------------------------------

/// The result of executing a [`BackendOp`](super::super::backend_op::BackendOp).
///
/// For now this wraps a raw `serde_json::Value` since CDP responses are
/// untyped. As the protocol matures, specific op results may get their own
/// typed variants.
#[derive(Debug, Clone)]
pub struct OpResult {
    /// Raw CDP response `result` field.
    pub value: serde_json::Value,
}

impl OpResult {
    /// Wrap a raw CDP response value.
    pub fn new(value: serde_json::Value) -> Self {
        Self { value }
    }

    /// Create a null (empty) result.
    pub fn null() -> Self {
        Self {
            value: serde_json::Value::Null,
        }
    }
}

// ---------------------------------------------------------------------------
// TargetInfo
// ---------------------------------------------------------------------------

/// Information about a CDP target (tab).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    /// CDP target ID.
    pub target_id: String,
    /// Target type (e.g. "page", "background_page", "service_worker").
    pub target_type: String,
    /// Page title.
    pub title: String,
    /// Page URL.
    pub url: String,
    /// Whether this target is currently attached to.
    pub attached: bool,
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Health status returned by `BackendSession::health()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    /// Whether the backend connection is alive.
    pub connected: bool,
    /// Browser product string (e.g. "Chrome/125.0.6422.76").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_version: Option<String>,
    /// Browser uptime if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// ShutdownPolicy
// ---------------------------------------------------------------------------

/// Controls how the backend shuts down the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPolicy {
    /// Send `Browser.close` and wait for graceful exit.
    Graceful,
    /// Kill the process immediately (Local only, no-op for Cloud/Extension).
    ForceKill,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_kind_display() {
        assert_eq!(BackendKind::Local.to_string(), "local");
        assert_eq!(BackendKind::Extension.to_string(), "extension");
        assert_eq!(BackendKind::Cloud.to_string(), "cloud");
    }

    #[test]
    fn backend_kind_serde_round_trip() {
        for kind in [
            BackendKind::Local,
            BackendKind::Extension,
            BackendKind::Cloud,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let decoded: BackendKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, decoded);
        }
    }

    #[test]
    fn checkpoint_serde_round_trip_local() {
        let cp = Checkpoint {
            kind: BackendKind::Local,
            pid: Some(12345),
            ws_url: "ws://127.0.0.1:9222/devtools/browser/abc".into(),
            cdp_port: Some(9222),
            user_data_dir: Some(PathBuf::from("/tmp/actionbook/profiles/default")),
            headers: None,
        };
        let json = serde_json::to_string(&cp).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.kind, BackendKind::Local);
        assert_eq!(decoded.pid, Some(12345));
        assert_eq!(decoded.cdp_port, Some(9222));
        assert!(decoded.headers.is_none());
    }

    #[test]
    fn checkpoint_serde_round_trip_cloud() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), "Bearer token123".into());
        let cp = Checkpoint {
            kind: BackendKind::Cloud,
            pid: None,
            ws_url: "wss://cloud.example.com/browser".into(),
            cdp_port: None,
            user_data_dir: None,
            headers: Some(headers),
        };
        let json = serde_json::to_string(&cp).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.kind, BackendKind::Cloud);
        assert!(decoded.pid.is_none());
        assert!(decoded.headers.is_some());
        assert_eq!(
            decoded.headers.unwrap().get("Authorization").unwrap(),
            "Bearer token123"
        );
    }

    #[test]
    fn op_result_null() {
        let r = OpResult::null();
        assert!(r.value.is_null());
    }

    #[test]
    fn op_result_from_value() {
        let r = OpResult::new(serde_json::json!({"nodeId": 42}));
        assert_eq!(r.value["nodeId"], 42);
    }

    #[test]
    fn target_info_serde_round_trip() {
        let ti = TargetInfo {
            target_id: "ABC123".into(),
            target_type: "page".into(),
            title: "Example".into(),
            url: "https://example.com".into(),
            attached: false,
        };
        let json = serde_json::to_string(&ti).unwrap();
        let decoded: TargetInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.target_id, "ABC123");
        assert_eq!(decoded.target_type, "page");
        assert!(!decoded.attached);
    }

    #[test]
    fn health_serde_round_trip() {
        let h = Health {
            connected: true,
            browser_version: Some("Chrome/125.0.6422.76".into()),
            uptime_secs: Some(3600),
        };
        let json = serde_json::to_string(&h).unwrap();
        let decoded: Health = serde_json::from_str(&json).unwrap();
        assert!(decoded.connected);
        assert_eq!(
            decoded.browser_version.as_deref(),
            Some("Chrome/125.0.6422.76")
        );
    }

    #[test]
    fn health_minimal_serde() {
        let h = Health {
            connected: false,
            browser_version: None,
            uptime_secs: None,
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(!json.contains("browser_version"));
        assert!(!json.contains("uptime_secs"));
        let decoded: Health = serde_json::from_str(&json).unwrap();
        assert!(!decoded.connected);
    }

    #[test]
    fn capabilities_serde_round_trip() {
        let caps = Capabilities {
            can_launch: true,
            can_attach: true,
            can_resume: true,
            supports_headless: true,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: Capabilities = serde_json::from_str(&json).unwrap();
        assert!(decoded.can_launch);
        assert!(decoded.supports_headless);
    }

    #[test]
    fn shutdown_policy_equality() {
        assert_eq!(ShutdownPolicy::Graceful, ShutdownPolicy::Graceful);
        assert_ne!(ShutdownPolicy::Graceful, ShutdownPolicy::ForceKill);
    }

    #[test]
    fn backend_event_debug() {
        let e = BackendEvent::Disconnected {
            reason: "ws closed".into(),
        };
        let debug = format!("{e:?}");
        assert!(debug.contains("Disconnected"));
        assert!(debug.contains("ws closed"));
    }

    #[test]
    fn backend_event_target_created() {
        let e = BackendEvent::TargetCreated {
            target_id: "T1".into(),
        };
        match e {
            BackendEvent::TargetCreated { target_id } => assert_eq!(target_id, "T1"),
            _ => panic!("wrong variant"),
        }
    }
}
