use serde::{Deserialize, Serialize};

use crate::browser::{interaction, navigation, observation, session, tab};

/// CLI → Daemon action protocol. Each variant wraps the command's Cmd type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    // ── Session lifecycle ──────────────────────────────────────
    StartSession(session::start::Cmd),
    ListSessions(session::list::Cmd),
    SessionStatus(session::status::Cmd),
    Close(session::close::Cmd),
    Restart(session::restart::Cmd),

    // ── Tab management ─────────────────────────────────────────
    NewTab(tab::open::Cmd),
    CloseTab(tab::close::Cmd),
    ListTabs(tab::list::Cmd),

    // ── Navigation ─────────────────────────────────────────────
    Goto(navigation::goto::Cmd),
    Back(navigation::back::Cmd),
    Forward(navigation::forward::Cmd),
    Reload(navigation::reload::Cmd),

    // ── Observation ────────────────────────────────────────────
    Snapshot(observation::snapshot::Cmd),
    Title(observation::title::Cmd),
    Url(observation::url::Cmd),
    Viewport(observation::viewport::Cmd),
    InspectPoint(observation::inspect_point::Cmd),

    // ── Interaction ────────────────────────────────────────────
    Eval(interaction::eval::Cmd),
    Click(interaction::click::Cmd),
    Type(interaction::type_text::Cmd),
    Fill(interaction::fill::Cmd),
    Select(interaction::select::Cmd),
}

impl Action {
    /// Normalized command name for the JSON envelope.
    pub fn command_name(&self) -> &str {
        match self {
            Action::StartSession(_) => session::start::COMMAND_NAME,
            Action::ListSessions(_) => session::list::COMMAND_NAME,
            Action::SessionStatus(_) => session::status::COMMAND_NAME,
            Action::Close(_) => session::close::COMMAND_NAME,
            Action::Restart(_) => session::restart::COMMAND_NAME,
            Action::NewTab(_) => tab::open::COMMAND_NAME,
            Action::CloseTab(_) => tab::close::COMMAND_NAME,
            Action::ListTabs(_) => tab::list::COMMAND_NAME,
            Action::Goto(_) => navigation::goto::COMMAND_NAME,
            Action::Back(_) => navigation::back::COMMAND_NAME,
            Action::Forward(_) => navigation::forward::COMMAND_NAME,
            Action::Reload(_) => navigation::reload::COMMAND_NAME,
            Action::Snapshot(_) => observation::snapshot::COMMAND_NAME,
            Action::Title(_) => observation::title::COMMAND_NAME,
            Action::Url(_) => observation::url::COMMAND_NAME,
            Action::Viewport(_) => observation::viewport::COMMAND_NAME,
            Action::InspectPoint(_) => observation::inspect_point::COMMAND_NAME,
            Action::Eval(_) => interaction::eval::COMMAND_NAME,
            Action::Click(_) => interaction::click::COMMAND_NAME,
            Action::Type(_) => interaction::type_text::COMMAND_NAME,
            Action::Fill(_) => interaction::fill::COMMAND_NAME,
            Action::Select(_) => interaction::select::COMMAND_NAME,
        }
    }
}
