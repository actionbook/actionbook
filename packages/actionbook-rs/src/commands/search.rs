use colored::Colorize;

use crate::api::{ApiClient, SearchActionsParams};
use crate::cli::Cli;
use crate::config::Config;
use crate::error::Result;

pub async fn run(
    _cli: &Cli,
    query: &str,
    domain: Option<&str>,
    url: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<()> {
    let config = Config::load()?;
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

    // Result is plain text, output directly
    println!("{}", result);

    // Print next step hint
    println!(
        "\n{} {}",
        "Next step:".cyan(),
        "actionbook get \"<area_id>\"".white()
    );

    Ok(())
}
