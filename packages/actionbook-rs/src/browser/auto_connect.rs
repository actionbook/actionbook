//! Auto-discover and connect to a running Chrome/Chromium instance.
//!
//! Discovery strategy (first match wins):
//! 1. Read `DevToolsActivePort` files from platform-specific Chrome user data dirs
//! 2. Probe common debugging ports (9222, 9229) via HTTP `/json/version`

use std::path::PathBuf;
use std::time::Duration;

/// Result of auto-discovery: the CDP WebSocket URL and the port it was found on.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DiscoveredBrowser {
    pub ws_url: String,
    pub port: u16,
}

/// Attempt to auto-discover a running Chrome instance.
///
/// Returns the first reachable browser's WebSocket URL, or an error if none found.
#[allow(dead_code)]
pub async fn auto_discover() -> Result<DiscoveredBrowser, String> {
    // Strategy 1: Check DevToolsActivePort files
    //
    // Chrome writes this file when remote debugging is active. If present,
    // trust it and return immediately (like agent-browser). Chrome M146+
    // (`chrome://inspect` mode) rejects bare WS probe connections with 403,
    // but the full CDP client (chromiumoxide) connects fine. Probing would
    // only add latency and false negatives.
    for dir in chrome_user_data_dirs() {
        if let Some((port, ws_path)) = read_devtools_active_port(&dir) {
            tracing::debug!(
                "DevToolsActivePort found in {:?}: port={}, ws_path={}",
                dir,
                port,
                ws_path
            );

            // Try HTTP /json/version first — gives us the real webSocketDebuggerUrl
            // (pre-M144 Chrome, or Chrome launched with --remote-debugging-port).
            match discover_via_http(port).await {
                Ok(browser) => return Ok(browser),
                Err(DiscoverError::NoCdpHttpApi(reason)) => {
                    // Port is alive but rejects external tools (chrome://inspect
                    // mode or Electron). Skip this directory entirely.
                    tracing::debug!("Skipping DevToolsActivePort: {}", reason);
                    continue;
                }
                Err(DiscoverError::Transient(reason)) => {
                    // HTTP /json/version failed — could be stale file, transient issue,
                    // or a non-browser service occupying this port.
                    // Try /json (page list) as a second validation: a real Chrome with
                    // debugging always exposes this. If it also fails, skip — Strategy 2
                    // port scanning will still find the browser if it has any working
                    // HTTP endpoint on 9222-9229.
                    if is_port_listening(port).await && probe_json_endpoint(port).await {
                        let ws_url = format!("ws://127.0.0.1:{}{}", port, ws_path);
                        tracing::debug!(
                            "HTTP /json/version failed ({}), but /json responded on port {} — trusting DevToolsActivePort WS: {}",
                            reason, port, ws_url
                        );
                        return Ok(DiscoveredBrowser { ws_url, port });
                    }
                    tracing::debug!("Skipping DevToolsActivePort for port {} ({})", port, reason);
                    continue;
                }
            }
        }
    }

    // Strategy 2: Probe well-known debugging ports (9222-9229 range)
    // Chrome uses 9222 by default; actionbook auto-picks next free port
    // when 9222 is busy, so we scan the full range concurrently.
    let mut handles = Vec::new();
    for port in 9222u16..=9229 {
        handles.push(tokio::spawn(async move {
            discover_via_http(port).await.ok().map(|b| (port, b))
        }));
    }
    // Collect results, pick the lowest port that responded as a real browser
    let mut found: Vec<(u16, DiscoveredBrowser)> = Vec::new();
    for handle in handles {
        if let Ok(Some(result)) = handle.await {
            found.push(result);
        }
    }
    found.sort_by_key(|(port, _)| *port);
    if let Some((_, browser)) = found.into_iter().next() {
        return Ok(browser);
    }

    Err("No running Chrome instance found. \
         Launch Chrome with --remote-debugging-port=9222, \
         or use `browser connect <endpoint>` to specify a URL."
        .to_string())
}

/// Read Chrome's `DevToolsActivePort` file from a user data directory.
///
/// File format (two lines):
///   Line 1: port number
///   Line 2: WebSocket path (e.g. `/devtools/browser/...`)
#[allow(dead_code)]
fn read_devtools_active_port(user_data_dir: &std::path::Path) -> Option<(u16, String)> {
    let path = user_data_dir.join("DevToolsActivePort");
    let content = std::fs::read_to_string(&path).ok()?;
    let mut lines = content.lines();
    let port: u16 = lines.next()?.trim().parse().ok()?;
    let ws_path = lines
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/devtools/browser".to_string());
    Some((port, ws_path))
}

/// Error from HTTP-based CDP discovery.
#[derive(Debug)]
#[allow(dead_code)]
enum DiscoverError {
    /// Port has no CDP HTTP API (e.g., Chrome M146 chrome://inspect mode).
    /// The caller should NOT attempt WS fallback — the port rejects external connections.
    NoCdpHttpApi(String),
    /// Transient or connection error — WS fallback may still work.
    Transient(String),
}

/// Discover a browser's WebSocket URL by querying `http://127.0.0.1:{port}/json/version`.
///
/// Rejects Electron apps (Slack, Discord, VS Code, etc.) by checking the User-Agent
/// for the `Electron` token. Only real browsers are accepted.
#[allow(dead_code)]
async fn discover_via_http(port: u16) -> Result<DiscoveredBrowser, DiscoverError> {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| DiscoverError::Transient(format!("HTTP probe port {}: {}", port, e)))?;

    // Chrome M146 chrome://inspect mode returns 404 for /json/version.
    // These ports also reject WebSocket connections with 403, so skip entirely.
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(DiscoverError::NoCdpHttpApi(format!(
            "Port {} returned 404 — likely chrome://inspect mode (no external CDP access)",
            port
        )));
    }

    let info: serde_json::Value = resp.json().await.map_err(|e| {
        DiscoverError::Transient(format!("Parse /json/version port {}: {}", port, e))
    })?;

    // Positive match: require Chrome or Chromium in the User-Agent.
    // This rejects Electron apps (Slack, Discord, VS Code), Node.js inspector,
    // and any other non-browser debug endpoint that exposes /json/version.
    let user_agent = info
        .get("User-Agent")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let is_real_browser = (user_agent.contains("Chrome/") || user_agent.contains("Chromium/"))
        && !user_agent.contains("Electron/");
    if !is_real_browser {
        return Err(DiscoverError::NoCdpHttpApi(format!(
            "Port {} is not a browser (UA: {})",
            port,
            &user_agent[..user_agent.len().min(80)]
        )));
    }

    let ws_url = info
        .get("webSocketDebuggerUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            DiscoverError::Transient(format!(
                "No webSocketDebuggerUrl in /json/version on port {}",
                port
            ))
        })?;

    Ok(DiscoveredBrowser { ws_url, port })
}

/// Quick TCP connect check — verifies the port is actually listening.
/// Used to reject stale DevToolsActivePort files whose Chrome has exited.
#[allow(dead_code)]
async fn is_port_listening(port: u16) -> bool {
    tokio::time::timeout(
        Duration::from_secs(1),
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Chrome-specific target type values returned by `/json`.
/// Used to positively identify a browser (vs. a random service returning JSON arrays).
#[allow(dead_code)]
const CHROME_TARGET_TYPES: &[&str] = &[
    "page",
    "background_page",
    "service_worker",
    "browser",
    "other",
    "webview",
    "iframe",
];

/// Probe `http://127.0.0.1:{port}/json` — Chrome's page list endpoint.
/// Returns true only if the response is a non-empty JSON array where at least
/// one entry has a `type` field matching a known Chrome target type AND a
/// `webSocketDebuggerUrl` field. This rejects empty arrays (could be anything)
/// and non-browser services that happen to return JSON arrays.
#[allow(dead_code)]
async fn probe_json_endpoint(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/json", port);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return false,
    };

    let pages: Vec<serde_json::Value> = match resp.json().await {
        Ok(p) => p,
        Err(_) => return false,
    };

    // Require at least one entry with a Chrome-specific type AND a WS URL.
    // Empty arrays are rejected — a real browser with debugging enabled
    // always has at least one target (the initial blank tab or about:blank).
    pages.iter().any(|p| {
        let has_chrome_type = p
            .get("type")
            .and_then(|v| v.as_str())
            .map(|t| CHROME_TARGET_TYPES.contains(&t))
            .unwrap_or(false);
        let has_ws_url = p.get("webSocketDebuggerUrl").is_some();
        has_chrome_type && has_ws_url
    })
}

/// Platform-specific Chrome/Chromium user data directories, in priority order.
#[allow(dead_code)]
fn chrome_user_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let base = home.join("Library/Application Support");
            for name in [
                "Google/Chrome",
                "Google/Chrome Canary",
                "Chromium",
                "BraveSoftware/Brave-Browser",
                "Microsoft Edge",
            ] {
                dirs.push(base.join(name));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            let config = home.join(".config");
            for name in [
                "google-chrome",
                "google-chrome-unstable",
                "chromium",
                "BraveSoftware/Brave-Browser",
                "microsoft-edge",
            ] {
                dirs.push(config.join(name));
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            for name in [
                r"Google\Chrome\User Data",
                r"Google\Chrome SxS\User Data",
                r"Chromium\User Data",
                r"BraveSoftware\Brave-Browser\User Data",
                r"Microsoft\Edge\User Data",
            ] {
                dirs.push(base.join(name));
            }
        }
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_user_data_dirs_returns_known_paths() {
        let dirs = chrome_user_data_dirs();
        // Should return at least one directory on any supported platform
        assert!(!dirs.is_empty(), "Expected at least one user data dir");
    }

    #[test]
    fn read_devtools_active_port_parses_two_line_format() {
        let tmp = tempfile::tempdir().unwrap();
        let content = "9222\n/devtools/browser/abc-123\n";
        std::fs::write(tmp.path().join("DevToolsActivePort"), content).unwrap();

        let result = read_devtools_active_port(tmp.path());
        assert_eq!(
            result,
            Some((9222, "/devtools/browser/abc-123".to_string()))
        );
    }

    #[test]
    fn read_devtools_active_port_defaults_ws_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("DevToolsActivePort"), "9333\n").unwrap();

        let result = read_devtools_active_port(tmp.path());
        assert_eq!(result, Some((9333, "/devtools/browser".to_string())));
    }

    #[test]
    fn read_devtools_active_port_returns_none_on_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read_devtools_active_port(tmp.path()), None);
    }

    #[test]
    fn read_devtools_active_port_returns_none_on_invalid_port() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("DevToolsActivePort"), "notaport\n/ws\n").unwrap();
        assert_eq!(read_devtools_active_port(tmp.path()), None);
    }
}
