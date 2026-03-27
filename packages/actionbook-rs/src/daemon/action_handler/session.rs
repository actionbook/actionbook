use super::*;

const TAB_METADATA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TAB_METADATA_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

pub(super) async fn handle_list_tabs(
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
) -> ActionResult {
    refresh_tabs_from_targets(backend, regs).await;

    let mut tabs: Vec<serde_json::Value> = regs
        .tabs
        .values()
        .map(|t| {
            json!({
                "tab_id": t.id.to_string(),
                "url": t.url,
                "title": t.title,
                "native_tab_id": t.target_id,
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

            let navigate_op = BackendOp::Navigate {
                target_id: target_id.clone(),
                url: url.to_string(),
            };
            if let Err(e) = backend.exec(navigate_op).await {
                return cdp_error_to_result(e);
            }

            wait_for_tab_load(backend, &target_id).await;
            refresh_tab_from_backend(backend, regs, tab_id).await;
            let tab = regs
                .tabs
                .get(&tab_id)
                .cloned()
                .expect("newly inserted tab should exist");

            ActionResult::ok(json!({
                "tab": {
                    "tab_id": tab.id.to_string(),
                    "url": tab.url,
                    "title": tab.title,
                    "native_tab_id": tab.target_id,
                },
                "created": true,
                "new_window": new_window,
            }))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

async fn refresh_tabs_from_targets(backend: &mut dyn BackendSession, regs: &mut Registries) {
    let Ok(targets) = backend.list_targets().await else {
        return;
    };

    for entry in regs.tabs.values_mut() {
        if let Some(target) = targets
            .iter()
            .find(|target| target.target_id == entry.target_id)
        {
            entry.url = target.url.clone();
            entry.title = target.title.clone();
        }
    }
}

async fn refresh_tab_from_backend(
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    tab_id: TabId,
) {
    let Some(target_id) = regs.tabs.get(&tab_id).map(|entry| entry.target_id.clone()) else {
        return;
    };

    if let Some(url) = fetch_string_value(backend, &target_id, "window.location.href").await {
        if let Some(entry) = regs.tabs.get_mut(&tab_id) {
            entry.url = url;
        }
    }

    if let Some(title) = fetch_string_value(backend, &target_id, "document.title").await {
        if let Some(entry) = regs.tabs.get_mut(&tab_id) {
            entry.title = title;
        }
    }
}

async fn wait_for_tab_load(backend: &mut dyn BackendSession, target_id: &str) {
    let deadline = tokio::time::Instant::now() + TAB_METADATA_TIMEOUT;
    loop {
        if let Some(ready_state) =
            fetch_string_value(backend, target_id, "document.readyState").await
        {
            if ready_state == "complete" {
                return;
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return;
        }

        tokio::time::sleep(TAB_METADATA_POLL_INTERVAL).await;
    }
}

async fn fetch_string_value(
    backend: &mut dyn BackendSession,
    target_id: &str,
    expression: &str,
) -> Option<String> {
    let result = backend
        .exec(BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: expression.to_string(),
            return_by_value: true,
        })
        .await
        .ok()?;

    extract_eval_value(&result.value)
        .as_str()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
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
