//! Claude AI analyzer module using cc-sdk

use crate::error::{HandbookError, Result};
use crate::handbook::{
    Action, ActionHandbook, BestPractice, ElementState, ErrorScenario, FilterCategory,
    HandbookOutput, NavigationItem, OverviewDoc, PageElement, WebContext,
};
use cc_sdk::{query, ClaudeCodeOptions};
use futures::StreamExt;
use serde_json::Value;
use tracing::{debug, info};
use url::Url;

/// Analyzer that uses Claude AI to generate handbooks from web context
pub struct Analyzer {
    options: Option<ClaudeCodeOptions>,
}

impl Analyzer {
    /// Create a new analyzer with default options
    pub fn new() -> Self {
        let options = Some(
            ClaudeCodeOptions::builder()
                .max_turns(3)
                .auto_download_cli(true)
                .build()
        );

        Self { options }
    }

    /// Create analyzer with custom options
    pub fn with_options(options: ClaudeCodeOptions) -> Self {
        Self {
            options: Some(options),
        }
    }

    /// Analyze web context and generate a complete handbook output
    pub async fn analyze(&self, context: &WebContext) -> Result<HandbookOutput> {
        info!("Analyzing web context for: {}", context.base_url);

        let prompt = self.build_analysis_prompt(context);
        self.analyze_with_full_prompt(context, &prompt).await
    }

    /// Analyze with custom prompt prefix (used by fixer)
    pub async fn analyze_with_prompt(&self, context: &WebContext, custom_prompt: &str) -> Result<HandbookOutput> {
        info!("Analyzing with custom prompt for: {}", context.base_url);

        // Combine custom prompt with standard context
        let base_prompt = self.build_analysis_prompt(context);
        let full_prompt = format!("{}\n\n---\n\n{}", custom_prompt, base_prompt);

        self.analyze_with_full_prompt(context, &full_prompt).await
    }

    /// Core analysis logic with full prompt
    async fn analyze_with_full_prompt(&self, context: &WebContext, prompt: &str) -> Result<HandbookOutput> {
        debug!("Analysis prompt length: {} chars", prompt.len());

        // Call Claude using cc-sdk - query returns a Stream
        let mut stream = query(prompt.to_string(), self.options.clone())
            .await
            .map_err(|e| HandbookError::ClaudeError(e.to_string()))?;

        // Collect all messages from the stream
        let mut response_text = String::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(message) => match message {
                    cc_sdk::Message::Assistant { message: assistant_msg } => {
                        // Extract text content from Assistant message
                        for content_block in &assistant_msg.content {
                            if let cc_sdk::ContentBlock::Text(text_content) = content_block {
                                response_text.push_str(&text_content.text);
                            }
                        }
                    }
                    cc_sdk::Message::Result { is_error, result, .. } => {
                        if is_error {
                            let err_msg = result.as_deref().unwrap_or("Unknown error");
                            return Err(HandbookError::ClaudeError(format!(
                                "AI task ended with error: {}",
                                err_msg
                            )));
                        }
                        // Normal completion, exit loop
                        break;
                    }
                    _ => {}
                },
                Err(e) => {
                    return Err(HandbookError::ClaudeError(format!("Stream error: {}", e)));
                }
            }
        }

        debug!("Claude response length: {} chars", response_text.len());

        // Parse the response into HandbookOutput
        self.parse_response(&response_text, context)
    }

    fn build_analysis_prompt(&self, context: &WebContext) -> String {
        let interactive_json =
            serde_json::to_string_pretty(&context.interactive_elements).unwrap_or_default();
        let sections_json = serde_json::to_string_pretty(&context.sections).unwrap_or_default();
        let navigation_json =
            serde_json::to_string_pretty(&context.navigation).unwrap_or_default();
        let content_blocks_json =
            serde_json::to_string_pretty(&context.content_blocks).unwrap_or_default();

        format!(
            r#"You are a web automation expert. Analyze the following website and generate TWO documents for AI agents:
1. An **action.md** handbook describing how to interact with the page AND extract content
2. An **overview.md** document providing context about the page

## Website Information

**URL**: {url}
**Title**: {title}
**Description**: {description}
**Detected Page Type**: {page_type}

## Navigation Links
```json
{navigation}
```

## Interactive Elements Found
```json
{interactive}
```

## Content Blocks (for information extraction)
```json
{content_blocks}
```

## Page Sections
```json
{sections}
```

## HTML Snippet (for context)
```html
{html}
```

---

## Your Task

Generate a JSON response with TWO parts: "action" and "overview".

**IMPORTANT**: If there are content_blocks (information-rich sections), include actions for EXTRACTING CONTENT from them, not just navigating.

### action (for action.md)
Describes how to interact with the page elements and perform common actions:
- **title**: Page title with "Actions" suffix (e.g., "First Round Capital - Companies Directory Actions")
- **intro**: Brief intro describing what this document covers
- **elements**: Array of page elements (e.g., cards, buttons) with their states and interactions
- **actions**: Array of common actions users can perform (browse, search, filter, etc.)
- **best_practices**: Tips for AI agents interacting with the page
- **error_handling**: Common error scenarios and solutions

### overview (for overview.md)
Provides context about the page structure:
- **title**: Page title with "Overview" suffix
- **url**: The page URL
- **overview**: Paragraph describing the page purpose
- **features**: Array of key features
- **important_notes**: Array of important notes (e.g., URL requirements)
- **url_patterns**: Array of URL patterns for different views
- **navigation**: Main navigation items
- **filter_categories**: Available filter categories with descriptions

Respond with ONLY valid JSON in this exact format:
```json
{{
  "site_name": "short-site-name",
  "action": {{
    "title": "Site Name - Page Actions",
    "intro": "This document describes how to interact with...",
    "elements": [
      {{
        "name": "Element Name",
        "description": "What this element is",
        "states": [
          {{
            "name": "collapsed",
            "visible_content": ["Item 1", "Item 2"]
          }},
          {{
            "name": "expanded",
            "visible_content": ["Item 1", "Item 2", "Item 3"]
          }}
        ],
        "interactions": ["Click to expand", "Hover for highlight"]
      }}
    ],
    "actions": [
      {{
        "name": "Browse All Items",
        "description": "View all items on the page",
        "element": "Item cards",
        "location": "Main content area",
        "steps": ["Navigate to the page", "Wait for content to load", "Scroll to view all items"]
      }}
    ],
    "best_practices": [
      {{
        "title": "Wait for page load",
        "description": "The page uses dynamic content, wait for main elements to render"
      }}
    ],
    "error_handling": [
      {{
        "scenario": "Content doesn't load",
        "solution": "Refresh the page or check network connection"
      }}
    ]
  }},
  "overview": {{
    "title": "Site Name - Page Overview",
    "url": "https://example.com/page",
    "overview": "This page displays...",
    "features": ["Feature 1", "Feature 2"],
    "important_notes": ["Note about URL parameters or requirements"],
    "url_patterns": [
      {{
        "name": "All",
        "url_param": "?category=all",
        "description": "Show all items"
      }}
    ],
    "navigation": [
      {{
        "text": "Home",
        "href": "/"
      }}
    ],
    "filter_categories": [
      {{
        "name": "All",
        "url_param": "?category=all",
        "description": "Show all items"
      }}
    ]
  }}
}}
```
"#,
            url = context.base_url,
            title = context.title,
            description = context.meta_description.as_deref().unwrap_or("N/A"),
            page_type = context.site_type,
            navigation = navigation_json,
            interactive = interactive_json,
            content_blocks = content_blocks_json,
            sections = sections_json,
            html = truncate_string(&context.html_snippet, 8000),
        )
    }

    fn parse_response(&self, response: &str, context: &WebContext) -> Result<HandbookOutput> {
        // Extract JSON from response (it might be wrapped in markdown code blocks)
        let json_str = extract_json(response);

        let parsed: Value = serde_json::from_str(&json_str).map_err(|e| {
            HandbookError::ParseError(format!(
                "Failed to parse Claude response as JSON: {}. Response: {}",
                e,
                truncate_string(response, 500)
            ))
        })?;

        // Extract site name
        let site_name = parsed["site_name"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| extract_site_name(&context.base_url));

        // Parse action handbook
        let action = self.parse_action_handbook(&parsed["action"], context)?;

        // Parse overview document
        let overview = self.parse_overview_doc(&parsed["overview"], context)?;

        Ok(HandbookOutput {
            site_name,
            action,
            overview,
        })
    }

    fn parse_action_handbook(
        &self,
        value: &Value,
        context: &WebContext,
    ) -> Result<ActionHandbook> {
        let title = value["title"]
            .as_str()
            .unwrap_or(&format!("{} - Actions", context.title))
            .to_string();

        let intro = value["intro"]
            .as_str()
            .unwrap_or("This document describes how to interact with this page.")
            .to_string();

        let elements = value["elements"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|el| PageElement {
                        name: el["name"].as_str().unwrap_or("").to_string(),
                        description: el["description"].as_str().unwrap_or("").to_string(),
                        states: el["states"]
                            .as_array()
                            .map(|states| {
                                states
                                    .iter()
                                    .map(|s| ElementState {
                                        name: s["name"].as_str().unwrap_or("").to_string(),
                                        visible_content: s["visible_content"]
                                            .as_array()
                                            .map(|vc| {
                                                vc.iter()
                                                    .filter_map(|v| {
                                                        v.as_str().map(|s| s.to_string())
                                                    })
                                                    .collect()
                                            })
                                            .unwrap_or_default(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        interactions: el["interactions"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let actions = value["actions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|a| Action {
                        name: a["name"].as_str().unwrap_or("").to_string(),
                        description: a["description"].as_str().unwrap_or("").to_string(),
                        element: a["element"].as_str().map(|s| s.to_string()),
                        location: a["location"].as_str().map(|s| s.to_string()),
                        steps: a["steps"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let best_practices = value["best_practices"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|bp| BestPractice {
                        title: bp["title"].as_str().unwrap_or("").to_string(),
                        description: bp["description"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let error_handling = value["error_handling"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|eh| ErrorScenario {
                        scenario: eh["scenario"].as_str().unwrap_or("").to_string(),
                        solution: eh["solution"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ActionHandbook {
            title,
            intro,
            elements,
            actions,
            best_practices,
            error_handling,
        })
    }

    fn parse_overview_doc(&self, value: &Value, context: &WebContext) -> Result<OverviewDoc> {
        let title = value["title"]
            .as_str()
            .unwrap_or(&format!("{} - Overview", context.title))
            .to_string();

        let url = value["url"]
            .as_str()
            .unwrap_or(&context.base_url)
            .to_string();

        let overview = value["overview"]
            .as_str()
            .unwrap_or("Overview of this page.")
            .to_string();

        let features = value["features"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let important_notes = value["important_notes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let url_patterns = value["url_patterns"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|p| FilterCategory {
                        name: p["name"].as_str().unwrap_or("").to_string(),
                        url_param: p["url_param"].as_str().map(|s| s.to_string()),
                        description: p["description"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let navigation = value["navigation"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|n| NavigationItem {
                        text: n["text"].as_str().unwrap_or("").to_string(),
                        href: n["href"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let filter_categories = value["filter_categories"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|c| FilterCategory {
                        name: c["name"].as_str().unwrap_or("").to_string(),
                        url_param: c["url_param"].as_str().map(|s| s.to_string()),
                        description: c["description"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(OverviewDoc {
            title,
            url,
            overview,
            features,
            important_notes,
            url_patterns,
            navigation,
            filter_categories,
        })
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract site name from URL
fn extract_site_name(url: &str) -> String {
    Url::parse(url)
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

/// Extract JSON from a response that might contain markdown code blocks
fn extract_json(response: &str) -> String {
    // Try to find JSON in code blocks first
    if let Some(start) = response.find("```json") {
        let after_marker = &response[start + 7..];
        if let Some(end) = after_marker.find("```") {
            return after_marker[..end].trim().to_string();
        }
    }

    // Try generic code block
    if let Some(start) = response.find("```") {
        let after_marker = &response[start + 3..];
        let content_start = after_marker.find('\n').unwrap_or(0) + 1;
        let after_newline = &after_marker[content_start..];
        if let Some(end) = after_newline.find("```") {
            return after_newline[..end].trim().to_string();
        }
    }

    // Try to find raw JSON object
    if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            return response[start..=end].to_string();
        }
    }

    response.to_string()
}

/// Truncate a string to a maximum length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }

        if end == 0 {
            "... [truncated]".to_string()
        } else {
            format!("{}... [truncated]", &s[..end])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_string;

    #[test]
    fn truncate_string_respects_utf8_boundaries() {
        let s = "你好世界"; // each char is 3 bytes in UTF-8
        let truncated = truncate_string(s, 4);
        assert!(truncated.starts_with('你'));
        assert!(truncated.contains("[truncated]"));
    }
}
