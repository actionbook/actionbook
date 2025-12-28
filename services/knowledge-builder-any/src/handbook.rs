//! Handbook data structures and markdown generation
//!
//! This module defines the data structures for generating handbook documentation
//! that follows the standard format with action.md and overview.md files.

use serde::{Deserialize, Serialize};

/// A single action that can be performed on the page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action name/title
    pub name: String,
    /// What this action does
    pub description: String,
    /// Target element description
    pub element: Option<String>,
    /// Where the element is located
    pub location: Option<String>,
    /// Step-by-step instructions
    pub steps: Vec<String>,
}

/// Page element description (e.g., company cards)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageElement {
    /// Element name
    pub name: String,
    /// Element description
    pub description: String,
    /// States the element can be in (collapsed, expanded, etc.)
    pub states: Vec<ElementState>,
    /// Interactions available
    pub interactions: Vec<String>,
}

/// Element state description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementState {
    /// State name (e.g., "collapsed", "expanded")
    pub name: String,
    /// What's visible in this state
    pub visible_content: Vec<String>,
}

/// Best practice for AI agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestPractice {
    /// Practice title
    pub title: String,
    /// Practice description
    pub description: String,
}

/// Error handling scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorScenario {
    /// Scenario description
    pub scenario: String,
    /// Recommended solution
    pub solution: String,
}

/// Action handbook (action.md) - describes how to interact with the page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHandbook {
    /// Page/Site title
    pub title: String,
    /// Brief intro description
    pub intro: String,
    /// Key page elements and their states
    pub elements: Vec<PageElement>,
    /// Common actions that can be performed
    pub actions: Vec<Action>,
    /// Best practices for AI agents
    pub best_practices: Vec<BestPractice>,
    /// Error handling scenarios
    pub error_handling: Vec<ErrorScenario>,
}

impl ActionHandbook {
    /// Convert to markdown format matching the standard action.md template
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        // Title
        md.push_str(&format!("# {}\n\n", self.title));
        md.push_str(&format!("{}\n\n", self.intro));

        // Page Elements
        for element in &self.elements {
            md.push_str(&format!("## {}\n\n", element.name));
            md.push_str(&format!("{}\n\n", element.description));

            for state in &element.states {
                md.push_str(&format!("In the **{}** state:\n\n", state.name));
                for (i, content) in state.visible_content.iter().enumerate() {
                    md.push_str(&format!("{}. {}\n", i + 1, content));
                }
                md.push('\n');
            }

            if !element.interactions.is_empty() {
                md.push_str("### Interactions\n\n");
                for interaction in &element.interactions {
                    md.push_str(&format!("- {}\n", interaction));
                }
                md.push('\n');
            }
        }

        // Common Actions
        md.push_str("## Common Actions\n\n");
        for (i, action) in self.actions.iter().enumerate() {
            md.push_str(&format!("### {}. {}\n\n", i + 1, action.name));
            md.push_str("```\n");
            md.push_str(&format!("Action: {}\n", action.description));
            if let Some(element) = &action.element {
                md.push_str(&format!("Element: {}\n", element));
            }
            if let Some(location) = &action.location {
                md.push_str(&format!("Location: {}\n", location));
            }
            md.push_str("Steps:\n");
            for (j, step) in action.steps.iter().enumerate() {
                md.push_str(&format!("{}. {}\n", j + 1, step));
            }
            md.push_str("```\n\n");
        }

        // Best Practices
        if !self.best_practices.is_empty() {
            md.push_str("## Best Practices for AI Agents\n\n");
            for (i, practice) in self.best_practices.iter().enumerate() {
                md.push_str(&format!(
                    "{}. **{}** - {}\n",
                    i + 1,
                    practice.title,
                    practice.description
                ));
            }
            md.push('\n');
        }

        // Error Handling
        if !self.error_handling.is_empty() {
            md.push_str("## Error Handling\n\n");
            md.push_str("| Scenario | Solution |\n");
            md.push_str("|----------|----------|\n");
            for error in &self.error_handling {
                md.push_str(&format!("| {} | {} |\n", error.scenario, error.solution));
            }
            md.push('\n');
        }

        md
    }
}

/// Filter/category information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCategory {
    /// Category name/label
    pub name: String,
    /// URL parameter value
    pub url_param: Option<String>,
    /// Description of what this category contains
    pub description: String,
}

/// Navigation link info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationItem {
    /// Link text
    pub text: String,
    /// Link destination
    pub href: String,
}

/// Overview document (overview.md) - provides context about the page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewDoc {
    /// Page/Site title
    pub title: String,
    /// Base URL
    pub url: String,
    /// Page overview description
    pub overview: String,
    /// Key features of the page
    pub features: Vec<String>,
    /// Important notes (e.g., URL requirements)
    pub important_notes: Vec<String>,
    /// URL patterns for different views
    pub url_patterns: Vec<FilterCategory>,
    /// Main navigation structure
    pub navigation: Vec<NavigationItem>,
    /// Filter categories available
    pub filter_categories: Vec<FilterCategory>,
}

impl OverviewDoc {
    /// Convert to markdown format matching the standard overview.md template
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        // Title
        md.push_str(&format!("# {}\n\n", self.title));
        md.push_str(&format!(
            "This document provides an overview of {} at {}.\n\n",
            self.title, self.url
        ));

        // Page Overview
        md.push_str("## Page Overview\n\n");
        md.push_str(&format!("{}\n\n", self.overview));

        // Features
        if !self.features.is_empty() {
            md.push_str("The page features:\n\n");
            for feature in &self.features {
                md.push_str(&format!("- {}\n", feature));
            }
            md.push('\n');
        }

        // Important Notes
        if !self.important_notes.is_empty() {
            for note in &self.important_notes {
                md.push_str(&format!("**Important**: {}\n\n", note));
            }
        }

        // URL Patterns
        if !self.url_patterns.is_empty() {
            md.push_str("| Button Label | URL Parameter |\n");
            md.push_str("|-------------|---------------|\n");
            for pattern in &self.url_patterns {
                if let Some(param) = &pattern.url_param {
                    md.push_str(&format!("| {} | `{}` |\n", pattern.name, param));
                }
            }
            md.push('\n');
        }

        // Page Structure
        md.push_str("## Page Structure\n\n");

        // Navigation
        if !self.navigation.is_empty() {
            md.push_str("### Main Navigation\n\n");
            md.push_str("The top navigation bar contains links to:\n");
            for nav in &self.navigation {
                md.push_str(&format!("- {}\n", nav.text));
            }
            md.push('\n');
        }

        // Filter Categories
        if !self.filter_categories.is_empty() {
            md.push_str("### Filter Categories\n\n");
            md.push_str("The page provides filtering capabilities through category tabs:\n\n");
            md.push_str("| Category | Description |\n");
            md.push_str("|----------|-------------|\n");
            for category in &self.filter_categories {
                md.push_str(&format!("| {} | {} |\n", category.name, category.description));
            }
            md.push('\n');
        }

        md
    }
}

/// Complete handbook output containing both action.md and overview.md content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandbookOutput {
    /// Site name (used for folder naming)
    pub site_name: String,
    /// Action handbook content
    pub action: ActionHandbook,
    /// Overview document content
    pub overview: OverviewDoc,
}

impl HandbookOutput {
    /// Get the folder name for this handbook (derived from site name)
    pub fn folder_name(&self) -> String {
        sanitize_folder_name(&self.site_name)
    }
}

/// Sanitize a user- or AI-provided site name into a safe folder name.
///
/// This is used for filesystem paths (e.g., `handbooks/{site_name}/`).
pub fn sanitize_folder_name(site_name: &str) -> String {
    let sanitized: String = site_name
        .to_lowercase()
        .replace(' ', "-")
        .replace('.', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect();

    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

/// Website context information extracted from crawling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebContext {
    /// Base URL of the website
    pub base_url: String,
    /// Website title
    pub title: String,
    /// Meta description
    pub meta_description: Option<String>,
    /// Detected site type
    pub site_type: SiteType,
    /// Main navigation links
    pub navigation: Vec<NavLink>,
    /// Interactive elements found
    pub interactive_elements: Vec<InteractiveElement>,
    /// Page sections
    pub sections: Vec<PageSection>,
    /// Content blocks for information extraction
    pub content_blocks: Vec<ContentBlock>,
    /// Raw HTML content (truncated for analysis)
    pub html_snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SiteType {
    /// Company/Portfolio listing
    Listing,
    /// Product/Service detail page
    Detail,
    /// Form/Input page
    Form,
    /// Dashboard/App
    Dashboard,
    /// Blog/Article
    Blog,
    /// Landing page
    Landing,
    /// Unknown type
    Unknown,
}

impl std::fmt::Display for SiteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SiteType::Listing => write!(f, "listing"),
            SiteType::Detail => write!(f, "detail"),
            SiteType::Form => write!(f, "form"),
            SiteType::Dashboard => write!(f, "dashboard"),
            SiteType::Blog => write!(f, "blog"),
            SiteType::Landing => write!(f, "landing"),
            SiteType::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavLink {
    pub text: String,
    pub href: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveElement {
    pub element_type: String, // button, input, select, link, etc.
    pub selector: String,
    pub text: Option<String>,
    pub attributes: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSection {
    pub heading: Option<String>,
    pub content_type: String, // text, list, grid, form, etc.
    pub selector: String,
}

/// Content block for extracting specific information from the page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    /// Unique identifier or class name
    pub id: String,
    /// Human-readable name/title
    pub name: String,
    /// Block description
    pub description: Option<String>,
    /// CSS selector to locate the block
    pub selector: String,
    /// Content type (article, news, recommendation, list, etc.)
    pub content_type: String,
    /// Heading text if found
    pub heading: Option<String>,
    /// Text preview (first 200 chars)
    pub preview: Option<String>,
}
