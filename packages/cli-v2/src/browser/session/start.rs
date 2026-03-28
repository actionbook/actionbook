use std::process::Child;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::config;
use crate::config::DEFAULT_PROFILE;
use crate::daemon::browser;
use crate::daemon::cdp::{cdp_navigate, ensure_scheme, ensure_scheme_or_fatal};
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result};
use crate::daemon::registry::{SessionState, SharedRegistry, TabEntry};
use crate::output::ResponseContext;
use crate::types::{Mode, SessionId, TabId};

/// Start or attach a browser session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Browser mode
    #[arg(long, value_enum)]
    pub mode: Option<Mode>,
    /// Headless mode
    #[arg(long, default_missing_value = "true", num_args = 0..=1)]
    pub headless: Option<bool>,
    /// Profile name
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(skip = None)]
    #[serde(default)]
    pub executable: Option<String>,
    /// Open this URL on start
    #[arg(long)]
    pub open_url: Option<String>,
    /// Connect to existing CDP endpoint
    #[arg(long)]
    pub cdp_endpoint: Option<String>,
    /// Headers for CDP endpoint (KEY:VALUE), may be repeated
    #[arg(long)]
    pub header: Vec<String>,
    /// Specify a semantic session ID
    #[arg(long)]
    pub set_session_id: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.start";

struct ReuseTarget {
    session_id: String,
    first_tab_id: String,
    cdp: Option<CdpSession>,
    cdp_port: Option<u16>,
}

enum StartDisposition {
    Reuse(ReuseTarget),
    Reserved(SessionId),
}

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
    let mode = cmd.mode.unwrap_or(Mode::Local);
    let headless = cmd.headless.unwrap_or(false);
    let profile_name = cmd.profile.as_deref().unwrap_or(DEFAULT_PROFILE);
    let cdp_endpoint = cmd.cdp_endpoint.as_deref();

    if profile_name.contains('/') || profile_name.contains('\\') || profile_name.contains("..") {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid profile name: {profile_name}"),
        );
    }

    // Cloud mode requires --cdp-endpoint
    if mode == Mode::Cloud && cdp_endpoint.is_none() {
        return ActionResult::fatal_with_hint(
            "MISSING_CDP_ENDPOINT",
            "--mode cloud requires --cdp-endpoint",
            "provide a cloud browser endpoint, e.g. --cdp-endpoint wss://...",
        );
    }

    // Parse headers from "KEY:VALUE" strings
    let headers = match parse_headers(&cmd.header) {
        Ok(h) => h,
        Err(e) => return e,
    };

    // ── Cloud mode ──────────────────────────────────────────────────
    if mode == Mode::Cloud {
        return execute_cloud(
            cmd,
            registry,
            cdp_endpoint.unwrap(),
            &headers,
            profile_name,
            headless,
        )
        .await;
    }

    // ── Local / Extension mode (unchanged) ──────────────────────────

    if cdp_endpoint.is_some() && mode != Mode::Local {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            "cdp-endpoint requires --mode local".to_string(),
        );
    }

    let disposition = {
        let mut reg = registry.lock().await;

        if cdp_endpoint.is_none()
            && mode == Mode::Local
            && let Some(existing) = reg.find_local_session_by_profile(profile_name, mode)
        {
            match existing.status {
                SessionState::Running => StartDisposition::Reuse(ReuseTarget {
                    session_id: existing.id.as_str().to_string(),
                    first_tab_id: existing
                        .tabs
                        .first()
                        .map(|tab| tab.id.0.clone())
                        .unwrap_or_default(),
                    cdp: existing.cdp.clone(),
                    cdp_port: existing.cdp_port,
                }),
                SessionState::Starting => {
                    return ActionResult::fatal_with_hint(
                        "SESSION_STARTING",
                        format!("session for profile '{profile_name}' is starting, please wait"),
                        "retry after a few seconds or use browser status to check",
                    );
                }
                SessionState::Closed => unreachable!("closed sessions are excluded from lookup"),
            }
        } else {
            match reg.reserve_session_start(
                cmd.set_session_id.as_deref(),
                cmd.profile.as_deref(),
                profile_name,
                mode,
                headless,
            ) {
                Ok(session_id) => StartDisposition::Reserved(session_id),
                Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
            }
        }
    };

    let session_id = match disposition {
        StartDisposition::Reuse(target) => {
            return reuse_running_session(cmd, registry, target).await;
        }
        StartDisposition::Reserved(session_id) => session_id,
    };

    let profiles_dir = config::profiles_dir();
    std::fs::create_dir_all(&profiles_dir).ok();
    let user_data_dir = profiles_dir.join(profile_name);
    std::fs::create_dir_all(&user_data_dir).ok();

    for lock in &["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        let p = user_data_dir.join(lock);
        if p.exists() {
            std::fs::remove_file(&p).ok();
        }
    }

    let (mut chrome_process, port, ws_url, mut targets) = if let Some(endpoint) = cdp_endpoint {
        let (ws_url, port) = match browser::resolve_cdp_endpoint(endpoint).await {
            Ok(value) => value,
            Err(e) => {
                return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string())
                    .await;
            }
        };

        let mut targets = browser::list_targets(port).await.unwrap_or_default();
        if let Some(url) = &cmd.open_url
            && let Some(target_id) = targets
                .first()
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
        {
            let page_ws = format!("ws://127.0.0.1:{port}/devtools/page/{target_id}");
            if let Err(e) = cdp_navigate(
                &page_ws,
                &ensure_scheme(url).unwrap_or_else(|_| url.to_string()),
            )
            .await
            {
                return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string())
                    .await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            targets = browser::list_targets(port).await.unwrap_or(targets);
        }

        (None, Some(port), ws_url, targets)
    } else {
        let executable = if let Some(executable) = cmd.executable.as_deref() {
            executable.to_string()
        } else {
            match browser::find_chrome() {
                Ok(e) => e,
                Err(e) => {
                    return fail_reserved_start(
                        registry,
                        &session_id,
                        e.error_code(),
                        e.to_string(),
                    )
                    .await;
                }
            }
        };

        let (chrome, port) = match browser::launch_chrome(
            &executable,
            headless,
            &user_data_dir.to_string_lossy(),
            cmd.open_url.as_deref(),
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string())
                    .await;
            }
        };

        let ws_url = match browser::discover_ws_url(port).await {
            Ok(ws) => ws,
            Err(e) => {
                return fail_reserved_start_with_chrome(
                    registry,
                    &session_id,
                    Some(chrome),
                    e.error_code(),
                    e.to_string(),
                )
                .await;
            }
        };

        if cmd.open_url.is_some() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        let targets = browser::list_targets(port).await.unwrap_or_default();
        (Some(chrome), Some(port), ws_url, targets)
    };

    if targets
        .first()
        .and_then(|t| t.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
        && let Some(p) = port
    {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        targets = browser::list_targets(p).await.unwrap_or(targets);
    }

    let mut tabs = Vec::new();
    for t in &targets {
        let target_id = t
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !target_id.is_empty() {
            let url = t
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = t
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
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
            return fail_reserved_start_with_chrome(
                registry,
                &session_id,
                chrome_process.take(),
                "CDP_CONNECTION_FAILED",
                e.to_string(),
            )
            .await;
        }
    };
    for tab in &tabs {
        if let Err(e) = cdp.attach(&tab.id.0).await {
            tracing::warn!("failed to attach tab {}: {e}", tab.id);
        }
    }

    let first_tab_id = tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();

    // Get real-time info for the first tab
    let (first_url, first_title) = if !first_tab_id.is_empty() {
        get_tab_info_from_targets(&targets, &first_tab_id)
    } else {
        (
            cmd.open_url.as_deref().unwrap_or("about:blank").to_string(),
            String::new(),
        )
    };

    let mut reg = registry.lock().await;
    let Some(entry) = reg.get_mut(session_id.as_str()) else {
        drop(reg);
        cleanup_chrome_process(chrome_process.take());
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!(
                "session '{}' was closed during startup",
                session_id.as_str()
            ),
        );
    };
    entry.mode = mode;
    entry.headless = headless;
    entry.profile = profile_name.to_string();
    entry.status = SessionState::Running;
    entry.cdp_port = port;
    entry.ws_url = ws_url.clone();
    entry.tabs = tabs;
    entry.chrome_process = chrome_process;
    entry.cdp = Some(cdp);

    ActionResult::ok(json!({
        "session": {
            "session_id": session_id.as_str(),
            "mode": mode.to_string(),
            "status": "running",
            "headless": headless,
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

/// Extract url/title for a target_id from a targets list.
fn get_tab_info_from_targets(targets: &[serde_json::Value], target_id: &str) -> (String, String) {
    for t in targets {
        if t.get("id").and_then(|v| v.as_str()) == Some(target_id) {
            let url = t
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = t
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return (url, title);
        }
    }
    (String::new(), String::new())
}

async fn reuse_running_session(
    cmd: &Cmd,
    registry: &SharedRegistry,
    target: ReuseTarget,
) -> ActionResult {
    if let Some(url) = &cmd.open_url {
        let final_url = ensure_scheme(url).unwrap_or_else(|_| url.to_string());
        if let Some(ref cdp) = target.cdp
            && !target.first_tab_id.is_empty()
        {
            let nav_result = cdp
                .execute_on_tab(
                    &target.first_tab_id,
                    "Page.navigate",
                    serde_json::json!({ "url": final_url }),
                )
                .await;
            if let Err(e) = nav_result {
                return cdp_error_to_result(e, "NAVIGATION_FAILED");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    // For local sessions, refresh tab info from /json/list; for cloud, use registry
    let (tab_url, tab_title) = if let Some(port) = target.cdp_port {
        let targets = browser::list_targets(port).await.unwrap_or_default();
        get_tab_info_from_targets(&targets, &target.first_tab_id)
    } else {
        // Cloud: get info from registry
        let reg = registry.lock().await;
        if let Some(entry) = reg.get(&target.session_id) {
            entry
                .tabs
                .iter()
                .find(|t| t.id.0 == target.first_tab_id)
                .map(|t| (t.url.clone(), t.title.clone()))
                .unwrap_or_default()
        } else {
            (String::new(), String::new())
        }
    };

    let reg = registry.lock().await;
    let Some(entry) = reg.get(&target.session_id) else {
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!("session '{}' not found", target.session_id),
        );
    };

    ActionResult::ok(json!({
        "session": {
            "session_id": entry.id.as_str(),
            "mode": entry.mode.to_string(),
            "status": entry.status.to_string(),
            "headless": entry.headless,
            "cdp_endpoint": entry.ws_url,
        },
        "tab": {
            "tab_id": target.first_tab_id,
            "url": tab_url,
            "title": tab_title,
        },
        "reused": true,
    }))
}

async fn fail_reserved_start(
    registry: &SharedRegistry,
    session_id: &SessionId,
    code: &str,
    message: String,
) -> ActionResult {
    registry.lock().await.remove(session_id.as_str());
    ActionResult::fatal(code, message)
}

async fn fail_reserved_start_with_chrome(
    registry: &SharedRegistry,
    session_id: &SessionId,
    chrome_process: Option<Child>,
    code: &str,
    message: String,
) -> ActionResult {
    cleanup_chrome_process(chrome_process);
    registry.lock().await.remove(session_id.as_str());
    ActionResult::fatal(code, message)
}

fn cleanup_chrome_process(chrome_process: Option<Child>) {
    if let Some(mut chrome) = chrome_process {
        let _ = chrome.kill();
        let _ = chrome.wait();
    }
}

// ── Cloud mode ──────────────────────────────────────────────────────

/// Execute cloud-mode session start.
///
/// Cloud sessions connect directly to a remote CDP endpoint via WebSocket
/// with optional auth headers. No local Chrome process is launched.
async fn execute_cloud(
    cmd: &Cmd,
    registry: &SharedRegistry,
    cdp_endpoint: &str,
    headers: &[(String, String)],
    profile_name: &str,
    headless: bool,
) -> ActionResult {
    let ws_url = match ensure_scheme_or_fatal(cdp_endpoint) {
        Ok(u) => u,
        Err(e) => return e,
    };

    // ── Cloud session reuse: match on cdp_endpoint ──
    {
        let reg = registry.lock().await;
        if let Some(existing) = reg.find_cloud_session_by_endpoint(cdp_endpoint) {
            match existing.status {
                SessionState::Running => {
                    // Health check via CDP — if it fails, drop old session and reconnect
                    if let Some(ref cdp) = existing.cdp {
                        match cdp.execute_browser("Target.getTargets", json!({})).await {
                            Ok(_) => {
                                return make_session_response(
                                    existing.id.as_str(),
                                    existing,
                                    true,
                                    cdp_endpoint,
                                );
                            }
                            Err(_) => {
                                // Stale — will fall through to create new session
                                tracing::info!(
                                    "cloud session '{}' health check failed, reconnecting",
                                    existing.id.as_str()
                                );
                            }
                        }
                    }
                }
                SessionState::Starting => {
                    return ActionResult::fatal_with_hint(
                        "SESSION_STARTING",
                        format!(
                            "cloud session for endpoint '{}' is starting, please wait",
                            redact_endpoint(cdp_endpoint)
                        ),
                        "retry after a few seconds or use browser status to check",
                    );
                }
                SessionState::Closed => {}
            }
        }
    }

    // ── Reserve placeholder ──
    let session_id = {
        let mut reg = registry.lock().await;
        match reg.reserve_session_start(
            cmd.set_session_id.as_deref(),
            cmd.profile.as_deref(),
            profile_name,
            Mode::Cloud,
            headless,
        ) {
            Ok(sid) => sid,
            Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
        }
    };

    // ── Connect with headers ──
    let cdp = match CdpSession::connect_with_headers(&ws_url, headers).await {
        Ok(c) => c,
        Err(e) => {
            return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string()).await;
        }
    };

    // ── Discover tabs via Target.getTargets ──
    let tabs = match discover_tabs_via_cdp(&cdp).await {
        Ok(t) => t,
        Err(e) => {
            return fail_reserved_start(registry, &session_id, "CDP_ERROR", e.to_string()).await;
        }
    };

    // ── Zero-tab fallback: create a new page ──
    let tabs = if tabs.is_empty() {
        let open_url = cmd.open_url.as_deref().unwrap_or("about:blank");
        match create_tab_via_cdp(&cdp, open_url).await {
            Ok(tab) => vec![tab],
            Err(e) => {
                return fail_reserved_start(
                    registry,
                    &session_id,
                    "CDP_ERROR",
                    format!("failed to create initial tab: {e}"),
                )
                .await;
            }
        }
    } else {
        tabs
    };

    // Attach all tabs
    for tab in &tabs {
        if let Err(e) = cdp.attach(&tab.id.0).await {
            tracing::warn!("cloud: failed to attach tab {}: {e}", tab.id);
        }
    }

    // Navigate first tab if open_url provided and we didn't just create with it
    if let Some(url) = &cmd.open_url
        && !tabs.is_empty()
        && tabs[0].url != *url
    {
        let final_url = match ensure_scheme_or_fatal(url) {
            Ok(u) => u,
            Err(e) => {
                registry.lock().await.remove(session_id.as_str());
                return e;
            }
        };
        let first_id = &tabs[0].id.0;
        if let Err(e) = cdp
            .execute_on_tab(first_id, "Page.navigate", json!({ "url": final_url }))
            .await
        {
            tracing::warn!("cloud: navigate on start failed: {e}");
        }
    }

    let first_tab_id = tabs.first().map(|t| t.id.0.clone()).unwrap_or_default();
    let first_url = tabs
        .first()
        .map(|t| t.url.clone())
        .unwrap_or_else(|| "about:blank".to_string());
    let first_title = tabs.first().map(|t| t.title.clone()).unwrap_or_default();

    // ── Finalize registry entry ──
    let mut reg = registry.lock().await;
    let Some(entry) = reg.get_mut(session_id.as_str()) else {
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!(
                "session '{}' was closed during startup",
                session_id.as_str()
            ),
        );
    };
    entry.mode = Mode::Cloud;
    entry.headless = headless;
    entry.profile = profile_name.to_string();
    entry.status = SessionState::Running;
    entry.cdp_port = None;
    entry.ws_url = ws_url.clone();
    entry.tabs = tabs;
    entry.chrome_process = None;
    entry.cdp = Some(cdp);
    entry.cdp_endpoint = Some(cdp_endpoint.to_string());
    entry.headers = headers.to_vec();

    ActionResult::ok(json!({
        "session": {
            "session_id": session_id.as_str(),
            "mode": "cloud",
            "status": "running",
            "headless": headless,
            "cdp_endpoint": redact_endpoint(cdp_endpoint),
        },
        "tab": {
            "tab_id": first_tab_id,
            "url": first_url,
            "title": first_title,
        },
        "reused": false,
    }))
}

/// Discover page tabs via CDP Target.getTargets.
async fn discover_tabs_via_cdp(cdp: &CdpSession) -> Result<Vec<TabEntry>, crate::error::CliError> {
    let resp = cdp.execute_browser("Target.getTargets", json!({})).await?;
    let target_infos = resp
        .pointer("/result/targetInfos")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let tabs: Vec<TabEntry> = target_infos
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
        .filter_map(|t| {
            let target_id = t.get("targetId").and_then(|v| v.as_str())?;
            let url = t
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = t
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(TabEntry {
                id: TabId(target_id.to_string()),
                url,
                title,
            })
        })
        .collect();
    Ok(tabs)
}

/// Create a new tab via CDP Target.createTarget.
async fn create_tab_via_cdp(
    cdp: &CdpSession,
    url: &str,
) -> Result<TabEntry, crate::error::CliError> {
    let resp = cdp
        .execute_browser("Target.createTarget", json!({ "url": url }))
        .await?;
    let target_id = resp
        .pointer("/result/targetId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::error::CliError::CdpError(
                "Target.createTarget did not return targetId".to_string(),
            )
        })?
        .to_string();
    Ok(TabEntry {
        id: TabId(target_id),
        url: url.to_string(),
        title: String::new(),
    })
}

/// Build a session response for reuse or new session.
fn make_session_response(
    session_id: &str,
    entry: &crate::daemon::registry::SessionEntry,
    reused: bool,
    cdp_endpoint: &str,
) -> ActionResult {
    let first_tab = entry.tabs.first();
    ActionResult::ok(json!({
        "session": {
            "session_id": session_id,
            "mode": entry.mode.to_string(),
            "status": entry.status.to_string(),
            "headless": entry.headless,
            "cdp_endpoint": redact_endpoint(cdp_endpoint),
        },
        "tab": {
            "tab_id": first_tab.map(|t| t.id.0.as_str()).unwrap_or(""),
            "url": first_tab.map(|t| t.url.as_str()).unwrap_or(""),
            "title": first_tab.map(|t| t.title.as_str()).unwrap_or(""),
        },
        "reused": reused,
    }))
}

/// Parse "KEY:VALUE" header strings into (key, value) tuples.
/// Returns error for malformed headers (missing colon, empty key).
fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>, ActionResult> {
    raw.iter()
        .map(|h| {
            let (key, value) = h.split_once(':').ok_or_else(|| {
                ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    "invalid header format, expected KEY:VALUE".to_string(),
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

/// Redact a CDP endpoint for safe display (mask auth tokens in query/path).
pub fn redact_endpoint(endpoint: &str) -> String {
    // Simple redaction: if the endpoint contains a token-like path segment, mask it
    if let Some(idx) = endpoint.find("://") {
        let after_scheme = &endpoint[idx + 3..];
        // Find host:port boundary
        if let Some(slash_idx) = after_scheme.find('/') {
            let host_port = &after_scheme[..slash_idx];
            let path = &after_scheme[slash_idx..];
            // Redact path if it looks like a token (long alphanumeric)
            let redacted_path = if path.len() > 10 {
                format!("/{}***", &path[1..5.min(path.len())])
            } else {
                path.to_string()
            };
            return format!("{}{}{}", &endpoint[..idx + 3], host_port, redacted_path);
        }
    }
    endpoint.to_string()
}
