use std::process::Child;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::session::provider::{
    ProviderEnv, ProviderSession, connect_provider, normalize_provider_name, supported_providers,
};
use crate::config;
use crate::config::DEFAULT_PROFILE;
use crate::daemon::browser;
use crate::daemon::cdp::{cdp_navigate, ensure_scheme, ensure_scheme_or_fatal};
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result};
use crate::daemon::registry::{SessionState, SharedRegistry};
use crate::output::ResponseContext;
use crate::types::{Mode, SessionId};

/// Start or attach a browser session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser start
  actionbook browser start --session research
  actionbook browser start --session research --open-url https://google.com
  actionbook browser start --headless --profile scraper
  actionbook browser start --mode cloud --cdp-endpoint wss://browser.example.com/ws

Cloud providers (-p / --provider):
  driver          requires DRIVER_API_KEY          # driver.dev
  hyperbrowser    requires HYPERBROWSER_API_KEY    # hyperbrowser.ai
  browseruse      requires BROWSER_USE_API_KEY     # browser-use.com

  -p <name> implies --mode cloud and is mutually exclusive with
  --cdp-endpoint and --mode local/extension. The daemon reads each
  provider's env vars from the CLI caller's shell, not its own
  process env. Each provider also reads optional tuning vars
  (profile, proxy, country, window size, ...) — see docs.

Provider examples:
  export HYPERBROWSER_API_KEY=...
  actionbook browser start -p hyperbrowser --session s1
  actionbook browser start -p driver --open-url https://example.com
  actionbook browser restart --session s1    # provider sessions: mints a fresh remote

--session: get-or-create — reuses an existing session with the given ID, or creates one if not found.
--set-session-id: always creates — fails if the ID is already in use.
Reuse: if a session with the same profile already exists, it is reused.
The returned session_id and tab_id are used to address all subsequent commands.")]
pub struct Cmd {
    /// Browser mode
    #[arg(long, value_enum)]
    pub mode: Option<Mode>,
    /// Headless mode
    #[arg(long, default_missing_value = "true", num_args = 0..=1)]
    pub headless: Option<bool>,
    /// Profile name
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(skip = None)]
    #[serde(default)]
    pub executable_path: Option<String>,
    /// Open this URL on start
    #[arg(long)]
    pub open_url: Option<String>,
    /// Extension mode only: attach to a pre-existing Chrome tab by its native
    /// numeric tab id. `browser list-tabs --json` surfaces `native_tab_id` for
    /// Actionbook-managed tabs (those already attached or in the "Actionbook"
    /// tab group). For tabs outside that set, read the id from Chrome's
    /// built-in DevTools (`chrome://inspect/#pages`) or from `chrome.tabs`
    /// extension APIs before invoking --tab-id.
    /// Mutually exclusive with --open-url. Required when `--mode extension`
    /// is used without `--open-url`; the legacy "attach active tab" default
    /// has been removed in protocol 0.3.0 — every tab must be explicit.
    #[arg(long, conflicts_with = "open_url")]
    pub tab_id: Option<u64>,
    /// Connect to existing CDP endpoint
    #[arg(long)]
    pub cdp_endpoint: Option<String>,
    /// Cloud browser provider (implies --mode cloud).
    ///
    /// `-p <name>` is mutually exclusive with `--cdp-endpoint` and
    /// `--mode local/extension`. Each provider reads its own
    /// `<PROVIDER>_API_KEY` from the CLI caller's shell env — the
    /// daemon's env was frozen at spawn time and is not consulted.
    /// Sessions are stateful: `browser restart --session <id>` mints
    /// a fresh remote session and preserves the session_id.
    #[arg(short = 'p', long, value_parser = provider_value_parser())]
    pub provider: Option<String>,
    /// Headers for CDP endpoint (KEY:VALUE), may be repeated
    #[arg(long)]
    pub header: Vec<String>,
    /// Session ID (get-or-create: reuse if exists, create with this ID if not)
    #[arg(long, conflicts_with = "set_session_id")]
    #[serde(default)]
    pub session: Option<String>,
    /// Specify a semantic session ID (always creates, fails if ID exists)
    #[arg(long)]
    pub set_session_id: Option<String>,
    /// Enable stealth/anti-detection mode (default: true). Use --no-stealth to disable.
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    #[serde(default = "default_stealth")]
    pub stealth: bool,
    /// Maximum number of network requests to track per tab (ring buffer capacity).
    /// Must be between 1 and 100000. Default: 500.
    #[arg(long, default_value_t = 500)]
    #[serde(default = "default_max_tracked_requests")]
    pub max_tracked_requests: usize,
    /// Snapshot of provider env vars forwarded from the CLI client to the
    /// daemon (DRIVER_*, HYPERBROWSER_*, BROWSER_USE_*).
    /// The daemon must NOT read these from its own process env — its env was
    /// frozen at daemon-spawn time and rarely matches the user's current shell.
    /// This field is populated automatically in `main.rs` before the action is
    /// sent over IPC, so callers do not need to set it.
    #[arg(skip)]
    #[serde(default)]
    pub provider_env: ProviderEnv,
}

fn default_max_tracked_requests() -> usize {
    500
}

fn default_stealth() -> bool {
    true
}

/// clap value parser for `-p / --provider`.
///
/// Using `PossibleValuesParser` (rather than a `ValueEnum`) keeps the
/// `Cmd.provider` field as `Option<String>`, so config/env merging in
/// `resolve_start_command` can stay on untyped strings. The per-value
/// `help` text is what gives agents a single-pass, self-contained
/// `--help` output: they see the allowed names *and* the required
/// auth env var in the same line. The alias on `browseruse` preserves
/// the historical `browser-use` spelling; `normalize_provider_name`
/// re-normalizes it at `execute()` time so the daemon-side IPC path
/// keeps the same validation as the CLI-side one.
fn provider_value_parser() -> clap::builder::PossibleValuesParser {
    use clap::builder::PossibleValue;
    clap::builder::PossibleValuesParser::new([
        PossibleValue::new("driver").help("driver.dev — requires DRIVER_API_KEY"),
        PossibleValue::new("hyperbrowser").help("hyperbrowser.ai — requires HYPERBROWSER_API_KEY"),
        PossibleValue::new("browseruse")
            .aliases(["browser-use"])
            .help("browser-use.com — requires BROWSER_USE_API_KEY"),
    ])
}

pub const COMMAND_NAME: &str = "browser start";

struct ReuseTarget {
    session_id: String,
    first_tab_id: String,
    first_native_id: String,
    cdp: Option<CdpSession>,
    cdp_port: Option<u16>,
}

enum StartDisposition {
    Reuse(ReuseTarget),
    Reserved(SessionId),
}

pub fn context(_cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Ok { data } = result {
        Some(ResponseContext {
            session_id: data["session"]["session_id"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            tab_id: Some(data["tab"]["tab_id"].as_str().unwrap_or("t1").to_string()),
            window_id: None,
            url: data["tab"]["url"].as_str().map(|s| s.to_string()),
            title: data["tab"]["title"].as_str().map(|s| s.to_string()),
        })
    } else {
        None
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let provider_name = match cmd.provider.as_deref() {
        Some(provider_name) => match normalize_provider_name(provider_name) {
            Some(provider) => Some(provider),
            None => {
                return ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    format!(
                        "unknown provider '{provider_name}'. Supported providers: {}",
                        supported_providers()
                    ),
                );
            }
        },
        None => None,
    };
    // Mode resolution precedence:
    //   1. --provider implies cloud (validated against any explicit --mode below)
    //   2. explicit --mode flag
    //   3. config file's browser.mode
    //   4. Local default
    let mode = if provider_name.is_some() {
        Mode::Cloud
    } else {
        cmd.mode.unwrap_or_else(|| {
            config::load_config()
                .map(|c| c.browser.mode)
                .unwrap_or(Mode::Local)
        })
    };
    let headless = cmd.headless.unwrap_or(false);
    let profile_name = cmd.profile.as_deref().unwrap_or(DEFAULT_PROFILE);
    let cdp_endpoint = cmd.cdp_endpoint.as_deref();

    if profile_name.contains('/') || profile_name.contains('\\') || profile_name.contains("..") {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid profile name: {profile_name}"),
        );
    }

    if provider_name.is_some() && cdp_endpoint.is_some() {
        return ActionResult::fatal_with_hint(
            "INVALID_ARGUMENT",
            "--provider cannot be used together with --cdp-endpoint".to_string(),
            "use --provider by itself, or use --mode cloud --cdp-endpoint to connect to an existing remote browser",
        );
    }

    if provider_name.is_some() && matches!(cmd.mode, Some(Mode::Local) | Some(Mode::Extension)) {
        return ActionResult::fatal_with_hint(
            "INVALID_ARGUMENT",
            "--provider requires cloud mode".to_string(),
            "remove --mode local/extension, or use --mode cloud with --provider",
        );
    }

    // Cloud mode requires --cdp-endpoint unless a provider is selected.
    if mode == Mode::Cloud && cdp_endpoint.is_none() && provider_name.is_none() {
        return ActionResult::fatal_with_hint(
            "MISSING_CDP_ENDPOINT",
            "--mode cloud requires --cdp-endpoint",
            "provide a cloud browser endpoint, e.g. --cdp-endpoint wss://...",
        );
    }

    if cmd.max_tracked_requests == 0 || cmd.max_tracked_requests > 100_000 {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!(
                "--max-tracked-requests must be between 1 and 100000, got {}",
                cmd.max_tracked_requests
            ),
        );
    }

    // Parse headers from "KEY:VALUE" strings
    let headers = match parse_headers(&cmd.header) {
        Ok(h) => h,
        Err(e) => return e,
    };

    // ── Get-or-create by session ID (all modes) ──
    // --session: reuse existing session if found, otherwise create with that ID.
    if let Some(ref sid) = cmd.session {
        let reg = registry.lock().await;
        if let Some(existing) = reg.get(sid) {
            match existing.status {
                SessionState::Running => {
                    let target = ReuseTarget {
                        session_id: existing.id.as_str().to_string(),
                        first_tab_id: existing
                            .tabs
                            .first()
                            .map(|tab| tab.id.0.clone())
                            .unwrap_or_default(),
                        first_native_id: existing
                            .tabs
                            .first()
                            .map(|tab| tab.native_id.clone())
                            .unwrap_or_default(),
                        cdp: existing.cdp.clone(),
                        cdp_port: existing.cdp_port,
                    };
                    drop(reg);
                    return reuse_running_session(cmd, registry, target).await;
                }
                SessionState::Starting => {
                    return ActionResult::fatal_with_hint(
                        "SESSION_STARTING",
                        format!("session '{}' is starting, please wait", sid),
                        "retry after a few seconds or use browser status to check",
                    );
                }
                SessionState::Closing => {
                    return ActionResult::fatal_with_hint(
                        "SESSION_CLOSING",
                        format!("session '{}' is being closed, please wait", sid),
                        "retry after a few seconds or use browser status to check",
                    );
                }
                SessionState::Closed => {
                    // Closed session — fall through to create a new one with this ID
                }
            }
        }
        // Session not found or closed — fall through to create with this ID
    }

    // Effective set_id: --session (get-or-create) falls back to --set-session-id (force-create)
    let effective_set_id = cmd.session.as_deref().or(cmd.set_session_id.as_deref());

    // ── Cloud mode ──────────────────────────────────────────────────
    if mode == Mode::Cloud {
        if let Some(provider_name) = provider_name {
            // Explicit session IDs can be rejected locally before we create a
            // provider-managed browser. This is only a fast-path preflight:
            // `execute_cloud` still performs the authoritative reserve later
            // so concurrent starts cannot slip through.
            if effective_set_id.is_some() {
                let mut reg = registry.lock().await;
                if let Err(e) = reg.generate_session_id(effective_set_id) {
                    let hint = e.hint();
                    return ActionResult::fatal_with_hint(e.error_code(), e.to_string(), &hint);
                }
            }

            // Provider session reuse: lookup is keyed on (provider, profile),
            // but presence in the registry only proves the entry was once
            // healthy. The remote browser may have been killed by the
            // provider's idle reaper or torn down out-of-band, so we always
            // probe with `Target.getTargets` before reusing — same pattern as
            // `find_cloud_session_by_endpoint` further down.
            let reuse_candidate = {
                let reg = registry.lock().await;
                if effective_set_id.is_none()
                    && let Some(existing) =
                        reg.find_cloud_session_by_provider(provider_name, profile_name)
                {
                    match existing.status {
                        SessionState::Running => Some((
                            ReuseTarget {
                                session_id: existing.id.as_str().to_string(),
                                first_tab_id: existing
                                    .tabs
                                    .first()
                                    .map(|tab| tab.id.0.clone())
                                    .unwrap_or_default(),
                                first_native_id: existing
                                    .tabs
                                    .first()
                                    .map(|tab| tab.native_id.clone())
                                    .unwrap_or_default(),
                                cdp: existing.cdp.clone(),
                                cdp_port: existing.cdp_port,
                            },
                            existing.cdp.clone(),
                        )),
                        SessionState::Starting => {
                            return ActionResult::fatal_with_hint(
                                "SESSION_STARTING",
                                format!(
                                    "session for provider '{provider_name}' and profile '{profile_name}' is starting, please wait"
                                ),
                                "retry after a few seconds or use browser status to check",
                            );
                        }
                        // `find_cloud_session_by_provider` filters on
                        // `is_active()`, which excludes both Closing and
                        // Closed — these arms exist only to keep the match
                        // exhaustive.
                        SessionState::Closing | SessionState::Closed => None,
                    }
                } else {
                    None
                }
            };

            if let Some((target, cdp)) = reuse_candidate {
                let stale_session_id = target.session_id.clone();
                let healthy = match cdp.as_ref() {
                    Some(cdp) => cdp
                        .execute_browser("Target.getTargets", json!({}))
                        .await
                        .is_ok(),
                    None => false,
                };
                if healthy {
                    return reuse_running_session(cmd, registry, target).await;
                }
                // Stale entry — drop it before falling through to a fresh
                // provider connect, otherwise the next reuse attempt will
                // race against the same dead session.
                tracing::info!(
                    "cloud provider session '{stale_session_id}' health check failed, reconnecting"
                );
                // Take the stale entry out of the registry *and* best-effort
                // close its remote provider session. A failed `Target.getTargets`
                // can mean the remote is dead, but it can also mean the WS is
                // wedged while the paid remote browser is still alive — without
                // the explicit close, that's a billed orphan session, which is
                // the exact leak this PR is trying to prevent elsewhere.
                let stale_entry = {
                    let mut reg = registry.lock().await;
                    reg.remove(&stale_session_id)
                };
                if let Some(entry) = stale_entry
                    && let Some(ps) = entry.provider_session.as_ref()
                    && let Err(err) =
                        crate::browser::session::provider::close_provider_session(ps).await
                {
                    tracing::warn!(
                        "failed to clean up stale provider session '{}' for provider '{}': {err}",
                        ps.session_id,
                        ps.provider
                    );
                }
            }

            let provider_connection = match connect_provider(
                provider_name,
                profile_name,
                headless,
                cmd.stealth,
                &cmd.provider_env,
            )
            .await
            {
                Ok(connection) => connection,
                Err(err) => return ActionResult::fatal(err.error_code(), err.to_string()),
            };

            let mut combined_headers = provider_connection.headers.clone();
            combined_headers.extend(headers.clone());

            return execute_cloud(
                cmd,
                registry,
                &provider_connection.cdp_endpoint,
                &combined_headers,
                profile_name,
                headless,
                Some(provider_connection.provider.as_str()),
                provider_connection.session.clone(),
                cmd.max_tracked_requests,
            )
            .await;
        }

        return execute_cloud(
            cmd,
            registry,
            cdp_endpoint.unwrap(),
            &headers,
            profile_name,
            headless,
            None,
            None,
            cmd.max_tracked_requests,
        )
        .await;
    }

    // ── Extension mode ─────────────────────────────────────────────
    if mode == Mode::Extension {
        if cdp_endpoint.is_some() {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                "--cdp-endpoint is not supported with --mode extension".to_string(),
            );
        }
        return execute_extension(cmd, registry, profile_name, headless).await;
    }

    // ── Local mode ─────────────────────────────────────────────────

    // Guard: only Local mode should reach here. Cloud and Extension return
    // earlier; if a new mode is added but not handled, fail explicitly rather
    // than silently launching a local Chrome.
    if mode != Mode::Local {
        return ActionResult::fatal_with_hint(
            "UNSUPPORTED_MODE",
            format!("mode '{mode:?}' is not supported by this daemon version"),
            "upgrade the CLI binary and restart the daemon",
        );
    }

    let disposition = {
        let mut reg = registry.lock().await;

        if cdp_endpoint.is_none()
            && effective_set_id.is_none()
            && mode == Mode::Local
            && let Some(existing) = reg.find_local_session_by_profile(profile_name, mode)
        {
            match existing.status {
                SessionState::Running => StartDisposition::Reuse(ReuseTarget {
                    session_id: existing.id.as_str().to_string(),
                    first_tab_id: existing
                        .tabs
                        .first()
                        .map(|tab| tab.id.0.clone())
                        .unwrap_or_default(),
                    first_native_id: existing
                        .tabs
                        .first()
                        .map(|tab| tab.native_id.clone())
                        .unwrap_or_default(),
                    cdp: existing.cdp.clone(),
                    cdp_port: existing.cdp_port,
                }),
                SessionState::Starting => {
                    return ActionResult::fatal_with_hint(
                        "SESSION_STARTING",
                        format!("session for profile '{profile_name}' is starting, please wait"),
                        "retry after a few seconds or use browser status to check",
                    );
                }
                SessionState::Closing | SessionState::Closed => {
                    unreachable!("inactive sessions are excluded from lookup via is_active()")
                }
            }
        } else {
            match reg.reserve_session_start(
                effective_set_id,
                cmd.profile.as_deref(),
                profile_name,
                mode,
                headless,
                cmd.stealth,
            ) {
                Ok(session_id) => StartDisposition::Reserved(session_id),
                Err(e @ crate::error::CliError::SessionAlreadyExists { .. })
                | Err(e @ crate::error::CliError::SessionIdAlreadyExists(_)) => {
                    let hint = e.hint();
                    return ActionResult::fatal_with_hint(e.error_code(), e.to_string(), &hint);
                }
                Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
            }
        }
    };

    let session_id = match disposition {
        StartDisposition::Reuse(target) => {
            return reuse_running_session(cmd, registry, target).await;
        }
        StartDisposition::Reserved(session_id) => session_id,
    };

    let profiles_dir = config::profiles_dir();
    std::fs::create_dir_all(&profiles_dir).ok();
    let user_data_dir = profiles_dir.join(profile_name);
    std::fs::create_dir_all(&user_data_dir).ok();

    // Set Chrome profile display name for the default "actionbook" profile.
    if profile_name == DEFAULT_PROFILE
        && let Err(e) = ensure_profile_display_name(&user_data_dir)
    {
        tracing::warn!("failed to set profile display name: {e}");
    }

    for lock in &["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        let p = user_data_dir.join(lock);
        if p.exists() {
            std::fs::remove_file(&p).ok();
        }
    }

    // Kill any orphan Chrome from a previous daemon crash/SIGKILL.
    // When the daemon is SIGKILL'd, it cannot run its graceful shutdown path,
    // so Chrome is left alive using the same user-data-dir. A new Chrome
    // launched against the same dir would race with the orphan and likely crash,
    // causing discover_ws_url to time out with CDP_CONNECTION_FAILED.
    //
    // Windows: remember the orphan PID so we can give an actionable error
    // message if Chrome still fails to start (kill may fail in some environments).
    #[cfg(windows)]
    let mut orphan_pid_hint: Option<u32> = None;

    let chrome_pid_file = user_data_dir.join("chrome.pid");
    if let Ok(pid_str) = std::fs::read_to_string(&chrome_pid_file) {
        if let Ok(_pid) = pid_str.trim().parse::<i32>() {
            #[cfg(unix)]
            {
                unsafe extern "C" {
                    safe fn kill(pid: i32, sig: i32) -> i32;
                }
                // kill(pid, 0) checks liveness without sending a signal (POSIX).
                if kill(_pid, 0) == 0 {
                    kill(_pid, 9); // SIGKILL orphan
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
            #[cfg(windows)]
            {
                // Reopen the named Job Object left by the crashed daemon and
                // terminate it — kills the orphan Chrome main process and all
                // helpers (renderer, GPU, utility) atomically.
                if let Some(job) = crate::daemon::chrome_reaper::ChromeJobObject::open(profile_name)
                {
                    tracing::debug!(
                        profile_name,
                        "orphan recovery: terminating Job Object for profile"
                    );
                    job.terminate();
                    // Brief wait for processes to fully exit before we try to
                    // acquire the user-data-dir lock for the new Chrome instance.
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                // Fallback: directly kill the known orphan PID in case the Job
                // Object was already released (e.g. Chrome exited on its own).
                crate::daemon::chrome_reaper::terminate_pid_and_wait(_pid as u32);
                // Remember the PID in case the kill failed and Chrome still holds
                // the user-data-dir lock; used for a clearer error message below.
                orphan_pid_hint = Some(_pid as u32);
            }
        }
        let _ = std::fs::remove_file(&chrome_pid_file);
    }

    // Windows: Job Object is created inside the local-Chrome branch below and
    // stored here so it survives the if-else scope and reaches the registry.
    #[cfg(windows)]
    let mut chrome_job: Option<crate::daemon::chrome_reaper::ChromeJobObject> = None;

    let (mut chrome_process, port, ws_url, mut targets) = if let Some(endpoint) = cdp_endpoint {
        let (ws_url, port) = match browser::resolve_cdp_endpoint(endpoint).await {
            Ok(value) => value,
            Err(e) => {
                return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string())
                    .await;
            }
        };

        let mut targets = browser::list_targets(port).await.unwrap_or_default();
        if let Some(url) = &cmd.open_url
            && let Some(target_id) = targets
                .first()
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
        {
            let page_ws = format!("ws://127.0.0.1:{port}/devtools/page/{target_id}");
            if let Err(e) = cdp_navigate(
                &page_ws,
                &ensure_scheme(url).unwrap_or_else(|_| "about:blank".to_string()),
            )
            .await
            {
                return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string())
                    .await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            targets = browser::list_targets(port).await.unwrap_or(targets);
        }

        (None, Some(port), ws_url, targets)
    } else {
        let executable = if let Some(executable) = cmd.executable_path.as_deref() {
            executable.to_string()
        } else {
            match browser::find_chrome() {
                Ok(e) => e,
                Err(e) => {
                    return fail_reserved_start(
                        registry,
                        &session_id,
                        e.error_code(),
                        e.to_string(),
                    )
                    .await;
                }
            }
        };

        let (chrome, port) = match browser::launch_chrome(
            &executable,
            headless,
            &user_data_dir.to_string_lossy(),
            None,
            cmd.stealth,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                return fail_reserved_start(registry, &session_id, e.error_code(), e.to_string())
                    .await;
            }
        };

        let ws_url = match browser::discover_ws_url(port).await {
            Ok(ws) => ws,
            Err(e) => {
                // On Windows, if we detected an orphan PID but couldn't kill it
                // (e.g. Job Object nesting restrictions in CI), give an actionable
                // error instead of a generic CDP_CONNECTION_FAILED timeout.
                #[cfg(windows)]
                if let Some(orphan_pid) = orphan_pid_hint {
                    return fail_reserved_start_with_chrome(
                        registry,
                        &session_id,
                        Some(chrome),
                        "CHROME_ORPHAN_STILL_RUNNING",
                        format!(
                            "Chrome from a previous session (PID {orphan_pid}) is still \
                             running and holding the profile lock. Kill it manually: \
                             taskkill /F /IM chrome.exe"
                        ),
                    )
                    .await;
                }
                return fail_reserved_start_with_chrome(
                    registry,
                    &session_id,
                    Some(chrome),
                    e.error_code(),
                    e.to_string(),
                )
                .await;
            }
        };

        let targets = browser::list_targets(port).await.unwrap_or_default();
        // Write Chrome PID so a future daemon restart can detect and kill this
        // process if the daemon is SIGKILL'd before it can run graceful shutdown.
        let _ = std::fs::write(&chrome_pid_file, chrome.id().to_string());

        // Windows: create a named Job Object and assign Chrome's main process
        // to it.  All Chrome child processes (renderer, GPU, utility) inherit
        // job membership automatically.  TerminateJobObject on close/restart
        // kills the entire group atomically — no WMI or process enumeration.
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let job = crate::daemon::chrome_reaper::ChromeJobObject::create(profile_name);
            if let Some(ref j) = job {
                // RawHandle (*mut c_void) == HANDLE (*mut c_void) in windows-sys 0.59
                j.assign(chrome.as_raw_handle() as _);
            }
            chrome_job = job;
        }

        (Some(chrome), Some(port), ws_url, targets)
    };

    if targets
        .first()
        .and_then(|t| t.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
        && let Some(p) = port
    {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        targets = browser::list_targets(p).await.unwrap_or(targets);
    }

    // Collect (native_id, url, title) tuples; short IDs are assigned when pushed into the entry.
    let mut native_tabs: Vec<(String, String, String)> = Vec::new();
    for t in &targets {
        let native_id = t
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !native_id.is_empty() {
            let url = t
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = t
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            native_tabs.push((native_id, url, title));
        }
    }

    // Create persistent CDP connection and attach all initial tabs
    let cdp = match CdpSession::connect_with_config(&ws_url, &[], cmd.max_tracked_requests).await {
        Ok(c) => c,
        Err(e) => {
            return fail_reserved_start_with_chrome(
                registry,
                &session_id,
                chrome_process.take(),
                "CDP_CONNECTION_FAILED",
                e.to_string(),
            )
            .await;
        }
    };
    // Fetch real User-Agent from browser, strip Headless markers for stealth.
    // Only fetched when stealth is enabled; passed to attach() which gates injection on Some(ua).
    let user_agent: Option<String> = if cmd.stealth {
        if let Ok(v) = cdp
            .execute("Browser.getVersion", serde_json::json!({}), None)
            .await
        {
            let raw = v["result"]["userAgent"].as_str().unwrap_or("").to_string();
            let ua = raw
                .replace("HeadlessChrome", "Chrome")
                .replace("Headless", "");
            if ua.is_empty() { None } else { Some(ua) }
        } else {
            None
        }
    } else {
        None
    };

    for (native_id, ..) in &native_tabs {
        if let Err(e) = cdp.attach(native_id, user_agent.as_deref()).await {
            tracing::warn!("failed to attach tab {native_id}: {e}");
        }
    }

    let first_native_id = native_tabs.first().map(|t| t.0.clone()).unwrap_or_default();

    // Navigate to open_url after attach so the stealth script is already injected.
    if let Some(url) = &cmd.open_url
        && !first_native_id.is_empty()
    {
        let final_url = ensure_scheme(url).unwrap_or_else(|_| "about:blank".to_string());
        let _ = cdp
            .execute_on_tab(
                &first_native_id,
                "Page.navigate",
                serde_json::json!({ "url": final_url }),
            )
            .await;
        // Update native_tabs[0] URL to reflect the navigated URL so the registry
        // stores the correct URL when push_tab is called below.
        if let Some(first) = native_tabs.first_mut() {
            first.1 = final_url;
        }
    }

    // Get real-time info for the first tab
    let (first_url, first_title) = if !first_native_id.is_empty() {
        get_tab_info_from_targets(&targets, &first_native_id)
    } else {
        (
            cmd.open_url.as_deref().unwrap_or("about:blank").to_string(),
            String::new(),
        )
    };

    let mut reg = registry.lock().await;
    let Some(entry) = reg.get_mut(session_id.as_str()) else {
        drop(reg);
        cleanup_chrome_process(chrome_process.take());
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!(
                "session '{}' was closed during startup",
                session_id.as_str()
            ),
        );
    };
    entry.mode = mode;
    entry.headless = headless;
    entry.profile = profile_name.to_string();
    entry.status = SessionState::Running;
    entry.cdp_port = port;
    entry.ws_url = ws_url.clone();
    entry.max_tracked_requests = cmd.max_tracked_requests;
    for (native_id, url, title) in native_tabs {
        entry.push_tab(native_id, url, title);
    }
    entry.chrome_process = chrome_process;
    #[cfg(windows)]
    {
        entry.job_object = chrome_job;
    }
    entry.cdp = Some(cdp);
    entry.stealth_ua = user_agent;

    // Create per-session data directory for artifacts (snapshots, etc.)
    let session_data_dir = config::session_data_dir(session_id.as_str());
    std::fs::create_dir_all(&session_data_dir).ok();

    let first_short_id = entry
        .tabs
        .first()
        .map(|t| t.id.0.clone())
        .unwrap_or_default();

    ActionResult::ok(json!({
        "session": {
            "session_id": session_id.as_str(),
            "mode": mode.to_string(),
            "status": "running",
            "headless": headless,
            // Local loopback CDP URLs must be emitted verbatim so the caller
            // can actually connect (e.g. `chrome --remote-debugging-port` or
            // DevTools). `endpoint_for_mode` skips redaction for non-cloud
            // modes and keeps it for cloud.
            "cdp_endpoint": endpoint_for_mode(mode, &ws_url),
        },
        "tab": {
            "tab_id": first_short_id,
            "native_tab_id": first_native_id,
            "url": first_url,
            "title": first_title,
        },
        "reused": false,
    }))
}

/// Extract url/title for a target_id from a targets list.
fn get_tab_info_from_targets(targets: &[serde_json::Value], target_id: &str) -> (String, String) {
    for t in targets {
        if t.get("id").and_then(|v| v.as_str()) == Some(target_id) {
            let url = t
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = t
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return (url, title);
        }
    }
    (String::new(), String::new())
}

async fn reuse_running_session(
    cmd: &Cmd,
    registry: &SharedRegistry,
    target: ReuseTarget,
) -> ActionResult {
    if let Some(url) = &cmd.open_url {
        let final_url = ensure_scheme(url).unwrap_or_else(|_| "about:blank".to_string());
        if let Some(ref cdp) = target.cdp
            && !target.first_native_id.is_empty()
        {
            let nav_result = cdp
                .execute_on_tab(
                    &target.first_native_id,
                    "Page.navigate",
                    serde_json::json!({ "url": final_url }),
                )
                .await;
            if let Err(e) = nav_result {
                return cdp_error_to_result(e, "NAVIGATION_FAILED");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    // For local sessions, refresh tab info from /json/list; for cloud, use registry
    let (tab_url, tab_title) = if let Some(port) = target.cdp_port {
        let targets = browser::list_targets(port).await.unwrap_or_default();
        get_tab_info_from_targets(&targets, &target.first_native_id)
    } else {
        // Cloud: get info from registry
        let reg = registry.lock().await;
        if let Some(entry) = reg.get(&target.session_id) {
            entry
                .tabs
                .iter()
                .find(|t| t.native_id == target.first_native_id)
                .map(|t| (t.url.clone(), t.title.clone()))
                .unwrap_or_default()
        } else {
            (String::new(), String::new())
        }
    };

    let reg = registry.lock().await;
    let Some(entry) = reg.get(&target.session_id) else {
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!("session '{}' not found", target.session_id),
        );
    };

    ActionResult::ok(json!({
        "session": {
            "session_id": entry.id.as_str(),
            "mode": entry.mode.to_string(),
            "status": entry.status.to_string(),
            "headless": entry.headless,
            // Cloud ws_urls embed tokens (e.g. Hyperbrowser JWT) as query
            // params and must be redacted; local loopback ws_urls must be
            // emitted verbatim so the caller can actually attach.
            "cdp_endpoint": endpoint_for_mode(entry.mode, &entry.ws_url),
        },
        "tab": {
            "tab_id": target.first_tab_id,
            "url": tab_url,
            "title": tab_title,
        },
        "reused": true,
    }))
}

async fn fail_reserved_start(
    registry: &SharedRegistry,
    session_id: &SessionId,
    code: &str,
    message: String,
) -> ActionResult {
    registry.lock().await.remove(session_id.as_str());
    ActionResult::fatal(code, message)
}

async fn fail_reserved_start_with_hint(
    registry: &SharedRegistry,
    session_id: &SessionId,
    code: &str,
    message: String,
    hint: &'static str,
) -> ActionResult {
    registry.lock().await.remove(session_id.as_str());
    ActionResult::fatal_with_hint(code, message, hint)
}

/// URL schemes that Chrome's `chrome.debugger.attach` refuses with -32000.
/// Detecting them pre-/post-attach lets us surface a clean `RESTRICTED_ACTIVE_TAB`
/// error instead of leaking Chrome's raw "Cannot access a chrome:// URL" message
/// (or, worse, a transient "session closed" flake when the extension drops the
/// WS on error). The list matches Chromium's debugger_api.cc restriction set.
fn is_restricted_attach_scheme(url: &str) -> bool {
    const RESTRICTED: &[&str] = &[
        "chrome://",
        "chrome-extension://",
        "chrome-untrusted://",
        "chrome-search://",
        "devtools://",
        "edge://",
        "view-source:",
    ];
    RESTRICTED.iter().any(|p| url.starts_with(p))
}

/// Detects Chrome's CDP rejection when `debugger.attach` targets a restricted
/// tab. Chrome's message is stable across versions: "Cannot access a chrome://
/// URL" / "Cannot access a chrome-extension:// URL" / etc.
fn is_chrome_attach_restriction_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("cannot access")
        && (lower.contains("chrome://")
            || lower.contains("chrome-extension://")
            || lower.contains("chrome-untrusted://")
            || lower.contains("devtools://")
            || lower.contains("edge://"))
}

/// Tear down a provider-side session as part of a failed cloud start.
///
/// `panic = "abort"` means we cannot rely on `Drop` for this — every error
/// branch in `execute_cloud` must funnel through this helper, otherwise we
/// leak paid provider sessions.
async fn cleanup_provider_session_if_any(provider_session: &Option<ProviderSession>) {
    if let Some(ps) = provider_session
        && let Err(err) = crate::browser::session::provider::close_provider_session(ps).await
    {
        tracing::warn!(
            "failed to clean up provider session '{}' for provider '{}': {err}",
            ps.session_id,
            ps.provider
        );
    }
}

/// Helper that pairs `cleanup_provider_session_if_any` with `fail_reserved_start`
/// so all four error branches in `execute_cloud` reduce to a single call.
async fn fail_reserved_cloud_start(
    registry: &SharedRegistry,
    session_id: &SessionId,
    provider_session: &Option<ProviderSession>,
    code: &str,
    message: String,
) -> ActionResult {
    cleanup_provider_session_if_any(provider_session).await;
    fail_reserved_start(registry, session_id, code, message).await
}

async fn fail_reserved_start_with_chrome(
    registry: &SharedRegistry,
    session_id: &SessionId,
    chrome_process: Option<Child>,
    code: &str,
    message: String,
) -> ActionResult {
    cleanup_chrome_process(chrome_process);
    registry.lock().await.remove(session_id.as_str());
    ActionResult::fatal(code, message)
}

fn cleanup_chrome_process(mut chrome_process: Option<Child>) {
    crate::daemon::chrome_reaper::kill_and_reap_option(&mut chrome_process);
}

// ── Cloud mode ──────────────────────────────────────────────────────

/// Execute cloud-mode session start.
///
/// Cloud sessions connect directly to a remote CDP endpoint via WebSocket
/// with optional auth headers. No local Chrome process is launched.
///
/// The argument list is wider than clippy's 7-arg heuristic because all of
/// these inputs are resolved by the caller (either from the CLI `start`
/// command or from a `restart` handoff) and have no natural grouping —
/// folding them into a struct would just be a lint-placation rename.
#[allow(clippy::too_many_arguments)]
async fn execute_cloud(
    cmd: &Cmd,
    registry: &SharedRegistry,
    cdp_endpoint: &str,
    headers: &[(String, String)],
    profile_name: &str,
    headless: bool,
    provider_name: Option<&str>,
    provider_session: Option<ProviderSession>,
    max_tracked_requests: usize,
) -> ActionResult {
    let ws_url = match ensure_scheme_or_fatal(cdp_endpoint) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let effective_set_id = cmd.session.as_deref().or(cmd.set_session_id.as_deref());

    // ── Cloud session reuse: match on cdp_endpoint ──
    if effective_set_id.is_none() {
        let reg = registry.lock().await;
        if let Some(existing) = reg.find_cloud_session_by_endpoint(cdp_endpoint) {
            match existing.status {
                SessionState::Running => {
                    // Health check via CDP — if it fails, drop old session and reconnect
                    if let Some(ref cdp) = existing.cdp {
                        match cdp.execute_browser("Target.getTargets", json!({})).await {
                            Ok(_) => {
                                return make_session_response(
                                    existing.id.as_str(),
                                    existing,
                                    true,
                                    cdp_endpoint,
                                );
                            }
                            Err(_) => {
                                // Stale — will fall through to create new session
                                tracing::info!(
                                    "cloud session '{}' health check failed, reconnecting",
                                    existing.id.as_str()
                                );
                            }
                        }
                    }
                }
                SessionState::Starting => {
                    return ActionResult::fatal_with_hint(
                        "SESSION_STARTING",
                        format!(
                            "cloud session for endpoint '{}' is starting, please wait",
                            redact_endpoint(cdp_endpoint)
                        ),
                        "retry after a few seconds or use browser status to check",
                    );
                }
                // Closing / Closed are filtered by `find_cloud_session_by_endpoint`
                // via `is_active()`; these arms only satisfy exhaustiveness.
                SessionState::Closing | SessionState::Closed => {}
            }
        }
    }

    // ── Reserve placeholder ──
    let session_id = {
        let mut reg = registry.lock().await;
        match reg.reserve_session_start(
            effective_set_id,
            cmd.profile.as_deref(),
            profile_name,
            Mode::Cloud,
            headless,
            cmd.stealth,
        ) {
            Ok(sid) => sid,
            Err(e) => {
                cleanup_provider_session_if_any(&provider_session).await;
                return ActionResult::fatal(e.error_code(), e.to_string());
            }
        }
    };

    // ── Connect with config ──
    let cdp = match CdpSession::connect_with_config(&ws_url, headers, max_tracked_requests).await {
        Ok(c) => c,
        Err(e) => {
            return fail_reserved_cloud_start(
                registry,
                &session_id,
                &provider_session,
                e.error_code(),
                e.to_string(),
            )
            .await;
        }
    };

    // ── Discover tabs via Target.getTargets ──
    let tabs = match discover_tabs_via_cdp(&cdp).await {
        Ok(t) => t,
        Err(e) => {
            return fail_reserved_cloud_start(
                registry,
                &session_id,
                &provider_session,
                "CDP_ERROR",
                e.to_string(),
            )
            .await;
        }
    };

    // ── Zero-tab fallback: create a new page ──
    let tabs = if tabs.is_empty() {
        let open_url = cmd.open_url.as_deref().unwrap_or("about:blank");
        match create_tab_via_cdp(&cdp, open_url).await {
            Ok(tab) => vec![tab],
            Err(e) => {
                return fail_reserved_cloud_start(
                    registry,
                    &session_id,
                    &provider_session,
                    "CDP_ERROR",
                    format!("failed to create initial tab: {e}"),
                )
                .await;
            }
        }
    } else {
        tabs
    };

    // Attach all tabs
    for (native_id, ..) in &tabs {
        if let Err(e) = cdp.attach(native_id, None).await {
            tracing::warn!("cloud: failed to attach tab {native_id}: {e}");
        }
    }

    // Navigate first tab if open_url provided and we didn't just create with it
    if let Some(url) = &cmd.open_url
        && !tabs.is_empty()
        && tabs[0].1 != *url
    {
        let final_url = match ensure_scheme_or_fatal(url) {
            Ok(u) => u,
            Err(e) => {
                cleanup_provider_session_if_any(&provider_session).await;
                registry.lock().await.remove(session_id.as_str());
                return e;
            }
        };
        let first_native = &tabs[0].0;
        if let Err(e) = cdp
            .execute_on_tab(first_native, "Page.navigate", json!({ "url": final_url }))
            .await
        {
            tracing::warn!("cloud: navigate on start failed: {e}");
        }
    }

    let first_native_id = tabs.first().map(|t| t.0.clone()).unwrap_or_default();
    let first_url = tabs
        .first()
        .map(|t| t.1.clone())
        .unwrap_or_else(|| "about:blank".to_string());
    let first_title = tabs.first().map(|t| t.2.clone()).unwrap_or_default();

    // ── Finalize registry entry ──
    // Race window: the placeholder reserved at the top of this function can be
    // removed by a concurrent `close`/`restart` while we were busy minting the
    // remote session. In that case the local `provider_session` variable holds
    // a handle the registry never got to see — no other caller will ever
    // release it — so we must tear it down here before returning, otherwise
    // the cloud browser leaks and keeps billing.
    let mut reg = registry.lock().await;
    let Some(entry) = reg.get_mut(session_id.as_str()) else {
        drop(reg);
        cleanup_provider_session_if_any(&provider_session).await;
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!(
                "session '{}' was closed during startup",
                session_id.as_str()
            ),
        );
    };
    entry.mode = Mode::Cloud;
    entry.headless = headless;
    entry.profile = profile_name.to_string();
    entry.status = SessionState::Running;
    entry.cdp_port = None;
    entry.ws_url = ws_url.clone();
    entry.max_tracked_requests = max_tracked_requests;
    for (native_id, url, title) in tabs {
        entry.push_tab(native_id, url, title);
    }
    entry.chrome_process = None;
    entry.cdp = Some(cdp);
    entry.cdp_endpoint = Some(cdp_endpoint.to_string());
    entry.headers = headers.to_vec();
    entry.provider = provider_name.map(|provider| provider.to_string());
    entry.provider_session = provider_session;

    // Create per-session data directory for artifacts (snapshots, etc.)
    let session_data_dir = config::session_data_dir(session_id.as_str());
    std::fs::create_dir_all(&session_data_dir).ok();

    let first_short_id = entry
        .tabs
        .first()
        .map(|t| t.id.0.clone())
        .unwrap_or_default();

    ActionResult::ok(json!({
        "session": {
            "session_id": session_id.as_str(),
            "mode": "cloud",
            "status": "running",
            "headless": headless,
            "cdp_endpoint": redact_endpoint(cdp_endpoint),
            "provider": provider_name,
        },
        "tab": {
            "tab_id": first_short_id,
            "native_tab_id": first_native_id,
            "url": first_url,
            "title": first_title,
        },
        "reused": false,
    }))
}

// ── Extension mode ────────────────────────────────────────────────────

/// Execute extension-mode session start.
///
/// Connects to the extension bridge WS, which transparently relays CDP
/// commands to the Chrome extension. Reuses the same CDP flow as cloud mode.
async fn execute_extension(
    cmd: &Cmd,
    registry: &SharedRegistry,
    profile_name: &str,
    headless: bool,
) -> ActionResult {
    use crate::daemon::bridge::{BRIDGE_PORT, BridgeError, ensure_bridge};

    // Lazy bridge: bind 19222 on the first --mode extension call.
    // ensure_bridge is idempotent and recovers from a previous Failed state.
    let bridge_state = match ensure_bridge(registry).await {
        Ok(bs) => bs,
        Err(BridgeError::BindFailed {
            port,
            source,
            holder,
        }) => {
            let (message, hint) = match holder {
                Some(h) => (
                    format!(
                        "extension bridge failed to bind port {port} (held by {} pid {}): {source}",
                        h.command, h.pid
                    ),
                    format!(
                        "stop {} (pid {}) — e.g. `kill {}` — then run `actionbook daemon restart`",
                        h.command, h.pid, h.pid
                    ),
                ),
                None => {
                    // Platform-specific diagnostic command in the hint:
                    // `lsof` ships on macOS/Linux by default; Windows users
                    // need `netstat -ano` + tasklist.
                    #[cfg(unix)]
                    let diag = format!("lsof -iTCP:{port} -sTCP:LISTEN");
                    #[cfg(windows)]
                    let diag = format!("netstat -ano | findstr :{port}");
                    #[cfg(not(any(unix, windows)))]
                    let diag = format!("your system's port-lookup tool for port {port}");
                    (
                        format!("extension bridge failed to bind port {port}: {source}"),
                        format!(
                            "another process is holding port {port} — run `{diag}` to find it, then `actionbook daemon restart`"
                        ),
                    )
                }
            };
            return ActionResult::fatal_with_hint("BRIDGE_BIND_FAILED", message, hint);
        }
    };

    // Bridge is bound; now wait for the Chrome extension's WS handshake to
    // complete (the extension uses exponential-backoff reconnect after a
    // daemon restart so it can take 100ms–2s).
    let bridge_ws_url = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            {
                let bs = bridge_state.lock().await;
                if bs.is_extension_connected() {
                    break;
                }
            }
            if std::time::Instant::now() >= deadline {
                return ActionResult::fatal_with_hint(
                    "EXTENSION_NOT_CONNECTED",
                    "no Chrome extension connected to the bridge within 5s",
                    "open chrome://extensions, ensure the Actionbook extension is enabled and its popup shows Connected",
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        format!("ws://127.0.0.1:{BRIDGE_PORT}")
    };

    // Reserve a session placeholder.
    let effective_set_id = cmd.session.as_deref().or(cmd.set_session_id.as_deref());
    let session_id = {
        let mut reg = registry.lock().await;
        match reg.reserve_session_start(
            effective_set_id,
            cmd.profile.as_deref(),
            profile_name,
            Mode::Extension,
            headless,
            cmd.stealth,
        ) {
            Ok(sid) => sid,
            Err(e) => return ActionResult::fatal(e.error_code(), e.to_string()),
        }
    };

    // Connect CdpSession to bridge (transparent relay to extension).
    let cdp = match CdpSession::connect_with_config(&bridge_ws_url, &[], cmd.max_tracked_requests)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return fail_reserved_start(
                registry,
                &session_id,
                "CDP_CONNECTION_FAILED",
                format!("failed to connect to extension bridge: {e}"),
            )
            .await;
        }
    };

    // Extension-specific tab discovery via Extension.listTabs / Extension.attachTab.
    //
    // Unlike local/cloud mode (which use CDP Target.getTargets), the extension
    // bridge requires an Extension.attachTab call before any CDP command can be
    // relayed.  We use the Extension.* custom methods to list, create, and attach
    // tabs, then register them in CdpSession so subsequent execute_on_tab works.

    let open_url = cmd.open_url.as_deref();

    // If open_url provided, create (or reuse) a tab via Extension.createTab
    // which auto-attaches the debugger.  Otherwise attach the active tab.
    let tabs: Vec<(String, String, String)> = if let Some(url) = open_url {
        let final_url = match ensure_scheme_or_fatal(url) {
            Ok(u) => u,
            Err(e) => {
                cdp.close().await;
                registry.lock().await.remove(session_id.as_str());
                return e;
            }
        };
        match cdp
            .execute_browser("Extension.createTab", json!({ "url": final_url }))
            .await
        {
            Ok(resp) => {
                let result = &resp["result"];
                let tab_id = result["tabId"].as_i64().unwrap_or(0).to_string();
                let tab_url = result["url"].as_str().unwrap_or(&final_url).to_string();
                let title = result["title"].as_str().unwrap_or("").to_string();
                vec![(tab_id, tab_url, title)]
            }
            Err(e) => {
                cdp.close().await;
                return fail_reserved_start(
                    registry,
                    &session_id,
                    "CDP_ERROR",
                    format!("failed to create tab via extension: {e}"),
                )
                .await;
            }
        }
    } else if let Some(explicit_tab_id) = cmd.tab_id {
        // Explicit attach to a pre-existing Chrome tab by its native id.
        // No "active tab" default in protocol 0.3.0 — every attach is explicit.
        match cdp
            .execute_browser("Extension.attachTab", json!({ "tabId": explicit_tab_id }))
            .await
        {
            Ok(resp) => {
                let result = &resp["result"];
                let tab_id = result["tabId"]
                    .as_i64()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| explicit_tab_id.to_string());
                let tab_url = result["url"].as_str().unwrap_or("about:blank").to_string();
                let title = result["title"].as_str().unwrap_or("").to_string();
                if is_restricted_attach_scheme(&tab_url) {
                    cdp.close().await;
                    return fail_reserved_start_with_hint(
                        registry,
                        &session_id,
                        "RESTRICTED_TAB",
                        format!("tab {explicit_tab_id} is a restricted URL: {tab_url}"),
                        "pass --open-url <url> or pick a different --tab-id",
                    )
                    .await;
                }
                vec![(tab_id, tab_url, title)]
            }
            Err(e) => {
                let msg = e.to_string();
                cdp.close().await;
                if is_chrome_attach_restriction_error(&msg) {
                    return fail_reserved_start_with_hint(
                        registry,
                        &session_id,
                        "RESTRICTED_TAB",
                        format!("tab {explicit_tab_id} cannot be debugged"),
                        "pass --open-url <url> or pick a different --tab-id",
                    )
                    .await;
                }
                return fail_reserved_start(
                    registry,
                    &session_id,
                    "CDP_ERROR",
                    format!("failed to attach tab {explicit_tab_id} via extension: {e}"),
                )
                .await;
            }
        }
    } else {
        // Neither --open-url nor --tab-id: extension mode requires one.
        cdp.close().await;
        return fail_reserved_start_with_hint(
            registry,
            &session_id,
            "MISSING_TAB_TARGET",
            "extension mode requires either --open-url or --tab-id".to_string(),
            "pass --open-url <url> to create a new tab, or --tab-id <N>. list-tabs only shows Actionbook-managed tabs; for unmanaged tabs read the id from chrome://inspect/#pages",
        )
        .await;
    };

    // Register extension tabs in CdpSession so execute_on_tab works.
    // Extension bridge ignores sessionId, so an empty string is fine.
    for (native_id, ..) in &tabs {
        cdp.register_extension_tab(native_id).await;
    }

    let first_native_id = tabs.first().map(|t| t.0.clone()).unwrap_or_default();
    let first_url = tabs
        .first()
        .map(|t| t.1.clone())
        .unwrap_or_else(|| "about:blank".to_string());
    let first_title = tabs.first().map(|t| t.2.clone()).unwrap_or_default();

    // Finalize registry entry.
    let mut reg = registry.lock().await;
    let Some(entry) = reg.get_mut(session_id.as_str()) else {
        return ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!(
                "session '{}' was closed during startup",
                session_id.as_str()
            ),
        );
    };
    entry.mode = Mode::Extension;
    entry.headless = headless;
    entry.profile = profile_name.to_string();
    entry.status = SessionState::Running;
    entry.cdp_port = None;
    entry.ws_url = bridge_ws_url.clone();
    entry.max_tracked_requests = cmd.max_tracked_requests;
    for (native_id, url, title) in tabs {
        entry.push_tab(native_id, url, title);
    }
    entry.chrome_process = None;
    entry.cdp = Some(cdp);

    let session_data_dir = config::session_data_dir(session_id.as_str());
    std::fs::create_dir_all(&session_data_dir).ok();

    let first_short_id = entry
        .tabs
        .first()
        .map(|t| t.id.0.clone())
        .unwrap_or_default();

    ActionResult::ok(json!({
        "session": {
            "session_id": session_id.as_str(),
            "mode": "extension",
            "status": "running",
            "headless": headless,
        },
        "tab": {
            "tab_id": first_short_id,
            "native_tab_id": first_native_id,
            "url": first_url,
            "title": first_title,
        },
        "reused": false,
    }))
}

/// Discover page tabs via CDP Target.getTargets.
/// Returns (native_id, url, title) tuples; short IDs are assigned by `SessionEntry::push_tab`.
async fn discover_tabs_via_cdp(
    cdp: &CdpSession,
) -> Result<Vec<(String, String, String)>, crate::error::CliError> {
    let resp = cdp.execute_browser("Target.getTargets", json!({})).await?;
    let target_infos = resp
        .pointer("/result/targetInfos")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let tabs = target_infos
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
        .filter_map(|t| {
            let native_id = t.get("targetId").and_then(|v| v.as_str())?;
            let url = t
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = t
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some((native_id.to_string(), url, title))
        })
        .collect();
    Ok(tabs)
}

/// Create a new tab via CDP Target.createTarget.
/// Returns (native_id, url, title); short ID is assigned by `SessionEntry::push_tab`.
async fn create_tab_via_cdp(
    cdp: &CdpSession,
    url: &str,
) -> Result<(String, String, String), crate::error::CliError> {
    let resp = cdp
        .execute_browser("Target.createTarget", json!({ "url": url }))
        .await?;
    let native_id = resp
        .pointer("/result/targetId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::error::CliError::cdp_with_code(
                crate::daemon::cdp_error_classifier::CdpErrorCode::ProtocolError,
                "Target.createTarget did not return targetId",
                None,
            )
        })?
        .to_string();
    Ok((native_id, url.to_string(), String::new()))
}

/// Build a session response for reuse or new session.
fn make_session_response(
    session_id: &str,
    entry: &crate::daemon::registry::SessionEntry,
    reused: bool,
    cdp_endpoint: &str,
) -> ActionResult {
    let first_tab = entry.tabs.first();
    ActionResult::ok(json!({
        "session": {
            "session_id": session_id,
            "mode": entry.mode.to_string(),
            "status": entry.status.to_string(),
            "headless": entry.headless,
            "cdp_endpoint": redact_endpoint(cdp_endpoint),
            "provider": entry.provider,
        },
        "tab": {
            "tab_id": first_tab.map(|t| t.id.0.as_str()).unwrap_or(""),
            "native_tab_id": first_tab.map(|t| t.native_id.as_str()).unwrap_or(""),
            "url": first_tab.map(|t| t.url.as_str()).unwrap_or(""),
            "title": first_tab.map(|t| t.title.as_str()).unwrap_or(""),
        },
        "reused": reused,
    }))
}

/// Parse "KEY:VALUE" header strings into (key, value) tuples.
/// Returns error for malformed headers (missing colon, empty key).
fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>, ActionResult> {
    raw.iter()
        .map(|h| {
            let (key, value) = h.split_once(':').ok_or_else(|| {
                ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    "invalid header format, expected KEY:VALUE".to_string(),
                )
            })?;
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.is_empty() {
                return Err(ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    "header key must not be empty".to_string(),
                ));
            }
            Ok((key, value))
        })
        .collect()
}

/// Query parameter names that are treated as secrets and fully redacted in
/// `redact_endpoint`. Match is case-insensitive. Provider WSS endpoints carry
/// the API key directly in the query string, so this list is what stops the
/// daemon from echoing credentials back to logs/responses.
const SECRET_QUERY_KEYS: &[&str] = &[
    "apikey",
    "api_key",
    "api-key",
    "token",
    "access_token",
    "accesstoken",
    "auth",
    "authorization",
    "key",
    "password",
    "secret",
    "x-api-key",
];

fn is_secret_query_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SECRET_QUERY_KEYS.contains(&lower.as_str())
}

fn redact_query_string(query: &str) -> String {
    query
        .split('&')
        .map(|pair| {
            if let Some((key, _value)) = pair.split_once('=') {
                if is_secret_query_key(key) {
                    format!("{key}=***")
                } else {
                    pair.to_string()
                }
            } else {
                pair.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Emit a CDP endpoint for the given mode.
///
/// Local endpoints (`ws://127.0.0.1:PORT/devtools/browser/<uuid>`) carry no
/// secrets: the UUID segment is just Chrome's internal target ID, and
/// connecting still requires local loopback access. Running these through
/// `redact_endpoint` turns them into `ws://127.0.0.1:PORT/devt***`, which is
/// not actionable — the user can't attach to the truncated URL. So local
/// sessions emit the verbatim ws_url and cloud sessions always go through
/// redaction (provider WSS URLs embed API keys as query params and tokens
/// as path segments).
pub fn endpoint_for_mode(mode: Mode, endpoint: &str) -> String {
    if mode == Mode::Cloud {
        redact_endpoint(endpoint)
    } else {
        endpoint.to_string()
    }
}

/// Redact a CDP endpoint for safe display.
///
/// - Query parameters whose keys appear in `SECRET_QUERY_KEYS` are masked to
///   `key=***` (this is where every cloud provider currently puts the API key).
/// - Long path segments are replaced with a short prefix + `***` so that
///   token-in-path schemes (e.g. `/connect/<token>`) are also redacted.
/// - Scheme, host:port and the rest of the URL structure are preserved so
///   the redacted form is still useful for debugging.
pub fn redact_endpoint(endpoint: &str) -> String {
    let Some(scheme_end) = endpoint.find("://") else {
        return endpoint.to_string();
    };
    let scheme = &endpoint[..scheme_end + 3];
    let after_scheme = &endpoint[scheme_end + 3..];

    // Split off the query string first so we can redact it independently.
    let (path_part, query_part) = match after_scheme.find('?') {
        Some(q_idx) => (&after_scheme[..q_idx], Some(&after_scheme[q_idx + 1..])),
        None => (after_scheme, None),
    };

    // Split host:port from path.
    let (host_port, path) = match path_part.find('/') {
        Some(slash_idx) => (&path_part[..slash_idx], &path_part[slash_idx..]),
        None => (path_part, ""),
    };

    let redacted_path = if path.len() > 10 {
        // Path looks like it carries an opaque token; keep the first few chars
        // for context and mask the rest.
        let prefix_end = 5.min(path.len());
        format!("{}***", &path[..prefix_end])
    } else {
        path.to_string()
    };

    let mut out = format!("{scheme}{host_port}{redacted_path}");
    if let Some(query) = query_part {
        out.push('?');
        out.push_str(&redact_query_string(query));
    }
    out
}

#[cfg(test)]
mod restricted_scheme_tests {
    use super::{is_chrome_attach_restriction_error, is_restricted_attach_scheme};

    #[test]
    fn restricted_schemes_match() {
        for url in [
            "chrome://newtab/",
            "chrome://extensions",
            "chrome-extension://foo/popup.html",
            "chrome-untrusted://terminal",
            "devtools://devtools/bundled/inspector.html",
            "edge://settings",
            "view-source:https://example.com",
        ] {
            assert!(
                is_restricted_attach_scheme(url),
                "should be restricted: {url}"
            );
        }
    }

    #[test]
    fn normal_urls_are_not_restricted() {
        for url in [
            "https://mail.google.com/",
            "http://localhost:3000",
            "about:blank",
            "file:///tmp/foo.html",
        ] {
            assert!(
                !is_restricted_attach_scheme(url),
                "should be allowed: {url}"
            );
        }
    }

    #[test]
    fn chrome_attach_restriction_error_matches_real_cdp_message() {
        // Shape from the user-reported transcript.
        assert!(is_chrome_attach_restriction_error(
            "cdp error: CDP error -32000: Cannot access a chrome:// URL"
        ));
        assert!(is_chrome_attach_restriction_error(
            "CDP error -32000: Cannot access a chrome-extension:// URL"
        ));
        assert!(is_chrome_attach_restriction_error(
            "Cannot access a devtools:// URL"
        ));
    }

    #[test]
    fn unrelated_errors_do_not_match() {
        assert!(!is_chrome_attach_restriction_error(
            "session closed: session was closed while command was pending"
        ));
        assert!(!is_chrome_attach_restriction_error(
            "CDP error -32602: invalid params"
        ));
        assert!(!is_chrome_attach_restriction_error(
            "cannot access chrome" // missing scheme marker
        ));
    }
}

#[cfg(test)]
mod redact_tests {
    use super::redact_endpoint;

    #[test]
    fn redacts_apikey_query_param() {
        let url = "wss://connect.browser-use.com?apiKey=super-secret-token&proxyCountryCode=us";
        let red = redact_endpoint(url);
        assert!(!red.contains("super-secret-token"), "leaked token: {red}");
        assert!(red.contains("apiKey=***"), "expected mask: {red}");
        assert!(
            red.contains("proxyCountryCode=us"),
            "kept non-secret: {red}"
        );
    }

    #[test]
    fn redacts_token_query_param_case_insensitive() {
        let url = "wss://cdp.driver.dev?Token=abc123def456&profile=foo";
        let red = redact_endpoint(url);
        assert!(!red.contains("abc123def456"), "leaked token: {red}");
        assert!(red.contains("Token=***"));
        assert!(red.contains("profile=foo"));
    }

    #[test]
    fn redacts_long_path_segment() {
        let url = "wss://cloud.example.com/connect/very-long-opaque-token-segment";
        let red = redact_endpoint(url);
        assert!(
            !red.contains("very-long-opaque-token-segment"),
            "leaked: {red}"
        );
        assert!(red.starts_with("wss://cloud.example.com/"));
        assert!(red.ends_with("***"));
    }

    #[test]
    fn keeps_short_path_unchanged() {
        let url = "wss://example.com/ws";
        assert_eq!(redact_endpoint(url), "wss://example.com/ws");
    }

    #[test]
    fn handles_endpoint_without_scheme() {
        let url = "example.com/ws?token=secret";
        // No scheme — pass through unchanged (best-effort).
        assert_eq!(redact_endpoint(url), "example.com/ws?token=secret");
    }

    use super::endpoint_for_mode;
    use crate::types::Mode;

    #[test]
    fn endpoint_for_mode_keeps_local_ws_verbatim() {
        // A real local Chrome CDP URL has a long devtools/browser path that
        // `redact_endpoint` would truncate to `/devt***`. The caller can't
        // attach to that, so local mode must emit the URL untouched.
        let url = "ws://127.0.0.1:9222/devtools/browser/abc-123-def-456-opaque-guid";
        assert_eq!(endpoint_for_mode(Mode::Local, url), url);
        assert_eq!(endpoint_for_mode(Mode::Extension, url), url);
    }

    #[test]
    fn endpoint_for_mode_redacts_cloud_ws_with_apikey() {
        // Cloud URLs must still be redacted so the daemon never echoes a
        // provider API key back to stdout.
        let url = "wss://connect.browser-use.com?apiKey=super-secret-token";
        let emitted = endpoint_for_mode(Mode::Cloud, url);
        assert!(
            !emitted.contains("super-secret-token"),
            "cloud endpoint must be redacted: {emitted}",
        );
        assert!(emitted.contains("apiKey=***"), "expected mask: {emitted}");
    }
}

#[cfg(test)]
mod provider_start_tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::browser::session::provider::{ProviderEnv, ProviderSession};
    use crate::daemon::registry::{SessionEntry, SessionState, new_shared_registry};
    use crate::types::SessionId;

    fn spawn_single_response_server(
        response: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("mock server addr");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        request.extend_from_slice(&buf[..n]);
                        if request.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err)
                        if matches!(
                            err.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        break;
                    }
                    Err(err) => panic!("read request: {err}"),
                }
            }

            stream
                .write_all(response.as_bytes())
                .expect("write response");
            String::from_utf8(request).expect("utf8 request")
        });
        (format!("http://{}", addr), handle)
    }

    #[tokio::test]
    async fn conflicting_set_session_id_cleans_up_provider_session() {
        let (base_url, request_handle) =
            spawn_single_response_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let registry = new_shared_registry();

        let mut existing = SessionEntry::starting(
            SessionId::new("hyp3").expect("session id"),
            Mode::Cloud,
            false,
            true,
            crate::config::DEFAULT_PROFILE.to_string(),
        );
        existing.status = SessionState::Running;
        registry.lock().await.insert(existing);

        let result = execute_cloud(
            &Cmd {
                mode: Some(Mode::Cloud),
                headless: Some(false),
                profile: None,
                executable_path: None,
                open_url: None,
                tab_id: None,
                cdp_endpoint: None,
                provider: Some("hyperbrowser".to_string()),
                header: vec![],
                session: None,
                set_session_id: Some("hyp3".to_string()),
                stealth: true,
                max_tracked_requests: 500,
                provider_env: ProviderEnv::new(),
            },
            &registry,
            "ws://example.test/devtools/browser/fake",
            &[],
            crate::config::DEFAULT_PROFILE,
            false,
            Some("hyperbrowser"),
            Some(ProviderSession {
                provider: "hyperbrowser".to_string(),
                session_id: "hb-conflict-1".to_string(),
                provider_env: ProviderEnv::from([
                    ("HYPERBROWSER_API_KEY".to_string(), "hb-key".to_string()),
                    ("HYPERBROWSER_API_URL".to_string(), base_url.clone()),
                ]),
            }),
            500,
        )
        .await;

        match result {
            ActionResult::Fatal { code, message, .. } => {
                assert_eq!(code, "SESSION_ALREADY_EXISTS");
                assert!(message.contains("session id 'hyp3' is already in use"));
            }
            other => panic!("expected fatal result, got {other:?}"),
        }

        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("PUT /api/session/hb-conflict-1/stop HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("content-length: 0"));
    }

    #[tokio::test]
    async fn explicit_provider_session_conflict_fails_before_connect() {
        let registry = new_shared_registry();

        let mut existing = SessionEntry::starting(
            SessionId::new("hyp3").expect("session id"),
            Mode::Cloud,
            false,
            true,
            crate::config::DEFAULT_PROFILE.to_string(),
        );
        existing.status = SessionState::Running;
        registry.lock().await.insert(existing);

        let result = execute(
            &Cmd {
                mode: Some(Mode::Cloud),
                headless: Some(false),
                profile: None,
                executable_path: None,
                open_url: None,
                tab_id: None,
                cdp_endpoint: None,
                provider: Some("hyperbrowser".to_string()),
                header: vec![],
                session: None,
                set_session_id: Some("hyp3".to_string()),
                stealth: true,
                max_tracked_requests: 500,
                provider_env: ProviderEnv::from([
                    ("HYPERBROWSER_API_KEY".to_string(), "hb-key".to_string()),
                    (
                        "HYPERBROWSER_API_URL".to_string(),
                        "http://127.0.0.1:9".to_string(),
                    ),
                ]),
            },
            &registry,
        )
        .await;

        match result {
            ActionResult::Fatal { code, message, .. } => {
                assert_eq!(code, "SESSION_ALREADY_EXISTS");
                assert!(message.contains("session id 'hyp3' is already in use"));
            }
            other => panic!("expected fatal result, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn explicit_set_session_id_skips_endpoint_reuse_checks() {
        let registry = new_shared_registry();
        let endpoint = "ws://127.0.0.1:9/devtools/browser/fake";

        let mut existing = SessionEntry::starting(
            SessionId::new("dr3").expect("session id"),
            Mode::Cloud,
            false,
            true,
            crate::config::DEFAULT_PROFILE.to_string(),
        );
        existing.status = SessionState::Starting;
        existing.cdp_endpoint = Some(endpoint.to_string());
        existing.provider = Some("browseruse".to_string());
        registry.lock().await.insert(existing);

        let result = execute_cloud(
            &Cmd {
                mode: Some(Mode::Cloud),
                headless: Some(false),
                profile: None,
                executable_path: None,
                open_url: None,
                tab_id: None,
                cdp_endpoint: None,
                provider: Some("browseruse".to_string()),
                header: vec![],
                session: None,
                set_session_id: Some("bs1".to_string()),
                stealth: true,
                max_tracked_requests: 500,
                provider_env: ProviderEnv::new(),
            },
            &registry,
            endpoint,
            &[],
            crate::config::DEFAULT_PROFILE,
            false,
            Some("browseruse"),
            None,
            500,
        )
        .await;

        match result {
            ActionResult::Fatal { code, .. } => {
                assert_eq!(code, "CDP_CONNECTION_FAILED");
            }
            other => panic!("expected fatal result, got {other:?}"),
        }

        let reg = registry.lock().await;
        assert!(
            reg.get("bs1").is_none(),
            "failed start should clean placeholder"
        );
        assert_eq!(
            reg.get("dr3").map(|entry| entry.status),
            Some(SessionState::Starting)
        );
    }
}

/// Set the profile display name in Chrome's Local State and Preferences files
/// so the "actionbook" profile shows its name in Chrome's profile picker.
/// Preserves any user-customized name.
fn ensure_profile_display_name(user_data_dir: &std::path::Path) -> Result<(), String> {
    let local_state_path = user_data_dir.join("Local State");
    let preferences_path = user_data_dir.join("Default").join("Preferences");

    let mut local_state = read_json_or_default(&local_state_path)?;
    let mut preferences = read_json_or_default(&preferences_path)?;

    // Don't overwrite a name the user set manually.
    if has_custom_profile_name(&local_state, &preferences) {
        return Ok(());
    }

    // Local State: profile.info_cache.Default.name
    let info_cache = local_state
        .as_object_mut()
        .ok_or("local_state not object")?
        .entry("profile")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("profile not object")?
        .entry("info_cache")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("info_cache not object")?
        .entry("Default")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Default not object")?;
    info_cache.insert("name".to_string(), json!(DEFAULT_PROFILE));
    info_cache.insert("is_using_default_name".to_string(), json!(false));

    // Preferences: profile.name
    let prefs_profile = preferences
        .as_object_mut()
        .ok_or("preferences not object")?
        .entry("profile")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("profile not object")?;
    prefs_profile.insert("name".to_string(), json!(DEFAULT_PROFILE));

    write_json(&local_state_path, &local_state)?;
    write_json(&preferences_path, &preferences)?;
    Ok(())
}

fn has_custom_profile_name(
    local_state: &serde_json::Value,
    _preferences: &serde_json::Value,
) -> bool {
    // Use Chrome's `is_using_default_name` flag instead of comparing against a
    // hardcoded English default like "Your Chrome".  This handles all locales
    // (e.g. "Ihr Chrome" in German) and Chromium-branded builds.
    let is_using_default = local_state
        .pointer("/profile/info_cache/Default/is_using_default_name")
        .and_then(|v| v.as_bool())
        .unwrap_or(true); // absent → fresh profile → treat as default

    if is_using_default {
        return false;
    }

    // `is_using_default_name` is false — either the user customized the name,
    // or we previously wrote "actionbook" (which also sets it to false).
    // Only treat it as custom if the name differs from ours.
    let name = local_state
        .pointer("/profile/info_cache/Default/name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    !name.is_empty() && name != DEFAULT_PROFILE
}

fn read_json_or_default(path: &std::path::Path) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn write_json(path: &std::path::Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(value).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(path, content).map_err(|e| format!("write {}: {e}", path.display()))
}
