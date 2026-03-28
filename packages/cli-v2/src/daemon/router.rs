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
        Action::Eval(cmd) => browser::interaction::eval::execute(cmd, registry).await,
    }
}
