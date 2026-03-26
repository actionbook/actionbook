//! Request Router — dispatches Actions to global handlers or session actors.
//!
//! The router is the daemon's front door. It receives [`Action`]s from the UDS
//! server, classifies them by addressing level, and either handles them directly
//! (global commands) or forwards them to the appropriate session actor via a
//! channel + oneshot pattern.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{oneshot, Mutex};
use tracing::info;

use super::action::Action;
use super::action_result::ActionResult;
use super::backend::{AttachSpec, BrowserBackendFactory, StartSpec, TargetInfo};
use super::registry::{SessionHandle, SessionRegistry, SessionState};
use super::session_actor::{ActionRequest, SessionActor};
use super::types::{Mode, SessionId};

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// The request router — classifies Actions and dispatches them.
pub struct Router {
    pub registry: Arc<Mutex<SessionRegistry>>,
    /// Backend factories keyed by Mode.
    factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>>,
    /// Path for persisting daemon state (None = no persistence).
    pub state_path: Option<PathBuf>,
}

impl Router {
    /// Create a new router with the given registry (no factories — StartSession will fail).
    pub fn new(registry: Arc<Mutex<SessionRegistry>>) -> Self {
        Router {
            registry,
            factories: HashMap::new(),
            state_path: None,
        }
    }

    /// Create a new router with a single backend factory (backwards compat).
    pub fn with_factory(
        registry: Arc<Mutex<SessionRegistry>>,
        factory: Arc<dyn BrowserBackendFactory>,
    ) -> Self {
        let mut factories = HashMap::new();
        factories.insert(Mode::Local, factory);
        Router {
            registry,
            factories,
            state_path: None,
        }
    }

    /// Create a new router with multiple backend factories keyed by mode.
    pub fn with_factories(
        registry: Arc<Mutex<SessionRegistry>>,
        factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>>,
    ) -> Self {
        Router {
            registry,
            factories,
            state_path: None,
        }
    }

    /// Route an action to the appropriate handler and return the result.
    pub async fn route(&self, action: Action) -> ActionResult {
        match &action {
            // --- Global commands handled directly ---
            Action::ListSessions => self.handle_list_sessions().await,
            Action::StartSession {
                mode,
                profile,
                headless,
                open_url,
                cdp_endpoint,
                ws_headers,
            } => {
                self.handle_start_session(
                    *mode,
                    profile.clone(),
                    *headless,
                    open_url.clone(),
                    cdp_endpoint.clone(),
                    ws_headers.clone(),
                )
                .await
            }

            // --- Close commands: forward to actor, then remove from registry ---
            Action::Close { session } | Action::CloseSession { session } => {
                let session_id = *session;
                let result = self.forward_to_session(session_id, action).await;
                if result.is_ok() {
                    let mut registry = self.registry.lock().await;
                    registry.remove(session_id);
                    self.trigger_save(&registry);
                }
                result
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

    /// Handle `StartSession` — create a backend, spawn actor, register in registry.
    ///
    /// Uses a placeholder reservation to prevent TOCTOU races: a `Starting`
    /// entry is inserted under the registry lock before any await, so
    /// concurrent StartSession requests see the reservation and fail.
    async fn handle_start_session(
        &self,
        mode: Mode,
        profile: Option<String>,
        headless: bool,
        open_url: Option<String>,
        cdp_endpoint: Option<String>,
        ws_headers: Option<HashMap<String, String>>,
    ) -> ActionResult {
        let factory = match self.factories.get(&mode) {
            Some(f) => Arc::clone(f),
            None => {
                return ActionResult::fatal(
                    "no_backend_factory",
                    format!("no backend factory configured for mode '{mode}'"),
                    "available modes depend on daemon configuration",
                );
            }
        };

        // Cloud mode requires a CDP endpoint.
        if mode == Mode::Cloud && cdp_endpoint.is_none() {
            return ActionResult::fatal(
                "missing_cdp_endpoint",
                "cloud mode requires a CDP endpoint",
                "pass --cdp-endpoint wss://... when using --mode cloud",
            );
        }

        // Atomically check uniqueness and reserve a slot under a single lock.
        let profile_name = profile.unwrap_or_else(|| "default".into());
        let session_id = {
            let mut registry = self.registry.lock().await;
            let existing = registry.list_sessions();

            // For Local mode, enforce 1-profile-1-session constraint.
            if mode == Mode::Local
                && existing
                    .iter()
                    .any(|s| s.profile == profile_name && s.state != SessionState::Closed)
            {
                return ActionResult::fatal(
                    "session_exists",
                    format!("a session with profile '{profile_name}' already exists"),
                    "close the existing session first, or use a different profile",
                );
            }

            // Extension mode: 1 extension = 1 session (v1.0 product constraint).
            if mode == Mode::Extension
                && existing
                    .iter()
                    .any(|s| s.mode == Mode::Extension && s.state != SessionState::Closed)
            {
                return ActionResult::fatal(
                    "extension_session_exists",
                    "an extension session already exists (limit: 1)",
                    "close the existing extension session first",
                );
            }

            // Reserve a slot with state=Starting so concurrent requests see it.
            let id = registry.next_session_id();
            let (placeholder_tx, _placeholder_rx) = tokio::sync::mpsc::channel(1);
            let placeholder = SessionHandle {
                tx: placeholder_tx,
                profile: profile_name.clone(),
                mode,
                state: SessionState::Starting,
                tab_count: 0,
                created_at: std::time::Instant::now(),
            };
            registry.register(id, placeholder);
            id
        }; // lock released — backend start can proceed without holding it.

        // Create backend session based on mode.
        let backend = match mode {
            Mode::Local => {
                let spec = StartSpec {
                    profile: profile_name.clone(),
                    headless,
                    open_url,
                    extra_args: vec![],
                };
                match factory.start(spec).await {
                    Ok(b) => b,
                    Err(e) => {
                        self.registry.lock().await.remove(session_id);
                        return ActionResult::fatal(
                            "backend_start_failed",
                            format!("failed to start browser: {e}"),
                            "check that Chrome/Chromium is installed and accessible",
                        );
                    }
                }
            }
            Mode::Extension => {
                let spec = AttachSpec {
                    ws_url: String::new(), // Extension factory ignores ws_url; it binds its own port.
                    headers: None,
                };
                match factory.attach(spec).await {
                    Ok(b) => b,
                    Err(e) => {
                        self.registry.lock().await.remove(session_id);
                        return ActionResult::fatal(
                            "backend_attach_failed",
                            format!("failed to connect to extension: {e}"),
                            "ensure the Actionbook browser extension is installed and active",
                        );
                    }
                }
            }
            Mode::Cloud => {
                // cdp_endpoint is validated above; use expect for clarity.
                let ws_url = match cdp_endpoint {
                    Some(ep) => ep,
                    None => {
                        self.registry.lock().await.remove(session_id);
                        return ActionResult::fatal(
                            "missing_cdp_endpoint",
                            "cloud mode requires a CDP endpoint (internal error: should have been caught earlier)",
                            "pass --cdp-endpoint wss://... when using --mode cloud",
                        );
                    }
                };
                let spec = AttachSpec {
                    ws_url,
                    headers: ws_headers,
                };
                match factory.attach(spec).await {
                    Ok(b) => b,
                    Err(e) => {
                        self.registry.lock().await.remove(session_id);
                        return ActionResult::fatal(
                            "backend_attach_failed",
                            format!("failed to connect to cloud browser: {e}"),
                            "check that the WSS endpoint is reachable and auth is correct",
                        );
                    }
                }
            }
        };

        // Discover initial tabs.
        let targets: Vec<TargetInfo> = backend.list_targets().await.unwrap_or_default();

        let tab_ids: Vec<String> = targets
            .iter()
            .filter(|t| t.target_type == "page")
            .map(|t| t.target_id.clone())
            .collect();

        let (tx, _join_handle) = SessionActor::spawn(session_id, backend, targets);

        // Upgrade the placeholder to a real session handle.
        let mut registry = self.registry.lock().await;
        let handle = SessionHandle {
            tx,
            profile: profile_name,
            mode,
            state: SessionState::Ready,
            tab_count: tab_ids.len(),
            created_at: std::time::Instant::now(),
        };
        registry.register(session_id, handle);

        info!("started session {session_id} ({mode}) with {} tab(s)", tab_ids.len());

        // Save state after session creation.
        self.trigger_save(&registry);

        ActionResult::ok(serde_json::json!({
            "session_id": session_id.to_string(),
            "tab_ids": tab_ids,
        }))
    }

    /// Trigger a state save (best-effort, errors are logged).
    pub fn trigger_save(&self, registry: &SessionRegistry) {
        let Some(ref state_path) = self.state_path else {
            return;
        };
        let state = self.build_state_snapshot(registry);
        if let Err(e) = super::persistence::save_state(state_path, &state) {
            tracing::warn!("failed to save daemon state: {e}");
        }
    }

    /// Build a serializable snapshot of the current registry state.
    fn build_state_snapshot(
        &self,
        registry: &SessionRegistry,
    ) -> super::persistence::DaemonStateFile {
        use super::persistence::*;
        let summaries = registry.list_sessions();
        let sessions: Vec<PersistedSession> = summaries
            .iter()
            .map(|s| {
                let checkpoint = match s.mode {
                    Mode::Local => BackendCheckpoint::Local(LocalCheckpoint {
                        pid: 0,
                        ws_url: String::new(),
                        user_data_dir: String::new(),
                    }),
                    Mode::Extension => BackendCheckpoint::Extension(ExtensionCheckpoint {
                        bridge_port: 0,
                        extension_id: String::new(),
                    }),
                    Mode::Cloud => BackendCheckpoint::Cloud(CloudCheckpoint {
                        wss_endpoint: String::new(),
                        auth_headers: HashMap::new(),
                        resume_token: None,
                    }),
                };
                PersistedSession {
                    uuid: {
                        use rand::Rng;
                        let mut rng = rand::thread_rng();
                        format!("{:016x}{:016x}", rng.gen::<u64>(), rng.gen::<u64>())
                    },
                    id: s.id,
                    mode: s.mode,
                    profile: s.profile.clone(),
                    tabs: vec![], // Tab details require actor query — omitted for now.
                    checkpoint,
                }
            })
            .collect();
        DaemonStateFile {
            version: DaemonStateFile::CURRENT_VERSION,
            sessions,
        }
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
                        format!("session {session_id} does not exist"),
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
                format!("session {session_id} is no longer responding"),
                "run `actionbook browser list-sessions` to check session status",
            );
        }

        match reply_rx.await {
            Ok(result) => result,
            Err(_) => ActionResult::fatal(
                "session_dead",
                format!("session {session_id} dropped the response"),
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
    use crate::daemon::backend::TargetInfo;
    use crate::daemon::registry::{SessionHandle, SessionState};
    use crate::daemon::session_actor::SessionActor;
    use crate::daemon::types::{Mode, TabId};

    // -- Mock backend (reusable for router tests) --
    use crate::daemon::backend::{
        BackendEvent, BackendSession, Checkpoint, Health, OpResult, ShutdownPolicy,
    };
    use crate::daemon::backend_op::BackendOp;
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
                kind: crate::daemon::backend::BackendKind::Local,
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

    // -- Mock factory for testing mode-based dispatch --

    use crate::daemon::backend::{
        AttachSpec, BackendKind, BrowserBackendFactory, Capabilities, StartSpec,
    };

    /// A mock factory that records which mode it represents and always
    /// returns a MockBackend from start() and attach().
    struct MockFactory {
        mode: Mode,
    }

    impl MockFactory {
        fn new(mode: Mode) -> Self {
            Self { mode }
        }
    }

    #[async_trait]
    impl BrowserBackendFactory for MockFactory {
        fn kind(&self) -> BackendKind {
            match self.mode {
                Mode::Local => BackendKind::Local,
                Mode::Extension => BackendKind::Extension,
                Mode::Cloud => BackendKind::Cloud,
            }
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                can_launch: self.mode == Mode::Local,
                can_attach: true,
                can_resume: self.mode != Mode::Extension,
                supports_headless: self.mode != Mode::Extension,
            }
        }

        async fn start(&self, _spec: StartSpec) -> crate::error::Result<Box<dyn BackendSession>> {
            Ok(Box::new(MockBackend))
        }

        async fn attach(
            &self,
            _spec: AttachSpec,
        ) -> crate::error::Result<Box<dyn BackendSession>> {
            Ok(Box::new(MockBackend))
        }

        async fn resume(
            &self,
            _cp: Checkpoint,
        ) -> crate::error::Result<Box<dyn BackendSession>> {
            Ok(Box::new(MockBackend))
        }
    }

    fn make_multi_factory_router() -> Router {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
        factories.insert(Mode::Local, Arc::new(MockFactory::new(Mode::Local)));
        factories.insert(Mode::Extension, Arc::new(MockFactory::new(Mode::Extension)));
        factories.insert(Mode::Cloud, Arc::new(MockFactory::new(Mode::Cloud)));
        Router::with_factories(registry, factories)
    }

    #[tokio::test]
    async fn start_session_local_uses_local_factory() {
        let router = make_multi_factory_router();
        let result = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
            })
            .await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["session_id"], "s0");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn start_session_extension_uses_extension_factory() {
        let router = make_multi_factory_router();
        let result = router
            .route(Action::StartSession {
                mode: Mode::Extension,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
            })
            .await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["session_id"], "s0");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn start_session_cloud_uses_cloud_factory() {
        let router = make_multi_factory_router();
        let result = router
            .route(Action::StartSession {
                mode: Mode::Cloud,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: Some("wss://cloud.example.com/browser".into()),
                ws_headers: None,
            })
            .await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["session_id"], "s0");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn start_session_cloud_without_endpoint_returns_fatal() {
        let router = make_multi_factory_router();
        let result = router
            .route(Action::StartSession {
                mode: Mode::Cloud,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => {
                assert_eq!(code, "missing_cdp_endpoint");
            }
            _ => panic!("expected Fatal, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn start_session_missing_factory_returns_fatal() {
        // Router with no factories registered.
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let router = Router::new(registry);
        let result = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => {
                assert_eq!(code, "no_backend_factory");
            }
            _ => panic!("expected Fatal, got: {result:?}"),
        }
    }
}
