use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

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

    /// Browser mode override (reads from config.toml by default)
    #[arg(long, env = "ACTIONBOOK_BROWSER_MODE", value_enum, global = true)]
    pub browser_mode: Option<BrowserMode>,

    /// [Deprecated: use --browser-mode=extension] Route commands through Chrome Extension bridge
    #[arg(long, env = "ACTIONBOOK_EXTENSION", global = true, hide = true)]
    pub extension: bool,

    /// [Deprecated] Extension bridge port override
    #[arg(long, env = "ACTIONBOOK_EXTENSION_PORT", global = true, default_value_t = DEFAULT_EXTENSION_PORT, hide = true)]
    pub extension_port: u16,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Block image downloads (faster page loads)
    #[arg(long, env = "ACTIONBOOK_BLOCK_IMAGES", global = true)]
    pub block_images: bool,

    /// Block images, fonts, CSS, and media (fastest page loads)
    #[arg(long, env = "ACTIONBOOK_BLOCK_MEDIA", global = true)]
    pub block_media: bool,

    /// Disable CSS animations, transitions, and smooth scrolling on all pages
    #[arg(long, env = "ACTIONBOOK_NO_ANIMATIONS", global = true)]
    pub no_animations: bool,

    /// Auto-dismiss JavaScript dialogs (alert, confirm, prompt)
    #[arg(long, env = "ACTIONBOOK_AUTO_DISMISS_DIALOGS", global = true)]
    pub auto_dismiss_dialogs: bool,

    /// Session tag for log correlation (auto-generated if omitted)
    #[arg(long, env = "ACTIONBOOK_SESSION_TAG", global = true)]
    pub session_tag: Option<String>,

    /// Rewrite URLs to privacy-friendly frontends (x.com→xcancel.com, reddit→old.reddit)
    #[arg(long, env = "ACTIONBOOK_REWRITE_URLS", global = true)]
    pub rewrite_urls: bool,

    /// Wait hint for navigation: instant, fast, normal, slow, heavy, or milliseconds
    #[arg(long, env = "ACTIONBOOK_WAIT_HINT", global = true)]
    pub wait_hint: Option<String>,

    /// Use Camoufox browser backend
    #[arg(long, env = "ACTIONBOOK_CAMOFOX", global = true)]
    pub camofox: bool,

    /// Camoufox server port
    #[arg(long, env = "ACTIONBOOK_CAMOFOX_PORT", global = true)]
    pub camofox_port: Option<u16>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Browser automation commands
    Browser {
        #[command(subcommand)]
        command: BrowserCommands,
    },

    /// Application automation commands (control Electron apps, etc.)
    App {
        #[command(subcommand)]
        command: AppCommands,
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
        /// CSS selector (or use --ref for snapshot ref)
        #[arg(required_unless_present = "ref")]
        selector: Option<String>,
        /// Wait for element before clicking (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
        /// Snapshot ref (e.g., e0, e5) from last `browser snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
        /// Use human-like bezier curve mouse movement
        #[arg(long)]
        human: bool,
    },

    /// Type text into an element (appends to existing)
    Type {
        /// Text to type (required)
        text: String,
        /// CSS selector (or use --ref for snapshot ref)
        #[arg(required_unless_present = "ref")]
        selector: Option<String>,
        /// Wait for element before typing (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
        /// Snapshot ref (e.g., e0, e5) from last `browser snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
        /// Use human-like typing with natural delays and occasional typos
        #[arg(long)]
        human: bool,
    },

    /// Clear and type text into an element
    Fill {
        /// Text to fill (required)
        text: String,
        /// CSS selector (or use --ref for snapshot ref)
        #[arg(required_unless_present = "ref")]
        selector: Option<String>,
        /// Wait for element before filling (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
        /// Snapshot ref (e.g., e0, e5) from last `browser snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
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

    /// Send keyboard hotkey (e.g., Control+A, Control+Shift+ArrowRight)
    Hotkey {
        /// Keys separated by '+' (e.g., "Control+A", "Control+Shift+C")
        keys: String,
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
        /// Extraction mode: raw (innerText) or readability (smart extraction, default)
        #[arg(long, default_value = "readability")]
        mode: String,
    },

    /// Get accessibility snapshot via CDP Accessibility Tree
    Snapshot {
        /// Only show interactive elements (buttons, links, inputs)
        #[arg(short = 'i', long)]
        interactive: bool,
        /// Include cursor-interactive elements (cursor:pointer, onclick, tabindex)
        #[arg(short = 'C', long)]
        cursor: bool,
        /// Remove empty structural elements (generic, group, list, etc.)
        #[arg(short = 'c', long)]
        compact: bool,
        /// Maximum tree depth
        #[arg(short = 'd', long)]
        depth: Option<usize>,
        /// Scope to elements under this CSS selector
        #[arg(short = 's', long)]
        selector: Option<String>,
        /// Output format: compact, json (default: compact)
        #[arg(long, default_value = "compact")]
        format: String,
        /// Show diff from last snapshot (added/changed/removed)
        #[arg(long)]
        diff: bool,
        /// Truncate output to approximately N tokens (for LLM context window management)
        #[arg(long)]
        max_tokens: Option<usize>,
    },

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

    /// Scroll the page
    Scroll {
        #[command(subcommand)]
        direction: ScrollDirection,
        /// Enable smooth scrolling
        #[arg(long)]
        smooth: bool,
        /// Wait for scroll to complete (scrollend event)
        #[arg(long)]
        wait: bool,
    },

    /// Execute a batch of actions from JSON (stdin or file)
    Batch {
        /// Path to JSON file with actions (reads from stdin if omitted)
        #[arg(long)]
        file: Option<String>,
        /// Delay between steps in milliseconds
        #[arg(long, default_value = "50")]
        delay: u64,
    },

    /// Rotate browser fingerprint (UA, platform, screen, hardware)
    Fingerprint {
        #[command(subcommand)]
        command: FingerprintCommands,
    },

    /// Capture console log messages from the page
    Console {
        /// Duration to listen for messages in milliseconds (0 = snapshot current)
        #[arg(long, default_value = "0")]
        duration: u64,
        /// Filter by log level: all, error, warning, info, log
        #[arg(long, default_value = "all")]
        level: String,
    },

    /// Wait for network to become idle (no pending requests)
    WaitIdle {
        /// Timeout in milliseconds
        #[arg(long, default_value = "30000")]
        timeout: u64,
        /// Idle threshold in milliseconds (no requests for this long)
        #[arg(long, default_value = "500")]
        idle_time: u64,
    },

    /// Get detailed info about an element (bounding box, attributes, styles)
    Info {
        /// CSS selector
        selector: String,
    },

    /// Manage localStorage and sessionStorage
    Storage {
        #[command(subcommand)]
        command: StorageCommands,
    },

    /// Emulate a device (mobile, tablet, desktop presets)
    Emulate {
        /// Device name: iphone-14, iphone-se, pixel-7, ipad, desktop-hd, or custom WxH
        device: String,
    },

    /// Wait for a JavaScript expression to return true
    WaitFn {
        /// JavaScript expression that should return a truthy value
        expression: String,
        /// Timeout in milliseconds
        #[arg(long, default_value = "30000")]
        timeout: u64,
        /// Polling interval in milliseconds
        #[arg(long, default_value = "100")]
        interval: u64,
    },

    /// Upload file(s) to a file input element
    Upload {
        /// File path(s) to upload
        #[arg(required = true)]
        files: Vec<String>,
        /// CSS selector for file input (auto-detects input[type="file"] if omitted)
        #[arg(short = 's', long)]
        selector: Option<String>,
        /// Snapshot ref (e.g., e0) from last `browser snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
        /// Wait for element before uploading (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
    },

    /// Fetch page content in one shot (navigate → wait → extract → close)
    Fetch {
        /// URL to fetch
        url: String,
        /// Output format: snapshot, text, html (default: text)
        #[arg(long, default_value = "text")]
        format: String,
        /// Truncate output to approximately N tokens
        #[arg(long)]
        max_tokens: Option<usize>,
        /// Timeout in milliseconds for the entire operation
        #[arg(long, default_value = "60000")]
        timeout: u64,
        /// Try HTTP fetch first, fallback to browser if needed
        #[arg(long)]
        lite: bool,
    },

    /// Close the browser
    Close,

    /// Restart the browser
    Restart,

    /// Connect to an existing browser
    Connect {
        /// CDP endpoint (port or WebSocket URL)
        endpoint: String,
        /// Optional HTTP headers for WebSocket auth (key:value pairs, repeatable)
        #[arg(long = "header", short = 'H', value_name = "KEY:VALUE")]
        headers: Vec<String>,
    },

    /// Manage browser tabs (list, create, switch, close)
    Tab {
        #[command(subcommand)]
        command: TabCommands,
    },

    /// Switch iframe context
    SwitchFrame {
        /// Target: iframe selector, "parent", or "default" for main frame
        target: String,
    },
}

#[derive(Subcommand, Clone)]
pub enum AppCommands {
    /// Launch an application by name (e.g., "Slack", "VSCode")
    Launch {
        /// Application name or bundle ID
        app_name: String,
    },

    /// Attach to a running application by name or port
    Attach {
        /// Application name, bundle ID, or CDP port/WebSocket URL
        target: String,
    },

    /// List all discoverable applications with CDP support
    List,

    /// Show application status and connection info
    Status,

    /// Close the connected application
    Close,

    /// Restart the connected application
    Restart,

    /// Navigate current window to URL
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

    /// List all open pages/windows
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
        /// CSS selector (or use --ref for snapshot ref)
        #[arg(required_unless_present = "ref")]
        selector: Option<String>,
        /// Wait for element before clicking (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
        /// Snapshot ref (e.g., e0, e5) from last `app snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
        /// Use human-like bezier curve mouse movement
        #[arg(long)]
        human: bool,
    },

    /// Type text into an element (appends to existing)
    Type {
        /// Text to type (required)
        text: String,
        /// CSS selector (or use --ref for snapshot ref)
        #[arg(required_unless_present = "ref")]
        selector: Option<String>,
        /// Wait for element before typing (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
        /// Snapshot ref (e.g., e0, e5) from last `app snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
        /// Use human-like typing with natural delays and occasional typos
        #[arg(long)]
        human: bool,
    },

    /// Clear and type text into an element
    Fill {
        /// Text to fill (required)
        text: String,
        /// CSS selector (or use --ref for snapshot ref)
        #[arg(required_unless_present = "ref")]
        selector: Option<String>,
        /// Wait for element before filling (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
        /// Snapshot ref (e.g., e0, e5) from last `app snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
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

    /// Send keyboard hotkey (e.g., Control+A, Control+Shift+ArrowRight)
    Hotkey {
        /// Keys separated by '+' (e.g., "Control+A", "Control+Shift+C")
        keys: String,
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
        /// Extraction mode: raw (innerText) or readability (smart extraction, default)
        #[arg(long, default_value = "readability")]
        mode: String,
    },

    /// Get accessibility snapshot via CDP Accessibility Tree
    Snapshot {
        /// Only show interactive elements (buttons, links, inputs)
        #[arg(short = 'i', long)]
        interactive: bool,
        /// Include cursor-interactive elements (cursor:pointer, onclick, tabindex)
        #[arg(short = 'C', long)]
        cursor: bool,
        /// Remove empty structural elements (generic, group, list, etc.)
        #[arg(short = 'c', long)]
        compact: bool,
        /// Maximum tree depth
        #[arg(short = 'd', long)]
        depth: Option<usize>,
        /// Scope to elements under this CSS selector
        #[arg(short = 's', long)]
        selector: Option<String>,
        /// Output format: compact, json (default: compact)
        #[arg(long, default_value = "compact")]
        format: String,
        /// Show diff from last snapshot (added/changed/removed)
        #[arg(long)]
        diff: bool,
        /// Truncate output to approximately N tokens (for LLM context window management)
        #[arg(long)]
        max_tokens: Option<usize>,
    },

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

    /// Scroll the page
    Scroll {
        #[command(subcommand)]
        direction: ScrollDirection,
        /// Enable smooth scrolling
        #[arg(long)]
        smooth: bool,
        /// Wait for scroll to complete (scrollend event)
        #[arg(long)]
        wait: bool,
    },

    /// Execute a batch of actions from JSON (stdin or file)
    Batch {
        /// Path to JSON file with actions (reads from stdin if omitted)
        #[arg(long)]
        file: Option<String>,
        /// Delay between steps in milliseconds
        #[arg(long, default_value = "50")]
        delay: u64,
    },

    /// Rotate fingerprint (UA, platform, screen, hardware)
    Fingerprint {
        #[command(subcommand)]
        command: FingerprintCommands,
    },

    /// Capture console log messages from the page
    Console {
        /// Duration to listen for messages in milliseconds (0 = snapshot current)
        #[arg(long, default_value = "0")]
        duration: u64,
        /// Filter by log level: all, error, warning, info, log
        #[arg(long, default_value = "all")]
        level: String,
    },

    /// Wait for network to become idle (no pending requests)
    WaitIdle {
        /// Timeout in milliseconds
        #[arg(long, default_value = "30000")]
        timeout: u64,
        /// Idle threshold in milliseconds (no requests for this long)
        #[arg(long, default_value = "500")]
        idle_time: u64,
    },

    /// Get detailed info about an element (bounding box, attributes, styles)
    Info {
        /// CSS selector
        selector: String,
    },

    /// Manage localStorage and sessionStorage
    Storage {
        #[command(subcommand)]
        command: StorageCommands,
    },

    /// Emulate a device (mobile, tablet, desktop presets)
    Emulate {
        /// Device name: iphone-14, iphone-se, pixel-7, ipad, desktop-hd, or custom WxH
        device: String,
    },

    /// Wait for a JavaScript expression to return true
    WaitFn {
        /// JavaScript expression that should return a truthy value
        expression: String,
        /// Timeout in milliseconds
        #[arg(long, default_value = "30000")]
        timeout: u64,
        /// Polling interval in milliseconds
        #[arg(long, default_value = "100")]
        interval: u64,
    },

    /// Upload file(s) to a file input element
    Upload {
        /// File path(s) to upload
        #[arg(required = true)]
        files: Vec<String>,
        /// CSS selector for file input (auto-detects input[type="file"] if omitted)
        #[arg(short = 's', long)]
        selector: Option<String>,
        /// Snapshot ref (e.g., e0) from last `app snapshot`
        #[arg(long, name = "ref")]
        ref_id: Option<String>,
        /// Wait for element before uploading (ms), 0 to skip
        #[arg(long, default_value = "0")]
        wait: u64,
    },

    /// Manage application tabs/windows (list, create, switch, close)
    Tab {
        #[command(subcommand)]
        command: TabCommands,
    },

    /// Switch iframe context
    SwitchFrame {
        /// Target: iframe selector, "parent", or "default" for main frame
        target: String,
    },
}

#[derive(Subcommand, Clone)]
pub enum TabCommands {
    /// List all open tabs/pages
    List,

    /// Create a new tab with optional URL
    New {
        /// Optional URL to open in the new tab
        url: Option<String>,
    },

    /// Switch to a specific tab by ID
    Switch {
        /// Page/tab ID (get from 'tab list')
        page_id: String,
    },

    /// Close a specific tab by ID
    Close {
        /// Page/tab ID to close (defaults to active tab)
        page_id: Option<String>,
    },

    /// Show currently active tab
    Active,
}

#[derive(Subcommand, Clone)]
pub enum FingerprintCommands {
    /// Generate and apply a new random fingerprint
    Rotate {
        /// Target OS: windows, mac, linux, random
        #[arg(long, default_value = "random")]
        os: String,
        /// Target screen resolution (e.g., 1920x1080, random)
        #[arg(long, default_value = "random")]
        screen: String,
    },
}

#[derive(Subcommand, Clone)]
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
    /// Clear all cookies for the current page (or specified domain)
    Clear {
        /// Explicit domain to clear (e.g., "example.com")
        #[arg(long)]
        domain: Option<String>,
        /// Preview which cookies would be cleared without deleting
        #[arg(long)]
        dry_run: bool,
        /// Skip confirmation — required to actually clear
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand, Clone)]
pub enum StorageCommands {
    /// Get a value from localStorage
    Get {
        /// Key to get
        key: String,
        /// Use sessionStorage instead of localStorage
        #[arg(long)]
        session: bool,
    },
    /// Set a value in localStorage
    Set {
        /// Key to set
        key: String,
        /// Value to set
        value: String,
        /// Use sessionStorage instead of localStorage
        #[arg(long)]
        session: bool,
    },
    /// Remove a key from localStorage
    Remove {
        /// Key to remove
        key: String,
        /// Use sessionStorage instead of localStorage
        #[arg(long)]
        session: bool,
    },
    /// Clear all localStorage data
    Clear {
        /// Use sessionStorage instead of localStorage
        #[arg(long)]
        session: bool,
    },
    /// List all keys in localStorage
    List {
        /// Use sessionStorage instead of localStorage
        #[arg(long)]
        session: bool,
    },
}

#[derive(Subcommand, Clone)]
pub enum ScrollDirection {
    /// Scroll down by pixels
    Down {
        /// Number of pixels to scroll (default: one viewport height)
        #[arg(default_value = "0")]
        pixels: i32,
    },

    /// Scroll up by pixels
    Up {
        /// Number of pixels to scroll (default: one viewport height)
        #[arg(default_value = "0")]
        pixels: i32,
    },

    /// Scroll to the bottom of the page
    Bottom,

    /// Scroll to the top of the page
    Top,

    /// Scroll to a specific element
    To {
        /// CSS selector
        selector: String,
        /// Alignment: start, center, end, nearest
        #[arg(long, default_value = "center")]
        align: String,
    },
}

#[derive(Subcommand, Clone)]
pub enum ExtensionCommands {
    #[command(hide = true)]
    /// Start the extension bridge WebSocket server
    ///
    /// Note: The bridge is automatically started when needed by browser commands.
    /// This command is provided for debugging and manual control only.
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
    pub async fn run(&self) -> Result<()> {
        crate::update_notifier::maybe_notify(self).await;

        match &self.command {
            Commands::Browser { command } => commands::browser::run(self, command).await,
            Commands::App { command } => commands::app::run(self, command).await,
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
