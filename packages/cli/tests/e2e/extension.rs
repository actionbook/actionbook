//! E2E tests for `actionbook extension` commands.

use std::fs;
use std::path::{Path, PathBuf};

use crate::harness::{SoloEnv, assert_success, parse_json, skip};

fn extension_dir(env: &SoloEnv) -> PathBuf {
    Path::new(&env.actionbook_home).join("extension")
}

fn extension_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../actionbook-extension")
        .canonicalize()
        .expect("extension fixture dir")
}

fn extension_fixture_version() -> String {
    let manifest_path = extension_fixture_dir().join("manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read extension fixture manifest"))
            .expect("parse extension fixture manifest");
    manifest["version"]
        .as_str()
        .expect("extension fixture manifest version")
        .to_string()
}

fn reserve_unused_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

#[test]
fn extension_path_outputs_path_field() {
    if skip() {
        return;
    }

    let env = SoloEnv::new();
    let out = env.headless_json(&["extension", "path"], 10);
    assert_success(&out, "extension path");
    let v = parse_json(&out);

    assert_eq!(v["command"], "extension path");
    assert_eq!(
        v["data"]["path"],
        extension_dir(&env).to_string_lossy().to_string()
    );
    assert_eq!(v["data"]["installed"], false);
    assert!(v["data"]["version"].is_null());
}

#[test]
fn extension_install_force_and_uninstall_round_trip() {
    if skip() {
        return;
    }

    let env = SoloEnv::new();
    let expected_path = extension_dir(&env);
    let expected_path_str = expected_path.to_string_lossy().to_string();
    let expected_version = extension_fixture_version();
    let fixture_dir = extension_fixture_dir();
    let fixture_dir_str = fixture_dir.to_string_lossy().to_string();

    let install = env.headless_json_with_env(
        &["extension", "install", "--force"],
        &[(
            "ACTIONBOOK_EXTENSION_TEST_SOURCE_DIR",
            fixture_dir_str.as_str(),
        )],
        30,
    );
    assert_success(&install, "extension install --force");
    let install_v = parse_json(&install);
    assert_eq!(install_v["command"], "extension install");
    assert_eq!(install_v["data"]["path"], expected_path_str);
    assert_eq!(install_v["data"]["version"], expected_version);
    assert!(
        expected_path.join("manifest.json").exists(),
        "installed extension should include manifest.json"
    );

    let path = env.headless_json(&["extension", "path"], 10);
    assert_success(&path, "extension path after install");
    let path_v = parse_json(&path);
    assert_eq!(path_v["data"]["path"], expected_path_str);
    assert_eq!(path_v["data"]["installed"], true);
    assert_eq!(path_v["data"]["version"], expected_version);

    let uninstall = env.headless_json(&["extension", "uninstall"], 10);
    assert_success(&uninstall, "extension uninstall");
    let uninstall_v = parse_json(&uninstall);
    assert_eq!(uninstall_v["command"], "extension uninstall");
    assert_eq!(uninstall_v["data"]["uninstalled"], true);
    assert!(
        !expected_path.exists(),
        "extension directory should be removed after uninstall"
    );
}

#[test]
fn extension_status_with_running_daemon_does_not_crash() {
    if skip() {
        return;
    }

    let env = SoloEnv::new();
    let start = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "about:blank",
        ],
        30,
    );
    assert_success(&start, "warm daemon for extension status");

    let out = env.headless_json(&["extension", "status"], 10);
    assert_success(&out, "extension status with daemon running");
    let v = parse_json(&out);

    assert_eq!(v["command"], "extension status");
    let bridge = v["data"]["bridge"].as_str().expect("bridge state string");
    assert!(
        matches!(bridge, "listening" | "not_listening" | "failed"),
        "unexpected bridge state: {bridge}"
    );
    assert!(
        v["data"]["extension_connected"].is_boolean(),
        "extension_connected must be boolean"
    );
}

#[test]
fn extension_ping_returns_bridge_not_listening() {
    if skip() {
        return;
    }

    let env = SoloEnv::new();
    let port = reserve_unused_port().to_string();
    let out = env.headless_json_with_env(
        &["extension", "ping"],
        &[("ACTIONBOOK_EXTENSION_BRIDGE_PORT", port.as_str())],
        10,
    );
    assert_success(&out, "extension ping on unused bridge port");
    let v = parse_json(&out);

    assert_eq!(v["command"], "extension ping");
    assert_eq!(v["data"]["bridge"], "not_listening");
    assert!(
        v["data"]["rtt_ms"].is_null() || v["data"]["rtt_ms"].is_number(),
        "rtt_ms must be null or number when bridge is not listening"
    );
}
