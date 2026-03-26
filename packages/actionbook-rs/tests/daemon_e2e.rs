//! E2E integration tests for the daemon v2 architecture.
//!
//! These tests validate the full round-trip:
//! CLI Client -> UDS -> DaemonServer -> Router -> SessionActor -> BackendSession -> Chrome
//!
//! Tests that require Chrome are marked `#[ignore]` — run them with:
//! ```sh
//! cargo test --test daemon_e2e -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use actionbook::daemon::action::Action;
use actionbook::daemon::action_result::ActionResult;
use actionbook::daemon::client::DaemonClient;
use actionbook::daemon::registry::{SessionHandle, SessionRegistry, SessionState};
use actionbook::daemon::router::Router;
use actionbook::daemon::server::DaemonServer;
use actionbook::daemon::session_actor::SessionActor;
use actionbook::daemon::types::{Mode, SessionId, TabId};

// ===========================================================================
// Test helpers
// ===========================================================================

/// Start a test daemon in-process (no fork) with an empty registry.
///
/// Uses a random socket path in a temp directory. Returns the socket path,
/// the server's JoinHandle, and the shutdown flag.
async fn start_test_daemon() -> TestDaemon {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir
        .path()
        .join(format!("test-daemon-{}.sock", std::process::id()));
    let pid_path = dir.path().join("test.pid");

    let registry = Arc::new(Mutex::new(SessionRegistry::new()));
    let router = Arc::new(Router::new(Arc::clone(&registry)));
    let shutdown = Arc::new(AtomicBool::new(false));

    let server = DaemonServer::new(socket_path.clone(), pid_path, Arc::clone(&router));
    let shutdown_clone = Arc::clone(&shutdown);
    let handle = tokio::spawn(async move {
        let _ = server.run(shutdown_clone).await;
    });

    // Wait for the server to bind.
    tokio::time::sleep(Duration::from_millis(50)).await;

    TestDaemon {
        socket_path,
        shutdown,
        _handle: handle,
        registry,
        _dir: dir,
    }
}

/// Start a test daemon pre-populated with mock sessions.
///
/// Each profile name becomes a session with one mock tab. Returns the daemon
/// handle and the assigned SessionIds.
async fn start_daemon_with_mock_sessions(profiles: &[&str]) -> (TestDaemon, Vec<SessionId>) {
    let daemon = start_test_daemon().await;
    let mut ids = Vec::new();

    {
        let mut reg = daemon.registry.lock().await;
        for profile in profiles {
            // Profile-based ID: try "profile", then "profile-2", "profile-3", ...
            let id = {
                let mut suffix = 0u32;
                loop {
                    let candidate = SessionId::from_profile(profile, suffix);
                    if !reg.contains(&candidate) {
                        break candidate;
                    }
                    suffix += 1;
                }
            };
            let handle = spawn_mock_session(id.clone(), profile);
            reg.register(id.clone(), handle);
            ids.push(id);
        }
    }

    (daemon, ids)
}

/// Spawn a mock session actor with one tab and return its SessionHandle.
fn spawn_mock_session(id: SessionId, profile: &str) -> SessionHandle {
    use actionbook::daemon::backend::TargetInfo;

    let targets = vec![TargetInfo {
        target_id: format!("mock-target-{}", id),
        target_type: "page".into(),
        title: "Mock Page".into(),
        url: "https://mock.example.com".into(),
        attached: false,
    }];

    let backend = Box::new(MockBackend::new());
    let (tx, _join) = SessionActor::spawn(id, backend, targets);

    SessionHandle {
        tx,
        profile: profile.into(),
        mode: Mode::Local,
        headless: false,
        state: SessionState::Ready,
        tab_count: 1,
        created_at: std::time::Instant::now(),
    }
}

/// Connect a DaemonClient to a test daemon.
async fn test_client(socket_path: &Path) -> DaemonClient {
    DaemonClient::connect(socket_path)
        .await
        .expect("connect to test daemon")
        .with_timeout(Duration::from_secs(5))
}

/// Send an action over a fresh connection and return the ActionResult.
///
/// The v2 server handles one request per connection, so each call creates
/// a new UDS connection. This matches the CLI's behavior.
async fn send_action(daemon: &TestDaemon, action: Action) -> ActionResult {
    let mut client = test_client(&daemon.socket_path).await;
    client.send_action(action).await.expect("send action")
}

/// Send an action and assert the result is Ok, returning the data payload.
async fn send_ok(daemon: &TestDaemon, action: Action) -> serde_json::Value {
    let result = send_action(daemon, action).await;
    match result {
        ActionResult::Ok { data } => data,
        other => panic!("expected Ok, got {other:?}"),
    }
}

/// Send an action and assert the result is Fatal with the expected error code.
async fn send_fatal(daemon: &TestDaemon, action: Action, expected_code: &str) {
    let result = send_action(daemon, action).await;
    match result {
        ActionResult::Fatal { code, .. } => {
            assert_eq!(
                code, expected_code,
                "expected fatal code '{expected_code}', got '{code}'"
            );
        }
        other => panic!("expected Fatal(code={expected_code}), got {other:?}"),
    }
}

/// Handle for a running test daemon -- stops on drop.
struct TestDaemon {
    socket_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    _handle: JoinHandle<()>,
    registry: Arc<Mutex<SessionRegistry>>,
    _dir: tempfile::TempDir,
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Mock backend for tests that don't need a real browser
// ---------------------------------------------------------------------------

use actionbook::daemon::backend::{
    BackendEvent, BackendSession, Checkpoint, Health, OpResult, ShutdownPolicy, TargetInfo,
};
use actionbook::daemon::backend_op::BackendOp;
use async_trait::async_trait;
use futures::stream::{self, BoxStream};

use std::sync::atomic::AtomicU32;

/// Mock backend that returns sensible responses for common BackendOp commands.
///
/// - `Navigate` returns `{}`
/// - `GetAccessibilityTree` returns a placeholder tree
/// - `CaptureScreenshot` returns a placeholder base64 string
/// - `CreateTarget` returns a fresh `targetId`
/// - `CloseTarget` returns `{}`
/// - `Evaluate` returns `{"result": {"value": null}}`
/// - All others return `OpResult::null()`
struct MockBackend {
    target_counter: AtomicU32,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            target_counter: AtomicU32::new(100),
        }
    }
}

#[async_trait]
impl BackendSession for MockBackend {
    fn events(&mut self) -> BoxStream<'static, BackendEvent> {
        Box::pin(stream::empty())
    }

    async fn exec(&mut self, op: BackendOp) -> actionbook::error::Result<OpResult> {
        match op {
            BackendOp::Navigate { .. } => Ok(OpResult::new(serde_json::json!({}))),
            BackendOp::GetAccessibilityTree { .. } => Ok(OpResult::new(serde_json::json!({
                "nodes": [{"role": "document", "name": "Mock Page"}]
            }))),
            BackendOp::CaptureScreenshot { .. } => Ok(OpResult::new(serde_json::json!({
                "data": "base64-mock-screenshot"
            }))),
            BackendOp::CreateTarget { .. } => {
                let n = self
                    .target_counter
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(OpResult::new(serde_json::json!({
                    "targetId": format!("mock-new-target-{n}")
                })))
            }
            BackendOp::CloseTarget { .. } => Ok(OpResult::new(serde_json::json!({}))),
            BackendOp::Evaluate { .. } => Ok(OpResult::new(serde_json::json!({
                "result": {"type": "object", "value": null}
            }))),
            _ => Ok(OpResult::null()),
        }
    }

    async fn list_targets(&self) -> actionbook::error::Result<Vec<TargetInfo>> {
        Ok(vec![])
    }

    async fn checkpoint(&self) -> actionbook::error::Result<Checkpoint> {
        Ok(Checkpoint {
            kind: actionbook::daemon::backend::BackendKind::Local,
            pid: Some(1),
            ws_url: "ws://mock".into(),
            cdp_port: None,
            user_data_dir: None,
            headers: None,
        })
    }

    async fn health(&self) -> actionbook::error::Result<Health> {
        Ok(Health {
            connected: true,
            browser_version: Some("MockChrome/1.0".into()),
            uptime_secs: None,
        })
    }

    async fn shutdown(&mut self, _: ShutdownPolicy) -> actionbook::error::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock backend factory — wraps MockBackend for router-driven StartSession tests
// ---------------------------------------------------------------------------

use actionbook::daemon::backend::{
    AttachSpec, BackendKind, BrowserBackendFactory, Capabilities, StartSpec,
};
use std::collections::HashMap;

/// A configurable mock backend factory for testing StartSession flows.
///
/// Implements `BrowserBackendFactory` and returns `MockBackend` sessions from
/// both `start()` and `attach()`. The `kind` determines the `BackendKind`
/// reported by the factory.
struct MockBackendFactory {
    backend_kind: BackendKind,
}

impl MockBackendFactory {
    fn new(kind: BackendKind) -> Self {
        Self { backend_kind: kind }
    }
}

#[async_trait]
impl BrowserBackendFactory for MockBackendFactory {
    fn kind(&self) -> BackendKind {
        self.backend_kind
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_launch: self.backend_kind == BackendKind::Local,
            can_attach: true,
            can_resume: self.backend_kind == BackendKind::Local,
            supports_headless: self.backend_kind == BackendKind::Local,
        }
    }

    async fn start(&self, _spec: StartSpec) -> actionbook::error::Result<Box<dyn BackendSession>> {
        Ok(Box::new(MockBackend::new()))
    }

    async fn attach(
        &self,
        _spec: AttachSpec,
    ) -> actionbook::error::Result<Box<dyn BackendSession>> {
        Ok(Box::new(MockBackend::new()))
    }

    async fn resume(&self, _cp: Checkpoint) -> actionbook::error::Result<Box<dyn BackendSession>> {
        Ok(Box::new(MockBackend::new()))
    }
}

/// Start a test daemon with backend factories registered for the given modes.
async fn start_test_daemon_with_factories(
    factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>>,
) -> TestDaemon {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir
        .path()
        .join(format!("test-daemon-{}.sock", std::process::id()));
    let pid_path = dir.path().join("test.pid");

    let registry = Arc::new(Mutex::new(SessionRegistry::new()));
    let router = Arc::new(Router::with_factories(Arc::clone(&registry), factories));
    let shutdown = Arc::new(AtomicBool::new(false));

    let server = DaemonServer::new(socket_path.clone(), pid_path, Arc::clone(&router));
    let shutdown_clone = Arc::clone(&shutdown);
    let handle = tokio::spawn(async move {
        let _ = server.run(shutdown_clone).await;
    });

    // Wait for the server to bind.
    tokio::time::sleep(Duration::from_millis(50)).await;

    TestDaemon {
        socket_path,
        shutdown,
        _handle: handle,
        registry,
        _dir: dir,
    }
}

// ===========================================================================
// Tests: daemon + mock backend (no Chrome required)
// ===========================================================================

#[tokio::test]
async fn e2e_list_sessions_empty() {
    let daemon = start_test_daemon().await;

    let data = send_ok(&daemon, Action::ListSessions).await;
    let sessions = data["sessions"].as_array().expect("sessions array");
    assert!(sessions.is_empty());
}

#[tokio::test]
async fn e2e_list_sessions_with_mock_sessions() {
    let (daemon, ids) = start_daemon_with_mock_sessions(&["default", "work"]).await;

    let data = send_ok(&daemon, Action::ListSessions).await;
    let sessions = data["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["id"].as_str().unwrap(), ids[0].as_str());
    assert_eq!(sessions[1]["id"].as_str().unwrap(), ids[1].as_str());
}

#[tokio::test]
async fn e2e_missing_session_returns_fatal() {
    let daemon = start_test_daemon().await;

    send_fatal(
        &daemon,
        Action::Goto {
            session: "nonexistent-session".parse::<SessionId>().unwrap(),
            tab: TabId(0),
            url: "https://example.com".into(),
        },
        "session_not_found",
    )
    .await;
}

#[tokio::test]
async fn e2e_missing_tab_returns_fatal() {
    let (daemon, ids) = start_daemon_with_mock_sessions(&["default"]).await;

    send_fatal(
        &daemon,
        Action::Click {
            session: ids[0].clone(),
            tab: TabId(99),
            selector: "#submit".into(),
            button: None,
            count: None,
        },
        "tab_not_found",
    )
    .await;
}

#[tokio::test]
async fn e2e_list_tabs_via_router() {
    let (daemon, ids) = start_daemon_with_mock_sessions(&["default"]).await;

    let data = send_ok(
        &daemon,
        Action::ListTabs {
            session: ids[0].clone(),
        },
    )
    .await;

    let tabs = data["tabs"].as_array().expect("tabs array");
    assert_eq!(tabs.len(), 1, "mock session should have 1 tab");
    assert_eq!(tabs[0]["id"], "t0");
}

#[tokio::test]
async fn e2e_goto_and_snapshot_stub() {
    let (daemon, ids) = start_daemon_with_mock_sessions(&["default"]).await;

    // Goto (stub updates URL in actor)
    let data = send_ok(
        &daemon,
        Action::Goto {
            session: ids[0].clone(),
            tab: TabId(0),
            url: "https://example.com/page2".into(),
        },
    )
    .await;
    assert_eq!(data["navigated"], "https://example.com/page2");

    // Snapshot (returns accessibility tree from mock backend)
    let data = send_ok(
        &daemon,
        Action::Snapshot {
            session: ids[0].clone(),
            tab: TabId(0),
            interactive: false,
            compact: true,
            cursor: false,
            depth: None,
            selector: None,
        },
    )
    .await;
    // The mock backend returns {"nodes": [...]}, action_handler passes it through.
    assert!(
        data["nodes"].is_array(),
        "snapshot should contain nodes array, got: {data}"
    );
}

#[tokio::test]
async fn e2e_close_session_stub() {
    let (daemon, ids) = start_daemon_with_mock_sessions(&["default"]).await;

    let data = send_ok(
        &daemon,
        Action::CloseSession {
            session: ids[0].clone(),
        },
    )
    .await;
    assert_eq!(data["closed"].as_str().unwrap(), ids[0].as_str());
}

#[tokio::test]
async fn e2e_multi_tab_new_and_close() {
    let (daemon, ids) = start_daemon_with_mock_sessions(&["default"]).await;

    // Start with 1 tab (t0), add a new tab.
    let data = send_ok(
        &daemon,
        Action::NewTab {
            session: ids[0].clone(),
            url: "https://second-tab.example.com".into(),
            new_window: false,
            window: None,
        },
    )
    .await;
    let new_tab = data["tab"].as_str().expect("tab id").to_string();
    assert!(new_tab.starts_with('t'), "tab id should start with 't'");

    // List tabs -- should have 2.
    let data = send_ok(
        &daemon,
        Action::ListTabs {
            session: ids[0].clone(),
        },
    )
    .await;
    let tabs = data["tabs"].as_array().expect("tabs array");
    assert_eq!(tabs.len(), 2, "should have 2 tabs after NewTab");

    // Close the new tab.
    // Parse the tab ID from the string (e.g. "t1" -> TabId(1))
    let new_tab_id: TabId = new_tab.parse().expect("parse tab id");
    let data = send_ok(
        &daemon,
        Action::CloseTab {
            session: ids[0].clone(),
            tab: new_tab_id,
        },
    )
    .await;
    assert_eq!(data["closed_tab"], new_tab.as_str());

    // List tabs -- should be back to 1.
    let data = send_ok(
        &daemon,
        Action::ListTabs {
            session: ids[0].clone(),
        },
    )
    .await;
    let tabs = data["tabs"].as_array().expect("tabs array");
    assert_eq!(tabs.len(), 1, "should have 1 tab after CloseTab");
}

// ===========================================================================
// Tests: Phase 2 — Extension/Cloud backend integration (mock, no real browser)
// ===========================================================================

#[tokio::test]
async fn e2e_start_extension_session_mock() {
    let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
    factories.insert(
        Mode::Extension,
        Arc::new(MockBackendFactory::new(BackendKind::Extension)),
    );

    let daemon = start_test_daemon_with_factories(factories).await;

    // Start an extension session
    let data = send_ok(
        &daemon,
        Action::StartSession {
            mode: Mode::Extension,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        },
    )
    .await;

    let session_id = data["session_id"].as_str().expect("session_id");
    assert_eq!(session_id, "local-1");

    // Verify the session appears in list-sessions with Extension mode
    let data = send_ok(&daemon, Action::ListSessions).await;
    let sessions = data["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], "local-1");
    assert_eq!(sessions[0]["mode"], "extension");
}

#[tokio::test]
async fn e2e_start_cloud_session_mock() {
    let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
    factories.insert(
        Mode::Cloud,
        Arc::new(MockBackendFactory::new(BackendKind::Cloud)),
    );

    let daemon = start_test_daemon_with_factories(factories).await;

    // Start a cloud session with a CDP endpoint
    let data = send_ok(
        &daemon,
        Action::StartSession {
            mode: Mode::Cloud,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: Some("wss://mock-cloud.example.com/browser".into()),
            ws_headers: None,
            set_session_id: None,
        },
    )
    .await;

    let session_id = data["session_id"].as_str().expect("session_id");
    assert_eq!(session_id, "local-1");

    // Verify the session appears in list-sessions with Cloud mode
    let data = send_ok(&daemon, Action::ListSessions).await;
    let sessions = data["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], "local-1");
    assert_eq!(sessions[0]["mode"], "cloud");
}

#[tokio::test]
async fn e2e_multi_mode_sessions() {
    let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
    factories.insert(
        Mode::Local,
        Arc::new(MockBackendFactory::new(BackendKind::Local)),
    );
    factories.insert(
        Mode::Cloud,
        Arc::new(MockBackendFactory::new(BackendKind::Cloud)),
    );

    let daemon = start_test_daemon_with_factories(factories).await;

    // Start a local session
    let data = send_ok(
        &daemon,
        Action::StartSession {
            mode: Mode::Local,
            profile: Some("local-profile".into()),
            headless: true,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        },
    )
    .await;
    assert_eq!(data["session_id"].as_str().unwrap(), "local-profile");

    // Start a cloud session simultaneously
    let data = send_ok(
        &daemon,
        Action::StartSession {
            mode: Mode::Cloud,
            profile: Some("cloud-profile".into()),
            headless: false,
            open_url: None,
            cdp_endpoint: Some("wss://mock.example.com/browser".into()),
            ws_headers: None,
            set_session_id: None,
        },
    )
    .await;
    assert_eq!(data["session_id"].as_str().unwrap(), "cloud-profile");

    // Verify both appear in list-sessions with correct modes
    let data = send_ok(&daemon, Action::ListSessions).await;
    let sessions = data["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2);

    assert_eq!(sessions[0]["id"], "cloud-profile");
    assert_eq!(sessions[0]["mode"], "cloud");
    assert_eq!(sessions[0]["profile"], "cloud-profile");

    assert_eq!(sessions[1]["id"], "local-profile");
    assert_eq!(sessions[1]["mode"], "local");
    assert_eq!(sessions[1]["profile"], "local-profile");
}

#[tokio::test]
async fn e2e_cloud_requires_endpoint() {
    let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
    factories.insert(
        Mode::Cloud,
        Arc::new(MockBackendFactory::new(BackendKind::Cloud)),
    );

    let daemon = start_test_daemon_with_factories(factories).await;

    // Start cloud session WITHOUT cdp_endpoint -> should fail
    send_fatal(
        &daemon,
        Action::StartSession {
            mode: Mode::Cloud,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        },
        "missing_cdp_endpoint",
    )
    .await;
}

#[tokio::test]
async fn e2e_extension_single_session_constraint() {
    let mut factories: HashMap<Mode, Arc<dyn BrowserBackendFactory>> = HashMap::new();
    factories.insert(
        Mode::Extension,
        Arc::new(MockBackendFactory::new(BackendKind::Extension)),
    );

    let daemon = start_test_daemon_with_factories(factories).await;

    // Start first extension session -> should succeed
    let data = send_ok(
        &daemon,
        Action::StartSession {
            mode: Mode::Extension,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        },
    )
    .await;
    assert_eq!(data["session_id"].as_str().unwrap(), "local-1");

    // Start second extension session -> should fail with extension_session_exists
    send_fatal(
        &daemon,
        Action::StartSession {
            mode: Mode::Extension,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        },
        "extension_session_exists",
    )
    .await;
}

// ===========================================================================
// Tests: full E2E with real Chrome (requires Chrome installed, run with --ignored)
// ===========================================================================

#[tokio::test]
#[ignore = "requires Chrome installed -- run with `cargo test --test daemon_e2e -- --ignored`"]
async fn e2e_start_goto_snapshot_close() {
    // TODO: Wire up when StartSession is implemented in the router/daemon_main.
    //
    // Test plan:
    // 1. Start daemon (in-process, empty registry)
    // 2. Send StartSession { mode: Local, headless: true, open_url: "https://example.com" }
    //    -> assert Ok, extract session_id (s0) and initial tab (t0)
    // 3. Send ListSessions -> assert contains s0
    // 4. Send Snapshot { session: s0, tab: t0, compact: true }
    //    -> assert non-empty snapshot containing "Example Domain"
    // 5. Send Goto { session: s0, tab: t0, url: "https://httpbin.org/html" }
    //    -> assert Ok
    // 6. Send Snapshot again -> assert content changed (contains "Herman Melville")
    // 7. Send CloseSession { session: s0 } -> assert Ok
    // 8. Send ListSessions -> assert empty
    // 9. Shutdown daemon (set shutdown flag)

    let daemon = start_test_daemon().await;

    // Step 2: StartSession
    let start_result = send_action(
        &daemon,
        Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: true,
            open_url: Some("https://example.com".into()),
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        },
    )
    .await;

    // No local backend factory is registered in this test, so StartSession returns Fatal.
    match start_result {
        ActionResult::Fatal { code, .. } => {
            assert_eq!(
                code, "no_backend_factory",
                "StartSession should fail when no backend factory is registered for the mode"
            );
        }
        ActionResult::Ok { data } => {
            // Future: extract session_id and tab_id from data, run steps 3-8.
            let _session_id = data["session"].as_str().expect("session id");
            let _tab_id = data["tab"].as_str().expect("tab id");
            // Steps 3-8 would go here.
        }
        other => {
            panic!("unexpected result for StartSession: {other:?}");
        }
    }
}

#[tokio::test]
#[ignore = "requires Chrome installed -- run with `cargo test --test daemon_e2e -- --ignored`"]
async fn e2e_real_click_and_type() {
    // TODO: Wire up when action handler is implemented.
    //
    // Test plan:
    // 1. Start session with headless Chrome
    // 2. Goto a page with input fields (e.g. httpbin.org/forms/post)
    // 3. Click an input field
    // 4. Type text into it
    // 5. Eval to read the input's value -> assert matches typed text
    // 6. Close session

    let _daemon = start_test_daemon().await;

    // Placeholder -- will be filled in when StartSession + action handler are ready.
}
