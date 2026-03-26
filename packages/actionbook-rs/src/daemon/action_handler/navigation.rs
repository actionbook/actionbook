use super::*;

pub(super) async fn handle_goto(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    tab: TabId,
    url: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t.to_string(),
        Err(r) => return r,
    };

    let from_url = regs
        .tabs
        .get(&tab)
        .map(|e| e.url.clone())
        .unwrap_or_default();

    let op = BackendOp::Navigate {
        target_id: target_id.clone(),
        url: url.to_string(),
    };

    match backend.exec(op).await {
        Ok(_) => {
            // Update the tab registry with the new URL
            if let Some(entry) = regs.tabs.get_mut(&tab) {
                entry.url = url.to_string();
            }

            // Fetch page title for context
            let title = fetch_title(backend, &target_id).await;
            if let Some(ref t) = title {
                if let Some(entry) = regs.tabs.get_mut(&tab) {
                    entry.title = t.clone();
                }
            }

            ActionResult::ok(json!({
                "kind": "goto",
                "requested_url": url,
                "from_url": from_url,
                "to_url": url,
                "title": title.unwrap_or_default(),
            }))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_history(
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    session_id: SessionId,
    tab: TabId,
    direction: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t.to_string(),
        Err(r) => return r,
    };

    let from_url = regs
        .tabs
        .get(&tab)
        .map(|e| e.url.clone())
        .unwrap_or_default();

    let op = BackendOp::Evaluate {
        target_id: target_id.clone(),
        expression: format!("history.{direction}()"),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(_) => {
            // Update the tab registry URL after navigation.
            let mut to_url = from_url.clone();
            let url_op = BackendOp::Evaluate {
                target_id: target_id.clone(),
                expression: "window.location.href".to_string(),
                return_by_value: true,
            };
            if let Ok(url_val) = backend.exec(url_op).await {
                if let Some(url) = url_val.value.as_str() {
                    to_url = url.to_string();
                    if let Some(entry) = regs.tabs.get_mut(&tab) {
                        entry.url = url.to_string();
                    }
                }
            }
            // Fetch page title for context
            let title = fetch_title(backend, &target_id).await;
            if let Some(ref t) = title {
                if let Some(entry) = regs.tabs.get_mut(&tab) {
                    entry.title = t.clone();
                }
            }

            ActionResult::ok(json!({
                "kind": direction,
                "from_url": from_url,
                "to_url": to_url,
                "title": title.unwrap_or_default(),
            }))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_reload(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &mut Registries,
    tab: TabId,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t.to_string(),
        Err(r) => return r,
    };

    let from_url = regs
        .tabs
        .get(&tab)
        .map(|e| e.url.clone())
        .unwrap_or_default();

    let op = BackendOp::Evaluate {
        target_id: target_id.clone(),
        expression: "location.reload()".to_string(),
        return_by_value: true,
    };

    match backend.exec(op).await {
        Ok(_) => {
            // Update the tab registry URL after reload (URL may have changed due to redirects).
            let mut to_url = from_url.clone();
            let url_op = BackendOp::Evaluate {
                target_id: target_id.clone(),
                expression: "window.location.href".to_string(),
                return_by_value: true,
            };
            if let Ok(url_val) = backend.exec(url_op).await {
                if let Some(url) = url_val.value.as_str() {
                    to_url = url.to_string();
                    if let Some(entry) = regs.tabs.get_mut(&tab) {
                        entry.url = url.to_string();
                    }
                }
            }
            // Fetch page title for context
            let title = fetch_title(backend, &target_id).await;
            if let Some(ref t) = title {
                if let Some(entry) = regs.tabs.get_mut(&tab) {
                    entry.title = t.clone();
                }
            }

            ActionResult::ok(json!({
                "kind": "reload",
                "from_url": from_url,
                "to_url": to_url,
                "title": title.unwrap_or_default(),
            }))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

/// Fetch `document.title` from the target. Returns `None` on failure.
async fn fetch_title(backend: &mut dyn BackendSession, target_id: &str) -> Option<String> {
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: "document.title".to_string(),
        return_by_value: true,
    };
    backend
        .exec(op)
        .await
        .ok()
        .and_then(|v| v.value.as_str().map(String::from))
        .filter(|s| !s.is_empty())
}
