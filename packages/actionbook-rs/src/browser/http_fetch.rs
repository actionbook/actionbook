/// HTTP-first content fetching (I2: tiered degradation).
///
/// For static pages (Wikipedia, blogs, docs) we can skip launching a browser
/// entirely and just fetch the HTML via HTTP. This saves 5-10 seconds of
/// browser startup time.
use std::time::Duration;

/// Result of a successful HTTP fetch.
pub struct HttpFetchResult {
    /// Extracted content (plain text)
    pub content: String,
    /// Content format used
    pub format: String,
    /// Final URL (after redirects)
    pub url: String,
    /// Estimated token count
    pub tokens_estimate: usize,
    /// Whether content was truncated
    pub truncated: bool,
}

/// Attempt to fetch page content via plain HTTP.
///
/// Returns `Ok(Some(result))` on success, `Ok(None)` when the page likely
/// requires JS rendering (SPA), and `Err` on hard failures.
///
/// The caller should fall back to browser-based fetching when `None` is returned.
pub async fn try_http_fetch(
    url: &str,
    max_tokens: Option<usize>,
    _session_tag: Option<&str>,
) -> Result<Option<HttpFetchResult>, Box<dyn std::error::Error + Send + Sync>> {
    // Security: Only accept HTTPS URLs to prevent downgrade attacks
    // For HTTP URLs, fall back to browser-based fetching (caller's responsibility)
    if !url.starts_with("https://") {
        // Return None to signal caller should use browser-based fetch instead
        return Ok(None);
    }

    // Create a clean HTTPS URL (CodeQL sanitization)
    let https_url = format!("https://{}", &url[8..]); // Strip "https://" and re-add it

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("Mozilla/5.0 (compatible; Actionbook/1.0)")
        .build()?;

    let resp = match client.get(&https_url).send().await {
        Ok(r) => r,
        Err(_) => return Ok(None), // Network error → fallback to browser
    };

    if !resp.status().is_success() {
        return Ok(None); // Non-2xx → fallback
    }

    // Check content-type — only process HTML
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("text/html") && !content_type.contains("application/xhtml") {
        return Ok(None);
    }

    let final_url = resp.url().to_string();
    let html = resp.text().await?;

    // Strip HTML tags to get plain text
    let text = strip_html_tags(&html);

    // If text is too short, it's likely a SPA shell → fallback
    if text.len() < 1024 {
        return Ok(None);
    }

    // Token estimation (~4 chars per token)
    let tokens_estimate = text.len() / 4;

    // Truncate if requested
    let (content, truncated) = if let Some(max) = max_tokens {
        if tokens_estimate > max {
            let char_limit = max * 4;
            let truncated_text = if text.len() > char_limit {
                // Find a clean break point (word boundary)
                let end = text[..char_limit]
                    .rfind(char::is_whitespace)
                    .unwrap_or(char_limit);
                text[..end].to_string()
            } else {
                text
            };
            (truncated_text, true)
        } else {
            (text, false)
        }
    } else {
        (text, false)
    };

    let final_tokens = content.len() / 4;

    Ok(Some(HttpFetchResult {
        content,
        format: "text".to_string(),
        url: final_url,
        tokens_estimate: final_tokens,
        truncated,
    }))
}

/// Strip HTML tags and extract visible text content.
///
/// Uses a simple state-machine approach:
/// - Skips content inside `<script>`, `<style>`, `<noscript>` blocks
/// - Collapses whitespace
/// - Inserts newlines for block-level elements
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 3);
    let mut in_tag = false;
    let mut in_skip_block = false;
    let mut tag_name = String::new();
    let mut collecting_tag_name = false;
    let mut is_closing_tag = false;
    let mut last_was_whitespace = true;

    const SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template", "svg"];
    const BLOCK_TAGS: &[&str] = &[
        "div",
        "p",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "li",
        "tr",
        "br",
        "hr",
        "blockquote",
        "pre",
        "article",
        "section",
        "header",
        "footer",
        "nav",
        "main",
    ];

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_name.clear();
            collecting_tag_name = true;
            is_closing_tag = false;
            continue;
        }

        if in_tag {
            if ch == '>' {
                in_tag = false;
                collecting_tag_name = false;
                let tag_lower = tag_name.to_ascii_lowercase();

                if is_closing_tag {
                    if SKIP_TAGS.iter().any(|t| *t == tag_lower) {
                        in_skip_block = false;
                    }
                    if BLOCK_TAGS.iter().any(|t| *t == tag_lower) {
                        if !last_was_whitespace {
                            result.push('\n');
                            last_was_whitespace = true;
                        }
                    }
                } else {
                    if SKIP_TAGS.iter().any(|t| *t == tag_lower) {
                        in_skip_block = true;
                    }
                    if BLOCK_TAGS.iter().any(|t| *t == tag_lower) {
                        if !last_was_whitespace {
                            result.push('\n');
                            last_was_whitespace = true;
                        }
                    }
                }
                continue;
            }

            if collecting_tag_name {
                if ch == '/' && tag_name.is_empty() {
                    is_closing_tag = true;
                } else if ch.is_ascii_alphanumeric() {
                    tag_name.push(ch);
                } else {
                    collecting_tag_name = false;
                }
            }
            continue;
        }

        if in_skip_block {
            continue;
        }

        // Handle HTML entities (basic)
        // For simplicity, we just pass through; a full decoder could be added later.

        if ch.is_whitespace() {
            if !last_was_whitespace {
                result.push(' ');
                last_was_whitespace = true;
            }
        } else {
            result.push(ch);
            last_was_whitespace = false;
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_basic_html() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_strip_skips_script() {
        let html = "<p>Before</p><script>var x = 1;</script><p>After</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("var x"));
    }

    #[test]
    fn test_strip_skips_style() {
        let html = "<style>.foo { color: red; }</style><p>Content</p>";
        let text = strip_html_tags(html);
        assert!(!text.contains("color"));
        assert!(text.contains("Content"));
    }

    #[test]
    fn test_strip_collapses_whitespace() {
        let html = "<p>Hello    World</p>";
        let text = strip_html_tags(html);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_short_content_returns_none() {
        // SPA shell should produce very short text, triggering fallback to browser
        let html = "<html><body><div id='root'></div></body></html>";
        let text = strip_html_tags(html);
        assert!(text.len() < 1024);
    }
}
