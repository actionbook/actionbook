use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Set a cookie
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser cookies set session_id value123 --session s1 --domain example.com --path /")]
pub struct Cmd {
    /// Cookie name
    #[arg()]
    pub name: String,
    /// Cookie value
    #[arg()]
    pub value: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Cookie domain
    #[arg(long)]
    pub domain: Option<String>,
    /// Cookie path (default: /)
    #[arg(long)]
    pub path: Option<String>,
    /// Mark cookie as Secure
    #[arg(long)]
    pub secure: bool,
    /// Mark cookie as HttpOnly
    #[arg(long = "http-only")]
    pub http_only: bool,
    /// SameSite policy (Strict, Lax, None)
    #[arg(long = "same-site")]
    pub same_site: Option<String>,
    /// Expiration as Unix timestamp in seconds
    #[arg(long)]
    pub expires: Option<f64>,
}

pub const COMMAND_NAME: &str = "browser.cookies.set";

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
    let (cdp, target_id) = {
        let reg = registry.lock().await;
        let entry = match reg.get(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };
        let cdp = match entry.cdp.clone() {
            Some(c) => c,
            None => {
                return ActionResult::fatal(
                    "INTERNAL_ERROR",
                    format!("no CDP connection for session '{}'", cmd.session),
                );
            }
        };
        let target_id = match entry.tabs.first() {
            Some(t) => t.id.0.clone(),
            None => {
                return ActionResult::fatal(
                    "NO_TAB",
                    format!("no active tab in session '{}'", cmd.session),
                );
            }
        };
        (cdp, target_id)
    };

    // Derive domain: use explicit --domain if provided, else evaluate window.location.hostname
    // on the first active tab and prepend a leading dot for subdomain matching.
    let resolved_domain: Option<String> = if let Some(ref d) = cmd.domain {
        Some(d.clone())
    } else {
        let resp = cdp
            .execute_on_tab(
                &target_id,
                "Runtime.evaluate",
                json!({ "expression": "window.location.hostname", "returnByValue": true }),
            )
            .await;
        match resp {
            Ok(v) => {
                let hostname = v
                    .pointer("/result/result/value")
                    .and_then(|h| h.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if hostname.is_empty() {
                    None
                } else {
                    Some(format!(".{hostname}"))
                }
            }
            Err(_) => None,
        }
    };

    // Default path to "/" per §13.3 when omitted.
    let resolved_path = cmd.path.as_deref().unwrap_or("/").to_string();

    let mut params = json!({
        "name": cmd.name,
        "value": cmd.value,
        "path": resolved_path,
    });
    if let Some(ref domain) = resolved_domain {
        params["domain"] = json!(domain);
    }
    if cmd.secure {
        params["secure"] = json!(true);
    }
    if cmd.http_only {
        params["httpOnly"] = json!(true);
    }
    if let Some(ref ss) = cmd.same_site {
        params["sameSite"] = json!(ss);
    }
    if let Some(exp) = cmd.expires {
        params["expires"] = json!(exp);
    }

    match cdp
        .execute_on_tab(&target_id, "Network.setCookie", params)
        .await
    {
        Ok(_) => {}
        Err(e) => return ActionResult::fatal("CDP_ERROR", e.to_string()),
    };

    let domain_val = resolved_domain.unwrap_or_default();

    ActionResult::ok(json!({
        "action": "set",
        "affected": 1,
        "domain": domain_val,
    }))
}
