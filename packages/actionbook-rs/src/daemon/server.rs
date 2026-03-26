//! UDS server — accepts client connections and dispatches requests via the router.
//!
//! [`DaemonServer`] binds a Unix Domain Socket, reads length-prefixed frames,
//! decodes [`Request`]s, routes them through the [`Router`], and sends back
//! length-prefixed [`Response`] frames. Each connection is handled in its own
//! tokio task.

use std::fs;
use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::router::Router;
use super::wire::{self, Request, Response};

/// Default idle timeout: daemon self-exits after this duration with no connections.
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600); // 10 minutes

/// The UDS daemon server.
pub struct DaemonServer {
    socket_path: PathBuf,
    pid_path: PathBuf,
    router: Arc<Router>,
    idle_timeout: Duration,
}

impl DaemonServer {
    /// Create a new server.
    ///
    /// The socket and PID files are placed at the given paths.
    pub fn new(socket_path: PathBuf, pid_path: PathBuf, router: Arc<Router>) -> Self {
        let idle_timeout = std::env::var("ACTIONBOOK_IDLE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_IDLE_TIMEOUT);

        DaemonServer {
            socket_path,
            pid_path,
            router,
            idle_timeout,
        }
    }

    /// Start the server: bind UDS, write PID file, and run the accept loop.
    ///
    /// Returns when a shutdown signal is received or the idle timeout fires.
    pub async fn run(&self, shutdown: Arc<AtomicBool>) -> std::io::Result<()> {
        // Remove stale socket if it exists.
        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path)?;
        }

        // Ensure parent directory exists.
        if let Some(parent) = self.socket_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        // Restrict socket to owner only (0o600) to prevent other users from
        // sending commands to the daemon.
        #[cfg(unix)]
        fs::set_permissions(&self.socket_path, fs::Permissions::from_mode(0o600))?;
        info!("daemon listening on {}", self.socket_path.display());

        // Write PID file.
        self.write_pid_file()?;

        let active_connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let last_activity = Arc::new(Mutex::new(tokio::time::Instant::now()));

        loop {
            let idle_check = {
                let last = last_activity.lock().await;
                let elapsed = last.elapsed();
                if elapsed >= self.idle_timeout && active_connections.load(Ordering::Relaxed) == 0 {
                    info!(
                        "idle timeout reached ({:.0}s), shutting down",
                        self.idle_timeout.as_secs_f64()
                    );
                    break;
                }
                self.idle_timeout.saturating_sub(elapsed)
            };

            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            debug!("accepted connection");
                            *last_activity.lock().await = tokio::time::Instant::now();
                            active_connections.fetch_add(1, Ordering::Relaxed);

                            let router = Arc::clone(&self.router);
                            let conns = Arc::clone(&active_connections);
                            let activity = Arc::clone(&last_activity);

                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, &router).await {
                                    warn!("connection error: {e}");
                                }
                                conns.fetch_sub(1, Ordering::Relaxed);
                                *activity.lock().await = tokio::time::Instant::now();
                            });
                        }
                        Err(e) => {
                            error!("accept error: {e}");
                        }
                    }
                }

                _ = tokio::time::sleep(idle_check) => {
                    // Will re-check idle condition at top of loop.
                }

                _ = tokio::signal::ctrl_c() => {
                    info!("received SIGINT, shutting down");
                    break;
                }
            }

            if shutdown.load(Ordering::Relaxed) {
                info!("shutdown requested");
                break;
            }
        }

        self.cleanup();
        Ok(())
    }

    /// Write the current process PID to the PID file.
    fn write_pid_file(&self) -> std::io::Result<()> {
        if let Some(parent) = self.pid_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::File::create(&self.pid_path)?;
        write!(f, "{}", std::process::id())?;
        Ok(())
    }

    /// Remove socket and PID files.
    fn cleanup(&self) {
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.pid_path);
        info!("cleaned up socket and PID files");
    }
}

// ---------------------------------------------------------------------------
// Connection handler
// ---------------------------------------------------------------------------

/// Per-connection idle timeout: drop connections idle for more than 5 minutes.
/// Applied to each individual length-prefix read, so active connections that
/// keep sending requests within the window will never be killed.
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Handle a client connection: read requests in a loop, route, write responses.
///
/// The loop continues until the client closes the connection (EOF on read),
/// the idle timeout fires, or an I/O error occurs.
async fn handle_connection(mut stream: UnixStream, router: &Router) -> std::io::Result<()> {
    let mut len_buf = [0u8; 4];

    loop {
        // Timeout each individual read — idle connections that stop sending
        // requests will be closed, but active connections that keep sending
        // within the window will never be killed.
        match tokio::time::timeout(CONNECTION_IDLE_TIMEOUT, stream.read_exact(&mut len_buf)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                debug!("connection idle timeout ({CONNECTION_IDLE_TIMEOUT:?}), closing");
                return Ok(());
            }
        }
        let payload_len = u32::from_le_bytes(len_buf);

        wire::validate_frame_length(payload_len)
            .map_err(|msg| std::io::Error::new(std::io::ErrorKind::InvalidData, msg))?;

        // Read payload.
        let mut payload = vec![0u8; payload_len as usize];
        stream.read_exact(&mut payload).await?;

        let request: Request = wire::decode_payload(&payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        debug!("request id={} action={:?}", request.id, request.action);

        // Route the action.
        let result = router.route(request.action).await;

        // Build and send response.
        let response = Response::new(request.id, result);
        let frame = wire::encode_frame(&response).map_err(std::io::Error::other)?;

        stream.write_all(&frame).await?;
        stream.flush().await?;
    }
}

// ---------------------------------------------------------------------------
// Default paths
// ---------------------------------------------------------------------------

/// Default daemon socket path: `~/.actionbook/daemons/v2.sock`.
pub fn default_socket_path() -> PathBuf {
    daemon_dir().join("v2.sock")
}

/// Default daemon PID path: `~/.actionbook/daemons/v2.pid`.
pub fn default_pid_path() -> PathBuf {
    daemon_dir().join("v2.pid")
}

fn daemon_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".actionbook")
        .join("daemons")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::action::Action;
    use crate::daemon::action_result::ActionResult;
    use crate::daemon::backend::TargetInfo;
    use crate::daemon::registry::{SessionHandle, SessionRegistry, SessionState};
    use crate::daemon::session_actor::SessionActor;
    use crate::daemon::types::{Mode, SessionId};
    use crate::daemon::wire::{self, Request};

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
        async fn exec(&mut self, _: BackendOp) -> crate::error::Result<OpResult> {
            Ok(OpResult::null())
        }
        async fn list_targets(&self) -> crate::error::Result<Vec<TargetInfo>> {
            Ok(vec![])
        }
        async fn checkpoint(&self) -> crate::error::Result<Checkpoint> {
            Ok(Checkpoint {
                kind: crate::daemon::backend::BackendKind::Local,
                pid: Some(1),
                ws_url: "ws://m".into(),
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

    async fn setup_server_and_connect() -> (
        tokio::task::JoinHandle<std::io::Result<()>>,
        UnixStream,
        Arc<AtomicBool>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let pid_path = dir.path().join("test.pid");

        let registry = Arc::new(Mutex::new(SessionRegistry::new()));
        {
            let mut reg = registry.lock().await;
            let backend = Box::new(MockBackend);
            let targets = vec![TargetInfo {
                target_id: "T1".into(),
                target_type: "page".into(),
                title: "Test".into(),
                url: "https://test.com".into(),
                attached: false,
            }];
            let (tx, _join) = SessionActor::spawn(SessionId(0), backend, targets);
            reg.register_session(SessionHandle {
                tx,
                profile: "test".into(),
                mode: Mode::Local,
                state: SessionState::Ready,
                tab_count: 1,
                created_at: Instant::now(),
            });
        }
        let router = Arc::new(Router::new(registry));
        let shutdown = Arc::new(AtomicBool::new(false));

        let server = DaemonServer::new(sock_path.clone(), pid_path, router);
        let shutdown_clone = Arc::clone(&shutdown);
        let handle = tokio::spawn(async move { server.run(shutdown_clone).await });

        // Wait briefly for the server to bind.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let stream = UnixStream::connect(&sock_path).await.unwrap();
        (handle, stream, shutdown, dir)
    }

    #[tokio::test]
    async fn server_handles_list_sessions() {
        let (_handle, mut stream, shutdown, _dir) = setup_server_and_connect().await;

        let req = Request::new(1, Action::ListSessions);
        let frame = wire::encode_frame(&req).unwrap();
        stream.write_all(&frame).await.unwrap();
        stream.flush().await.unwrap();

        // Read response.
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();

        let resp: Response = serde_json::from_slice(&payload).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_ok());

        shutdown.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn server_routes_to_session_actor() {
        let (_handle, mut stream, shutdown, _dir) = setup_server_and_connect().await;

        let req = Request::new(
            42,
            Action::ListTabs {
                session: SessionId(0),
            },
        );
        let frame = wire::encode_frame(&req).unwrap();
        stream.write_all(&frame).await.unwrap();
        stream.flush().await.unwrap();

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();

        let resp: Response = serde_json::from_slice(&payload).unwrap();
        assert_eq!(resp.id, 42);
        assert!(resp.result.is_ok());
        // The real session actor returns a list of tabs.
        match resp.result {
            ActionResult::Ok { data } => {
                let tabs = data["tabs"].as_array().unwrap();
                assert_eq!(tabs.len(), 1);
            }
            _ => panic!("expected Ok"),
        }

        shutdown.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn server_returns_fatal_for_unknown_session() {
        let (_handle, mut stream, shutdown, _dir) = setup_server_and_connect().await;

        let req = Request::new(
            7,
            Action::ListTabs {
                session: SessionId(99),
            },
        );
        let frame = wire::encode_frame(&req).unwrap();
        stream.write_all(&frame).await.unwrap();
        stream.flush().await.unwrap();

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();

        let resp: Response = serde_json::from_slice(&payload).unwrap();
        assert_eq!(resp.id, 7);
        assert!(!resp.result.is_ok());
        match resp.result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "session_not_found"),
            _ => panic!("expected Fatal"),
        }

        shutdown.store(true, Ordering::Relaxed);
    }

    #[test]
    fn default_paths_are_under_actionbook() {
        let sock = default_socket_path();
        assert!(sock.to_string_lossy().contains(".actionbook"));
        assert!(sock.ends_with("v2.sock"));

        let pid = default_pid_path();
        assert!(pid.ends_with("v2.pid"));
    }
}
