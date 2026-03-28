use std::collections::HashMap;
use std::process::Child;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::daemon::cdp_session::CdpSession;
use crate::types::{Mode, SessionId, TabId};

/// Tab metadata. `id` is Chrome's native CDP target ID.
#[derive(Debug, Clone)]
pub struct TabEntry {
    pub id: TabId,
    pub url: String,
    pub title: String,
}

/// Session entry in the registry.
pub struct SessionEntry {
    pub id: SessionId,
    pub mode: Mode,
    pub headless: bool,
    pub profile: String,
    pub status: String, // "running", "closed"
    pub cdp_port: u16,
    pub ws_url: String,
    pub tabs: Vec<TabEntry>,
    pub chrome_process: Option<Child>,
    /// Persistent CDP connection for this session.
    pub cdp: Option<CdpSession>,
}

impl SessionEntry {
    pub fn tabs_count(&self) -> usize {
        self.tabs.len()
    }
}

/// Thread-safe session registry.
pub struct SessionRegistry {
    sessions: HashMap<String, SessionEntry>,
    next_auto_id: u32,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRegistry {
    pub fn new() -> Self {
        SessionRegistry {
            sessions: HashMap::new(),
            next_auto_id: 0,
        }
    }

    pub fn generate_session_id(
        &mut self,
        set_id: Option<&str>,
        profile: Option<&str>,
    ) -> Result<SessionId, crate::error::CliError> {
        if let Some(id) = set_id {
            let sid = SessionId::new(id)
                .map_err(|e| crate::error::CliError::InvalidSessionId(e.to_string()))?;
            if self.sessions.contains_key(sid.as_str()) {
                return Err(crate::error::CliError::SessionAlreadyExists(
                    sid.to_string(),
                ));
            }
            return Ok(sid);
        }
        let sid = if let Some(p) = profile {
            SessionId::from_profile(p, self.next_auto_id)
        } else {
            SessionId::auto_generate(self.next_auto_id)
        };
        self.next_auto_id += 1;
        // Handle collision
        if self.sessions.contains_key(sid.as_str()) {
            let sid = SessionId::auto_generate(self.next_auto_id);
            self.next_auto_id += 1;
            Ok(sid)
        } else {
            Ok(sid)
        }
    }

    pub fn insert(&mut self, entry: SessionEntry) {
        self.sessions.insert(entry.id.as_str().to_string(), entry);
    }

    pub fn get(&self, session_id: &str) -> Option<&SessionEntry> {
        self.sessions.get(session_id)
    }

    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut SessionEntry> {
        self.sessions.get_mut(session_id)
    }

    pub fn remove(&mut self, session_id: &str) -> Option<SessionEntry> {
        self.sessions.remove(session_id)
    }

    pub fn list(&self) -> Vec<&SessionEntry> {
        self.sessions.values().collect()
    }
}

pub type SharedRegistry = Arc<Mutex<SessionRegistry>>;

pub fn new_shared_registry() -> SharedRegistry {
    Arc::new(Mutex::new(SessionRegistry::new()))
}
