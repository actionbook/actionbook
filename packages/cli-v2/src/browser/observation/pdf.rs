use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Save the current page as a PDF.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Output file path
    pub path: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.pdf";

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

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    let resp = cdp
        .execute_on_tab(
            &target_id,
            "Page.printToPDF",
            json!({
                "transferMode": "ReturnAsBase64",
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"));

    let resp = match resp {
        Ok(v) => v,
        Err(e) => return e,
    };

    let data_b64 = resp
        .pointer("/result/data")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let pdf_bytes = match BASE64.decode(data_b64) {
        Ok(b) => b,
        Err(e) => return ActionResult::fatal("CDP_ERROR", format!("base64 decode failed: {e}")),
    };

    let bytes_len = pdf_bytes.len() as u64;

    if let Err(e) = std::fs::write(&cmd.path, &pdf_bytes) {
        return ActionResult::fatal(
            "ARTIFACT_WRITE_FAILED",
            format!("failed to write PDF to '{}': {e}", cmd.path),
        );
    }

    ActionResult::ok(json!({
        "artifact": {
            "path": cmd.path,
            "mime_type": "application/pdf",
            "bytes": bytes_len,
        },
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}
