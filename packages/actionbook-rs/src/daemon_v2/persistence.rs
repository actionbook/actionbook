//! Daemon state persistence — save/load session registry to disk.
//!
//! The daemon persists its session registry to `~/.actionbook/daemon-state.json`
//! so that after a crash or restart it can attempt to reconnect to browser
//! processes that are still alive.
//!
//! Only **durable** state is persisted (session identity, backend kind, profile,
//! tab aliases, backend-specific checkpoint). **Ephemeral** state (WebSocket
//! connections, CDP session IDs, DOM handles) is not persisted.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::types::{Mode, SessionId, TabId};

// ---------------------------------------------------------------------------
// DaemonStateFile — top-level persisted structure
// ---------------------------------------------------------------------------

/// Top-level structure persisted to `daemon-state.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonStateFile {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// All persisted sessions.
    pub sessions: Vec<PersistedSession>,
}

impl DaemonStateFile {
    /// Current schema version.
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a new empty state file.
    pub fn new() -> Self {
        DaemonStateFile {
            version: Self::CURRENT_VERSION,
            sessions: Vec::new(),
        }
    }
}

impl Default for DaemonStateFile {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// PersistedSession
// ---------------------------------------------------------------------------

/// A single session's durable state, sufficient for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedSession {
    /// Stable UUID for correlating across daemon restarts.
    pub uuid: String,
    /// Short alias (s0, s1, ...) used in the wire protocol.
    pub id: SessionId,
    /// Browser connection mode.
    pub mode: Mode,
    /// Profile name that was used to start this session.
    pub profile: String,
    /// Persisted tab entries.
    pub tabs: Vec<PersistedTab>,
    /// Backend-specific checkpoint for reconnection.
    pub checkpoint: BackendCheckpoint,
}

// ---------------------------------------------------------------------------
// PersistedTab
// ---------------------------------------------------------------------------

/// A tab's durable state — alias and stable target key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedTab {
    /// Short alias (t0, t1, ...).
    pub id: TabId,
    /// Stable CDP target key (survives Chrome restarts better than target ID).
    pub stable_target_key: String,
    /// Last known URL.
    pub url: String,
    /// Last known title.
    pub title: String,
}

// ---------------------------------------------------------------------------
// BackendCheckpoint — backend-specific recovery data
// ---------------------------------------------------------------------------

/// Backend-specific data needed to reconnect after a daemon restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum BackendCheckpoint {
    /// Local Chrome: PID, WebSocket URL, user-data-dir.
    Local(LocalCheckpoint),
    /// Extension bridge: port and extension ID.
    Extension(ExtensionCheckpoint),
    /// Cloud browser: WSS endpoint, auth headers, resume token.
    Cloud(CloudCheckpoint),
}

/// Checkpoint for a locally-launched Chrome process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalCheckpoint {
    /// Chrome process ID.
    pub pid: u32,
    /// CDP WebSocket URL (e.g. `ws://127.0.0.1:9222/devtools/browser/...`).
    pub ws_url: String,
    /// Path to the Chrome user-data-dir.
    pub user_data_dir: String,
}

/// Checkpoint for an extension-bridge session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionCheckpoint {
    /// Port the daemon's extension bridge WS server listens on.
    pub bridge_port: u16,
    /// Chrome extension ID.
    pub extension_id: String,
}

/// Checkpoint for a cloud browser session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudCheckpoint {
    /// Remote WSS endpoint URL.
    pub wss_endpoint: String,
    /// Auth headers required for reconnection.
    pub auth_headers: HashMap<String, String>,
    /// Opaque resume token from the cloud provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Default state file path
// ---------------------------------------------------------------------------

/// Returns the default path for `daemon-state.json`: `~/.actionbook/daemon-state.json`.
///
/// Returns `None` if the home directory cannot be determined.
pub fn default_state_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".actionbook").join("daemon-state.json"))
}

// ---------------------------------------------------------------------------
// save / load
// ---------------------------------------------------------------------------

/// Persist daemon state atomically: write to a temp file, fsync, then rename.
///
/// This ensures that a crash mid-write never leaves a corrupt state file.
pub fn save_state(path: &Path, state: &DaemonStateFile) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir"))?;

    // Ensure the directory exists.
    fs::create_dir_all(parent)?;

    // Write to a temp file in the same directory (so rename is atomic on POSIX).
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    tmp.write_all(json.as_bytes())?;
    tmp.as_file().sync_all()?;

    // Atomic rename.
    tmp.persist(path)?;
    Ok(())
}

/// Load daemon state from disk.
///
/// - Returns `Ok(state)` on success.
/// - Returns `Ok(DaemonStateFile::new())` if the file does not exist.
/// - Returns `Err` if the file exists but cannot be read or parsed.
pub fn load_state(path: &Path) -> std::io::Result<DaemonStateFile> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let state: DaemonStateFile = serde_json::from_str(&contents)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(state)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DaemonStateFile::new()),
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> DaemonStateFile {
        DaemonStateFile {
            version: DaemonStateFile::CURRENT_VERSION,
            sessions: vec![
                PersistedSession {
                    uuid: "550e8400-e29b-41d4-a716-446655440000".into(),
                    id: SessionId(0),
                    mode: Mode::Local,
                    profile: "default".into(),
                    tabs: vec![
                        PersistedTab {
                            id: TabId(0),
                            stable_target_key: "ABC123".into(),
                            url: "https://example.com".into(),
                            title: "Example".into(),
                        },
                        PersistedTab {
                            id: TabId(1),
                            stable_target_key: "DEF456".into(),
                            url: "https://rust-lang.org".into(),
                            title: "Rust".into(),
                        },
                    ],
                    checkpoint: BackendCheckpoint::Local(LocalCheckpoint {
                        pid: 12345,
                        ws_url: "ws://127.0.0.1:9222/devtools/browser/abc".into(),
                        user_data_dir: "/tmp/chrome-profile".into(),
                    }),
                },
                PersistedSession {
                    uuid: "660e8400-e29b-41d4-a716-446655440001".into(),
                    id: SessionId(1),
                    mode: Mode::Cloud,
                    profile: "work".into(),
                    tabs: vec![PersistedTab {
                        id: TabId(0),
                        stable_target_key: "GHI789".into(),
                        url: "https://app.example.com".into(),
                        title: "App".into(),
                    }],
                    checkpoint: BackendCheckpoint::Cloud(CloudCheckpoint {
                        wss_endpoint: "wss://cloud.example.com/session/xyz".into(),
                        auth_headers: HashMap::from([("Authorization".into(), "Bearer tok".into())]),
                        resume_token: Some("resume-abc".into()),
                    }),
                },
            ],
        }
    }

    #[test]
    fn serde_round_trip() {
        let state = sample_state();
        let json = serde_json::to_string_pretty(&state).unwrap();
        let decoded: DaemonStateFile = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon-state.json");
        let state = sample_state();

        save_state(&path, &state).unwrap();
        let loaded = load_state(&path).unwrap();
        assert_eq!(state, loaded);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let state = load_state(&path).unwrap();
        assert_eq!(state, DaemonStateFile::new());
        assert!(state.sessions.is_empty());
    }

    #[test]
    fn load_corrupt_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon-state.json");
        fs::write(&path, "not valid json {{{").unwrap();
        let result = load_state(&path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("state.json");
        let state = DaemonStateFile::new();
        save_state(&path, &state).unwrap();
        let loaded = load_state(&path).unwrap();
        assert_eq!(state, loaded);
    }

    #[test]
    fn extension_checkpoint_serde() {
        let session = PersistedSession {
            uuid: "ext-uuid".into(),
            id: SessionId(2),
            mode: Mode::Extension,
            profile: "ext".into(),
            tabs: vec![],
            checkpoint: BackendCheckpoint::Extension(ExtensionCheckpoint {
                bridge_port: 9333,
                extension_id: "abcdefghijklmnop".into(),
            }),
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains(r#""kind":"Extension""#));
        let decoded: PersistedSession = serde_json::from_str(&json).unwrap();
        assert_eq!(session, decoded);
    }

    #[test]
    fn cloud_checkpoint_without_resume_token() {
        let cp = BackendCheckpoint::Cloud(CloudCheckpoint {
            wss_endpoint: "wss://example.com".into(),
            auth_headers: HashMap::new(),
            resume_token: None,
        });
        let json = serde_json::to_string(&cp).unwrap();
        // resume_token should be skipped when None
        assert!(!json.contains("resume_token"));
        let decoded: BackendCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(cp, decoded);
    }

    #[test]
    fn default_state_path_is_under_home() {
        if let Some(path) = default_state_path() {
            assert!(path.ends_with(".actionbook/daemon-state.json"));
        }
        // On CI without a home dir, this may return None — that's fine.
    }

    #[test]
    fn empty_state_version() {
        let state = DaemonStateFile::new();
        assert_eq!(state.version, DaemonStateFile::CURRENT_VERSION);
    }
}
