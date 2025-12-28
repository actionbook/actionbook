//! Handbook validation module
//!
//! Validates generated handbooks and identifies quality issues

use crate::handbook::{HandbookOutput, WebContext};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tracing::{debug, info};

/// Quality issues found in generated handbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Issue severity
    pub severity: IssueSeverity,
    /// Issue category
    pub category: IssueCategory,
    /// Human-readable description
    pub description: String,
    /// Suggested fix
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IssueSeverity {
    /// Critical issue - handbook is unusable
    Critical,
    /// Major issue - handbook quality is poor
    Major,
    /// Minor issue - handbook could be improved
    Minor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IssueCategory {
    /// Missing required content
    MissingContent,
    /// Insufficient detail
    InsufficientDetail,
    /// Invalid selectors or structure
    InvalidStructure,
    /// Wrong focus (e.g., only operations, no content extraction)
    WrongFocus,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the handbook passes validation
    pub is_valid: bool,
    /// Quality score (0-100)
    pub quality_score: u32,
    /// List of issues found
    pub issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    /// Check if handbook needs fixing
    pub fn needs_fix(&self) -> bool {
        !self.is_valid || self.has_critical_issues()
    }

    /// Check if there are critical issues
    pub fn has_critical_issues(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Critical)
    }

    /// Get critical and major issues
    pub fn important_issues(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| {
                issue.severity == IssueSeverity::Critical
                    || issue.severity == IssueSeverity::Major
            })
            .collect()
    }
}

/// Handbook validator
pub struct Validator {
    /// Minimum number of actions required
    min_actions: usize,
    /// Minimum number of content extraction actions (for content sites)
    min_extraction_actions: usize,
    /// Minimum quality score to pass
    min_quality_score: u32,
}

impl Validator {
    /// Create a new validator with default thresholds
    pub fn new() -> Self {
        Self {
            min_actions: 3,
            min_extraction_actions: 2,
            min_quality_score: 60,
        }
    }

    /// Create a validator with custom thresholds
    pub fn with_thresholds(
        min_actions: usize,
        min_extraction_actions: usize,
        min_quality_score: u32,
    ) -> Self {
        Self {
            min_actions,
            min_extraction_actions,
            min_quality_score,
        }
    }

    /// Validate a generated handbook
    pub fn validate(
        &self,
        handbook: &HandbookOutput,
        context: &WebContext,
    ) -> ValidationResult {
        info!("Validating handbook: {}", handbook.site_name);
        let mut issues = Vec::new();

        // Check 1: Basic structure validation
        self.validate_basic_structure(handbook, &mut issues);

        // Check 2: Content completeness
        self.validate_content_completeness(handbook, &mut issues);

        // Check 3: Action quality
        self.validate_action_quality(handbook, context, &mut issues);

        // Check 4: Selector validity
        self.validate_selectors(handbook, context, &mut issues);

        // Check 5: Content extraction focus (for content-rich sites)
        self.validate_content_extraction_focus(handbook, context, &mut issues);

        // Calculate quality score
        let quality_score = self.calculate_quality_score(handbook, &issues);

        let is_valid = quality_score >= self.min_quality_score && !self.has_blockers(&issues);

        debug!(
            "Validation result: {} issues, score: {}",
            issues.len(),
            quality_score
        );

        ValidationResult {
            is_valid,
            quality_score,
            issues,
        }
    }

    fn validate_basic_structure(&self, handbook: &HandbookOutput, issues: &mut Vec<ValidationIssue>) {
        // Check if action handbook is empty
        if handbook.action.title.is_empty() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Critical,
                category: IssueCategory::MissingContent,
                description: "Action handbook title is empty".to_string(),
                suggestion: Some("Regenerate handbook with valid title".to_string()),
            });
        }

        if handbook.action.intro.is_empty() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Major,
                category: IssueCategory::MissingContent,
                description: "Action handbook introduction is empty".to_string(),
                suggestion: Some("Add introduction describing the handbook purpose".to_string()),
            });
        }

        // Check if overview is empty
        if handbook.overview.overview.is_empty() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Major,
                category: IssueCategory::MissingContent,
                description: "Overview document is empty".to_string(),
                suggestion: Some("Add page overview description".to_string()),
            });
        }
    }

    fn validate_content_completeness(
        &self,
        handbook: &HandbookOutput,
        issues: &mut Vec<ValidationIssue>,
    ) {
        // Check minimum number of actions
        if handbook.action.actions.len() < self.min_actions {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Critical,
                category: IssueCategory::InsufficientDetail,
                description: format!(
                    "Too few actions: {} (minimum: {})",
                    handbook.action.actions.len(),
                    self.min_actions
                ),
                suggestion: Some(format!(
                    "Generate at least {} common actions for this page",
                    self.min_actions
                )),
            });
        }

        // Check if actions have steps
        let actions_without_steps = handbook
            .action
            .actions
            .iter()
            .filter(|a| a.steps.is_empty())
            .count();

        if actions_without_steps > 0 {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Major,
                category: IssueCategory::InsufficientDetail,
                description: format!(
                    "{} action(s) missing step-by-step instructions",
                    actions_without_steps
                ),
                suggestion: Some("Add detailed steps for each action".to_string()),
            });
        }

        // Check if best practices exist
        if handbook.action.best_practices.is_empty() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Minor,
                category: IssueCategory::InsufficientDetail,
                description: "No best practices provided".to_string(),
                suggestion: Some("Add best practices for AI agents".to_string()),
            });
        }
    }

    fn validate_action_quality(
        &self,
        handbook: &HandbookOutput,
        _context: &WebContext,
        issues: &mut Vec<ValidationIssue>,
    ) {
        // Check if actions have meaningful names
        let generic_action_names = ["Action 1", "Action 2", "Do something", "Interact"];

        for action in &handbook.action.actions {
            if generic_action_names
                .iter()
                .any(|&name| action.name.contains(name))
            {
                issues.push(ValidationIssue {
                    severity: IssueSeverity::Major,
                    category: IssueCategory::InsufficientDetail,
                    description: format!("Generic action name: '{}'", action.name),
                    suggestion: Some("Use specific, descriptive action names".to_string()),
                });
            }

            // Check if steps are too short
            let short_steps = action.steps.iter().filter(|s| s.len() < 10).count();
            if short_steps > action.steps.len() / 2 {
                issues.push(ValidationIssue {
                    severity: IssueSeverity::Minor,
                    category: IssueCategory::InsufficientDetail,
                    description: format!(
                        "Action '{}' has {} overly brief steps",
                        action.name, short_steps
                    ),
                    suggestion: Some("Provide more detailed step descriptions".to_string()),
                });
            }
        }
    }

    fn validate_selectors(
        &self,
        handbook: &HandbookOutput,
        _context: &WebContext,
        issues: &mut Vec<ValidationIssue>,
    ) {
        fn step_has_selector(step: &str) -> bool {
            static SELECTOR_RE: OnceLock<Regex> = OnceLock::new();
            let selector_re = SELECTOR_RE.get_or_init(|| {
                Regex::new(r"(?i)(selector\s*[:=]|`[^`]{2,}`|#[A-Za-z_][\w-]*|\.[A-Za-z_][\w-]*|\[[^\]]+\])")
                    .expect("invalid selector regex")
            });

            selector_re.is_match(step)
        }

        // Count actions with specific selectors
        let mut actions_with_selectors = 0;

        for action in &handbook.action.actions {
            // Check if steps mention specific selectors
            let has_selector = action.steps.iter().any(|step| step_has_selector(step));

            if has_selector {
                actions_with_selectors += 1;
            }
        }

        // If most actions don't have selectors, it's a problem
        if actions_with_selectors < handbook.action.actions.len() / 2 {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Major,
                category: IssueCategory::InvalidStructure,
                description: format!(
                    "Only {}/{} actions have specific selectors",
                    actions_with_selectors,
                    handbook.action.actions.len()
                ),
                suggestion: Some(
                    "Include specific CSS selectors in action steps for precise element location"
                        .to_string(),
                ),
            });
        }
    }

    fn validate_content_extraction_focus(
        &self,
        handbook: &HandbookOutput,
        context: &WebContext,
        issues: &mut Vec<ValidationIssue>,
    ) {
        // If the site has content blocks, we expect content extraction actions
        if context.content_blocks.is_empty() {
            debug!("No content blocks found, skipping content extraction validation");
            return;
        }

        // Count content extraction actions
        let extraction_keywords = ["extract", "read", "get", "retrieve", "parse", "fetch"];
        let extraction_actions = handbook
            .action
            .actions
            .iter()
            .filter(|a| {
                let name_lower = a.name.to_lowercase();
                extraction_keywords
                    .iter()
                    .any(|&keyword| name_lower.contains(keyword))
            })
            .count();

        info!(
            "Found {} content blocks, {} extraction actions",
            context.content_blocks.len(),
            extraction_actions
        );

        if extraction_actions < self.min_extraction_actions {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Critical,
                category: IssueCategory::WrongFocus,
                description: format!(
                    "Page has {} content blocks but only {} extraction action(s)",
                    context.content_blocks.len(),
                    extraction_actions
                ),
                suggestion: Some(format!(
                    "Add actions for extracting content from blocks: {}",
                    context
                        .content_blocks
                        .iter()
                        .take(3)
                        .map(|b| format!("#{}", b.id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            });
        }

        // Check if extraction actions reference actual content blocks
        let referenced_blocks = handbook
            .action
            .actions
            .iter()
            .filter(|a| {
                let desc = a.description.to_lowercase();
                context
                    .content_blocks
                    .iter()
                    .any(|b| desc.contains(&b.id.to_lowercase()) || desc.contains(&b.name.to_lowercase()))
            })
            .count();

        if referenced_blocks == 0 && !context.content_blocks.is_empty() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Major,
                category: IssueCategory::WrongFocus,
                description: "Extraction actions don't reference actual content blocks".to_string(),
                suggestion: Some(
                    "Reference specific content block IDs in extraction actions".to_string(),
                ),
            });
        }
    }

    fn calculate_quality_score(&self, handbook: &HandbookOutput, issues: &[ValidationIssue]) -> u32 {
        let mut score = 100u32;

        // Deduct points for issues
        for issue in issues {
            let deduction = match issue.severity {
                IssueSeverity::Critical => 20,
                IssueSeverity::Major => 10,
                IssueSeverity::Minor => 3,
            };
            score = score.saturating_sub(deduction);
        }

        // Bonus for comprehensive content
        if handbook.action.actions.len() >= 8 {
            score = score.saturating_add(5);
        }
        if handbook.action.best_practices.len() >= 5 {
            score = score.saturating_add(3);
        }
        if !handbook.action.error_handling.is_empty() {
            score = score.saturating_add(2);
        }

        score
    }

    fn has_blockers(&self, issues: &[ValidationIssue]) -> bool {
        issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Critical)
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}
