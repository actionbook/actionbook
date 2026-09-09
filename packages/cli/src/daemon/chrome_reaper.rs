//! Centralized Chrome process cleanup.
//!
//! All Chrome kill/reap logic funnels through this module. Every call site
//! (session close, restart, start-failure, daemon shutdown) uses these
//! helpers instead of inlining `child.kill()` / `child.wait()`.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const OWNERSHIP_VERSION: u32 = 1;
const OWNERSHIP_FILE: &str = "chrome-ownership.json";
const LEGACY_PID_MARKER: &str = "chrome.pid";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ChromeOwnership {
    version: u32,
    session_id: String,
    profile: String,
    profile_dir: String,
    pid: u32,
    process_start_identity: String,
    command_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OrphanCleanup {
    NoOwnership,
    AlreadyGone { profile: String },
    Stopped { profile: String },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OwnershipError {
    #[error("cannot read Chrome ownership metadata at {path}: {reason}")]
    MetadataRead { path: String, reason: String },
    #[error("invalid Chrome ownership metadata at {path}: {reason}")]
    MetadataInvalid { path: String, reason: String },
    #[error("Chrome ownership metadata is inconsistent: {0}")]
    MetadataMismatch(String),
    #[error("Chrome PID {pid} ownership could not be proven: {reason}")]
    Unverified { pid: u32, reason: String },
    #[error("failed to stop verified Actionbook Chrome PID {pid}: {reason}")]
    StopFailed { pid: u32, reason: String },
}

impl OwnershipError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::StopFailed { .. } => "CHROME_STOP_FAILED",
            _ => "CHROME_OWNERSHIP_UNVERIFIED",
        }
    }

    pub(crate) fn hint(&self) -> &'static str {
        "inspect the reported ownership metadata and process; do not delete PID/lock files or kill Chrome unless its Actionbook ownership can be verified"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessIdentity {
    start_identity: String,
    command_line: String,
}

#[derive(Debug, Clone)]
struct ProcessSnapshot {
    pid: u32,
    ppid: u32,
    identity: ProcessIdentity,
}

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

pub(crate) fn record_chrome_ownership(
    session_id: &str,
    profile: &str,
    profile_dir: &Path,
    child: &Child,
) -> Result<(), OwnershipError> {
    let profile_dir = canonical_profile_dir(profile_dir)?;
    remove_legacy_pid_marker(&profile_dir)?;
    let identity = inspect_process(child.id()).map_err(|reason| OwnershipError::Unverified {
        pid: child.id(),
        reason,
    })?;
    let Some(identity) = identity else {
        return Err(OwnershipError::Unverified {
            pid: child.id(),
            reason: "process exited before ownership metadata could be recorded".to_string(),
        });
    };

    let ownership = ChromeOwnership {
        version: OWNERSHIP_VERSION,
        session_id: session_id.to_string(),
        profile: profile.to_string(),
        profile_dir: profile_dir.to_string_lossy().into_owned(),
        pid: child.id(),
        process_start_identity: identity.start_identity,
        command_line: identity.command_line,
    };
    validate_record(&ownership, session_id, Some(profile), &profile_dir)?;
    if !verify_owned_process(&ownership)? {
        return Err(OwnershipError::Unverified {
            pid: child.id(),
            reason: "process exited before ownership metadata could be verified".to_string(),
        });
    }

    write_record_atomic(&profile_dir.join(OWNERSHIP_FILE), &ownership)
}

pub(crate) fn recover_orphan_for_session(
    session_id: &str,
) -> Result<OrphanCleanup, OwnershipError> {
    let Some((record, profile_dir)) = find_profile_record_for_session(session_id)? else {
        return Ok(OrphanCleanup::NoOwnership);
    };
    recover_verified_record(record, &profile_dir)
}

pub(crate) fn recover_orphan_for_profile(
    profile: &str,
    profile_dir: &Path,
) -> Result<OrphanCleanup, OwnershipError> {
    let profile_dir = canonical_profile_dir(profile_dir)?;
    remove_legacy_pid_marker(&profile_dir)?;
    let Some(profile_record) = read_record(&profile_dir.join(OWNERSHIP_FILE))? else {
        let matching = processes_using_profile(&profile_dir)?;
        if matching.is_empty() {
            return Ok(OrphanCleanup::NoOwnership);
        }
        return Err(OwnershipError::Unverified {
            pid: matching[0],
            reason: format!(
                "process uses profile '{}' but durable ownership metadata is missing",
                profile_dir.display()
            ),
        });
    };
    validate_record(
        &profile_record,
        &profile_record.session_id,
        Some(profile),
        &profile_dir,
    )?;
    recover_verified_record(profile_record, &profile_dir)
}

fn recover_verified_record(
    ownership: ChromeOwnership,
    profile_dir: &Path,
) -> Result<OrphanCleanup, OwnershipError> {
    if !profile_dir.is_dir() {
        return Err(OwnershipError::MetadataMismatch(format!(
            "owned profile directory '{}' is missing",
            profile_dir.display()
        )));
    }

    let was_running = verify_owned_process(&ownership)?;
    if was_running {
        terminate_owned_process_tree(&ownership)?;
    } else {
        let matching = wait_for_profile_processes_to_exit(profile_dir)?;
        if !matching.is_empty() {
            return Err(OwnershipError::Unverified {
                pid: matching[0],
                reason: format!(
                    "recorded PID {} is gone but another process still uses profile '{}'",
                    ownership.pid,
                    profile_dir.display()
                ),
            });
        }
    }

    cleanup_terminated_ownership(&ownership, profile_dir)?;
    if was_running {
        Ok(OrphanCleanup::Stopped {
            profile: ownership.profile,
        })
    } else {
        Ok(OrphanCleanup::AlreadyGone {
            profile: ownership.profile,
        })
    }
}

fn validate_record(
    record: &ChromeOwnership,
    session_id: &str,
    profile: Option<&str>,
    profile_dir: &Path,
) -> Result<(), OwnershipError> {
    if record.version != OWNERSHIP_VERSION {
        return Err(OwnershipError::MetadataMismatch(format!(
            "unsupported ownership version {}",
            record.version
        )));
    }
    if record.session_id != session_id {
        return Err(OwnershipError::MetadataMismatch(format!(
            "record names session '{}' instead of '{session_id}'",
            record.session_id
        )));
    }
    if crate::types::SessionId::new(record.session_id.clone()).is_err() {
        return Err(OwnershipError::MetadataMismatch(
            "record contains an invalid session ID".to_string(),
        ));
    }
    if profile.is_some_and(|expected| record.profile != expected) {
        return Err(OwnershipError::MetadataMismatch(format!(
            "record names profile '{}' instead of '{}'",
            record.profile,
            profile.unwrap_or_default()
        )));
    }
    if record.profile.contains('/')
        || record.profile.contains('\\')
        || record.profile.contains("..")
    {
        return Err(OwnershipError::MetadataMismatch(
            "record contains an unsafe profile name".to_string(),
        ));
    }
    if Path::new(&record.profile_dir) != profile_dir {
        return Err(OwnershipError::MetadataMismatch(format!(
            "recorded profile path '{}' does not match expected path '{}'",
            record.profile_dir,
            profile_dir.display()
        )));
    }
    if record.pid == 0
        || record.process_start_identity.trim().is_empty()
        || record.command_line.trim().is_empty()
    {
        return Err(OwnershipError::MetadataMismatch(
            "record is missing process identity fields".to_string(),
        ));
    }
    Ok(())
}

fn expected_profile_dir(profile: &str) -> Result<PathBuf, OwnershipError> {
    if profile.contains('/') || profile.contains('\\') || profile.contains("..") {
        return Err(OwnershipError::MetadataMismatch(format!(
            "unsafe profile name '{profile}'"
        )));
    }
    canonical_profile_dir(&crate::config::profiles_dir().join(profile))
}

fn canonical_profile_dir(path: &Path) -> Result<PathBuf, OwnershipError> {
    std::fs::canonicalize(path).map_err(|error| OwnershipError::MetadataRead {
        path: path.display().to_string(),
        reason: error.to_string(),
    })
}

fn read_record(path: &Path) -> Result<Option<ChromeOwnership>, OwnershipError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(OwnershipError::MetadataRead {
                path: path.display().to_string(),
                reason: error.to_string(),
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(OwnershipError::MetadataInvalid {
            path: path.display().to_string(),
            reason: "ownership path is not a regular file".to_string(),
        });
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(OwnershipError::MetadataRead {
                path: path.display().to_string(),
                reason: error.to_string(),
            });
        }
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| OwnershipError::MetadataInvalid {
            path: path.display().to_string(),
            reason: error.to_string(),
        })
}

fn write_record_atomic(path: &Path, record: &ChromeOwnership) -> Result<(), OwnershipError> {
    let bytes = serde_json::to_vec(record).map_err(|error| OwnershipError::MetadataInvalid {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temp, bytes).map_err(|error| OwnershipError::MetadataRead {
        path: temp.display().to_string(),
        reason: error.to_string(),
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&temp, path).map_err(|error| OwnershipError::MetadataRead {
        path: path.display().to_string(),
        reason: error.to_string(),
    })
}

fn remove_optional_file(path: &Path) -> Result<(), OwnershipError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(OwnershipError::MetadataRead {
            path: path.display().to_string(),
            reason: error.to_string(),
        }),
    }
}

fn remove_legacy_pid_marker(profile_dir: &Path) -> Result<(), OwnershipError> {
    remove_optional_file(&profile_dir.join(LEGACY_PID_MARKER))
}

fn clear_ownership_after_managed_termination(profile_dir: &Path) -> Result<(), OwnershipError> {
    remove_optional_file(&profile_dir.join(OWNERSHIP_FILE))?;
    remove_legacy_pid_marker(profile_dir)
}

fn find_profile_record_for_session(
    session_id: &str,
) -> Result<Option<(ChromeOwnership, PathBuf)>, OwnershipError> {
    let base = crate::config::profiles_dir();
    let entries = match std::fs::read_dir(&base) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(OwnershipError::MetadataRead {
                path: base.display().to_string(),
                reason: error.to_string(),
            });
        }
    };
    let mut profile_dirs = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| OwnershipError::MetadataRead {
                    path: base.display().to_string(),
                    reason: error.to_string(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    profile_dirs.sort();

    let mut matching = Vec::new();
    for profile_dir in profile_dirs {
        if !profile_dir.is_dir() {
            continue;
        }
        remove_legacy_pid_marker(&profile_dir)?;
        let Some(record) = read_record(&profile_dir.join(OWNERSHIP_FILE))? else {
            continue;
        };
        if record.session_id != session_id {
            continue;
        }
        let canonical_dir = canonical_profile_dir(&profile_dir)?;
        let expected_dir = expected_profile_dir(&record.profile)?;
        if canonical_dir != expected_dir {
            return Err(OwnershipError::MetadataMismatch(format!(
                "ownership file '{}' names profile '{}'",
                profile_dir.join(OWNERSHIP_FILE).display(),
                record.profile
            )));
        }
        validate_record(&record, session_id, Some(&record.profile), &canonical_dir)?;
        matching.push((record, canonical_dir));
    }

    match matching.len() {
        0 => Ok(None),
        1 => Ok(matching.pop()),
        count => Err(OwnershipError::MetadataMismatch(format!(
            "{count} profile ownership records name session '{session_id}'"
        ))),
    }
}

fn verify_owned_process(record: &ChromeOwnership) -> Result<bool, OwnershipError> {
    let current = inspect_process(record.pid).map_err(|reason| OwnershipError::Unverified {
        pid: record.pid,
        reason,
    })?;
    let Some(current) = current else {
        return Ok(false);
    };
    if current.start_identity != record.process_start_identity {
        return Err(OwnershipError::Unverified {
            pid: record.pid,
            reason: "process start identity differs (stale/reused PID)".to_string(),
        });
    }
    if current.command_line != record.command_line {
        return Err(OwnershipError::Unverified {
            pid: record.pid,
            reason: "process command line differs from the recorded Chrome command".to_string(),
        });
    }

    #[cfg(unix)]
    {
        let launch_profile = current
            .command_line
            .split_once("--user-data-dir=")
            .and_then(|(_, tail)| tail.split(" --").next())
            .and_then(|path| std::fs::canonicalize(path).ok());
        if launch_profile.as_deref() != Some(Path::new(&record.profile_dir))
            || !current.command_line.contains("--remote-debugging-port=")
        {
            return Err(OwnershipError::Unverified {
                pid: record.pid,
                reason: "command line does not resolve to the recorded profile path and Actionbook CDP launch marker".to_string(),
            });
        }
    }
    #[cfg(windows)]
    if !ChromeJobObject::contains_pid(&record.profile, record.pid) {
        return Err(OwnershipError::Unverified {
            pid: record.pid,
            reason: "process is not a member of the named Actionbook profile Job Object"
                .to_string(),
        });
    }
    Ok(true)
}

fn wait_for_profile_processes_to_exit(profile_dir: &Path) -> Result<Vec<u32>, OwnershipError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let matching = processes_using_profile(profile_dir)?;
        if matching.is_empty() || Instant::now() >= deadline {
            return Ok(matching);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn cleanup_terminated_ownership(
    record: &ChromeOwnership,
    profile_dir: &Path,
) -> Result<(), OwnershipError> {
    if inspect_process(record.pid)
        .map_err(|reason| OwnershipError::StopFailed {
            pid: record.pid,
            reason,
        })?
        .is_some()
    {
        return Err(OwnershipError::StopFailed {
            pid: record.pid,
            reason: "process is still alive after termination".to_string(),
        });
    }
    let matching = processes_using_profile(profile_dir)?;
    if !matching.is_empty() {
        return Err(OwnershipError::StopFailed {
            pid: record.pid,
            reason: format!("profile process tree still contains PIDs {matching:?}"),
        });
    }

    clear_ownership_after_managed_termination(profile_dir)?;
    for lock in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        remove_optional_file(&profile_dir.join(lock))?;
    }

    let sessions_base = crate::config::sessions_dir();
    let session_dir = crate::config::session_data_dir(&record.session_id);
    if !session_dir.is_absolute() || !session_dir.starts_with(&sessions_base) {
        return Err(OwnershipError::MetadataMismatch(format!(
            "session data path '{}' is outside '{}'",
            session_dir.display(),
            sessions_base.display()
        )));
    }
    match std::fs::remove_dir_all(&session_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(OwnershipError::MetadataRead {
            path: session_dir.display().to_string(),
            reason: error.to_string(),
        }),
    }
}

#[cfg(unix)]
fn inspect_process(pid: u32) -> Result<Option<ProcessIdentity>, String> {
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    if kill(pid as i32, 0) != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(3) {
            return Ok(None);
        }
        return Err(format!("cannot probe process: {error}"));
    }

    fn ps_field(pid: u32, field: &str) -> Result<String, String> {
        let output = std::process::Command::new("ps")
            .args(["-ww", "-p", &pid.to_string(), "-o", field])
            .output()
            .map_err(|error| format!("failed to execute ps: {error}"))?;
        if !output.status.success() {
            return Err(format!("ps failed with status {}", output.status));
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            return Err(format!("ps returned an empty {field}"));
        }
        Ok(value)
    }

    Ok(Some(ProcessIdentity {
        start_identity: ps_field(pid, "lstart=")?,
        command_line: ps_field(pid, "command=")?,
    }))
}

#[cfg(unix)]
fn unix_process_ids() -> Result<Vec<(u32, u32)>, String> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .map_err(|error| format!("failed to list processes: {error}"))?;
    if !output.status.success() {
        return Err(format!("ps failed with status {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
        })
        .collect())
}

#[cfg(unix)]
fn processes_using_profile(profile_dir: &Path) -> Result<Vec<u32>, OwnershipError> {
    let output = std::process::Command::new("ps")
        .args(["-eww", "-axo", "pid=,command="])
        .output()
        .map_err(|error| OwnershipError::MetadataRead {
            path: "process table".to_string(),
            reason: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(OwnershipError::MetadataRead {
            path: "process table".to_string(),
            reason: format!("ps failed with status {}", output.status),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let split = line.find(char::is_whitespace)?;
            let pid = line[..split].parse::<u32>().ok()?;
            let command = line[split..].trim_start();
            let launch_profile = command
                .split_once("--user-data-dir=")
                .and_then(|(_, tail)| tail.split(" --").next())
                .and_then(|path| std::fs::canonicalize(path).ok());
            (launch_profile.as_deref() == Some(profile_dir)).then_some(pid)
        })
        .collect())
}

#[cfg(unix)]
fn terminate_owned_process_tree(record: &ChromeOwnership) -> Result<(), OwnershipError> {
    let root_identity = inspect_process(record.pid)
        .map_err(|reason| OwnershipError::Unverified {
            pid: record.pid,
            reason,
        })?
        .ok_or_else(|| OwnershipError::Unverified {
            pid: record.pid,
            reason: "process exited between ownership verification and termination".to_string(),
        })?;
    let process_ids = unix_process_ids().map_err(|reason| OwnershipError::Unverified {
        pid: record.pid,
        reason,
    })?;
    let mut owned_pids = vec![record.pid];
    loop {
        let before = owned_pids.len();
        for (pid, ppid) in &process_ids {
            if owned_pids.contains(ppid) && !owned_pids.contains(pid) {
                owned_pids.push(*pid);
            }
        }
        if owned_pids.len() == before {
            break;
        }
    }
    let mut snapshots = Vec::new();
    for pid in owned_pids {
        if let Some(identity) =
            inspect_process(pid).map_err(|reason| OwnershipError::Unverified { pid, reason })?
        {
            let ppid = process_ids
                .iter()
                .find_map(|(candidate, parent)| (*candidate == pid).then_some(*parent))
                .unwrap_or(0);
            snapshots.push(ProcessSnapshot {
                pid,
                ppid,
                identity,
            });
        }
    }
    if snapshots
        .first()
        .is_none_or(|snapshot| snapshot.identity != root_identity)
    {
        return Err(OwnershipError::Unverified {
            pid: record.pid,
            reason: "root process identity changed before tree capture".to_string(),
        });
    }

    signal_if_same(&snapshots[0], 15)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if snapshots.iter().all(process_snapshot_gone) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Chrome normally tears down its helpers after root SIGTERM. Any verified
    // survivors are force-killed using the start identities captured while the
    // verified root still owned the process tree.
    snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.ppid));
    for snapshot in &snapshots {
        if !process_snapshot_gone(snapshot) {
            signal_if_same(snapshot, 9)?;
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if snapshots.iter().all(process_snapshot_gone) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(OwnershipError::StopFailed {
        pid: record.pid,
        reason: "verified process tree did not terminate within 5 seconds".to_string(),
    })
}

#[cfg(unix)]
fn process_snapshot_gone(snapshot: &ProcessSnapshot) -> bool {
    match inspect_process(snapshot.pid) {
        Ok(None) => true,
        Ok(Some(identity)) => identity != snapshot.identity,
        Err(_) => false,
    }
}

#[cfg(unix)]
fn signal_if_same(snapshot: &ProcessSnapshot, signal: i32) -> Result<(), OwnershipError> {
    let current = inspect_process(snapshot.pid).map_err(|reason| OwnershipError::Unverified {
        pid: snapshot.pid,
        reason,
    })?;
    let Some(current) = current else {
        return Ok(());
    };
    if current != snapshot.identity {
        return Ok(());
    }
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    if kill(snapshot.pid as i32, signal) == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(3) {
        Ok(())
    } else {
        Err(OwnershipError::StopFailed {
            pid: snapshot.pid,
            reason: error.to_string(),
        })
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
    Foundation::{CloseHandle, FALSE, FILETIME, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, OpenJobObjectW,
            TerminateJobObject,
        },
        Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
};

// Access rights required for verification and orphan termination.
#[cfg(windows)]
const JOB_OBJECT_QUERY: u32 = 0x0004;
#[cfg(windows)]
const JOB_OBJECT_TERMINATE: u32 = 0x0008;

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
    terminate_on_drop: bool,
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
        Some(Self {
            handle,
            terminate_on_drop: true,
        })
    }

    /// Reopen an existing named Job Object for orphan recovery.
    ///
    /// Returns `None` if the job no longer exists (Chrome already exited and
    /// released the last handle) or if access is denied.
    pub fn open(profile_name: &str) -> Option<Self> {
        let name = format!("Local\\actionbook-chrome-{profile_name}");
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            OpenJobObjectW(
                JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE,
                FALSE,
                wide.as_ptr(),
            )
        };
        if handle.is_null() {
            return None;
        }
        Some(Self {
            handle,
            terminate_on_drop: false,
        })
    }

    /// Return whether `pid` belongs to the named profile Job Object.
    pub fn contains_pid(profile_name: &str, pid: u32) -> bool {
        let Some(job) = Self::open(profile_name) else {
            return false;
        };
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
        if process.is_null() {
            return false;
        }
        let mut in_job = FALSE;
        let result = unsafe { IsProcessInJob(process, job.handle, &mut in_job) != FALSE };
        unsafe { CloseHandle(process) };
        result && in_job != FALSE
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
        unsafe {
            if self.terminate_on_drop {
                // Created jobs own their process tree and retain the existing
                // last-resort cleanup behavior. Reopened verification handles
                // never terminate merely because an inspection scope ended.
                TerminateJobObject(self.handle, 1);
            }
            CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
fn inspect_process(pid: u32) -> Result<Option<ProcessIdentity>, String> {
    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        // ERROR_INVALID_PARAMETER means the PID does not exist.
        if error.raw_os_error() == Some(87) {
            return Ok(None);
        }
        return Err(format!("OpenProcess failed: {error}"));
    }

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    if unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) } == FALSE
    {
        let error = std::io::Error::last_os_error();
        unsafe { CloseHandle(handle) };
        return Err(format!("GetProcessTimes failed: {error}"));
    }
    let mut path = vec![0u16; 32_768];
    let mut length = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut length) } == FALSE {
        let error = std::io::Error::last_os_error();
        unsafe { CloseHandle(handle) };
        return Err(format!("QueryFullProcessImageNameW failed: {error}"));
    }
    unsafe { CloseHandle(handle) };
    path.truncate(length as usize);
    let command_line = String::from_utf16(&path)
        .map_err(|error| format!("invalid process image path: {error}"))?;
    let start_identity =
        ((creation.dwHighDateTime as u64) << 32 | creation.dwLowDateTime as u64).to_string();
    Ok(Some(ProcessIdentity {
        start_identity,
        command_line,
    }))
}

#[cfg(windows)]
fn processes_using_profile(_profile_dir: &Path) -> Result<Vec<u32>, OwnershipError> {
    // Windows ownership is established by the named profile Job Object and the
    // recorded root PID identity rather than process command-line enumeration.
    Ok(Vec::new())
}

#[cfg(windows)]
fn terminate_owned_process_tree(record: &ChromeOwnership) -> Result<(), OwnershipError> {
    let Some(job) = ChromeJobObject::open(&record.profile) else {
        return Err(OwnershipError::Unverified {
            pid: record.pid,
            reason: "named Actionbook profile Job Object is missing".to_string(),
        });
    };
    if !ChromeJobObject::contains_pid(&record.profile, record.pid) {
        return Err(OwnershipError::Unverified {
            pid: record.pid,
            reason: "recorded PID is not in the named profile Job Object".to_string(),
        });
    }
    if !job.terminate() {
        return Err(OwnershipError::StopFailed {
            pid: record.pid,
            reason: std::io::Error::last_os_error().to_string(),
        });
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if inspect_process(record.pid)
            .map_err(|reason| OwnershipError::StopFailed {
                pid: record.pid,
                reason,
            })?
            .is_none()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(OwnershipError::StopFailed {
        pid: record.pid,
        reason: "Job Object process tree did not terminate within 5 seconds".to_string(),
    })
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
    fn stale_pid_identity_never_authorizes_process_termination() {
        let mut child = spawn_sleeper();
        let identity = inspect_process(child.id())
            .expect("inspect sleeper")
            .expect("sleeper alive");
        let ownership = ChromeOwnership {
            version: OWNERSHIP_VERSION,
            session_id: "stale-session".to_string(),
            profile: "stale-profile".to_string(),
            profile_dir: "/tmp/stale-profile".to_string(),
            pid: child.id(),
            process_start_identity: format!("wrong-{}", identity.start_identity),
            command_line: identity.command_line,
        };

        assert!(
            matches!(
                verify_owned_process(&ownership),
                Err(OwnershipError::Unverified { .. })
            ),
            "reused PID identity must be rejected"
        );
        assert!(
            is_process_alive(child.id()),
            "verification must never kill an unrelated process"
        );
        let _ = child.kill();
        let _ = child.wait();
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
