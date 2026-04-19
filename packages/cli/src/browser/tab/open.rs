use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::{ensure_scheme, ensure_scheme_or_fatal};
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;
use crate::types::Mode;

/// Open a new tab
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser new-tab https://example.com --session my-session
  actionbook browser new-tab https://a.com https://b.com --session my-session
  actionbook browser new-tab https://a.com https://b.com --session s0 --tab inbox --tab docs
  actionbook browser open https://github.com --session my-session
  actionbook browser new-tab https://example.com --session s0 --tab inbox

The new tab is assigned the next available ID (t2, t3, ...) unless --tab / --set-tab-id is provided.
When opening multiple URLs, repeat --tab once per URL in the same order.
Use the returned tab_id to address this tab in subsequent commands.")]
pub struct Cmd {
    /// URL(s) to open
    #[arg(required = true, num_args = 1..)]
    pub urls: Vec<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Set a custom tab ID instead of auto-assigning
    #[arg(long, visible_alias = "tab")]
    pub set_tab_id: Vec<String>,
    /// Open in new window
    #[arg(long)]
    pub new_window: bool,
    /// Window ID
    #[arg(long)]
    pub window: Option<String>,
}

pub const COMMAND_NAME: &str = "browser new-tab";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    match result {
        ActionResult::Ok { data } if cmd.urls.len() == 1 => Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: data["tab"]["tab_id"].as_str().map(|s| s.to_string()),
            window_id: None,
            url: data["tab"]["url"].as_str().map(|s| s.to_string()),
            title: data["tab"]["title"].as_str().map(|s| s.to_string()),
        }),
        ActionResult::Ok { .. } => Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: None,
            window_id: None,
            url: None,
            title: None,
        }),
        ActionResult::Fatal { code, .. } if code == "PARTIAL_FAILURE" => Some(ResponseContext {
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
    if cmd.urls.len() == 1 {
        return execute_single(cmd, registry).await;
    }

    if !cmd.set_tab_id.is_empty() && cmd.set_tab_id.len() != cmd.urls.len() {
        return ActionResult::fatal_with_hint(
            "INVALID_ARGUMENT",
            format!(
                "got {} tab IDs for {} URLs",
                cmd.set_tab_id.len(),
                cmd.urls.len()
            ),
            "repeat --tab once per URL, or omit --tab to auto-assign IDs",
        );
    }

    let (cdp, stealth_ua, mode) = match session_cdp(&cmd.session, registry).await {
        Ok(parts) => parts,
        Err(err) => return err,
    };

    let mut opened_tabs = Vec::new();
    let mut failures = Vec::new();

    for (index, raw_url) in cmd.urls.iter().enumerate() {
        let final_url = match ensure_scheme(raw_url) {
            Ok(url) => url,
            Err(err) => {
                failures.push(json!({
                    "url": raw_url,
                    "code": "INVALID_ARGUMENT",
                    "message": err.to_string(),
                }));
                continue;
            }
        };

        match open_one_tab(
            &cmd.session,
            &cdp,
            stealth_ua.as_deref(),
            mode,
            registry,
            &final_url,
            cmd.set_tab_id.get(index).map(String::as_str),
        )
        .await
        {
            Ok(tab) => opened_tabs.push(tab),
            Err(err) => {
                failures.push(failure_json(raw_url, &err));
                if is_session_not_found(&err) {
                    append_skipped_failures(&cmd.urls, index + 1, &err, &mut failures);
                    break;
                }
            }
        }
    }

    if failures.is_empty() {
        let opened_count = opened_tabs.len();
        return ActionResult::ok(json!({
            "session_id": cmd.session,
            "tabs": opened_tabs,
            "requested_urls": cmd.urls.len(),
            "opened_tabs": opened_count,
            "failed_urls": 0,
            "created": true,
            "new_window": cmd.new_window,
        }));
    }

    let opened_count = opened_tabs.len();
    let failed_count = failures.len();
    ActionResult::fatal_with_details(
        "PARTIAL_FAILURE",
        format!("opened {opened_count} of {} tabs", cmd.urls.len()),
        "inspect error.details.failures for URLs that did not open",
        json!({
            "session_id": cmd.session,
            "requested_urls": cmd.urls.len(),
            "opened_tabs": opened_count,
            "failed_urls": failed_count,
            "tabs": opened_tabs,
            "failures": failures,
            "new_window": cmd.new_window,
        }),
    )
}

async fn execute_single(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    if cmd.set_tab_id.len() > 1 {
        return ActionResult::fatal_with_hint(
            "INVALID_ARGUMENT",
            format!("got {} tab IDs for 1 URL", cmd.set_tab_id.len()),
            "pass a single --tab value, or omit it to auto-assign the tab ID",
        );
    }

    let final_url = match ensure_scheme_or_fatal(&cmd.urls[0]) {
        Ok(u) => u,
        Err(e) => return e,
    };

    let (cdp, stealth_ua, mode) = match session_cdp(&cmd.session, registry).await {
        Ok(parts) => parts,
        Err(err) => return err,
    };

    match open_one_tab(
        &cmd.session,
        &cdp,
        stealth_ua.as_deref(),
        mode,
        registry,
        &final_url,
        cmd.set_tab_id.first().map(String::as_str),
    )
    .await
    {
        Ok(tab) => ActionResult::ok(json!({
            "tab": tab,
            "created": true,
            "new_window": cmd.new_window,
        })),
        Err(err) => err,
    }
}

async fn session_cdp(
    session_id: &str,
    registry: &SharedRegistry,
) -> Result<(CdpSession, Option<String>, Mode), ActionResult> {
    let reg = registry.lock().await;
    match reg.get(session_id) {
        Some(entry) => match entry.cdp.clone() {
            Some(cdp) => Ok((cdp, entry.stealth_ua.clone(), entry.mode)),
            None => Err(ActionResult::fatal_with_hint(
                "INTERNAL_ERROR",
                format!("no CDP connection for session '{session_id}'"),
                "try restarting the session",
            )),
        },
        None => Err(ActionResult::fatal_with_hint(
            "SESSION_NOT_FOUND",
            format!("session '{session_id}' not found"),
            "run `actionbook browser list-sessions` to see available sessions",
        )),
    }
}

async fn open_one_tab(
    session_id: &str,
    cdp: &CdpSession,
    stealth_ua: Option<&str>,
    mode: Mode,
    registry: &SharedRegistry,
    final_url: &str,
    custom_tab_id: Option<&str>,
) -> Result<serde_json::Value, ActionResult> {
    // Extension mode: the bridge's CDP allowlist forbids `Target.createTarget`
    // (an extension must not spawn debugger-controlled targets behind the
    // user's back). Use the extension's custom `Extension.createTab` method,
    // which calls `chrome.tabs.create` under the hood and auto-attaches the
    // debugger — symmetric with the `--open-url` path in `session::start`.
    if mode == Mode::Extension {
        let resp = match cdp
            .execute_browser("Extension.createTab", json!({ "url": final_url }))
            .await
        {
            Ok(r) => r,
            Err(e) => return Err(cdp_error_to_result(e, "CDP_ERROR")),
        };
        let result = &resp["result"];
        let tab_id = match result["tabId"].as_i64() {
            Some(n) => n.to_string(),
            None => {
                return Err(ActionResult::fatal(
                    "CDP_ERROR",
                    format!("Extension.createTab did not return tabId: {}", resp),
                ));
            }
        };
        let tab_url = result["url"].as_str().unwrap_or(final_url).to_string();
        let title = result["title"].as_str().unwrap_or("").to_string();
        let native_id = tab_id; // extension uses Chrome tab ID as native identifier

        let short_tab_id = {
            let mut reg = registry.lock().await;
            match reg.get_mut(session_id) {
                Some(entry) => {
                    if let Some(custom_id) = custom_tab_id {
                        match entry.push_tab_with_id(
                            custom_id.to_string(),
                            native_id.clone(),
                            tab_url.clone(),
                            title.clone(),
                        ) {
                            Ok(id) => id,
                            Err(err_result) => return Err(err_result),
                        }
                    } else {
                        entry.push_tab(native_id.clone(), tab_url.clone(), title.clone());
                        entry
                            .tabs
                            .last()
                            .map(|t| t.id.0.clone())
                            .unwrap_or_default()
                    }
                }
                None => {
                    return Err(ActionResult::fatal(
                        "SESSION_NOT_FOUND",
                        format!("session '{session_id}' was closed during tab creation"),
                    ));
                }
            }
        };

        // Register the new tab in CdpSession so subsequent execute_on_tab
        // finds it. Mirrors what tab::list does when it discovers a new tab
        // via Extension.listTabs — without this, `goto --tab <new>` fails
        // with INTERNAL_ERROR "no CDP session for target '<native_id>'".
        cdp.register_extension_tab(&native_id).await;

        return Ok(json!({
            "tab_id": short_tab_id,
            "native_tab_id": native_id,
            "url": tab_url,
            "title": title,
        }));
    }

    // Local / cloud / CDP-direct modes: use the standard CDP `Target.createTarget`.
    let resp = match cdp
        .execute_browser("Target.createTarget", json!({ "url": final_url }))
        .await
    {
        Ok(r) => r,
        Err(e) => return Err(cdp_error_to_result(e, "CDP_ERROR")),
    };
    let target_id = match resp.pointer("/result/targetId").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return Err(ActionResult::fatal(
                "CDP_ERROR",
                format!("Target.createTarget did not return targetId: {}", resp),
            ));
        }
    };

    // Attach before registering — rollback on failure.
    if let Err(e) = cdp.attach(&target_id, stealth_ua).await {
        // Rollback: close the target we just created
        let _ = cdp
            .execute_browser("Target.closeTarget", json!({ "targetId": target_id }))
            .await;
        return Err(cdp_error_to_result(e, "CDP_ERROR"));
    }

    // Wait for navigation to commit (issue #004).
    // Target.createTarget kicks off navigation async but returns before the
    // main frame has moved away from about:blank. Without this wait, any
    // immediately-subsequent read command (text / eval) sees the blank page.
    await_navigation_committed(cdp, &target_id, final_url).await;

    let short_tab_id = {
        let mut reg = registry.lock().await;
        match reg.get_mut(session_id) {
            Some(entry) => {
                if let Some(custom_id) = custom_tab_id {
                    match entry.push_tab_with_id(
                        custom_id.to_string(),
                        target_id.clone(),
                        final_url.to_string(),
                        String::new(),
                    ) {
                        Ok(id) => id,
                        Err(err_result) => {
                            // Rollback: detach and close the target
                            let _ = cdp.detach(&target_id).await;
                            let _ = cdp
                                .execute_browser(
                                    "Target.closeTarget",
                                    json!({ "targetId": target_id }),
                                )
                                .await;
                            return Err(err_result);
                        }
                    }
                } else {
                    entry.push_tab(target_id.clone(), final_url.to_string(), String::new());
                    entry
                        .tabs
                        .last()
                        .map(|t| t.id.0.clone())
                        .unwrap_or_default()
                }
            }
            None => {
                // Session was closed concurrently — detach and close the target
                let _ = cdp.detach(&target_id).await;
                let _ = cdp
                    .execute_browser("Target.closeTarget", json!({ "targetId": target_id }))
                    .await;
                return Err(ActionResult::fatal(
                    "SESSION_NOT_FOUND",
                    format!("session '{session_id}' was closed during tab creation"),
                ));
            }
        }
    };

    Ok(json!({
        "tab_id": short_tab_id,
        "native_tab_id": target_id,
        "url": final_url,
        "title": "",
    }))
}

fn is_session_not_found(result: &ActionResult) -> bool {
    matches!(
        result,
        ActionResult::Fatal { code, .. } if code == "SESSION_NOT_FOUND"
    )
}

fn append_skipped_failures(
    urls: &[String],
    start_index: usize,
    result: &ActionResult,
    failures: &mut Vec<serde_json::Value>,
) {
    failures.extend(
        urls.iter()
            .skip(start_index)
            .map(|url| failure_json(url, result)),
    );
}

fn failure_json(url: &str, result: &ActionResult) -> serde_json::Value {
    match result {
        ActionResult::Fatal { code, message, .. } => json!({
            "url": url,
            "code": code,
            "message": message,
        }),
        ActionResult::Retryable { reason, .. } => json!({
            "url": url,
            "code": "RETRYABLE",
            "message": reason,
        }),
        ActionResult::UserAction { action, .. } => json!({
            "url": url,
            "code": "USER_ACTION",
            "message": action,
        }),
        ActionResult::Ok { .. } => json!({
            "url": url,
            "code": "INTERNAL_ERROR",
            "message": "unexpected success while recording failure",
        }),
    }
}

/// Poll document.URL until the tab has navigated away from about:blank.
///
/// Target.createTarget with a `url` argument kicks off navigation but returns
/// before the main frame commits. Without this wait, a read command issued
/// immediately after `new-tab` sees the blank page. Issue #004.
///
/// - Skips the wait when the requested URL is itself an internal page (about:*,
///   chrome://*, javascript:, etc.) — user intent was a blank/internal tab.
/// - Polls every 50 ms, gives up after 3 s. On timeout we return without
///   error: the tab is still usable and the caller's subsequent `wait network-idle`
///   or `text` will surface the issue if navigation genuinely failed.
async fn await_navigation_committed(cdp: &CdpSession, target_id: &str, requested_url: &str) {
    // URL schemes are case-insensitive per RFC 3986 §3.1. Lower-case the
    // prefix so `ABOUT:blank`, `Chrome://newtab`, etc. bypass the polling
    // loop the same way `about:blank` does.
    if url_has_internal_scheme(requested_url) {
        return;
    }

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(3000);
    let tick = std::time::Duration::from_millis(50);

    loop {
        let current = crate::browser::navigation::get_tab_url(cdp, target_id).await;
        let is_blank = current.is_empty() || observed_is_blank(&current);
        if !is_blank {
            return;
        }
        if start.elapsed() >= timeout {
            return;
        }
        tokio::time::sleep(tick).await;
    }
}

/// True for requested URLs that name an internal page. Case-insensitive.
fn url_has_internal_scheme(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("about:")
        || lower.starts_with("chrome:")
        || lower.starts_with("javascript:")
        || lower.starts_with("data:")
}

/// True for observed URLs that the browser considers "not yet navigated".
/// Lowercased so the browser's case-normalized scheme still matches if it
/// ever starts echoing the user's casing.
fn observed_is_blank(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("about:") || lower.starts_with("chrome://newtab")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_scheme_detection_is_case_insensitive() {
        // RFC 3986: URL schemes are case-insensitive. If we miss uppercase
        // variants we'd pay a 3s polling stall before timing out for tabs
        // that are intentionally internal.
        assert!(url_has_internal_scheme("about:blank"));
        assert!(url_has_internal_scheme("ABOUT:blank"));
        assert!(url_has_internal_scheme("About:blank"));
        assert!(url_has_internal_scheme("chrome://newtab"));
        assert!(url_has_internal_scheme("Chrome://newtab"));
        assert!(url_has_internal_scheme("CHROME://newtab"));
        assert!(url_has_internal_scheme("javascript:alert(1)"));
        assert!(url_has_internal_scheme("JavaScript:alert(1)"));
        assert!(url_has_internal_scheme("data:text/html,hi"));
        assert!(url_has_internal_scheme("DATA:text/html,hi"));

        // Non-internal schemes stay out.
        assert!(!url_has_internal_scheme("https://example.com"));
        assert!(!url_has_internal_scheme("HTTPS://EXAMPLE.COM"));
        assert!(!url_has_internal_scheme("file:///tmp/page.html"));
    }

    #[test]
    fn observed_blank_detection_is_case_insensitive() {
        assert!(observed_is_blank("about:blank"));
        assert!(observed_is_blank("ABOUT:blank"));
        assert!(observed_is_blank("chrome://newtab"));
        assert!(observed_is_blank("Chrome://newtab"));
        assert!(!observed_is_blank("https://example.com"));
        assert!(!observed_is_blank("chrome://extensions"));
    }

    #[test]
    fn append_skipped_failures_records_remaining_urls() {
        let urls = vec![
            "https://b.com".to_string(),
            "https://c.com".to_string(),
            "javascript:alert(1)".to_string(),
        ];
        let err = ActionResult::fatal(
            "SESSION_NOT_FOUND",
            "session 's0' was closed during tab creation",
        );
        let mut failures = vec![failure_json(&urls[0], &err)];

        append_skipped_failures(&urls, 1, &err, &mut failures);

        assert_eq!(failures.len(), 3);
        assert_eq!(failures[1]["url"], json!("https://c.com"));
        assert_eq!(failures[1]["code"], json!("SESSION_NOT_FOUND"));
        assert_eq!(failures[2]["url"], json!("javascript:alert(1)"));
        assert_eq!(
            failures[2]["message"],
            json!("session 's0' was closed during tab creation")
        );
    }
}
