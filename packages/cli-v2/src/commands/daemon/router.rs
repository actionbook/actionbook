use serde_json::json;

use crate::action::Action;
use crate::action_result::ActionResult;
use crate::error::CliError;
use crate::types::TabId;

use super::browser;
use super::registry::{SessionEntry, SharedRegistry, TabEntry};

/// Route an action to the appropriate handler.
pub async fn route(action: &Action, registry: &SharedRegistry) -> ActionResult {
    match action {
        Action::StartSession {
            mode,
            headless,
            profile,
            open_url,
            cdp_endpoint: _,
            set_session_id,
        } => handle_start(
            *mode,
            *headless,
            profile.as_deref(),
            open_url.as_deref(),
            set_session_id.as_deref(),
            registry,
        )
        .await,
        Action::ListSessions => handle_list_sessions(registry).await,
        Action::SessionStatus { session_id } => {
            handle_status(session_id, registry).await
        }
        Action::Close { session_id } => handle_close(session_id, registry).await,
        Action::Restart { session_id } => handle_restart(session_id, registry).await,
        Action::Goto {
            session_id,
            tab_id,
            url,
        } => handle_goto(session_id, tab_id, url, registry).await,
        Action::NewTab {
            session_id,
            url,
            ..
        } => handle_new_tab(session_id, url, registry).await,
        Action::Eval {
            session_id,
            tab_id,
            expression,
        } => handle_cdp_eval(session_id, tab_id, expression, registry).await,
        Action::Snapshot {
            session_id,
            tab_id,
        } => handle_snapshot(session_id, tab_id, registry).await,
        _ => ActionResult::fatal("UNSUPPORTED_OPERATION", "not yet implemented"),
    }
}

async fn handle_start(
    mode: crate::types::Mode,
    headless: bool,
    profile: Option<&str>,
    open_url: Option<&str>,
    set_session_id: Option<&str>,
    registry: &SharedRegistry,
) -> ActionResult {
    let mut reg = registry.lock().await;

    let session_id = match reg.generate_session_id(set_session_id, profile) {
        Ok(id) => id,
        Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
    };

    let executable = match browser::find_chrome() {
        Ok(e) => e,
        Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
    };

    let port = browser::find_available_port();
    let profile_name = profile.unwrap_or("actionbook");

    // Fix #7: validate profile name — reject path traversal
    if profile_name.contains('/') || profile_name.contains('\\') || profile_name.contains("..") {
        return ActionResult::fatal("INVALID_ARGUMENT", format!("invalid profile name: {profile_name}"));
    }

    let data_dir = std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            format!("{home}/.local/share")
        });
    let user_data_dir = format!("{data_dir}/actionbook/profiles/{profile_name}");
    std::fs::create_dir_all(&user_data_dir).ok();

    let mut chrome = match browser::launch_chrome(&executable, port, headless, &user_data_dir, open_url) {
        Ok(c) => c,
        Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
    };

    // Fix #3: kill Chrome if subsequent setup fails
    let ws_url = match browser::discover_ws_url(port).await {
        Ok(ws) => ws,
        Err(e) => {
            let _ = chrome.kill();
            let _ = chrome.wait();
            return ActionResult::fatal(e.error_code(), e.to_string());
        }
    };

    let targets = browser::list_targets(port).await.unwrap_or_default();
    let mut tabs = Vec::new();
    let mut next_tab_id = 1u32;
    for t in &targets {
        let target_id = t.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
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
            url: open_url.unwrap_or("about:blank").to_string(),
            title: String::new(),
        });
        next_tab_id = 2;
    }

    let first_tab = tabs[0].clone();

    let entry = SessionEntry {
        id: session_id.clone(),
        mode,
        headless,
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
            "mode": mode.to_string(),
            "status": "running",
            "headless": headless,
            "cdp_endpoint": ws_url,
        },
        "tab": {
            "tab_id": first_tab.id.to_string(),
            "url": first_tab.url,
            "title": first_tab.title,
            "native_tab_id": serde_json::Value::Null,
        },
        "reused": false,
    }))
}

async fn handle_list_sessions(registry: &SharedRegistry) -> ActionResult {
    let reg = registry.lock().await;
    let sessions: Vec<serde_json::Value> = reg
        .list()
        .iter()
        .map(|s| {
            json!({
                "session_id": s.id.as_str(),
                "mode": s.mode.to_string(),
                "status": s.status,
                "headless": s.headless,
                "tabs_count": s.tabs_count(),
            })
        })
        .collect();
    ActionResult::ok(json!({
        "total_sessions": sessions.len(),
        "sessions": sessions,
    }))
}

async fn handle_status(session_id: &str, registry: &SharedRegistry) -> ActionResult {
    let reg = registry.lock().await;
    let entry = match reg.get(session_id) {
        Some(e) => e,
        None => {
            return ActionResult::fatal_with_hint(
                "SESSION_NOT_FOUND",
                format!("session '{session_id}' not found"),
                "run `actionbook browser list-sessions` to see available sessions",
            );
        }
    };
    let tabs: Vec<serde_json::Value> = entry
        .tabs
        .iter()
        .map(|t| {
            json!({
                "tab_id": t.id.to_string(),
                "url": t.url,
                "title": t.title,
            })
        })
        .collect();
    ActionResult::ok(json!({
        "session": {
            "session_id": entry.id.as_str(),
            "mode": entry.mode.to_string(),
            "status": entry.status,
            "headless": entry.headless,
            "tabs_count": entry.tabs_count(),
        },
        "tabs": tabs,
        "capabilities": {
            "snapshot": true,
            "pdf": true,
            "upload": true,
        },
    }))
}

async fn handle_close(session_id: &str, registry: &SharedRegistry) -> ActionResult {
    let mut reg = registry.lock().await;
    let mut entry = match reg.remove(session_id) {
        Some(e) => e,
        None => {
            return ActionResult::fatal_with_hint(
                "SESSION_NOT_FOUND",
                format!("session '{session_id}' not found"),
                "run `actionbook browser list-sessions` to see available sessions",
            );
        }
    };
    let closed_tabs = entry.tabs_count();

    if let Some(mut child) = entry.chrome_process.take() {
        let _ = child.kill();
        // Avoid blocking async runtime — wait in a blocking thread
        tokio::task::spawn_blocking(move || { let _ = child.wait(); });
    }

    ActionResult::ok(json!({
        "session_id": session_id,
        "status": "closed",
        "closed_tabs": closed_tabs,
    }))
}

async fn handle_restart(session_id: &str, registry: &SharedRegistry) -> ActionResult {
    let (mode, headless, profile, open_url);
    {
        let mut reg = registry.lock().await;
        let mut entry = match reg.remove(session_id) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{session_id}' not found"),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };
        mode = entry.mode;
        headless = entry.headless;
        profile = entry.profile.clone();
        open_url = entry.tabs.first().map(|t| t.url.clone());

        if let Some(ref mut child) = entry.chrome_process {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    let result = handle_start(
        mode,
        headless,
        Some(&profile),
        open_url.as_deref(),
        Some(session_id),
        registry,
    )
    .await;

    match result {
        ActionResult::Ok { data } => {
            // Build restart response per §7.5: session includes tabs_count
            let mut session = data.get("session").cloned().unwrap_or(json!({}));
            // Add tabs_count (start response doesn't include it, but restart needs it)
            if session.get("tabs_count").is_none() {
                session["tabs_count"] = json!(1);
            }
            ActionResult::ok(json!({
                "session": session,
                "reopened": true,
            }))
        }
        other => other,
    }
}

async fn handle_goto(
    session_id: &str,
    tab_id: &str,
    url: &str,
    registry: &SharedRegistry,
) -> ActionResult {
    let final_url = ensure_scheme(url);

    // Extract data needed for CDP, then release lock
    let ws_url = {
        let reg = registry.lock().await;
        let entry = match reg.get(session_id) {
            Some(e) => e,
            None => return ActionResult::fatal("SESSION_NOT_FOUND", format!("session '{session_id}' not found")),
        };
        let parsed_tab: TabId = match tab_id.parse() {
            Ok(t) => t,
            Err(e) => return ActionResult::fatal("INVALID_ARGUMENT", format!("invalid tab id: {e}")),
        };
        let tab = match entry.tabs.iter().find(|t| t.id == parsed_tab) {
            Some(t) => t,
            None => return ActionResult::fatal("TAB_NOT_FOUND", format!("tab '{tab_id}' not found")),
        };
        if tab.target_id.is_empty() {
            None
        } else {
            Some(format!("ws://127.0.0.1:{}/devtools/page/{}", entry.cdp_port, tab.target_id))
        }
    }; // lock released

    // CDP I/O without holding lock
    if let Some(ref ws) = ws_url {
        let _ = cdp_navigate(ws, &final_url).await;
    }

    // Re-acquire lock to update tab URL
    {
        let mut reg = registry.lock().await;
        if let Some(entry) = reg.get_mut(session_id) {
            if let Ok(parsed_tab) = tab_id.parse::<TabId>() {
                if let Some(tab) = entry.tabs.iter_mut().find(|t| t.id == parsed_tab) {
                    tab.url.clone_from(&final_url);
                }
            }
        }
    }

    ActionResult::ok(json!({
        "kind": "goto",
        "to_url": final_url,
    }))
}

async fn handle_new_tab(
    session_id: &str,
    url: &str,
    registry: &SharedRegistry,
) -> ActionResult {
    let final_url = ensure_scheme(url);

    // Extract port, release lock before HTTP I/O
    let cdp_port = {
        let reg = registry.lock().await;
        match reg.get(session_id) {
            Some(e) => e.cdp_port,
            None => return ActionResult::fatal("SESSION_NOT_FOUND", format!("session '{session_id}' not found")),
        }
    }; // lock released

    // Fix #10: URL-encode the target URL for /json/new
    let create_url = format!(
        "http://127.0.0.1:{}/json/new?{}",
        cdp_port,
        urlencoding::encode(&final_url)
    );
    let target_id = match reqwest::get(&create_url).await {
        Ok(resp) => resp
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(|s| s.to_string()))
            .unwrap_or_default(),
        Err(_) => String::new(),
    };

    // Re-acquire lock to insert tab
    let tab_id = {
        let mut reg = registry.lock().await;
        let entry = match reg.get_mut(session_id) {
            Some(e) => e,
            None => return ActionResult::fatal("SESSION_NOT_FOUND", format!("session '{session_id}' not found")),
        };
        let tid = TabId(entry.next_tab_id);
        entry.next_tab_id += 1;
        entry.tabs.push(TabEntry {
            id: tid,
            target_id,
            url: final_url.clone(),
            title: String::new(),
        });
        tid
    };

    ActionResult::ok(json!({
        "tab_id": tab_id.to_string(),
        "url": final_url,
    }))
}

/// Resolve WebSocket URL for a tab, releasing the lock.
fn resolve_tab_ws_url(
    _session_id: &str,
    tab_id: &str,
    entry: &super::registry::SessionEntry,
) -> Result<String, ActionResult> {
    let parsed_tab: TabId = tab_id.parse().map_err(|e| {
        ActionResult::fatal("INVALID_ARGUMENT", format!("invalid tab id: {e}"))
    })?;
    let tab = entry.tabs.iter().find(|t| t.id == parsed_tab).ok_or_else(|| {
        ActionResult::fatal("TAB_NOT_FOUND", format!("tab '{tab_id}' not found"))
    })?;
    Ok(if !tab.target_id.is_empty() {
        format!("ws://127.0.0.1:{}/devtools/page/{}", entry.cdp_port, tab.target_id)
    } else {
        entry.ws_url.clone()
    })
}

/// Runtime.evaluate via CDP WebSocket.
async fn handle_cdp_eval(
    session_id: &str,
    tab_id: &str,
    expression: &str,
    registry: &SharedRegistry,
) -> ActionResult {
    // Extract WS URL then release lock before CDP I/O
    let ws_url = {
        let reg = registry.lock().await;
        let entry = match reg.get(session_id) {
            Some(e) => e,
            None => return ActionResult::fatal("SESSION_NOT_FOUND", format!("session '{session_id}' not found")),
        };
        match resolve_tab_ws_url(session_id, tab_id, entry) {
            Ok(url) => url,
            Err(err) => return err,
        }
    }; // lock released

    match cdp_runtime_evaluate(&ws_url, expression).await {
        Ok(value) => ActionResult::ok(json!({ "value": value })),
        Err(e) => ActionResult::fatal("EVAL_FAILED", e.to_string()),
    }
}

async fn handle_snapshot(
    session_id: &str,
    tab_id: &str,
    registry: &SharedRegistry,
) -> ActionResult {
    // Extract WS URL then release lock before CDP I/O
    let ws_url = {
        let reg = registry.lock().await;
        let entry = match reg.get(session_id) {
            Some(e) => e,
            None => return ActionResult::fatal("SESSION_NOT_FOUND", format!("session '{session_id}' not found")),
        };
        match resolve_tab_ws_url(session_id, tab_id, entry) {
            Ok(url) => url,
            Err(err) => return err,
        }
    };

    match cdp_get_ax_tree(&ws_url).await {
        Ok(snapshot) => ActionResult::ok(json!({ "snapshot": snapshot })),
        Err(e) => ActionResult::fatal("INTERNAL_ERROR", e.to_string()),
    }
}

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

fn ws_text(s: String) -> Message {
    Message::Text(s.into())
}

fn msg_to_string(msg: &Message) -> Option<String> {
    match msg {
        Message::Text(t) => Some(t.to_string()),
        _ => None,
    }
}

/// CDP Runtime.evaluate via WebSocket.
async fn cdp_runtime_evaluate(ws_url: &str, expression: &str) -> Result<String, CliError> {
    let (mut ws, _) = connect_async(ws_url)
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

    let msg = json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": { "expression": expression, "returnByValue": true }
    });
    ws.send(ws_text(msg.to_string())).await.map_err(|e| CliError::CdpError(e.to_string()))?;

    while let Some(raw) = ws.next().await {
        let raw = raw.map_err(|e| CliError::CdpError(e.to_string()))?;
        if let Some(text) = msg_to_string(&raw) {
            let resp: serde_json::Value = serde_json::from_str(&text).map_err(|e| CliError::CdpError(e.to_string()))?;
            if resp.get("id").and_then(|v| v.as_u64()) == Some(1) {
                if let Some(result) = resp.get("result").and_then(|r| r.get("result")) {
                    let value = result.get("value").map(|v| {
                        if v.is_string() { v.as_str().unwrap().to_string() } else { v.to_string() }
                    }).unwrap_or_default();
                    let _ = ws.close(None).await;
                    return Ok(value);
                }
                if let Some(exc) = resp.get("result").and_then(|r| r.get("exceptionDetails")) {
                    let emsg = exc.get("text").and_then(|v| v.as_str()).unwrap_or("expression error");
                    let _ = ws.close(None).await;
                    return Err(CliError::EvalFailed(emsg.to_string()));
                }
            }
        }
    }
    Err(CliError::CdpError("no response from CDP".to_string()))
}

/// CDP Page.navigate via WebSocket.
async fn cdp_navigate(ws_url: &str, url: &str) -> Result<(), CliError> {
    let (mut ws, _) = connect_async(ws_url)
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

    let msg = json!({ "id": 1, "method": "Page.navigate", "params": { "url": url } });
    ws.send(ws_text(msg.to_string())).await.map_err(|e| CliError::CdpError(e.to_string()))?;

    while let Some(raw) = ws.next().await {
        let raw = raw.map_err(|e| CliError::CdpError(e.to_string()))?;
        if let Some(text) = msg_to_string(&raw) {
            let resp: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            if resp.get("id").and_then(|v| v.as_u64()) == Some(1) {
                let _ = ws.close(None).await;
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Get accessibility tree via CDP.
async fn cdp_get_ax_tree(ws_url: &str) -> Result<String, CliError> {
    let (mut ws, _) = connect_async(ws_url)
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

    let msg = json!({ "id": 1, "method": "Accessibility.getFullAXTree", "params": {} });
    ws.send(ws_text(msg.to_string())).await.map_err(|e| CliError::CdpError(e.to_string()))?;

    while let Some(raw) = ws.next().await {
        let raw = raw.map_err(|e| CliError::CdpError(e.to_string()))?;
        if let Some(text) = msg_to_string(&raw) {
            let resp: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            if resp.get("id").and_then(|v| v.as_u64()) == Some(1) {
                let _ = ws.close(None).await;
                return Ok(text);
            }
        }
    }
    Err(CliError::CdpError("no response".to_string()))
}

fn ensure_scheme(url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else if url.starts_with("about:") || url.starts_with("data:") || url.starts_with("chrome:") || url.starts_with("javascript:") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}
