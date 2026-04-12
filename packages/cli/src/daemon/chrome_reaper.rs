//! Centralized Chrome process cleanup.
//!
//! All Chrome kill/reap logic funnels through this module. Every call site
//! (session close, restart, start-failure, daemon shutdown) uses these
//! helpers instead of inlining `child.kill()` / `child.wait()`.

use std::process::Child;
#[cfg(unix)]
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
    #[cfg(windows)]
    {
        // On Windows, kill the entire process tree (/T) to ensure Chrome's
        // helper processes (renderer, GPU, utility) are also terminated.
        // child.kill() alone only terminates the main process, leaving helpers
        // alive and keeping the user-data-dir lock held.
        let pid = child.id();
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
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

// ─── Windows Chrome cleanup helpers ───────────────────────────────────────

/// Run a PowerShell command and parse the `COUNT:<n>` line from its stdout.
/// Returns 0 on any error or if no matching line is found.
#[cfg(windows)]
fn ps_run_and_get_count(ps_cmd: &str) -> u32 {
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_cmd])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("COUNT:")
                    .and_then(|n| n.parse::<u32>().ok())
            })
        })
        .unwrap_or(0)
}

/// Build a PowerShell snippet that:
///   1. Finds all processes whose `CommandLine` contains `dir_pattern`
///      (optionally excluding `exclude_pid`),
///   2. Kills each with `taskkill /F /PID`,
///   3. Waits for each to fully exit via `Wait-Process -Timeout 5` (process-
///      handle based — no WMI lag), and
///   4. Outputs `COUNT:<n>` where `n` is the number of PIDs that were found.
///
/// Waiting on process handles rather than polling WMI avoids the "re-parenting
/// transient" that can make Chrome helpers briefly appear or disappear in WMI
/// after the main Chrome process exits.
#[cfg(windows)]
fn build_kill_wait_ps(dir_pattern: &str, exclude_pid: Option<u32>) -> String {
    let filter = match exclude_pid {
        Some(pid) => format!(
            "Where-Object {{ $_.CommandLine -like '*{}*' -and $_.ProcessId -ne {} }}",
            dir_pattern, pid
        ),
        None => format!(
            "Where-Object {{ $_.CommandLine -like '*{}*' }}",
            dir_pattern
        ),
    };
    format!(
        "$pids = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | \
         {} | \
         Select-Object -ExpandProperty ProcessId); \
         foreach ($p in $pids) {{ & taskkill.exe /F /PID $p 2>&1 | Out-Null }}; \
         foreach ($p in $pids) {{ \
             Wait-Process -Id ([int]$p) -Timeout 5 -ErrorAction SilentlyContinue \
         }}; \
         Write-Output \"COUNT:$($pids.Count)\"",
        filter
    )
}

/// Kill any remaining Chrome processes whose command line contains the given
/// `user_data_dir` path.  Fire-and-forget — does not wait for exit.
/// Prefer [`kill_and_wait_for_chrome_by_user_data_dir`] when a confirmed-dead
/// guarantee is needed.
#[cfg(windows)]
pub fn kill_chrome_by_user_data_dir(user_data_dir: &std::path::Path) {
    let dir_str = user_data_dir.display().to_string().replace('\'', "''");
    let ps_cmd = format!(
        "Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | \
         Where-Object {{ $_.CommandLine -like '*{}*' }} | \
         ForEach-Object {{ & taskkill.exe /F /PID $_.ProcessId 2>&1 | Out-Null }}",
        dir_str
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_cmd])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Kill all Chrome processes matching `user_data_dir` and block until they
/// have fully exited (confirmed via process handles, not WMI polling).
///
/// Loops up to 10 seconds to catch any Chrome that relaunches itself.
/// Intentionally synchronous — callers in async contexts should use
/// [`kill_and_wait_for_chrome_by_user_data_dir_async`].
#[cfg(windows)]
pub fn kill_and_wait_for_chrome_by_user_data_dir(user_data_dir: &std::path::Path) {
    let dir_str = user_data_dir.display().to_string().replace('\'', "''");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ps_cmd = build_kill_wait_ps(&dir_str, None);
        let found = ps_run_and_get_count(&ps_cmd);
        if found == 0 || std::time::Instant::now() >= deadline {
            break;
        }
        // found > 0: processes were found, killed, and waited on; sleep briefly
        // then check again in case Chrome relaunched during the window.
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Async wrapper around [`kill_and_wait_for_chrome_by_user_data_dir`].
#[cfg(windows)]
pub async fn kill_and_wait_for_chrome_by_user_data_dir_async(user_data_dir: std::path::PathBuf) {
    let _ = tokio::task::spawn_blocking(move || {
        kill_and_wait_for_chrome_by_user_data_dir(&user_data_dir);
    })
    .await;
}

/// Kill all Chrome HELPER processes for `user_data_dir` (every process
/// matching the dir except `main_pid`) and block until they have fully exited.
///
/// **Call this BEFORE [`kill_and_reap_async`].**  When Chrome's main process
/// dies first, its re-parented helpers can briefly enter a transient state
/// where they appear or disappear unpredictably in WMI.  Killing helpers
/// while the main process is still alive avoids that window entirely.
///
/// Intentionally synchronous — use [`kill_chrome_helpers_and_wait_async`] from
/// async contexts.
#[cfg(windows)]
pub fn kill_chrome_helpers_and_wait(user_data_dir: &std::path::Path, main_pid: u32) {
    let dir_str = user_data_dir.display().to_string().replace('\'', "''");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ps_cmd = build_kill_wait_ps(&dir_str, Some(main_pid));
        let found = ps_run_and_get_count(&ps_cmd);
        if found == 0 || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Async wrapper around [`kill_chrome_helpers_and_wait`].
#[cfg(windows)]
pub async fn kill_chrome_helpers_and_wait_async(user_data_dir: std::path::PathBuf, main_pid: u32) {
    let _ = tokio::task::spawn_blocking(move || {
        kill_chrome_helpers_and_wait(&user_data_dir, main_pid);
    })
    .await;
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
