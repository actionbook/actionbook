use std::fs;
use std::path::{Path, PathBuf};

use actionbook_cli::daemon::browser::launch_chrome;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn src_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel)
}

fn read_src(rel: &str) -> String {
    fs::read_to_string(src_path(rel)).unwrap_or_else(|e| panic!("failed to read {rel}: {e}"))
}

#[cfg(unix)]
#[tokio::test]
async fn launch_chrome_adds_stealth_flags_and_omits_open_url_arg() {
    let dir = tempdir().expect("tempdir");
    let args_log = dir.path().join("args.log");
    let user_data_dir = dir.path().join("profile");
    fs::create_dir_all(&user_data_dir).expect("profile dir");

    let fake_chrome = dir.path().join("fake-chrome.sh");
    let script = format!(
        "#!/bin/sh\nprintf '%s\n' \"$@\" > \"{}\"\necho 'DevTools listening on ws://127.0.0.1:9222/devtools/browser/fake' 1>&2\nsleep 30\n",
        args_log.display()
    );
    fs::write(&fake_chrome, script).expect("write fake chrome");
    let mut perms = fs::metadata(&fake_chrome)
        .expect("stat fake chrome")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_chrome, perms).expect("chmod fake chrome");

    let (mut child, port) = launch_chrome(
        fake_chrome.to_str().expect("fake chrome path"),
        true,
        user_data_dir.to_str().expect("user data dir"),
        Some("https://example.com/stealth-check"),
        true,
    )
    .await
    .expect("launch fake chrome");

    assert_eq!(port, 9222);

    let args = fs::read_to_string(&args_log).expect("read args log");
    let argv: Vec<&str> = args.lines().collect();

    assert!(argv.contains(&"--disable-dev-shm-usage"));
    assert!(argv.contains(&"--disable-save-password-bubble"));
    assert!(argv.contains(&"--disable-translate"));
    assert!(argv.contains(&"--window-size=1920,1080"));
    assert!(argv.contains(&"--force-webrtc-ip-handling-policy=disable_non_proxied_udp"));
    assert!(
        !argv
            .iter()
            .any(|arg| arg.contains("example.com/stealth-check")),
        "open_url must not be passed as a Chrome launch arg: {argv:?}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn stealth_module_is_registered_and_contains_required_v1_evasions() {
    let browser_mod = read_src("browser/mod.rs");
    assert!(
        browser_mod.contains("pub mod stealth;"),
        "browser/mod.rs should register the stealth module"
    );

    let stealth = fs::read_to_string(src_path("browser/stealth.rs"))
        .expect("browser/stealth.rs should exist for V1");

    assert!(stealth.contains("navigator.webdriver"));

    let has_dynamic_cdc_cleanup = stealth.contains("Object.keys(window)")
        && (stealth.contains("startsWith('cdc_')") || stealth.contains("startsWith(\"cdc_\")"));
    assert!(
        has_dynamic_cdc_cleanup,
        "stealth JS should clean cdc markers dynamically"
    );

    assert!(stealth.contains("window.chrome"));
    assert!(stealth.contains("hardwareConcurrency"));
    assert!(stealth.contains("deviceMemory"));
    assert!(stealth.contains("language"));
    assert!(stealth.contains("languages"));
    assert!(stealth.contains("platform"));
    assert!(stealth.contains("navigator.plugins"));
    assert!(stealth.contains("navigator.permissions.query"));
    assert!(stealth.contains("WebGLRenderingContext"));
    assert!(stealth.contains("37445"));
    assert!(stealth.contains("37446"));
    assert!(
        stealth.contains("Navigator.prototype._s") && stealth.contains("return;"),
        "stealth JS should guard against double injection"
    );
    assert!(
        stealth.contains("format!("),
        "stealth JS should be built with format!() for dynamic WebGL values"
    );
    assert!(
        !stealth.contains("Native Client") && !stealth.contains("application/x-nacl"),
        "stealth plugins list should not expose NaCl"
    );

    // V2 evasions: Playwright/Puppeteer traces
    assert!(
        stealth.contains("__playwright"),
        "stealth JS should remove Playwright traces"
    );
    assert!(
        stealth.contains("__pw_manual"),
        "stealth JS should remove __pw_manual trace"
    );
    assert!(
        stealth.contains("__PW_inspect"),
        "stealth JS should remove __PW_inspect trace"
    );

    // V2: maxTouchPoints
    assert!(
        stealth.contains("maxTouchPoints"),
        "stealth JS should set maxTouchPoints"
    );

    // V2: screen properties
    assert!(
        stealth.contains("colorDepth"),
        "stealth JS should set screen colorDepth"
    );
    assert!(
        stealth.contains("pixelDepth"),
        "stealth JS should set screen pixelDepth"
    );

    // V2: canvas fingerprint noise
    assert!(
        stealth.contains("HTMLCanvasElement"),
        "stealth JS should add canvas fingerprint noise"
    );

    // V2: chrome.loadTimes realistic fields
    assert!(
        stealth.contains("requestTime") && stealth.contains("navigationType"),
        "chrome.loadTimes should return realistic timing fields"
    );
}

#[test]
fn attach_source_injects_page_enable_stealth_script_and_user_agent_override() {
    let source = read_src("daemon/cdp_session.rs");
    assert!(source.contains("\"Page.enable\""));
    assert!(source.contains("\"Page.addScriptToEvaluateOnNewDocument\""));
    assert!(source.contains("\"Emulation.setUserAgentOverride\""));
    assert!(source.contains("\"Emulation.setDeviceMetricsOverride\""));
}

#[test]
fn start_source_fetches_user_agent_dynamically_and_does_not_pass_open_url_to_launch() {
    let source = read_src("browser/session/start.rs");
    assert!(source.contains("\"Browser.getVersion\""));
    assert!(
        !source.contains("cmd.open_url.as_deref(),"),
        "open_url should no longer be passed directly into launch_chrome()"
    );
}

#[test]
fn goto_source_no_longer_registers_document_start_scripts() {
    let source = read_src("browser/navigation/goto.rs");
    assert!(
        !source.contains("Page.addScriptToEvaluateOnNewDocument"),
        "goto.rs should stop registering document-start scripts once attach() owns stealth injection"
    );
}

#[test]
fn start_source_exposes_stealth_flags_and_defaults_them_on() {
    let source = read_src("browser/session/start.rs");
    // Cmd struct should have a stealth field
    assert!(
        source.contains("pub stealth: bool"),
        "start.rs Cmd should have a stealth field"
    );
    // Default value function must return true
    assert!(
        source.contains("fn default_stealth() -> bool") && source.contains("true"),
        "default_stealth() should return true"
    );
    // Clap annotation for --stealth/--no-stealth with default true
    assert!(
        source.contains("default_value = \"true\""),
        "stealth arg should default to true"
    );
    // stealth is passed to launch_chrome
    assert!(
        source.contains("cmd.stealth"),
        "cmd.stealth should be used (e.g. passed to launch_chrome)"
    );
}
