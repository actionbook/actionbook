use clap::{Args, Parser, Subcommand};

use crate::action::Action;
use crate::action_result::ActionResult;
use crate::browser::{interaction, navigation, observation, session, tab};
use crate::output::ResponseContext;
use crate::setup;

fn tab_context(session: &str, tab: &str) -> Option<ResponseContext> {
    Some(ResponseContext {
        session_id: session.to_string(),
        tab_id: Some(tab.to_string()),
        window_id: None,
        url: None,
        title: None,
    })
}

#[derive(Parser, Debug)]
#[command(
    name = "actionbook",
    about = "Actionbook CLI - Browser automation for AI agents",
    disable_version_flag = true
)]
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
    /// Interactive configuration wizard
    Setup(setup::Cmd),
    /// Show help
    Help,
}

/// Unimplemented tab-level command args.
#[derive(Args, Debug, Clone)]
pub struct TabArgs {
    /// Session ID
    #[arg(long)]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    pub tab: String,
}

#[derive(Subcommand, Debug)]
#[command(disable_help_subcommand = true)]
pub enum BrowserCommands {
    /// Show browser help
    Help,

    // ── Session lifecycle ──────────────────────────────────────
    /// Start or attach a browser session
    Start(session::start::Cmd),
    /// List all active sessions
    ListSessions(session::list::Cmd),
    /// Show session status
    Status(session::status::Cmd),
    /// Close a session
    Close(session::close::Cmd),
    /// Restart a session
    Restart(session::restart::Cmd),

    // ── Tab management ─────────────────────────────────────────
    /// List tabs in a session
    ListTabs(tab::list::Cmd),
    /// Open a new tab
    #[command(alias = "open")]
    NewTab(tab::open::Cmd),
    /// Close a tab
    CloseTab(tab::close::Cmd),

    // ── Navigation ─────────────────────────────────────────────
    /// Navigate to URL
    Goto(navigation::goto::Cmd),
    /// Go back
    #[command(after_help = "\
Examples:
  actionbook browser back --session s1 --tab t1")]
    Back(TabArgs),
    /// Go forward
    #[command(after_help = "\
Examples:
  actionbook browser forward --session s1 --tab t1")]
    Forward(TabArgs),
    /// Reload page
    #[command(after_help = "\
Examples:
  actionbook browser reload --session s1 --tab t1")]
    Reload(TabArgs),

    // ── Observation ────────────────────────────────────────────
    /// Capture accessibility snapshot
    Snapshot(observation::snapshot::Cmd),
    /// Get current page title
    Title(observation::title::Cmd),
    /// Get current page URL
    Url(observation::url::Cmd),
    /// Get viewport dimensions
    Viewport(observation::viewport::Cmd),
    /// Read element or page HTML
    Html(observation::html::Cmd),
    /// Read element or page text
    Text(observation::text::Cmd),
    /// Read element value
    Value(observation::value::Cmd),
    /// Read a named element attribute
    Attr(observation::attr::Cmd),
    /// Read all attributes on an element
    Attrs(observation::attrs::Cmd),
    /// Read an element bounding box
    Box(observation::r#box::Cmd),
    /// Read computed styles for an element
    Styles(observation::styles::Cmd),
    /// Describe element properties and context
    Describe(observation::describe::Cmd),
    /// Get element state
    State(observation::state::Cmd),
    /// Query elements with cardinality constraints
    Query(observation::query::Cmd),
    /// Inspect element at coordinates
    InspectPoint(observation::inspect_point::Cmd),
    /// Save page as PDF
    #[command(after_help = "\
Examples:
  actionbook browser pdf /tmp/page.pdf --session s1 --tab t1")]
    Pdf(observation::pdf::Cmd),
    /// Take screenshot
    #[command(after_help = "\
Examples:
  actionbook browser screenshot /tmp/page.png --session s1 --tab t1")]
    Screenshot {
        /// Output file path
        path: String,
        /// Session ID
        #[arg(long)]
        session: String,
        /// Tab ID
        #[arg(long)]
        tab: String,
    },

    // ── Interaction ────────────────────────────────────────────
    /// Evaluate JavaScript
    Eval(interaction::eval::Cmd),
    /// Click an element
    Click(interaction::click::Cmd),
    /// Hover over an element
    Hover(interaction::hover::Cmd),
    /// Focus an element
    Focus(interaction::focus::Cmd),
    /// Press a key or key combination
    Press(interaction::press::Cmd),
    /// Fill an input field
    Fill(interaction::fill::Cmd),
    /// Type text (keystroke by keystroke)
    Type(interaction::type_text::Cmd),
    /// Select a value from a dropdown
    Select(interaction::select::Cmd),
    /// Drag an element to a target
    Drag(interaction::drag::Cmd),
    /// Upload files to a file input
    Upload(interaction::upload::Cmd),
    /// Move the mouse to absolute coordinates
    MouseMove(interaction::mouse_move::Cmd),
    /// Get the current cursor position
    CursorPosition(interaction::cursor_position::Cmd),
    /// Scroll the page or a container
    Scroll(interaction::scroll::Cmd),
}

impl BrowserCommands {
    /// Convert to wire Action. Returns None for unimplemented commands.
    pub fn to_action(&self) -> Option<Action> {
        Some(match self {
            Self::Start(cmd) => Action::StartSession(cmd.clone()),
            Self::ListSessions(cmd) => Action::ListSessions(cmd.clone()),
            Self::Status(cmd) => Action::SessionStatus(cmd.clone()),
            Self::Close(cmd) => Action::Close(cmd.clone()),
            Self::Restart(cmd) => Action::Restart(cmd.clone()),
            Self::ListTabs(cmd) => Action::ListTabs(cmd.clone()),
            Self::NewTab(cmd) => Action::NewTab(cmd.clone()),
            Self::CloseTab(cmd) => Action::CloseTab(cmd.clone()),
            Self::Goto(cmd) => Action::Goto(cmd.clone()),
            Self::Back(a) => Action::Back(navigation::back::Cmd {
                session: a.session.clone(),
                tab: a.tab.clone(),
            }),
            Self::Forward(a) => Action::Forward(navigation::forward::Cmd {
                session: a.session.clone(),
                tab: a.tab.clone(),
            }),
            Self::Reload(a) => Action::Reload(navigation::reload::Cmd {
                session: a.session.clone(),
                tab: a.tab.clone(),
            }),
            Self::Snapshot(cmd) => Action::Snapshot(cmd.clone()),
            Self::Title(cmd) => Action::Title(cmd.clone()),
            Self::Url(cmd) => Action::Url(cmd.clone()),
            Self::Viewport(cmd) => Action::Viewport(cmd.clone()),
            Self::Html(cmd) => Action::Html(cmd.clone()),
            Self::Text(cmd) => Action::Text(cmd.clone()),
            Self::Value(cmd) => Action::Value(cmd.clone()),
            Self::Attr(cmd) => Action::Attr(cmd.clone()),
            Self::Attrs(cmd) => Action::Attrs(cmd.clone()),
            Self::Box(cmd) => Action::Box(cmd.clone()),
            Self::Styles(cmd) => Action::Styles(cmd.clone()),
            Self::Describe(cmd) => Action::Describe(cmd.clone()),
            Self::State(cmd) => Action::State(cmd.clone()),
            Self::Query(cmd) => Action::Query(cmd.clone()),
            Self::InspectPoint(cmd) => Action::InspectPoint(cmd.clone()),
            Self::Pdf(cmd) => Action::Pdf(cmd.clone()),
            Self::Eval(cmd) => Action::Eval(cmd.clone()),
            Self::Click(cmd) => Action::Click(cmd.clone()),
            Self::Hover(cmd) => Action::Hover(cmd.clone()),
            Self::Focus(cmd) => Action::Focus(cmd.clone()),
            Self::Press(cmd) => Action::Press(cmd.clone()),
            Self::Type(cmd) => Action::Type(cmd.clone()),
            Self::Fill(cmd) => Action::Fill(cmd.clone()),
            Self::Select(cmd) => Action::Select(cmd.clone()),
            Self::Drag(cmd) => Action::Drag(cmd.clone()),
            Self::Upload(cmd) => Action::Upload(cmd.clone()),
            Self::MouseMove(cmd) => Action::MouseMove(cmd.clone()),
            Self::CursorPosition(cmd) => Action::CursorPosition(cmd.clone()),
            Self::Scroll(cmd) => Action::Scroll(cmd.clone()),
            _ => return None,
        })
    }

    /// Normalized command name for the JSON envelope.
    pub fn command_name(&self) -> &str {
        match self {
            Self::Help => "help",
            Self::Start(_) => session::start::COMMAND_NAME,
            Self::ListSessions(_) => session::list::COMMAND_NAME,
            Self::Status(_) => session::status::COMMAND_NAME,
            Self::Close(_) => session::close::COMMAND_NAME,
            Self::Restart(_) => session::restart::COMMAND_NAME,
            Self::ListTabs(_) => tab::list::COMMAND_NAME,
            Self::NewTab(_) => tab::open::COMMAND_NAME,
            Self::CloseTab(_) => tab::close::COMMAND_NAME,
            Self::Goto(_) => navigation::goto::COMMAND_NAME,
            Self::Back(_) => "browser.back",
            Self::Forward(_) => "browser.forward",
            Self::Reload(_) => "browser.reload",
            Self::Snapshot(_) => observation::snapshot::COMMAND_NAME,
            Self::Title(_) => observation::title::COMMAND_NAME,
            Self::Url(_) => observation::url::COMMAND_NAME,
            Self::Viewport(_) => observation::viewport::COMMAND_NAME,
            Self::Html(_) => observation::html::COMMAND_NAME,
            Self::Text(_) => observation::text::COMMAND_NAME,
            Self::Value(_) => observation::value::COMMAND_NAME,
            Self::Attr(_) => observation::attr::COMMAND_NAME,
            Self::Attrs(_) => observation::attrs::COMMAND_NAME,
            Self::Box(_) => observation::r#box::COMMAND_NAME,
            Self::Styles(_) => observation::styles::COMMAND_NAME,
            Self::Describe(_) => observation::describe::COMMAND_NAME,
            Self::State(_) => observation::state::COMMAND_NAME,
            Self::Query(_) => observation::query::COMMAND_NAME,
            Self::InspectPoint(_) => observation::inspect_point::COMMAND_NAME,
            Self::Pdf(_) => observation::pdf::COMMAND_NAME,
            Self::Screenshot { .. } => "browser.screenshot",
            Self::Eval(_) => interaction::eval::COMMAND_NAME,
            Self::Click(_) => interaction::click::COMMAND_NAME,
            Self::Hover(_) => interaction::hover::COMMAND_NAME,
            Self::Focus(_) => interaction::focus::COMMAND_NAME,
            Self::Press(_) => interaction::press::COMMAND_NAME,
            Self::Fill(_) => interaction::fill::COMMAND_NAME,
            Self::Type(_) => interaction::type_text::COMMAND_NAME,
            Self::Select(_) => interaction::select::COMMAND_NAME,
            Self::Drag(_) => interaction::drag::COMMAND_NAME,
            Self::Upload(_) => interaction::upload::COMMAND_NAME,
            Self::MouseMove(_) => interaction::mouse_move::COMMAND_NAME,
            Self::CursorPosition(_) => interaction::cursor_position::COMMAND_NAME,
            Self::Scroll(_) => interaction::scroll::COMMAND_NAME,
        }
    }

    /// Build response context from command args and result.
    pub fn context(&self, result: &ActionResult) -> Option<ResponseContext> {
        match self {
            Self::Help => None,
            Self::Start(cmd) => session::start::context(cmd, result),
            Self::ListSessions(cmd) => session::list::context(cmd, result),
            Self::Status(cmd) => session::status::context(cmd, result),
            Self::Close(cmd) => session::close::context(cmd, result),
            Self::Restart(cmd) => session::restart::context(cmd, result),
            Self::ListTabs(cmd) => tab::list::context(cmd, result),
            Self::NewTab(cmd) => tab::open::context(cmd, result),
            Self::CloseTab(cmd) => tab::close::context(cmd, result),
            Self::Goto(cmd) => navigation::goto::context(cmd, result),
            Self::Snapshot(cmd) => observation::snapshot::context(cmd, result),
            Self::Title(cmd) => observation::title::context(cmd, result),
            Self::Url(cmd) => observation::url::context(cmd, result),
            Self::Viewport(cmd) => observation::viewport::context(cmd, result),
            Self::Html(cmd) => observation::html::context(cmd, result),
            Self::Text(cmd) => observation::text::context(cmd, result),
            Self::Value(cmd) => observation::value::context(cmd, result),
            Self::Attr(cmd) => observation::attr::context(cmd, result),
            Self::Attrs(cmd) => observation::attrs::context(cmd, result),
            Self::Box(cmd) => observation::r#box::context(cmd, result),
            Self::Styles(cmd) => observation::styles::context(cmd, result),
            Self::Describe(cmd) => observation::describe::context(cmd, result),
            Self::State(cmd) => observation::state::context(cmd, result),
            Self::Query(cmd) => observation::query::context(cmd, result),
            Self::InspectPoint(cmd) => observation::inspect_point::context(cmd, result),
            Self::Pdf(cmd) => observation::pdf::context(cmd, result),
            Self::Eval(cmd) => interaction::eval::context(cmd, result),
            Self::Back(a) => navigation::back::context(
                &navigation::back::Cmd {
                    session: a.session.clone(),
                    tab: a.tab.clone(),
                },
                result,
            ),
            Self::Forward(a) => navigation::forward::context(
                &navigation::forward::Cmd {
                    session: a.session.clone(),
                    tab: a.tab.clone(),
                },
                result,
            ),
            Self::Reload(a) => navigation::reload::context(
                &navigation::reload::Cmd {
                    session: a.session.clone(),
                    tab: a.tab.clone(),
                },
                result,
            ),
            Self::Click(cmd) => interaction::click::context(cmd, result),
            Self::Hover(cmd) => interaction::hover::context(cmd, result),
            Self::Focus(cmd) => interaction::focus::context(cmd, result),
            Self::Press(cmd) => interaction::press::context(cmd, result),
            Self::Type(cmd) => interaction::type_text::context(cmd, result),
            Self::Fill(cmd) => interaction::fill::context(cmd, result),
            Self::Select(cmd) => interaction::select::context(cmd, result),
            Self::Drag(cmd) => interaction::drag::context(cmd, result),
            Self::Upload(cmd) => interaction::upload::context(cmd, result),
            Self::MouseMove(cmd) => interaction::mouse_move::context(cmd, result),
            Self::CursorPosition(cmd) => interaction::cursor_position::context(cmd, result),
            Self::Scroll(cmd) => interaction::scroll::context(cmd, result),
            Self::Screenshot { session, tab, .. } => tab_context(session, tab),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_parse_from_parses_setup_non_interactive_flags() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "setup",
            "--target",
            "codex",
            "--api-key",
            "sk-test",
            "--browser",
            "local",
            "--non-interactive",
            "--reset",
        ])
        .expect("parse setup");

        match cli.command {
            Some(Commands::Setup(cmd)) => {
                assert_eq!(cmd.target.as_deref(), Some("codex"));
                assert_eq!(cmd.api_key.as_deref(), Some("sk-test"));
                assert_eq!(cmd.browser.as_deref(), Some("local"));
                assert!(cmd.non_interactive);
                assert!(cmd.reset);
            }
            other => panic!("expected setup command, got {other:?}"),
        }
    }

    #[test]
    fn try_parse_from_accepts_browser_hover_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "hover",
            "#submit",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser hover command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_focus_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "focus",
            "#submit",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser focus command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_press_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "press",
            "Enter",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser press command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_drag_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "drag",
            "#source",
            "#target",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser drag command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_upload_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "upload",
            "#file-input",
            "/tmp/example.txt",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser upload command should parse");
    }

    #[test]
    fn try_parse_from_rejects_browser_upload_without_files() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "upload",
            "#file-input",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(
            cli.is_err(),
            "browser upload should require at least one file"
        );
    }

    #[test]
    fn try_parse_from_accepts_browser_eval_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "eval",
            "2 + 2",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser eval command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_mouse_move_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser mouse-move command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_cursor_position_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "cursor-position",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser cursor-position command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_scroll_direction_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "scroll",
            "down",
            "180",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
        ]);

        assert!(cli.is_ok(), "browser scroll direction command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_scroll_edge_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "scroll",
            "bottom",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
            "--container",
            "#scroll-box",
        ]);

        assert!(cli.is_ok(), "browser scroll edge command should parse");
    }

    #[test]
    fn try_parse_from_accepts_browser_scroll_into_view_command() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "scroll",
            "into-view",
            "#target",
            "--session",
            "session-1",
            "--tab",
            "tab-1",
            "--align",
            "center",
        ]);

        assert!(cli.is_ok(), "browser scroll into-view command should parse");
    }

    #[test]
    fn try_parse_from_parses_html_without_selector() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "html",
            "--session",
            "s1",
            "--tab",
            "t1",
        ])
        .expect("parse html");

        match cli.command {
            Some(Commands::Browser {
                command: BrowserCommands::Html(cmd),
            }) => {
                assert_eq!(cmd.selector, None);
                assert_eq!(cmd.session, "s1");
                assert_eq!(cmd.tab, "t1");
            }
            other => panic!("expected browser html command, got {other:?}"),
        }
    }

    #[test]
    fn try_parse_from_parses_attr_selector_and_name() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "attr",
            "#email",
            "aria-label",
            "--session",
            "s1",
            "--tab",
            "t1",
        ])
        .expect("parse attr");

        match cli.command {
            Some(Commands::Browser {
                command: BrowserCommands::Attr(cmd),
            }) => {
                assert_eq!(cmd.selector, "#email");
                assert_eq!(cmd.name, "aria-label");
                assert_eq!(cmd.session, "s1");
                assert_eq!(cmd.tab, "t1");
            }
            other => panic!("expected browser attr command, got {other:?}"),
        }
    }

    #[test]
    fn try_parse_from_parses_styles_names_after_options() {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "styles",
            "#target",
            "--session",
            "s1",
            "--tab",
            "t1",
            "color",
            "backgroundColor",
            "z-index",
        ])
        .expect("parse styles");

        match cli.command {
            Some(Commands::Browser {
                command: BrowserCommands::Styles(cmd),
            }) => {
                assert_eq!(cmd.selector, "#target");
                assert_eq!(cmd.session, "s1");
                assert_eq!(cmd.tab, "t1");
                assert_eq!(cmd.names, vec!["color", "backgroundColor", "z-index"]);
            }
            other => panic!("expected browser styles command, got {other:?}"),
        }
    }
}
