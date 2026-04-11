use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::CONTENT_LENGTH;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::CliError;

const HYPERBROWSER_API_BASE: &str = "https://api.hyperbrowser.ai";
const BROWSER_USE_API_BASE: &str = "https://api.browser-use.com/api/v3";
// driver.dev is a stateful provider: POST /v1/browser/session mints a session
// and returns a per-session distributed cdpUrl (e.g. wss://do-ric1-1.lex-milan.driver.dev/...).
// We never connect directly to driver.dev/cdp; the URL is always the one the
// API returns. Override only if you're pointing at a private control plane.
const DRIVER_DEV_API_BASE: &str = "https://api.driver.dev";

/// Per-request snapshot of provider-related environment variables, forwarded
/// from the CLI client process to the daemon. The daemon must NOT call
/// `std::env::var` for provider config: its own environment is frozen at
/// daemon-spawn time and almost never matches the user's current shell.
pub type ProviderEnv = BTreeMap<String, String>;

/// Env-var name prefixes considered "provider config". Used by the CLI client
/// to filter `std::env::vars()` down to the values worth forwarding.
///
/// `DRIVER_` is intentionally broad: driver.dev's official docs use bare
/// `DRIVER_API_KEY` (the auth credential), so we forward anything starting
/// with `DRIVER_` to pick it up alongside the namespaced `DRIVER_DEV_*`
/// tuning knobs. The downside is that an unrelated tool using a `DRIVER_*`
/// env var will leak its value into the daemon — acceptable because (a) the
/// daemon is local to the user and (b) we never log these values, only
/// forward them in the IPC payload.
pub const PROVIDER_ENV_PREFIXES: &[&str] = &["DRIVER_", "HYPERBROWSER_", "BROWSER_USE_"];

/// Collect every env var on the current process whose name starts with one of
/// the provider prefixes. Called from the CLI client (NOT the daemon) right
/// before sending a Start/Restart action.
pub fn collect_provider_env_from_process() -> ProviderEnv {
    std::env::vars()
        .filter(|(name, _)| {
            PROVIDER_ENV_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .collect()
}

/// HTTP request timeout for cloud provider control-plane API calls.
/// Provider APIs occasionally hang; without an explicit timeout the daemon
/// thread is stuck indefinitely waiting on `connect_provider`.
const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

fn build_provider_http_client() -> Result<reqwest::Client, CliError> {
    reqwest::Client::builder()
        .timeout(PROVIDER_HTTP_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(CliError::from)
}

/// Map an HTTP status code to a typed CliError so that callers (and the LLM
/// consumer) can distinguish auth, rate-limit and server errors from generic
/// API errors. The body is included in the message verbatim — provider APIs
/// already redact secrets in their error responses.
fn map_provider_http_status(provider: &str, status: StatusCode, body: &str) -> CliError {
    let snippet = body.chars().take(512).collect::<String>();
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => CliError::ApiUnauthorized(format!(
            "{provider} API rejected credentials ({}): {snippet}",
            status.as_u16()
        )),
        StatusCode::TOO_MANY_REQUESTS => CliError::ApiRateLimited(format!(
            "{provider} API rate-limited ({}): {snippet}",
            status.as_u16()
        )),
        s if s.is_server_error() => CliError::ApiServerError(format!(
            "{provider} API server error ({}): {snippet}",
            status.as_u16()
        )),
        s => CliError::ApiError(format!("{provider} API error ({}): {snippet}", s.as_u16())),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSession {
    pub provider: String,
    pub session_id: String,
    /// Snapshot of the provider env vars used to start this session. Carried
    /// forward so close/restart can talk to the provider control plane even
    /// when the user's current shell no longer has the keys exported.
    #[serde(default)]
    pub provider_env: ProviderEnv,
}

#[derive(Debug, Clone)]
pub struct ProviderConnection {
    pub provider: String,
    pub cdp_endpoint: String,
    pub headers: Vec<(String, String)>,
    pub session: Option<ProviderSession>,
}

pub fn normalize_provider_name(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "driver" => Some("driver"),
        "hyperbrowser" => Some("hyperbrowser"),
        "browseruse" | "browser-use" => Some("browseruse"),
        _ => None,
    }
}

pub fn supported_providers() -> &'static str {
    "driver, hyperbrowser, browseruse"
}

pub async fn connect_provider(
    provider_name: &str,
    profile_name: &str,
    _headless: bool,
    _stealth: bool,
    env: &ProviderEnv,
) -> Result<ProviderConnection, CliError> {
    let provider = normalize_provider_name(provider_name).ok_or_else(|| {
        CliError::InvalidArgument(format!(
            "unknown provider '{provider_name}'. Supported providers: {}",
            supported_providers()
        ))
    })?;

    // Each helper stamps `env` directly onto its `ProviderSession` so the
    // descriptor is always fully populated when it leaves the helper. No
    // post-hoc fix-up is needed here.
    let connection = match provider {
        "driver" => connect_driver_dev(profile_name, env).await?,
        "hyperbrowser" => connect_hyperbrowser(profile_name, env).await?,
        "browseruse" => connect_browser_use(profile_name, env).await?,
        _ => {
            return Err(CliError::InvalidArgument(format!(
                "unknown provider '{provider_name}'. Supported providers: {}",
                supported_providers()
            )));
        }
    };

    Ok(connection)
}

pub async fn close_provider_session(session: &ProviderSession) -> Result<(), CliError> {
    // Use a short, bounded timeout for cleanup so a hung provider API can't
    // block daemon shutdown or session restarts.
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(err) => return Err(CliError::from(err)),
    };
    let env = &session.provider_env;
    match session.provider.as_str() {
        "hyperbrowser" => {
            let api_key = read_required_env(env, "HYPERBROWSER_API_KEY")?;
            let api_base = read_trimmed_env(env, "HYPERBROWSER_API_URL")
                .unwrap_or_else(|| HYPERBROWSER_API_BASE.to_string());
            let response = client
                .put(format!(
                    "{}/api/session/{}/stop",
                    api_base.trim_end_matches('/'),
                    session.session_id
                ))
                .header("x-api-key", api_key)
                // Hyperbrowser's edge rejects empty stop requests unless the
                // client sends an explicit zero-length Content-Length header.
                .header(CONTENT_LENGTH, "0")
                .send()
                .await?;
            let status = response.status();
            let response_text = response.text().await?;
            if !status.is_success() {
                return Err(map_provider_http_status(
                    "Hyperbrowser",
                    status,
                    &response_text,
                ));
            }
        }
        "driver" => {
            // driver.dev sessions auto-stop after 1h, but we explicitly DELETE
            // them to release billing immediately. The session_id is the same
            // opaque string returned by POST /v1/browser/session, passed back
            // as a query parameter.
            match read_driver_dev_api_key(env) {
                Ok(api_key) => {
                    let api_base = read_trimmed_env(env, "DRIVER_DEV_API_URL")
                        .unwrap_or_else(|| DRIVER_DEV_API_BASE.to_string());
                    let response = client
                        .delete(format!(
                            "{}/v1/browser/session",
                            api_base.trim_end_matches('/')
                        ))
                        .query(&[("sessionId", session.session_id.as_str())])
                        .header("Authorization", format!("Bearer {api_key}"))
                        .send()
                        .await?;
                    let status = response.status();
                    let response_text = response.text().await?;
                    if !status.is_success() {
                        if is_driver_dev_auth_failure(&response_text) {
                            return Err(CliError::ApiUnauthorized(format!(
                                "driver rejected credentials ({}): {}",
                                status.as_u16(),
                                response_text.chars().take(512).collect::<String>()
                            )));
                        }
                        return Err(map_provider_http_status("driver", status, &response_text));
                    }
                }
                Err(err) => return Err(err),
            }
        }
        "browseruse" => {
            let api_key = read_required_env(env, "BROWSER_USE_API_KEY")?;
            let api_base = read_trimmed_env(env, "BROWSER_USE_API_URL")
                .unwrap_or_else(|| BROWSER_USE_API_BASE.to_string());
            let response = client
                .patch(format!(
                    "{}/browsers/{}",
                    api_base.trim_end_matches('/'),
                    session.session_id
                ))
                .header("X-Browser-Use-API-Key", api_key)
                .header("Content-Type", "application/json")
                .json(&json!({ "action": "stop" }))
                .send()
                .await?;
            let status = response.status();
            let response_text = response.text().await?;
            if !status.is_success() {
                return Err(map_provider_http_status(
                    "Browser Use",
                    status,
                    &response_text,
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// driver.dev returns HTTP 500 (instead of 401) with bodies like
/// `{"error":"Invalid consumer token"}` for bad credentials. Sniff the body
/// so the generic 5xx → "server error, retry" mapping doesn't kick in.
///
/// Conservative match: only the substrings driver.dev actually uses today,
/// so a real upstream 5xx with the word "token" in a stack trace doesn't
/// get reclassified.
///
/// Both branches log at info level so we can tell from daemon logs which
/// classification fired when a user reports "I set the key but keep getting
/// server error" (or vice versa). We only log a short prefix of the body to
/// avoid leaking anything sensitive that upstream might echo back.
fn is_driver_dev_auth_failure(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    let hit = lower.contains("invalid consumer token")
        || lower.contains("invalid token")
        || lower.contains("invalid api key")
        || lower.contains("unauthorized");
    let snippet: String = body.chars().take(120).collect();
    if hit {
        tracing::info!(
            "driver.dev response classified as auth failure (→ ApiUnauthorized): {snippet}"
        );
    } else {
        tracing::info!(
            "driver.dev response classified as server error (→ ApiServerError): {snippet}"
        );
    }
    hit
}

/// Read the driver.dev API key from `DRIVER_API_KEY`, matching driver.dev's
/// official docs. Only this name is accepted — see PR #507 discussion for the
/// decision to drop the `DRIVER_DEV_API_KEY` alias.
fn read_driver_dev_api_key(env: &ProviderEnv) -> Result<String, CliError> {
    read_trimmed_env(env, "DRIVER_API_KEY").ok_or_else(|| {
        CliError::InvalidArgument("DRIVER_API_KEY environment variable is not set".to_string())
    })
}

async fn connect_driver_dev(
    profile_name: &str,
    env: &ProviderEnv,
) -> Result<ProviderConnection, CliError> {
    // Escape hatch: allow a fully-qualified WSS URL to bypass the control plane.
    // Useful for replaying captured CDP URLs in tests, or for pointing at a
    // private deployment that exposes a CDP socket directly.
    if let Some(ws_url) = read_trimmed_env(env, "DRIVER_DEV_WS_URL")
        .or_else(|| read_trimmed_env(env, "DRIVER_DEV_CDP_ENDPOINT"))
    {
        return Ok(ProviderConnection {
            provider: "driver".to_string(),
            cdp_endpoint: ws_url,
            headers: Vec::new(),
            session: None,
        });
    }

    let api_key = read_driver_dev_api_key(env)?;
    let api_base = read_trimmed_env(env, "DRIVER_DEV_API_URL")
        .unwrap_or_else(|| DRIVER_DEV_API_BASE.to_string());

    // Build optional session-creation body. Empty `{}` is valid; only set fields
    // when the user actually configured them.
    let mut body = json!({});
    if let Some(country) = read_trimmed_env(env, "DRIVER_DEV_COUNTRY") {
        body["country"] = json!(country);
    }
    if let Some(node_id) = read_trimmed_env(env, "DRIVER_DEV_NODE_ID") {
        body["nodeId"] = json!(node_id);
    }
    if let Some(session_type) = read_trimmed_env(env, "DRIVER_DEV_TYPE") {
        // Driver supports `consumer_distributed` (default) and `hosted`. Pass
        // through verbatim — the API will reject anything else.
        body["type"] = json!(session_type);
    }
    if let Some(proxy_url) = read_trimmed_env(env, "DRIVER_DEV_PROXY_URL") {
        body["proxyUrl"] = json!(proxy_url);
    }
    if let Some(window_size) = read_trimmed_env(env, "DRIVER_DEV_WINDOW_SIZE") {
        body["windowSize"] = json!(window_size);
    }
    if let Some(profile) =
        read_trimmed_env(env, "DRIVER_DEV_PROFILE").or_else(|| non_default_profile(profile_name))
    {
        let persist = parse_env_bool(env, "DRIVER_DEV_PROFILE_PERSIST").unwrap_or(true);
        body["profile"] = json!({
            "name": profile,
            "persist": persist,
        });
    }

    let client = build_provider_http_client()?;
    let response = client
        .post(format!(
            "{}/v1/browser/session",
            api_base.trim_end_matches('/')
        ))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;
    if !status.is_success() {
        // driver.dev returns HTTP 500 with `{"error":"Invalid consumer token"}`
        // for bad credentials instead of 401. Reclassify so callers (and the
        // LLM agent) know not to retry — a "server error" wrongly suggests
        // transient failure.
        if is_driver_dev_auth_failure(&response_text) {
            return Err(CliError::ApiUnauthorized(format!(
                "driver rejected credentials ({}): {}",
                status.as_u16(),
                response_text.chars().take(512).collect::<String>()
            )));
        }
        return Err(map_provider_http_status("driver", status, &response_text));
    }

    let data: Value = serde_json::from_str(&response_text)?;
    let session_id = data
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::ApiError(format!("driver API response missing sessionId: {data}"))
        })?
        .to_string();
    let cdp_endpoint = data
        .get("cdpUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::ApiError(format!("driver API response missing cdpUrl: {data}")))?
        .to_string();

    Ok(ProviderConnection {
        provider: "driver".to_string(),
        cdp_endpoint,
        headers: Vec::new(),
        session: Some(ProviderSession {
            provider: "driver".to_string(),
            session_id,
            // Snapshot the env so close/restart can talk to api.driver.dev
            // later even when the calling shell no longer has
            // DRIVER_API_KEY exported.
            provider_env: env.clone(),
        }),
    })
}

async fn connect_hyperbrowser(
    profile_name: &str,
    env: &ProviderEnv,
) -> Result<ProviderConnection, CliError> {
    let api_key = read_required_env(env, "HYPERBROWSER_API_KEY")?;
    let api_base = read_trimmed_env(env, "HYPERBROWSER_API_URL")
        .unwrap_or_else(|| HYPERBROWSER_API_BASE.to_string());
    let use_proxy = parse_env_bool(env, "HYPERBROWSER_USE_PROXY").unwrap_or(false);
    let persist_changes = parse_env_bool(env, "HYPERBROWSER_PERSIST_CHANGES").unwrap_or(true);
    let profile_id = read_trimmed_env(env, "HYPERBROWSER_PROFILE_ID")
        .or_else(|| non_default_profile(profile_name));

    let mut body = json!({ "useProxy": use_proxy });
    if let Some(profile_id) = profile_id {
        body["profile"] = json!({
            "id": normalize_hyperbrowser_profile_id(&profile_id)?,
            "persistChanges": persist_changes,
        });
    }

    let client = build_provider_http_client()?;
    let response = client
        .post(format!("{}/api/session", api_base.trim_end_matches('/')))
        .header("x-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;
    if !status.is_success() {
        return Err(map_provider_http_status(
            "Hyperbrowser",
            status,
            &response_text,
        ));
    }

    let data: Value = serde_json::from_str(&response_text)?;
    let session_id = data
        .get("id")
        .or_else(|| data.get("sessionId"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::ApiError(format!(
                "Hyperbrowser API returned incomplete session data: {data}"
            ))
        })?
        .to_string();
    let cdp_endpoint = data
        .get("wsEndpoint")
        .or_else(|| data.get("sessionWebsocketUrl"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::ApiError(format!(
                "Hyperbrowser API returned incomplete session data: {data}"
            ))
        })?
        .to_string();

    Ok(ProviderConnection {
        provider: "hyperbrowser".to_string(),
        cdp_endpoint,
        headers: Vec::new(),
        session: Some(ProviderSession {
            provider: "hyperbrowser".to_string(),
            session_id,
            provider_env: env.clone(),
        }),
    })
}

async fn connect_browser_use(
    profile_name: &str,
    env: &ProviderEnv,
) -> Result<ProviderConnection, CliError> {
    if let Some(ws_url) = read_trimmed_env(env, "BROWSER_USE_WS_URL") {
        return Ok(ProviderConnection {
            provider: "browseruse".to_string(),
            cdp_endpoint: ws_url,
            headers: Vec::new(),
            session: None,
        });
    }

    let api_key = read_required_env(env, "BROWSER_USE_API_KEY")?;
    let api_base = read_trimmed_env(env, "BROWSER_USE_API_URL")
        .unwrap_or_else(|| BROWSER_USE_API_BASE.to_string());

    let mut body = json!({});
    if let Some(value) = read_trimmed_env(env, "BROWSER_USE_PROXY_COUNTRY_CODE") {
        body["proxyCountryCode"] = json!(value);
    }
    if let Some(value) = read_trimmed_env(env, "BROWSER_USE_PROFILE_ID")
        .or_else(|| non_default_profile(profile_name))
    {
        body["profileId"] = json!(value);
    }
    if let Some(value) = read_trimmed_env(env, "BROWSER_USE_TIMEOUT") {
        body["timeout"] = json!(parse_env_u64("BROWSER_USE_TIMEOUT", &value)?);
    }
    if let Some(value) = read_trimmed_env(env, "BROWSER_USE_BROWSER_SCREEN_WIDTH") {
        body["browserScreenWidth"] =
            json!(parse_env_u64("BROWSER_USE_BROWSER_SCREEN_WIDTH", &value)?);
    }
    if let Some(value) = read_trimmed_env(env, "BROWSER_USE_BROWSER_SCREEN_HEIGHT") {
        body["browserScreenHeight"] =
            json!(parse_env_u64("BROWSER_USE_BROWSER_SCREEN_HEIGHT", &value)?);
    }

    let client = build_provider_http_client()?;
    let response = client
        .post(format!("{}/browsers", api_base.trim_end_matches('/')))
        .header("X-Browser-Use-API-Key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;
    if !status.is_success() {
        return Err(map_provider_http_status(
            "Browser Use",
            status,
            &response_text,
        ));
    }

    let data: Value = serde_json::from_str(&response_text)?;
    let session_id = data
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::ApiError(format!(
                "Browser Use API returned incomplete session data: {data}"
            ))
        })?
        .to_string();
    let cdp_endpoint = data
        .get("cdpUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::ApiError(format!(
                "Browser Use API returned incomplete session data: {data}"
            ))
        })?
        .to_string();

    Ok(ProviderConnection {
        provider: "browseruse".to_string(),
        cdp_endpoint,
        headers: Vec::new(),
        session: Some(ProviderSession {
            provider: "browseruse".to_string(),
            session_id,
            provider_env: env.clone(),
        }),
    })
}

fn read_trimmed_env(env: &ProviderEnv, name: &str) -> Option<String> {
    env.get(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_required_env(env: &ProviderEnv, name: &str) -> Result<String, CliError> {
    read_trimmed_env(env, name)
        .ok_or_else(|| CliError::InvalidArgument(format!("{name} environment variable is not set")))
}

fn parse_env_bool(env: &ProviderEnv, name: &str) -> Option<bool> {
    read_trimmed_env(env, name).and_then(|value| match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    })
}

fn parse_env_u64(name: &str, value: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::InvalidArgument(format!("invalid {name}: {value}")))
}

fn non_default_profile(profile_name: &str) -> Option<String> {
    let trimmed = profile_name.trim();
    if trimmed.is_empty() || trimmed == crate::config::DEFAULT_PROFILE {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_hyperbrowser_profile_id(profile_id: &str) -> Result<String, CliError> {
    let raw = profile_id.trim();
    if raw.is_empty() {
        return Err(CliError::InvalidArgument(
            "hyperbrowser profile id must not be empty".to_string(),
        ));
    }

    match Uuid::parse_str(raw) {
        Ok(uuid) => Ok(uuid.to_string()),
        Err(_) => Ok(
            Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("actionbook:{raw}").as_bytes()).to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use super::*;

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

    #[test]
    fn normalizes_provider_aliases() {
        assert_eq!(normalize_provider_name("driver"), Some("driver"));
        assert_eq!(normalize_provider_name("browser-use"), Some("browseruse"));
        assert_eq!(normalize_provider_name("browseruse"), Some("browseruse"));
        assert_eq!(
            normalize_provider_name("hyperbrowser"),
            Some("hyperbrowser")
        );
        assert_eq!(normalize_provider_name("driver.dev"), None);
        assert_eq!(normalize_provider_name("unknown"), None);
    }

    #[tokio::test]
    async fn driver_dev_provider_name_is_rejected() {
        let err = connect_provider(
            "driver.dev",
            crate::config::DEFAULT_PROFILE,
            false,
            true,
            &ProviderEnv::new(),
        )
        .await
        .expect_err("driver.dev alias should be rejected");

        assert!(matches!(err, CliError::InvalidArgument(_)));
        assert!(err.to_string().contains("unknown provider 'driver.dev'"));
    }

    #[test]
    fn hyperbrowser_profile_ids_are_normalized_to_uuid() {
        let normalized = normalize_hyperbrowser_profile_id("user-42").expect("normalized uuid");
        assert!(Uuid::parse_str(&normalized).is_ok());
        assert_eq!(
            normalized,
            Uuid::new_v5(&Uuid::NAMESPACE_URL, b"actionbook:user-42").to_string()
        );
    }

    #[test]
    fn keeps_explicit_uuid_profile_ids() {
        let raw = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(
            normalize_hyperbrowser_profile_id(raw).expect("uuid"),
            raw.to_string()
        );
    }

    fn env_with(entries: &[(&str, &str)]) -> ProviderEnv {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn read_trimmed_env_returns_value_from_map() {
        let env = env_with(&[("DRIVER_API_KEY", "  key-1  ")]);
        assert_eq!(
            read_trimmed_env(&env, "DRIVER_API_KEY"),
            Some("key-1".to_string())
        );
    }

    #[test]
    fn read_trimmed_env_treats_blank_as_missing() {
        let env = env_with(&[("DRIVER_API_KEY", "   ")]);
        assert_eq!(read_trimmed_env(&env, "DRIVER_API_KEY"), None);
    }

    #[test]
    fn read_required_env_errors_when_missing() {
        let env = ProviderEnv::new();
        let err = read_required_env(&env, "DRIVER_API_KEY").unwrap_err();
        assert!(matches!(err, CliError::InvalidArgument(_)));
    }

    #[test]
    fn parse_env_bool_understands_truthy_values() {
        let env = env_with(&[
            ("HYPERBROWSER_USE_PROXY", "true"),
            ("HYPERBROWSER_PERSIST_CHANGES", "0"),
        ]);
        assert_eq!(parse_env_bool(&env, "HYPERBROWSER_USE_PROXY"), Some(true));
        assert_eq!(
            parse_env_bool(&env, "HYPERBROWSER_PERSIST_CHANGES"),
            Some(false)
        );
        assert_eq!(parse_env_bool(&env, "MISSING"), None);
    }

    #[tokio::test]
    async fn driver_dev_ws_url_override_short_circuits_api_call() {
        // The DRIVER_DEV_WS_URL escape hatch must bypass api.driver.dev so
        // that offline tests / private deployments work without hitting the
        // network. This also pins the env-map vs process-env contract: even
        // if the daemon process has DRIVER_DEV_WS_URL exported, the per-request
        // env map is what's used.
        let env = env_with(&[(
            "DRIVER_DEV_WS_URL",
            "wss://example.test/devtools/browser/abc",
        )]);
        let connection = connect_driver_dev(crate::config::DEFAULT_PROFILE, &env)
            .await
            .expect("driver.dev connection should build from override");
        assert_eq!(connection.provider, "driver");
        assert_eq!(
            connection.cdp_endpoint,
            "wss://example.test/devtools/browser/abc"
        );
        // Override path is "stateless"-like — no provider session to clean up.
        assert!(connection.session.is_none());
    }

    #[test]
    fn browser_use_ws_url_override_is_stateless() {
        let env = env_with(&[(
            "BROWSER_USE_WS_URL",
            "wss://connect.browser-use.com?apiKey=bu-key",
        )]);
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let connection = rt
            .block_on(connect_browser_use(crate::config::DEFAULT_PROFILE, &env))
            .expect("browseruse connection");
        assert_eq!(connection.provider, "browseruse");
        assert!(connection.session.is_none());
    }

    #[tokio::test]
    async fn connect_browser_use_creates_provider_session_via_api() {
        let (base_url, request_handle) = spawn_single_response_server(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 81\r\n\r\n{\"id\":\"bu-s-1\",\"cdpUrl\":\"wss://cdp.browser-use.test/session-1\",\"status\":\"active\"}",
        );
        let env = env_with(&[
            ("BROWSER_USE_API_URL", &base_url),
            ("BROWSER_USE_API_KEY", "bu-key"),
            ("BROWSER_USE_TIMEOUT", "30"),
            ("BROWSER_USE_BROWSER_SCREEN_WIDTH", "1440"),
            ("BROWSER_USE_BROWSER_SCREEN_HEIGHT", "900"),
        ]);

        let connection = connect_browser_use(crate::config::DEFAULT_PROFILE, &env)
            .await
            .expect("browseruse connection");
        assert_eq!(connection.provider, "browseruse");
        assert_eq!(
            connection.cdp_endpoint,
            "wss://cdp.browser-use.test/session-1"
        );
        assert_eq!(
            connection
                .session
                .as_ref()
                .map(|session| session.session_id.as_str()),
            Some("bu-s-1")
        );

        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("POST /browsers HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-browser-use-api-key: bu-key")
        );
        assert!(request.contains("\"timeout\":30"));
        assert!(request.contains("\"browserScreenWidth\":1440"));
        assert!(request.contains("\"browserScreenHeight\":900"));
    }

    #[tokio::test]
    async fn close_driver_session_calls_delete_endpoint() {
        let (base_url, request_handle) =
            spawn_single_response_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let session = ProviderSession {
            provider: "driver".to_string(),
            session_id: "driver-s-1".to_string(),
            provider_env: env_with(&[
                ("DRIVER_DEV_API_URL", &base_url),
                ("DRIVER_API_KEY", "driver-key"),
            ]),
        };

        close_provider_session(&session)
            .await
            .expect("driver close should succeed");

        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("DELETE /v1/browser/session?sessionId=driver-s-1 HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer driver-key")
        );
    }

    #[tokio::test]
    async fn close_browser_use_session_calls_patch_endpoint() {
        let (base_url, request_handle) =
            spawn_single_response_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let session = ProviderSession {
            provider: "browseruse".to_string(),
            session_id: "bu-s-1".to_string(),
            provider_env: env_with(&[
                ("BROWSER_USE_API_URL", &base_url),
                ("BROWSER_USE_API_KEY", "bu-key"),
            ]),
        };

        close_provider_session(&session)
            .await
            .expect("browseruse close should succeed");

        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("PATCH /browsers/bu-s-1 HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-browser-use-api-key: bu-key")
        );
        assert!(request.contains("\"action\":\"stop\""));
    }

    #[tokio::test]
    async fn close_hyperbrowser_session_sends_zero_length_body() {
        let (base_url, request_handle) =
            spawn_single_response_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let session = ProviderSession {
            provider: "hyperbrowser".to_string(),
            session_id: "hb-s-1".to_string(),
            provider_env: env_with(&[
                ("HYPERBROWSER_API_URL", &base_url),
                ("HYPERBROWSER_API_KEY", "hb-key"),
            ]),
        };

        close_provider_session(&session)
            .await
            .expect("hyperbrowser close should succeed");

        let request = request_handle.join().expect("request join");
        assert!(request.starts_with("PUT /api/session/hb-s-1/stop HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("content-length: 0"));
    }

    #[test]
    fn driver_dev_auth_failure_sniffer_catches_known_strings() {
        assert!(is_driver_dev_auth_failure(
            r#"{"error":"Invalid consumer token"}"#
        ));
        assert!(is_driver_dev_auth_failure(r#"{"error":"invalid token"}"#));
        assert!(is_driver_dev_auth_failure(r#"{"error":"unauthorized"}"#));
        // Not auth: a generic upstream 500 should still go through the
        // ApiServerError path so the agent knows it's safe to retry.
        assert!(!is_driver_dev_auth_failure(
            r#"{"error":"upstream timeout"}"#
        ));
        assert!(!is_driver_dev_auth_failure(
            r#"{"error":"node selection failed"}"#
        ));
    }

    #[test]
    fn driver_dev_api_key_reads_from_driver_api_key() {
        // Only the official-docs name is accepted — see PR #507 for the
        // decision to drop the `DRIVER_DEV_API_KEY` alias.
        let env = env_with(&[("DRIVER_API_KEY", "official-name-key")]);
        assert_eq!(
            read_driver_dev_api_key(&env).expect("reads the key"),
            "official-name-key"
        );

        // The retired namespaced name must NOT be honored: that would
        // resurrect the silent fallback we just removed.
        let env = env_with(&[("DRIVER_DEV_API_KEY", "stale")]);
        assert!(read_driver_dev_api_key(&env).is_err());

        let env = ProviderEnv::new();
        assert!(read_driver_dev_api_key(&env).is_err());
    }

    #[test]
    fn collect_provider_env_from_process_filters_by_prefix() {
        // Smoke test: should never panic, should not include unrelated vars.
        let env = collect_provider_env_from_process();
        for name in env.keys() {
            assert!(
                PROVIDER_ENV_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix)),
                "unexpected env var leaked into provider env: {name}"
            );
        }
    }
}
