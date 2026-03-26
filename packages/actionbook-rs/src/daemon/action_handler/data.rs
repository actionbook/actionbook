use super::*;

pub(super) fn resolve_any_tab(
    session_id: SessionId,
    regs: &Registries,
) -> Result<&str, ActionResult> {
    regs.tabs
        .values()
        .next()
        .map(|t| t.target_id.as_str())
        .ok_or_else(|| {
            ActionResult::fatal(
                "no_tabs",
                format!("session {session_id} has no open tabs for cookie operations"),
                format!("open a tab first with `actionbook browser open -s {session_id} <url>`"),
            )
        })
}

pub(super) async fn handle_cookies_list(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    domain: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let op = BackendOp::GetCookies {
        target_id: target_id.to_string(),
    };
    match backend.exec(op).await {
        Ok(result) => {
            let cookies = result
                .value
                .get("cookies")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let items = filter_cookies_by_domain(cookies, domain);
            ActionResult::ok(json!({"items": items}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_cookies_get(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    name: &str,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let op = BackendOp::GetCookies {
        target_id: target_id.to_string(),
    };
    match backend.exec(op).await {
        Ok(result) => {
            let cookies = result
                .value
                .get("cookies")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let found: Vec<_> = cookies
                .into_iter()
                .filter(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
                .collect();
            let item = found.into_iter().next().unwrap_or(json!(null));
            ActionResult::ok(json!({"item": item}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_cookies_set(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    name: &str,
    value: &str,
    domain: Option<&str>,
    path: Option<&str>,
    secure: Option<bool>,
    http_only: Option<bool>,
    same_site: Option<SameSite>,
    expires: Option<f64>,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    // If no domain specified, derive from current page URL
    let effective_domain = match domain {
        Some(d) => d.to_string(),
        None => {
            let url_op = BackendOp::Evaluate {
                target_id: target_id.to_string(),
                expression: "window.location.hostname".to_string(),
                return_by_value: true,
            };
            let hostname = match backend.exec(url_op).await {
                Ok(val) => {
                    let raw = extract_eval_value(&val.value);
                    raw.as_str().unwrap_or("localhost").to_string()
                }
                Err(_) => "localhost".to_string(),
            };
            // CDP expects domain with leading dot for subdomain matching
            if hostname.starts_with('.') {
                hostname
            } else {
                format!(".{hostname}")
            }
        }
    };
    let op = BackendOp::SetCookie {
        target_id: target_id.to_string(),
        name: name.to_string(),
        value: value.to_string(),
        domain: effective_domain,
        path: path.unwrap_or("/").to_string(),
        secure,
        http_only,
        same_site: same_site.map(|s| s.to_string()),
        expires,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({
            "action": "set",
            "affected": 1,
            "domain": domain
        })),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_cookies_delete(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    name: &str,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    // Derive domain from current page for CDP
    let domain = {
        let url_op = BackendOp::Evaluate {
            target_id: target_id.to_string(),
            expression: "window.location.hostname".to_string(),
            return_by_value: true,
        };
        match backend.exec(url_op).await {
            Ok(val) => {
                let raw = extract_eval_value(&val.value);
                let h = raw.as_str().unwrap_or("localhost").to_string();
                if h.starts_with('.') {
                    h
                } else {
                    format!(".{h}")
                }
            }
            Err(_) => ".localhost".to_string(),
        }
    };
    let op = BackendOp::DeleteCookies {
        target_id: target_id.to_string(),
        name: name.to_string(),
        domain: Some(domain),
        path: None,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({
            "action": "delete",
            "affected": 1
        })),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_cookies_clear(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    domain: Option<&str>,
) -> ActionResult {
    let target_id = match resolve_any_tab(session_id, regs) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let get_op = BackendOp::GetCookies {
        target_id: target_id.to_string(),
    };
    let cookies = match backend.exec(get_op).await {
        Ok(result) => result
            .value
            .get("cookies")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        Err(e) => return cdp_error_to_result(e),
    };
    let cookies = filter_cookies_by_domain(cookies, domain);
    let mut deleted = 0;
    for cookie in &cookies {
        let cname = cookie.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let cdomain = cookie.get("domain").and_then(|d| d.as_str());
        let cpath = cookie.get("path").and_then(|p| p.as_str());
        let op = BackendOp::DeleteCookies {
            target_id: target_id.to_string(),
            name: cname.to_string(),
            domain: cdomain.map(|s| s.to_string()),
            path: cpath.map(|s| s.to_string()),
        };
        if let Err(e) = backend.exec(op).await {
            return cdp_error_to_result(e);
        }
        deleted += 1;
    }
    ActionResult::ok(json!({
        "action": "clear",
        "affected": deleted,
        "domain": domain
    }))
}

fn normalize_cookie_domain(domain: &str) -> String {
    domain.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn cookie_matches_domain(cookie: &Value, domain: Option<&str>) -> bool {
    let Some(domain) = domain else {
        return true;
    };
    let Some(cookie_domain) = cookie.get("domain").and_then(|v| v.as_str()) else {
        return false;
    };
    normalize_cookie_domain(cookie_domain) == normalize_cookie_domain(domain)
}

fn filter_cookies_by_domain(cookies: Vec<Value>, domain: Option<&str>) -> Vec<Value> {
    cookies
        .into_iter()
        .filter(|cookie| cookie_matches_domain(cookie, domain))
        .collect()
}

fn storage_js_name(kind: StorageKind) -> &'static str {
    match kind {
        StorageKind::Local => "localStorage",
        StorageKind::Session => "sessionStorage",
    }
}

pub(super) async fn handle_storage_list(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let js = format!(
        r#"(function() {{ const keys = []; for (let i = 0; i < {store}.length; i++) {{ keys.push({store}.key(i)); }} return keys; }})()"#
    );
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"keys": val, "kind": kind.to_string()}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_storage_get(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
    key: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let key_json = match serde_json::to_string(key) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_key", e.to_string(), "check key"),
    };
    let js = format!("{store}.getItem({key_json})");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(result) => {
            let val = extract_eval_value(&result.value);
            ActionResult::ok(json!({"key": key, "value": val, "kind": kind.to_string()}))
        }
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_storage_set(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
    key: &str,
    value: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let key_json = match serde_json::to_string(key) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_key", e.to_string(), "check key"),
    };
    let value_json = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_value", e.to_string(), "check value"),
    };
    let js = format!("{store}.setItem({key_json}, {value_json})");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"set": key, "kind": kind.to_string()})),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_storage_delete(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
    key: &str,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let key_json = match serde_json::to_string(key) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("invalid_key", e.to_string(), "check key"),
    };
    let js = format!("{store}.removeItem({key_json})");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"deleted": key, "kind": kind.to_string()})),
        Err(e) => cdp_error_to_result(e),
    }
}

pub(super) async fn handle_storage_clear(
    session_id: SessionId,
    backend: &mut dyn BackendSession,
    regs: &Registries,
    tab: TabId,
    kind: StorageKind,
) -> ActionResult {
    let target_id = match resolve_tab(session_id, regs, tab) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let store = storage_js_name(kind);
    let js = format!("{store}.clear()");
    let op = BackendOp::Evaluate {
        target_id: target_id.to_string(),
        expression: js,
        return_by_value: true,
    };
    match backend.exec(op).await {
        Ok(_) => ActionResult::ok(json!({"cleared": kind.to_string()})),
        Err(e) => cdp_error_to_result(e),
    }
}
