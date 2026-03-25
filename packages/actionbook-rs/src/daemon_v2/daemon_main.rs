//! Daemon entry point — initializes the daemon runtime and starts the UDS server.
//!
//! This module wires together the router, server, persistence, and recovery
//! modules into a running daemon process.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{info, warn};

use super::persistence;
use super::recovery;
use super::registry::SessionRegistry;
use super::router::Router;
use super::server::{self, DaemonServer};

/// Configuration for the daemon process.
pub struct DaemonConfig {
    /// Path to the UDS socket.
    pub socket_path: std::path::PathBuf,
    /// Path to the PID file.
    pub pid_path: std::path::PathBuf,
    /// Path to the persisted state file.
    pub state_path: std::path::PathBuf,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        DaemonConfig {
            socket_path: server::default_socket_path(),
            pid_path: server::default_pid_path(),
            state_path: persistence::default_state_path()
                .unwrap_or_else(|| std::path::PathBuf::from("daemon-state.json")),
        }
    }
}

/// Run the daemon with the given configuration.
///
/// This is the main entry point called from the daemon binary/fork.
/// It loads persisted state, plans recovery, creates the router, and
/// starts the UDS server.
pub async fn run_daemon(config: DaemonConfig) -> std::io::Result<()> {
    info!("daemon starting");

    // Load persisted state.
    let state = persistence::load_state(&config.state_path)?;
    info!(
        "loaded {} persisted session(s) from {}",
        state.sessions.len(),
        config.state_path.display()
    );

    // Plan recovery for persisted sessions.
    let plan = recovery::plan_recovery(&state.sessions);
    for action in &plan.actions {
        match &action.action {
            recovery::RecoveryAction::Reconnect { .. } => {
                info!("session {}: will attempt reconnect", action.session_id);
            }
            recovery::RecoveryAction::MarkLost { reason } => {
                warn!("session {}: marked lost — {reason}", action.session_id);
            }
            recovery::RecoveryAction::MarkUserAction { reason } => {
                warn!(
                    "session {}: needs user action — {reason}",
                    action.session_id
                );
            }
        }
    }

    // TODO: Execute recovery plan (Phase 1.3+) — reconnect sessions via backend factory.
    // For now, we start fresh with an empty registry.

    let registry = Arc::new(Mutex::new(SessionRegistry::new()));
    let router = Arc::new(Router::new(registry));
    let shutdown = Arc::new(AtomicBool::new(false));

    let server = DaemonServer::new(config.socket_path, config.pid_path, router);
    server.run(shutdown).await?;

    info!("daemon stopped");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_paths() {
        let config = DaemonConfig::default();
        assert!(config.socket_path.to_string_lossy().contains("v2.sock"));
        assert!(config.pid_path.to_string_lossy().contains("v2.pid"));
        assert!(config
            .state_path
            .to_string_lossy()
            .contains("daemon-state.json"));
    }
}
