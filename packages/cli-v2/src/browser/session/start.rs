use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::config;
use crate::config::DEFAULT_PROFILE;
use crate::daemon::browser;
use crate::daemon::cdp::{cdp_navigate, ensure_scheme};
use crate::daemon::cdp_session::CdpSession;
use crate::daemon::registry::{SessionEntry, SharedRegistry, TabEntry};
use crate::output::ResponseContext;
use crate::types::{Mode, TabId};

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
    /// Header for CDP endpoint (KEY:VALUE)
    #[arg(long)]
    pub header: Option<String>,
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
    let mode = cmd.mode.unwrap_or(Mode::Local);
    let headless = cmd.headless.unwrap_or(false);
    let profile_name = cmd.profile.as_deref().unwrap_or(DEFAULT_PROFILE);
    let cdp_endpoint = cmd.cdp_endpoint.as_deref();

    let mut reg = registry.lock().await;

    // Local mode: 1 profile = max 1 session. Reuse existing if same profile.
    if cdp_endpoint.is_none()
        && mode == Mode::Local
        && let Some(session_id) = reg
            .list()
            .iter()
            .find(|s| s.profile == profile_name && s.mode == mode)
            .map(|s| s.id.as_str().to_string())
    {
        if let Some(url) = &cmd.open_url {
            let final_url = ensure_scheme(url);
            let entry = reg.get(&session_id).unwrap();
            let first_tab_id = entry
                .tabs
                .first()
                .map(|t| t.id.0.clone())
                .unwrap_or_default();
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
            let targets = browser::list_targets(cdp_port).await.unwrap_or_default();
            let (tab_url, tab_title) = get_tab_info_from_targets(&targets, &first_tab_id);

            let reg = registry.lock().await;
            let entry = reg.get(&session_id).unwrap();
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

        // Reuse without open-url: fetch real-time info
        let entry = reg.get(&session_id).unwrap();
        let first_tab_id = entry
            .tabs
            .first()
            .map(|t| t.id.0.clone())
            .unwrap_or_default();
        let cdp_port = entry.cdp_port;
        drop(reg);
        let targets = browser::list_targets(cdp_port).await.unwrap_or_default();
        let (tab_url, tab_title) = get_tab_info_from_targets(&targets, &first_tab_id);
        let reg = registry.lock().await;
        let entry = reg.get(&session_id).unwrap();
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

    let session_id =
        match reg.generate_session_id(cmd.set_session_id.as_deref(), cmd.profile.as_deref()) {
            Ok(id) => id,
            Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
        };

    if profile_name.contains('/') || profile_name.contains('\\') || profile_name.contains("..") {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid profile name: {profile_name}"),
        );
    }

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
        if mode != Mode::Local {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                "cdp-endpoint requires --mode local".to_string(),
            );
        }

        let (ws_url, port) = match browser::resolve_cdp_endpoint(endpoint).await {
            Ok(value) => value,
            Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
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
                return ActionResult::fatal(e.error_code(), e.to_string());
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            targets = browser::list_targets(port).await.unwrap_or(targets);
        }

        (None, port, ws_url, targets)
    } else {
        let executable = if let Some(executable) = cmd.executable.as_deref() {
            executable.to_string()
        } else {
            match browser::find_chrome() {
                Ok(e) => e,
                Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
            }
        };

        let (mut chrome, port) = match browser::launch_chrome(
            &executable,
            headless,
            &user_data_dir.to_string_lossy(),
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
            if let Some(mut chrome) = chrome_process.take() {
                let _ = chrome.kill();
                let _ = chrome.wait();
            }
            return ActionResult::fatal("CDP_CONNECTION_FAILED", e.to_string());
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

    let entry = SessionEntry {
        id: session_id.clone(),
        mode,
        headless,
        profile: profile_name.to_string(),
        status: "running".to_string(),
        cdp_port: port,
        ws_url: ws_url.clone(),
        tabs,
        chrome_process,
        cdp: Some(cdp),
    };
    reg.insert(entry);

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
