use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::{SessionState, SharedRegistry};
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
    // ── Phase 1: reserve the close under the registry lock. ──
    //
    // Grab the provider handle (if any) AND flip the entry to `Closing`
    // atomically. A second `browser close` call arriving while the first
    // is still mid-flight (two agents racing, a retry timer firing on
    // top of a slow provider PUT, etc.) must NOT issue its own provider
    // stop — once the first call succeeds the remote session is gone,
    // and the second stop would either 404 or kill a new session that
    // reused the same provider ID.
    let (provider_session, prior_status) = {
        let mut reg = registry.lock().await;
        let entry = match reg.get_mut(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };
        if entry.status == SessionState::Closing {
            return ActionResult::fatal_with_hint(
                "SESSION_CLOSING",
                format!("session '{}' is already closing", cmd.session),
                "wait for the in-flight close to finish, then run `actionbook browser list-sessions`",
            );
        }
        let handle = entry.provider_session.clone();
        let prior = entry.status;
        entry.status = SessionState::Closing;
        (handle, prior)
    };

    // ── Phase 2: remote provider stop (slow, network-bound). ──
    //
    // No lock held here — other sessions can proceed concurrently, and
    // the short-circuit above prevents duplicate stops on *this* session.
    if let Some(provider_session) = provider_session.as_ref()
        && let Err(err) =
            crate::browser::session::provider::close_provider_session(provider_session).await
    {
        // Revert state so the caller can retry. Without this the entry
        // would stay stuck in Closing forever on a transient network blip.
        {
            let mut reg = registry.lock().await;
            if let Some(entry) = reg.get_mut(&cmd.session) {
                entry.status = prior_status;
            }
        }
        return ActionResult::fatal_with_hint(
            err.error_code(),
            format!(
                "failed to close provider session for '{}': {err}",
                cmd.session
            ),
            err.hint(),
        );
    }

    // ── Phase 3: local teardown under the registry lock. ──
    // Extract everything from registry then release the lock before slow I/O.
    // Windows: also extract the Job Object so we can terminate all Chrome
    // processes (main + helpers) atomically after releasing the lock.
    #[cfg(windows)]
    let chrome_job: Option<crate::daemon::chrome_reaper::ChromeJobObject>;

    let (closed_tabs, cdp, chrome_process, profile_to_clean, mode, ext_native_tab_ids) = {
        let mut reg = registry.lock().await;
        let mut entry = match reg.remove(&cmd.session) {
            Some(e) => e,
            None => {
                // Phase 1 set status=Closing, so this should be unreachable
                // under normal operation — another path would have to forcibly
                // evict the entry while we were in Phase 2. Treat it as success
                // from the caller's perspective: the remote is already stopped.
                return ActionResult::ok(json!({
                    "session_id": cmd.session,
                    "status": "closed",
                    "closed_tabs": 0,
                }));
            }
        };
        let tabs = entry.tabs_count();
        let entry_mode = entry.mode;

        // Only delete non-default profile directories for local sessions.
        // The default profile ("actionbook") is long-lived and preserves
        // user state (cookies, localStorage) across sessions.
        let profile_cleanup =
            if entry.chrome_process.is_some() && entry.profile != crate::config::DEFAULT_PROFILE {
                Some(entry.profile.clone())
            } else {
                None
            };

        // Extension mode: collect the native (Chrome numeric) tab IDs we
        // attached so we can ask the extension to close them. native_id is
        // a stringified i64 here.
        let ext_ids: Vec<u64> = if entry_mode == Mode::Extension {
            entry
                .tabs
                .iter()
                .filter_map(|t| t.native_id.parse::<u64>().ok())
                .collect()
        } else {
            Vec::new()
        };

        #[cfg(windows)]
        {
            chrome_job = entry.job_object.take();
        }

        reg.clear_session_ref_caches(&cmd.session);
        (
            tabs,
            entry.cdp.take(),
            entry.chrome_process.take(),
            profile_cleanup,
            entry_mode,
            ext_ids,
        )
    };
    // Registry lock released here — slow I/O below won't block other sessions.

    // Extension mode: detach the debugger AND close the chrome tabs the
    // session opened. Symmetric with local mode killing its chrome process —
    // the session "owns" the tabs it created via Extension.createTab and
    // attached via --tab-id, so leaving them around on close would leak
    // browser state across runs (and was responsible for tab-explosion in
    // the e2e suite).
    if mode == Mode::Extension
        && let Some(ref cdp) = cdp
        && !ext_native_tab_ids.is_empty()
        && let Err(e) = cdp
            .execute_browser(
                "Extension.closeTabs",
                serde_json::json!({ "tabIds": ext_native_tab_ids }),
            )
            .await
    {
        tracing::warn!("extension: failed to close tabs {ext_native_tab_ids:?}: {e}");
    }

    // Close CDP session AFTER extension cleanup is complete.
    if let Some(cdp) = cdp {
        cdp.clear_iframe_sessions().await;
        cdp.close().await;
    }

    if let Some(child) = chrome_process {
        // Windows: terminate the Job Object first — this kills all Chrome
        // processes (main process + renderer/GPU/utility helpers) atomically
        // without needing WMI or process enumeration.
        #[cfg(windows)]
        if let Some(job) = chrome_job {
            job.terminate();
        }

        // Reap the main Chrome process to release the OS process handle.
        // On Windows this is a no-op kill (already dead) followed by wait().
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

    fn spawn_single_response_server(
        response: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        spawn_single_response_server_with_delay(response, Duration::from_millis(0))
    }

    fn spawn_single_response_server_with_delay(
        response: &'static str,
        delay: Duration,
    ) -> (String, thread::JoinHandle<String>) {
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

            if !delay.is_zero() {
                thread::sleep(delay);
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
        let entry = reg.get("hyp1").expect("session should remain for retry");
        assert_eq!(
            entry.status,
            SessionState::Running,
            "state must be reverted to Running on failure so retries can proceed",
        );
    }

    #[tokio::test]
    async fn concurrent_close_second_caller_short_circuits() {
        // Two simultaneous `browser close s1` calls: only the first may
        // issue a provider stop. The second must short-circuit with
        // SESSION_CLOSING — no duplicate PUT, no race on provider teardown.
        //
        // Invariant: the mock server accepts exactly one connection. If
        // the second caller reached the provider stop path it would
        // block forever on connect (the listener is already drained),
        // so we wrap it in a timeout to turn that into a test failure.
        let (base_url, request_handle) = spawn_single_response_server_with_delay(
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
            Duration::from_millis(400),
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

        let reg_a = registry.clone();
        let a_handle = tokio::spawn(async move {
            execute(
                &Cmd {
                    session: "hyp1".to_string(),
                },
                &reg_a,
            )
            .await
        });

        // Give caller A time to mark the entry Closing and enter the
        // slow provider PUT.
        tokio::time::sleep(Duration::from_millis(75)).await;

        let b_result = tokio::time::timeout(
            Duration::from_secs(2),
            execute(
                &Cmd {
                    session: "hyp1".to_string(),
                },
                &registry,
            ),
        )
        .await
        .expect("second caller must not block on provider stop");

        match b_result {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "SESSION_CLOSING"),
            other => panic!("expected SESSION_CLOSING fatal, got {other:?}"),
        }

        let a_result = a_handle.await.expect("join A");
        assert!(
            matches!(a_result, ActionResult::Ok { .. }),
            "caller A should succeed, got {a_result:?}",
        );

        // Exactly one provider stop hit the mock server.
        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("PUT /api/session/hb-session-1/stop HTTP/1.1"));

        let reg = registry.lock().await;
        assert!(
            reg.get("hyp1").is_none(),
            "session should be removed after successful close",
        );
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
