//! Centralized Chrome process cleanup.
//!
//! All Chrome kill/reap logic funnels through this module. Every call site
//! (session close, restart, start-failure, daemon shutdown) uses these
//! helpers instead of inlining `child.kill()` / `child.wait()`.

use std::process::Child;
use std::time::{Duration, Instant};

/// Gracefully terminate and reap a Chrome child process.
///
/// Sends SIGTERM first so Chrome can flush Preferences (window placement,
/// cookies, etc.), then waits up to 3 seconds for exit. Falls back to
/// SIGKILL if the process is still alive.
///
/// This is intentionally synchronous — callers in async contexts should
/// wrap it in `spawn_blocking(...).await`.
pub fn kill_and_reap(child: &mut Child) {
    // Send SIGTERM for graceful shutdown (Unix only).
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe extern "C" {
            safe fn kill(pid: i32, sig: i32) -> i32;
        }
        let _ = kill(pid, 15); // SIGTERM

        // Wait up to 3s for Chrome to exit gracefully.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return, // exited
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => break, // timed out or error
            }
        }
    }

    // Force kill (fallback on Unix, primary on Windows).
    let _ = child.kill();
    let _ = child.wait();
}

/// Async wrapper: moves the `Child` into a blocking task, kills, reaps,
/// and **awaits** completion (unlike the old fire-and-forget pattern).
pub async fn kill_and_reap_async(mut child: Child) {
    let _ = tokio::task::spawn_blocking(move || {
        kill_and_reap(&mut child);
    })
    .await;
}

/// Take `Option<Child>`, kill and reap if present. Takes ownership
/// (sets to `None`) to prevent double-cleanup from Drop.
pub fn kill_and_reap_option(child: &mut Option<Child>) {
    if let Some(mut c) = child.take() {
        kill_and_reap(&mut c);
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    /// Spawn a process that sleeps forever, useful for testing kill/reap.
    fn spawn_sleeper() -> Child {
        Command::new("sleep")
            .arg("3600")
            .spawn()
            .expect("failed to spawn sleep process")
    }

    fn is_process_alive(pid: u32) -> bool {
        // kill -0 checks existence without sending a signal
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[test]
    fn kill_and_reap_kills_running_process() {
        let mut child = spawn_sleeper();
        let pid = child.id();
        assert!(is_process_alive(pid), "process should be alive before kill");

        kill_and_reap(&mut child);

        // After kill+reap, the process must no longer exist
        assert!(
            !is_process_alive(pid),
            "process should be dead after kill_and_reap"
        );
    }

    #[test]
    fn kill_and_reap_idempotent_on_already_exited() {
        let mut child = spawn_sleeper();
        let _ = child.kill();
        let _ = child.wait();

        // Calling again on an already-reaped process should not panic
        kill_and_reap(&mut child);
    }

    #[test]
    fn kill_and_reap_option_none_is_noop() {
        let mut opt: Option<Child> = None;
        kill_and_reap_option(&mut opt); // must not panic
    }

    #[test]
    fn kill_and_reap_option_some_kills_process() {
        let child = spawn_sleeper();
        let pid = child.id();
        let mut opt = Some(child);

        kill_and_reap_option(&mut opt);

        assert!(
            !is_process_alive(pid),
            "process should be dead after kill_and_reap_option"
        );
    }

    #[tokio::test]
    async fn kill_and_reap_async_awaits_completion() {
        let child = spawn_sleeper();
        let pid = child.id();
        assert!(is_process_alive(pid));

        kill_and_reap_async(child).await;

        assert!(
            !is_process_alive(pid),
            "process should be dead after kill_and_reap_async"
        );
    }
}
