use crate::api::ApiClient;
use crate::cli::Cli;
use crate::config;
use crate::error::CliError;

pub async fn run(cli: &Cli, area_id: &str) -> Result<(), CliError> {
    let mut config = config::load_config()?;
    if let Some(ref key) = cli.api_key {
        config.api.api_key = Some(key.clone());
    }
    let client = ApiClient::from_config(&config)?;

    let result = client.get_action_by_area_id(area_id).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "area_id": area_id,
                "result": result,
            })
        );
    } else {
        println!("{}", result);
    }

    Ok(())
}
