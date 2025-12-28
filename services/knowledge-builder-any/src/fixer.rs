//! Handbook fixer module
//!
//! Automatically fixes quality issues in generated handbooks using AI

use crate::analyzer::Analyzer;
use crate::error::Result;
use crate::handbook::{HandbookOutput, WebContext};
use crate::validator::{ValidationIssue, ValidationResult};
use tracing::info;

/// Handbook fixer that uses AI to improve quality
pub struct Fixer {
    analyzer: Analyzer,
    max_attempts: usize,
}

impl Fixer {
    /// Create a new fixer with default settings
    pub fn new() -> Self {
        Self {
            analyzer: Analyzer::new(),
            max_attempts: 3,
        }
    }

    /// Create a fixer with custom max attempts
    pub fn with_max_attempts(max_attempts: usize) -> Self {
        Self {
            analyzer: Analyzer::new(),
            max_attempts,
        }
    }

    /// Attempt to fix handbook issues
    pub async fn fix(
        &self,
        _handbook: HandbookOutput,
        context: &WebContext,
        validation: &ValidationResult,
        attempt: usize,
    ) -> Result<HandbookOutput> {
        info!(
            "Attempt {}/{}: Fixing {} issue(s) in handbook",
            attempt,
            self.max_attempts,
            validation.issues.len()
        );

        // Build a targeted fix prompt based on issues
        let fix_prompt = self.build_fix_prompt(context, validation);

        // Use analyzer to regenerate handbook with fixes
        let fixed_handbook = self.analyzer.analyze_with_prompt(context, &fix_prompt).await?;

        Ok(fixed_handbook)
    }

    fn build_fix_prompt(&self, context: &WebContext, validation: &ValidationResult) -> String {
        let mut prompt = String::from(
            r#"You are a web automation expert. The previous handbook generation had quality issues.
Please regenerate an improved handbook that addresses the following problems:

"#,
        );

        // Group issues by category
        let critical_issues: Vec<&ValidationIssue> = validation
            .issues
            .iter()
            .filter(|i| matches!(i.severity, crate::validator::IssueSeverity::Critical))
            .collect();

        let major_issues: Vec<&ValidationIssue> = validation
            .issues
            .iter()
            .filter(|i| matches!(i.severity, crate::validator::IssueSeverity::Major))
            .collect();

        // Add critical issues
        if !critical_issues.is_empty() {
            prompt.push_str("## CRITICAL ISSUES TO FIX:\n\n");
            for (i, issue) in critical_issues.iter().enumerate() {
                prompt.push_str(&format!(
                    "{}. **{}**: {}\n",
                    i + 1,
                    match issue.category {
                        crate::validator::IssueCategory::MissingContent => "Missing Content",
                        crate::validator::IssueCategory::InsufficientDetail => "Insufficient Detail",
                        crate::validator::IssueCategory::InvalidStructure => "Invalid Structure",
                        crate::validator::IssueCategory::WrongFocus => "Wrong Focus",
                    },
                    issue.description
                ));

                if let Some(suggestion) = &issue.suggestion {
                    prompt.push_str(&format!("   → FIX: {}\n", suggestion));
                }
                prompt.push('\n');
            }
        }

        // Add major issues
        if !major_issues.is_empty() {
            prompt.push_str("## MAJOR ISSUES TO FIX:\n\n");
            for (i, issue) in major_issues.iter().enumerate() {
                prompt.push_str(&format!(
                    "{}. **{}**: {}\n",
                    i + 1,
                    match issue.category {
                        crate::validator::IssueCategory::MissingContent => "Missing Content",
                        crate::validator::IssueCategory::InsufficientDetail => "Insufficient Detail",
                        crate::validator::IssueCategory::InvalidStructure => "Invalid Structure",
                        crate::validator::IssueCategory::WrongFocus => "Wrong Focus",
                    },
                    issue.description
                ));

                if let Some(suggestion) = &issue.suggestion {
                    prompt.push_str(&format!("   → FIX: {}\n", suggestion));
                }
                prompt.push('\n');
            }
        }

        // Add specific requirements based on content blocks
        if !context.content_blocks.is_empty() {
            prompt.push_str(&format!(
                r#"
## SPECIAL ATTENTION REQUIRED:

This page has {} content-rich sections that MUST have extraction actions:

"#,
                context.content_blocks.len()
            ));

            for block in context.content_blocks.iter().take(5) {
                prompt.push_str(&format!(
                    "- **{}** (selector: `{}`): {}\n",
                    block.name,
                    block.selector,
                    block.description.as_deref().unwrap_or("Content block")
                ));
            }

            prompt.push_str(
                r#"
You MUST generate extraction actions for these content blocks with:
1. Specific selector from the list above
2. Step-by-step extraction instructions
3. Clear description of what data to extract

Example format:
```
Action: Extract [Block Name] Content
Description: Retrieve [specific data] from the [block name] section
Element: [Block name] section
Location: [Description] - Selector: [exact selector from above]
Steps:
1. Locate section with selector '[exact selector]'
2. Extract [specific field] from heading
3. Extract [specific field] from content
4. Extract [specific field] links
5. Return structured data
```

"#,
            );
        }

        // Add general guidance
        prompt.push_str(
            r#"
## REQUIREMENTS FOR REGENERATION:

1. **Completeness**: Include ALL required sections (title, intro, elements, actions, best_practices, error_handling)
2. **Detail**: Every action must have detailed step-by-step instructions (3-7 steps minimum)
3. **Specificity**: Use specific CSS selectors (IDs, classes) not generic tags
4. **Balance**: Include BOTH interaction actions (click, search) AND content extraction actions (extract, read, parse)
5. **Format**: Return ONLY valid JSON matching the expected structure

Regenerate the complete handbook JSON now with all fixes applied:
"#,
        );

        prompt
    }
}

impl Default for Fixer {
    fn default() -> Self {
        Self::new()
    }
}
