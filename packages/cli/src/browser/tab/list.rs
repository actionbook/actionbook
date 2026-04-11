use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;
use crate::types::Mode;

/// List tabs in a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser list-tabs --session my-session
  actionbook browser list-tabs --session my-session --json

Returns each tab's ID (t1, t2, ...), URL, and title.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser list-tabs";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    match result {
        ActionResult::Ok { .. } => Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: None,
            window_id: None,
            url: None,
            title: None,
        }),
        _ => None,
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Get CdpSession and mode from registry
    let (cdp, mode) = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(e) => match e.cdp.clone() {
                Some(c) => (c, e.mode),
                None => {
                    return ActionResult::fatal_with_hint(
                        "INTERNAL_ERROR",
                        format!("no CDP connection for session '{}'", cmd.session),
                        "try restarting the session",
                    );
                }
            },
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        }
    };

    // Fetch live tab list — method depends on session mode.
    // Extension mode uses Extension.listTabs; local/cloud uses Target.getTargets.
    let live_pages: Vec<(String, String, String)> = if mode == Mode::Extension {
        let resp = match cdp.execute_browser("Extension.listTabs", json!({})).await {
            Ok(r) => r,
            Err(e) => return cdp_error_to_result(e, "CDP_CONNECTION_FAILED"),
        };
        resp.pointer("/result/tabs")
            .and_then(|v| v.as_array())
            .map(|tabs| {
                tabs.iter()
                    .map(|t| {
                        let id = t
                            .get("id")
                            .and_then(|v| v.as_i64())
                            .map(|n| n.to_string())
                            .unwrap_or_default();
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
                        (id, url, title)
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        let resp = match cdp.execute_browser("Target.getTargets", json!({})).await {
            Ok(r) => r,
            Err(e) => return cdp_error_to_result(e, "CDP_CONNECTION_FAILED"),
        };
        resp.pointer("/result/targetInfos")
            .and_then(|v| v.as_array())
            .map(|infos| {
                infos
                    .iter()
                    .filter(|tgt| tgt.get("type").and_then(|v| v.as_str()) == Some("page"))
                    .map(|tgt| {
                        let native_id = tgt
                            .get("targetId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let url = tgt
                            .get("url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let title = tgt
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        (native_id, url, title)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    // Sync registry with live CDP state:
    // - Matching native_id → keep short tab ID, update url/title
    // - Stale registry tabs (not in CDP) → remove
    // - New CDP tabs (not in registry) → assign new short ID
    let (tabs, to_attach): (Vec<serde_json::Value>, Vec<String>) = {
        let mut reg = registry.lock().await;
        let entry = match reg.get_mut(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };

        // Remove stale tabs whose native_id no longer exists in CDP
        entry
            .tabs
            .retain(|t| live_pages.iter().any(|(nid, _, _)| *nid == t.native_id));

        let mut result = Vec::new();
        let mut to_attach = Vec::new();
        for (native_id, url, title) in &live_pages {
            // Find existing short ID or assign a new one
            if let Some(existing) = entry.tabs.iter_mut().find(|t| t.native_id == *native_id) {
                // Update url/title from live CDP data
                existing.url = url.to_string();
                existing.title = title.to_string();
                result.push(json!({
                    "tab_id": existing.id.0,
                    "native_tab_id": native_id,
                    "url": url,
                    "title": title,
                }));
            } else {
                // New tab — assign next short ID and mark for CDP attach
                entry.push_tab(native_id.to_string(), url.to_string(), title.to_string());
                let new_tab = entry.tabs.last().unwrap();
                to_attach.push(native_id.to_string());
                result.push(json!({
                    "tab_id": new_tab.id.0,
                    "native_tab_id": native_id,
                    "url": url,
                    "title": title,
                }));
            }
        }
        (result, to_attach)
    };

    // Attach newly discovered tabs outside the registry lock.
    // Extension mode uses register_extension_tab (no Target.attachToTarget);
    // local/cloud mode uses the standard CDP attach.
    for native_id in &to_attach {
        if mode == Mode::Extension {
            cdp.register_extension_tab(native_id).await;
        } else if let Err(e) = cdp.attach(native_id, None).await {
            tracing::warn!("failed to attach discovered tab {native_id}: {e}");
        }
    }

    ActionResult::ok(json!({
        "total_tabs": tabs.len(),
        "tabs": tabs,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::cdp_session::CdpSession;
    use crate::daemon::registry::{self, SessionEntry, SessionState};
    use crate::types::SessionId;
    use futures_util::{SinkExt, StreamExt};
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    async fn mock_ws() -> (
        String,
        tokio::sync::mpsc::Receiver<(
            futures_util::stream::SplitStream<
                tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
            >,
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
                Message,
            >,
        )>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let url = format!("ws://127.0.0.1:{}", addr.port());
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (writer, reader) = ws.split();
                if tx.send((reader, writer)).await.is_err() {
                    break;
                }
            }
        });
        (url, rx)
    }

    #[tokio::test]
    async fn extension_mode_uses_extension_list_tabs() {
        let (url, mut conns) = mock_ws().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        let registry = registry::new_shared_registry();
        {
            let mut reg = registry.lock().await;
            let mut entry = SessionEntry::starting(
                SessionId::new("s1").unwrap(),
                Mode::Extension,
                false,
                false,
                "default".to_string(),
            );
            entry.status = SessionState::Running;
            entry.cdp = Some(cdp.clone());
            reg.insert(entry);
        }

        let cmd = Cmd {
            session: "s1".to_string(),
        };

        let handle = tokio::spawn({
            let registry = registry.clone();
            async move { execute(&cmd, &registry).await }
        });

        // Read the CDP request — must be Extension.listTabs, NOT Target.getTargets
        let msg = loop {
            let raw = reader.next().await.unwrap().unwrap();
            if let Message::Text(t) = raw {
                let v: serde_json::Value = serde_json::from_str(t.as_ref()).unwrap();
                break v;
            }
        };
        let id = msg["id"].as_u64().unwrap();
        assert_eq!(
            msg["method"], "Extension.listTabs",
            "extension mode must call Extension.listTabs, not Target.getTargets"
        );

        // Reply with two tabs
        writer
            .send(Message::Text(
                json!({
                    "id": id,
                    "result": {
                        "tabs": [
                            { "id": 100, "url": "https://a.com", "title": "Tab A", "active": true, "windowId": 1 },
                            { "id": 200, "url": "https://b.com", "title": "Tab B", "active": false, "windowId": 1 },
                        ]
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

        let result = handle.await.unwrap();
        let data = match &result {
            ActionResult::Ok { data, .. } => data.clone(),
            other => panic!("expected Ok, got {other:?}"),
        };

        assert_eq!(data["total_tabs"], 2);
        let tabs = data["tabs"].as_array().unwrap();
        assert_eq!(tabs[0]["url"], "https://a.com");
        assert_eq!(tabs[0]["title"], "Tab A");
        assert_eq!(tabs[1]["url"], "https://b.com");
        assert_eq!(tabs[1]["title"], "Tab B");

        // Verify no Target.attachToTarget was sent — extension mode must use
        // register_extension_tab (in-memory only, no WS message).
        // Give a brief window for any stray messages to arrive.
        let stray =
            tokio::time::timeout(std::time::Duration::from_millis(100), reader.next()).await;
        assert!(
            stray.is_err(),
            "extension mode must not send Target.attachToTarget; got unexpected WS message"
        );

        // Verify tabs were registered in the registry with short IDs
        let reg = registry.lock().await;
        let entry = reg.get("s1").unwrap();
        assert_eq!(entry.tabs.len(), 2);
        assert_eq!(entry.tabs[0].native_id, "100");
        assert_eq!(entry.tabs[1].native_id, "200");
    }
}
