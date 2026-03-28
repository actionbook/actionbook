use std::process::Child;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::config;
use crate::config::DEFAULT_PROFILE;
use crate::daemon::browser;
use crate::daemon::cdp::{cdp_navigate, ensure_scheme};
use crate::daemon::cdp_session::CdpSession;
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
    pub executable_path: Option<String>,
    /// Open this URL on start
    #[arg(long)]
    pub open_url: Option<String>,
    /// Connect to existing CDP endpoint
    #[arg(long)]
    pub cdp_endpoint: Option<String>,
    /// Header for CDP endpoint (KEY:VALUE)
    #[arg(long)]
    pub header: Option<String>,
    /// Specify a semantic session ID
    #[arg(long)]
    pub set_session_id: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.start";

struct ReuseTarget {
    session_id: String,
    first_tab_id: String,
    cdp: Option<CdpSession>,
    cdp_port: u16,
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
            if let Err(e) = cdp_navigate(&page_ws, &ensure_scheme(url)).await {
                return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string())
                    .await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            targets = browser::list_targets(port).await.unwrap_or(targets);
        }

        (None, port, ws_url, targets)
    } else {
        let executable = if let Some(executable) = cmd.executable_path.as_deref() {
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
        (Some(chrome), port, ws_url, targets)
    };

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
        let final_url = ensure_scheme(url);
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
                return ActionResult::fatal(
                    "NAVIGATION_FAILED",
                    format!("reuse navigate failed: {e}"),
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    let targets = browser::list_targets(target.cdp_port)
        .await
        .unwrap_or_default();
    let (tab_url, tab_title) = get_tab_info_from_targets(&targets, &target.first_tab_id);

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
