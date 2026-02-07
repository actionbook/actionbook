use std::process::Command;

use colored::Colorize;
use dialoguer::Select;

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use crate::cli::{Cli, SetupTarget};
use crate::error::{ActionbookError, Result};

const SKILLS_PACKAGE: &str = "actionbook/actionbook";

/// Map a SetupTarget to the skills CLI `-a` agent flag value.
pub fn target_to_agent_flag(target: &SetupTarget) -> Option<&'static str> {
    match target {
        SetupTarget::Claude => Some("claude-code"),
        SetupTarget::Codex => Some("codex"),
        SetupTarget::Cursor => Some("cursor"),
        SetupTarget::Windsurf => Some("windsurf"),
        SetupTarget::Antigravity => Some("antigravity"),
        SetupTarget::Opencode => Some("opencode"),
        SetupTarget::Standalone | SetupTarget::All => None,
    }
}

/// Get a human-readable display name for a target.
pub fn target_display_name(t: &SetupTarget) -> &'static str {
    match t {
        SetupTarget::Claude => "Claude Code",
        SetupTarget::Codex => "Codex",
        SetupTarget::Cursor => "Cursor",
        SetupTarget::Windsurf => "Windsurf",
        SetupTarget::Antigravity => "Antigravity",
        SetupTarget::Opencode => "Opencode",
        SetupTarget::Standalone => "Standalone CLI",
        SetupTarget::All => "All",
    }
}

/// Build the npx command arguments for skills installation.
fn build_skills_command(target: Option<&SetupTarget>, auto_confirm: bool) -> Vec<String> {
    let mut args = vec![
        "skills".to_string(),
        "add".to_string(),
        SKILLS_PACKAGE.to_string(),
    ];

    if let Some(t) = target {
        if let Some(agent) = target_to_agent_flag(t) {
            args.push("-a".to_string());
            args.push(agent.to_string());
        }
    }

    if auto_confirm {
        args.push("-y".to_string());
    }

    args
}

/// Format the full command string for display purposes.
fn format_skills_command(target: Option<&SetupTarget>) -> String {
    let mut cmd = format!("npx skills add {}", SKILLS_PACKAGE);
    if let Some(t) = target {
        if let Some(agent) = target_to_agent_flag(t) {
            cmd.push_str(&format!(" -a {}", agent));
        }
    }
    cmd
}

/// Result of the skills installation step.
#[derive(Debug)]
pub struct SkillsResult {
    pub npx_available: bool,
    pub action: SkillsAction,
    pub command: String,
}

#[derive(Debug, PartialEq)]
pub enum SkillsAction {
    Installed,
    Skipped,
    Prompted,
    Failed,
}

impl std::fmt::Display for SkillsAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillsAction::Installed => write!(f, "installed"),
            SkillsAction::Skipped => write!(f, "skipped"),
            SkillsAction::Prompted => write!(f, "prompted"),
            SkillsAction::Failed => write!(f, "failed"),
        }
    }
}

/// Install skills via `npx skills add`. Interactive step in the setup wizard.
pub fn install_skills(
    cli: &Cli,
    env: &EnvironmentInfo,
    non_interactive: bool,
) -> Result<SkillsResult> {
    let command_str = format_skills_command(None);

    if !env.npx_available {
        // npx not available — show manual instructions
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "skills",
                    "npx_available": false,
                    "action": "prompted",
                    "command": command_str,
                })
            );
        } else {
            println!("  {}  npx not found", "■".yellow());
            println!("  {}", "│".dimmed());
            println!(
                "  {}  To install Actionbook skills for your AI coding tools, run:",
                "│".dimmed()
            );
            println!(
                "  {}  {} {}",
                "│".dimmed(),
                "$".dimmed(),
                command_str.cyan()
            );
            println!("  {}", "│".dimmed());
            println!(
                "  {}  {}",
                "│".dimmed(),
                "(requires Node.js: https://nodejs.org)".dimmed()
            );
            println!("  {}", "└".dimmed());
        }

        return Ok(SkillsResult {
            npx_available: false,
            action: SkillsAction::Prompted,
            command: command_str,
        });
    }

    // npx available
    if !cli.json && !non_interactive {
        println!(
            "  {}  Source: {}",
            "◇".dimmed(),
            format!("https://github.com/{}.git", SKILLS_PACKAGE).dimmed()
        );
        println!("  {}", "│".dimmed());
    }

    if non_interactive {
        // Auto-install in non-interactive mode
        return run_npx_skills(cli, None, true);
    }

    // Interactive: prompt user
    let choices = vec!["Install now (recommended)", "Skip"];
    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Install Actionbook skills for your AI coding tools?")
        .items(&choices)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    match selection {
        0 => run_npx_skills(cli, None, false),
        _ => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "skills",
                        "npx_available": true,
                        "action": "skipped",
                        "command": command_str,
                    })
                );
            } else {
                println!("  {}  Skills installation skipped", "◇".dimmed());
                println!("  {}", "│".dimmed());
                println!("  {}  You can install later with:", "│".dimmed());
                println!(
                    "  {}  {} {}",
                    "│".dimmed(),
                    "$".dimmed(),
                    command_str.cyan()
                );
                println!("  {}", "└".dimmed());
            }
            Ok(SkillsResult {
                npx_available: true,
                action: SkillsAction::Skipped,
                command: command_str,
            })
        }
    }
}

/// Quick-mode: install skills for a specific target via `npx skills add`.
pub fn install_skills_for_target(cli: &Cli, target: &SetupTarget) -> Result<SkillsResult> {
    let npx_available = which::which("npx").is_ok();
    let command_str = format_skills_command(Some(target));

    if !npx_available {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "skills",
                    "npx_available": false,
                    "action": "prompted",
                    "command": command_str,
                })
            );
        } else {
            println!("  {}  npx not found", "■".yellow());
            println!("  {}", "│".dimmed());
            println!("  {}  Run this command manually:", "│".dimmed());
            println!(
                "  {}  {} {}",
                "│".dimmed(),
                "$".dimmed(),
                command_str.cyan()
            );
            println!("  {}", "│".dimmed());
            println!(
                "  {}  {}",
                "│".dimmed(),
                "(requires Node.js: https://nodejs.org)".dimmed()
            );
            println!("  {}", "└".dimmed());
        }

        return Ok(SkillsResult {
            npx_available: false,
            action: SkillsAction::Prompted,
            command: command_str,
        });
    }

    if !cli.json {
        println!(
            "  {}  Source: {}",
            "◇".dimmed(),
            format!("https://github.com/{}.git", SKILLS_PACKAGE).dimmed()
        );
        println!("  {}", "│".dimmed());
    }

    run_npx_skills(cli, Some(target), true)
}

/// Execute `npx skills add` as a child process, inheriting stdin/stdout.
fn run_npx_skills(
    cli: &Cli,
    target: Option<&SetupTarget>,
    auto_confirm: bool,
) -> Result<SkillsResult> {
    let args = build_skills_command(target, auto_confirm);
    let command_str = format_skills_command(target);

    if !cli.json {
        println!(
            "  {}  Running: {}",
            "◇".dimmed(),
            format!("npx {}", args.join(" ")).cyan()
        );
        println!("  {}", "│".dimmed());
    }

    // In JSON mode, pipe subprocess output to avoid interleaving with structured JSON
    let (stdout_cfg, stderr_cfg) = if cli.json {
        (std::process::Stdio::piped(), std::process::Stdio::piped())
    } else {
        (
            std::process::Stdio::inherit(),
            std::process::Stdio::inherit(),
        )
    };

    let status = Command::new("npx")
        .args(&args)
        .stdin(std::process::Stdio::inherit())
        .stdout(stdout_cfg)
        .stderr(stderr_cfg)
        .status();

    match status {
        Ok(exit) if exit.success() => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "skills",
                        "npx_available": true,
                        "action": "installed",
                        "command": command_str,
                    })
                );
            } else {
                println!();
                println!(
                    "  {}  {}",
                    "◇".green(),
                    "Skills installed successfully".green()
                );
                println!("  {}", "└".dimmed());
            }
            Ok(SkillsResult {
                npx_available: true,
                action: SkillsAction::Installed,
                command: command_str,
            })
        }
        Ok(exit) => {
            let code = exit.code().unwrap_or(-1);
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "skills",
                        "npx_available": true,
                        "action": "failed",
                        "command": command_str,
                        "exit_code": code,
                    })
                );
            } else {
                println!();
                println!(
                    "  {}  Skills installation failed (exit code: {})",
                    "■".red(),
                    code
                );
                println!("  {}", "│".dimmed());
                println!("  {}  You can retry manually:", "│".dimmed());
                println!(
                    "  {}  {} {}",
                    "│".dimmed(),
                    "$".dimmed(),
                    command_str.cyan()
                );
                println!("  {}", "└".dimmed());
            }
            Ok(SkillsResult {
                npx_available: true,
                action: SkillsAction::Failed,
                command: command_str,
            })
        }
        Err(e) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "skills",
                        "npx_available": true,
                        "action": "failed",
                        "command": command_str,
                        "error": e.to_string(),
                    })
                );
            } else {
                println!(
                    "  {}  Failed to run npx: {}",
                    "■".red(),
                    e.to_string().dimmed()
                );
                println!("  {}", "│".dimmed());
                println!("  {}  You can retry manually:", "│".dimmed());
                println!(
                    "  {}  {} {}",
                    "│".dimmed(),
                    "$".dimmed(),
                    command_str.cyan()
                );
                println!("  {}", "└".dimmed());
            }
            Ok(SkillsResult {
                npx_available: true,
                action: SkillsAction::Failed,
                command: command_str,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_to_agent_flag() {
        assert_eq!(
            target_to_agent_flag(&SetupTarget::Claude),
            Some("claude-code")
        );
        assert_eq!(target_to_agent_flag(&SetupTarget::Codex), Some("codex"));
        assert_eq!(target_to_agent_flag(&SetupTarget::Cursor), Some("cursor"));
        assert_eq!(
            target_to_agent_flag(&SetupTarget::Windsurf),
            Some("windsurf")
        );
        assert_eq!(
            target_to_agent_flag(&SetupTarget::Antigravity),
            Some("antigravity")
        );
        assert_eq!(
            target_to_agent_flag(&SetupTarget::Opencode),
            Some("opencode")
        );
        assert_eq!(target_to_agent_flag(&SetupTarget::Standalone), None);
        assert_eq!(target_to_agent_flag(&SetupTarget::All), None);
    }

    #[test]
    fn test_target_display_name() {
        assert_eq!(target_display_name(&SetupTarget::Claude), "Claude Code");
        assert_eq!(target_display_name(&SetupTarget::Cursor), "Cursor");
        assert_eq!(target_display_name(&SetupTarget::Codex), "Codex");
        assert_eq!(target_display_name(&SetupTarget::Windsurf), "Windsurf");
        assert_eq!(
            target_display_name(&SetupTarget::Antigravity),
            "Antigravity"
        );
        assert_eq!(target_display_name(&SetupTarget::Opencode), "Opencode");
        assert_eq!(
            target_display_name(&SetupTarget::Standalone),
            "Standalone CLI"
        );
        assert_eq!(target_display_name(&SetupTarget::All), "All");
    }

    #[test]
    fn test_build_skills_command_no_target() {
        let args = build_skills_command(None, false);
        assert_eq!(args, vec!["skills", "add", SKILLS_PACKAGE]);
    }

    #[test]
    fn test_build_skills_command_with_target() {
        let args = build_skills_command(Some(&SetupTarget::Claude), false);
        assert_eq!(
            args,
            vec!["skills", "add", SKILLS_PACKAGE, "-a", "claude-code"]
        );
    }

    #[test]
    fn test_build_skills_command_auto_confirm() {
        let args = build_skills_command(Some(&SetupTarget::Cursor), true);
        assert_eq!(
            args,
            vec!["skills", "add", SKILLS_PACKAGE, "-a", "cursor", "-y"]
        );
    }

    #[test]
    fn test_build_skills_command_all_no_agent_flag() {
        let args = build_skills_command(Some(&SetupTarget::All), true);
        assert_eq!(args, vec!["skills", "add", SKILLS_PACKAGE, "-y"]);
    }

    #[test]
    fn test_format_skills_command_no_target() {
        let cmd = format_skills_command(None);
        assert_eq!(cmd, format!("npx skills add {}", SKILLS_PACKAGE));
    }

    #[test]
    fn test_format_skills_command_with_target() {
        let cmd = format_skills_command(Some(&SetupTarget::Claude));
        assert_eq!(
            cmd,
            format!("npx skills add {} -a claude-code", SKILLS_PACKAGE)
        );
    }

    #[test]
    fn test_skills_action_display() {
        assert_eq!(format!("{}", SkillsAction::Installed), "installed");
        assert_eq!(format!("{}", SkillsAction::Skipped), "skipped");
        assert_eq!(format!("{}", SkillsAction::Prompted), "prompted");
        assert_eq!(format!("{}", SkillsAction::Failed), "failed");
    }
}
