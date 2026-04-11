use std::process::{Command, Stdio};

use clap::ValueEnum;
use dialoguer::Select;

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use crate::error::CliError;

/// The skills package installed via `npx skills add`.
pub const SKILLS_PACKAGE: &str = "actionbook/actionbook";

/// AI coding tool target for skills installation.
///
/// Used by the `--target` flag to run `npx skills add` in quick mode
/// (bypassing the full setup wizard) and as the `-a` agent hint passed
/// through to the skills CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum SetupTarget {
    /// Claude Code
    Claude,
    /// Codex
    Codex,
    /// Cursor
    Cursor,
    /// Windsurf
    Windsurf,
    /// Antigravity
    Antigravity,
    /// Opencode
    Opencode,
    /// Standalone CLI (no AI tool integration)
    Standalone,
    /// Install for all known agents
    All,
}

/// Map a `SetupTarget` to the skills CLI `-a` agent flag value.
pub fn target_to_agent_flag(target: &SetupTarget) -> Option<&'static str> {
    match target {
        SetupTarget::Claude => Some("claude-code"),
        SetupTarget::Codex => Some("codex"),
        SetupTarget::Cursor => Some("cursor"),
        SetupTarget::Windsurf => Some("windsurf"),
        SetupTarget::Antigravity => Some("antigravity"),
        SetupTarget::Opencode => Some("opencode"),
        SetupTarget::Standalone => None,
        SetupTarget::All => Some("*"),
    }
}

/// Human-readable display name for a target.
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

/// Build the `npx` subcommand arguments (without the `npx` prefix).
fn build_skills_command(target: Option<&SetupTarget>, auto_confirm: bool) -> Vec<String> {
    let mut args = vec![
        "skills".to_string(),
        "add".to_string(),
        SKILLS_PACKAGE.to_string(),
    ];

    if let Some(t) = target
        && let Some(agent) = target_to_agent_flag(t)
    {
        args.push("-a".to_string());
        args.push(agent.to_string());
    }

    if auto_confirm {
        args.push("-y".to_string());
    }

    args
}

/// Format the full command string for display / logging.
fn format_skills_command(target: Option<&SetupTarget>) -> String {
    let mut cmd = format!("npx skills add {}", SKILLS_PACKAGE);
    if let Some(t) = target
        && let Some(agent) = target_to_agent_flag(t)
    {
        if agent == "*" {
            cmd.push_str(" -a '*'");
        } else {
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

#[derive(Debug, PartialEq, Eq)]
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

/// Install skills via `npx skills add`. Used inside the full setup wizard.
///
/// - npx missing → print manual instructions, return `Prompted`.
/// - non-interactive → install silently with `-y`.
/// - interactive → ask the user whether to install now.
pub fn install_skills(
    json: bool,
    env: &EnvironmentInfo,
    non_interactive: bool,
) -> Result<SkillsResult, CliError> {
    let command_str = format_skills_command(None);

    if !env.npx_available {
        print_missing_npx(json, &command_str);
        return Ok(SkillsResult {
            npx_available: false,
            action: SkillsAction::Prompted,
            command: command_str,
        });
    }

    if !json && !non_interactive {
        println!("  |    source: https://github.com/{}.git", SKILLS_PACKAGE);
        println!("  |");
    }

    if non_interactive {
        return run_npx_skills(json, None, true);
    }

    let choices = ["Install now (recommended)", "Skip"];
    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Install Actionbook skills for your AI coding tools?")
        .items(&choices)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {e}")))?;

    match selection {
        0 => run_npx_skills(json, None, false),
        _ => {
            if json {
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
                println!("  |  Skills installation skipped");
                println!("  |  Install later with: {}", command_str);
            }
            Ok(SkillsResult {
                npx_available: true,
                action: SkillsAction::Skipped,
                command: command_str,
            })
        }
    }
}

/// Quick mode: install skills for a specific target via `npx skills add`.
/// Skips the full setup wizard — only runs the skills step.
pub fn install_skills_for_target(
    json: bool,
    target: &SetupTarget,
) -> Result<SkillsResult, CliError> {
    let npx_available = which::which("npx").is_ok();
    let command_str = format_skills_command(Some(target));

    if !npx_available {
        print_missing_npx(json, &command_str);
        return Ok(SkillsResult {
            npx_available: false,
            action: SkillsAction::Prompted,
            command: command_str,
        });
    }

    if !json {
        println!("  |  source: https://github.com/{}.git", SKILLS_PACKAGE);
        println!("  |");
    }

    run_npx_skills(json, Some(target), true)
}

fn print_missing_npx(json: bool, command_str: &str) {
    if json {
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
        println!("  |  npx not found");
        println!("  |  To install Actionbook skills manually, run:");
        println!("  |    $ {}", command_str);
        println!("  |  (requires Node.js: https://nodejs.org)");
    }
}

/// Execute `npx skills add` as a child process.
///
/// In non-JSON mode stdio is inherited so the user sees the skills CLI
/// output directly. In JSON mode subprocess output is piped and discarded
/// to keep stdout a clean JSON stream.
fn run_npx_skills(
    json: bool,
    target: Option<&SetupTarget>,
    auto_confirm: bool,
) -> Result<SkillsResult, CliError> {
    let args = build_skills_command(target, auto_confirm);
    let command_str = format_skills_command(target);

    if !json {
        println!("  |  running: npx {}", args.join(" "));
        println!("  |");
    }

    // In JSON mode discard subprocess output entirely (Stdio::null) to keep
    // stdout a clean JSON stream. Piping + waiting via .status() would risk a
    // deadlock when `npx skills add` exceeds the ~16KB pipe buffer (the
    // "Installation Summary" block alone can exceed it). Using Stdio::null
    // side-steps the issue without needing to drain buffers.
    let (stdout_cfg, stderr_cfg) = if json {
        (Stdio::null(), Stdio::null())
    } else {
        (Stdio::inherit(), Stdio::inherit())
    };

    let status = Command::new("npx")
        .args(&args)
        .stdin(if json {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .stdout(stdout_cfg)
        .stderr(stderr_cfg)
        .status();

    match status {
        Ok(exit) if exit.success() => {
            if json {
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
                println!("  -  Skills installed successfully");
            }
            Ok(SkillsResult {
                npx_available: true,
                action: SkillsAction::Installed,
                command: command_str,
            })
        }
        Ok(exit) => {
            let code = exit.code().unwrap_or(-1);
            if json {
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
                println!("  !  Skills installation failed (exit code: {})", code);
                println!("  |  You can retry manually:");
                println!("  |    $ {}", command_str);
            }
            Ok(SkillsResult {
                npx_available: true,
                action: SkillsAction::Failed,
                command: command_str,
            })
        }
        Err(e) => {
            if json {
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
                println!("  !  Failed to run npx: {}", e);
                println!("  |  You can retry manually:");
                println!("  |    $ {}", command_str);
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
    fn target_to_agent_flag_maps_known_agents() {
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
        assert_eq!(target_to_agent_flag(&SetupTarget::All), Some("*"));
    }

    #[test]
    fn target_display_name_returns_human_strings() {
        assert_eq!(target_display_name(&SetupTarget::Claude), "Claude Code");
        assert_eq!(target_display_name(&SetupTarget::Codex), "Codex");
        assert_eq!(target_display_name(&SetupTarget::Cursor), "Cursor");
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
    fn build_skills_command_no_target() {
        let args = build_skills_command(None, false);
        assert_eq!(args, vec!["skills", "add", SKILLS_PACKAGE]);
    }

    #[test]
    fn build_skills_command_with_target() {
        let args = build_skills_command(Some(&SetupTarget::Claude), false);
        assert_eq!(
            args,
            vec!["skills", "add", SKILLS_PACKAGE, "-a", "claude-code"]
        );
    }

    #[test]
    fn build_skills_command_auto_confirm_adds_y() {
        let args = build_skills_command(Some(&SetupTarget::Cursor), true);
        assert_eq!(
            args,
            vec!["skills", "add", SKILLS_PACKAGE, "-a", "cursor", "-y"]
        );
    }

    #[test]
    fn build_skills_command_all_target_omits_agent_flag() {
        let args = build_skills_command(Some(&SetupTarget::All), true);
        assert_eq!(args, vec!["skills", "add", SKILLS_PACKAGE, "-a", "*", "-y"]);
    }

    #[test]
    fn format_skills_command_no_target() {
        let cmd = format_skills_command(None);
        assert_eq!(cmd, format!("npx skills add {}", SKILLS_PACKAGE));
    }

    #[test]
    fn format_skills_command_with_target() {
        let cmd = format_skills_command(Some(&SetupTarget::Claude));
        assert_eq!(
            cmd,
            format!("npx skills add {} -a claude-code", SKILLS_PACKAGE)
        );
    }

    #[test]
    fn format_skills_command_with_all_target_quotes_star_agent() {
        let cmd = format_skills_command(Some(&SetupTarget::All));
        assert_eq!(cmd, format!("npx skills add {} -a '*'", SKILLS_PACKAGE));
    }

    #[test]
    fn skills_action_display() {
        assert_eq!(format!("{}", SkillsAction::Installed), "installed");
        assert_eq!(format!("{}", SkillsAction::Skipped), "skipped");
        assert_eq!(format!("{}", SkillsAction::Prompted), "prompted");
        assert_eq!(format!("{}", SkillsAction::Failed), "failed");
    }

    #[test]
    fn install_skills_without_npx_returns_prompted() {
        let env = EnvironmentInfo {
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
            shell: None,
            browsers: vec![],
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: None,
        };
        let result = install_skills(true, &env, true).expect("should succeed");
        assert!(!result.npx_available);
        assert_eq!(result.action, SkillsAction::Prompted);
        assert_eq!(result.command, format!("npx skills add {}", SKILLS_PACKAGE));
    }
}
