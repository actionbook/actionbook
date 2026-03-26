use super::*;

pub(super) fn handle_list_tabs(regs: &Registries) -> ActionResult {
    let mut tabs: Vec<serde_json::Value> = regs
        .tabs
        .values()
        .map(|t| {
            json!({
                "tab_id": t.id.to_string(),
                "url": t.url,
                "title": t.title,
            })
        })
        .collect();
    tabs.sort_by(|a, b| a["tab_id"].as_str().cmp(&b["tab_id"].as_str()));
    let total_tabs = tabs.len();
    ActionResult::ok(json!({"total_tabs": total_tabs, "tabs": tabs}))
}

pub(super) fn handle_list_windows(regs: &Registries) -> ActionResult {
    let mut windows: Vec<serde_json::Value> = regs
        .windows
        .values()
        .map(|w| {
            json!({
                "id": w.id.to_string(),
                "tabs": w.tabs.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            })
        })
        .collect();
    windows.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    ActionResult::ok(json!({"windows": windows}))
}

pub(super) async fn handle_new_tab(
    _session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    url: &str,
    new_window: bool,
    window: Option<WindowId>,
) -> ActionResult {
    let op = BackendOp::CreateTarget {
        url: url.to_string(),
        window_id: window.map(|w| w.0 as i64),
        new_window,
    };

    match backend.exec(op).await {
        Ok(result) => {
            let target_id = result
                .value
                .get("targetId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if target_id.is_empty() {
                return ActionResult::fatal(
                    "create_target_failed",
                    "Target.createTarget did not return a targetId",
                    "check browser logs",
                );
            }

            let win_id = if new_window {
                let wid = regs.alloc_window_id();
                regs.windows.insert(
                    wid,
                    WindowEntry {
                        id: wid,
                        tabs: Vec::new(),
                    },
                );
                wid
            } else if let Some(w) = window {
                regs.windows.entry(w).or_insert_with(|| WindowEntry {
                    id: w,
                    tabs: Vec::new(),
                });
                w
            } else {
                regs.windows
                    .keys()
                    .min_by_key(|w| w.0)
                    .copied()
                    .unwrap_or_else(|| {
                        let wid = regs.alloc_window_id();
                        regs.windows.insert(
                            wid,
                            WindowEntry {
                                id: wid,
                                tabs: Vec::new(),
                            },
                        );
                        wid
                    })
            };

            let tab_id = regs.alloc_tab_id();
            regs.tabs.insert(
                tab_id,
                TabEntry {
                    id: tab_id,
                    target_id: target_id.clone(),
                    window: win_id,
                    url: url.to_string(),
                    title: String::new(),
                },
            );
            if let Some(win) = regs.windows.get_mut(&win_id) {
                win.tabs.push(tab_id);
            }

            ActionResult::ok(json!({
                "tab": tab_id.to_string(),
                "target_id": target_id,
                "window": win_id.to_string(),
                "url": url,
            }))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_close_tab(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t.to_string(),
        Err(r) => return r,
    };

    let op = BackendOp::CloseTarget { target_id };

    match backend.exec(op).await {
        Ok(_) => {
            if let Some(entry) = regs.tabs.remove(&tab) {
                if let Some(win) = regs.windows.get_mut(&entry.window) {
                    win.tabs.retain(|t| *t != tab);
                }
            }
            ActionResult::ok(json!({"closed_tab_id": tab.to_string()}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}
