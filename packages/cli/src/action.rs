use serde::{Deserialize, Serialize};

use crate::browser::{cookies, interaction, navigation, observation, session, storage, tab, wait};
use crate::extension;

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
    BatchOpen(tab::batch_open::Cmd),
    CloseTab(tab::close::Cmd),
    ListTabs(tab::list::Cmd),

    // ── Navigation ─────────────────────────────────────────────
    Goto(navigation::goto::Cmd),
    Back(navigation::back::Cmd),
    Forward(navigation::forward::Cmd),
    Reload(navigation::reload::Cmd),

    // ── Observation ────────────────────────────────────────────
    BatchSnapshot(observation::batch_snapshot::Cmd),
    Snapshot(observation::snapshot::Cmd),
    Screenshot(observation::screenshot::Cmd),
    Title(observation::title::Cmd),
    Url(observation::url::Cmd),
    Viewport(observation::viewport::Cmd),
    Html(observation::html::Cmd),
    Text(observation::text::Cmd),
    Value(observation::value::Cmd),
    Attr(observation::attr::Cmd),
    Attrs(observation::attrs::Cmd),
    Box(observation::r#box::Cmd),
    Styles(observation::styles::Cmd),
    Describe(observation::describe::Cmd),
    State(observation::state::Cmd),
    Query(observation::query::Cmd),
    InspectPoint(observation::inspect_point::Cmd),
    Pdf(observation::pdf::Cmd),
    LogsConsole(observation::logs_console::Cmd),
    LogsErrors(observation::logs_errors::Cmd),
    NetworkRequests(observation::network_requests::Cmd),
    NetworkRequestDetail(observation::network_request_detail::Cmd),
    NetworkHarStart(observation::network_har::StartCmd),
    NetworkHarStop(observation::network_har::StopCmd),

    // ── Cookies ────────────────────────────────────────────────
    CookiesList(cookies::list::Cmd),
    CookiesGet(cookies::get::Cmd),
    CookiesSet(cookies::set::Cmd),
    CookiesDelete(cookies::delete::Cmd),
    CookiesClear(cookies::clear::Cmd),

    // ── Storage ────────────────────────────────────────────────
    StorageList(storage::list::Cmd),
    StorageGet(storage::get::Cmd),
    StorageSet(storage::set::Cmd),
    StorageDelete(storage::delete::Cmd),
    StorageClear(storage::clear::Cmd),

    // ── Wait ───────────────────────────────────────────────────
    WaitElement(wait::element::Cmd),
    WaitNavigation(wait::navigation::Cmd),
    WaitNetworkIdle(wait::network_idle::Cmd),
    WaitCondition(wait::condition::Cmd),

    // ── Extension ──────────────────────────────────────────────
    ExtensionStatus(extension::status::Cmd),

    // ── Interaction ────────────────────────────────────────────
    Eval(interaction::eval::Cmd),
    Click(interaction::click::Cmd),
    BatchClick(interaction::batch_click::Cmd),
    Hover(interaction::hover::Cmd),
    Focus(interaction::focus::Cmd),
    Press(interaction::press::Cmd),
    Type(interaction::type_text::Cmd),
    Fill(interaction::fill::Cmd),
    Select(interaction::select::Cmd),
    Drag(interaction::drag::Cmd),
    Upload(interaction::upload::Cmd),
    MouseMove(interaction::mouse_move::Cmd),
    CursorPosition(interaction::cursor_position::Cmd),
    Scroll(interaction::scroll::Cmd),
}

impl Action {
    /// Extract session/tab addressing for log lines.
    ///
    /// Returns e.g. `"s0/t1"`, `"s0"`, or `"-"` (for list-sessions).
    pub fn session_tab_label(&self) -> String {
        // Helper: most commands carry (session, tab).
        macro_rules! st {
            ($cmd:expr) => {
                format!("{}/{}", $cmd.session, $cmd.tab)
            };
        }
        macro_rules! s_only {
            ($cmd:expr) => {
                $cmd.session.clone()
            };
        }

        match self {
            // Extension (no session/tab)
            Action::ExtensionStatus(_) => "-".into(),

            // Session-level (no tab)
            Action::StartSession(_) | Action::ListSessions(_) => "-".into(),
            Action::SessionStatus(c) => s_only!(c),
            Action::Close(c) => s_only!(c),
            Action::Restart(c) => s_only!(c),

            // Tab management
            Action::NewTab(c) => s_only!(c),
            Action::BatchOpen(c) => s_only!(c),
            Action::CloseTab(c) => st!(c),
            Action::ListTabs(c) => s_only!(c),

            // Navigation
            Action::Goto(c) => st!(c),
            Action::Back(c) => st!(c),
            Action::Forward(c) => st!(c),
            Action::Reload(c) => st!(c),

            // Observation
            Action::BatchSnapshot(c) => c.session.clone(),
            Action::Snapshot(c) => st!(c),
            Action::Screenshot(c) => st!(c),
            Action::Title(c) => st!(c),
            Action::Url(c) => st!(c),
            Action::Viewport(c) => st!(c),
            Action::Html(c) => st!(c),
            Action::Text(c) => st!(c),
            Action::Value(c) => st!(c),
            Action::Attr(c) => st!(c),
            Action::Attrs(c) => st!(c),
            Action::Box(c) => st!(c),
            Action::Styles(c) => st!(c),
            Action::Describe(c) => st!(c),
            Action::State(c) => st!(c),
            Action::Query(c) => format!("{}/{}", c.session(), c.tab()),
            Action::InspectPoint(c) => st!(c),
            Action::Pdf(c) => st!(c),
            Action::LogsConsole(c) => st!(c),
            Action::LogsErrors(c) => st!(c),
            Action::NetworkRequests(c) => st!(c),
            Action::NetworkRequestDetail(c) => st!(c),
            Action::NetworkHarStart(c) => st!(c),
            Action::NetworkHarStop(c) => st!(c),

            // Cookies (session-level, no tab)
            Action::CookiesList(c) => s_only!(c),
            Action::CookiesGet(c) => s_only!(c),
            Action::CookiesSet(c) => s_only!(c),
            Action::CookiesDelete(c) => s_only!(c),
            Action::CookiesClear(c) => s_only!(c),

            // Storage
            Action::StorageList(c) => st!(c),
            Action::StorageGet(c) => st!(c),
            Action::StorageSet(c) => st!(c),
            Action::StorageDelete(c) => st!(c),
            Action::StorageClear(c) => st!(c),

            // Wait
            Action::WaitElement(c) => st!(c),
            Action::WaitNavigation(c) => st!(c),
            Action::WaitNetworkIdle(c) => st!(c),
            Action::WaitCondition(c) => st!(c),

            // Interaction
            Action::Eval(c) => st!(c),
            Action::Click(c) => st!(c),
            Action::BatchClick(c) => st!(c),
            Action::Hover(c) => st!(c),
            Action::Focus(c) => st!(c),
            Action::Press(c) => st!(c),
            Action::Type(c) => st!(c),
            Action::Fill(c) => st!(c),
            Action::Select(c) => st!(c),
            Action::Drag(c) => st!(c),
            Action::Upload(c) => st!(c),
            Action::MouseMove(c) => st!(c),
            Action::CursorPosition(c) => st!(c),
            Action::Scroll(c) => st!(c),
        }
    }

    /// Normalized command name for the JSON envelope.
    pub fn command_name(&self) -> &str {
        match self {
            Action::ExtensionStatus(_) => extension::status::COMMAND_NAME,
            Action::StartSession(_) => session::start::COMMAND_NAME,
            Action::ListSessions(_) => session::list::COMMAND_NAME,
            Action::SessionStatus(_) => session::status::COMMAND_NAME,
            Action::Close(_) => session::close::COMMAND_NAME,
            Action::Restart(_) => session::restart::COMMAND_NAME,
            Action::NewTab(_) => tab::open::COMMAND_NAME,
            Action::BatchOpen(_) => tab::batch_open::COMMAND_NAME,
            Action::CloseTab(_) => tab::close::COMMAND_NAME,
            Action::ListTabs(_) => tab::list::COMMAND_NAME,
            Action::Goto(_) => navigation::goto::COMMAND_NAME,
            Action::Back(_) => navigation::back::COMMAND_NAME,
            Action::Forward(_) => navigation::forward::COMMAND_NAME,
            Action::Reload(_) => navigation::reload::COMMAND_NAME,
            Action::BatchSnapshot(_) => observation::batch_snapshot::COMMAND_NAME,
            Action::Snapshot(_) => observation::snapshot::COMMAND_NAME,
            Action::Screenshot(_) => observation::screenshot::COMMAND_NAME,
            Action::Title(_) => observation::title::COMMAND_NAME,
            Action::Url(_) => observation::url::COMMAND_NAME,
            Action::Viewport(_) => observation::viewport::COMMAND_NAME,
            Action::Html(_) => observation::html::COMMAND_NAME,
            Action::Text(_) => observation::text::COMMAND_NAME,
            Action::Value(_) => observation::value::COMMAND_NAME,
            Action::Attr(_) => observation::attr::COMMAND_NAME,
            Action::Attrs(_) => observation::attrs::COMMAND_NAME,
            Action::Box(_) => observation::r#box::COMMAND_NAME,
            Action::Styles(_) => observation::styles::COMMAND_NAME,
            Action::Describe(_) => observation::describe::COMMAND_NAME,
            Action::State(_) => observation::state::COMMAND_NAME,
            Action::Query(_) => observation::query::COMMAND_NAME,
            Action::InspectPoint(_) => observation::inspect_point::COMMAND_NAME,
            Action::Pdf(_) => observation::pdf::COMMAND_NAME,
            Action::LogsConsole(_) => observation::logs_console::COMMAND_NAME,
            Action::LogsErrors(_) => observation::logs_errors::COMMAND_NAME,
            Action::NetworkRequests(_) => observation::network_requests::COMMAND_NAME,
            Action::NetworkRequestDetail(_) => observation::network_request_detail::COMMAND_NAME,
            Action::NetworkHarStart(_) => observation::network_har::START_COMMAND_NAME,
            Action::NetworkHarStop(_) => observation::network_har::STOP_COMMAND_NAME,
            Action::CookiesList(_) => cookies::list::COMMAND_NAME,
            Action::CookiesGet(_) => cookies::get::COMMAND_NAME,
            Action::CookiesSet(_) => cookies::set::COMMAND_NAME,
            Action::CookiesDelete(_) => cookies::delete::COMMAND_NAME,
            Action::CookiesClear(_) => cookies::clear::COMMAND_NAME,
            Action::StorageList(cmd) => storage::list::command_name(cmd.kind),
            Action::StorageGet(cmd) => storage::get::command_name(cmd.kind),
            Action::StorageSet(cmd) => storage::set::command_name(cmd.kind),
            Action::StorageDelete(cmd) => storage::delete::command_name(cmd.kind),
            Action::StorageClear(cmd) => storage::clear::command_name(cmd.kind),
            Action::WaitElement(_) => wait::element::COMMAND_NAME,
            Action::WaitNavigation(_) => wait::navigation::COMMAND_NAME,
            Action::WaitNetworkIdle(_) => wait::network_idle::COMMAND_NAME,
            Action::WaitCondition(_) => wait::condition::COMMAND_NAME,
            Action::Eval(_) => interaction::eval::COMMAND_NAME,
            Action::Click(_) => interaction::click::COMMAND_NAME,
            Action::BatchClick(_) => interaction::batch_click::COMMAND_NAME,
            Action::Hover(_) => interaction::hover::COMMAND_NAME,
            Action::Focus(_) => interaction::focus::COMMAND_NAME,
            Action::Press(_) => interaction::press::COMMAND_NAME,
            Action::Type(_) => interaction::type_text::COMMAND_NAME,
            Action::Fill(_) => interaction::fill::COMMAND_NAME,
            Action::Select(_) => interaction::select::COMMAND_NAME,
            Action::Drag(_) => interaction::drag::COMMAND_NAME,
            Action::Upload(_) => interaction::upload::COMMAND_NAME,
            Action::MouseMove(_) => interaction::mouse_move::COMMAND_NAME,
            Action::CursorPosition(_) => interaction::cursor_position::COMMAND_NAME,
            Action::Scroll(_) => interaction::scroll::COMMAND_NAME,
        }
    }
}
