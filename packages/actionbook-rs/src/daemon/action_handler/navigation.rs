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
            // Wait for page load to complete before fetching title
            let post_load_url =
                wait_for_page_load(backend, &target_id, from_url.clone(), true).await;

            // Update the tab registry with the post-load URL
            if let Some(entry) = regs.tabs.get_mut(&tab) {
                entry.url = post_load_url.clone();
            }

            // Fetch page title after load
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
                "to_url": post_load_url,
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
            // Wait for navigation to complete, then get post-load URL
            let to_url = wait_for_page_load(backend, &target_id, from_url.clone(), true).await;

            if let Some(entry) = regs.tabs.get_mut(&tab) {
                entry.url = to_url.clone();
            }

            // Fetch page title after load
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
            // Wait for reload to complete, then get post-load URL
            let to_url = wait_for_page_load(backend, &target_id, from_url.clone(), false).await;

            if let Some(entry) = regs.tabs.get_mut(&tab) {
                entry.url = to_url.clone();
            }

            // Fetch page title after load
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

/// Wait until the page finishes loading (up to 10s).
///
/// When `url_change_required` is true (goto/back/forward), only accepts
/// `readyState === "complete"` after the URL has changed from `fallback_url`,
/// guarding against seeing the old document's readyState before navigation commits.
///
/// When `url_change_required` is false (reload), accepts `readyState === "complete"`
/// regardless of URL change, since reload keeps the same URL.
///
/// On timeout, returns the last URL observed during polling.
async fn wait_for_page_load(
    backend: &mut dyn BackendSession,
    target_id: &str,
    fallback_url: String,
    url_change_required: bool,
) -> String {
    let poll_interval = std::time::Duration::from_millis(150);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);

    let mut last_seen_url = fallback_url.clone();
    let mut url_has_changed = false;

    loop {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: r#"(function(){ return { url: window.location.href, ready: document.readyState }; })()"#.to_string(),
            return_by_value: true,
        };
        if let Ok(result) = backend.exec(op).await {
            let val = extract_eval_value(&result.value);
            let ready = val.get("ready").and_then(|v| v.as_str()).unwrap_or("");
            let url = val
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if !url.is_empty() {
                last_seen_url = url.clone();
                if url != fallback_url {
                    url_has_changed = true;
                }
            }

            let ready_to_return = if url_change_required {
                // For goto/back/forward: require URL change first to avoid seeing old page
                ready == "complete" && url_has_changed
            } else {
                // For reload: URL stays same, just wait for complete
                ready == "complete"
            };

            if ready_to_return {
                return last_seen_url;
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return last_seen_url;
        }

        tokio::time::sleep(poll_interval).await;
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
