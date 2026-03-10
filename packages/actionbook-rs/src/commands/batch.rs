//! Batch action execution — runs a sequence of browser actions from JSON.
//!
//! JSON format:
//! ```json
//! {
//!   "actions": [
//!     {"kind": "goto", "url": "https://example.com"},
//!     {"kind": "click", "selector": "#login"},
//!     {"kind": "type", "selector": "#email", "text": "user@test.com"},
//!     {"kind": "snapshot"}
//!   ],
//!   "stopOnError": true
//! }
//! ```

use std::io::Read;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::browser::BrowserDriver;
use crate::cli::Cli;
use crate::config::Config;
use crate::error::{ActionbookError, Result};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchInput {
    actions: Vec<BatchAction>,
    #[serde(default = "default_stop_on_error")]
    stop_on_error: bool,
}

fn default_stop_on_error() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct BatchAction {
    kind: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    selector: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    timeout: Option<u64>,
}

#[derive(Debug, Serialize)]
struct StepResult {
    index: usize,
    kind: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct BatchOutput {
    results: Vec<StepResult>,
    total: usize,
    successful: usize,
    failed: usize,
}

pub async fn run(cli: &Cli, config: &Config, file: Option<&str>, delay_ms: u64) -> Result<()> {
    // Read JSON input
    let json_str = match file {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| ActionbookError::Other(format!("Failed to read file '{}': {}", path, e)))?,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| ActionbookError::Other(format!("Failed to read stdin: {}", e)))?;
            buf
        }
    };

    let input: BatchInput = serde_json::from_str(&json_str)
        .map_err(|e| ActionbookError::Other(format!("Invalid batch JSON: {}", e)))?;

    // Create browser driver
    let mut driver = super::browser::create_browser_driver_public(cli, config).await?;

    let total = input.actions.len();
    let mut results = Vec::with_capacity(total);
    let mut successful = 0usize;
    let mut failed = 0usize;

    for (i, action) in input.actions.iter().enumerate() {
        let result = execute_action(&mut driver, action).await;

        let step = match result {
            Ok(data) => {
                successful += 1;
                StepResult {
                    index: i,
                    kind: action.kind.clone(),
                    success: true,
                    error: None,
                    data,
                }
            }
            Err(e) => {
                failed += 1;
                let step = StepResult {
                    index: i,
                    kind: action.kind.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    data: None,
                };

                if input.stop_on_error {
                    results.push(step);
                    break;
                }
                step
            }
        };

        results.push(step);

        // Delay between steps
        if delay_ms > 0 && i + 1 < total {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    let output = BatchOutput {
        results,
        total,
        successful,
        failed,
    };

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

async fn execute_action(
    driver: &mut BrowserDriver,
    action: &BatchAction,
) -> Result<Option<serde_json::Value>> {
    match action.kind.as_str() {
        "goto" => {
            let url = action
                .url
                .as_deref()
                .ok_or_else(|| ActionbookError::Other("'goto' requires 'url' field".to_string()))?;
            driver.goto(url).await?;
            Ok(None)
        }
        "click" => {
            let selector = action.selector.as_deref().ok_or_else(|| {
                ActionbookError::Other("'click' requires 'selector' field".to_string())
            })?;
            driver.click(selector).await?;
            Ok(None)
        }
        "type" => {
            let selector = action.selector.as_deref().ok_or_else(|| {
                ActionbookError::Other("'type' requires 'selector' field".to_string())
            })?;
            let text = action.text.as_deref().unwrap_or("");
            driver.type_text(selector, text).await?;
            Ok(None)
        }
        "fill" => {
            let selector = action.selector.as_deref().ok_or_else(|| {
                ActionbookError::Other("'fill' requires 'selector' field".to_string())
            })?;
            let text = action.text.as_deref().unwrap_or("");
            driver.fill(selector, text).await?;
            Ok(None)
        }
        "select" => {
            let selector = action.selector.as_deref().ok_or_else(|| {
                ActionbookError::Other("'select' requires 'selector' field".to_string())
            })?;
            let value = action.value.as_deref().ok_or_else(|| {
                ActionbookError::Other("'select' requires 'value' field".to_string())
            })?;
            driver.select(selector, value).await?;
            Ok(None)
        }
        "snapshot" => {
            let raw = driver.get_accessibility_tree_raw().await?;
            // Phase 2b: Convert to JSON string for typed deserialization
            let raw_json = serde_json::to_string(&raw)?;
            let (nodes, _cache) = crate::browser::snapshot::parse_ax_tree(
                &raw_json,
                crate::browser::snapshot::SnapshotFilter::All,
                None,
                None,
            )?;
            let output = crate::browser::snapshot::format_compact(&nodes);
            Ok(Some(serde_json::json!({
                "nodeCount": nodes.len(),
                "content": output,
            })))
        }
        "text" => {
            let content = driver.get_content().await?;
            Ok(Some(serde_json::json!({ "content": content })))
        }
        "screenshot" => {
            let data = driver.screenshot().await?;
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            Ok(Some(serde_json::json!({
                "bytes": data.len(),
                "base64Length": b64.len(),
            })))
        }
        "scroll" => {
            // Default scroll down by one viewport height
            let js = "window.scrollBy(0, window.innerHeight)";
            driver.eval(js).await?;
            Ok(None)
        }
        "wait" => {
            let timeout_ms = action.timeout.unwrap_or(5000);
            if let Some(ref selector) = action.selector {
                // Wait for element (CDP only)
                match driver {
                    BrowserDriver::Cdp(ref mgr) => {
                        mgr.wait_for_element(None, selector, timeout_ms).await?;
                    }
                    #[cfg(feature = "camoufox")]
                    _ => {
                        tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
                    }
                }
            } else {
                // Just sleep
                tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
            }
            Ok(None)
        }
        other => Err(ActionbookError::Other(format!(
            "Unknown action kind: '{}'",
            other
        ))),
    }
}
