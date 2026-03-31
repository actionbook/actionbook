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

pub const COMMAND_NAME: &str = "browser press";

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

/// CDP key definition: physical code, virtual key code, and text generated.
struct KeyDef {
    code: String,
    key_code: u32,
    text: Option<String>,
}

/// Map a key name to its full CDP definition (code, keyCode, text).
///
/// Chrome's `Input.dispatchKeyEvent` needs `windowsVirtualKeyCode` and `code`
/// to trigger native browser behaviours (form submit on Enter, focus switch on
/// Tab, etc.).  Without these the DOM event fires but the browser action does
/// not.
fn key_definition(key: &str) -> Option<KeyDef> {
    match key {
        "Enter" => Some(KeyDef {
            code: "Enter".into(),
            key_code: 13,
            text: Some("\r".into()),
        }),
        "Tab" => Some(KeyDef {
            code: "Tab".into(),
            key_code: 9,
            text: None,
        }),
        "Escape" => Some(KeyDef {
            code: "Escape".into(),
            key_code: 27,
            text: None,
        }),
        "Backspace" => Some(KeyDef {
            code: "Backspace".into(),
            key_code: 8,
            text: None,
        }),
        "Delete" => Some(KeyDef {
            code: "Delete".into(),
            key_code: 46,
            text: None,
        }),
        " " => Some(KeyDef {
            code: "Space".into(),
            key_code: 32,
            text: Some(" ".into()),
        }),
        "ArrowUp" => Some(KeyDef {
            code: "ArrowUp".into(),
            key_code: 38,
            text: None,
        }),
        "ArrowDown" => Some(KeyDef {
            code: "ArrowDown".into(),
            key_code: 40,
            text: None,
        }),
        "ArrowLeft" => Some(KeyDef {
            code: "ArrowLeft".into(),
            key_code: 37,
            text: None,
        }),
        "ArrowRight" => Some(KeyDef {
            code: "ArrowRight".into(),
            key_code: 39,
            text: None,
        }),
        "Home" => Some(KeyDef {
            code: "Home".into(),
            key_code: 36,
            text: None,
        }),
        "End" => Some(KeyDef {
            code: "End".into(),
            key_code: 35,
            text: None,
        }),
        "PageUp" => Some(KeyDef {
            code: "PageUp".into(),
            key_code: 33,
            text: None,
        }),
        "PageDown" => Some(KeyDef {
            code: "PageDown".into(),
            key_code: 34,
            text: None,
        }),
        "Insert" => Some(KeyDef {
            code: "Insert".into(),
            key_code: 45,
            text: None,
        }),
        _ => {
            // F1–F12
            if let Some(suffix) = key.strip_prefix('F')
                && let Ok(n) = suffix.parse::<u32>()
                && (1..=12).contains(&n)
            {
                return Some(KeyDef {
                    code: key.into(),
                    key_code: 111 + n,
                    text: None,
                });
            }
            // Single printable character
            if key.len() == 1 {
                let ch = key.chars().next().unwrap();
                if ch.is_ascii_alphabetic() {
                    let upper = ch.to_ascii_uppercase();
                    return Some(KeyDef {
                        code: format!("Key{upper}"),
                        key_code: upper as u32,
                        text: Some(ch.to_string()),
                    });
                }
                if ch.is_ascii_digit() {
                    return Some(KeyDef {
                        code: format!("Digit{ch}"),
                        key_code: ch as u32,
                        text: Some(ch.to_string()),
                    });
                }
                // Symbol/punctuation keys (/, ., -, ;, =, etc.)
                // Use the ASCII code as keyCode and the character as text
                // so they're dispatched as keyDown with text, not rawKeyDown.
                if ch.is_ascii_graphic() {
                    return Some(KeyDef {
                        code: String::new(),
                        key_code: ch as u32,
                        text: Some(ch.to_string()),
                    });
                }
            }
            None
        }
    }
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
    let def = key_definition(&key);

    // Chrome CDP dispatches key events with specific type semantics:
    //   - "keyDown" with `text`: generates both keydown + keypress DOM events
    //     and triggers native behaviours (form submit, focus switch, etc.)
    //   - "rawKeyDown" without `text`: generates keydown only, no text/native action
    //   - "keyUp": generates keyup DOM event
    //
    // For keys that produce text (Enter → "\r", Space → " ", printable chars),
    // use "keyDown" + text so Chrome fires native actions.
    // For pure-functional keys (Tab, Escape, arrows), use "rawKeyDown".

    // Suppress text only for shortcut modifiers (Ctrl/Alt/Meta).
    // Shift alone is a text modifier (Shift+A → "A", Shift+1 → "!"),
    // so it must still carry text to trigger native key behaviour.
    const SHORTCUT_MODIFIERS: u32 = 2 | 1 | 4; // Control | Alt | Meta
    let has_shortcut_modifier = (modifiers & SHORTCUT_MODIFIERS) != 0;

    let text_for_key = if has_shortcut_modifier {
        None
    } else {
        def.as_ref().and_then(|d| d.text.clone())
    };

    // keyDown or rawKeyDown depending on whether the key generates text
    let mut key_down = if text_for_key.is_some() {
        json!({ "type": "keyDown", "key": key, "modifiers": modifiers })
    } else {
        json!({ "type": "rawKeyDown", "key": key, "modifiers": modifiers })
    };

    if let Some(ref d) = def {
        key_down["code"] = json!(d.code);
        key_down["windowsVirtualKeyCode"] = json!(d.key_code);
        key_down["nativeVirtualKeyCode"] = json!(d.key_code);
    }
    if let Some(ref text) = text_for_key {
        key_down["text"] = json!(text);
        key_down["unmodifiedText"] = json!(text);
    }

    if let Err(e) = cdp
        .execute_on_tab(&target_id, "Input.dispatchKeyEvent", key_down)
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // keyUp
    let mut key_up = json!({
        "type": "keyUp",
        "key": key,
        "modifiers": modifiers,
    });
    if let Some(ref d) = def {
        key_up["code"] = json!(d.code);
        key_up["windowsVirtualKeyCode"] = json!(d.key_code);
        key_up["nativeVirtualKeyCode"] = json!(d.key_code);
    }

    if let Err(e) = cdp
        .execute_on_tab(&target_id, "Input.dispatchKeyEvent", key_up)
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
