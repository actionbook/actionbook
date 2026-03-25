//! Request Router — dispatches Actions to global handlers or session actors.
//!
//! The router is the daemon's front door. It receives [`Action`]s from the UDS
//! server, classifies them by addressing level, and either handles them directly
//! (global commands) or forwards them to the appropriate session actor via a
//! channel + oneshot pattern.

use std::sync::Arc;

use tokio::sync::{oneshot, Mutex};

use super::action::Action;
use super::action_result::ActionResult;
use super::registry::SessionRegistry;
use super::session_actor::ActionRequest;
use super::types::SessionId;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// The request router — classifies Actions and dispatches them.
pub struct Router {
    pub registry: Arc<Mutex<SessionRegistry>>,
}

impl Router {
    /// Create a new router with the given registry.
    pub fn new(registry: Arc<Mutex<SessionRegistry>>) -> Self {
        Router { registry }
    }

    /// Route an action to the appropriate handler and return the result.
    pub async fn route(&self, action: Action) -> ActionResult {
        match &action {
            // --- Global commands handled directly ---
            Action::ListSessions => self.handle_list_sessions().await,
            Action::StartSession { .. } => {
                // StartSession is handled by the caller (daemon_main / server)
                // because it requires spawning a session actor, which needs
                // access to the backend factory. The router returns a placeholder
                // so the server layer can intercept.
                ActionResult::fatal(
                    "not_implemented",
                    "StartSession must be handled by the server layer",
                    "this is an internal error — please report it",
                )
            }

            // --- Session/Tab commands: forward to session actor ---
            _ => {
                let session_id = match action.session_id() {
                    Some(id) => id,
                    None => {
                        return ActionResult::fatal(
                            "unknown_action",
                            "unrecognized global action",
                            "run `actionbook browser --help` for available commands",
                        );
                    }
                };
                self.forward_to_session(session_id, action).await
            }
        }
    }

    /// Handle `ListSessions` — returns all active sessions.
    async fn handle_list_sessions(&self) -> ActionResult {
        let registry = self.registry.lock().await;
        let summaries = registry.list_sessions();
        let sessions: Vec<serde_json::Value> = summaries
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id.to_string(),
                    "profile": s.profile,
                    "mode": s.mode.to_string(),
                    "state": s.state.to_string(),
                    "tab_count": s.tab_count,
                    "uptime_secs": s.uptime_secs,
                })
            })
            .collect();
        ActionResult::ok(serde_json::json!({ "sessions": sessions }))
    }

    /// Forward an action to the session actor via its channel.
    async fn forward_to_session(&self, session_id: SessionId, action: Action) -> ActionResult {
        // Clone the sender and release the lock immediately — never hold the
        // mutex across an await point (send can block if the channel is full).
        let tx = {
            let registry = self.registry.lock().await;
            match registry.get(session_id) {
                Some(h) => h.tx.clone(),
                None => {
                    return ActionResult::fatal(
                        "session_not_found",
                        &format!("session {session_id} does not exist"),
                        "run `actionbook browser list-sessions` to see available sessions",
                    );
                }
            }
        }; // lock released here

        let (reply_tx, reply_rx) = oneshot::channel();
        let msg = ActionRequest {
            action,
            response_tx: reply_tx,
        };

        // Try to send — if the channel is closed the session actor has died.
        if tx.send(msg).await.is_err() {
            return ActionResult::fatal(
                "session_dead",
                &format!("session {session_id} is no longer responding"),
                "run `actionbook browser list-sessions` to check session status",
            );
        }

        match reply_rx.await {
            Ok(result) => result,
            Err(_) => ActionResult::fatal(
                "session_dead",
                &format!("session {session_id} dropped the response"),
                "the session may have crashed — try again or close it",
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_v2::backend::TargetInfo;
    use crate::daemon_v2::registry::{SessionHandle, SessionState};
    use crate::daemon_v2::session_actor::SessionActor;
    use crate::daemon_v2::types::{Mode, TabId};

    // -- Mock backend (reusable for router tests) --
    use crate::daemon_v2::backend::{
        BackendEvent, BackendSession, Checkpoint, Health, OpResult, ShutdownPolicy,
    };
    use crate::daemon_v2::backend_op::BackendOp;
    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};
    use std::time::Instant;

    struct MockBackend;

    #[async_trait]
    impl BackendSession for MockBackend {
        fn events(&mut self) -> BoxStream<'static, BackendEvent> {
            Box::pin(stream::empty())
        }
        async fn exec(&mut self, _op: BackendOp) -> crate::error::Result<OpResult> {
            Ok(OpResult::null())
        }
        async fn list_targets(&self) -> crate::error::Result<Vec<TargetInfo>> {
            Ok(vec![])
        }
        async fn checkpoint(&self) -> crate::error::Result<Checkpoint> {
            Ok(Checkpoint {
                kind: crate::daemon_v2::backend::BackendKind::Local,
                pid: Some(1),
                ws_url: "ws://mock".into(),
                cdp_port: None,
                user_data_dir: None,
                headers: None,
            })
        }
        async fn health(&self) -> crate::error::Result<Health> {
            Ok(Health {
                connected: true,
                browser_version: None,
                uptime_secs: None,
            })
        }
        async fn shutdown(&mut self, _: ShutdownPolicy) -> crate::error::Result<()> {
            Ok(())
        }
    }

    fn spawn_mock_session(id: SessionId) -> SessionHandle {
        let backend = Box::new(MockBackend);
        let targets = vec![TargetInfo {
            target_id: "T1".into(),
            target_type: "page".into(),
            title: "Test".into(),
            url: "https://test.com".into(),
            attached: false,
        }];
        let (tx, _handle) = SessionActor::spawn(id, backend, targets);
        SessionHandle {
            tx,
            profile: "default".into(),
            mode: Mode::Local,
            state: SessionState::Ready,
            tab_count: 1,
            created_at: Instant::now(),
        }
    }

    #[tokio::test]
    async fn list_sessions_empty() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let router = Router::new(registry);
        let result = router.route(Action::ListSessions).await;
        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                let sessions = data["sessions"].as_array().unwrap();
                assert!(sessions.is_empty());
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn list_sessions_with_entries() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        {
            let mut reg = registry.lock().await;
            let h1 = spawn_mock_session(SessionId(0));
            let h2 = spawn_mock_session(SessionId(1));
            reg.register_session(h1);
            reg.register_session(h2);
        }
        let router = Router::new(registry);
        let result = router.route(Action::ListSessions).await;
        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                let sessions = data["sessions"].as_array().unwrap();
                assert_eq!(sessions.len(), 2);
                assert_eq!(sessions[0]["id"], "s0");
                assert_eq!(sessions[1]["id"], "s1");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn route_to_nonexistent_session_returns_fatal() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let router = Router::new(registry);
        let action = Action::Goto {
            session: SessionId(99),
            tab: TabId(0),
            url: "https://example.com".into(),
        };
        let result = router.route(action).await;
        match result {
            ActionResult::Fatal { code, hint, .. } => {
                assert_eq!(code, "session_not_found");
                assert!(hint.contains("list-sessions"));
            }
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn route_forwards_to_session_actor() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        {
            let mut reg = registry.lock().await;
            let handle = spawn_mock_session(SessionId(0));
            reg.register_session(handle);
        }
        let router = Router::new(registry);

        let action = Action::ListTabs {
            session: SessionId(0),
        };
        let result = router.route(action).await;
        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                // The mock session has 1 page target registered as a tab.
                let tabs = data["tabs"].as_array().unwrap();
                assert_eq!(tabs.len(), 1);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn route_to_dead_session_returns_fatal() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        {
            let mut reg = registry.lock().await;
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            drop(rx); // Simulate dead actor by dropping receiver.
            let handle = SessionHandle {
                tx,
                profile: "dead".into(),
                mode: Mode::Local,
                state: SessionState::Ready,
                tab_count: 0,
                created_at: Instant::now(),
            };
            reg.register_session(handle);
        }
        let router = Router::new(registry);

        let action = Action::ListTabs {
            session: SessionId(0),
        };
        let result = router.route(action).await;
        match result {
            ActionResult::Fatal { code, .. } => {
                assert_eq!(code, "session_dead");
            }
            _ => panic!("expected Fatal"),
        }
    }
}
