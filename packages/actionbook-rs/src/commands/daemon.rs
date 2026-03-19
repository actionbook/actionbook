use colored::Colorize;

use crate::cli::{Cli, DaemonCommands};
use crate::commands::browser::effective_profile_name;
use crate::config::Config;
use crate::error::Result;

pub async fn run(cli: &Cli, command: &DaemonCommands) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = (cli, command);
        return Err(crate::error::ActionbookError::FeatureNotSupported(
            "Daemon mode is only supported on Unix (macOS/Linux)".to_string(),
        ));
    }

    #[cfg(unix)]
    {
        run_unix(cli, command).await
    }
}

#[cfg(unix)]
async fn run_unix(cli: &Cli, command: &DaemonCommands) -> Result<()> {
    use crate::daemon::{lifecycle, server};

    let config = Config::load()?;
    let profile = effective_profile_name(cli, &config).to_string();

    match command {
        DaemonCommands::Serve {
            profile: prof_override,
        } => {
            let profile = prof_override
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&profile);
            // Daemon is profile-scoped; -S/--session is not applicable here.
            // Session routing is handled internally by the daemon's routing table.
            if cli.session.is_some() {
                tracing::warn!(
                    "The -S/--session flag is ignored by `daemon serve`. \
                     The daemon manages all sessions for profile '{}'.",
                    profile
                );
            }
            server::run_with_session(profile, None).await
        }
        DaemonCommands::Status => {
            // Daemon is per-profile, not per-session
            if cli.session.is_some() {
                tracing::warn!(
                    "The -S/--session flag is ignored by `daemon status`. \
                     The daemon is profile-scoped (profile '{}').",
                    profile
                );
            }
            let alive = lifecycle::is_daemon_alive(&profile).await;
            let sock = lifecycle::socket_path(&profile);
            let pid = lifecycle::pid_path(&profile);
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "profile": profile,
                        "running": alive,
                        "socket": sock.display().to_string(),
                        "pid_file": pid.display().to_string(),
                    })
                );
            } else if alive {
                println!(
                    "{} Daemon for profile '{}' is {}",
                    "●".green(),
                    profile,
                    "running".green()
                );
                println!("  Socket: {}", sock.display());
            } else {
                println!(
                    "{} Daemon for profile '{}' is {}",
                    "○".dimmed(),
                    profile,
                    "not running".dimmed()
                );
            }
            Ok(())
        }
        DaemonCommands::Stop => {
            if cli.session.is_some() {
                tracing::warn!(
                    "The -S/--session flag is ignored by `daemon stop`. \
                     The daemon is profile-scoped (profile '{}').",
                    profile
                );
            }
            lifecycle::stop_daemon(&profile).await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "profile": profile,
                        "stopped": true,
                    })
                );
            } else {
                println!("Daemon for profile '{}' stopped", profile);
            }
            Ok(())
        }
    }
}
