//! Web crawler module for fetching and parsing web pages

use crate::error::{HandbookError, Result};
use crate::handbook::{ContentBlock, InteractiveElement, NavLink, PageSection, SiteType, WebContext};
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use url::Url;

/// Configuration for the web crawler
#[derive(Debug, Clone)]
pub struct CrawlerConfig {
    /// Connection timeout (default: 10 seconds)
    pub connect_timeout: Duration,
    /// Request timeout (default: 30 seconds)
    pub request_timeout: Duration,
    /// Maximum number of retry attempts (default: 3)
    pub max_retries: u32,
    /// Initial delay between retries (default: 1 second, doubles each retry)
    pub retry_base_delay: Duration,
    /// Maximum delay between retries (default: 10 seconds)
    pub retry_max_delay: Duration,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_base_delay: Duration::from_secs(1),
            retry_max_delay: Duration::from_secs(10),
        }
    }
}

/// Web crawler for fetching and analyzing web pages
pub struct Crawler {
    client: Client,
    config: CrawlerConfig,
}

impl Crawler {
    /// Create a new crawler instance with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(CrawlerConfig::default())
    }

    /// Create a new crawler instance with custom configuration
    pub fn with_config(config: CrawlerConfig) -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .build()
            .map_err(|e| HandbookError::FetchError {
                url: "client_init".to_string(),
                source: e,
            })?;

        Ok(Self { client, config })
    }

    /// Fetch a URL and return the HTML content with retry support
    pub async fn fetch(&self, url: &str) -> Result<String> {
        info!("Fetching URL: {}", url);

        let mut last_error = String::new();

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                // Calculate exponential backoff delay
                let delay = std::cmp::min(
                    self.config.retry_base_delay * 2u32.saturating_pow(attempt - 1),
                    self.config.retry_max_delay,
                );
                warn!(
                    "Retry attempt {}/{} for {} after {:?}",
                    attempt, self.config.max_retries, url, delay
                );
                sleep(delay).await;
            }

            match self.fetch_once(url).await {
                Ok(html) => {
                    if attempt > 0 {
                        info!("Successfully fetched {} on attempt {}", url, attempt + 1);
                    }
                    return Ok(html);
                }
                Err(e) => {
                    last_error = e.to_string();
                    warn!(
                        "Fetch attempt {} failed for {}: {}",
                        attempt + 1,
                        url,
                        last_error
                    );

                    // Don't retry on client errors (4xx) except 429 (rate limit)
                    if let HandbookError::HttpStatusError { status, .. } = &e {
                        if (400..500).contains(status) && *status != 429 {
                            return Err(e);
                        }
                    }
                }
            }
        }

        Err(HandbookError::RetryExhausted {
            url: url.to_string(),
            attempts: self.config.max_retries + 1,
            last_error,
        })
    }

    /// Single fetch attempt without retry
    async fn fetch_once(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| HandbookError::FetchError {
                url: url.to_string(),
                source: e,
            })?;

        // Check for HTTP errors (4xx, 5xx)
        let status = response.status();
        if !status.is_success() {
            return Err(HandbookError::HttpStatusError {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }

        let html = response
            .text()
            .await
            .map_err(|e| HandbookError::FetchError {
                url: url.to_string(),
                source: e,
            })?;

        debug!("Fetched {} bytes from {}", html.len(), url);
        Ok(html)
    }

    /// Parse HTML and extract web context
    pub fn parse(&self, url: &str, html: &str) -> Result<WebContext> {
        info!("Parsing HTML from: {}", url);

        let document = Html::parse_document(html);
        let base_url = Url::parse(url).map_err(|_| HandbookError::InvalidUrl(url.to_string()))?;

        // Extract title
        let title = self.extract_title(&document);

        // Extract meta description
        let meta_description = self.extract_meta_description(&document);

        // Extract navigation
        let navigation = self.extract_navigation(&document, &base_url);

        // Extract interactive elements
        let interactive_elements = self.extract_interactive_elements(&document);

        // Extract page sections
        let sections = self.extract_sections(&document);

        // Extract content blocks for information extraction
        let content_blocks = self.extract_content_blocks(&document);

        // Detect site type
        let site_type = self.detect_site_type(&document, &interactive_elements, &sections);

        // Get a truncated HTML snippet for Claude analysis
        let html_snippet = Self::get_html_snippet(html, 15000);

        Ok(WebContext {
            base_url: url.to_string(),
            title,
            meta_description,
            site_type,
            navigation,
            interactive_elements,
            sections,
            content_blocks,
            html_snippet,
        })
    }

    /// Fetch and parse a URL in one step
    pub async fn crawl(&self, url: &str) -> Result<WebContext> {
        let html = self.fetch(url).await?;
        self.parse(url, &html)
    }

    fn extract_title(&self, document: &Html) -> String {
        let selector = Selector::parse("title").unwrap();
        document
            .select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn extract_meta_description(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("meta[name='description']").unwrap();
        document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
    }

    fn extract_navigation(&self, document: &Html, base_url: &Url) -> Vec<NavLink> {
        let mut nav_links = Vec::new();

        // Try common navigation selectors
        let nav_selectors = ["nav a", "header a", "[role='navigation'] a", ".nav a"];

        for selector_str in nav_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    if let Some(href) = element.value().attr("href") {
                        let text: String = element.text().collect();
                        let text = text.trim().to_string();

                        if !text.is_empty() && !href.starts_with('#') {
                            let full_href = base_url
                                .join(href)
                                .map(|u| u.to_string())
                                .unwrap_or_else(|_| href.to_string());

                            nav_links.push(NavLink {
                                text,
                                href: full_href,
                            });
                        }
                    }
                }
            }
        }

        // Deduplicate
        nav_links.sort_by(|a, b| a.href.cmp(&b.href));
        nav_links.dedup_by(|a, b| a.href == b.href);

        nav_links
    }

    fn extract_interactive_elements(&self, document: &Html) -> Vec<InteractiveElement> {
        let mut elements = Vec::new();

        // Buttons
        if let Ok(selector) = Selector::parse("button, [role='button'], input[type='button'], input[type='submit']") {
            for el in document.select(&selector) {
                let text: String = el.text().collect();
                let selector = self.build_selector(&el);
                let attributes = self.extract_attributes(&el);

                elements.push(InteractiveElement {
                    element_type: "button".to_string(),
                    selector,
                    text: if text.trim().is_empty() {
                        None
                    } else {
                        Some(text.trim().to_string())
                    },
                    attributes,
                });
            }
        }

        // Input fields
        if let Ok(selector) = Selector::parse("input[type='text'], input[type='email'], input[type='search'], textarea") {
            for el in document.select(&selector) {
                let selector = self.build_selector(&el);
                let attributes = self.extract_attributes(&el);
                let placeholder = el.value().attr("placeholder").map(|s| s.to_string());

                elements.push(InteractiveElement {
                    element_type: "input".to_string(),
                    selector,
                    text: placeholder,
                    attributes,
                });
            }
        }

        // Select dropdowns
        if let Ok(selector) = Selector::parse("select") {
            for el in document.select(&selector) {
                let selector = self.build_selector(&el);
                let attributes = self.extract_attributes(&el);

                elements.push(InteractiveElement {
                    element_type: "select".to_string(),
                    selector,
                    text: None,
                    attributes,
                });
            }
        }

        // Links with action-like text
        if let Ok(selector) = Selector::parse("a") {
            for el in document.select(&selector) {
                let text: String = el.text().collect();
                let text = text.trim().to_string();

                // Only include links that look like actions
                let action_keywords = ["filter", "sort", "view", "show", "apply", "search", "find"];
                if action_keywords
                    .iter()
                    .any(|kw| text.to_lowercase().contains(kw))
                {
                    let selector = self.build_selector(&el);
                    let attributes = self.extract_attributes(&el);

                    elements.push(InteractiveElement {
                        element_type: "link".to_string(),
                        selector,
                        text: Some(text),
                        attributes,
                    });
                }
            }
        }

        elements
    }

    fn extract_sections(&self, document: &Html) -> Vec<PageSection> {
        let mut sections = Vec::new();

        // Main content sections
        let section_selectors = [
            ("main", "main"),
            ("article", "article"),
            ("section", "section"),
            ("[role='main']", "[role='main']"),
        ];

        for (selector_str, base_selector) in section_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                for (idx, el) in document.select(&selector).enumerate() {
                    // Try to find a heading
                    let heading = if let Ok(h_selector) = Selector::parse("h1, h2, h3") {
                        el.select(&h_selector)
                            .next()
                            .map(|h| h.text().collect::<String>().trim().to_string())
                    } else {
                        None
                    };

                    // Detect content type
                    let content_type = self.detect_content_type(&el);

                    let selector = if idx == 0 {
                        base_selector.to_string()
                    } else {
                        format!("{}:nth-of-type({})", base_selector, idx + 1)
                    };

                    sections.push(PageSection {
                        heading,
                        content_type,
                        selector,
                    });
                }
            }
        }

        sections
    }

    fn detect_content_type(&self, element: &scraper::ElementRef) -> String {
        let html = element.html();

        if html.contains("<form") {
            return "form".to_string();
        }
        if html.contains("<table") {
            return "table".to_string();
        }
        if html.contains("<ul") || html.contains("<ol") {
            return "list".to_string();
        }
        if html.contains("grid") || html.contains("flex") {
            return "grid".to_string();
        }

        "text".to_string()
    }

    fn detect_site_type(
        &self,
        document: &Html,
        interactive_elements: &[InteractiveElement],
        sections: &[PageSection],
    ) -> SiteType {
        let html = document.html().to_lowercase();

        // Check for listing patterns
        if html.contains("companies") || html.contains("portfolio") || html.contains("catalog") {
            return SiteType::Listing;
        }

        // Check for form pages
        let form_elements = interactive_elements
            .iter()
            .filter(|e| e.element_type == "input" || e.element_type == "select")
            .count();
        if form_elements > 3 {
            return SiteType::Form;
        }

        // Check for blog/article
        if html.contains("<article") || html.contains("blog") {
            return SiteType::Blog;
        }

        // Check for dashboard patterns
        if html.contains("dashboard") || html.contains("analytics") {
            return SiteType::Dashboard;
        }

        // Check sections for grid/list content
        let has_grid = sections.iter().any(|s| s.content_type == "grid" || s.content_type == "list");
        if has_grid {
            return SiteType::Listing;
        }

        SiteType::Unknown
    }

    fn build_selector(&self, element: &scraper::ElementRef) -> String {
        let el = element.value();

        // Prefer data-testid
        if let Some(test_id) = el.attr("data-testid") {
            return format!("[data-testid='{}']", test_id);
        }

        // Then id
        if let Some(id) = el.attr("id") {
            return format!("#{}", id);
        }

        // Then aria-label
        if let Some(label) = el.attr("aria-label") {
            return format!("[aria-label='{}']", label);
        }

        // Then class + tag
        let tag = el.name();
        if let Some(classes) = el.attr("class") {
            let first_class = classes.split_whitespace().next().unwrap_or("");
            if !first_class.is_empty() {
                return format!("{}.{}", tag, first_class);
            }
        }

        // Fallback to tag name
        tag.to_string()
    }

    fn extract_attributes(&self, element: &scraper::ElementRef) -> Vec<(String, String)> {
        let el = element.value();
        let mut attrs = Vec::new();

        for attr in ["id", "class", "name", "data-testid", "aria-label", "type", "href"] {
            if let Some(value) = el.attr(attr) {
                attrs.push((attr.to_string(), value.to_string()));
            }
        }

        attrs
    }

    fn extract_content_blocks(&self, document: &Html) -> Vec<ContentBlock> {
        let mut blocks = Vec::new();

        // Strategy 1: Find divs with meaningful IDs (common pattern in content sites)
        if let Ok(selector) = Selector::parse("div[id], section[id], article[id]") {
            for el in document.select(&selector) {
                if let Some(id) = el.value().attr("id") {
                    // Filter out navigation, footer, header, and UI component IDs
                    let id_lower = id.to_lowercase();
                    if id_lower.contains("nav")
                        || id_lower.contains("header")
                        || id_lower.contains("footer")
                        || id_lower.contains("menu")
                        || id_lower.contains("sidebar")
                        || id_lower.contains("cookie")
                        || id_lower.contains("modal")
                        || id_lower.starts_with("vector-")
                        || id_lower.starts_with("p-")
                    {
                        continue;
                    }

                    // Extract heading
                    let heading = if let Ok(h_selector) = Selector::parse("h1, h2, h3, h4") {
                        el.select(&h_selector)
                            .next()
                            .map(|h| h.text().collect::<String>().trim().to_string())
                    } else {
                        None
                    };

                    // Extract text preview
                    let text: String = el.text().collect();
                    let text = text.trim();
                    let preview = if text.len() > 200 {
                        Some(format!("{}...", truncate_utf8(text, 200)))
                    } else if !text.is_empty() {
                        Some(text.to_string())
                    } else {
                        None
                    };

                    // Detect content type
                    let content_type = self.detect_block_content_type(&el);

                    // Generate human-readable name from ID
                    let name = id
                        .replace('-', " ")
                        .replace('_', " ")
                        .split_whitespace()
                        .map(|word| {
                            let mut chars = word.chars();
                            match chars.next() {
                                None => String::new(),
                                Some(c) => c.to_uppercase().chain(chars).collect(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");

                    blocks.push(ContentBlock {
                        id: id.to_string(),
                        name,
                        description: heading.clone(),
                        selector: format!("#{}", id),
                        content_type,
                        heading,
                        preview,
                    });
                }
            }
        }

        // Strategy 2: Find articles and sections with specific classes
        if let Ok(selector) = Selector::parse("article, section[class*='content'], div[class*='section']") {
            for el in document.select(&selector) {
                // Skip if already found by ID
                if el.value().attr("id").is_some() {
                    continue;
                }

                let classes = el.value().attr("class").unwrap_or("");
                let class_list: Vec<&str> = classes.split_whitespace().collect();

                // Only process if has meaningful class names
                if class_list.is_empty() {
                    continue;
                }

                // Extract heading
                let heading = if let Ok(h_selector) = Selector::parse("h1, h2, h3, h4") {
                    el.select(&h_selector)
                        .next()
                        .map(|h| h.text().collect::<String>().trim().to_string())
                } else {
                    None
                };

                // Extract text preview
                let text: String = el.text().collect();
                let text = text.trim();
                if text.len() < 20 {
                    continue; // Skip very short blocks
                }

                let preview = if text.len() > 200 {
                    Some(format!("{}...", truncate_utf8(text, 200)))
                } else {
                    Some(text.to_string())
                };

                let content_type = self.detect_block_content_type(&el);
                let selector_str = if !class_list.is_empty() {
                    format!(".{}", class_list[0])
                } else {
                    el.value().name().to_string()
                };

                blocks.push(ContentBlock {
                    id: class_list[0].to_string(),
                    name: heading.clone().unwrap_or_else(|| "Content Block".to_string()),
                    description: heading.clone(),
                    selector: selector_str,
                    content_type,
                    heading,
                    preview,
                });
            }
        }

        debug!("Extracted {} content blocks", blocks.len());
        blocks
    }

    fn detect_block_content_type(&self, element: &scraper::ElementRef) -> String {
        let html = element.html().to_lowercase();

        // Check for various content patterns
        if html.contains("<ul") || html.contains("<ol") {
            "list".to_string()
        } else if html.contains("<table") {
            "table".to_string()
        } else if html.contains("<img") || html.contains("<figure") {
            "media".to_string()
        } else if html.contains("<article") || html.contains("article") {
            "article".to_string()
        } else if html.contains("news") {
            "news".to_string()
        } else if html.contains("<form") {
            "form".to_string()
        } else {
            "text".to_string()
        }
    }

    fn get_html_snippet(html: &str, max_len: usize) -> String {
        if html.len() <= max_len {
            return html.to_string();
        }

        // Try to cut at a reasonable point
        let snippet = truncate_utf8(html, max_len);
        if let Some(last_close) = snippet.rfind('>') {
            snippet[..=last_close].to_string()
        } else {
            snippet.to_string()
        }
    }
}

fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}

impl Default for Crawler {
    fn default() -> Self {
        Self::new().expect("Failed to create crawler")
    }
}

#[cfg(test)]
mod tests {
    use super::{truncate_utf8, Crawler, CrawlerConfig};
    use std::time::Duration;

    #[test]
    fn truncate_utf8_handles_non_char_boundary() {
        let s = "你好世界"; // 12 bytes
        assert_eq!(truncate_utf8(s, 4), "你");
    }

    #[test]
    fn get_html_snippet_handles_unicode_without_panic() {
        let html = "<div>你好世界</div>";
        let snippet = Crawler::get_html_snippet(html, 10);
        assert!(!snippet.is_empty());
        assert!(snippet.starts_with("<div>"));
    }

    #[test]
    fn crawler_config_default_values() {
        let config = CrawlerConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_base_delay, Duration::from_secs(1));
        assert_eq!(config.retry_max_delay, Duration::from_secs(10));
    }

    #[test]
    fn crawler_with_custom_config() {
        let config = CrawlerConfig {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(15),
            max_retries: 5,
            retry_base_delay: Duration::from_millis(500),
            retry_max_delay: Duration::from_secs(5),
        };
        let crawler = Crawler::with_config(config).unwrap();
        assert_eq!(crawler.config.max_retries, 5);
    }

    #[test]
    fn exponential_backoff_calculation() {
        // Test that backoff doubles each time but caps at max
        let config = CrawlerConfig {
            retry_base_delay: Duration::from_secs(1),
            retry_max_delay: Duration::from_secs(10),
            ..Default::default()
        };

        // Attempt 1: 1 * 2^0 = 1s
        let delay1 = std::cmp::min(
            config.retry_base_delay * 2u32.saturating_pow(0),
            config.retry_max_delay,
        );
        assert_eq!(delay1, Duration::from_secs(1));

        // Attempt 2: 1 * 2^1 = 2s
        let delay2 = std::cmp::min(
            config.retry_base_delay * 2u32.saturating_pow(1),
            config.retry_max_delay,
        );
        assert_eq!(delay2, Duration::from_secs(2));

        // Attempt 3: 1 * 2^2 = 4s
        let delay3 = std::cmp::min(
            config.retry_base_delay * 2u32.saturating_pow(2),
            config.retry_max_delay,
        );
        assert_eq!(delay3, Duration::from_secs(4));

        // Attempt 4: 1 * 2^3 = 8s
        let delay4 = std::cmp::min(
            config.retry_base_delay * 2u32.saturating_pow(3),
            config.retry_max_delay,
        );
        assert_eq!(delay4, Duration::from_secs(8));

        // Attempt 5: 1 * 2^4 = 16s, but capped at 10s
        let delay5 = std::cmp::min(
            config.retry_base_delay * 2u32.saturating_pow(4),
            config.retry_max_delay,
        );
        assert_eq!(delay5, Duration::from_secs(10));
    }
}
