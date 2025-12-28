//! Handbook Builder CLI
//!
//! A command-line tool for generating handbook documentation from websites.
//! Supports both single-run mode (build/crawl) and worker mode (polling build_tasks).

use anyhow::Result;
use clap::{Parser, Subcommand};
use handbook_builder::db::create_pool_from_env;
use handbook_builder::worker::{setup_signal_handler, TaskProcessor, TaskRunner, WorkerConfig};
use handbook_builder::{build_handbook, sanitize_folder_name, Crawler};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "handbook-builder")]
#[command(about = "Generate handbook documentation from websites using Claude AI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze a website and generate handbook (action.md + overview.md)
    Build {
        /// URL to analyze
        #[arg(short, long)]
        url: String,

        /// Output directory for handbooks (default: ./handbooks)
        #[arg(short, long, default_value = "./handbooks")]
        output_dir: PathBuf,

        /// Custom site name for the folder (auto-detected if not provided)
        #[arg(short, long)]
        name: Option<String>,

        /// Output as JSON instead of markdown files
        #[arg(long)]
        json: bool,
    },

    /// Crawl a website and show extracted information (without AI analysis)
    Crawl {
        /// URL to crawl
        #[arg(short, long)]
        url: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Run as worker, polling build_tasks table for pending tasks
    Worker {
        /// Poll interval in seconds (default: 5)
        #[arg(short, long, default_value = "5")]
        poll_interval: u64,

        /// Disable embedding generation
        #[arg(long)]
        no_embeddings: bool,

        /// Run once and exit (for testing)
        #[arg(long)]
        once: bool,

        /// Task timeout in seconds (default: 300)
        #[arg(short, long, default_value = "300")]
        timeout: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Commands::Build {
            url,
            output_dir,
            name,
            json,
        } => {
            info!("Building handbook for: {}", url);

            // Determine folder name first
            let requested_name = name.clone().unwrap_or_else(|| {
                // Extract site name from URL as default
                url::Url::parse(&url)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_string()))
                    .map(|h| {
                        h.replace("www.", "")
                            .split('.')
                            .next()
                            .unwrap_or(&h)
                            .to_string()
                    })
                    .unwrap_or_else(|| "unknown".to_string())
            });
            let folder_name = sanitize_folder_name(&requested_name);

            // Build handbook with the determined folder name and output directory
            let output = build_handbook(
                &url,
                Some(&folder_name),
                Some(output_dir.to_str().unwrap_or("./handbooks")),
            )
            .await?;

            if json {
                // Output as JSON to stdout
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                let site_dir = output_dir.join(&folder_name);

                // Create directory
                std::fs::create_dir_all(&site_dir)?;

                // Write action.md
                let action_path = site_dir.join("action.md");
                std::fs::write(&action_path, output.action.to_markdown())?;
                info!("Written: {}", action_path.display());

                // Write overview.md
                let overview_path = site_dir.join("overview.md");
                std::fs::write(&overview_path, output.overview.to_markdown())?;
                info!("Written: {}", overview_path.display());

                println!("\nHandbook generated successfully!");
                println!("  Site: {}", folder_name);
                println!("  Location: {}", site_dir.display());
                println!("  Files:");
                println!("    - action.md");
                println!("    - overview.md");

                // Check if prompt.md was created
                let prompt_path = site_dir.join("prompt.md");
                if prompt_path.exists() {
                    println!("    - prompt.md (customize for future generations)");
                }
            }
        }

        Commands::Crawl { url, json } => {
            info!("Crawling: {}", url);

            let crawler = Crawler::new()?;
            let context = crawler.crawl(&url).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&context)?);
            } else {
                println!("=== Web Context ===\n");
                println!("URL: {}", context.base_url);
                println!("Title: {}", context.title);
                println!(
                    "Description: {}",
                    context.meta_description.as_deref().unwrap_or("N/A")
                );
                println!("Site Type: {}", context.site_type);
                println!();

                println!("=== Navigation ({} links) ===", context.navigation.len());
                for link in context.navigation.iter().take(10) {
                    println!("  - {} -> {}", link.text, link.href);
                }
                if context.navigation.len() > 10 {
                    println!("  ... and {} more", context.navigation.len() - 10);
                }
                println!();

                println!(
                    "=== Interactive Elements ({}) ===",
                    context.interactive_elements.len()
                );
                for el in context.interactive_elements.iter().take(10) {
                    println!(
                        "  - [{}] {} ({})",
                        el.element_type,
                        el.text.as_deref().unwrap_or("N/A"),
                        el.selector
                    );
                }
                if context.interactive_elements.len() > 10 {
                    println!(
                        "  ... and {} more",
                        context.interactive_elements.len() - 10
                    );
                }
                println!();

                println!("=== Sections ({}) ===", context.sections.len());
                for section in &context.sections {
                    println!(
                        "  - [{}] {} ({})",
                        section.content_type,
                        section.heading.as_deref().unwrap_or("No heading"),
                        section.selector
                    );
                }
            }
        }

        Commands::Worker {
            poll_interval,
            no_embeddings,
            once,
            timeout,
        } => {
            // Load .env file if present
            dotenvy::dotenv().ok();

            info!("Initializing worker...");

            // Create database pool
            let pool = create_pool_from_env().await?;
            info!("Database connection established");

            // Build worker config
            let config = WorkerConfig::builder()
                .poll_interval_secs(poll_interval)
                .task_timeout(Duration::from_secs(timeout))
                .enable_embeddings(!no_embeddings)
                .build();

            // Get OpenAI API key from env
            let openai_api_key = std::env::var("OPENAI_API_KEY").ok();

            // Create processor and runner
            let processor = TaskProcessor::new(config.clone(), openai_api_key.as_deref());
            let runner = TaskRunner::new(pool, config, processor);

            if once {
                // Run once mode
                info!("Running in single-task mode...");
                match runner.run_once().await {
                    Ok(true) => {
                        println!("Task processed successfully");
                    }
                    Ok(false) => {
                        println!("No pending tasks found");
                    }
                    Err(e) => {
                        eprintln!("Error processing task: {}", e);
                        return Err(e.into());
                    }
                }
            } else {
                // Setup graceful shutdown
                let shutdown = runner.shutdown_handle();
                setup_signal_handler(shutdown);

                // Run continuous worker loop
                runner.run().await?;
            }
        }
    }

    Ok(())
}
