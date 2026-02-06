use colored::Colorize;

use crate::api::ApiClient;
use crate::cli::{Cli, SourcesCommands};
use crate::config::Config;
use crate::error::Result;

pub async fn run(cli: &Cli, command: &SourcesCommands) -> Result<()> {
    match command {
        SourcesCommands::List => list(cli).await,
        SourcesCommands::Search { query } => search(cli, query).await,
    }
}

async fn list(cli: &Cli) -> Result<()> {
    let mut config = Config::load()?;
    if let Some(ref key) = cli.api_key {
        config.api.api_key = Some(key.clone());
    }
    let client = ApiClient::from_config(&config)?;

    let response = client.list_sources(Some(50)).await?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&response.results)?);
    } else {
        if response.results.is_empty() {
            println!("{} No sources available", "!".yellow());
            return Ok(());
        }

        println!("{} {} sources available\n", "✓".green(), response.count);

        for source in &response.results {
            println!(
                "{} {} {}",
                "●".cyan(),
                source.name.bold(),
                format!("(ID: {})", source.id).dimmed()
            );
            println!("  {} {}", "URL:".dimmed(), source.base_url);

            if let Some(ref desc) = source.description {
                if !desc.is_empty() {
                    let desc_preview = if desc.len() > 80 {
                        format!("{}...", &desc[..80])
                    } else {
                        desc.clone()
                    };
                    println!("  {} {}", "Desc:".dimmed(), desc_preview);
                }
            }

            if !source.tags.is_empty() {
                println!("  {} {}", "Tags:".dimmed(), source.tags.join(", "));
            }

            if let Some(score) = source.health_score {
                let score_color = if score >= 0.8 {
                    "●".green()
                } else if score >= 0.5 {
                    "●".yellow()
                } else {
                    "●".red()
                };
                println!("  {} Health: {:.0}%", score_color, score * 100.0);
            }

            println!();
        }
    }

    Ok(())
}

async fn search(cli: &Cli, query: &str) -> Result<()> {
    let mut config = Config::load()?;
    if let Some(ref key) = cli.api_key {
        config.api.api_key = Some(key.clone());
    }
    let client = ApiClient::from_config(&config)?;

    let response = client.search_sources(query, Some(20)).await?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&response.results)?);
    } else {
        if response.results.is_empty() {
            println!(
                "{} No sources found matching: {}",
                "!".yellow(),
                query.bold()
            );
            return Ok(());
        }

        println!(
            "{} Found {} sources for: {}\n",
            "✓".green(),
            response.count,
            query.bold()
        );

        for source in &response.results {
            println!(
                "{} {} {}",
                "●".cyan(),
                source.name.bold(),
                format!("(ID: {})", source.id).dimmed()
            );
            println!("  {} {}", "URL:".dimmed(), source.base_url);

            if let Some(ref desc) = source.description {
                if !desc.is_empty() {
                    let desc_preview = if desc.len() > 80 {
                        format!("{}...", &desc[..80])
                    } else {
                        desc.clone()
                    };
                    println!("  {} {}", "Desc:".dimmed(), desc_preview);
                }
            }

            if !source.tags.is_empty() {
                println!("  {} {}", "Tags:".dimmed(), source.tags.join(", "));
            }

            println!();
        }
    }

    Ok(())
}
