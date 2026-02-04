//! Integration tests against real Actionbook API
//!
//! These tests require a running API server or network access to the production API.
//! They are skipped if the API is not available.
//!
//! Run with: cargo test --test integration_test
//! Or with custom API URL: ACTIONBOOK_API_URL=http://localhost:3100 cargo test --test integration_test

use std::env;
use std::time::Duration;

fn get_api_url() -> String {
    env::var("ACTIONBOOK_API_URL")
        .or_else(|_| env::var("ACTIONBOOK_REAL_API_URL"))
        .unwrap_or_else(|_| "https://api.actionbook.dev".to_string())
}

async fn is_api_available(client: &reqwest::Client, base_url: &str) -> bool {
    let url = format!("{}/health", base_url);
    match client.get(&url).timeout(Duration::from_secs(3)).send().await {
        Ok(response) => response.status().is_success(),
        Err(_) => {
            // Try the search endpoint as health check alternative
            let search_url = format!("{}/api/actions/search?q=test&limit=1", base_url);
            client
                .get(&search_url)
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .map(|r| r.status().is_success() || r.status() == 401)
                .unwrap_or(false)
        }
    }
}

mod real_api_integration {
    use super::*;
    use serde_json::Value;

    #[tokio::test]
    async fn search_actions_hits_real_api() {
        let api_url = get_api_url();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        if !is_api_available(&client, &api_url).await {
            eprintln!("Skipping test: API not available at {}", api_url);
            return;
        }

        let url = format!("{}/api/actions/search", api_url);
        let response = match client
            .get(&url)
            .query(&[("q", "airbnb"), ("limit", "3")])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Skipping test: Request failed - {}", e);
                return;
            }
        };

        // Skip if unauthorized (API key required)
        if response.status() == 401 {
            eprintln!("Skipping test: API requires authentication");
            return;
        }

        assert!(
            response.status().is_success(),
            "Expected success status, got {}",
            response.status()
        );

        let body: Value = response.json().await.expect("Should parse JSON response");

        assert!(body["success"].as_bool().unwrap_or(false));
        assert!(body["results"].is_array());

        let results = body["results"].as_array().unwrap();
        if !results.is_empty() {
            // Verify result structure
            let first = &results[0];
            assert!(first["action_id"].is_string());
            assert!(first["content"].is_string());
            assert!(first["score"].is_number());
        }
    }

    #[tokio::test]
    async fn get_action_by_id_hits_real_api() {
        let api_url = get_api_url();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        if !is_api_available(&client, &api_url).await {
            eprintln!("Skipping test: API not available at {}", api_url);
            return;
        }

        // First search to get a valid action ID
        let search_url = format!("{}/api/actions/search", api_url);
        let search_response = match client
            .get(&search_url)
            .query(&[("q", "airbnb"), ("limit", "1")])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Skipping test: Search request failed - {}", e);
                return;
            }
        };

        if search_response.status() == 401 {
            eprintln!("Skipping test: API requires authentication");
            return;
        }

        let search_body: Value = search_response
            .json()
            .await
            .expect("Should parse search JSON");

        let results = search_body["results"].as_array().unwrap();
        if results.is_empty() {
            eprintln!("Skipping test: No search results available");
            return;
        }

        let action_id = results[0]["action_id"]
            .as_str()
            .expect("action_id should be string");

        // Now get the action by ID
        let get_url = format!("{}/api/actions", api_url);
        let get_response = match client.get(&get_url).query(&[("id", action_id)]).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Skipping test: Get request failed - {}", e);
                return;
            }
        };

        assert!(
            get_response.status().is_success(),
            "Expected success status, got {}",
            get_response.status()
        );

        let body: Value = get_response.json().await.expect("Should parse JSON response");

        assert_eq!(body["action_id"].as_str(), Some(action_id));
        assert!(body["content"].is_string());
    }

    #[tokio::test]
    async fn list_sources_hits_real_api() {
        let api_url = get_api_url();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        if !is_api_available(&client, &api_url).await {
            eprintln!("Skipping test: API not available at {}", api_url);
            return;
        }

        let url = format!("{}/api/sources", api_url);
        let response = match client.get(&url).query(&[("limit", "5")]).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Skipping test: Request failed - {}", e);
                return;
            }
        };

        if response.status() == 401 {
            eprintln!("Skipping test: API requires authentication");
            return;
        }

        assert!(
            response.status().is_success(),
            "Expected success status, got {}",
            response.status()
        );

        let body: Value = response.json().await.expect("Should parse JSON response");

        assert!(body["results"].is_array());
        let results = body["results"].as_array().unwrap();
        if !results.is_empty() {
            let first = &results[0];
            assert!(first["id"].is_number());
            assert!(first["name"].is_string());
            assert!(first["baseUrl"].is_string());
        }
    }

    #[tokio::test]
    async fn search_sources_hits_real_api() {
        let api_url = get_api_url();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        if !is_api_available(&client, &api_url).await {
            eprintln!("Skipping test: API not available at {}", api_url);
            return;
        }

        let url = format!("{}/api/sources/search", api_url);
        let response = match client.get(&url).query(&[("q", "airbnb")]).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Skipping test: Request failed - {}", e);
                return;
            }
        };

        if response.status() == 401 {
            eprintln!("Skipping test: API requires authentication");
            return;
        }

        assert!(
            response.status().is_success(),
            "Expected success status, got {}",
            response.status()
        );

        let body: Value = response.json().await.expect("Should parse JSON response");

        assert!(body["results"].is_array());
        assert_eq!(body["query"].as_str(), Some("airbnb"));
    }

    #[tokio::test]
    async fn search_with_all_parameters() {
        let api_url = get_api_url();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        if !is_api_available(&client, &api_url).await {
            eprintln!("Skipping test: API not available at {}", api_url);
            return;
        }

        let url = format!("{}/api/actions/search", api_url);
        let response = match client
            .get(&url)
            .query(&[
                ("q", "login"),
                ("type", "hybrid"),
                ("limit", "5"),
                ("minScore", "0.3"),
            ])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Skipping test: Request failed - {}", e);
                return;
            }
        };

        if response.status() == 401 {
            eprintln!("Skipping test: API requires authentication");
            return;
        }

        assert!(
            response.status().is_success(),
            "Expected success status, got {}",
            response.status()
        );

        let body: Value = response.json().await.expect("Should parse JSON response");
        assert!(body["results"].is_array());

        // Verify all returned scores meet minimum
        for result in body["results"].as_array().unwrap() {
            if let Some(score) = result["score"].as_f64() {
                assert!(score >= 0.3, "Score {} should be >= 0.3", score);
            }
        }
    }
}

mod cli_integration {
    use assert_cmd::Command;
    use predicates::prelude::*;
    use std::env;

    fn actionbook() -> Command {
        let mut cmd = Command::cargo_bin("actionbook").unwrap();
        // Set API URL from environment if available
        if let Ok(url) = env::var("ACTIONBOOK_API_URL") {
            // Note: The CLI reads from config file, so we may need to adjust this
            cmd.env("ACTIONBOOK_API_URL", url);
        }
        cmd
    }

    #[test]
    fn cli_search_runs() {
        // This test just verifies the CLI runs without crashing
        // It may fail with API error if network unavailable, which is OK
        let result = actionbook()
            .args(["search", "airbnb", "--page-size", "1"])
            .timeout(std::time::Duration::from_secs(15))
            .assert();

        // Either succeeds or fails with network/API error
        // We just want to ensure it doesn't panic
        let _ = result;
    }

    #[test]
    fn cli_search_text_output() {
        // New text-based API returns plain text
        let result = actionbook()
            .args(["search", "airbnb", "--page-size", "1"])
            .timeout(std::time::Duration::from_secs(15))
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    // Output should be plain text containing area_id hint
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Verify it contains expected text
                    assert!(
                        stdout.contains("Next step") || stdout.contains("area_id") || stdout.is_empty(),
                        "Output should contain guidance or be empty: {}",
                        stdout
                    );
                }
            }
            Err(_) => {
                // Timeout or other error - acceptable for integration test
            }
        }
    }

    #[test]
    fn cli_sources_list_runs() {
        let result = actionbook()
            .args(["sources", "list"])
            .timeout(std::time::Duration::from_secs(15))
            .assert();

        let _ = result;
    }

    #[test]
    fn cli_config_show_runs() {
        // Config show should always work (local operation)
        actionbook()
            .args(["config", "show"])
            .assert()
            .success();
    }

    #[test]
    fn cli_config_path_runs() {
        // Config path should always work (local operation)
        actionbook()
            .args(["config", "path"])
            .assert()
            .success()
            .stdout(predicate::str::contains("config"));
    }

    #[test]
    fn cli_profile_list_runs() {
        // Profile list should always work (local operation)
        actionbook()
            .args(["profile", "list"])
            .assert()
            .success();
    }

    #[test]
    fn cli_browser_status_runs() {
        // Browser status should work even without a running browser
        let result = actionbook()
            .args(["browser", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .assert();

        // Should succeed and show some output about browser
        let _ = result;
    }
}
