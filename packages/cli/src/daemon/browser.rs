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
