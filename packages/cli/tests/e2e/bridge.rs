//! E2E tests for extension bridge lazy startup, port-holder diagnosis, and
//! the `actionbook daemon restart` recovery path.
//!
//! Bridge port 19222 is process-global and not isolatable per-test. These
//! tests therefore:
//!   - Use `SoloEnv` so each test owns its daemon (cleanly killed on Drop).
//!   - Hold `BRIDGE_PORT_LOCK` across tests in this file to serialize against
//!     each other (the global e2e harness already runs with --test-threads=1,
//!     but the lock makes the dependency explicit and protects against future
//!     parallelization).
//!   - Skip immediately if `lsof` reveals 19222 is occupied at test entry by
//!     a process outside our control (e.g. a developer's user-level daemon).
//!
//! Note: extension-mode tests don't have a real Chrome extension to handshake
//! with, so they intentionally let the call fail with `EXTENSION_NOT_CONNECTED`
//! after the bridge bind step. We assert on what happened to the bridge listener
//! (port bound? error code? holder hint?), not on session establishment.
//!
//! Unix-only: port-holder identification uses `lsof`. On Windows the equivalent
//! lookup needs `netstat` parsing or netstat2; until that's wired the assertions
//! "is 19222 free?" and "who holds it?" silently pass, masking regressions —
//! so the whole module is gated to Unix.

#![cfg(unix)]

use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::harness::{SoloEnv, parse_json, skip, stderr_str, stdout_str};

/// Serialize bridge port 19222 contention across this file's tests.
static BRIDGE_PORT_LOCK: Mutex<()> = Mutex::new(());

const BRIDGE_PORT: u16 = 19222;

/// True if 19222 is bound by some external process at this moment.
/// Returns `None` when port is free.
fn port_holder_pid() -> Option<u32> {
    // Best-effort lsof shell-out; harness lives outside of CLI source so we
    // can't reuse the production diagnostic.
    let out = std::process::Command::new("lsof")
        .args(["-tiTCP:19222", "-sTCP:LISTEN"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let pid_str = stdout.trim().lines().next()?.trim();
    pid_str.parse().ok()
}

/// Skip a test if 19222 is held by a process we don't control. Returns the
/// guard so the caller can early-return.
fn ensure_port_free_or_skip() -> bool {
    if let Some(pid) = port_holder_pid() {
        eprintln!(
            "bridge e2e: port 19222 already held by pid {pid} (likely user's daemon) — skipping"
        );
        return true;
    }
    false
}

/// Wait up to `max` for the daemon at `home` to write its pid file and have
/// the process actually running.
fn wait_for_daemon_up(env: &SoloEnv, max: Duration) -> Option<u32> {
    let pid_path = std::path::Path::new(&env.actionbook_home).join("daemon.pid");
    let start = Instant::now();
    while start.elapsed() < max {
        if let Ok(s) = std::fs::read_to_string(&pid_path)
            && let Ok(pid) = s.trim().parse::<u32>()
            && pid_alive(pid)
        {
            return Some(pid);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ===========================================================================
// 1. lazy_bridge_not_started_on_daemon_boot
//    Cold daemon must not bind 19222 just because it started — only the first
//    `--mode extension` call should trigger ensure_bridge.
// ===========================================================================

#[test]
fn lazy_bridge_not_started_on_daemon_boot() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    if ensure_port_free_or_skip() {
        return;
    }
    let env = SoloEnv::new();

    // `list-sessions` triggers daemon auto-start without touching extension.
    let out = env.headless_json(&["browser", "list-sessions"], 10);
    assert!(
        out.status.success(),
        "list-sessions failed: stderr={}",
        stderr_str(&out)
    );
    assert!(
        wait_for_daemon_up(&env, Duration::from_secs(5)).is_some(),
        "daemon did not come up"
    );

    // Give the daemon a beat in case ensure-style background tasks land late.
    std::thread::sleep(Duration::from_millis(500));

    let holder = port_holder_pid();
    assert!(
        holder.is_none(),
        "lazy bridge violated: port 19222 held by pid {holder:?} after non-extension call"
    );
}

// ===========================================================================
// 2. ensure_on_first_extension_call_binds
//    A `browser start --mode extension` must bind 19222 (even if the
//    extension handshake itself fails because no Chrome is running).
// ===========================================================================

#[test]
fn ensure_on_first_extension_call_binds() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    if ensure_port_free_or_skip() {
        return;
    }
    let env = SoloEnv::new();

    // Warm up the daemon with a non-extension call so we can verify the lazy
    // contract holds at the daemon-up moment.
    let out = env.headless_json(&["browser", "list-sessions"], 10);
    assert!(out.status.success(), "warm-up failed");
    let daemon_pid = wait_for_daemon_up(&env, Duration::from_secs(5)).expect("daemon up");
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        port_holder_pid().is_none(),
        "lazy contract pre-condition violated: 19222 was already held before any extension call"
    );

    // Extension start will likely end with EXTENSION_NOT_CONNECTED (no real
    // Chrome), but the bridge bind side-effect is what we're verifying.
    let _ = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "extension",
            "--set-session-id",
            "ext-bind",
        ],
        20,
    );

    // Daemon must still be up (bridge failure should not crash daemon).
    assert!(pid_alive(daemon_pid), "daemon died after extension start");
    let holder = port_holder_pid();
    assert_eq!(
        holder,
        Some(daemon_pid),
        "after extension call, 19222 must be held by this test's daemon (got holder={holder:?}, daemon={daemon_pid})"
    );
}

// ===========================================================================
// 3. ensure_retries_within_window
//    With 19222 occupied for ~4s, the first extension call must wait through
//    bind retries and then succeed acquiring the port.
// ===========================================================================

#[test]
fn ensure_retries_within_window() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    if ensure_port_free_or_skip() {
        return;
    }
    let env = SoloEnv::new();

    // Hold the port from a background thread for 4s, then release.
    let blocker = TcpListener::bind(("127.0.0.1", BRIDGE_PORT))
        .expect("test must bind 19222 to set up scenario");
    let blocker_handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        drop(blocker);
    });

    let started = Instant::now();
    let _out = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "extension",
            "--set-session-id",
            "ext-retry",
        ],
        25,
    );
    let elapsed = started.elapsed();
    blocker_handle.join().unwrap();

    // Bind retry ladder is 100/500/1000/2000/5000ms (≈8.6s window). Hog
    // released at ~4s → bind should succeed on attempt 4 or 5 (T≈3.6s);
    // the call also includes the 5s extension-handshake poll afterwards.
    // Lower bound 3s = retries actually waited; upper bound 22s = scenario
    // ran within the harness timeout (25s) and didn't burn the full window.
    assert!(
        elapsed >= Duration::from_secs(3),
        "extension call returned too early ({elapsed:?}) — retries skipped?"
    );
    assert!(
        elapsed < Duration::from_secs(22),
        "extension call took too long ({elapsed:?}) — retry ladder is broken or extended past spec"
    );

    // After the retries the daemon must end up holding 19222.
    let holder = port_holder_pid();
    let daemon_pid = wait_for_daemon_up(&env, Duration::from_secs(1));
    assert_eq!(
        holder, daemon_pid,
        "after retries, 19222 must be held by daemon — got holder={holder:?}, daemon={daemon_pid:?}"
    );
}

// ===========================================================================
// 4. ensure_surfaces_holder_after_retries_exhausted
//    Hog 19222 for >9s so retries exhaust. The error must include the
//    holder's pid (and ideally command) so the user knows what to stop.
// ===========================================================================

#[test]
fn ensure_surfaces_holder_after_retries_exhausted() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    if ensure_port_free_or_skip() {
        return;
    }
    let env = SoloEnv::new();

    // Hold for the full retry window plus margin.
    let blocker = TcpListener::bind(("127.0.0.1", BRIDGE_PORT))
        .expect("test must bind 19222 to set up scenario");
    let test_pid = std::process::id();

    let out = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "extension",
            "--set-session-id",
            "ext-fail",
        ],
        20,
    );

    // Release after the call returns to keep cleanup tidy.
    drop(blocker);

    // Output is a JSON envelope; error code must be BRIDGE_BIND_FAILED and
    // hint must mention this test's pid (the real holder).
    let v = parse_json(&out);
    let code = v["error"]["code"].as_str().unwrap_or("");
    assert_eq!(
        code,
        "BRIDGE_BIND_FAILED",
        "expected BRIDGE_BIND_FAILED, got code={code}; full body={}",
        stdout_str(&out)
    );
    // Holder pid must surface in user-facing output. Accept either error
    // message (the BridgeError::Display content) or hint (the actionable
    // guidance) — Phase 3 chooses where to put it, but it must appear
    // *somewhere* the user can read.
    let message = v["error"]["message"].as_str().unwrap_or("");
    let hint = v["error"]["hint"].as_str().unwrap_or("");
    let pid_str = test_pid.to_string();
    let combined = format!("{message} | {hint}");
    assert!(
        combined.contains(&pid_str),
        "error envelope must reference holder pid {test_pid} — got message={message:?} hint={hint:?}"
    );
}

// ===========================================================================
// 5. daemon_restart_kills_and_allows_auto_respawn
//    `actionbook daemon restart` exits the running daemon. A subsequent CLI
//    call auto-spawns a fresh daemon with a different pid.
// ===========================================================================

#[test]
fn daemon_restart_kills_and_allows_auto_respawn() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    if ensure_port_free_or_skip() {
        return;
    }
    let env = SoloEnv::new();

    // Bring up an initial daemon.
    let out = env.headless_json(&["browser", "list-sessions"], 10);
    assert!(out.status.success(), "warm-up list-sessions failed");
    let pid_before = wait_for_daemon_up(&env, Duration::from_secs(5)).expect("daemon up");

    // `daemon restart` is the new subcommand we want to introduce.
    let out = env.headless(&["daemon", "restart"], 15);
    assert!(
        out.status.success(),
        "daemon restart failed (binary may not yet expose the subcommand): stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    // Old pid must be gone within a short grace window.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) && pid_alive(pid_before) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !pid_alive(pid_before),
        "old daemon pid {pid_before} still alive after restart"
    );

    // Next CLI call must auto-respawn a daemon with a different pid.
    let out = env.headless_json(&["browser", "list-sessions"], 10);
    assert!(
        out.status.success(),
        "post-restart list-sessions failed: stderr={}",
        stderr_str(&out)
    );
    let pid_after = wait_for_daemon_up(&env, Duration::from_secs(5)).expect("daemon respawned");
    assert_ne!(
        pid_after, pid_before,
        "daemon pid did not change after restart (before={pid_before}, after={pid_after})"
    );
}

// ===========================================================================
// 6. daemon_restart_recovers_bridge_after_failed
//    After bridge has been forced into Failed state by a long port hog,
//    `daemon restart` releases the daemon-level resources and the next
//    extension call succeeds binding 19222.
// ===========================================================================

#[test]
fn daemon_restart_recovers_bridge_after_failed() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    if ensure_port_free_or_skip() {
        return;
    }
    let env = SoloEnv::new();

    // Force a Failed bridge: hog 19222 across the full retry window during
    // first extension call.
    let blocker = TcpListener::bind(("127.0.0.1", BRIDGE_PORT))
        .expect("test must bind 19222 for the failure scenario");
    let _ = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "extension",
            "--set-session-id",
            "ext-pre",
        ],
        20,
    );
    drop(blocker); // release so subsequent steps can bind

    // Sanity: daemon is still up.
    let pid_before = wait_for_daemon_up(&env, Duration::from_secs(2)).expect("daemon still alive");

    // Recovery path: daemon restart.
    let out = env.headless(&["daemon", "restart"], 15);
    assert!(
        out.status.success(),
        "daemon restart failed: {}",
        stderr_str(&out)
    );
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) && pid_alive(pid_before) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!pid_alive(pid_before), "old daemon did not exit");

    // After restart, a fresh extension call must successfully bind 19222 —
    // and crucially, the daemon holding 19222 must be a NEW process. This
    // separates "daemon restart actually restarted the daemon" from
    // "ensure_bridge silently self-recovered" (which is covered by UT
    // ensure_recovers_from_failed; this test is specifically about the
    // user-visible daemon-restart subcommand).
    let _ = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "extension",
            "--set-session-id",
            "ext-post",
        ],
        20,
    );
    let daemon_pid_after = wait_for_daemon_up(&env, Duration::from_secs(2));
    assert!(daemon_pid_after.is_some(), "no daemon up after restart");
    let new_pid = daemon_pid_after.unwrap();
    assert_ne!(
        new_pid, pid_before,
        "daemon pid did not change — restart was a no-op (broken: pid_before={pid_before}, pid_after={new_pid})"
    );
    let holder = port_holder_pid();
    assert_eq!(
        holder,
        Some(new_pid),
        "after daemon restart, 19222 must be held by NEW daemon pid {new_pid} — got holder={holder:?}"
    );
}
