use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Get detail for a single network request, including response body.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser network request 1234.1 --session s1 --tab t1

Returns full request detail including response body fetched via Network.getResponseBody.
Use `browser network requests` to list request IDs first.")]
pub struct Cmd {
    /// Request ID (from `browser network requests`)
    pub request_id: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser network request";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("__ctx_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("__ctx_title")
                .and_then(|v| v.as_str())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
        Some(sid) => sid,
        None => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!("no CDP session for target '{target_id}'"),
            );
        }
    };

    let req = match cdp
        .network_request_detail(&cdp_session_id, &cmd.request_id)
        .await
    {
        Some(r) => r,
        None => {
            return ActionResult::fatal(
                "REQUEST_NOT_FOUND",
                format!("no tracked request with ID '{}'", cmd.request_id),
            );
        }
    };

    // Fetch response body on demand via Network.getResponseBody.
    let (response_body, response_body_base64, body_error): (Option<String>, bool, Option<String>) =
        match cdp
            .execute_on_tab(
                &target_id,
                "Network.getResponseBody",
                json!({ "requestId": cmd.request_id }),
            )
            .await
        {
            Ok(resp) => {
                let body = resp
                    .pointer("/result/body")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let base64 = resp
                    .pointer("/result/base64Encoded")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                (body, base64, None)
            }
            Err(e) => (None, false, Some(e.to_string())),
        };

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    let mut request_json = json!({
        "request_id": req.request_id,
        "url": req.url,
        "method": req.method,
        "resource_type": req.resource_type,
        "timestamp": req.timestamp_ms,
        "status": req.status,
        "mime_type": req.mime_type,
        "request_headers": req.request_headers,
        "response_headers": req.response_headers,
        "response_body": Value::Null,
        "response_body_base64": response_body_base64,
    });

    if let Some(body) = response_body {
        request_json["response_body"] = Value::String(body);
    }
    if let Some(err) = body_error {
        request_json["body_error"] = Value::String(err);
    }
    if let Some(post_data) = req.post_data {
        request_json["post_data"] = Value::String(post_data);
    }

    ActionResult::ok(json!({
        "request": request_json,
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}
