use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::time::Duration;

use crate::error::CliError;

/// Find Chrome executable.
pub fn find_chrome() -> Result<String, CliError> {
    #[cfg(not(windows))]
    let candidates: &[&str] = &[
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ];
    #[cfg(windows)]
    let candidates: &[&str] = &[
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        "chrome.exe",
        "chrome",
    ];

    // Check LOCALAPPDATA on Windows (per-user install).
    #[cfg(windows)]
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let path = format!(r"{local}\Google\Chrome\Application\chrome.exe");
        if std::path::Path::new(&path).exists() {
            return Ok(path);
        }
    }

    for c in candidates {
        if std::path::Path::new(c).exists() {
            return Ok(c.to_string());
        }
        #[cfg(not(windows))]
        let which_cmd = "which";
        #[cfg(windows)]
        let which_cmd = "where";
        if let Ok(output) = std::process::Command::new(which_cmd).arg(c).output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }
    Err(CliError::BrowserNotFound)
}

/// Launch Chrome with CDP enabled.
/// Returns (Child, actual_cdp_port).
/// Uses --remote-debugging-port=0 so Chrome picks a free port itself,
/// then reads the actual port from stderr ("DevTools listening on ws://...").
pub async fn launch_chrome(
    executable: &str,
    headless: bool,
    user_data_dir: &str,
    open_url: Option<&str>,
    stealth: bool,
) -> Result<(Child, u16), CliError> {
    let mut args = vec![
        "--remote-debugging-port=0".to_string(),
        format!("--user-data-dir={user_data_dir}"),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
    ];
    if stealth {
        // Stealth launch args — based on actionbook-rs + Camoufox patterns.
        //
        // NOTE: --disable-blink-features=AutomationControlled intentionally omitted.
        // It triggers Chrome's "unsupported command line flag" warning bar which
        // is itself a detection signal. navigator.webdriver is hidden via CDP
        // injection (Page.addScriptToEvaluateOnNewDocument) instead.

        // WebRTC IP leak prevention
        args.push("--force-webrtc-ip-handling-policy=disable_non_proxied_udp".to_string());

        // NOTE: --disable-site-isolation-trials and --disable-features=IsolateOrigins
        // intentionally omitted — they trigger Chrome's "unsupported command line flag"
        // warning bar, which is itself a bot detection signal.

        // Stability & clean UI
        args.push("--disable-dev-shm-usage".to_string());
        args.push("--disable-save-password-bubble".to_string());
        args.push("--disable-translate".to_string());
        args.push("--disable-background-timer-throttling".to_string());
        args.push("--disable-backgrounding-occluded-windows".to_string());
    }
    if headless {
        args.push("--headless=new".to_string());
    }
    // open_url is NOT passed as a Chrome launch arg — Chrome starts on about:blank.
    // The caller navigates after attach() so the stealth script is already injected.
    let _ = open_url;

    let exe = executable.to_string();
    // Spawn Chrome and read stderr in a blocking thread to avoid blocking tokio

    tokio::task::spawn_blocking(move || -> Result<(Child, u16), CliError> {
        let mut child = std::process::Command::new(&exe)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CliError::BrowserLaunchFailed(e.to_string()))?;

        let stderr = child.stderr.take().ok_or_else(|| {
            CliError::BrowserLaunchFailed("failed to capture Chrome stderr".to_string())
        })?;

        // Read stderr to find "DevTools listening on ws://HOST:PORT/..."
        let (tx, rx) = std::sync::mpsc::channel::<u16>();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                if line.contains("DevTools listening on")
                    && let Some(ws_start) = line.find("ws://")
                {
                    let after_ws = &line[ws_start + 5..];
                    if let Some(colon) = after_ws.find(':') {
                        let after_colon = &after_ws[colon + 1..];
                        let port_str: String = after_colon
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect();
                        if let Ok(p) = port_str.parse::<u16>() {
                            let _ = tx.send(p);
                            return;
                        }
                    }
                }
            }
        });

        let port = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| {
                crate::daemon::chrome_reaper::kill_and_reap(&mut child);
                CliError::CdpConnectionFailed(
                    "Chrome did not print DevTools listening URL within 30s".to_string(),
                )
            })?;

        Ok((child, port))
    })
    .await
    .map_err(|e| CliError::Internal(format!("spawn_blocking failed: {e}")))?
}

/// Discover the WebSocket debugger URL from Chrome's /json/version endpoint.
pub async fn discover_ws_url(port: u16) -> Result<String, CliError> {
    discover_ws_url_from_base(&format!("http://127.0.0.1:{port}")).await
}

pub async fn discover_ws_url_from_base(base_url: &str) -> Result<String, CliError> {
    let url = format!("{}/json/version", base_url.trim_end_matches('/'));

    // Up to 30 seconds (150 × 200ms)
    for attempt in 0..150 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        match reqwest::get(&url).await {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>().await
                    && let Some(ws) = json.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
                {
                    return Ok(ws.to_string());
                }
            }
            Err(_) => continue,
        }
    }
    Err(CliError::CdpConnectionFailed(format!(
        "Chrome did not expose CDP at {base_url} within 30s"
    )))
}

/// Get list of targets (tabs) from Chrome.
pub async fn list_targets(port: u16) -> Result<Vec<serde_json::Value>, CliError> {
    list_targets_from_base(&format!("http://127.0.0.1:{port}")).await
}

pub async fn list_targets_from_base(base_url: &str) -> Result<Vec<serde_json::Value>, CliError> {
    let url = format!("{}/json/list", base_url.trim_end_matches('/'));
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;
    let targets: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;
    Ok(targets
        .into_iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
        .collect())
}

pub async fn resolve_cdp_endpoint(endpoint: &str) -> Result<(String, u16), CliError> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return Err(CliError::InvalidArgument(
            "cdp endpoint cannot be empty".to_string(),
        ));
    }

    if let Ok(port) = trimmed.parse::<u16>() {
        let ws_url = discover_ws_url(port).await?;
        return Ok((ws_url, port));
    }

    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        let port = parse_endpoint_port(trimmed)?;
        return Ok((trimmed.to_string(), port));
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let port = parse_endpoint_port(trimmed)?;
        let origin = endpoint_origin(trimmed)?;
        let ws_url = discover_ws_url_from_base(&origin).await?;
        return Ok((ws_url, port));
    }

    Err(CliError::InvalidArgument(format!(
        "unsupported cdp endpoint: {trimmed}"
    )))
}

fn endpoint_origin(endpoint: &str) -> Result<String, CliError> {
    let scheme_end = endpoint
        .find("://")
        .ok_or_else(|| CliError::InvalidArgument(format!("invalid endpoint: {endpoint}")))?;
    let after_scheme = &endpoint[scheme_end + 3..];
    let authority = after_scheme
        .split('/')
        .next()
        .ok_or_else(|| CliError::InvalidArgument(format!("invalid endpoint: {endpoint}")))?;
    if authority.is_empty() {
        return Err(CliError::InvalidArgument(format!(
            "invalid endpoint: {endpoint}"
        )));
    }
    Ok(format!("{}://{}", &endpoint[..scheme_end], authority))
}

fn parse_endpoint_port(endpoint: &str) -> Result<u16, CliError> {
    let scheme_end = endpoint
        .find("://")
        .ok_or_else(|| CliError::InvalidArgument(format!("invalid endpoint: {endpoint}")))?;
    let after_scheme = &endpoint[scheme_end + 3..];
    let authority = after_scheme
        .split('/')
        .next()
        .ok_or_else(|| CliError::InvalidArgument(format!("invalid endpoint: {endpoint}")))?;
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let port_str = host_port
        .rsplit_once(':')
        .map(|(_, port)| port)
        .ok_or_else(|| CliError::InvalidArgument(format!("endpoint missing port: {endpoint}")))?;
    port_str.parse::<u16>().map_err(|_| {
        CliError::InvalidArgument(format!("invalid endpoint port in {endpoint}: {port_str}"))
    })
}

// ── Auto-connect: discover a locally running Chrome ───────────────────────────

/// Parse the port number from a `DevToolsActivePort` file's content.
///
/// The file's first line is the port number; the second line is a hash path.
/// Returns `None` if the content is empty, non-numeric, or malformed.
pub fn parse_devtools_active_port(content: &str) -> Option<u16> {
    content.lines().next()?.trim().parse::<u16>().ok()
}

/// Return the platform-specific candidate paths for Chrome's `DevToolsActivePort` file.
///
/// `os` matches `std::env::consts::OS` values ("macos", "linux", "windows").
/// `home` is the user's home directory. `local_appdata` is `%LOCALAPPDATA%` on Windows.
pub fn devtools_active_port_candidates_for(
    os: &str,
    home: &std::path::Path,
    local_appdata: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    match os {
        "macos" => vec![home.join("Library/Application Support/Google/Chrome/DevToolsActivePort")],
        "linux" => vec![home.join(".config/google-chrome/DevToolsActivePort")],
        "windows" => local_appdata
            .map(|p| {
                // Use explicit Windows path separators so the result is correct
                // on all host platforms (PathBuf::join uses the host separator).
                let base = p.to_string_lossy();
                vec![std::path::PathBuf::from(format!(
                    "{}\\Google\\Chrome\\User Data\\DevToolsActivePort",
                    base
                ))]
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// Probe a single port's `/json/version` endpoint with a quick timeout.
async fn probe_json_version(port: u16, timeout: Duration) -> Option<String> {
    let url = format!("http://127.0.0.1:{port}/json/version");
    let result = tokio::time::timeout(timeout, reqwest::get(&url)).await;
    match result {
        Ok(Ok(resp)) if resp.status().is_success() => {
            resp.json::<serde_json::Value>().await.ok().and_then(|j| {
                j.get("webSocketDebuggerUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        }
        _ => None,
    }
}

/// Discover a running Chrome by checking `DevToolsActivePort` file candidates and
/// then probing `probe_ports` directly.
///
/// Returns `(ws_url, port)` for the first reachable instance.
///
/// Error codes:
/// - `CHROME_CDP_UNREACHABLE`: a `DevToolsActivePort` file was found but the
///   indicated port is unreachable (Chrome may have crashed, leaving a stale file).
/// - `CHROME_AUTO_CONNECT_NOT_FOUND`: no file found and no probe port is listening
///   (Chrome is not running or was not started with `--remote-debugging-port`).
pub async fn auto_discover_chrome_from_candidates(
    candidates: &[std::path::PathBuf],
    probe_ports: &[u16],
    timeout: Duration,
) -> Result<(String, u16), crate::error::CliError> {
    let mut found_active_port_file = false;

    // Phase 1: DevToolsActivePort file candidates (Chrome writes this on startup).
    for path in candidates {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let port = match parse_devtools_active_port(&content) {
            Some(p) => p,
            None => continue,
        };
        found_active_port_file = true;
        if let Some(ws_url) = probe_json_version(port, timeout).await {
            return Ok((ws_url, port));
        }
        // Stale file — fall through to probe_ports below.
    }

    // Phase 2: probe well-known ports.
    for &port in probe_ports {
        if let Some(ws_url) = probe_json_version(port, timeout).await {
            return Ok((ws_url, port));
        }
    }

    if found_active_port_file {
        Err(crate::error::CliError::ChromeCdpUnreachable(
            "DevToolsActivePort file found but Chrome is unreachable on the indicated port"
                .to_string(),
        ))
    } else {
        Err(crate::error::CliError::ChromeAutoConnectNotFound)
    }
}

/// Discover a running Chrome using platform-default paths and ports [9222, 9229].
pub async fn auto_discover_chrome() -> Result<(String, u16), crate::error::CliError> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let local_appdata = dirs::data_local_dir();
    let candidates =
        devtools_active_port_candidates_for(std::env::consts::OS, &home, local_appdata.as_deref());
    auto_discover_chrome_from_candidates(&candidates, &[9222, 9229], Duration::from_millis(500))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn parse_devtools_active_port_accepts_first_numeric_line() {
        assert_eq!(
            parse_devtools_active_port("9222\n/devtools/browser/abc123\n"),
            Some(9222)
        );
    }

    #[test]
    fn parse_devtools_active_port_rejects_empty_content() {
        assert_eq!(parse_devtools_active_port(""), None);
        assert_eq!(parse_devtools_active_port("\n"), None);
    }

    #[test]
    fn parse_devtools_active_port_rejects_junk_content() {
        assert_eq!(
            parse_devtools_active_port("/devtools/browser/abc123\n"),
            None
        );
    }

    #[test]
    fn parse_devtools_active_port_rejects_non_numeric_port() {
        assert_eq!(parse_devtools_active_port("not-a-port\nhash\n"), None);
    }

    #[test]
    fn devtools_active_port_candidates_cover_default_platform_paths() {
        let mac = devtools_active_port_candidates_for(
            "macos",
            Path::new("/Users/alice"),
            Some(Path::new("/Users/alice/AppData/Local")),
        );
        assert_eq!(
            mac,
            vec![PathBuf::from(
                "/Users/alice/Library/Application Support/Google/Chrome/DevToolsActivePort",
            )]
        );

        let linux = devtools_active_port_candidates_for(
            "linux",
            Path::new("/home/alice"),
            Some(Path::new("/home/alice/.local/share")),
        );
        assert_eq!(
            linux,
            vec![PathBuf::from(
                "/home/alice/.config/google-chrome/DevToolsActivePort",
            )]
        );

        let windows = devtools_active_port_candidates_for(
            "windows",
            Path::new("C:\\Users\\Alice"),
            Some(Path::new("C:\\Users\\Alice\\AppData\\Local")),
        );
        assert_eq!(
            windows,
            vec![PathBuf::from(
                "C:\\Users\\Alice\\AppData\\Local\\Google\\Chrome\\User Data\\DevToolsActivePort",
            )]
        );
    }

    enum MockVersionBehavior {
        Ok { ws_url: &'static str },
        Status500,
        Timeout,
    }

    fn reserve_unused_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("bind ephemeral port")
            .local_addr()
            .expect("local addr")
            .port()
    }

    fn spawn_json_version_server(behavior: MockVersionBehavior) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local addr").port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept /json/version request");
            match behavior {
                MockVersionBehavior::Ok { ws_url } => {
                    let mut req_buf = [0_u8; 1024];
                    let _ = stream.read(&mut req_buf);
                    let body = serde_json::json!({
                        "Browser": "Chrome/136.0.0.0",
                        "webSocketDebuggerUrl": ws_url,
                    })
                    .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write 200 response");
                }
                MockVersionBehavior::Status500 => {
                    let mut req_buf = [0_u8; 1024];
                    let _ = stream.read(&mut req_buf);
                    stream
                        .write_all(
                            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .expect("write 500 response");
                }
                MockVersionBehavior::Timeout => {
                    thread::sleep(Duration::from_millis(250));
                }
            }
        });
        (port, handle)
    }

    #[tokio::test]
    async fn auto_discover_uses_json_version_probe_when_available() {
        let (port, server) = spawn_json_version_server(MockVersionBehavior::Ok {
            ws_url: "ws://127.0.0.1:9222/devtools/browser/mock-browser-id",
        });

        let result = auto_discover_chrome_from_candidates(&[], &[port], Duration::from_millis(50))
            .await
            .expect("auto-discover should succeed from /json/version");
        assert_eq!(
            result,
            (
                "ws://127.0.0.1:9222/devtools/browser/mock-browser-id".to_string(),
                port,
            )
        );

        server.join().expect("join mock server");
    }

    #[tokio::test]
    async fn auto_discover_falls_through_http_500_and_timeout_to_next_probe() {
        let (port_500, server_500) = spawn_json_version_server(MockVersionBehavior::Status500);
        let (port_timeout, server_timeout) =
            spawn_json_version_server(MockVersionBehavior::Timeout);
        let (port_ok, server_ok) = spawn_json_version_server(MockVersionBehavior::Ok {
            ws_url: "ws://127.0.0.1:9229/devtools/browser/fallback-id",
        });

        let result = auto_discover_chrome_from_candidates(
            &[],
            &[port_500, port_timeout, port_ok],
            Duration::from_millis(50),
        )
        .await
        .expect("auto-discover should fall through to a later successful probe");
        assert_eq!(
            result,
            (
                "ws://127.0.0.1:9229/devtools/browser/fallback-id".to_string(),
                port_ok,
            )
        );

        server_500.join().expect("join 500 mock server");
        server_timeout.join().expect("join timeout mock server");
        server_ok.join().expect("join success mock server");
    }

    #[tokio::test]
    async fn auto_discover_returns_not_found_when_no_candidates_work() {
        let port_a = reserve_unused_port();
        let port_b = reserve_unused_port();

        let err =
            auto_discover_chrome_from_candidates(&[], &[port_a, port_b], Duration::from_millis(50))
                .await
                .expect_err("auto-discover should fail when no Chrome candidates are reachable");

        assert_eq!(err.error_code(), "CHROME_AUTO_CONNECT_NOT_FOUND");
    }

    #[tokio::test]
    async fn auto_discover_stale_devtools_active_port_falls_through_then_reports_unreachable() {
        let temp = tempdir().expect("tempdir");
        let stale_port = reserve_unused_port();
        let path = temp.path().join("DevToolsActivePort");
        std::fs::write(&path, format!("{stale_port}\n/devtools/browser/stale\n"))
            .expect("write stale DevToolsActivePort");

        let fallback_a = reserve_unused_port();
        let fallback_b = reserve_unused_port();

        let err = auto_discover_chrome_from_candidates(
            &[path],
            &[fallback_a, fallback_b],
            Duration::from_millis(50),
        )
        .await
        .expect_err("stale DevToolsActivePort should fall through, then fail as unreachable");

        assert_eq!(err.error_code(), "CHROME_CDP_UNREACHABLE");
    }
}
