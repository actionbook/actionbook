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
//
// Uses Win32 Job Objects to track and terminate all Chrome processes for a
// session (main process + renderer/GPU/utility helpers).  A named Job Object
// is created at Chrome launch and stored in the session registry.  On close
// or daemon restart, TerminateJobObject kills the entire process group
// atomically — no WMI, PowerShell, or process enumeration needed.
//
// Named format: "Local\actionbook-chrome-{profile_name}"
// This name survives daemon crashes, allowing the next daemon to reopen the
// job and kill orphaned Chrome processes during orphan recovery.

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, FALSE, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, OpenJobObjectW, TerminateJobObject,
        },
        Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject},
    },
};

// JOB_OBJECT_TERMINATE (0x0004) — required access right for OpenJobObjectW.
// Not exported by windows-sys 0.59's Win32_System_JobObjects feature.
#[cfg(windows)]
const JOB_OBJECT_TERMINATE: u32 = 0x0004;

/// `SYNCHRONIZE` access right (0x00100000) — required by `WaitForSingleObject`
/// on a process handle.
#[cfg(windows)]
const SYNCHRONIZE: u32 = 0x0010_0000;

/// A named Win32 Job Object that owns all Chrome processes for a session.
///
/// When Chrome's main process is assigned to this job, all Chrome child
/// processes (renderer, GPU, utility) automatically join the job as well.
/// `TerminateJobObject` then kills the entire group atomically.
///
/// The job is named `Local\actionbook-chrome-{profile_name}` so a new daemon
/// instance can reopen it after a crash to kill orphaned Chrome processes.
///
/// # Drop behaviour
///
/// Dropping a `ChromeJobObject` calls `TerminateJobObject` (kills all
/// remaining processes in the job) and then `CloseHandle`.  This ensures
/// that Chrome processes are always cleaned up when the job handle goes out
/// of scope — including in error paths inside `browser start`.
///
/// Note: a SIGKILL / `taskkill /F` on the daemon does **not** run Rust
/// destructors, so Chrome remains alive after a daemon crash (the required
/// behaviour for orphan-recovery tests).
#[cfg(windows)]
pub struct ChromeJobObject {
    handle: HANDLE,
}

#[cfg(windows)]
unsafe impl Send for ChromeJobObject {}
#[cfg(windows)]
unsafe impl Sync for ChromeJobObject {}

#[cfg(windows)]
impl ChromeJobObject {
    /// Create a new named Job Object for `profile_name`.
    ///
    /// Returns `None` if `CreateJobObjectW` fails (very unlikely in practice).
    pub fn create(profile_name: &str) -> Option<Self> {
        let name = format!("Local\\actionbook-chrome-{profile_name}");
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), wide.as_ptr()) };
        if handle.is_null() {
            tracing::warn!(profile_name, "ChromeJobObject::create failed");
            return None;
        }
        Some(Self { handle })
    }

    /// Reopen an existing named Job Object for orphan recovery.
    ///
    /// Returns `None` if the job no longer exists (Chrome already exited and
    /// released the last handle) or if access is denied.
    pub fn open(profile_name: &str) -> Option<Self> {
        let name = format!("Local\\actionbook-chrome-{profile_name}");
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe { OpenJobObjectW(JOB_OBJECT_TERMINATE, FALSE, wide.as_ptr()) };
        if handle.is_null() {
            return None;
        }
        Some(Self { handle })
    }

    /// Assign a process to this Job Object.
    ///
    /// `process_handle` must have `PROCESS_SET_QUOTA | PROCESS_TERMINATE` access.
    /// The `Child::as_raw_handle()` handle satisfies this on Windows.
    pub fn assign(&self, process_handle: HANDLE) -> bool {
        unsafe { AssignProcessToJobObject(self.handle, process_handle) != FALSE }
    }

    /// Terminate all processes currently in this Job Object.
    ///
    /// Returns `true` if the call succeeded.  Safe to call on an already-empty
    /// job (no-op).
    pub fn terminate(&self) -> bool {
        unsafe { TerminateJobObject(self.handle, 1) != FALSE }
    }
}

#[cfg(windows)]
impl Drop for ChromeJobObject {
    fn drop(&mut self) {
        // Terminate any surviving Chrome processes before releasing the handle.
        // This is the last-resort backstop for error paths where the caller did
        // not call terminate() explicitly.  Calling terminate() on an empty job
        // is a no-op, so this is safe in the normal close/shutdown paths too.
        unsafe {
            TerminateJobObject(self.handle, 1);
            CloseHandle(self.handle);
        }
    }
}

/// Force-terminate a single known PID and wait up to 5 s for it to exit.
/// Used as a fallback for orphan recovery when the Job Object cannot be opened.
#[cfg(windows)]
pub fn terminate_pid_and_wait(pid: u32) {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 1);
            WaitForSingleObject(handle, 5000);
            CloseHandle(handle);
        }
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
