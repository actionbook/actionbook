use colored::Colorize;

use crate::api::{ApiClient, SearchActionsParams};
use crate::cli::Cli;
use crate::config;
use crate::error::CliError;

pub async fn run(
    cli: &Cli,
    query: &str,
    domain: Option<&str>,
    url: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<(), CliError> {
    let mut config = config::load_config()?;
    if let Some(ref key) = cli.api_key {
        config.api.api_key = Some(key.clone());
    }
    let client = ApiClient::from_config(&config)?;

    let params = SearchActionsParams {
        query: query.to_string(),
        domain: domain.map(|s| s.to_string()),
        url: url.map(|s| s.to_string()),
        page: Some(page),
        page_size: Some(page_size),
        background: None,
    };

    let result = client.search_actions(params).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "query": query,
                "results": result,
            })
        );
    } else {
        println!("{}", result);

        // Print next step hint only when there are results
        if !result.contains("Total: 0") {
            println!(
                "\n{} {}",
                "Next step:".cyan(),
                "actionbook get \"<area_id>\"".white()
            );
        }
    }

    Ok(())
}
