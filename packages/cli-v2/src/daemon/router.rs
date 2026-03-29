use crate::action::Action;
use crate::action_result::ActionResult;
use crate::browser;

use super::registry::SharedRegistry;

/// Route an action to the appropriate handler.
pub async fn route(action: &Action, registry: &SharedRegistry) -> ActionResult {
    match action {
        Action::StartSession(cmd) => browser::session::start::execute(cmd, registry).await,
        Action::ListSessions(cmd) => browser::session::list::execute(cmd, registry).await,
        Action::SessionStatus(cmd) => browser::session::status::execute(cmd, registry).await,
        Action::Close(cmd) => browser::session::close::execute(cmd, registry).await,
        Action::Restart(cmd) => browser::session::restart::execute(cmd, registry).await,
        Action::Goto(cmd) => browser::navigation::goto::execute(cmd, registry).await,
        Action::Back(cmd) => browser::navigation::back::execute(cmd, registry).await,
        Action::Forward(cmd) => browser::navigation::forward::execute(cmd, registry).await,
        Action::Reload(cmd) => browser::navigation::reload::execute(cmd, registry).await,
        Action::ListTabs(cmd) => browser::tab::list::execute(cmd, registry).await,
        Action::NewTab(cmd) => browser::tab::open::execute(cmd, registry).await,
        Action::CloseTab(cmd) => browser::tab::close::execute(cmd, registry).await,
        Action::Snapshot(cmd) => browser::observation::snapshot::execute(cmd, registry).await,
        Action::Title(cmd) => browser::observation::title::execute(cmd, registry).await,
        Action::Url(cmd) => browser::observation::url::execute(cmd, registry).await,
        Action::Viewport(cmd) => browser::observation::viewport::execute(cmd, registry).await,
        Action::Html(cmd) => browser::observation::html::execute(cmd, registry).await,
        Action::Text(cmd) => browser::observation::text::execute(cmd, registry).await,
        Action::Value(cmd) => browser::observation::value::execute(cmd, registry).await,
        Action::Attr(cmd) => browser::observation::attr::execute(cmd, registry).await,
        Action::InspectPoint(cmd) => {
            browser::observation::inspect_point::execute(cmd, registry).await
        }
        Action::Eval(cmd) => browser::interaction::eval::execute(cmd, registry).await,
        Action::Click(cmd) => browser::interaction::click::execute(cmd, registry).await,
        Action::Hover(cmd) => browser::interaction::hover::execute(cmd, registry).await,
        Action::Focus(cmd) => browser::interaction::focus::execute(cmd, registry).await,
        Action::Press(cmd) => browser::interaction::press::execute(cmd, registry).await,
        Action::Type(cmd) => browser::interaction::type_text::execute(cmd, registry).await,
        Action::Fill(cmd) => browser::interaction::fill::execute(cmd, registry).await,
        Action::Select(cmd) => browser::interaction::select::execute(cmd, registry).await,
        Action::Drag(cmd) => browser::interaction::drag::execute(cmd, registry).await,
        Action::Upload(cmd) => browser::interaction::upload::execute(cmd, registry).await,
        Action::MouseMove(cmd) => browser::interaction::mouse_move::execute(cmd, registry).await,
        Action::CursorPosition(cmd) => {
            browser::interaction::cursor_position::execute(cmd, registry).await
        }
        Action::Scroll(cmd) => browser::interaction::scroll::execute(cmd, registry).await,
    }
}
