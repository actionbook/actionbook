use super::*;

pub(super) async fn handle_wait_navigation(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    timeout_ms: Option<u64>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
    let poll_interval = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + timeout;

    // Get the current URL first
    let get_url_js = "window.location.href".to_string();
    let initial_url = {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: get_url_js.clone(),
            return_by_value: true,
        };
        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                val.as_str().unwrap_or("").to_string()
            }
            Err(e) => return cdp_error_to_result(e),
        }
    };

    // Poll until URL changes or document.readyState is complete
    loop {
        let check_js = r#"(function() {
                const url = window.location.href;
                const ready = document.readyState;
                return { url: url, ready: ready };
            })()"#
            .to_string();
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: check_js,
            return_by_value: true,
        };

        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                let current_url = val.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let ready = val.get("ready").and_then(|v| v.as_str()).unwrap_or("");
                if current_url != initial_url || ready == "complete" {
                    return ActionResult::ok(json!({
                        "navigated": true,
                        "url": current_url,
                        "readyState": ready,
                    }));
                }
            }
            Err(e) => return cdp_error_to_result(e),
        }

        if tokio::time::Instant::now() >= deadline {
            return ActionResult::retryable(
                "navigation_timeout",
                format!(
                    "navigation did not complete within {}ms",
                    timeout.as_millis()
                ),
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub(super) async fn handle_wait_network_idle(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    timeout_ms: Option<u64>,
    idle_time_ms: Option<u64>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
    let idle_time = idle_time_ms.unwrap_or(500);
    let poll_interval = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + timeout;

    // Use JS Performance API to detect ongoing requests
    let check_js = format!(
        r#"(function() {{
            const entries = performance.getEntriesByType('resource');
            const now = performance.now();
            const recent = entries.filter(e => now - e.responseEnd < {idle_time});
            return {{ pending: recent.length, now: now }};
        }})()"#
    );

    loop {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: check_js.clone(),
            return_by_value: true,
        };

        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                let pending = val.get("pending").and_then(|v| v.as_i64()).unwrap_or(1);
                if pending == 0 {
                    return ActionResult::ok(json!({"network_idle": true}));
                }
            }
            Err(e) => return cdp_error_to_result(e),
        }

        if tokio::time::Instant::now() >= deadline {
            return ActionResult::retryable(
                "network_idle_timeout",
                format!(
                    "network did not become idle within {}ms",
                    timeout.as_millis()
                ),
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub(super) async fn handle_wait_condition(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    expression: &str,
    timeout_ms: Option<u64>,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
    let poll_interval = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: expression.to_string(),
            return_by_value: true,
        };

        match backend.exec(op).await {
            Ok(result) => {
                let val = extract_eval_value(&result.value);
                // Check for truthiness
                let truthy = match &val {
                    serde_json::Value::Bool(b) => *b,
                    serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                    serde_json::Value::String(s) => !s.is_empty(),
                    serde_json::Value::Null => false,
                    _ => true, // objects and arrays are truthy
                };
                if truthy {
                    return ActionResult::ok(json!({"condition_met": true, "value": val}));
                }
            }
            Err(e) => return cdp_error_to_result(e),
        }

        if tokio::time::Instant::now() >= deadline {
            return ActionResult::retryable(
                "condition_timeout",
                format!(
                    "condition '{}' not met within {}ms",
                    expression,
                    timeout.as_millis()
                ),
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}
