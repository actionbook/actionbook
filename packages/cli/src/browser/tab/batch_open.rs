use std::collections::HashSet;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::ensure_scheme_or_fatal;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;
use crate::types::Mode;

/// Open multiple tabs in one call (batch)
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser batch-new-tab --urls https://a.com https://b.com --session s0
  actionbook browser batch-new-tab --urls https://a.com https://b.com --tabs inbox settings --session s0
  actionbook browser batch-open --urls https://a.com https://b.com --session s0

Opens each URL as a new tab. If --tabs is provided, its length must match
--urls. Otherwise tab IDs are auto-assigned (t2, t3, ...).
Stops on first failure and reports how many tabs were successfully opened.")]
pub struct Cmd {
    /// URLs to open (1 or more)
    #[arg(long, required = true, num_args(1..))]
    pub urls: Vec<String>,
    /// Optional tab IDs (must match --urls count, or omit for auto-assign)
    #[arg(long, num_args(1..))]
    #[serde(default)]
    pub tabs: Vec<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser batch-new-tab";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    if cmd.urls.is_empty() {
        return ActionResult::fatal("INVALID_ARGUMENT", "batch-new-tab requires at least 1 URL");
    }

    if !cmd.tabs.is_empty() && cmd.tabs.len() != cmd.urls.len() {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!(
                "--tabs length ({}) must match --urls length ({})",
                cmd.tabs.len(),
                cmd.urls.len()
            ),
        );
    }

    // Check for duplicate tab IDs within --tabs
    if !cmd.tabs.is_empty() {
        let mut seen = HashSet::new();
        for tab_id in &cmd.tabs {
            if !seen.insert(tab_id.as_str()) {
                return ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    format!("duplicate tab ID '{}' in --tabs", tab_id),
                );
            }
        }
    }

    // Validate all URLs upfront
    let mut final_urls = Vec::with_capacity(cmd.urls.len());
    for url in &cmd.urls {
        match ensure_scheme_or_fatal(url) {
            Ok(u) => final_urls.push(u),
            Err(e) => return e,
        }
    }

    // Get CdpSession, stealth_ua, and mode from registry
    let (cdp, stealth_ua, mode) = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(e) => match e.cdp.clone() {
                Some(c) => (c, e.stealth_ua.clone(), e.mode),
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

    let mut results = Vec::new();

    for (i, final_url) in final_urls.iter().enumerate() {
        let custom_tab_id = cmd.tabs.get(i);

        // Extension mode uses chrome.tabs.create (wrapped as Extension.createTab);
        // bridge's CDP allowlist forbids raw Target.createTarget. Parallel to the
        // branch in `tab/open.rs`.
        if mode == Mode::Extension {
            let resp = match cdp
                .execute_browser("Extension.createTab", json!({ "url": final_url }))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    return batch_error(
                        i,
                        final_url,
                        cdp_error_to_result(e, "CDP_ERROR"),
                        &results,
                        cmd.urls.len(),
                    );
                }
            };
            let result = &resp["result"];
            let native_id = match result["tabId"].as_i64() {
                Some(n) => n.to_string(),
                None => {
                    return batch_error(
                        i,
                        final_url,
                        ActionResult::fatal(
                            "CDP_ERROR",
                            format!("Extension.createTab did not return tabId: {}", resp),
                        ),
                        &results,
                        cmd.urls.len(),
                    );
                }
            };
            let tab_url = result["url"].as_str().unwrap_or(final_url).to_string();
            let title = result["title"].as_str().unwrap_or("").to_string();

            let short_tab_id = {
                let mut reg = registry.lock().await;
                match reg.get_mut(&cmd.session) {
                    Some(e) => {
                        if let Some(custom_id) = custom_tab_id {
                            match e.push_tab_with_id(
                                custom_id.clone(),
                                native_id.clone(),
                                tab_url.clone(),
                                title.clone(),
                            ) {
                                Ok(id) => id,
                                Err(err_result) => {
                                    return batch_error(
                                        i,
                                        final_url,
                                        err_result,
                                        &results,
                                        cmd.urls.len(),
                                    );
                                }
                            }
                        } else {
                            e.push_tab(native_id.clone(), tab_url.clone(), title.clone());
                            e.tabs.last().map(|t| t.id.0.clone()).unwrap_or_default()
                        }
                    }
                    None => {
                        return ActionResult::fatal(
                            "SESSION_NOT_FOUND",
                            format!(
                                "session '{}' was closed during batch tab creation",
                                cmd.session
                            ),
                        );
                    }
                }
            };

            // Register in CdpSession so execute_on_tab finds this native_id.
            // Same rationale as the single-tab path in tab/open.rs.
            cdp.register_extension_tab(&native_id).await;

            results.push(json!({
                "tab_id": short_tab_id,
                "native_tab_id": native_id,
                "url": tab_url,
                "title": title,
            }));
            continue;
        }

        // Local / cloud / CDP-direct: raw CDP Target.createTarget.
        let resp = match cdp
            .execute_browser("Target.createTarget", json!({ "url": final_url }))
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return batch_error(
                    i,
                    final_url,
                    cdp_error_to_result(e, "CDP_ERROR"),
                    &results,
                    cmd.urls.len(),
                );
            }
        };
        let target_id = match resp.pointer("/result/targetId").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => {
                return batch_error(
                    i,
                    final_url,
                    ActionResult::fatal(
                        "CDP_ERROR",
                        format!("Target.createTarget did not return targetId: {}", resp),
                    ),
                    &results,
                    cmd.urls.len(),
                );
            }
        };

        // Attach before registering — rollback on failure.
        if let Err(e) = cdp.attach(&target_id, stealth_ua.as_deref()).await {
            let _ = cdp
                .execute_browser("Target.closeTarget", json!({ "targetId": target_id }))
                .await;
            return batch_error(
                i,
                final_url,
                cdp_error_to_result(e, "CDP_ERROR"),
                &results,
                cmd.urls.len(),
            );
        }

        // Register the new tab
        let short_tab_id = {
            let mut reg = registry.lock().await;
            match reg.get_mut(&cmd.session) {
                Some(e) => {
                    if let Some(custom_id) = custom_tab_id {
                        match e.push_tab_with_id(
                            custom_id.clone(),
                            target_id.clone(),
                            final_url.clone(),
                            String::new(),
                        ) {
                            Ok(id) => id,
                            Err(err_result) => {
                                let _ = cdp.detach(&target_id).await;
                                let _ = cdp
                                    .execute_browser(
                                        "Target.closeTarget",
                                        json!({ "targetId": target_id }),
                                    )
                                    .await;
                                return batch_error(
                                    i,
                                    final_url,
                                    err_result,
                                    &results,
                                    cmd.urls.len(),
                                );
                            }
                        }
                    } else {
                        e.push_tab(target_id.clone(), final_url.clone(), String::new());
                        e.tabs.last().map(|t| t.id.0.clone()).unwrap_or_default()
                    }
                }
                None => {
                    let _ = cdp.detach(&target_id).await;
                    let _ = cdp
                        .execute_browser("Target.closeTarget", json!({ "targetId": target_id }))
                        .await;
                    return ActionResult::fatal(
                        "SESSION_NOT_FOUND",
                        format!(
                            "session '{}' was closed during batch tab creation",
                            cmd.session
                        ),
                    );
                }
            }
        };

        results.push(json!({
            "tab_id": short_tab_id,
            "native_tab_id": target_id,
            "url": final_url,
            "title": "",
        }));
    }

    ActionResult::ok(json!({
        "action": "batch-new-tab",
        "opened": results.len(),
        "tabs": results,
    }))
}

/// Build a fail-fast error with partial progress info.
fn batch_error(
    index: usize,
    url: &str,
    cause: ActionResult,
    completed: &[serde_json::Value],
    total: usize,
) -> ActionResult {
    let cause_msg = match &cause {
        ActionResult::Fatal { message, .. } => message.clone(),
        _ => String::new(),
    };
    ActionResult::fatal_with_details(
        "BATCH_OPEN_ERROR",
        format!("open failed at index {index} (url: {url}): {cause_msg}"),
        format!(
            "completed {}/{total}, retry from index {index}",
            completed.len(),
        ),
        json!({
            "failed_index": index,
            "failed_url": url,
            "completed": completed.len(),
            "cause": cause_msg,
        }),
    )
}
