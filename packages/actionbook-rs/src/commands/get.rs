use crate::api::ApiClient;
use crate::cli::Cli;
use crate::config::Config;
use crate::error::Result;

pub async fn run(_cli: &Cli, area_id: &str) -> Result<()> {
    let config = Config::load()?;
    let client = ApiClient::from_config(&config)?;

    let result = client.get_action_by_area_id(area_id).await?;

    // Result is plain text, output directly
    println!("{}", result);

    Ok(())
}
