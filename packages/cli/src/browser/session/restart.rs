use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::session::provider::ProviderEnv;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;
use crate::types::Mode;

/// Restart a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser restart --session my-session

Closes and reopens the session with the same profile and mode.
The session_id is preserved; tab IDs reset to t1.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Provider env vars forwarded from the CLI client (see start::Cmd::provider_env).
    /// Used for stateful provider restarts that need to mint a fresh session.
    #[arg(skip)]
    #[serde(default)]
    pub provider_env: ProviderEnv,
}

pub const COMMAND_NAME: &str = "browser restart";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    let mut ctx = ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    };
    if let ActionResult::Ok { data } = result {
        if let Some(tab_id) = data
            .pointer("/session/tab_id")
            .or_else(|| data.pointer("/tab/tab_id"))
            .and_then(|v| v.as_str())
        {
            ctx.tab_id = Some(tab_id.to_string());
        } else {
            ctx.tab_id = Some("t1".to_string());
        }
    }
    Some(ctx)
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (
        mode,
        headless,
        stealth,
        profile,
        mut open_url,
        cdp_endpoint,
        provider,
        headers,
        provider_session,
        saved_provider_env,
        cdp,
        chrome_process,
        max_tracked_requests,
    );
    {
        let mut reg = registry.lock().await;
        let mut entry = match reg.remove(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };
        mode = entry.mode;
        headless = entry.headless;
        stealth = entry.stealth;
        profile = entry.profile.clone();
        // tab.url is refreshed on `goto` (see browser/navigation/goto.rs)
        // and by `list-tabs`, so it reflects the most recent navigation.
        // Some cloud providers (driver) launch with a
        // `data:text/html,<title>...` watermark page that the L3 security
        // check rejects on re-navigation. So:
        //   - If the saved URL is a dangerous scheme (`data:`, `javascript:`),
        //     drop it. Restart will boot the new session to its provider
        //     default and the user can `goto` again from there.
        //   - `about:blank` is also dropped to avoid an unnecessary navigation.
        open_url = entry.tabs.first().and_then(|t| {
            let lower = t.url.to_ascii_lowercase();
            if lower.is_empty()
                || lower == "about:blank"
                || lower.starts_with("data:")
                || lower.starts_with("javascript:")
            {
                None
            } else {
                Some(t.url.clone())
            }
        });
        // Extension mode requires either --open-url or --tab-id. If the
        // previous session's URL was filtered out (about:blank, data:, etc.)
        // and we have no native tab id to carry through, open a fresh tab so
        // start::execute doesn't see MISSING_TAB_TARGET.
        if mode == Mode::Extension && open_url.is_none() {
            open_url = Some("about:blank".to_string());
        }
        cdp_endpoint = entry.cdp_endpoint.clone();
        provider = entry.provider.clone();
        headers = entry
            .headers
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>();
        provider_session = entry.provider_session.clone();
        saved_provider_env = provider_session.as_ref().map(|s| s.provider_env.clone());
        cdp = entry.cdp.take();
        chrome_process = entry.chrome_process.take();
        max_tracked_requests = entry.max_tracked_requests;

        reg.clear_session_ref_caches(&cmd.session);
    }
    // Registry lock released — slow cleanup below won't block other sessions.

    if let Some(cdp) = cdp {
        cdp.clear_iframe_sessions().await;
        cdp.close().await;
    }
    if let Some(child) = chrome_process {
        crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
    }
    // Provider-managed cloud sessions hand back a session descriptor whose
    // lifetime is bound to the remote control plane. Tear it down here before
    // minting a fresh one. The only remaining stateless path is an explicit
    // Browser Use WS override, which has no provider session handle to release.
    let had_provider_session = provider_session.is_some();
    if let Some(provider_session) = provider_session
        && let Err(err) =
            crate::browser::session::provider::close_provider_session(&provider_session).await
    {
        tracing::warn!(
            "failed to close provider session '{}' for provider '{}' during restart: {err}",
            provider_session.session_id,
            provider_session.provider
        );
    }

    // Restart credential-reuse policy:
    //
    //   stateless provider (provider.is_some() && !had_provider_session)
    //     → reuse the original cdp_endpoint + headers and DROP the provider
    //       flag from the start command. This avoids re-running
    //       `connect_provider`, which would re-read env vars and could pick
    //       up a different API key / profile / proxy than the one the
    //       session was originally launched with. The provider tag is
    //       re-applied to the new registry entry below so observability
    //       survives the round-trip.
    //
    //   stateful provider (had_provider_session)
    //     → the old remote session is gone, so we MUST mint a new one. Pass
    //       --provider through and clear cdp_endpoint so `start::execute`
    //       walks the provider connect path. Keep the user-supplied headers
    //       — provider helpers (`connect_driver_dev` etc.) only inject auth
    //       on top, and callers that needed custom CDP headers for the
    //       initial connect will still need them for the reconnect.
    //
    //   plain cloud (no provider)
    //     → unchanged: reuse cdp_endpoint + headers verbatim.
    let is_stateless_provider_reuse = provider.is_some() && !had_provider_session;
    let preserved_provider_tag = if is_stateless_provider_reuse {
        provider.clone()
    } else {
        None
    };

    let (effective_cdp_endpoint, effective_provider, effective_headers) =
        if is_stateless_provider_reuse {
            (cdp_endpoint, None, headers)
        } else if had_provider_session {
            (None, provider, headers)
        } else {
            (cdp_endpoint, provider, headers)
        };

    // Stateful restart needs provider env to mint a fresh remote session.
    // Prefer the env the user supplied with the restart call (latest shell
    // state) so credential rotation works; fall back to the snapshot we saved
    // at start time so restarts from a different shell don't break.
    let effective_provider_env = if !cmd.provider_env.is_empty() {
        cmd.provider_env.clone()
    } else {
        saved_provider_env.unwrap_or_default()
    };

    let start_cmd = super::start::Cmd {
        mode: Some(mode),
        // Restart preserves the session's effective runtime settings and
        // intentionally does not re-run config/env resolution.
        headless: Some(headless),
        profile: Some(profile),
        executable_path: None,
        open_url,
        // Restart re-creates the session; if extension mode the original
        // tab id is gone after debugger detach, so don't carry it through.
        tab_id: None,
        cdp_endpoint: effective_cdp_endpoint,
        provider: effective_provider,
        header: effective_headers,
        session: None,
        set_session_id: Some(cmd.session.clone()),
        stealth,
        max_tracked_requests,
        provider_env: effective_provider_env,
    };

    let result = super::start::execute(&start_cmd, registry).await;

    // Re-apply the provider tag for stateless reuse, so the restarted entry
    // still reports `provider=driver` (etc) and so subsequent
    // `find_cloud_session_by_provider` lookups continue to match.
    if let Some(provider_tag) = preserved_provider_tag
        && let ActionResult::Ok { ref data } = result
        && let Some(new_session_id) = data.pointer("/session/session_id").and_then(|v| v.as_str())
    {
        let mut reg = registry.lock().await;
        if let Some(entry) = reg.get_mut(new_session_id) {
            entry.provider = Some(provider_tag);
        }
    }

    match result {
        ActionResult::Ok { data } => {
            let mut session = data.get("session").cloned().unwrap_or(json!({}));
            if session.get("tabs_count").is_none() {
                session["tabs_count"] = json!(1);
            }
            ActionResult::ok(json!({
                "session": session,
                "reopened": true,
            }))
        }
        other => other,
    }
}
