use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "actionbook", about = "Actionbook CLI - Browser automation for AI agents", disable_version_flag = true)]
pub struct Cli {
    /// JSON output (default is plain text)
    #[arg(long, global = true)]
    pub json: bool,

    /// Timeout in milliseconds
    #[arg(long, global = true)]
    pub timeout: Option<u64>,

    /// Print version
    #[arg(long)]
    pub version: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
#[command(disable_help_subcommand = true)]
pub enum Commands {
    /// Browser automation commands
    Browser {
        #[command(subcommand)]
        command: BrowserCommands,
    },
    /// Search for actions
    Search {
        /// Search keywords
        query: String,
        /// Filter by domain
        #[arg(short = 'd', long)]
        domain: Option<String>,
        /// Filter by URL
        #[arg(short = 'u', long)]
        url: Option<String>,
        /// Page number
        #[arg(short = 'p', long, default_value = "1")]
        page: u32,
        /// Items per page
        #[arg(short = 's', long, default_value = "10")]
        page_size: u32,
    },
    /// Get action details
    Get {
        /// Action area ID
        area_id: String,
    },
    /// Show help
    Help,
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum BrowserCommands {
    /// Start or attach a browser session
    Start {
        /// Browser mode
        #[arg(long, value_enum, default_value = "local")]
        mode: CliMode,
        /// Headless mode
        #[arg(long)]
        headless: bool,
        /// Profile name
        #[arg(long)]
        profile: Option<String>,
        /// Open this URL on start
        #[arg(long)]
        open_url: Option<String>,
        /// Connect to existing CDP endpoint
        #[arg(long)]
        cdp_endpoint: Option<String>,
        /// Header for CDP endpoint (KEY:VALUE)
        #[arg(long)]
        header: Option<String>,
        /// Specify a semantic session ID
        #[arg(long)]
        set_session_id: Option<String>,
    },
    /// List all active sessions
    ListSessions,
    /// Show session status
    Status {
        /// Session ID
        #[arg(long)]
        session: String,
    },
    /// Close a session
    Close {
        /// Session ID
        #[arg(long)]
        session: String,
    },
    /// Restart a session
    Restart {
        /// Session ID
        #[arg(long)]
        session: String,
    },
    /// Navigate to URL
    Goto {
        /// Target URL
        url: String,
        /// Session ID
        #[arg(long)]
        session: String,
        /// Tab ID
        #[arg(long)]
        tab: String,
    },
    /// Open a new tab
    #[command(name = "new-tab")]
    NewTab {
        /// URL to open
        url: String,
        /// Session ID
        #[arg(long)]
        session: String,
        /// Open in new window
        #[arg(long)]
        new_window: bool,
    },
    /// Open a URL (alias for new-tab)
    Open {
        /// URL to open
        url: String,
        /// Session ID
        #[arg(long)]
        session: String,
        /// Open in new window
        #[arg(long)]
        new_window: bool,
    },
    /// Close a tab
    #[command(name = "close-tab")]
    CloseTab {
        /// Session ID
        #[arg(long)]
        session: String,
        /// Tab ID
        #[arg(long)]
        tab: String,
    },
    /// List tabs
    #[command(name = "list-tabs")]
    ListTabs {
        /// Session ID
        #[arg(long)]
        session: String,
    },
    /// Capture accessibility snapshot
    Snapshot {
        /// Session ID
        #[arg(long)]
        session: String,
        /// Tab ID
        #[arg(long)]
        tab: String,
    },
    /// Evaluate JavaScript
    Eval {
        /// JavaScript expression
        expression: String,
        /// Session ID
        #[arg(long)]
        session: String,
        /// Tab ID
        #[arg(long)]
        tab: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum DaemonCommands {
    /// Start daemon in foreground
    Serve,
    /// Stop daemon
    Stop,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum CliMode {
    Local,
    Extension,
    Cloud,
}

impl From<CliMode> for crate::types::Mode {
    fn from(m: CliMode) -> Self {
        match m {
            CliMode::Local => crate::types::Mode::Local,
            CliMode::Extension => crate::types::Mode::Extension,
            CliMode::Cloud => crate::types::Mode::Cloud,
        }
    }
}
