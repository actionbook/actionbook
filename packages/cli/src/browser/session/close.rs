use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;
use crate::types::Mode;

/// Close a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser close --session my-session

Closes the browser and all tabs in the session. The session ID cannot be reused.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser close";

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
    let provider_session = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(entry) => entry.provider_session.clone(),
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        }
    };

    if let Some(provider_session) = provider_session.as_ref()
        && let Err(err) =
            crate::browser::session::provider::close_provider_session(provider_session).await
    {
        return ActionResult::fatal_with_hint(
            err.error_code(),
            format!("failed to close provider session for '{}': {err}", cmd.session),
            err.hint(),
        );
    }

    // Extract everything from registry then release the lock before slow I/O.
    let (closed_tabs, cdp, chrome_process, profile_to_clean, mode) = {
        let mut reg = registry.lock().await;
        let mut entry = match reg.remove(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };
        let tabs = entry.tabs_count();
        let entry_mode = entry.mode;

        // Only delete non-default profile directories for local sessions.
        // The default profile ("actionbook") is long-lived and preserves
        // user state (cookies, localStorage) across sessions.
        let profile =
            if entry.chrome_process.is_some() && entry.profile != crate::config::DEFAULT_PROFILE {
                Some(entry.profile.clone())
            } else {
                None
            };

        reg.clear_session_ref_caches(&cmd.session);
        (
            tabs,
            entry.cdp.take(),
            entry.chrome_process.take(),
            profile,
            entry_mode,
        )
    };
    // Registry lock released here — slow I/O below won't block other sessions.

    // Extension mode: detach debugger before tearing down the CDP connection.
    // Extension mode doesn't own the browser — we only release the debugger,
    // leaving tabs open for the user.
    if mode == Mode::Extension
        && let Some(ref cdp) = cdp
        && let Err(e) = cdp
            .execute_browser("Extension.detachTab", serde_json::json!({}))
            .await
    {
        tracing::warn!("extension: failed to detach: {e}");
    }

    // Close CDP session AFTER extension cleanup is complete.
    if let Some(cdp) = cdp {
        cdp.clear_iframe_sessions().await;
        cdp.close().await;
    }

    if let Some(child) = chrome_process {
        crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
    }

    // Remove non-default profile directory after Chrome has fully exited.
    if let Some(profile) = profile_to_clean {
        let profile_dir = crate::config::profiles_dir().join(&profile);
        // Remove chrome.pid so a future browser start does not mistake the
        // now-dead PID for an orphan.
        let _ = std::fs::remove_file(profile_dir.join("chrome.pid"));
        if profile_dir.exists() {
            let _ = std::fs::remove_dir_all(&profile_dir);
        }
    }

    // Remove per-session data directory (snapshots, etc.).
    // Safety: only delete if the path is an absolute path under sessions_dir().
    let sessions_base = crate::config::sessions_dir();
    let session_data_dir = sessions_base.join(&cmd.session);
    if session_data_dir.is_absolute()
        && session_data_dir.starts_with(&sessions_base)
        && session_data_dir.exists()
    {
        let _ = std::fs::remove_dir_all(&session_data_dir);
    }

    ActionResult::ok(json!({
        "session_id": cmd.session,
        "status": "closed",
        "closed_tabs": closed_tabs,
    }))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::browser::session::provider::{ProviderEnv, ProviderSession};
    use crate::daemon::registry::{SessionEntry, SessionState, new_shared_registry};
    use crate::types::{Mode, SessionId};

    fn spawn_single_response_server(response: &'static str) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("mock server addr");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        request.extend_from_slice(&buf[..n]);
                        if request.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err)
                        if matches!(
                            err.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        break;
                    }
                    Err(err) => panic!("read request: {err}"),
                }
            }

            stream
                .write_all(response.as_bytes())
                .expect("write response");
            String::from_utf8(request).expect("utf8 request")
        });
        (format!("http://{}", addr), handle)
    }

    fn make_provider_session(
        provider: &str,
        session_id: &str,
        provider_env: ProviderEnv,
    ) -> ProviderSession {
        ProviderSession {
            provider: provider.to_string(),
            session_id: session_id.to_string(),
            provider_env,
        }
    }

    async fn insert_cloud_session(
        registry: &crate::daemon::registry::SharedRegistry,
        session_id: &str,
        provider_session: ProviderSession,
    ) {
        let mut entry = SessionEntry::starting(
            SessionId::new(session_id).expect("session id"),
            Mode::Cloud,
            true,
            true,
            "profile".to_string(),
        );
        entry.status = SessionState::Running;
        entry.provider = Some(provider_session.provider.clone());
        entry.provider_session = Some(provider_session);
        registry.lock().await.insert(entry);
    }

    #[tokio::test]
    async fn provider_close_failure_keeps_session_for_retry() {
        let (base_url, request_handle) = spawn_single_response_server(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 12\r\n\r\nbad-provider",
        );
        let registry = new_shared_registry();
        insert_cloud_session(
            &registry,
            "hyp1",
            make_provider_session(
                "hyperbrowser",
                "hb-session-1",
                ProviderEnv::from([
                    ("HYPERBROWSER_API_KEY".to_string(), "hb-key".to_string()),
                    ("HYPERBROWSER_API_URL".to_string(), base_url.clone()),
                ]),
            ),
        )
        .await;

        let result = execute(
            &Cmd {
                session: "hyp1".to_string(),
            },
            &registry,
        )
        .await;

        match result {
            ActionResult::Fatal { code, message, .. } => {
                assert_eq!(code, "API_SERVER_ERROR");
                assert!(message.contains("failed to close provider session"));
                assert!(message.contains("Hyperbrowser API server error"));
            }
            other => panic!("expected fatal result, got {other:?}"),
        }

        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("PUT /api/session/hb-session-1/stop HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("content-length: 0"));

        let reg = registry.lock().await;
        assert!(reg.get("hyp1").is_some(), "session should remain for retry");
    }

    #[tokio::test]
    async fn provider_close_success_removes_session() {
        let (base_url, request_handle) =
            spawn_single_response_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let registry = new_shared_registry();
        insert_cloud_session(
            &registry,
            "hyp1",
            make_provider_session(
                "hyperbrowser",
                "hb-session-1",
                ProviderEnv::from([
                    ("HYPERBROWSER_API_KEY".to_string(), "hb-key".to_string()),
                    ("HYPERBROWSER_API_URL".to_string(), base_url.clone()),
                ]),
            ),
        )
        .await;

        let result = execute(
            &Cmd {
                session: "hyp1".to_string(),
            },
            &registry,
        )
        .await;
        assert!(matches!(result, ActionResult::Ok { .. }));

        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("PUT /api/session/hb-session-1/stop HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("content-length: 0"));

        let reg = registry.lock().await;
        assert!(reg.get("hyp1").is_none(), "session should be removed");
    }
}
