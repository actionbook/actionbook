use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::browser::{element, element::element_not_found, navigation};
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Describe element properties and context.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Target element selector
    pub selector: String,
    /// Include nearby context (parent, siblings, children)
    #[arg(long)]
    #[serde(default)]
    pub nearby: bool,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.describe";

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

    let (_, object_id) = match element::resolve_selector_object(
        &cdp,
        &target_id,
        &cmd.selector,
        registry,
        &cmd.session,
        &cmd.tab,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return e,
    };

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    let nearby_js = if cmd.nearby { "true" } else { "false" };

    let js = format!(
        r#"function() {{
var gr=function(e){{ var r=e.getAttribute('role'); if(r) return r; var t=e.tagName.toLowerCase(); if(t==='a') return'link'; if(t==='button') return'button'; if(t==='input'){{ var tp=(e.type||'text').toLowerCase(); if(tp==='checkbox') return'checkbox'; if(tp==='radio') return'radio'; if(tp==='submit'||tp==='button'||tp==='reset') return'button'; return'textbox'; }} if(t==='select') return'combobox'; if(t==='textarea') return'textbox'; if(t==='li') return'listitem'; if(t==='span') return'text'; return t; }};
var gn=function(e){{ var l=e.getAttribute('aria-label'); if(l) return l.trim(); var lb=e.getAttribute('aria-labelledby'); if(lb){{ var le=document.getElementById(lb); if(le) return(le.innerText||'').trim(); }} if(e.placeholder) return e.placeholder; if(e.title) return e.title; return(e.innerText||'').trim().substring(0,50); }};
var ga=function(e){{ var a={{}}; if(e.type) a.type=e.type; if(e.href) a.href=e.href; return a; }};
var gst=function(e){{ var r=e.getBoundingClientRect(); var s=window.getComputedStyle(e); return{{visible:r.width>0&&r.height>0&&s.visibility!=='hidden'&&s.display!=='none',enabled:!e.disabled}}; }};
var sm=function(e){{ var r=gr(e); var n=gn(e); return n?r+' "'+n.replace(/"/g,'\\"')+'"':r; }};
var res={{role:gr(this),name:gn(this),tag:this.tagName.toLowerCase(),attributes:ga(this),state:gst(this),nearby:null}};
if({nearby_js}){{ var par=this.parentElement; var prv=this.previousElementSibling; var nxt=this.nextElementSibling; var chl=Array.from(this.children).slice(0,3); res.nearby={{parent:par?sm(par):null,previous_sibling:prv?sm(prv):null,next_sibling:nxt?sm(nxt):null,children:chl.map(sm)}}; }}
return res;
}}"#
    );

    let resp = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": js,
                "returnByValue": true,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"));

    let resp = match resp {
        Ok(v) => v,
        Err(e) => return e,
    };

    if resp.pointer("/result/exceptionDetails").is_some() {
        let description = resp
            .pointer("/result/exceptionDetails/exception/description")
            .and_then(|v| v.as_str())
            .unwrap_or("JS exception during describe");
        return ActionResult::fatal("JS_EXCEPTION", description.to_string());
    }

    let val = resp
        .pointer("/result/result/value")
        .cloned()
        .unwrap_or(Value::Null);

    if val.is_null() {
        return element_not_found(&cmd.selector);
    }

    let summary = {
        let role = val["role"].as_str().unwrap_or("");
        let name = val["name"].as_str().unwrap_or("");
        if name.is_empty() {
            role.to_string()
        } else {
            format!("{role} \"{name}\"")
        }
    };

    ActionResult::ok(json!({
        "target": { "selector": cmd.selector },
        "summary": summary,
        "role": val["role"],
        "name": val["name"],
        "tag": val["tag"],
        "attributes": val["attributes"],
        "state": val["state"],
        "nearby": val["nearby"],
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}
