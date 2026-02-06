use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use colored::Colorize;
use dialoguer::{MultiSelect, Select};

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use super::templates;
use crate::cli::{Cli, SetupTarget};
use crate::error::{ActionbookError, Result};

/// Result of generating a single integration file
#[derive(Debug)]
pub struct TargetResult {
    pub target: SetupTarget,
    pub path: PathBuf,
    pub status: FileStatus,
}

#[derive(Debug, PartialEq)]
pub enum FileStatus {
    Created,
    Updated,
    Skipped,
    AlreadyUpToDate,
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Created => write!(f, "created"),
            FileStatus::Updated => write!(f, "updated (backup saved)"),
            FileStatus::Skipped => write!(f, "skipped"),
            FileStatus::AlreadyUpToDate => write!(f, "already up to date"),
        }
    }
}

/// Interactively select usage modes, with auto-detection pre-selecting items.
pub fn select_modes(
    cli: &Cli,
    env: &EnvironmentInfo,
    mode_flag: Option<&[SetupTarget]>,
    non_interactive: bool,
) -> Result<Vec<SetupTarget>> {
    // If modes provided via flag, use them directly
    if let Some(modes) = mode_flag {
        let targets = expand_targets(modes);
        if cli.json {
            let names: Vec<&str> = targets.iter().map(target_name).collect();
            println!(
                "{}",
                serde_json::json!({
                    "step": "modes",
                    "selected": names,
                })
            );
        } else {
            for t in &targets {
                println!("  {} Mode: {}", "✓".green(), target_name(t));
            }
        }
        return Ok(targets);
    }

    if non_interactive {
        // Auto-select based on detected environment
        let mut targets = Vec::new();
        if env.claude_code {
            targets.push(SetupTarget::Claude);
        }
        if env.cursor {
            targets.push(SetupTarget::Cursor);
        }
        if env.codex {
            targets.push(SetupTarget::Codex);
        }
        if targets.is_empty() {
            // Default to standalone if nothing detected
            targets.push(SetupTarget::Standalone);
        }

        if cli.json {
            let names: Vec<&str> = targets.iter().map(target_name).collect();
            println!(
                "{}",
                serde_json::json!({
                    "step": "modes",
                    "selected": names,
                    "auto_detected": true,
                })
            );
        } else {
            for t in &targets {
                println!("  {} Mode: {} (auto-detected)", "✓".green(), target_name(t));
            }
        }
        return Ok(targets);
    }

    // Interactive multi-select
    let items = vec![
        format!(
            "Claude Code{}",
            if env.claude_code {
                "  (detected ✓)".to_string()
            } else {
                String::new()
            }
        ),
        format!(
            "Cursor{}",
            if env.cursor {
                "  (detected ✓)".to_string()
            } else {
                String::new()
            }
        ),
        format!(
            "Codex (OpenAI){}",
            if env.codex {
                "  (detected ✓)".to_string()
            } else {
                String::new()
            }
        ),
        "Standalone CLI".to_string(),
    ];

    // Pre-select detected tools
    let defaults: Vec<bool> = vec![env.claude_code, env.cursor, env.codex, false];

    let selections = MultiSelect::with_theme(&setup_theme())
        .with_prompt("How will you use Actionbook? (Space to toggle, Enter to confirm)")
        .items(&items)
        .defaults(&defaults)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    let all_targets = [
        SetupTarget::Claude,
        SetupTarget::Cursor,
        SetupTarget::Codex,
        SetupTarget::Standalone,
    ];

    let targets: Vec<SetupTarget> = selections.iter().map(|&i| all_targets[i]).collect();

    if targets.is_empty() {
        if !cli.json {
            println!(
                "  {} No modes selected, skipping integration files.",
                "!".yellow()
            );
        }
    } else if cli.json {
        let names: Vec<&str> = targets.iter().map(target_name).collect();
        println!(
            "{}",
            serde_json::json!({
                "step": "modes",
                "selected": names,
            })
        );
    } else {
        for t in &targets {
            println!("  {} Mode: {}", "✓".green(), target_name(t));
        }
    }

    Ok(targets)
}

/// Generate integration files for the selected targets.
///
/// Idempotent behavior:
/// - File doesn't exist → create (auto-create dirs)
/// - File exists + same content → skip ("already up to date")
/// - File exists + different + --force → backup + overwrite
/// - File exists + different + interactive → prompt keep/overwrite
/// - File exists + different + non-interactive → skip
pub fn generate_integration_files(
    cli: &Cli,
    targets: &[SetupTarget],
    force: bool,
    non_interactive: bool,
) -> Result<Vec<TargetResult>> {
    let mut results = Vec::new();

    for target in targets {
        if let Some((path, content)) = target_file_info(target) {
            let result =
                write_file_idempotent(cli, *target, &path, content, force, non_interactive)?;
            results.push(result);
        }
    }

    Ok(results)
}

/// Map a target to its file path and template content.
fn target_file_info(target: &SetupTarget) -> Option<(PathBuf, &'static str)> {
    match target {
        SetupTarget::Claude => Some((
            PathBuf::from(".claude/skills/actionbook/SKILL.md"),
            templates::CLAUDE_SKILL_TEMPLATE,
        )),
        SetupTarget::Codex => Some((
            PathBuf::from(".agents/skills/actionbook/SKILL.md"),
            templates::CODEX_SKILL_TEMPLATE,
        )),
        SetupTarget::Cursor => Some((
            PathBuf::from(".cursor/rules/actionbook.md"),
            templates::CURSOR_RULES_TEMPLATE,
        )),
        SetupTarget::Standalone => None, // No file to generate
        SetupTarget::All => None,        // Expanded before reaching here
    }
}

/// Write a file with idempotent semantics and optional backup.
fn write_file_idempotent(
    cli: &Cli,
    target: SetupTarget,
    rel_path: &Path,
    content: &str,
    force: bool,
    non_interactive: bool,
) -> Result<TargetResult> {
    let path = rel_path.to_path_buf();

    if path.exists() {
        let existing = fs::read_to_string(&path)?;

        if existing == content {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": path.display().to_string(),
                        "status": "already_up_to_date",
                    })
                );
            } else {
                println!("  {} {} (already up to date)", "✓".green(), path.display());
            }
            return Ok(TargetResult {
                target,
                path,
                status: FileStatus::AlreadyUpToDate,
            });
        }

        // Content differs
        if force {
            backup_file(&path)?;
            write_with_dirs(&path, content)?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": path.display().to_string(),
                        "status": "updated",
                    })
                );
            } else {
                println!(
                    "  {} {} (updated, backup saved)",
                    "✓".green(),
                    path.display()
                );
            }
            return Ok(TargetResult {
                target,
                path,
                status: FileStatus::Updated,
            });
        }

        if non_interactive {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": path.display().to_string(),
                        "status": "skipped",
                        "reason": "file_exists_different_content",
                    })
                );
            } else {
                println!(
                    "  {} {} (skipped, file exists with different content)",
                    "○".dimmed(),
                    path.display()
                );
            }
            return Ok(TargetResult {
                target,
                path,
                status: FileStatus::Skipped,
            });
        }

        // Interactive: ask the user
        let choices = vec!["Keep existing file", "Overwrite (backup saved)"];
        let selection = Select::with_theme(&setup_theme())
            .with_prompt(format!("{} already exists. What to do?", path.display()))
            .items(&choices)
            .default(0)
            .report(false)
            .interact()
            .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

        if selection == 0 {
            if !cli.json {
                println!("  {} {} (kept)", "✓".green(), path.display());
            }
            return Ok(TargetResult {
                target,
                path,
                status: FileStatus::Skipped,
            });
        }

        backup_file(&path)?;
        write_with_dirs(&path, content)?;

        if !cli.json {
            println!(
                "  {} {} (updated, backup saved)",
                "✓".green(),
                path.display()
            );
        }
        return Ok(TargetResult {
            target,
            path,
            status: FileStatus::Updated,
        });
    }

    // File doesn't exist: create
    write_with_dirs(&path, content)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "file": path.display().to_string(),
                "status": "created",
            })
        );
    } else {
        println!("  {} {} (created)", "✓".green(), path.display());
    }

    Ok(TargetResult {
        target,
        path,
        status: FileStatus::Created,
    })
}

/// Create parent directories and write content to a file.
fn write_with_dirs(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

/// Create a backup of the file with a timestamp suffix.
fn backup_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_name = format!("{}.bak.{}", path.display(), timestamp);
    fs::copy(path, &backup_name)?;
    Ok(())
}

/// Expand `All` target into individual targets (excluding Standalone),
/// deduplicating while preserving order.
fn expand_targets(targets: &[SetupTarget]) -> Vec<SetupTarget> {
    let mut result = Vec::new();
    for t in targets {
        match t {
            SetupTarget::All => {
                for expanded in [SetupTarget::Claude, SetupTarget::Cursor, SetupTarget::Codex] {
                    if !result.contains(&expanded) {
                        result.push(expanded);
                    }
                }
            }
            other => {
                if !result.contains(other) {
                    result.push(*other);
                }
            }
        }
    }
    result
}

fn target_name(t: &SetupTarget) -> &'static str {
    match t {
        SetupTarget::Claude => "Claude Code",
        SetupTarget::Cursor => "Cursor",
        SetupTarget::Codex => "Codex",
        SetupTarget::Standalone => "Standalone CLI",
        SetupTarget::All => "All",
    }
}

/// Get a human-readable name for a target (public for mod.rs).
pub fn target_display_name(t: &SetupTarget) -> &'static str {
    target_name(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_expand_targets_all() {
        let targets = expand_targets(&[SetupTarget::All]);
        assert_eq!(targets.len(), 3);
        assert!(targets.contains(&SetupTarget::Claude));
        assert!(targets.contains(&SetupTarget::Cursor));
        assert!(targets.contains(&SetupTarget::Codex));
    }

    #[test]
    fn test_expand_targets_dedup() {
        let targets = expand_targets(&[SetupTarget::All, SetupTarget::Codex]);
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], SetupTarget::Claude);
        assert_eq!(targets[1], SetupTarget::Cursor);
        assert_eq!(targets[2], SetupTarget::Codex);
    }

    #[test]
    fn test_expand_targets_individual() {
        let targets = expand_targets(&[SetupTarget::Claude, SetupTarget::Standalone]);
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&SetupTarget::Claude));
        assert!(targets.contains(&SetupTarget::Standalone));
    }

    #[test]
    fn test_target_file_info_claude() {
        let info = target_file_info(&SetupTarget::Claude);
        assert!(info.is_some());
        let (path, content) = info.unwrap();
        assert!(path.ends_with("SKILL.md"));
        assert!(!content.is_empty());
    }

    #[test]
    fn test_target_file_info_standalone_none() {
        assert!(target_file_info(&SetupTarget::Standalone).is_none());
    }

    #[test]
    fn test_write_with_dirs_creates_parents() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("a/b/c/test.md");
        write_with_dirs(&file_path, "hello").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "hello");
    }

    #[test]
    fn test_backup_file_creates_bak() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.md");
        fs::write(&file_path, "original").unwrap();
        backup_file(&file_path).unwrap();

        // Check that a .bak file exists
        let entries: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            entries.len() >= 2,
            "Expected at least 2 files (original + backup)"
        );
        let has_backup = entries
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains(".bak."));
        assert!(has_backup, "Expected a .bak file");
    }

    #[test]
    fn test_backup_nonexistent_file_noop() {
        let result = backup_file(Path::new("/nonexistent/file.md"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_target_name() {
        assert_eq!(target_name(&SetupTarget::Claude), "Claude Code");
        assert_eq!(target_name(&SetupTarget::Cursor), "Cursor");
        assert_eq!(target_name(&SetupTarget::Codex), "Codex");
        assert_eq!(target_name(&SetupTarget::Standalone), "Standalone CLI");
    }
}
