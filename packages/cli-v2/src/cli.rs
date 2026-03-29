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
pub enum BrowserCommands {
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
    Back(TabArgs),
    /// Go forward
    Forward(TabArgs),
    /// Reload page
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
    /// Inspect element at coordinates
    InspectPoint(observation::inspect_point::Cmd),
    /// Take screenshot
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
    /// Fill an input field
    Fill(interaction::fill::Cmd),
    /// Type text (keystroke by keystroke)
    Type(interaction::type_text::Cmd),
    /// Select a value from a dropdown
    Select(interaction::select::Cmd),
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
            Self::InspectPoint(cmd) => Action::InspectPoint(cmd.clone()),
            Self::Eval(cmd) => Action::Eval(cmd.clone()),
            Self::Click(cmd) => Action::Click(cmd.clone()),
            Self::Type(cmd) => Action::Type(cmd.clone()),
            Self::Fill(cmd) => Action::Fill(cmd.clone()),
            Self::Select(cmd) => Action::Select(cmd.clone()),
            _ => return None,
        })
    }

    /// Normalized command name for the JSON envelope.
    pub fn command_name(&self) -> &str {
        match self {
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
            Self::InspectPoint(_) => observation::inspect_point::COMMAND_NAME,
            Self::Screenshot { .. } => "browser.screenshot",
            Self::Eval(_) => interaction::eval::COMMAND_NAME,
            Self::Click(_) => interaction::click::COMMAND_NAME,
            Self::Fill(_) => interaction::fill::COMMAND_NAME,
            Self::Type(_) => interaction::type_text::COMMAND_NAME,
            Self::Select(_) => interaction::select::COMMAND_NAME,
        }
    }

    /// Build response context from command args and result.
    pub fn context(&self, result: &ActionResult) -> Option<ResponseContext> {
        match self {
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
            Self::InspectPoint(cmd) => observation::inspect_point::context(cmd, result),
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
            Self::Type(cmd) => interaction::type_text::context(cmd, result),
            Self::Fill(cmd) => interaction::fill::context(cmd, result),
            Self::Select(cmd) => interaction::select::context(cmd, result),
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
}
