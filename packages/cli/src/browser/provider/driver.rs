use serde::Deserialize;

use crate::error::CliError;

const API_BASE: &str = "https://api.driver.dev";

pub struct DriverSession {
    pub session_id: String,
    pub cdp_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateResponse {
    session_id: String,
    cdp_url: Option<String>,
    status: String,
    #[serde(default)]
    #[allow(dead_code)]
    served_by: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

pub async fn create_session(api_key: &str) -> Result<DriverSession, CliError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{API_BASE}/v1/browser/session"))
        .bearer_auth(api_key)
        .json(&serde_json::json!({"type": "hosted"}))
        .send()
        .await?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CliError::DriverApiError(
            "invalid or missing Driver.dev API key (401)".to_string(),
        ));
    }
    if !status.is_success() {
        let body: ErrorResponse = resp.json().await.unwrap_or(ErrorResponse {
            error: format!("HTTP {status}"),
        });
        return Err(CliError::DriverApiError(body.error));
    }

    let body: CreateResponse = resp.json().await?;

    if body.status == "error" {
        return Err(CliError::DriverApiError(
            "Driver.dev session creation returned error status".to_string(),
        ));
    }

    let cdp_url = body.cdp_url.ok_or_else(|| {
        CliError::DriverApiError("Driver.dev session has no cdpUrl — browser may not be ready".to_string())
    })?;

    Ok(DriverSession {
        session_id: body.session_id,
        cdp_url,
    })
}

pub async fn stop_session(api_key: &str, session_id: &str) {
    let client = reqwest::Client::new();
    let result = client
        .delete(format!("{API_BASE}/v1/browser/session"))
        .bearer_auth(api_key)
        .query(&[("sessionId", session_id)])
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("driver.dev: stopped remote session {session_id}");
        }
        Ok(resp) => {
            tracing::warn!(
                "driver.dev: failed to stop session {session_id}: HTTP {}",
                resp.status()
            );
        }
        Err(e) => {
            tracing::warn!("driver.dev: failed to stop session {session_id}: {e}");
        }
    }
}
