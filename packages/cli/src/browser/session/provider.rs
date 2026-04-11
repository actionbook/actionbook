use std::env;
use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::CliError;

const HYPERBROWSER_API_BASE: &str = "https://api.hyperbrowser.ai";
const BROWSERLESS_API_BASE: &str = "https://production-sfo.browserless.io";
const BROWSER_USE_WS_BASE: &str = "wss://connect.browser-use.com";
const DRIVER_DEV_WS_BASE: &str = "wss://cdp.driver.dev";

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
        s => CliError::ApiError(format!(
            "{provider} API error ({}): {snippet}",
            s.as_u16()
        )),
    }
}

#[derive(Debug, Clone)]
pub struct ProviderSession {
    pub provider: String,
    pub session_id: String,
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
        "driver" | "driver.dev" => Some("driver.dev"),
        "hyperbrowser" => Some("hyperbrowser"),
        "browseruse" | "browser-use" => Some("browseruse"),
        "browserless" => Some("browserless"),
        _ => None,
    }
}

pub fn supported_providers() -> &'static str {
    "driver.dev, hyperbrowser, browseruse, browserless"
}

pub async fn connect_provider(
    provider_name: &str,
    profile_name: &str,
    _headless: bool,
    stealth: bool,
) -> Result<ProviderConnection, CliError> {
    let provider = normalize_provider_name(provider_name).ok_or_else(|| {
        CliError::InvalidArgument(format!(
            "unknown provider '{provider_name}'. Supported providers: {}",
            supported_providers()
        ))
    })?;

    match provider {
        "driver.dev" => connect_driver_dev(profile_name).await,
        "hyperbrowser" => connect_hyperbrowser(profile_name).await,
        "browseruse" => connect_browser_use(profile_name).await,
        "browserless" => connect_browserless(stealth).await,
        _ => Err(CliError::InvalidArgument(format!(
            "unknown provider '{provider_name}'. Supported providers: {}",
            supported_providers()
        ))),
    }
}

pub async fn close_provider_session(session: &ProviderSession) {
    // Use a short, bounded timeout for cleanup so a hung provider API can't
    // block daemon shutdown or session restarts.
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(
                "failed to build cleanup client for provider '{}': {err}",
                session.provider
            );
            return;
        }
    };
    match session.provider.as_str() {
        "hyperbrowser" => {
            if let Some(api_key) = read_trimmed_env("HYPERBROWSER_API_KEY") {
                let api_base = read_trimmed_env("HYPERBROWSER_API_URL")
                    .unwrap_or_else(|| HYPERBROWSER_API_BASE.to_string());
                let _ = client
                    .put(format!(
                        "{}/api/session/{}/stop",
                        api_base.trim_end_matches('/'),
                        session.session_id
                    ))
                    .header("x-api-key", api_key)
                    .send()
                    .await;
            }
        }
        "browserless" => {
            // Browserless returns a stop URL; store it directly as the cleanup handle.
            let _ = client.delete(&session.session_id).send().await;
        }
        _ => {}
    }
}

async fn connect_driver_dev(profile_name: &str) -> Result<ProviderConnection, CliError> {
    let cdp_endpoint = if let Some(ws_url) = read_trimmed_env("DRIVER_DEV_WS_URL")
        .or_else(|| read_trimmed_env("DRIVER_DEV_CDP_ENDPOINT"))
    {
        ws_url
    } else {
        let api_key = read_required_env("DRIVER_DEV_API_KEY")?;
        let base = read_trimmed_env("DRIVER_DEV_WS_BASE_URL")
            .unwrap_or_else(|| DRIVER_DEV_WS_BASE.to_string());
        let mut query = vec![("token", api_key)];
        if let Some(profile) =
            read_trimmed_env("DRIVER_DEV_PROFILE").or_else(|| non_default_profile(profile_name))
        {
            query.push(("profile", profile));
        }
        build_ws_url(&base, &query)
    };

    Ok(ProviderConnection {
        provider: "driver.dev".to_string(),
        cdp_endpoint,
        headers: Vec::new(),
        session: None,
    })
}

async fn connect_hyperbrowser(profile_name: &str) -> Result<ProviderConnection, CliError> {
    let api_key = read_required_env("HYPERBROWSER_API_KEY")?;
    let api_base = read_trimmed_env("HYPERBROWSER_API_URL")
        .unwrap_or_else(|| HYPERBROWSER_API_BASE.to_string());
    let use_proxy = parse_env_bool("HYPERBROWSER_USE_PROXY").unwrap_or(false);
    let persist_changes = parse_env_bool("HYPERBROWSER_PERSIST_CHANGES").unwrap_or(true);
    let profile_id =
        read_trimmed_env("HYPERBROWSER_PROFILE_ID").or_else(|| non_default_profile(profile_name));

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
        }),
    })
}

async fn connect_browser_use(profile_name: &str) -> Result<ProviderConnection, CliError> {
    let api_key = read_required_env("BROWSER_USE_API_KEY")?;
    let base =
        read_trimmed_env("BROWSER_USE_WS_URL").unwrap_or_else(|| BROWSER_USE_WS_BASE.to_string());

    let mut query = vec![("apiKey", api_key)];
    if let Some(value) = read_trimmed_env("BROWSER_USE_PROXY_COUNTRY_CODE") {
        query.push(("proxyCountryCode", value));
    }
    if let Some(value) =
        read_trimmed_env("BROWSER_USE_PROFILE_ID").or_else(|| non_default_profile(profile_name))
    {
        query.push(("profileId", value));
    }
    if let Some(value) = read_trimmed_env("BROWSER_USE_TIMEOUT") {
        query.push(("timeout", value));
    }
    if let Some(value) = read_trimmed_env("BROWSER_USE_BROWSER_SCREEN_WIDTH") {
        query.push(("browserScreenWidth", value));
    }
    if let Some(value) = read_trimmed_env("BROWSER_USE_BROWSER_SCREEN_HEIGHT") {
        query.push(("browserScreenHeight", value));
    }

    Ok(ProviderConnection {
        provider: "browseruse".to_string(),
        cdp_endpoint: build_ws_url(&base, &query),
        headers: Vec::new(),
        session: None,
    })
}

async fn connect_browserless(stealth: bool) -> Result<ProviderConnection, CliError> {
    let api_key = read_required_env("BROWSERLESS_API_KEY")?;
    let api_base =
        read_trimmed_env("BROWSERLESS_API_URL").unwrap_or_else(|| BROWSERLESS_API_BASE.to_string());
    let browser_type =
        read_trimmed_env("BROWSERLESS_BROWSER_TYPE").unwrap_or_else(|| "chromium".to_string());
    let ttl = read_trimmed_env("BROWSERLESS_TTL").unwrap_or_else(|| "300000".to_string());
    let use_stealth = parse_env_bool("BROWSERLESS_STEALTH").unwrap_or(stealth);

    if !matches!(browser_type.as_str(), "chromium" | "chrome") {
        return Err(CliError::InvalidArgument(format!(
            "BROWSERLESS_BROWSER_TYPE '{browser_type}' is not supported; use chromium or chrome"
        )));
    }

    let client = build_provider_http_client()?;
    let response = client
        .post(format!("{}/session", api_base.trim_end_matches('/')))
        .query(&[("token", api_key.as_str())])
        .header("Content-Type", "application/json")
        .json(&json!({
            "ttl": ttl.parse::<u64>().map_err(|_| {
                CliError::InvalidArgument(format!("invalid BROWSERLESS_TTL: {ttl}"))
            })?,
            "stealth": use_stealth,
            "browser": browser_type,
        }))
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;
    if !status.is_success() {
        return Err(map_provider_http_status(
            "Browserless",
            status,
            &response_text,
        ));
    }

    let data: Value = serde_json::from_str(&response_text)?;
    let cdp_endpoint = data
        .get("connect")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::ApiError("Browserless response missing 'connect' URL".to_string())
        })?
        .to_string();
    let stop_url = data
        .get("stop")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::ApiError("Browserless response missing 'stop' URL".to_string()))?
        .to_string();

    Ok(ProviderConnection {
        provider: "browserless".to_string(),
        cdp_endpoint,
        headers: Vec::new(),
        session: Some(ProviderSession {
            provider: "browserless".to_string(),
            session_id: stop_url,
        }),
    })
}

fn build_ws_url(base: &str, query: &[(&str, String)]) -> String {
    if query.is_empty() {
        return base.to_string();
    }

    let separator = if base.contains('?') { '&' } else { '?' };
    let query_string = query
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}{separator}{query_string}")
}

fn read_trimmed_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_required_env(name: &str) -> Result<String, CliError> {
    read_trimmed_env(name)
        .ok_or_else(|| CliError::InvalidArgument(format!("{name} environment variable is not set")))
}

fn parse_env_bool(name: &str) -> Option<bool> {
    read_trimmed_env(name).and_then(|value| match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    })
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
    use super::*;

    #[test]
    fn normalizes_provider_aliases() {
        assert_eq!(normalize_provider_name("driver"), Some("driver.dev"));
        assert_eq!(normalize_provider_name("driver.dev"), Some("driver.dev"));
        assert_eq!(normalize_provider_name("browser-use"), Some("browseruse"));
        assert_eq!(normalize_provider_name("browseruse"), Some("browseruse"));
        assert_eq!(
            normalize_provider_name("hyperbrowser"),
            Some("hyperbrowser")
        );
        assert_eq!(normalize_provider_name("browserless"), Some("browserless"));
        assert_eq!(normalize_provider_name("unknown"), None);
    }

    #[test]
    fn builds_ws_urls_with_query_parameters() {
        let url = build_ws_url(
            "wss://connect.browser-use.com",
            &[
                ("apiKey", "key-123".to_string()),
                ("proxyCountryCode", "us".to_string()),
            ],
        );

        assert_eq!(
            url,
            "wss://connect.browser-use.com?apiKey=key-123&proxyCountryCode=us"
        );
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
}
