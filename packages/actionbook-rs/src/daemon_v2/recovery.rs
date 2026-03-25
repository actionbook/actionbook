//! Crash recovery planning — decide how to reconcile each persisted session.
//!
//! After a daemon restart, the daemon reads `daemon-state.json` and uses this
//! module to build a [`RecoveryPlan`]. The plan describes what action to take
//! for each persisted session (reconnect, mark lost, or request user action)
//! **without** performing the actual reconnection. The backend layer executes
//! the plan via `backend.resume(checkpoint)`.
//!
//! Decision logic per backend mode:
//!
//! | Mode      | Decision                                                |
//! |-----------|---------------------------------------------------------|
//! | Local     | PID alive + is Chrome? -> Reconnect. Otherwise MarkLost |
//! | Extension | Always MarkUserAction (needs extension reconnection)    |
//! | Cloud     | Always Reconnect (try resume token)                     |

use serde::{Deserialize, Serialize};

use super::persistence::{BackendCheckpoint, PersistedSession};
use super::types::{Mode, SessionId};

// ---------------------------------------------------------------------------
// RecoveryPlan
// ---------------------------------------------------------------------------

/// A plan describing how to recover each persisted session after daemon restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryPlan {
    pub actions: Vec<SessionRecoveryAction>,
}

/// The recovery action for a single session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionRecoveryAction {
    /// Which session this action applies to.
    pub session_id: SessionId,
    /// The decided recovery strategy.
    pub action: RecoveryAction,
}

/// What to do with a session during recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum RecoveryAction {
    /// Attempt to reconnect using the persisted checkpoint.
    Reconnect {
        checkpoint: BackendCheckpoint,
    },
    /// Mark the session as lost (browser process gone, cannot recover).
    MarkLost {
        reason: String,
    },
    /// Mark the session as needing user intervention.
    MarkUserAction {
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// plan_recovery
// ---------------------------------------------------------------------------

/// Build a recovery plan for a list of persisted sessions.
///
/// For each session, the decision is based on backend mode:
/// - **Local**: check if the PID is alive and belongs to a Chrome process.
///   If yes, plan a reconnect. Otherwise, mark lost.
/// - **Extension**: always requires user action (the extension must reconnect).
/// - **Cloud**: always attempt reconnect (using the resume token if available).
pub fn plan_recovery(sessions: &[PersistedSession]) -> RecoveryPlan {
    let actions = sessions.iter().map(plan_single_session).collect();
    RecoveryPlan { actions }
}

fn plan_single_session(session: &PersistedSession) -> SessionRecoveryAction {
    let action = match (&session.mode, &session.checkpoint) {
        (Mode::Local, BackendCheckpoint::Local(cp)) => plan_local_recovery(cp),
        (Mode::Extension, _) => RecoveryAction::MarkUserAction {
            reason: "extension reconnection required — open Chrome and click the Actionbook extension icon".into(),
        },
        (Mode::Cloud, checkpoint) => RecoveryAction::Reconnect {
            checkpoint: checkpoint.clone(),
        },
        // Mode/checkpoint mismatch — should not happen, but handle gracefully.
        _ => RecoveryAction::MarkLost {
            reason: format!(
                "mode/checkpoint mismatch: mode={}, checkpoint does not match",
                session.mode
            ),
        },
    };
    SessionRecoveryAction {
        session_id: session.id,
        action,
    }
}

fn plan_local_recovery(
    cp: &super::persistence::LocalCheckpoint,
) -> RecoveryAction {
    if is_process_alive(cp.pid) && is_chrome_process(cp.pid) {
        RecoveryAction::Reconnect {
            checkpoint: BackendCheckpoint::Local(cp.clone()),
        }
    } else if is_process_alive(cp.pid) {
        RecoveryAction::MarkLost {
            reason: format!(
                "PID {} is alive but is not a Chrome process — it may have been reused by the OS",
                cp.pid
            ),
        }
    } else {
        RecoveryAction::MarkLost {
            reason: format!("Chrome process (PID {}) is no longer running", cp.pid),
        }
    }
}

// ---------------------------------------------------------------------------
// Process inspection helpers
// ---------------------------------------------------------------------------

/// Check whether a process with the given PID is still alive.
///
/// Uses `kill(pid, 0)` on Unix — this sends no signal but checks process existence.
#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    // Safety: kill(pid, 0) is a standard POSIX existence check.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    // On non-Unix platforms, conservatively assume the process is gone.
    false
}

/// Check whether the process with the given PID is a Chrome/Chromium process.
///
/// On macOS: uses `ps -p <pid> -o comm=` and checks for chrome/chromium in the name.
/// On Linux: reads `/proc/<pid>/comm`.
#[cfg(target_os = "macos")]
pub fn is_chrome_process(pid: u32) -> bool {
    match std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
    {
        Ok(output) => {
            let name = String::from_utf8_lossy(&output.stdout).to_lowercase();
            name.contains("chrome") || name.contains("chromium")
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "linux")]
pub fn is_chrome_process(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/comm")) {
        Ok(name) => {
            let lower = name.trim().to_lowercase();
            lower.contains("chrome") || lower.contains("chromium")
        }
        Err(_) => false,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn is_chrome_process(_pid: u32) -> bool {
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_v2::persistence::*;
    use crate::daemon_v2::types::*;
    use std::collections::HashMap;

    fn local_session(id: u32, pid: u32) -> PersistedSession {
        PersistedSession {
            uuid: format!("uuid-local-{id}"),
            id: SessionId(id),
            mode: Mode::Local,
            profile: "default".into(),
            tabs: vec![],
            checkpoint: BackendCheckpoint::Local(LocalCheckpoint {
                pid,
                ws_url: format!("ws://127.0.0.1:9222/devtools/browser/{id}"),
                user_data_dir: format!("/tmp/chrome-{id}"),
            }),
        }
    }

    fn extension_session(id: u32) -> PersistedSession {
        PersistedSession {
            uuid: format!("uuid-ext-{id}"),
            id: SessionId(id),
            mode: Mode::Extension,
            profile: "ext".into(),
            tabs: vec![],
            checkpoint: BackendCheckpoint::Extension(ExtensionCheckpoint {
                bridge_port: 9333,
                extension_id: "ext-abc".into(),
            }),
        }
    }

    fn cloud_session(id: u32) -> PersistedSession {
        PersistedSession {
            uuid: format!("uuid-cloud-{id}"),
            id: SessionId(id),
            mode: Mode::Cloud,
            profile: "work".into(),
            tabs: vec![],
            checkpoint: BackendCheckpoint::Cloud(CloudCheckpoint {
                wss_endpoint: "wss://cloud.example.com/session/xyz".into(),
                auth_headers: HashMap::new(),
                resume_token: Some("resume-tok".into()),
            }),
        }
    }

    #[test]
    fn empty_sessions_produce_empty_plan() {
        let plan = plan_recovery(&[]);
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn local_session_with_dead_pid_marks_lost() {
        // Use PID 0 which is never a user Chrome process.
        // On most systems kill(0, 0) returns 0 (checks own process group),
        // but it won't be a Chrome process, so the outcome is MarkLost.
        let session = local_session(0, 999_999_999);
        let plan = plan_recovery(&[session]);
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].session_id, SessionId(0));
        match &plan.actions[0].action {
            RecoveryAction::MarkLost { reason } => {
                assert!(
                    reason.contains("no longer running") || reason.contains("not a Chrome"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected MarkLost, got {other:?}"),
        }
    }

    #[test]
    fn extension_session_always_marks_user_action() {
        let session = extension_session(1);
        let plan = plan_recovery(&[session]);
        assert_eq!(plan.actions.len(), 1);
        match &plan.actions[0].action {
            RecoveryAction::MarkUserAction { reason } => {
                assert!(reason.contains("extension"));
            }
            other => panic!("expected MarkUserAction, got {other:?}"),
        }
    }

    #[test]
    fn cloud_session_always_reconnects() {
        let session = cloud_session(2);
        let plan = plan_recovery(&[session]);
        assert_eq!(plan.actions.len(), 1);
        match &plan.actions[0].action {
            RecoveryAction::Reconnect { checkpoint } => {
                assert!(matches!(checkpoint, BackendCheckpoint::Cloud(_)));
            }
            other => panic!("expected Reconnect, got {other:?}"),
        }
    }

    #[test]
    fn mixed_sessions_produce_correct_plan() {
        let sessions = vec![
            local_session(0, 999_999_999),
            extension_session(1),
            cloud_session(2),
        ];
        let plan = plan_recovery(&sessions);
        assert_eq!(plan.actions.len(), 3);

        // Local with dead PID -> MarkLost
        assert!(matches!(
            &plan.actions[0].action,
            RecoveryAction::MarkLost { .. }
        ));
        // Extension -> MarkUserAction
        assert!(matches!(
            &plan.actions[1].action,
            RecoveryAction::MarkUserAction { .. }
        ));
        // Cloud -> Reconnect
        assert!(matches!(
            &plan.actions[2].action,
            RecoveryAction::Reconnect { .. }
        ));
    }

    #[test]
    fn recovery_plan_serde_round_trip() {
        let plan = plan_recovery(&[cloud_session(0), extension_session(1)]);
        let json = serde_json::to_string_pretty(&plan).unwrap();
        let decoded: RecoveryPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, decoded);
    }

    #[test]
    fn mode_checkpoint_mismatch_marks_lost() {
        // Artificially create a mode/checkpoint mismatch.
        let bad = PersistedSession {
            uuid: "uuid-bad".into(),
            id: SessionId(99),
            mode: Mode::Local,
            profile: "default".into(),
            tabs: vec![],
            checkpoint: BackendCheckpoint::Cloud(CloudCheckpoint {
                wss_endpoint: "wss://wrong".into(),
                auth_headers: HashMap::new(),
                resume_token: None,
            }),
        };
        let plan = plan_recovery(&[bad]);
        match &plan.actions[0].action {
            RecoveryAction::MarkLost { reason } => {
                assert!(reason.contains("mismatch"));
            }
            other => panic!("expected MarkLost for mismatch, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn current_process_is_alive() {
        let pid = std::process::id();
        assert!(is_process_alive(pid));
    }

    #[cfg(unix)]
    #[test]
    fn bogus_pid_is_not_alive() {
        // PID close to u32::MAX is almost certainly not in use.
        assert!(!is_process_alive(4_000_000_000));
    }

    #[test]
    fn current_process_is_not_chrome() {
        let pid = std::process::id();
        assert!(!is_chrome_process(pid));
    }
}
