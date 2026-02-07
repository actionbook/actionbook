use clap::{Parser, Subcommand, ValueEnum};

use crate::commands;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BrowserMode {
    Builtin,
    System,
}

/// Actionbook CLI - Browser automation with zero installation
#[derive(Parser)]
#[command(name = "actionbook", bin_name = "actionbook")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Browser executable path (overrides auto-discovery)
    #[arg(long, env = "ACTIONBOOK_BROWSER_PATH", global = true)]
    pub browser_path: Option<String>,

    /// CDP port or WebSocket URL
    #[arg(long, env = "ACTIONBOOK_CDP", global = true)]
    pub cdp: Option<String>,

    /// Profile name to use
    #[arg(short = 'P', long, env = "ACTIONBOOK_PROFILE", global = true)]
    pub profile: Option<String>,

    /// Run in headless mode
    #[arg(long, env = "ACTIONBOOK_HEADLESS", global = true)]
    pub headless: bool,

    /// Enable stealth mode (requires --features stealth)
    #[arg(long, env = "ACTIONBOOK_STEALTH", global = true)]
    pub stealth: bool,

    /// Stealth OS profile: windows, macos-intel, macos-arm, linux
    #[arg(long, env = "ACTIONBOOK_STEALTH_OS", global = true)]
    pub stealth_os: Option<String>,

    /// Stealth GPU profile (e.g., nvidia-rtx4080, apple-m4-max, intel-uhd630)
    #[arg(long, env = "ACTIONBOOK_STEALTH_GPU", global = true)]
    pub stealth_gpu: Option<String>,

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

    /// Use extension mode (route browser commands through Chrome Extension bridge)
    #[arg(long, env = "ACTIONBOOK_EXTENSION", global = true)]
    pub extension: bool,

    /// Extension bridge port
    #[arg(long, env = "ACTIONBOOK_EXTENSION_PORT", global = true, default_value = "19222")]
    pub extension_port: u16,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Browser automation commands
    Browser {
        #[command(subcommand)]
        command: BrowserCommands,
    },

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

    /// Initial setup wizard
    Setup {
        /// Target platform (skip wizard, run `npx skills add` for specific agent)
        #[arg(short, long, value_enum)]
        target: Option<SetupTarget>,

        /// API key (non-interactive)
        #[arg(long, env = "ACTIONBOOK_API_KEY", hide_env_values = true)]
        api_key: Option<String>,

        /// Browser mode
        #[arg(long, value_enum)]
        browser: Option<BrowserMode>,

        /// Skip interactive prompts
        #[arg(long)]
        non_interactive: bool,

        /// Reset existing configuration and start fresh
        #[arg(long)]
        reset: bool,
    },
}

#[derive(Subcommand)]
pub enum BrowserCommands {
    /// Show browser status and detection results
    Status,

    /// Open a URL in a new tab
    Open {
        /// URL to open
        url: String,
    },

    /// Navigate current page to URL
    Goto {
        /// URL to navigate to
        url: String,
        /// Wait for navigation to complete (ms)
        #[arg(long, default_value = "30000")]
        timeout: u64,
    },

    /// Go back in history
    Back,

    /// Go forward in history
    Forward,

    /// Reload current page
    Reload,

    /// List all open pages/tabs
    Pages,

    /// Switch to a specific page by ID
    Switch {
        /// Page ID (from 'pages' command)
        page_id: String,
    },

    /// Wait for an element to appear
    Wait {
        /// CSS selector to wait for
        selector: String,
        /// Timeout in milliseconds
        #[arg(long, default_value = "30000")]
        timeout: u64,
    },

    /// Wait for navigation to complete
    WaitNav {
        /// Timeout in milliseconds
        #[arg(long, default_value = "30000")]
        timeout: u64,
    },

    /// Click an element
    Click {
        /// CSS selector
        selector: String,
        /// Wait for element before clicking (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
    },

    /// Type text into an element (appends to existing)
    Type {
        /// CSS selector
        selector: String,
        /// Text to type
        text: String,
        /// Wait for element before typing (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
    },

    /// Clear and type text into an element
    Fill {
        /// CSS selector
        selector: String,
        /// Text to fill
        text: String,
        /// Wait for element before filling (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
    },

    /// Select an option from dropdown
    Select {
        /// CSS selector for select element
        selector: String,
        /// Value to select
        value: String,
    },

    /// Hover over an element
    Hover {
        /// CSS selector
        selector: String,
    },

    /// Focus on an element
    Focus {
        /// CSS selector
        selector: String,
    },

    /// Press a keyboard key
    Press {
        /// Key to press (e.g., Enter, Tab, Escape, ArrowDown)
        key: String,
    },

    /// Take a screenshot
    Screenshot {
        /// Output file path (default: screenshot.png)
        #[arg(default_value = "screenshot.png")]
        path: String,
        /// Take full page screenshot
        #[arg(long)]
        full_page: bool,
    },

    /// Export page as PDF
    Pdf {
        /// Output file path
        path: String,
    },

    /// Execute JavaScript
    Eval {
        /// JavaScript code to execute
        code: String,
    },

    /// Get page HTML
    Html {
        /// Get only outer HTML of selector (optional)
        selector: Option<String>,
    },

    /// Get page text content
    Text {
        /// Get only text of selector (optional)
        selector: Option<String>,
    },

    /// Get accessibility snapshot
    Snapshot,

    /// Inspect DOM element at coordinates
    Inspect {
        /// X coordinate within viewport
        x: f64,
        /// Y coordinate within viewport
        y: f64,
        /// Optional description of what you're looking for
        #[arg(long)]
        desc: Option<String>,
    },

    /// Get viewport dimensions
    Viewport,

    /// Get or set cookies
    Cookies {
        #[command(subcommand)]
        command: Option<CookiesCommands>,
    },

    /// Close the browser
    Close,

    /// Restart the browser
    Restart,

    /// Connect to an existing browser
    Connect {
        /// CDP endpoint (port or WebSocket URL)
        endpoint: String,
    },
}

#[derive(Subcommand)]
pub enum CookiesCommands {
    /// List all cookies
    List,
    /// Get a specific cookie
    Get {
        /// Cookie name
        name: String,
    },
    /// Set a cookie
    Set {
        /// Cookie name
        name: String,
        /// Cookie value
        value: String,
        /// Cookie domain (optional)
        #[arg(long)]
        domain: Option<String>,
    },
    /// Delete a cookie
    Delete {
        /// Cookie name
        name: String,
    },
    /// Clear all cookies
    Clear,
}

#[derive(Subcommand)]
pub enum ExtensionCommands {
    /// Start the extension bridge WebSocket server
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "19222")]
        port: u16,
    },

    /// Check if the bridge server is running
    Status {
        /// Bridge server port
        #[arg(long, default_value = "19222")]
        port: u16,
    },

    /// Ping the extension through the bridge
    Ping {
        /// Bridge server port
        #[arg(long, default_value = "19222")]
        port: u16,
    },
}

#[derive(Subcommand)]
pub enum SourcesCommands {
    /// List all sources
    List,

    /// Search sources
    Search {
        /// Search query
        query: String,
    },
}

#[derive(Subcommand)]
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

#[derive(Subcommand)]
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
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Browser { command } => commands::browser::run(self, command).await,
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
            Commands::Sources { command } => commands::sources::run(self, command).await,
            Commands::Config { command } => commands::config::run(self, command).await,
            Commands::Profile { command } => commands::profile::run(self, command).await,
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
