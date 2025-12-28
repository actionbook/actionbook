//! Handbook Builder - A Rust service for analyzing websites and generating handbook documentation
//!
//! This service crawls websites, analyzes their structure using Claude AI,
//! and generates structured handbook documentation for AI agents.
//!
//! The output follows a standard format with two files:
//! - `action.md` - Describes how to interact with page elements
//! - `overview.md` - Provides context about the page structure

pub mod analyzer;
pub mod chunker;
pub mod crawler;
pub mod db;
pub mod embedding;
pub mod error;
pub mod fixer;
pub mod handbook;
pub mod prompt_manager;
pub mod validator;
pub mod worker;

pub use analyzer::Analyzer;
pub use crawler::{Crawler, CrawlerConfig};
pub use error::{HandbookError, Result};
pub use fixer::Fixer;
pub use handbook::{
    Action, ActionHandbook, BestPractice, ElementState, ErrorScenario, FilterCategory,
    sanitize_folder_name, HandbookOutput, NavigationItem, OverviewDoc, PageElement, WebContext,
};
pub use prompt_manager::PromptManager;
pub use validator::{ValidationResult, Validator};

/// Build a handbook from a URL with validation and auto-fix
///
/// This is the main entry point for generating handbooks.
/// Returns a HandbookOutput containing both action.md and overview.md content.
///
/// The process includes:
/// 1. Crawl the website
/// 2. Generate initial handbook with AI
/// 3. Validate quality
/// 4. Auto-fix issues if needed (up to 3 attempts)
/// 5. Generate/use customizable prompt.md
///
/// # Example
/// ```ignore
/// use handbook_builder::build_handbook;
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let output = build_handbook("https://example.com", Some("mysite")).await?;
///
///     // Save to files
///     let folder = format!("handbooks/{}", output.folder_name());
///     std::fs::create_dir_all(&folder)?;
///     std::fs::write(format!("{}/action.md", folder), output.action.to_markdown())?;
///     std::fs::write(format!("{}/overview.md", folder), output.overview.to_markdown())?;
///
///     Ok(())
/// }
/// ```
pub async fn build_handbook(url: &str, site_name: Option<&str>, output_dir: Option<&str>) -> Result<HandbookOutput> {
    build_handbook_with_config(url, site_name, output_dir, 3).await
}

/// Build a handbook without custom prompt files (for worker mode)
///
/// This version skips checking for custom prompt.md files and always uses
/// the default analysis prompt. Use this for automated/worker processing.
pub async fn build_handbook_simple(url: &str) -> Result<HandbookOutput> {
    use tracing::info;

    let crawler = Crawler::new()?;
    let analyzer = Analyzer::new();

    // Step 1: Crawl the website
    info!("Step 1: Crawling website...");
    let context = crawler.crawl(url).await?;
    info!(
        "Crawled {} interactive elements, {} content blocks",
        context.interactive_elements.len(),
        context.content_blocks.len()
    );

    // Step 2: Generate handbook with default prompt (no custom prompt)
    info!("Step 2: Generating handbook...");
    let handbook = analyzer.analyze(&context).await?;

    // Extract site name from URL
    let site_name = extract_site_name_from_url(url);

    info!("✓ Handbook generated for: {}", site_name);

    Ok(HandbookOutput {
        site_name,
        action: handbook.action,
        overview: handbook.overview,
    })
}

/// Build a handbook with custom max fix attempts and site name
pub async fn build_handbook_with_config(
    url: &str,
    site_name: Option<&str>,
    output_dir: Option<&str>,
    max_fix_attempts: usize,
) -> Result<HandbookOutput> {
    use tracing::{info, warn};

    let crawler = Crawler::new()?;
    let analyzer = Analyzer::new();
    let validator = Validator::new();
    let fixer = Fixer::with_max_attempts(max_fix_attempts);
    // Use provided output directory or default
    let base_dir = output_dir.unwrap_or("./handbooks");
    let prompt_manager = PromptManager::with_base_dir(base_dir);

    // Step 1: Crawl the website
    info!("Step 1: Crawling website...");
    let context = crawler.crawl(url).await?;
    info!(
        "Crawled {} interactive elements, {} content blocks",
        context.interactive_elements.len(),
        context.content_blocks.len()
    );

    // Determine site name for prompt file (use provided name or extract from URL)
    let site_name = site_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| extract_site_name_from_url(url));

    // Step 2: Check for existing custom prompt
    let custom_prompt = if prompt_manager.prompt_exists(&site_name) {
        info!("Found existing prompt file for: {}", site_name);
        info!("Using user-customized generation guidelines");
        Some(prompt_manager.load_prompt(&site_name)?)
    } else {
        info!("No existing prompt file. Will generate default prompt after handbook creation.");
        None
    };

    // Step 3: Generate initial handbook (with custom prompt if available)
    info!("Step 2: Generating handbook{}...",
        if custom_prompt.is_some() { " (with custom prompt)" } else { "" }
    );

    let mut handbook = if let Some(prompt) = &custom_prompt {
        // Use custom prompt to guide generation
        analyzer.analyze_with_prompt(&context, prompt).await?
    } else {
        // Generate with default prompt
        analyzer.analyze(&context).await?
    };

    // Step 4: Automatic validation and fix loop (quality improvement)
    let mut fixes_applied = 0usize;
    loop {
        info!(
            "Step 3.{}: Validating handbook quality...",
            fixes_applied + 1
        );
        let validation = validator.validate(&handbook, &context);

        info!(
            "Validation result: {} issues, quality score: {}",
            validation.issues.len(),
            validation.quality_score
        );

        // Print important issues
        for issue in validation.important_issues() {
            warn!(
                "[{:?}] {:?}: {}",
                issue.severity, issue.category, issue.description
            );
        }

        // Check if we need to fix
        if !validation.needs_fix() {
            info!("✓ Handbook quality is acceptable (score: {})", validation.quality_score);
            break;
        }

        // Check if we've exhausted attempts
        if fixes_applied >= max_fix_attempts {
            warn!(
                "⚠️  Max fix attempts ({}) reached. Returning best effort handbook (score: {})",
                max_fix_attempts, validation.quality_score
            );
            break;
        }

        // Attempt to fix
        let attempt = fixes_applied + 1;
        info!(
            "Attempting to fix issues (attempt {}/{})",
            attempt, max_fix_attempts
        );
        match fixer.fix(handbook.clone(), &context, &validation, attempt).await {
            Ok(fixed_handbook) => {
                handbook = fixed_handbook;
                fixes_applied += 1;
            }
            Err(e) => {
                warn!("Fix attempt failed: {}. Using previous version.", e);
                break;
            }
        }
    }

    // Step 5: Generate and save prompt file if this is the first time
    if custom_prompt.is_none() {
        info!("Step 4: Generating customizable prompt file...");
        let initial_prompt = prompt_manager.generate_initial_prompt(&site_name, &context);
        prompt_manager.save_prompt(&site_name, &initial_prompt)?;
        info!("✓ Prompt saved to: {}", prompt_manager.get_prompt_path(&site_name).display());
        info!("   Users can edit this file to customize future handbook generation.");
    }

    info!("✓ Handbook generation complete");
    Ok(handbook)
}

/// Extract a clean site name from URL for folder naming
fn extract_site_name_from_url(url: &str) -> String {
    url::Url::parse(url)
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
}
