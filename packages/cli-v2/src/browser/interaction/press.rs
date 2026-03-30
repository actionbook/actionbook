use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Press a key or key combination
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser press Enter --session s1 --tab t1
  actionbook browser press Tab --session s1 --tab t1
  actionbook browser press Control+A --session s1 --tab t1
  actionbook browser press Shift+Tab --session s1 --tab t1

Sends a key press to the currently focused element.
Use focus first to direct keys to a specific element.
Key names follow CDP conventions: Enter, Tab, Escape, ArrowDown, Control, Shift, Alt, Meta.")]
pub struct Cmd {
    /// Single key or key combination (e.g., Enter, Control+A, Shift+Tab)
    pub key: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.press";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("post_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            data.get("post_title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url,
        title,
    })
}

/// Known modifier names (case-insensitive match).
fn modifier_bit(name: &str) -> Option<u32> {
    match name.to_lowercase().as_str() {
        "control" | "ctrl" => Some(2),
        "shift" => Some(8),
        "alt" | "option" => Some(1),
        "meta" | "command" | "cmd" => Some(4),
        _ => None,
    }
}

/// Parse a key-or-chord string into (modifiers bitmask, main key).
fn parse_chord(input: &str) -> Result<(u32, String), ActionResult> {
    let parts: Vec<&str> = input.split('+').collect();
    let mut modifiers: u32 = 0;

    // All parts except the last are expected to be modifiers
    for &part in &parts[..parts.len() - 1] {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            return Err(ActionResult::fatal(
                "INVALID_ARGUMENT",
                format!("invalid chord: '{input}' — empty modifier segment"),
            ));
        }
        match modifier_bit(trimmed) {
            Some(bit) => modifiers |= bit,
            None => {
                return Err(ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    format!("invalid chord: '{input}' — unknown modifier '{trimmed}'"),
                ));
            }
        }
    }

    // The last part is the main key
    let main_key = parts.last().unwrap().trim().to_string();
    if main_key.is_empty() {
        return Err(ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid chord: '{input}' — missing key after modifier"),
        ));
    }

    Ok((modifiers, main_key))
}

/// Map a key name to the CDP `key` value.
/// Single printable characters with modifiers are lowercased.
fn cdp_key(key: &str, has_modifiers: bool) -> String {
    // Single character keys
    if key.len() == 1 {
        let ch = key.chars().next().unwrap();
        if has_modifiers && ch.is_ascii_alphabetic() {
            return ch.to_ascii_lowercase().to_string();
        }
        return key.to_string();
    }
    // Named keys — return as-is (Enter, Tab, Escape, etc.)
    key.to_string()
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Parse the key-or-chord
    let (modifiers, main_key) = match parse_chord(&cmd.key) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let key = cdp_key(&main_key, modifiers != 0);

    // Dispatch keyDown
    if let Err(e) = cdp
        .execute_on_tab(
            &target_id,
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyDown",
                "key": key,
                "modifiers": modifiers,
            }),
        )
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Dispatch keyUp
    if let Err(e) = cdp
        .execute_on_tab(
            &target_id,
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyUp",
                "key": key,
                "modifiers": modifiers,
            }),
        )
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "action": "press",
        "keys": cmd.key,
        "changed": {
            "url_changed": false,
            "focus_changed": false,
        },
        "post_url": url,
        "post_title": title,
    }))
}
