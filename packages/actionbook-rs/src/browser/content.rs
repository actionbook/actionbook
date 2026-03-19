//! Unified content retrieval API
//!
//! Provides a consistent interface for retrieving page content in different formats
//! across all browser backends (CDP, Camoufox, etc.)

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Content format for page retrieval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentFormat {
    /// Raw HTML content
    Html,
    /// Markdown conversion (AI-friendly, ~80% token reduction)
    Markdown,
    /// Accessibility tree (semantic structure, ~95% size reduction)
    AccessibilityTree,
}

impl Default for ContentFormat {
    fn default() -> Self {
        Self::Html
    }
}

impl std::fmt::Display for ContentFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Html => write!(f, "html"),
            Self::Markdown => write!(f, "markdown"),
            Self::AccessibilityTree => write!(f, "accessibility-tree"),
        }
    }
}

impl std::str::FromStr for ContentFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "html" => Ok(Self::Html),
            "markdown" | "md" => Ok(Self::Markdown),
            "accessibility-tree" | "a11y-tree" | "tree" => Ok(Self::AccessibilityTree),
            _ => Err(format!("Unknown content format: {}", s)),
        }
    }
}

/// Options for content retrieval
#[derive(Debug, Clone)]
pub struct ContentOptions {
    /// Desired output format
    pub format: ContentFormat,
    /// Include metadata (e.g., token count, page info)
    pub include_metadata: bool,
    /// Optimize for AI agent consumption
    pub optimize_for_ai: bool,
}

impl Default for ContentOptions {
    fn default() -> Self {
        Self {
            format: ContentFormat::Html,
            include_metadata: false,
            optimize_for_ai: false,
        }
    }
}

impl ContentOptions {
    /// Create options optimized for AI agent use
    pub fn for_ai_agent() -> Self {
        Self {
            format: ContentFormat::AccessibilityTree,
            include_metadata: true,
            optimize_for_ai: true,
        }
    }

    /// Create options for content analysis
    pub fn for_content_analysis() -> Self {
        Self {
            format: ContentFormat::Markdown,
            include_metadata: true,
            optimize_for_ai: true,
        }
    }

    /// Create options for debugging/development
    pub fn for_debugging() -> Self {
        Self {
            format: ContentFormat::Html,
            include_metadata: true,
            optimize_for_ai: false,
        }
    }
}

/// Content response with metadata
#[derive(Debug, Clone)]
pub struct ContentResponse {
    /// The actual content
    pub content: String,
    /// Content format
    pub format: ContentFormat,
    /// Estimated token count (for AI context management)
    pub estimated_tokens: Option<usize>,
    /// Original size in bytes
    pub size_bytes: usize,
    /// Page URL (if available)
    pub url: Option<String>,
    /// Page title (if available)
    pub title: Option<String>,
}

impl ContentResponse {
    /// Create a basic response
    pub fn new(content: String, format: ContentFormat) -> Self {
        let size_bytes = content.len();
        Self {
            content,
            format,
            estimated_tokens: None,
            size_bytes,
            url: None,
            title: None,
        }
    }

    /// Create response with metadata
    pub fn with_metadata(
        content: String,
        format: ContentFormat,
        url: Option<String>,
        title: Option<String>,
    ) -> Self {
        let size_bytes = content.len();
        let estimated_tokens = Some(Self::estimate_tokens_for_format(&content, format));

        Self {
            content,
            format,
            estimated_tokens,
            size_bytes,
            url,
            title,
        }
    }

    /// Estimate token count based on format
    fn estimate_tokens_for_format(content: &str, format: ContentFormat) -> usize {
        match format {
            // HTML: ~4 chars per token (more verbose with tags)
            ContentFormat::Html => content.len() / 4,
            // Markdown: ~5 chars per token (cleaner than HTML)
            ContentFormat::Markdown => content.len() / 5,
            // Accessibility Tree: ~10 chars per token (very compact)
            ContentFormat::AccessibilityTree => content.len() / 10,
        }
    }

    /// Get compression ratio compared to HTML
    pub fn compression_ratio(&self) -> f64 {
        match self.format {
            ContentFormat::Html => 1.0,
            ContentFormat::Markdown => 0.2, // ~80% reduction
            ContentFormat::AccessibilityTree => 0.05, // ~95% reduction
        }
    }

    /// Format metadata for display
    pub fn format_metadata(&self) -> String {
        let mut parts = vec![
            format!("Format: {}", self.format),
            format!("Size: {} bytes", self.size_bytes),
        ];

        if let Some(tokens) = self.estimated_tokens {
            parts.push(format!("Estimated tokens: {}", tokens));
        }

        if let Some(url) = &self.url {
            parts.push(format!("URL: {}", url));
        }

        if let Some(title) = &self.title {
            parts.push(format!("Title: {}", title));
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_format_parsing() {
        assert_eq!(
            "html".parse::<ContentFormat>().unwrap(),
            ContentFormat::Html
        );
        assert_eq!(
            "markdown".parse::<ContentFormat>().unwrap(),
            ContentFormat::Markdown
        );
        assert_eq!(
            "md".parse::<ContentFormat>().unwrap(),
            ContentFormat::Markdown
        );
        assert_eq!(
            "accessibility-tree".parse::<ContentFormat>().unwrap(),
            ContentFormat::AccessibilityTree
        );
        assert_eq!(
            "tree".parse::<ContentFormat>().unwrap(),
            ContentFormat::AccessibilityTree
        );
    }

    #[test]
    fn test_token_estimation() {
        let html = "<html><body><p>Hello World</p></body></html>";
        let response = ContentResponse::new(html.to_string(), ContentFormat::Html);
        assert_eq!(response.estimated_tokens, None);

        let response =
            ContentResponse::with_metadata(html.to_string(), ContentFormat::Html, None, None);
        assert!(response.estimated_tokens.is_some());
    }

    #[test]
    fn test_ai_optimization_presets() {
        let opts = ContentOptions::for_ai_agent();
        assert_eq!(opts.format, ContentFormat::AccessibilityTree);
        assert!(opts.optimize_for_ai);

        let opts = ContentOptions::for_content_analysis();
        assert_eq!(opts.format, ContentFormat::Markdown);
        assert!(opts.optimize_for_ai);
    }
}
