use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

pub const COMMAND_NAME: &str = "browser.query";

/// Query elements on the page with cardinality constraints.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    #[command(subcommand)]
    #[serde(flatten)]
    pub mode: QueryMode,
}

#[derive(Subcommand, Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum QueryMode {
    /// Query exactly one element
    One {
        /// CSS selector or extended syntax
        selector: String,
        /// Session ID
        #[arg(long)]
        #[serde(rename = "session_id")]
        session: String,
        /// Tab ID
        #[arg(long)]
        #[serde(rename = "tab_id")]
        tab: String,
    },
    /// Query all matching elements
    All {
        /// CSS selector or extended syntax
        selector: String,
        /// Session ID
        #[arg(long)]
        #[serde(rename = "session_id")]
        session: String,
        /// Tab ID
        #[arg(long)]
        #[serde(rename = "tab_id")]
        tab: String,
    },
    /// Query the nth matching element (1-based)
    Nth {
        /// 1-based index
        n: u32,
        /// CSS selector or extended syntax
        selector: String,
        /// Session ID
        #[arg(long)]
        #[serde(rename = "session_id")]
        session: String,
        /// Tab ID
        #[arg(long)]
        #[serde(rename = "tab_id")]
        tab: String,
    },
    /// Count matching elements
    Count {
        /// CSS selector or extended syntax
        selector: String,
        /// Session ID
        #[arg(long)]
        #[serde(rename = "session_id")]
        session: String,
        /// Tab ID
        #[arg(long)]
        #[serde(rename = "tab_id")]
        tab: String,
    },
}

impl Cmd {
    pub fn session(&self) -> &str {
        match &self.mode {
            QueryMode::One { session, .. }
            | QueryMode::All { session, .. }
            | QueryMode::Nth { session, .. }
            | QueryMode::Count { session, .. } => session,
        }
    }

    pub fn tab(&self) -> &str {
        match &self.mode {
            QueryMode::One { tab, .. }
            | QueryMode::All { tab, .. }
            | QueryMode::Nth { tab, .. }
            | QueryMode::Count { tab, .. } => tab,
        }
    }

    pub fn selector(&self) -> &str {
        match &self.mode {
            QueryMode::One { selector, .. }
            | QueryMode::All { selector, .. }
            | QueryMode::Nth { selector, .. }
            | QueryMode::Count { selector, .. } => selector,
        }
    }
}

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
        Some(cmd.tab().to_string())
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
        session_id: cmd.session().to_string(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

fn css_query_js(selector_json: &str) -> String {
    format!(
        r#"(function() {{
    var raw = {selector_json};
    function unquote(s) {{
        s = (s || '').trim();
        if ((s[0] === '"' && s[s.length-1] === '"') || (s[0] === "'" && s[s.length-1] === "'"))
            return s.slice(1, -1);
        return s;
    }}
    function extractCalls(input, pseudo) {{
        var marker = ':' + pseudo + '(';
        var values = [];
        var remaining = (input || '').trim();
        while (true) {{
            var start = remaining.indexOf(marker);
            if (start === -1) break;
            var depth = 0, end = -1;
            for (var i = start + marker.length - 1; i < remaining.length; i++) {{
                if (remaining[i] === '(') depth++;
                else if (remaining[i] === ')') {{ depth--; if (depth === 0) {{ end = i; break; }} }}
            }}
            if (end === -1) break;
            values.push(remaining.slice(start + marker.length, end));
            remaining = (remaining.slice(0, start) + remaining.slice(end + 1)).trim();
        }}
        return {{ remaining: remaining, values: values }};
    }}
    function extractFlag(input, suffix) {{
        var remaining = (input || '').trim();
        var enabled = false;
        while (remaining.endsWith(suffix)) {{
            enabled = true;
            remaining = remaining.slice(0, -suffix.length).trim();
        }}
        return {{ remaining: remaining, enabled: enabled }};
    }}
    function isVisible(el) {{
        var rect = el.getBoundingClientRect();
        var cs = window.getComputedStyle(el);
        return cs.display !== 'none' && cs.visibility !== 'hidden' && rect.width > 0 && rect.height > 0;
    }}
    var working = (raw || '').trim();
    var cI = extractCalls(working, 'contains'); working = cI.remaining;
    var hI = extractCalls(working, 'has'); working = hI.remaining;
    var vI = extractFlag(working, ':visible'); working = vI.remaining;
    var eI = extractFlag(working, ':enabled'); working = eI.remaining;
    var dI = extractFlag(working, ':disabled'); working = dI.remaining;
    var chI = extractFlag(working, ':checked'); working = chI.remaining;
    var base = working || '*';
    var cTexts = cI.values.map(unquote).filter(Boolean);
    var hSels = hI.values.map(unquote).filter(Boolean);
    var elements = Array.from(document.querySelectorAll(base));
    return elements.filter(function(el) {{
        if (vI.enabled && !isVisible(el)) return false;
        if (eI.enabled && !!el.disabled) return false;
        if (dI.enabled && !el.disabled) return false;
        if (chI.enabled && !el.checked) return false;
        var text = (el.innerText || el.textContent || '').trim();
        for (var i = 0; i < cTexts.length; i++) {{ if (text.indexOf(cTexts[i]) === -1) return false; }}
        for (var j = 0; j < hSels.length; j++) {{
            try {{ if (!el.querySelector(hSels[j])) return false; }}
            catch (_) {{ return false; }}
        }}
        return true;
    }}).slice(0, 500).map(function(el) {{
        var rect = el.getBoundingClientRect();
        var cs = window.getComputedStyle(el);
        var tag = el.tagName;
        var pos = 1;
        var sib = el.previousElementSibling;
        while (sib) {{ if (sib.tagName === tag) pos++; sib = sib.previousElementSibling; }}
        return {{
            selector: raw + ':nth-of-type(' + pos + ')',
            tag: el.tagName.toLowerCase(),
            text: (el.innerText || el.textContent || '').trim().substring(0, 80),
            visible: cs.display !== 'none' && cs.visibility !== 'hidden' && rect.width > 0 && rect.height > 0,
            enabled: !el.disabled
        }};
    }});
}})()"#
    )
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, cmd.session(), cmd.tab()).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    let selector = cmd.selector();
    let selector_json = match serde_json::to_string(selector) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("INVALID_ARGUMENT", e.to_string()),
    };

    let js = css_query_js(&selector_json);

    let resp = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({
                "expression": js,
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
            .unwrap_or("JS exception during query");
        return ActionResult::fatal("JS_EXCEPTION", description.to_string());
    }

    let val = resp
        .pointer("/result/result/value")
        .cloned()
        .unwrap_or(Value::Null);

    let items = val.as_array().cloned().unwrap_or_default();
    let count = items.len();
    let sample_selectors: Vec<Value> = items
        .iter()
        .filter_map(|item| item.get("selector").cloned())
        .take(3)
        .collect();

    match &cmd.mode {
        QueryMode::One { .. } => {
            if count == 0 {
                return ActionResult::fatal_with_details(
                    "ELEMENT_NOT_FOUND",
                    format!("no elements match selector '{selector}'"),
                    "check the selector or wait for the element to appear",
                    json!({
                        "query": selector,
                        "count": 0
                    }),
                );
            }
            if count > 1 {
                return ActionResult::fatal_with_details(
                    "MULTIPLE_MATCHES",
                    format!("Query mode 'one' requires exactly 1 match, found {count}"),
                    "use 'query all' or narrow your selector",
                    json!({
                        "query": selector,
                        "count": count,
                        "sample_selectors": sample_selectors,
                    }),
                );
            }
            ActionResult::ok(json!({
                "mode": "one",
                "query": selector,
                "count": 1,
                "item": items[0],
                "__ctx_url": url,
                "__ctx_title": title,
            }))
        }
        QueryMode::All { .. } => ActionResult::ok(json!({
            "mode": "all",
            "query": selector,
            "count": count,
            "items": items,
            "__ctx_url": url,
            "__ctx_title": title,
        })),
        QueryMode::Count { .. } => ActionResult::ok(json!({
            "mode": "count",
            "query": selector,
            "count": count,
            "__ctx_url": url,
            "__ctx_title": title,
        })),
        QueryMode::Nth { n, .. } => {
            let n = *n as usize;
            if n == 0 || n > count {
                return ActionResult::fatal_with_details(
                    "INDEX_OUT_OF_RANGE",
                    format!("index {n} out of range (found {count} matches)"),
                    "use 'query count' to check the number of matches first",
                    json!({
                        "query": selector,
                        "count": count,
                        "index": n,
                        "sample_selectors": sample_selectors,
                    }),
                );
            }
            ActionResult::ok(json!({
                "mode": "nth",
                "query": selector,
                "index": n,
                "count": count,
                "item": items[n - 1],
                "__ctx_url": url,
                "__ctx_title": title,
            }))
        }
    }
}
