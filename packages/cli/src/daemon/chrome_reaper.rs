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
// Uses Win32 APIs (CreateToolhelp32Snapshot + NtQueryInformationProcess) to
// enumerate and terminate Chrome processes by user-data-dir.  This approach
// works from any process context — including a DETACHED_PROCESS daemon —
// without relying on WMI or PowerShell, which can fail to enumerate
// processes when invoked from a detached or non-interactive process.

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{BOOL, CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE, UNICODE_STRING},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_TERMINATE,
        },
    },
};

/// `SYNCHRONIZE` access right (0x00100000) — required by `WaitForSingleObject`
/// on a process handle.
#[cfg(windows)]
const SYNCHRONIZE: u32 = 0x0010_0000;

/// Declare `NtQueryInformationProcess` from `ntdll.dll`.
///
/// `ProcessCommandLineInformation` (class 60, available since Windows 8.1)
/// only requires `PROCESS_QUERY_LIMITED_INFORMATION`, works from any process
/// context (no COM/WMI initialization needed), and returns the process
/// command-line string in our own output buffer — no cross-process memory
/// reads required.
#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process_handle: HANDLE,
        process_information_class: i32,
        process_information: *mut ::core::ffi::c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

/// Read the command-line string of a process via `NtQueryInformationProcess`.
///
/// Returns `None` if the process has already exited, access is denied, or
/// the command line cannot be decoded.
///
/// After the call, the kernel adjusts the `UNICODE_STRING.Buffer` pointer to
/// reference the string data immediately following the structure, within our
/// own allocation — no cross-process memory access needed.
#[cfg(windows)]
fn read_process_cmdline(pid: u32) -> Option<String> {
    const PROCESS_COMMAND_LINE_INFORMATION: i32 = 60;
    const STATUS_SUCCESS: i32 = 0;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if handle == 0 {
            return None;
        }

        // First call: obtain the required buffer size.
        let mut return_length: u32 = 0;
        NtQueryInformationProcess(
            handle,
            PROCESS_COMMAND_LINE_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut return_length,
        );

        if return_length == 0 {
            CloseHandle(handle);
            return None;
        }

        let mut buf = vec![0u8; return_length as usize];
        let status = NtQueryInformationProcess(
            handle,
            PROCESS_COMMAND_LINE_INFORMATION,
            buf.as_mut_ptr().cast(),
            return_length,
            &mut return_length,
        );

        CloseHandle(handle);

        if status != STATUS_SUCCESS {
            return None;
        }

        // The buffer starts with a UNICODE_STRING. The kernel sets Buffer to
        // point into our output buffer (self-relative), right after the struct.
        if buf.len() < std::mem::size_of::<UNICODE_STRING>() {
            return None;
        }

        // read_unaligned avoids alignment UB since buf is byte-aligned.
        let us: UNICODE_STRING = std::ptr::read_unaligned(buf.as_ptr().cast());
        if us.Buffer.is_null() || us.Length == 0 {
            return None;
        }

        let char_count = us.Length as usize / 2;
        let str_start = us.Buffer as usize;
        let buf_start = buf.as_ptr() as usize;
        let buf_end = buf_start + buf.len();

        // Verify the Buffer pointer is within our allocation.
        if str_start < buf_start || str_start.saturating_add(us.Length as usize) > buf_end {
            return None;
        }

        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            us.Buffer,
            char_count,
        )))
    }
}

/// Convert a null-terminated UTF-16 slice to a `String`.
#[cfg(windows)]
fn wstr_to_string(wstr: &[u16]) -> String {
    let len = wstr.iter().position(|&c| c == 0).unwrap_or(wstr.len());
    String::from_utf16_lossy(&wstr[..len])
}

/// Enumerate PIDs of `chrome.exe` processes whose command line contains
/// `dir_pattern` (case-insensitive substring match).
///
/// Uses [`CreateToolhelp32Snapshot`] — no WMI, no PowerShell.  Kernel
/// snapshots are unaffected by Chrome's helper-process re-parenting, so
/// all Chrome processes (main, renderer, GPU, utility) are always visible.
///
/// Pass `exclude_pid` to skip one PID (e.g., the main Chrome process when
/// enumerating helpers only).
#[cfg(windows)]
fn enumerate_chrome_pids_for_dir(dir_pattern: &str, exclude_pid: Option<u32>) -> Vec<u32> {
    let dir_lower = dir_pattern.to_lowercase();
    let mut result = Vec::new();

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return result;
    }

    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    if unsafe { Process32FirstW(snapshot, &mut entry) } != FALSE {
        loop {
            let pid = entry.th32ProcessID;
            let exe_name = wstr_to_string(&entry.szExeFile).to_lowercase();

            if exe_name == "chrome.exe" && exclude_pid != Some(pid) {
                if let Some(cmdline) = read_process_cmdline(pid) {
                    if cmdline.to_lowercase().contains(&dir_lower) {
                        result.push(pid);
                    }
                }
            }

            if unsafe { Process32NextW(snapshot, &mut entry) } == FALSE {
                break;
            }
        }
    }

    unsafe { CloseHandle(snapshot) };
    result
}

/// Force-terminate each PID in `pids` and wait up to 2 s per process for exit.
#[cfg(windows)]
fn terminate_pids_and_wait(pids: &[u32]) {
    for &pid in pids {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, pid);
            if handle == 0 {
                continue; // Process already exited or no access
            }
            TerminateProcess(handle, 1);
            WaitForSingleObject(handle, 2000);
            CloseHandle(handle);
        }
    }
}

/// Kill all Chrome processes matching `user_data_dir` and block until they
/// have fully exited.
///
/// Uses Win32 [`CreateToolhelp32Snapshot`] — works reliably from daemon
/// context without WMI.  Loops up to 10 seconds to catch any Chrome that
/// relaunches itself.  Intentionally synchronous — callers in async contexts
/// should use [`kill_and_wait_for_chrome_by_user_data_dir_async`].
#[cfg(windows)]
pub fn kill_and_wait_for_chrome_by_user_data_dir(user_data_dir: &std::path::Path) {
    let dir_str = user_data_dir.display().to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let pids = enumerate_chrome_pids_for_dir(&dir_str, None);
        if pids.is_empty() || std::time::Instant::now() >= deadline {
            break;
        }
        tracing::debug!(?pids, "chrome_reaper: force-terminating Chrome processes");
        terminate_pids_and_wait(&pids);
        std::thread::sleep(std::time::Duration::from_millis(100));
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

/// Kill all Chrome HELPER processes for `user_data_dir` (every `chrome.exe`
/// matching the dir except `main_pid`) and block until they exit.
///
/// Uses Win32 — no WMI or re-parenting transient issues.
/// Intentionally synchronous — use [`kill_chrome_helpers_and_wait_async`]
/// from async contexts.
#[cfg(windows)]
pub fn kill_chrome_helpers_and_wait(user_data_dir: &std::path::Path, main_pid: u32) {
    let dir_str = user_data_dir.display().to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let pids = enumerate_chrome_pids_for_dir(&dir_str, Some(main_pid));
        if pids.is_empty() || std::time::Instant::now() >= deadline {
            break;
        }
        tracing::debug!(?pids, main_pid, "chrome_reaper: force-terminating Chrome helpers");
        terminate_pids_and_wait(&pids);
        std::thread::sleep(std::time::Duration::from_millis(100));
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
