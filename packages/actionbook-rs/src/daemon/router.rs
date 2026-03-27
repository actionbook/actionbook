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
use super::types::{Mode, SessionId, TabId};

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
    #[allow(dead_code)]
    pub fn new(registry: Arc<Mutex<SessionRegistry>>) -> Self {
        Router {
            registry,
            factories: HashMap::new(),
            state_path: None,
        }
    }

    /// Create a new router with a single backend factory (backwards compat).
    #[allow(dead_code)]
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
                set_session_id,
            } => {
                self.handle_start_session(
                    *mode,
                    profile.clone(),
                    *headless,
                    open_url.clone(),
                    cdp_endpoint.clone(),
                    ws_headers.clone(),
                    set_session_id.clone(),
                )
                .await
            }

            // --- Close commands: forward to actor, then remove from registry ---
            Action::Close { session } | Action::CloseSession { session } => {
                let session_id = session.clone();
                let result = self.forward_to_session(&session_id, action).await;
                if result.is_ok() {
                    let mut registry = self.registry.lock().await;
                    registry.remove(&session_id);
                    self.trigger_save(&registry);
                }
                result
            }

            // --- Restart: close old session, start new one with same ID/profile/mode ---
            Action::RestartSession { session } => {
                self.handle_restart_session(session.clone()).await
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
                self.forward_to_session(&session_id, action).await
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
    #[allow(clippy::too_many_arguments)]
    async fn handle_start_session(
        &self,
        mode: Mode,
        profile: Option<String>,
        headless: bool,
        open_url: Option<String>,
        cdp_endpoint: Option<String>,
        ws_headers: Option<HashMap<String, String>>,
        set_session_id: Option<String>,
    ) -> ActionResult {
        self.handle_start_session_inner(
            mode,
            profile,
            headless,
            open_url,
            cdp_endpoint,
            ws_headers,
            None,
            set_session_id,
        )
        .await
    }

    /// Inner implementation that optionally reuses a specific session ID (for restart)
    /// or accepts an explicit `set_session_id` from the caller.
    #[allow(clippy::too_many_arguments)]
    async fn handle_start_session_inner(
        &self,
        mode: Mode,
        profile: Option<String>,
        headless: bool,
        open_url: Option<String>,
        cdp_endpoint: Option<String>,
        ws_headers: Option<HashMap<String, String>>,
        reuse_id: Option<SessionId>,
        set_session_id: Option<String>,
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
        let explicit_profile = profile.is_some();
        let profile_name = profile.unwrap_or_else(|| "default".into());
        let session_id = {
            let mut registry = self.registry.lock().await;
            let existing = registry.list_sessions();

            // For Local mode without explicit --profile, enforce 1-profile-1-session.
            // With explicit --profile, allow multiple sessions via collision suffix
            // (e.g. work, work-2, work-3).
            if mode == Mode::Local
                && !explicit_profile
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

            // Determine session ID: reuse_id (restart) > set_session_id (caller) > profile-derived > auto-generate.
            let id = if let Some(reuse) = reuse_id {
                reuse
            } else if let Some(ref explicit_id) = set_session_id {
                // Validate the caller-provided session ID.
                let validated = match SessionId::new(explicit_id) {
                    Ok(sid) => sid,
                    Err(_) => {
                        return ActionResult::fatal(
                            "invalid_session_id",
                            format!(
                                "invalid session id '{explicit_id}': must match ^[a-z][a-z0-9-]{{1,63}}$"
                            ),
                            "use lowercase letters, digits, and hyphens (e.g. 'research-google')",
                        );
                    }
                };
                // Check for conflicts with existing sessions.
                if registry.contains(&validated) {
                    return ActionResult::fatal(
                        "session_id_conflict",
                        format!("session id '{explicit_id}' is already in use"),
                        "choose a different --set-session-id or omit it to auto-generate",
                    );
                }
                validated
            } else if explicit_profile {
                // Profile-based ID: try "profile", then "profile-2", "profile-3", ...
                let mut suffix = 0u32;
                loop {
                    let candidate = SessionId::from_profile(&profile_name, suffix);
                    if !registry.contains(&candidate) {
                        break candidate;
                    }
                    suffix += 1;
                }
            } else {
                // No explicit profile — use local-N auto-generation.
                // Loop to skip IDs already taken (e.g. by --set-session-id local-1).
                loop {
                    let candidate = registry.next_session_id();
                    if !registry.contains(&candidate) {
                        break candidate;
                    }
                }
            };

            // Reserve a slot with state=Starting so concurrent requests see it.
            let (placeholder_tx, _placeholder_rx) = tokio::sync::mpsc::channel(1);
            let placeholder = SessionHandle {
                tx: placeholder_tx,
                profile: profile_name.clone(),
                mode,
                headless,
                state: SessionState::Starting,
                tab_count: 0,
                created_at: std::time::Instant::now(),
            };
            registry.register(id.clone(), placeholder);
            id
        }; // lock released — backend start can proceed without holding it.

        // Save open_url before it's moved into StartSpec, so we can update
        // the tab registry after session creation.
        let open_url_for_registry = open_url.clone();

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
                        self.registry.lock().await.remove(&session_id);
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
                        self.registry.lock().await.remove(&session_id);
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
                        self.registry.lock().await.remove(&session_id);
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
                        self.registry.lock().await.remove(&session_id);
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

        let (tx, _join_handle) = SessionActor::spawn(session_id.clone(), backend, targets);

        // Upgrade the placeholder to a real session handle.
        let mut registry = self.registry.lock().await;
        let handle = SessionHandle {
            tx,
            profile: profile_name,
            mode,
            headless,
            state: SessionState::Ready,
            tab_count: tab_ids.len(),
            created_at: std::time::Instant::now(),
        };
        registry.register(session_id.clone(), handle);

        info!(
            "started session {session_id} ({mode}) with {} tab(s)",
            tab_ids.len()
        );

        // Save state after session creation.
        self.trigger_save(&registry);
        drop(registry); // release lock before sending Goto

        // If open_url was specified, send a Goto to update the tab registry URL.
        // The backend already navigated during start, but the tab registry
        // recorded the pre-navigation URL from target discovery.
        if let Some(ref url) = open_url_for_registry {
            let goto = Action::Goto {
                session: session_id.clone(),
                tab: TabId(0),
                url: url.clone(),
            };
            let _ = self.forward_to_session(&session_id, goto).await;
        }

        ActionResult::ok(serde_json::json!({
            "session_id": session_id.to_string(),
            "tab_ids": tab_ids,
        }))
    }

    /// Handle `RestartSession` — close the current session and start a new
    /// one with the same profile, mode, and session ID.
    async fn handle_restart_session(&self, session_id: SessionId) -> ActionResult {
        // 1. Retrieve session metadata before closing.
        let (profile, mode, headless) = {
            let registry = self.registry.lock().await;
            match registry.get(&session_id) {
                Some(h) => (h.profile.clone(), h.mode, h.headless),
                None => {
                    return ActionResult::fatal(
                        "session_not_found",
                        format!("session {session_id} does not exist"),
                        "run `actionbook browser list-sessions` to see available sessions",
                    );
                }
            }
        };

        // 2. Close the existing session via the actor.
        let close_result = self
            .forward_to_session(
                &session_id,
                Action::CloseSession {
                    session: session_id.clone(),
                },
            )
            .await;
        if !close_result.is_ok() {
            return close_result;
        }

        // 3. Remove from registry (frees the profile for re-use).
        {
            let mut registry = self.registry.lock().await;
            registry.remove(&session_id);
        }

        // 4. Start a new session with the same profile/mode, reusing the original ID.
        self.handle_start_session_inner(
            mode,
            Some(profile),
            headless,
            None,
            None,
            None,
            Some(session_id),
            None,
        )
        .await
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
                    id: s.id.clone(),
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
    async fn forward_to_session(&self, session_id: &SessionId, action: Action) -> ActionResult {
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
            headless: false,
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
            let h1 = spawn_mock_session(SessionId::new_unchecked("local-1"));
            let h2 = spawn_mock_session(SessionId::new_unchecked("local-2"));
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
                assert_eq!(sessions[0]["id"], "local-1");
                assert_eq!(sessions[1]["id"], "local-2");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn route_to_nonexistent_session_returns_fatal() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let router = Router::new(registry);
        let action = Action::Goto {
            session: SessionId::new_unchecked("nonexistent"),
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
            let handle = spawn_mock_session(SessionId::new_unchecked("local-1"));
            reg.register_session(handle);
        }
        let router = Router::new(registry);

        let action = Action::ListTabs {
            session: SessionId::new_unchecked("local-1"),
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
                headless: false,
                state: SessionState::Ready,
                tab_count: 0,
                created_at: Instant::now(),
            };
            reg.register_session(handle);
        }
        let router = Router::new(registry);

        let action = Action::ListTabs {
            session: SessionId::new_unchecked("local-1"),
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

        async fn attach(&self, _spec: AttachSpec) -> crate::error::Result<Box<dyn BackendSession>> {
            Ok(Box::new(MockBackend))
        }

        async fn resume(&self, _cp: Checkpoint) -> crate::error::Result<Box<dyn BackendSession>> {
            Ok(Box::new(MockBackend))
        }
    }

    struct FailingFactory {
        mode: Mode,
        start_error: Option<&'static str>,
        attach_error: Option<&'static str>,
    }

    impl FailingFactory {
        fn local_start(error: &'static str) -> Self {
            Self {
                mode: Mode::Local,
                start_error: Some(error),
                attach_error: None,
            }
        }

        fn attach(mode: Mode, error: &'static str) -> Self {
            Self {
                mode,
                start_error: None,
                attach_error: Some(error),
            }
        }
    }

    #[async_trait]
    impl BrowserBackendFactory for FailingFactory {
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
            match self.start_error {
                Some(message) => Err(crate::error::ActionbookError::Other(message.into())),
                None => Ok(Box::new(MockBackend)),
            }
        }

        async fn attach(&self, _spec: AttachSpec) -> crate::error::Result<Box<dyn BackendSession>> {
            match self.attach_error {
                Some(message) => Err(crate::error::ActionbookError::Other(message.into())),
                None => Ok(Box::new(MockBackend)),
            }
        }

        async fn resume(&self, _cp: Checkpoint) -> crate::error::Result<Box<dyn BackendSession>> {
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
                set_session_id: None,
            })
            .await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["session_id"], "local-1");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn with_factory_registers_local_backend() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let router = Router::with_factory(registry, Arc::new(MockFactory::new(Mode::Local)));
        assert!(router.factories.contains_key(&Mode::Local));
        assert!(!router.factories.contains_key(&Mode::Extension));
        assert!(!router.factories.contains_key(&Mode::Cloud));
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
                set_session_id: None,
            })
            .await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["session_id"], "local-1");
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
                set_session_id: None,
            })
            .await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(data["session_id"], "local-1");
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
                set_session_id: None,
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
                set_session_id: None,
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => {
                assert_eq!(code, "no_backend_factory");
            }
            _ => panic!("expected Fatal, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn auto_generate_skips_occupied_ids() {
        let router = make_multi_factory_router();

        // First: explicitly claim "local-1" via --set-session-id with an explicit profile
        // to avoid the 1-profile-1-session constraint on the next auto-generate call.
        let result = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: Some("custom".into()),
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: Some("local-1".into()),
            })
            .await;
        assert!(
            result.is_ok(),
            "explicit local-1 should succeed: {result:?}"
        );

        // Second: auto-generate (no explicit profile, no set_session_id).
        // Should skip "local-1" (already taken) and get "local-2".
        let result = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        assert!(result.is_ok(), "auto-generate should succeed: {result:?}");
        match result {
            ActionResult::Ok { data } => {
                assert_eq!(
                    data["session_id"], "local-2",
                    "auto-generate should skip occupied local-1 and use local-2"
                );
            }
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn close_session_removes_registry_entry() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        {
            let mut reg = registry.lock().await;
            reg.register_session(spawn_mock_session(SessionId::new_unchecked("local-1")));
        }
        let router = Router::new(registry.clone());
        let result = router
            .route(Action::CloseSession {
                session: SessionId::new_unchecked("local-1"),
            })
            .await;
        assert!(result.is_ok(), "close should succeed: {result:?}");
        let reg = registry.lock().await;
        assert!(reg.get(&SessionId::new_unchecked("local-1")).is_none());
    }

    #[tokio::test]
    async fn restart_session_reuses_same_id() {
        let router = make_multi_factory_router();
        {
            let mut reg = router.registry.lock().await;
            reg.register_session(spawn_mock_session(SessionId::new_unchecked("local-1")));
        }
        let result = router
            .route(Action::RestartSession {
                session: SessionId::new_unchecked("local-1"),
            })
            .await;
        match result {
            ActionResult::Ok { data } => assert_eq!(data["session_id"], "local-1"),
            _ => panic!("expected Ok, got: {result:?}"),
        }
        let reg = router.registry.lock().await;
        let handle = reg
            .get(&SessionId::new_unchecked("local-1"))
            .expect("session should be recreated");
        assert_eq!(handle.profile, "default");
        assert_eq!(handle.mode, Mode::Local);
    }

    #[tokio::test]
    async fn restart_session_missing_returns_fatal() {
        let router = make_multi_factory_router();
        let result = router
            .route(Action::RestartSession {
                session: SessionId::new_unchecked("local-1"),
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "session_not_found"),
            _ => panic!("expected Fatal, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn local_default_profile_rejects_second_session() {
        let router = make_multi_factory_router();
        let first = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        assert!(first.is_ok(), "first start should succeed: {first:?}");

        let second = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        match second {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "session_exists"),
            _ => panic!("expected Fatal, got: {second:?}"),
        }
    }

    #[tokio::test]
    async fn explicit_profile_uses_collision_suffixes() {
        let router = make_multi_factory_router();

        let first = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: Some("work".into()),
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        let second = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: Some("work".into()),
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;

        match first {
            ActionResult::Ok { data } => assert_eq!(data["session_id"], "work"),
            _ => panic!("expected Ok, got: {first:?}"),
        }
        match second {
            ActionResult::Ok { data } => assert_eq!(data["session_id"], "work-2"),
            _ => panic!("expected Ok, got: {second:?}"),
        }
    }

    #[tokio::test]
    async fn extension_mode_rejects_second_session() {
        let router = make_multi_factory_router();
        let first = router
            .route(Action::StartSession {
                mode: Mode::Extension,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        assert!(
            first.is_ok(),
            "first extension start should succeed: {first:?}"
        );

        let second = router
            .route(Action::StartSession {
                mode: Mode::Extension,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        match second {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "extension_session_exists"),
            _ => panic!("expected Fatal, got: {second:?}"),
        }
    }

    #[tokio::test]
    async fn explicit_session_id_must_be_valid() {
        let router = make_multi_factory_router();
        let result = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: Some("custom".into()),
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: Some("bad id".into()),
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "invalid_session_id"),
            _ => panic!("expected Fatal, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn explicit_session_id_rejects_conflicts() {
        let router = make_multi_factory_router();
        let first = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: Some("custom".into()),
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: Some("local-7".into()),
            })
            .await;
        assert!(first.is_ok(), "first explicit id should succeed: {first:?}");

        let second = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: Some("other".into()),
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: Some("local-7".into()),
            })
            .await;
        match second {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "session_id_conflict"),
            _ => panic!("expected Fatal, got: {second:?}"),
        }
    }

    #[tokio::test]
    async fn local_start_failure_removes_placeholder() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
        factories.insert(Mode::Local, Arc::new(FailingFactory::local_start("boom")));
        let router = Router::with_factories(registry.clone(), factories);

        let result = router
            .route(Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "backend_start_failed"),
            _ => panic!("expected Fatal, got: {result:?}"),
        }
        let reg = registry.lock().await;
        assert!(reg.list_sessions().is_empty());
    }

    #[tokio::test]
    async fn extension_attach_failure_removes_placeholder() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
        factories.insert(
            Mode::Extension,
            Arc::new(FailingFactory::attach(Mode::Extension, "bridge down")),
        );
        let router = Router::with_factories(registry.clone(), factories);

        let result = router
            .route(Action::StartSession {
                mode: Mode::Extension,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "backend_attach_failed"),
            _ => panic!("expected Fatal, got: {result:?}"),
        }
        let reg = registry.lock().await;
        assert!(reg.list_sessions().is_empty());
    }

    #[tokio::test]
    async fn cloud_attach_failure_removes_placeholder() {
        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
        factories.insert(
            Mode::Cloud,
            Arc::new(FailingFactory::attach(Mode::Cloud, "cloud down")),
        );
        let router = Router::with_factories(registry.clone(), factories);

        let result = router
            .route(Action::StartSession {
                mode: Mode::Cloud,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: Some("wss://cloud.example.com/browser".into()),
                ws_headers: None,
                set_session_id: None,
            })
            .await;
        match result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "backend_attach_failed"),
            _ => panic!("expected Fatal, got: {result:?}"),
        }
        let reg = registry.lock().await;
        assert!(reg.list_sessions().is_empty());
    }
}
