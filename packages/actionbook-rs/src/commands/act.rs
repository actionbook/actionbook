use colored::Colorize;

use crate::api::ApiClient;
use crate::cli::Cli;
use crate::config::Config;
use crate::error::Result;

pub async fn run(cli: &Cli, area_id: &str) -> Result<()> {
    let mut config = Config::load()?;
    if let Some(ref key) = cli.api_key {
        config.api.api_key = Some(key.clone());
    }
    let client = ApiClient::from_config(&config)?;

    let detail = client.get_action_by_area_id_json(area_id).await?;

    if cli.json {
        let elements: Vec<serde_json::Value> = detail
            .elements
            .iter()
            .map(|(id, el)| {
                serde_json::json!({
                    "element_id": id,
                    "element_type": el.element_type,
                    "description": el.description,
                    "allow_methods": el.allow_methods,
                    "css_selector": el.css_selector,
                    "xpath_selector": el.xpath_selector,
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "area_id": detail.area_id,
                "url": detail.url,
                "description": detail.description,
                "elements": elements,
            })
        );
    } else {
        println!("{} {}", "Area:".cyan(), detail.area_id.white());
        if let Some(ref url) = detail.url {
            println!("{} {}", "URL:".cyan(), url);
        }
        if let Some(ref desc) = detail.description {
            println!("{} {}", "Description:".cyan(), desc);
        }
        println!();
        println!("{}", "Elements:".cyan().bold());

        let mut sorted_elements: Vec<_> = detail.elements.iter().collect();
        sorted_elements.sort_by_key(|(id, _)| id.to_string());

        for (i, (id, el)) in sorted_elements.iter().enumerate() {
            let el_type = el.element_type.as_deref().unwrap_or("unknown");
            let desc = el.description.as_deref().unwrap_or("");

            println!(
                "  {}. {} {} - {}",
                (i + 1).to_string().white(),
                id.yellow(),
                format!("[{}]", el_type).dimmed(),
                desc
            );

            if !el.allow_methods.is_empty() {
                println!(
                    "     {}: {}",
                    "Methods".dimmed(),
                    el.allow_methods.join(", ").green()
                );
            }
            if let Some(ref css) = el.css_selector {
                println!("     {}: {}", "CSS".dimmed(), css);
            }
            if let Some(ref xpath) = el.xpath_selector {
                println!("     {}: {}", "XPath".dimmed(), xpath);
            }
        }

        if detail.elements.is_empty() {
            println!("  {}", "(no elements found)".dimmed());
        }
    }

    Ok(())
}
