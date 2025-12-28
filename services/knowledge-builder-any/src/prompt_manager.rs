//! Prompt management module
//!
//! Manages site-specific prompt files for handbook generation quality control

use crate::error::{HandbookError, Result};
use crate::handbook::{sanitize_folder_name, WebContext};
use std::path::{Path, PathBuf};
use tracing::info;

/// Prompt manager for handling site-specific generation prompts
pub struct PromptManager {
    base_dir: PathBuf,
}

impl PromptManager {
    /// Create a new prompt manager with default base directory
    pub fn new() -> Self {
        Self {
            base_dir: PathBuf::from("./handbooks"),
        }
    }

    /// Create a prompt manager with custom base directory
    pub fn with_base_dir<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Check if a prompt file exists for the given site
    pub fn prompt_exists(&self, site_name: &str) -> bool {
        let prompt_path = self.get_prompt_path(site_name);
        prompt_path.exists()
    }

    /// Get the path to the prompt file for a site
    pub fn get_prompt_path(&self, site_name: &str) -> PathBuf {
        self.base_dir
            .join(sanitize_folder_name(site_name))
            .join("prompt.md")
    }

    /// Load existing prompt for a site
    pub fn load_prompt(&self, site_name: &str) -> Result<String> {
        let prompt_path = self.get_prompt_path(site_name);

        if !prompt_path.exists() {
            return Err(HandbookError::PromptNotFound(site_name.to_string()));
        }

        info!("Loading existing prompt from: {}", prompt_path.display());
        let prompt = std::fs::read_to_string(&prompt_path).map_err(|e| {
            HandbookError::IoError(format!("Failed to read prompt file: {}", e))
        })?;

        Ok(prompt)
    }

    /// Generate and save initial prompt for a new site
    pub fn generate_initial_prompt(&self, site_name: &str, context: &WebContext) -> String {
        info!("Generating initial prompt for site: {}", site_name);

        let prompt = self.build_initial_prompt_content(context);
        prompt
    }

    /// Save prompt to file
    pub fn save_prompt(&self, site_name: &str, prompt: &str) -> Result<()> {
        let prompt_path = self.get_prompt_path(site_name);

        // Ensure directory exists
        if let Some(parent) = prompt_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                HandbookError::IoError(format!("Failed to create directory: {}", e))
            })?;
        }

        info!("Saving prompt to: {}", prompt_path.display());
        std::fs::write(&prompt_path, prompt).map_err(|e| {
            HandbookError::IoError(format!("Failed to write prompt file: {}", e))
        })?;

        info!("✓ Prompt saved. Users can edit this file to customize handbook generation.");
        Ok(())
    }

    /// Build initial prompt content based on website characteristics
    fn build_initial_prompt_content(&self, context: &WebContext) -> String {
        let has_content_blocks = !context.content_blocks.is_empty();
        let has_interactive = !context.interactive_elements.is_empty();

        let mut prompt = String::from(
            r#"# Handbook Generation Prompt

This file controls how the AI generates handbooks for this website.
**Users can edit this file to customize handbook quality and focus.**

---

## Website Characteristics

"#,
        );

        // Add detected characteristics
        prompt.push_str(&format!("- **URL**: {}\n", context.base_url));
        prompt.push_str(&format!("- **Title**: {}\n", context.title));
        prompt.push_str(&format!("- **Site Type**: {}\n", context.site_type));
        prompt.push_str(&format!(
            "- **Interactive Elements**: {} found\n",
            context.interactive_elements.len()
        ));
        prompt.push_str(&format!(
            "- **Content Blocks**: {} found\n",
            context.content_blocks.len()
        ));
        prompt.push('\n');

        // Add generation guidelines based on site characteristics
        prompt.push_str("## Generation Guidelines\n\n");

        if has_content_blocks && has_interactive {
            prompt.push_str(
                r#"This is a **mixed-type website** with both interactive features and content sections.

### Requirements:
1. **MUST include content extraction actions**
   - Generate specific actions for extracting information from content blocks
   - Each extraction action should reference exact selectors
   - Include step-by-step extraction instructions

2. **MUST include interaction actions**
   - Cover search, navigation, filtering operations
   - Document form submissions and button clicks
   - Include state changes and user flows

3. **Balance**: Aim for roughly equal numbers of extraction and interaction actions
"#,
            );
        } else if has_content_blocks {
            prompt.push_str(
                r#"This is a **content-focused website** (blog, news, wiki, documentation).

### Requirements:
1. **PRIMARY FOCUS: Content extraction**
   - Most actions should be about extracting information
   - Document how to extract articles, lists, metadata
   - Include selectors for content-rich sections

2. **Secondary: Navigation**
   - Cover basic navigation and search
   - Keep interaction actions minimal
"#,
            );
        } else {
            prompt.push_str(
                r#"This is an **interaction-focused website** (SaaS, app, form).

### Requirements:
1. **PRIMARY FOCUS: User interactions**
   - Document all interactive elements (buttons, inputs, dropdowns)
   - Cover complete user workflows
   - Include form submissions and state changes

2. **Navigation**: Document main navigation paths
"#,
            );
        }

        // Add quality standards
        prompt.push_str(
            r#"
## Quality Standards

### Actions
- **Minimum**: 5 actions (adjust based on page complexity)
- **Naming**: Use clear, specific names (e.g., "Extract Featured Article" not "Get Content")
- **Steps**: Each action must have 3-7 detailed steps
- **Selectors**: Include specific CSS selectors in steps (prefer #id > .class > tag)

### Best Practices
- **Minimum**: 3 best practices
- Focus on timing, error handling, and edge cases
- Provide actionable advice for AI agents

### Error Handling
- **Minimum**: 2 error scenarios
- Cover common failure cases
- Provide concrete solutions

"#,
        );

        // Add site-specific notes if content blocks exist
        if !context.content_blocks.is_empty() {
            prompt.push_str("## Important Content Blocks\n\n");
            prompt.push_str(
                "The following content sections were detected and MUST be covered in extraction actions:\n\n",
            );

            for block in context.content_blocks.iter().take(10) {
                prompt.push_str(&format!(
                    "- **{}** (`{}`): {}\n",
                    block.name,
                    block.selector,
                    block.description.as_deref().unwrap_or("Content section")
                ));
            }
            prompt.push('\n');
        }

        // Add customization guide
        prompt.push_str(
            r#"---

## How to Customize This Prompt

Users can edit any section above to improve handbook quality:

1. **Add specific requirements**:
   ```
   - MUST include action for extracting user profiles
   - MUST document pagination workflow
   ```

2. **Adjust quality standards**:
   ```
   - Minimum: 10 actions (for complex sites)
   - Steps: Each action must have 5-10 steps (for detailed guidance)
   ```

3. **Add site-specific notes**:
   ```
   - This site requires JavaScript, note async loading in best practices
   - Login is required, document authentication flow first
   ```

4. **Specify focus areas**:
   ```
   - Focus on admin panel operations
   - Prioritize data export actions
   ```

After editing, regenerate the handbook with:
```bash
cargo run --release -- build --url "<URL>" --name "<site_name>"
```

The AI will read this prompt and generate an improved handbook following your guidelines.
"#,
        );

        prompt
    }
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new()
    }
}
