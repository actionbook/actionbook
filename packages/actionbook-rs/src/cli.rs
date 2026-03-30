use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;

use crate::commands;
use crate::config::DEFAULT_EXTENSION_PORT;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SetupTarget {
    Claude,
    Codex,
    Cursor,
    Windsurf,
    Antigravity,
    Opencode,
    Standalone,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserMode {
    /// Launch a dedicated debug browser, control via CDP
    #[serde(alias = "builtin")]
    Isolated,
    /// Use Chrome Extension bridge with user's existing browser
    #[serde(alias = "system")]
    Extension,
}

/// Actionbook CLI - Browser automation with zero installation
#[derive(Parser, Clone)]
#[command(name = "actionbook", bin_name = "actionbook")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Profile name to use
    #[arg(short = 'P', long, env = "ACTIONBOOK_PROFILE", global = true)]
    pub profile: Option<String>,

    /// API key for authenticated access
    #[arg(
        long,
        env = "ACTIONBOOK_API_KEY",
        global = true,
        hide_env_values = true
    )]
    pub api_key: Option<String>,

    /// Output in JSON format
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Search for action manuals by keyword
    Search {
        /// Search keyword (e.g., "airbnb search", "google login")
        query: String,

        /// Filter by domain (e.g., "airbnb.com")
        #[arg(short, long)]
        domain: Option<String>,

        /// Filter by URL
        #[arg(short, long)]
        url: Option<String>,

        /// Page number
        #[arg(short, long, default_value = "1")]
        page: u32,

        /// Results per page (1-100)
        #[arg(short = 's', long, default_value = "10")]
        page_size: u32,
    },

    /// Get complete action details by area ID
    Get {
        /// Area ID (e.g., "airbnb.com:/:default")
        area_id: String,
    },

    /// Show all executable elements and methods for an area
    Act {
        /// Area ID (e.g., "github.com:/login:default")
        area_id: String,
    },

    /// List or search sources
    Sources {
        #[command(subcommand)]
        command: SourcesCommands,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Profile management
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Extension bridge management (for controlling user's browser via Chrome Extension)
    Extension {
        #[command(subcommand)]
        command: ExtensionCommands,
    },

    /// Daemon management (persistent browser session manager)
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },

    /// Initial setup wizard
    #[command(
        after_help = "Agent-friendly non-interactive examples:\n  actionbook setup --non-interactive --target codex --browser isolated --api-key $ACTIONBOOK_API_KEY --json\n  actionbook setup --non-interactive --target claude --browser extension --json\n\nTips:\n  --target selects skill installation target agent type.\n  --non-interactive disables prompts (agent-safe).\n  --json emits machine-readable setup results."
    )]
    Setup {
        /// Skill installation target agent type. If used alone, runs quick install (`npx skills add`) and exits.
        #[arg(short, long, value_enum)]
        target: Option<SetupTarget>,

        /// API key (non-interactive)
        #[arg(long, env = "ACTIONBOOK_API_KEY", hide_env_values = true)]
        api_key: Option<String>,

        /// Browser mode for setup flow (e.g. with --non-interactive)
        #[arg(long, value_enum)]
        browser: Option<BrowserMode>,

        /// Run setup without interactive prompts (agent-safe)
        #[arg(long)]
        non_interactive: bool,

        /// Reset existing configuration and start fresh
        #[arg(long)]
        reset: bool,
    },
}

#[derive(Subcommand, Clone)]
pub enum DaemonCommands {
    /// Start daemon server (internal, auto-started)
    #[command(hide = true)]
    Serve {
        /// Profile name (overrides global --profile)
        #[arg(long)]
        profile: Option<String>,
    },

    /// Start the daemon in the foreground (used by auto-start and for debugging)
    #[command(name = "serve-v2")]
    ServeV2,

    /// Check daemon status
    Status,

    /// Stop the daemon
    Stop,
}

#[derive(Subcommand, Clone)]
pub enum ExtensionCommands {
    #[command(hide = true)]
    /// Start the extension bridge WebSocket server
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = DEFAULT_EXTENSION_PORT)]
        port: u16,
    },

    /// Check if the bridge server is running
    Status {
        /// Bridge server port
        #[arg(long, default_value_t = DEFAULT_EXTENSION_PORT)]
        port: u16,
    },

    /// Ping the extension through the bridge
    Ping {
        /// Bridge server port
        #[arg(long, default_value_t = DEFAULT_EXTENSION_PORT)]
        port: u16,
    },

    /// Install local debug extension fallback package from GitHub
    Install {
        /// Force reinstall even if already installed at same version
        #[arg(long)]
        force: bool,
    },

    /// Stop the running bridge server
    Stop {
        /// Bridge server port
        #[arg(long, default_value_t = DEFAULT_EXTENSION_PORT)]
        port: u16,
    },

    /// Print the extension install directory path
    Path,

    /// Remove the installed extension
    Uninstall,
}

#[derive(Subcommand, Clone)]
pub enum SourcesCommands {
    /// List all sources
    List,

    /// Search sources
    Search {
        /// Search query
        query: String,
    },
}

#[derive(Subcommand, Clone)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },

    /// Edit configuration file
    Edit,

    /// Show configuration file path
    Path,

    /// Reset configuration (delete config file)
    Reset,
}

#[derive(Subcommand, Clone)]
pub enum ProfileCommands {
    /// List all profiles
    List,

    /// Create a new profile
    Create {
        /// Profile name
        name: String,

        /// CDP port
        #[arg(long)]
        cdp_port: Option<u16>,
    },

    /// Delete a profile
    Delete {
        /// Profile name
        name: String,
    },

    /// Show profile details
    Show {
        /// Profile name
        name: String,
    },
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    #[allow(dead_code)]
    pub fn try_parse() -> std::result::Result<Self, clap::Error> {
        <Self as Parser>::try_parse()
    }

    #[allow(dead_code)]
    pub fn parse_from<I, T>(itr: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        <Self as Parser>::parse_from(itr)
    }

    #[allow(dead_code)]
    pub fn try_parse_from<I, T>(itr: I) -> std::result::Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        <Self as Parser>::try_parse_from(itr)
    }

    pub async fn run(&self) -> Result<()> {
        crate::update_notifier::maybe_notify(self).await;

        match &self.command {
            Commands::Extension { command } => commands::extension::run(self, command).await,
            Commands::Search {
                query,
                domain,
                url,
                page,
                page_size,
            } => {
                commands::search::run(
                    self,
                    query,
                    domain.as_deref(),
                    url.as_deref(),
                    *page,
                    *page_size,
                )
                .await
            }
            Commands::Get { area_id } => commands::get::run(self, area_id).await,
            Commands::Act { area_id } => commands::act::run(self, area_id).await,
            Commands::Sources { command } => commands::sources::run(self, command).await,
            Commands::Config { command } => commands::config::run(self, command).await,
            Commands::Profile { command } => commands::profile::run(self, command).await,
            Commands::Daemon { command } => commands::daemon::run(self, command).await,
            Commands::Setup {
                target,
                api_key,
                browser,
                non_interactive,
                reset,
            } => {
                commands::setup::run(
                    self,
                    commands::setup::SetupArgs {
                        target: *target,
                        api_key: api_key.as_deref(),
                        browser: *browser,
                        non_interactive: *non_interactive,
                        reset: *reset,
                    },
                )
                .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_parse_from_parses_config_path() {
        let parsed = Cli::try_parse_from(["actionbook", "config", "path"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Config {
                command: ConfigCommands::Path
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_config_show() {
        let parsed = Cli::try_parse_from(["actionbook", "config", "show"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Config {
                command: ConfigCommands::Show
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_config_get() {
        let parsed = Cli::try_parse_from(["actionbook", "config", "get", "api.base_url"]).unwrap();
        if let Commands::Config {
            command: ConfigCommands::Get { key },
        } = parsed.command
        {
            assert_eq!(key, "api.base_url");
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_config_set() {
        let parsed =
            Cli::try_parse_from(["actionbook", "config", "set", "api.api_key", "sk-test-123"])
                .unwrap();
        if let Commands::Config {
            command: ConfigCommands::Set { key, value },
        } = parsed.command
        {
            assert_eq!(key, "api.api_key");
            assert_eq!(value, "sk-test-123");
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_json_flag() {
        let parsed = Cli::try_parse_from(["actionbook", "--json", "config", "show"]).unwrap();
        assert!(parsed.json);
    }

    #[test]
    fn try_parse_from_parses_profile_flag() {
        let parsed =
            Cli::try_parse_from(["actionbook", "--profile", "work", "config", "show"]).unwrap();
        assert_eq!(parsed.profile.as_deref(), Some("work"));
    }

    #[test]
    fn try_parse_from_parses_profile_list() {
        let parsed = Cli::try_parse_from(["actionbook", "profile", "list"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Profile {
                command: ProfileCommands::List
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_daemon_stop() {
        let parsed = Cli::try_parse_from(["actionbook", "daemon", "stop"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Daemon {
                command: DaemonCommands::Stop
            }
        ));
    }

    #[test]
    fn browser_mode_serde_round_trip() {
        for mode in [BrowserMode::Isolated, BrowserMode::Extension] {
            let json = serde_json::to_string(&mode).unwrap();
            let decoded: BrowserMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, decoded);
        }
    }

    #[test]
    fn browser_mode_aliases_deserialize() {
        // "builtin" is an alias for Isolated
        let mode: BrowserMode = serde_json::from_str("\"builtin\"").unwrap();
        assert_eq!(mode, BrowserMode::Isolated);

        // "system" is an alias for Extension
        let mode: BrowserMode = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(mode, BrowserMode::Extension);
    }

    #[test]
    fn setup_target_equality() {
        assert_eq!(SetupTarget::Claude, SetupTarget::Claude);
        assert_ne!(SetupTarget::Claude, SetupTarget::Cursor);
        assert_eq!(SetupTarget::All, SetupTarget::All);
    }

    #[test]
    fn try_parse_from_parses_search_with_all_options() {
        let parsed = Cli::try_parse_from([
            "actionbook",
            "search",
            "airbnb login",
            "--domain",
            "airbnb.com",
            "--url",
            "https://airbnb.com/login",
            "--page",
            "2",
            "--page-size",
            "20",
        ])
        .unwrap();
        if let Commands::Search {
            query,
            domain,
            url,
            page,
            page_size,
        } = parsed.command
        {
            assert_eq!(query, "airbnb login");
            assert_eq!(domain.as_deref(), Some("airbnb.com"));
            assert_eq!(url.as_deref(), Some("https://airbnb.com/login"));
            assert_eq!(page, 2);
            assert_eq!(page_size, 20);
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_search_defaults() {
        let parsed = Cli::try_parse_from(["actionbook", "search", "google"]).unwrap();
        if let Commands::Search {
            page, page_size, ..
        } = parsed.command
        {
            assert_eq!(page, 1);
            assert_eq!(page_size, 10);
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_get() {
        let parsed = Cli::try_parse_from(["actionbook", "get", "airbnb.com:/:default"]).unwrap();
        if let Commands::Get { area_id } = parsed.command {
            assert_eq!(area_id, "airbnb.com:/:default");
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_act() {
        let parsed =
            Cli::try_parse_from(["actionbook", "act", "github.com:/login:default"]).unwrap();
        if let Commands::Act { area_id } = parsed.command {
            assert_eq!(area_id, "github.com:/login:default");
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_sources_list() {
        let parsed = Cli::try_parse_from(["actionbook", "sources", "list"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Sources {
                command: SourcesCommands::List
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_sources_search() {
        let parsed = Cli::try_parse_from(["actionbook", "sources", "search", "google"]).unwrap();
        if let Commands::Sources {
            command: SourcesCommands::Search { query },
        } = parsed.command
        {
            assert_eq!(query, "google");
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_config_edit() {
        let parsed = Cli::try_parse_from(["actionbook", "config", "edit"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Config {
                command: ConfigCommands::Edit
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_config_reset() {
        let parsed = Cli::try_parse_from(["actionbook", "config", "reset"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Config {
                command: ConfigCommands::Reset
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_profile_create_with_port() {
        let parsed = Cli::try_parse_from([
            "actionbook",
            "profile",
            "create",
            "work",
            "--cdp-port",
            "9333",
        ])
        .unwrap();
        if let Commands::Profile {
            command: ProfileCommands::Create { name, cdp_port },
        } = parsed.command
        {
            assert_eq!(name, "work");
            assert_eq!(cdp_port, Some(9333));
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_profile_create_without_port() {
        let parsed = Cli::try_parse_from(["actionbook", "profile", "create", "test"]).unwrap();
        if let Commands::Profile {
            command: ProfileCommands::Create { name, cdp_port },
        } = parsed.command
        {
            assert_eq!(name, "test");
            assert_eq!(cdp_port, None);
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_profile_delete() {
        let parsed = Cli::try_parse_from(["actionbook", "profile", "delete", "old"]).unwrap();
        if let Commands::Profile {
            command: ProfileCommands::Delete { name },
        } = parsed.command
        {
            assert_eq!(name, "old");
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_profile_show() {
        let parsed = Cli::try_parse_from(["actionbook", "profile", "show", "dev"]).unwrap();
        if let Commands::Profile {
            command: ProfileCommands::Show { name },
        } = parsed.command
        {
            assert_eq!(name, "dev");
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_daemon_status() {
        let parsed = Cli::try_parse_from(["actionbook", "daemon", "status"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Daemon {
                command: DaemonCommands::Status
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_setup_non_interactive() {
        let parsed = Cli::try_parse_from([
            "actionbook",
            "setup",
            "--non-interactive",
            "--target",
            "claude",
            "--browser",
            "isolated",
        ])
        .unwrap();
        if let Commands::Setup {
            target,
            non_interactive,
            browser,
            reset,
            ..
        } = parsed.command
        {
            assert_eq!(target, Some(SetupTarget::Claude));
            assert!(non_interactive);
            assert_eq!(browser, Some(BrowserMode::Isolated));
            assert!(!reset);
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_setup_reset() {
        let parsed = Cli::try_parse_from(["actionbook", "setup", "--reset"]).unwrap();
        if let Commands::Setup { reset, .. } = parsed.command {
            assert!(reset);
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_extension_status() {
        let parsed =
            Cli::try_parse_from(["actionbook", "extension", "status", "--port", "19333"]).unwrap();
        if let Commands::Extension {
            command: ExtensionCommands::Status { port },
        } = parsed.command
        {
            assert_eq!(port, 19333);
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_extension_install_force() {
        let parsed =
            Cli::try_parse_from(["actionbook", "extension", "install", "--force"]).unwrap();
        if let Commands::Extension {
            command: ExtensionCommands::Install { force },
        } = parsed.command
        {
            assert!(force);
        } else {
            panic!("wrong command parsed");
        }
    }

    #[test]
    fn try_parse_from_parses_extension_path() {
        let parsed = Cli::try_parse_from(["actionbook", "extension", "path"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Extension {
                command: ExtensionCommands::Path
            }
        ));
    }

    #[test]
    fn try_parse_from_parses_extension_uninstall() {
        let parsed = Cli::try_parse_from(["actionbook", "extension", "uninstall"]).unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Extension {
                command: ExtensionCommands::Uninstall
            }
        ));
    }

    #[test]
    fn try_parse_from_api_key_flag() {
        let parsed =
            Cli::try_parse_from(["actionbook", "--api-key", "sk-secret", "config", "show"])
                .unwrap();
        assert_eq!(parsed.api_key.as_deref(), Some("sk-secret"));
    }

    #[test]
    fn try_parse_from_no_json_by_default() {
        let parsed = Cli::try_parse_from(["actionbook", "config", "show"]).unwrap();
        assert!(!parsed.json);
    }

    #[test]
    fn try_parse_from_no_profile_by_default() {
        let parsed = Cli::try_parse_from(["actionbook", "config", "show"]).unwrap();
        assert!(parsed.profile.is_none());
    }

    #[test]
    fn try_parse_from_rejects_unknown_subcommand() {
        let result = Cli::try_parse_from(["actionbook", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn try_parse_from_search_requires_query() {
        let result = Cli::try_parse_from(["actionbook", "search"]);
        assert!(result.is_err());
    }

    #[test]
    fn try_parse_from_get_requires_area_id() {
        let result = Cli::try_parse_from(["actionbook", "get"]);
        assert!(result.is_err());
    }

    #[test]
    fn setup_target_all_variants() {
        for target_str in [
            "claude",
            "codex",
            "cursor",
            "windsurf",
            "antigravity",
            "opencode",
            "standalone",
            "all",
        ] {
            let parsed =
                Cli::try_parse_from(["actionbook", "setup", "--target", target_str]).unwrap();
            assert!(matches!(parsed.command, Commands::Setup { .. }));
        }
    }

    #[test]
    fn browser_mode_invalid_deserialize() {
        let result = serde_json::from_str::<BrowserMode>("\"invalid\"");
        assert!(result.is_err());
    }
}
