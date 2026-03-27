use std::process::{Child, Stdio};
use std::time::Duration;

use crate::error::CliError;

/// Find Chrome executable on macOS/Linux.
pub fn find_chrome() -> Result<String, CliError> {
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Ok(c.to_string());
        }
        // Check if it's in PATH
        if let Ok(output) = std::process::Command::new("which")
            .arg(c)
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(path);
                }
            }
        }
    }
    Err(CliError::BrowserNotFound)
}

/// Launch Chrome with CDP enabled.
pub fn launch_chrome(
    executable: &str,
    port: u16,
    headless: bool,
    user_data_dir: &str,
    open_url: Option<&str>,
) -> Result<Child, CliError> {
    let mut args = vec![
        format!("--remote-debugging-port={port}"),
        format!("--user-data-dir={user_data_dir}"),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
    ];
    if headless {
        args.push("--headless=new".to_string());
    }
    if let Some(url) = open_url {
        args.push(ensure_scheme(url));
    }

    let child = std::process::Command::new(executable)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CliError::BrowserLaunchFailed(e.to_string()))?;

    Ok(child)
}

/// Find an available port, trying 9222 first.
pub fn find_available_port() -> u16 {
    if std::net::TcpListener::bind(("127.0.0.1", 9222)).is_ok() {
        return 9222;
    }
    // Find a random available port
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

/// Discover the WebSocket debugger URL from Chrome's /json/version endpoint.
pub async fn discover_ws_url(port: u16) -> Result<String, CliError> {
    let url = format!("http://127.0.0.1:{port}/json/version");

    for attempt in 0..30 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        match reqwest::get(&url).await {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(ws) = json
                        .get("webSocketDebuggerUrl")
                        .and_then(|v| v.as_str())
                    {
                        return Ok(ws.to_string());
                    }
                }
            }
            Err(_) => continue,
        }
    }
    Err(CliError::CdpConnectionFailed(format!(
        "Chrome did not expose CDP on port {port} within 6s"
    )))
}

/// Get list of targets (tabs) from Chrome.
pub async fn list_targets(port: u16) -> Result<Vec<serde_json::Value>, CliError> {
    let url = format!("http://127.0.0.1:{port}/json/list");
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

fn ensure_scheme(url: &str) -> String {
    if url.contains("://") || url.contains(':') {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}
