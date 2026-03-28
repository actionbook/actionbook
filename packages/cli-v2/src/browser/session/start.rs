use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::browser;
use crate::daemon::cdp::ensure_scheme_or_fatal;
use crate::daemon::cdp_session::CdpSession;
use crate::daemon::registry::{SessionEntry, SharedRegistry, TabEntry};
use crate::output::ResponseContext;
use crate::types::{Mode, TabId};

/// Start or attach a browser session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Browser mode
    #[arg(long, value_enum, default_value = "local")]
    pub mode: Mode,
    /// Headless mode
    #[arg(long)]
    pub headless: bool,
    /// Profile name
    #[arg(long)]
    pub profile: Option<String>,
    /// Open this URL on start
    #[arg(long)]
    pub open_url: Option<String>,
    /// Connect to existing CDP endpoint
    #[arg(long)]
    pub cdp_endpoint: Option<String>,
    /// Headers for CDP endpoint (KEY:VALUE), can be specified multiple times
    #[arg(long)]
    pub header: Vec<String>,
    /// Specify a semantic session ID
    #[arg(long)]
    pub set_session_id: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.start";

pub fn context(_cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Ok { data } = result {
        Some(ResponseContext {
            session_id: data["session"]["session_id"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            tab_id: Some(data["tab"]["tab_id"].as_str().unwrap_or("t1").to_string()),
            window_id: None,
            url: data["tab"]["url"].as_str().map(|s| s.to_string()),
            title: data["tab"]["title"].as_str().map(|s| s.to_string()),
        })
    } else {
        None
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Cloud mode validation
    if cmd.mode == Mode::Cloud {
        if cmd.cdp_endpoint.is_none() {
            return ActionResult::fatal_with_hint(
                "MISSING_CDP_ENDPOINT",
                "--mode cloud requires --cdp-endpoint",
                "provide --cdp-endpoint <wss://...> to connect to a cloud browser",
            );
        }
        return execute_cloud(cmd, registry).await;
    }

    execute_local(cmd, registry).await
}

// ── Cloud mode ──────────────────────────────────────────────────────

async fn execute_cloud(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let endpoint = cmd.cdp_endpoint.as_deref().unwrap();

    // Parse headers
    let headers = match parse_headers(&cmd.header) {
        Ok(h) => h,
        Err(e) => return e,
    };

    let mut reg = registry.lock().await;

    // Cloud reuse: same cdp_endpoint = same session (single-connection constraint)
    if let Some(session_id) = reg
        .list()
        .iter()
        .find(|s| s.mode == Mode::Cloud && s.cdp_endpoint.as_deref() == Some(endpoint))
        .map(|s| s.id.as_str().to_string())
    {
        // If session is still connecting (placeholder), tell caller to retry
        if let Some(entry) = reg.get(&session_id) {
            if entry.status == "connecting" {
                return ActionResult::Retryable {
                    reason: "session is still connecting to cloud endpoint".to_string(),
                    hint: "retry in a moment".to_string(),
                };
            }
        }

        // Update headers silently if they changed
        if let Some(entry) = reg.get_mut(&session_id) {
            if entry.headers != headers {
                entry.headers = headers.clone();
            }
        }

        let entry = reg.get(&session_id).unwrap();
        let first_tab_id = entry.tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();

        // Health check: verify CDP connection is still alive
        if let Some(ref cdp) = entry.cdp {
            let cdp = cdp.clone();
            drop(reg);
            if let Err(_) = cdp.execute_browser("Target.getTargets", json!({})).await {
                // Connection dead — remove session and return error
                let mut reg = registry.lock().await;
                reg.remove(&session_id);
                return ActionResult::fatal_with_hint(
                    "CLOUD_CONNECTION_LOST",
                    "cloud browser connection lost",
                    "run `actionbook browser start --mode cloud ...` to reconnect",
                );
            }
            let mut reg = registry.lock().await;
            // Update headers if changed (after health check passed)
            if let Some(entry) = reg.get_mut(&session_id) {
                if entry.headers != headers {
                    entry.headers = headers;
                }
            }
            let entry = match reg.get(&session_id) {
                Some(e) => e,
                None => {
                    return ActionResult::fatal(
                        "SESSION_NOT_FOUND",
                        format!("session '{session_id}' was closed during health check"),
                    );
                }
            };
            let first_tab_id = entry.tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();

            // If --open-url, navigate the first tab
            if let Some(url) = &cmd.open_url {
                let final_url = match ensure_scheme_or_fatal(url) {
                    Ok(u) => u,
                    Err(e) => return e,
                };
                if !first_tab_id.is_empty() {
                    if let Some(ref cdp) = entry.cdp {
                        let cdp = cdp.clone();
                        drop(reg);
                        if let Err(e) = cdp
                            .execute_on_tab(&first_tab_id, "Page.navigate", json!({ "url": final_url }))
                            .await
                        {
                            return ActionResult::fatal(
                                "NAVIGATION_FAILED",
                                format!("cloud reuse navigate failed: {e}"),
                            );
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        let reg = registry.lock().await;
                        if let Some(entry) = reg.get(&session_id) {
                            return make_session_response(entry, &first_tab_id, "", "", true);
                        }
                        return ActionResult::fatal(
                            "SESSION_NOT_FOUND",
                            format!("session '{session_id}' was closed during reuse"),
                        );
                    }
                }
            }

            return make_session_response(entry, &first_tab_id, "", "", true);
        }

        // No CDP connection — this should not happen for running sessions
        return ActionResult::fatal(
            "INTERNAL_ERROR",
            format!("session '{session_id}' has no CDP connection"),
        );
    }

    // New cloud session — insert placeholder to prevent concurrent duplicate connections
    let session_id =
        match reg.generate_session_id(cmd.set_session_id.as_deref(), cmd.profile.as_deref()) {
            Ok(id) => id,
            Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
        };
    let profile_name = cmd.profile.as_deref().unwrap_or("actionbook");
    let placeholder = SessionEntry {
        id: session_id.clone(),
        mode: Mode::Cloud,
        headless: cmd.headless,
        profile: profile_name.to_string(),
        status: "connecting".to_string(),
        cdp_port: None,
        ws_url: endpoint.to_string(),
        cdp_endpoint: Some(endpoint.to_string()),
        headers: headers.clone(),
        tabs: Vec::new(),
        chrome_process: None,
        cdp: None,
    };
    reg.insert(placeholder);
    drop(reg);

    // Connect to cloud endpoint with headers
    let cdp = match CdpSession::connect_with_headers(endpoint, &headers).await {
        Ok(c) => c,
        Err(e) => {
            // Remove placeholder on failure
            let mut reg = registry.lock().await;
            reg.remove(session_id.as_str());
            return ActionResult::fatal(e.error_code(), e.to_string());
        }
    };

    // Discover existing tabs via CDP Target.getTargets
    let tabs = match discover_tabs_via_cdp(&cdp).await {
        Ok(t) => t,
        Err(e) => {
            let mut reg = registry.lock().await;
            reg.remove(session_id.as_str());
            return e;
        }
    };

    // Zero-page handling: create a tab if none exist
    let tabs = if tabs.is_empty() {
        let raw_url = cmd.open_url.as_deref().unwrap_or("about:blank");
        let url = match ensure_scheme_or_fatal(raw_url) {
            Ok(u) => u,
            Err(e) => {
                let mut reg = registry.lock().await;
                reg.remove(session_id.as_str());
                return e;
            }
        };
        match create_tab_via_cdp(&cdp, &url).await {
            Ok(tab) => vec![tab],
            Err(e) => {
                let mut reg = registry.lock().await;
                reg.remove(session_id.as_str());
                return e;
            }
        }
    } else {
        // Attach all discovered tabs
        for tab in &tabs {
            if let Err(e) = cdp.attach(&tab.id.0).await {
                tracing::warn!("failed to attach cloud tab {}: {e}", tab.id);
            }
        }
        // Navigate first tab if --open-url
        if let Some(url) = &cmd.open_url {
            let final_url = match ensure_scheme_or_fatal(url) {
                Ok(u) => u,
                Err(e) => {
                    let mut reg = registry.lock().await;
                    reg.remove(session_id.as_str());
                    return e;
                }
            };
            if let Some(first) = tabs.first() {
                if let Err(e) = cdp
                    .execute_on_tab(&first.id.0, "Page.navigate", json!({ "url": final_url }))
                    .await
                {
                    tracing::warn!("cloud reuse navigate failed: {e}");
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
        tabs
    };

    let first_tab_id = tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();
    let first_url = tabs.first().map(|t| t.url.clone()).unwrap_or_default();
    let first_title = tabs.first().map(|t| t.title.clone()).unwrap_or_default();

    // Replace placeholder with fully initialized entry
    let entry = SessionEntry {
        id: session_id.clone(),
        mode: Mode::Cloud,
        headless: cmd.headless,
        profile: profile_name.to_string(),
        status: "running".to_string(),
        cdp_port: None,
        ws_url: endpoint.to_string(),
        cdp_endpoint: Some(endpoint.to_string()),
        headers,
        tabs,
        chrome_process: None,
        cdp: Some(cdp),
    };

    let mut reg = registry.lock().await;
    reg.insert(entry);

    ActionResult::ok(json!({
        "session": {
            "session_id": session_id.as_str(),
            "mode": "cloud",
            "status": "running",
            "headless": cmd.headless,
            "cdp_endpoint": redact_endpoint(endpoint),
        },
        "tab": {
            "tab_id": first_tab_id,
            "url": first_url,
            "title": first_title,
        },
        "reused": false,
    }))
}

// ── Local mode ──────────────────────────────────────────────────────

async fn execute_local(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let mut reg = registry.lock().await;
    let profile_name = cmd.profile.as_deref().unwrap_or("actionbook");

    // Local mode: 1 profile = max 1 session. Reuse existing if same profile.
    if let Some(session_id) = reg
        .list()
        .iter()
        .find(|s| s.profile == profile_name && s.mode == cmd.mode)
        .map(|s| s.id.as_str().to_string())
    {
        if let Some(url) = &cmd.open_url {
            let final_url = match ensure_scheme_or_fatal(url) {
                Ok(u) => u,
                Err(e) => return e,
            };
            let entry = reg.get(&session_id).unwrap();
            let first_tab_id = entry.tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();
            let cdp = entry.cdp.clone();
            let cdp_port = entry.cdp_port;
            drop(reg);

            if let Some(ref cdp) = cdp
                && !first_tab_id.is_empty()
            {
                let nav_result = cdp
                    .execute_on_tab(
                        &first_tab_id,
                        "Page.navigate",
                        serde_json::json!({ "url": final_url }),
                    )
                    .await;
                if let Err(e) = nav_result {
                    return ActionResult::fatal(
                        "NAVIGATION_FAILED",
                        format!("reuse navigate failed: {e}"),
                    );
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }

            // Fetch real-time tab info
            let targets = if let Some(port) = cdp_port {
                browser::list_targets(port).await.unwrap_or_default()
            } else {
                Vec::new()
            };
            let (tab_url, tab_title) = get_tab_info_from_targets(&targets, &first_tab_id);

            let reg = registry.lock().await;
            if let Some(entry) = reg.get(&session_id) {
                return ActionResult::ok(json!({
                    "session": {
                        "session_id": entry.id.as_str(),
                        "mode": entry.mode.to_string(),
                        "status": entry.status,
                        "headless": entry.headless,
                        "cdp_endpoint": entry.ws_url,
                    },
                    "tab": {
                        "tab_id": first_tab_id,
                        "url": tab_url,
                        "title": tab_title,
                    },
                    "reused": true,
                }));
            }
            return ActionResult::fatal(
                "SESSION_NOT_FOUND",
                format!("session '{session_id}' was closed during reuse"),
            );
        }

        // Reuse without open-url: fetch real-time info
        let entry = reg.get(&session_id).unwrap();
        let first_tab_id = entry.tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();
        let cdp_port = entry.cdp_port;
        drop(reg);
        let targets = if let Some(port) = cdp_port {
            browser::list_targets(port).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let (tab_url, tab_title) = get_tab_info_from_targets(&targets, &first_tab_id);
        let reg = registry.lock().await;
        if let Some(entry) = reg.get(&session_id) {
            return ActionResult::ok(json!({
                "session": {
                    "session_id": entry.id.as_str(),
                    "mode": entry.mode.to_string(),
                    "status": entry.status,
                    "headless": entry.headless,
                    "cdp_endpoint": entry.ws_url,
                },
                "tab": {
                    "tab_id": first_tab_id,
                    "url": tab_url,
                    "title": tab_title,
                },
                "reused": true,
            }));
        }
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!("session '{session_id}' was closed during reuse"),
        );
    }

    let session_id =
        match reg.generate_session_id(cmd.set_session_id.as_deref(), cmd.profile.as_deref()) {
            Ok(id) => id,
            Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
        };

    let executable = match browser::find_chrome() {
        Ok(e) => e,
        Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
    };

    if profile_name.contains('/') || profile_name.contains('\\') || profile_name.contains("..") {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid profile name: {profile_name}"),
        );
    }

    let data_dir = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{home}/.local/share")
    });
    let user_data_dir = format!("{data_dir}/actionbook/profiles/{profile_name}");
    std::fs::create_dir_all(&user_data_dir).ok();

    for lock in &["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        let p = std::path::Path::new(&user_data_dir).join(lock);
        if p.exists() {
            std::fs::remove_file(&p).ok();
        }
    }

    let (mut chrome, port) = match browser::launch_chrome(
        &executable,
        cmd.headless,
        &user_data_dir,
        cmd.open_url.as_deref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
    };

    let ws_url = match browser::discover_ws_url(port).await {
        Ok(ws) => ws,
        Err(e) => {
            let _ = chrome.kill();
            let _ = chrome.wait();
            return ActionResult::fatal(e.error_code(), e.to_string());
        }
    };

    if cmd.open_url.is_some() {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    let mut targets = browser::list_targets(port).await.unwrap_or_default();

    if targets
        .first()
        .and_then(|t| t.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
    {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        targets = browser::list_targets(port).await.unwrap_or(targets);
    }

    let mut tabs = Vec::new();
    for t in &targets {
        let target_id = t
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !target_id.is_empty() {
            let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            tabs.push(TabEntry {
                id: TabId(target_id),
                url,
                title,
            });
        }
    }

    // Create persistent CDP connection and attach all initial tabs
    let cdp = match CdpSession::connect(&ws_url).await {
        Ok(c) => c,
        Err(e) => {
            let _ = chrome.kill();
            let _ = chrome.wait();
            return ActionResult::fatal("CDP_CONNECTION_FAILED", e.to_string());
        }
    };
    for tab in &tabs {
        if let Err(e) = cdp.attach(&tab.id.0).await {
            tracing::warn!("failed to attach tab {}: {e}", tab.id);
        }
    }

    let first_tab_id = tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();

    let (first_url, first_title) = if !first_tab_id.is_empty() {
        get_tab_info_from_targets(&targets, &first_tab_id)
    } else {
        (cmd.open_url.as_deref().unwrap_or("about:blank").to_string(), String::new())
    };

    let entry = SessionEntry {
        id: session_id.clone(),
        mode: cmd.mode,
        headless: cmd.headless,
        profile: profile_name.to_string(),
        status: "running".to_string(),
        cdp_port: Some(port),
        ws_url: ws_url.clone(),
        cdp_endpoint: None,
        headers: Vec::new(),
        tabs,
        chrome_process: Some(chrome),
        cdp: Some(cdp),
    };
    reg.insert(entry);

    ActionResult::ok(json!({
        "session": {
            "session_id": session_id.as_str(),
            "mode": cmd.mode.to_string(),
            "status": "running",
            "headless": cmd.headless,
            "cdp_endpoint": ws_url,
        },
        "tab": {
            "tab_id": first_tab_id,
            "url": first_url,
            "title": first_title,
        },
        "reused": false,
    }))
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Parse `--header KEY:VALUE` strings into `(key, value)` pairs.
/// Value may contain additional colons (e.g., `Authorization:Bearer abc:def`).
fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>, ActionResult> {
    raw.iter()
        .map(|h| {
            let (key, value) = h.split_once(':').ok_or_else(|| {
                ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    format!("invalid header format, expected KEY:VALUE"),
                )
            })?;
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.is_empty() {
                return Err(ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    "header key must not be empty".to_string(),
                ));
            }
            Ok((key, value))
        })
        .collect()
}

/// Discover page tabs via CDP Target.getTargets, filtering by type=="page".
async fn discover_tabs_via_cdp(cdp: &CdpSession) -> Result<Vec<TabEntry>, ActionResult> {
    let resp = cdp
        .execute_browser("Target.getTargets", json!({}))
        .await
        .map_err(|e| ActionResult::fatal("CDP_ERROR", format!("Target.getTargets failed: {e}")))?;

    let infos = resp
        .get("result")
        .and_then(|r| r.get("targetInfos"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    let tabs: Vec<TabEntry> = infos
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
        .filter_map(|t| {
            let id = t.get("targetId").and_then(|v| v.as_str())?.to_string();
            let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Some(TabEntry {
                id: TabId(id),
                url,
                title,
            })
        })
        .collect();

    Ok(tabs)
}

/// Create a new tab via CDP Target.createTarget.
async fn create_tab_via_cdp(cdp: &CdpSession, url: &str) -> Result<TabEntry, ActionResult> {
    let resp = cdp
        .execute_browser("Target.createTarget", json!({ "url": url }))
        .await
        .map_err(|e| ActionResult::fatal("CDP_ERROR", format!("Target.createTarget failed: {e}")))?;

    let target_id = resp
        .get("result")
        .and_then(|r| r.get("targetId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ActionResult::fatal("CDP_ERROR", format!("Target.createTarget did not return targetId: {resp}"))
        })?
        .to_string();

    cdp.attach(&target_id)
        .await
        .map_err(|e| ActionResult::fatal("CDP_ERROR", format!("failed to attach new tab: {e}")))?;

    Ok(TabEntry {
        id: TabId(target_id),
        url: url.to_string(),
        title: String::new(),
    })
}

/// Build a session response JSON.
fn make_session_response(
    entry: &crate::daemon::registry::SessionEntry,
    tab_id: &str,
    tab_url: &str,
    tab_title: &str,
    reused: bool,
) -> ActionResult {
    ActionResult::ok(json!({
        "session": {
            "session_id": entry.id.as_str(),
            "mode": entry.mode.to_string(),
            "status": entry.status,
            "headless": entry.headless,
            "cdp_endpoint": redact_endpoint(entry.cdp_endpoint.as_deref().unwrap_or(&entry.ws_url)),
        },
        "tab": {
            "tab_id": tab_id,
            "url": tab_url,
            "title": tab_title,
        },
        "reused": reused,
    }))
}

/// Redact query parameters from endpoint URLs to prevent credential leaks.
/// Cloud providers often encode auth tokens in signed URLs.
pub fn redact_endpoint(url: &str) -> String {
    if let Some(idx) = url.find('?') {
        format!("{}?[REDACTED]", &url[..idx])
    } else {
        url.to_string()
    }
}

/// Extract url/title for a target_id from a targets list.
fn get_tab_info_from_targets(targets: &[serde_json::Value], target_id: &str) -> (String, String) {
    for t in targets {
        if t.get("id").and_then(|v| v.as_str()) == Some(target_id) {
            let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            return (url, title);
        }
    }
    (String::new(), String::new())
}
