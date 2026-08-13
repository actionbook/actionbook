use clap::Args;
use serde::{Deserialize, Serialize};

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Stop an owned local browser while preserving its named profile
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser stop --session my-session

Stops an Actionbook-owned local browser and all its tabs while retaining the named profile directory and authentication state.
Use `browser close` instead when deletion of a temporary non-default profile is intended.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser stop";

pub fn context(cmd: &Cmd, _result: &ActionResult) -> Option<ResponseContext> {
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    super::close::execute_teardown(
        &cmd.session,
        registry,
        super::close::TeardownMode::PreserveProfile,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::daemon::registry::{SessionEntry, SessionState, new_shared_registry};
    use crate::output::JsonEnvelope;
    use crate::types::{Mode, SessionId};

    #[tokio::test]
    async fn preserve_profile_stop_is_idempotent_when_session_is_already_gone() {
        let registry = new_shared_registry();
        let cmd = Cmd {
            session: "missing-session".to_string(),
        };

        let result = execute(&cmd, &registry).await;
        let envelope = JsonEnvelope::from_result(
            COMMAND_NAME,
            context(&cmd, &result),
            &result,
            Duration::from_millis(1),
        );

        assert!(envelope.ok);
        assert_eq!(envelope.data["status"], "stopped");
        assert_eq!(envelope.data["closed_tabs"], 0);
        assert!(envelope.data["profile"].is_null());
        assert!(envelope.data["profile_preserved"].is_null());
        assert!(
            envelope
                .meta
                .warnings
                .iter()
                .any(|warning| warning.contains("already closed") || warning.contains("not found"))
        );
    }

    #[tokio::test]
    async fn preserve_profile_stop_rejects_attached_local_session_without_teardown() {
        let registry = new_shared_registry();
        let mut entry = SessionEntry::starting(
            SessionId::new("attached-session").expect("session id"),
            Mode::Local,
            true,
            true,
            "external-profile".to_string(),
        );
        entry.status = SessionState::Running;
        registry.lock().await.insert(entry);

        let result = execute(
            &Cmd {
                session: "attached-session".to_string(),
            },
            &registry,
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "UNSUPPORTED_MODE"),
            other => panic!("expected unsupported-mode fatal, got {other:?}"),
        }
        assert!(
            registry.lock().await.get("attached-session").is_some(),
            "rejected stop must leave the attached session running"
        );
    }

    #[tokio::test]
    async fn preserve_profile_stop_rejects_non_local_session_without_teardown() {
        let registry = new_shared_registry();
        let mut entry = SessionEntry::starting(
            SessionId::new("cloud-session").expect("session id"),
            Mode::Cloud,
            true,
            true,
            "remote-profile".to_string(),
        );
        entry.status = SessionState::Running;
        registry.lock().await.insert(entry);

        let result = execute(
            &Cmd {
                session: "cloud-session".to_string(),
            },
            &registry,
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "UNSUPPORTED_MODE"),
            other => panic!("expected unsupported-mode fatal, got {other:?}"),
        }
        assert!(
            registry.lock().await.get("cloud-session").is_some(),
            "rejected stop must leave the session running"
        );
    }
}
