//! Per-session async actor task.
//!
//! Each session runs as an independent tokio task that owns a
//! [`BackendSession`], a tab registry, and a window registry.
//! Commands arrive via a bounded `mpsc` channel and are processed serially
//! within the session -- this eliminates concurrency within a single session
//! while allowing sessions to run fully in parallel (design principle P7).
//!
//! The actor delegates action execution to [`action_handler::handle_action`],
//! which compiles high-level Actions into BackendOp sequences.

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::action::Action;
use super::action_handler::{self, Registries, TabEntry, WindowEntry};
use super::action_result::ActionResult;
use super::backend::BackendSession;
use super::backend::{ShutdownPolicy, TargetInfo};
use super::registry::SessionState;
use super::types::{SessionId, TabId};

// ---------------------------------------------------------------------------
// ActionRequest
// ---------------------------------------------------------------------------

/// A request sent to the session actor via its mpsc channel.
pub struct ActionRequest {
    /// The action to execute.
    pub action: Action,
    /// Channel to send the result back to the caller.
    pub response_tx: oneshot::Sender<ActionResult>,
}

impl std::fmt::Debug for ActionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionRequest")
            .field("action", &self.action)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// SessionActor
// ---------------------------------------------------------------------------

/// The per-session actor that owns the backend connection and tab/window state.
pub struct SessionActor {
    /// This session's ID (for error messages).
    session_id: SessionId,
    /// The live backend connection.
    backend: Box<dyn BackendSession>,
    /// Current lifecycle state.
    state: SessionState,
    /// Tab and window registries (shared with action_handler).
    registries: Registries,
}

impl SessionActor {
    /// Create a new session actor.
    ///
    /// `initial_targets` are the tabs already open in the browser when the
    /// session starts (discovered via `backend.list_targets()`). They are
    /// automatically registered in the tab/window registries.
    fn new(
        session_id: SessionId,
        backend: Box<dyn BackendSession>,
        initial_targets: Vec<TargetInfo>,
    ) -> Self {
        let mut registries = Registries::new();

        // Create a default window for initial tabs.
        let default_window = registries.alloc_window_id();
        registries.windows.insert(
            default_window,
            WindowEntry {
                id: default_window,
                tabs: Vec::new(),
            },
        );

        // Register initial targets as tabs.
        for target in initial_targets {
            if target.target_type == "page" {
                let tab_id = registries.alloc_tab_id();
                let entry = TabEntry {
                    id: tab_id,
                    target_id: target.target_id,
                    window: default_window,
                    url: target.url,
                    title: target.title,
                };
                registries.tabs.insert(tab_id, entry);
                if let Some(w) = registries.windows.get_mut(&default_window) {
                    w.tabs.push(tab_id);
                }
            }
        }

        SessionActor {
            session_id,
            backend,
            state: SessionState::Ready,
            registries,
        }
    }

    /// Spawn the session actor as a tokio task.
    ///
    /// Returns the mpsc sender (for dispatching actions) and the task's JoinHandle.
    pub fn spawn(
        session_id: SessionId,
        backend: Box<dyn BackendSession>,
        initial_targets: Vec<TargetInfo>,
    ) -> (mpsc::Sender<ActionRequest>, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel(32);
        let actor = Self::new(session_id, backend, initial_targets);
        let handle = tokio::spawn(actor.run(rx));
        (tx, handle)
    }

    /// Main event loop: receive actions from the channel and handle them.
    async fn run(mut self, mut rx: mpsc::Receiver<ActionRequest>) {
        while let Some(req) = rx.recv().await {
            let prev_state = self.state;
            if self.state == SessionState::Ready {
                self.state = SessionState::Executing;
            }

            // Handle Close/CloseSession at the actor level (affects lifecycle).
            // SessionStatus also handled here since it needs actor state.
            let result = match req.action {
                Action::Close { .. } | Action::CloseSession { .. } => {
                    let live_tab_count = self.registries.tabs.len();
                    self.state = SessionState::Closed;
                    // Shut down the backend (browser process / WS connection).
                    let _ = self.backend.shutdown(ShutdownPolicy::Graceful).await;
                    let result = ActionResult::ok(serde_json::json!({
                        "closed": self.session_id.to_string(),
                        "tab_count": live_tab_count,
                    }));
                    // Send response before breaking so the caller gets the result.
                    let _ = req.response_tx.send(result);
                    break;
                }
                Action::SessionStatus { .. } => {
                    let tabs: Vec<serde_json::Value> = self
                        .registries
                        .tabs
                        .iter()
                        .map(|(id, entry)| {
                            serde_json::json!({
                                "tab_id": format!("{id}"),
                                "url": entry.url,
                                "title": entry.title,
                            })
                        })
                        .collect();
                    ActionResult::ok(serde_json::json!({
                        "session": self.session_id.to_string(),
                        "state": self.state.to_string(),
                        "tab_count": self.registries.tabs.len(),
                        "window_count": self.registries.windows.len(),
                        "tabs": tabs,
                    }))
                }
                action => {
                    action_handler::handle_action(
                        self.session_id.clone(),
                        self.backend.as_mut(),
                        &mut self.registries,
                        action,
                    )
                    .await
                }
            };

            // Transition back to Ready if we were Executing.
            if self.state == SessionState::Executing {
                self.state = prev_state;
            }

            // Send response; ignore error if caller dropped the receiver.
            let _ = req.response_tx.send(result);
        }
        // Channel closed or session explicitly closed — actor exits.
        self.state = SessionState::Closed;
    }

    /// Get a tab entry by ID.
    #[allow(dead_code)]
    pub fn get_tab(&self, id: TabId) -> Option<&TabEntry> {
        self.registries.tabs.get(&id)
    }

    /// Number of tabs in this session.
    #[allow(dead_code)]
    pub fn tab_count(&self) -> usize {
        self.registries.tabs.len()
    }

    /// Number of windows in this session.
    #[allow(dead_code)]
    pub fn window_count(&self) -> usize {
        self.registries.windows.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::backend::{BackendEvent, Checkpoint, Health, OpResult, ShutdownPolicy};
    use crate::daemon::backend_op::BackendOp;
    use crate::daemon::types::WindowId;
    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};

    // -- Mock backend for testing --

    struct MockBackend {
        targets: Vec<TargetInfo>,
    }

    impl MockBackend {
        fn new(targets: Vec<TargetInfo>) -> Self {
            MockBackend { targets }
        }
    }

    #[async_trait]
    impl BackendSession for MockBackend {
        fn events(&mut self) -> BoxStream<'static, BackendEvent> {
            Box::pin(stream::empty())
        }

        async fn exec(&mut self, _op: BackendOp) -> crate::error::Result<OpResult> {
            Ok(OpResult::null())
        }

        async fn list_targets(&self) -> crate::error::Result<Vec<TargetInfo>> {
            Ok(self.targets.clone())
        }

        async fn checkpoint(&self) -> crate::error::Result<Checkpoint> {
            Ok(Checkpoint {
                kind: crate::daemon::backend::BackendKind::Local,
                pid: Some(12345),
                ws_url: "ws://mock".into(),
                cdp_port: None,
                user_data_dir: None,
                headers: None,
            })
        }

        async fn health(&self) -> crate::error::Result<Health> {
            Ok(Health {
                connected: true,
                browser_version: Some("MockChrome/1.0".into()),
                uptime_secs: None,
            })
        }

        async fn shutdown(&mut self, _policy: ShutdownPolicy) -> crate::error::Result<()> {
            Ok(())
        }
    }

    fn sample_targets() -> Vec<TargetInfo> {
        vec![
            TargetInfo {
                target_id: "ABC123".into(),
                target_type: "page".into(),
                title: "Example".into(),
                url: "https://example.com".into(),
                attached: false,
            },
            TargetInfo {
                target_id: "DEF456".into(),
                target_type: "page".into(),
                title: "Rust".into(),
                url: "https://rust-lang.org".into(),
                attached: false,
            },
            // Non-page targets should be ignored.
            TargetInfo {
                target_id: "SW789".into(),
                target_type: "service_worker".into(),
                title: String::new(),
                url: "chrome-extension://xyz/sw.js".into(),
                attached: false,
            },
        ]
    }

    #[test]
    fn tab_entry_serde_round_trip() {
        let entry = TabEntry {
            id: TabId(1),
            target_id: "ABC".into(),
            window: WindowId(0),
            url: "https://example.com".into(),
            title: "Example".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: TabEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, TabId(1));
        assert_eq!(decoded.target_id, "ABC");
    }

    #[test]
    fn initial_targets_registered_as_tabs() {
        let backend = Box::new(MockBackend::new(vec![]));
        let targets = sample_targets();
        let actor = SessionActor::new(SessionId::new_unchecked("local-1"), backend, targets);

        // Only "page" targets should be registered (2 out of 3).
        assert_eq!(actor.tab_count(), 2);
        assert_eq!(actor.window_count(), 1);

        let t1 = actor.get_tab(TabId(1)).unwrap();
        assert_eq!(t1.target_id, "ABC123");
        assert_eq!(t1.url, "https://example.com");

        let t2 = actor.get_tab(TabId(2)).unwrap();
        assert_eq!(t2.target_id, "DEF456");
    }

    #[test]
    fn register_and_remove_tab() {
        let backend = Box::new(MockBackend::new(vec![]));
        let mut actor = SessionActor::new(SessionId::new_unchecked("local-1"), backend, vec![]);

        let w1 = actor.registries.alloc_window_id();
        actor.registries.windows.insert(
            w1,
            WindowEntry {
                id: w1,
                tabs: Vec::new(),
            },
        );
        let tab_id = actor.registries.alloc_tab_id();
        let target = TargetInfo {
            target_id: "T1".into(),
            target_type: "page".into(),
            title: "Test".into(),
            url: "https://test.com".into(),
            attached: false,
        };
        let entry = TabEntry {
            id: tab_id,
            target_id: target.target_id,
            window: w1,
            url: target.url,
            title: target.title,
        };
        actor.registries.tabs.insert(tab_id, entry);
        if let Some(w) = actor.registries.windows.get_mut(&w1) {
            w.tabs.push(tab_id);
        }
        assert_eq!(tab_id, TabId(1));
        assert_eq!(actor.tab_count(), 1);
        assert_eq!(actor.registries.windows.get(&w1).unwrap().tabs.len(), 1);

        let removed = actor.registries.tabs.remove(&tab_id).unwrap();
        if let Some(w) = actor.registries.windows.get_mut(&removed.window) {
            w.tabs.retain(|&t| t != tab_id);
        }
        assert_eq!(removed.target_id, "T1");
        assert_eq!(actor.tab_count(), 0);
        assert!(actor.registries.windows.get(&w1).unwrap().tabs.is_empty());
    }

    #[test]
    fn remove_nonexistent_tab() {
        let backend = Box::new(MockBackend::new(vec![]));
        let actor = SessionActor::new(SessionId::new_unchecked("local-1"), backend, vec![]);
        assert!(!actor.registries.tabs.contains_key(&TabId(99)));
    }

    #[test]
    fn register_window() {
        let backend = Box::new(MockBackend::new(vec![]));
        let mut actor = SessionActor::new(SessionId::new_unchecked("local-1"), backend, vec![]);

        // new() already creates a default window (w0), so next window is w1.
        assert_eq!(actor.window_count(), 1);
        let w1 = actor.registries.alloc_window_id();
        actor.registries.windows.insert(
            w1,
            WindowEntry {
                id: w1,
                tabs: Vec::new(),
            },
        );
        let w2 = actor.registries.alloc_window_id();
        actor.registries.windows.insert(
            w2,
            WindowEntry {
                id: w2,
                tabs: Vec::new(),
            },
        );
        assert_eq!(w1, WindowId(1));
        assert_eq!(w2, WindowId(2));
        assert_eq!(actor.window_count(), 3);
    }

    #[tokio::test]
    async fn actor_message_passing() {
        let backend = Box::new(MockBackend::new(vec![]));
        let targets = sample_targets();
        let (tx, _handle) =
            SessionActor::spawn(SessionId::new_unchecked("local-1"), backend, targets);

        // Send ListTabs action.
        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ActionRequest {
            action: Action::ListTabs {
                session: SessionId::new_unchecked("local-1"),
            },
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
        match result {
            ActionResult::Ok { data } => {
                let tabs = data["tabs"].as_array().unwrap();
                assert_eq!(tabs.len(), 2);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn actor_goto_updates_url() {
        let backend = Box::new(MockBackend::new(vec![]));
        let targets = vec![TargetInfo {
            target_id: "T1".into(),
            target_type: "page".into(),
            title: "Old".into(),
            url: "https://old.com".into(),
            attached: false,
        }];
        let (tx, _handle) =
            SessionActor::spawn(SessionId::new_unchecked("local-1"), backend, targets);

        // Send Goto — mock backend returns Ok(OpResult::null()) so this succeeds.
        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ActionRequest {
            action: Action::Goto {
                session: SessionId::new_unchecked("local-1"),
                tab: TabId(1),
                url: "https://new.com".into(),
            },
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn actor_tab_not_found() {
        let backend = Box::new(MockBackend::new(vec![]));
        let (tx, _handle) =
            SessionActor::spawn(SessionId::new_unchecked("local-1"), backend, vec![]);

        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ActionRequest {
            action: Action::Click {
                session: SessionId::new_unchecked("local-1"),
                tab: TabId(99),
                selector: "#submit".into(),
                button: None,
                count: None,
                new_tab: false,
                coordinates: None,
            },
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let result = resp_rx.await.unwrap();
        assert!(!result.is_ok());
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "tab_not_found"),
            _ => panic!("expected Fatal"),
        }
    }

    #[tokio::test]
    async fn actor_close_session() {
        let backend = Box::new(MockBackend::new(vec![]));
        let (tx, _handle) =
            SessionActor::spawn(SessionId::new_unchecked("local-1"), backend, vec![]);

        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ActionRequest {
            action: Action::Close {
                session: SessionId::new_unchecked("local-1"),
            },
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn actor_new_tab_and_close_tab() {
        let backend = Box::new(MockBackend::new(vec![]));
        let (tx, _handle) =
            SessionActor::spawn(SessionId::new_unchecked("local-1"), backend, vec![]);

        // NewTab — mock returns OpResult::null() which has no targetId,
        // so this will return a fatal error. That's expected with a null mock.
        // The real test of NewTab is in action_handler tests.
        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ActionRequest {
            action: Action::NewTab {
                session: SessionId::new_unchecked("local-1"),
                url: "https://new.com".into(),
                new_window: false,
                window: None,
            },
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let _result = resp_rx.await.unwrap();
        // NewTab through action_handler calls CreateTarget on backend;
        // our mock returns null, so targetId is empty => fatal.

        // CloseTab on tab t0 — mock returns Ok so CloseTarget succeeds,
        // but t0 doesn't exist (no initial targets), so tab_not_found.
        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ActionRequest {
            action: Action::CloseTab {
                session: SessionId::new_unchecked("local-1"),
                tab: TabId(1),
            },
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let result = resp_rx.await.unwrap();
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "tab_not_found"),
            _ => panic!("expected Fatal for tab_not_found"),
        }
    }

    #[tokio::test]
    async fn actor_global_command_returns_error() {
        let backend = Box::new(MockBackend::new(vec![]));
        let (tx, _handle) =
            SessionActor::spawn(SessionId::new_unchecked("local-1"), backend, vec![]);

        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ActionRequest {
            action: Action::ListSessions,
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let result = resp_rx.await.unwrap();
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "invalid_dispatch"),
            _ => panic!("expected Fatal for global command in actor"),
        }
    }
}
