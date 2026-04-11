//! Windows daemon e2e tests — skipped on non-Windows platforms.
//!
//! These tests verify that `browser start` and `browser close` work correctly
//! using TCP localhost transport (the Windows IPC path).
//!
//! Run on Windows CI with:
//!   set RUN_E2E_TESTS=true
//!   cargo test --test e2e windows_daemon -- --test-threads=1 --nocapture

use crate::harness;

/// On Windows, the daemon must start using TCP localhost (no Unix sockets).
/// Verify that a browser session can be created and destroyed.
#[test]
#[cfg_attr(
    not(windows),
    ignore = "Windows daemon transport tests only run on Windows"
)]
fn daemon_starts_and_connects_on_windows() {
    if harness::skip() {
        return;
    }

    let env = harness::SoloEnv::new();

    let out = env.headless(&["browser", "start", "--set-session-id", "win-test"], 15);
    harness::assert_success(&out, "browser start on Windows");

    let out = env.headless(&["browser", "close", "--session", "win-test"], 10);
    harness::assert_success(&out, "browser close on Windows");
}

/// The daemon port file (`daemon.port`) must exist after starting a session on Windows.
#[test]
#[cfg_attr(
    not(windows),
    ignore = "Windows daemon transport tests only run on Windows"
)]
fn daemon_port_file_exists_after_start() {
    if harness::skip() {
        return;
    }

    let env = harness::SoloEnv::new();

    let out = env.headless(
        &["browser", "start", "--set-session-id", "win-portfile"],
        15,
    );
    harness::assert_success(&out, "browser start");

    let port_path = std::path::Path::new(&env.actionbook_home).join("daemon.port");
    assert!(
        port_path.exists(),
        "daemon.port file must exist after daemon start, path={}",
        port_path.display()
    );

    let port_str = std::fs::read_to_string(&port_path).unwrap();
    let port: u16 = port_str
        .trim()
        .parse()
        .expect("daemon.port must contain a valid port number");
    assert!(port > 0, "port must be non-zero");

    // Verify daemon actually responds on that port
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    assert!(
        std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(200)).is_ok(),
        "daemon must accept TCP connections on port {port}"
    );

    env.headless(&["browser", "close", "--session", "win-portfile"], 10);
}

/// Verify that `browser session-status` works on Windows (round-trip IPC via TCP).
#[test]
#[cfg_attr(
    not(windows),
    ignore = "Windows daemon transport tests only run on Windows"
)]
fn session_status_works_over_tcp() {
    if harness::skip() {
        return;
    }

    let env = harness::SoloEnv::new();

    env.headless(&["browser", "start", "--set-session-id", "win-status"], 15);

    let out = env.headless(&["browser", "status", "--session", "win-status"], 10);
    harness::assert_success(&out, "status over TCP");

    env.headless(&["browser", "close", "--session", "win-status"], 10);
}
