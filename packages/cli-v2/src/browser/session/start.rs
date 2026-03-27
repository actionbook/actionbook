use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::browser;
use crate::daemon::cdp::{cdp_navigate, ensure_scheme};
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
    let mut reg = registry.lock().await;
    let profile_name = cmd.profile.as_deref().unwrap_or("actionbook");

    // Local mode: 1 profile = max 1 session. Reuse existing if same profile.
    if cmd.mode == Mode::Local
        && let Some(session_id) = reg
            .list()
            .iter()
            .find(|s| s.profile == profile_name && s.mode == cmd.mode)
            .map(|s| s.id.as_str().to_string())
    {
        if let Some(url) = &cmd.open_url {
            let final_url = ensure_scheme(url);
            let entry = reg.get_mut(&session_id).unwrap();
            let first_tab = entry.tabs.first();

            if let Some(tab) = first_tab {
                let ws_url = if !tab.target_id.is_empty() {
                    Some(format!(
                        "ws://127.0.0.1:{}/devtools/page/{}",
                        entry.cdp_port, tab.target_id
                    ))
                } else {
                    None
                };
                let tab_info = (tab.id, tab.target_id.clone());
                drop(reg);
                if let Some(ref ws) = ws_url {
                    if let Err(e) = cdp_navigate(ws, &final_url).await {
                        return ActionResult::fatal(
                            "NAVIGATION_FAILED",
                            format!("reuse navigate failed: {e}"),
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                let mut reg = registry.lock().await;
                let entry = reg.get_mut(&session_id).unwrap();
                if let Ok(targets) = browser::list_targets(entry.cdp_port).await {
                    for target in &targets {
                        let tid = target.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        if tid == tab_info.1 {
                            if let Some(tab) = entry.tabs.iter_mut().find(|t| t.id == tab_info.0) {
                                tab.url = target
                                    .get("url")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                tab.title = target
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                            }
                            break;
                        }
                    }
                } else if let Some(tab) = entry.tabs.iter_mut().find(|t| t.id == tab_info.0) {
                    tab.url = final_url.clone();
                }
                let tab = entry.tabs.first().unwrap();
                return ActionResult::ok(json!({
                    "session": {
                        "session_id": entry.id.as_str(),
                        "mode": entry.mode.to_string(),
                        "status": entry.status,
                        "headless": entry.headless,
                        "cdp_endpoint": entry.ws_url,
                    },
                    "tab": {
                        "tab_id": tab.id.to_string(),
                        "url": tab.url,
                        "title": tab.title,
                        "native_tab_id": if tab.target_id.is_empty() { serde_json::Value::Null } else { json!(tab.target_id) },
                    },
                    "reused": true,
                }));
            }
        }

        let entry = reg.get(&session_id).unwrap();
        let first_tab = entry.tabs.first();
        return ActionResult::ok(json!({
            "session": {
                "session_id": entry.id.as_str(),
                "mode": entry.mode.to_string(),
                "status": entry.status,
                "headless": entry.headless,
                "cdp_endpoint": entry.ws_url,
            },
            "tab": {
                "tab_id": first_tab.map(|t| t.id.to_string()).unwrap_or_else(|| "t1".to_string()),
                "url": first_tab.map(|t| t.url.as_str()).unwrap_or(""),
                "title": first_tab.map(|t| t.title.as_str()).unwrap_or(""),
                "native_tab_id": first_tab.map(|t| if t.target_id.is_empty() { serde_json::Value::Null } else { json!(t.target_id) }).unwrap_or(serde_json::Value::Null),
            },
            "reused": true,
        }));
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
    let mut next_tab_id = 1u32;
    for t in &targets {
        let target_id = t
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
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
            id: TabId(next_tab_id),
            target_id,
            url,
            title,
        });
        next_tab_id += 1;
    }

    if tabs.is_empty() {
        tabs.push(TabEntry {
            id: TabId(1),
            target_id: String::new(),
            url: cmd.open_url.as_deref().unwrap_or("about:blank").to_string(),
            title: String::new(),
        });
        next_tab_id = 2;
    }

    let first_tab = tabs[0].clone();

    let entry = SessionEntry {
        id: session_id.clone(),
        mode: cmd.mode,
        headless: cmd.headless,
        profile: profile_name.to_string(),
        status: "running".to_string(),
        cdp_port: port,
        ws_url: ws_url.clone(),
        tabs,
        next_tab_id,
        chrome_process: Some(chrome),
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
            "tab_id": first_tab.id.to_string(),
            "url": first_tab.url,
            "title": first_tab.title,
            "native_tab_id": if first_tab.target_id.is_empty() { serde_json::Value::Null } else { json!(first_tab.target_id) },
        },
        "reused": false,
    }))
}
