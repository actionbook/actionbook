use std::collections::HashMap;
use std::fmt;
use std::process::Child;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::browser::observation::snapshot_transform::RefCache;
use crate::daemon::cdp_session::CdpSession;
use crate::error::CliError;
use crate::types::{Mode, SessionId, TabId};

/// Tab metadata. `id` is Chrome's native CDP target ID.
#[derive(Debug, Clone)]
pub struct TabEntry {
    pub id: TabId,
    pub url: String,
    pub title: String,
}

/// Session entry in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Starting,
    Running,
    Closed,
}

impl SessionState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionState::Starting => write!(f, "starting"),
            SessionState::Running => write!(f, "running"),
            SessionState::Closed => write!(f, "closed"),
        }
    }
}

pub struct SessionEntry {
    pub id: SessionId,
    pub mode: Mode,
    pub headless: bool,
    pub profile: String,
    pub status: SessionState,
    pub cdp_port: Option<u16>,
    pub ws_url: String,
    pub tabs: Vec<TabEntry>,
    pub chrome_process: Option<Child>,
    /// Persistent CDP connection for this session.
    pub cdp: Option<CdpSession>,
    /// Original CDP endpoint for cloud sessions (used for reuse matching & restart).
    pub cdp_endpoint: Option<String>,
    /// Custom headers for cloud CDP connections (e.g. auth tokens).
    pub headers: Vec<(String, String)>,
}

impl SessionEntry {
    pub fn starting(id: SessionId, mode: Mode, headless: bool, profile: String) -> Self {
        Self {
            id,
            mode,
            headless,
            profile,
            status: SessionState::Starting,
            cdp_port: None,
            ws_url: String::new(),
            tabs: Vec::new(),
            chrome_process: None,
            cdp: None,
            cdp_endpoint: None,
            headers: Vec::new(),
        }
    }

    pub fn tabs_count(&self) -> usize {
        self.tabs.len()
    }
}

/// Thread-safe session registry.
pub struct SessionRegistry {
    sessions: HashMap<String, SessionEntry>,
    next_auto_id: u32,
    /// Tab-scoped RefCache for stable snapshot refs. Key: "session_id\0tab_id"
    ref_caches: HashMap<String, RefCache>,
    /// Last known cursor position per tab. Key: "session_id\0tab_id"
    cursor_positions: HashMap<String, (f64, f64)>,
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
            ref_caches: HashMap::new(),
            cursor_positions: HashMap::new(),
        }
    }

    fn has_active_session_id(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .is_some_and(|entry| entry.status.is_active())
    }

    pub fn find_local_session_by_profile(
        &self,
        profile: &str,
        mode: Mode,
    ) -> Option<&SessionEntry> {
        self.sessions.values().find(|entry| {
            entry.mode == mode && entry.profile == profile && entry.status.is_active()
        })
    }

    pub fn find_cloud_session_by_endpoint(&self, endpoint: &str) -> Option<&SessionEntry> {
        self.sessions.values().find(|entry| {
            entry.mode == Mode::Cloud
                && entry.status.is_active()
                && entry.cdp_endpoint.as_deref() == Some(endpoint)
        })
    }

    pub fn generate_session_id(
        &mut self,
        set_id: Option<&str>,
        profile: Option<&str>,
    ) -> Result<SessionId, crate::error::CliError> {
        if let Some(id) = set_id {
            let sid = SessionId::new(id)
                .map_err(|e| crate::error::CliError::InvalidSessionId(e.to_string()))?;
            if self.has_active_session_id(sid.as_str()) {
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
        if self.has_active_session_id(sid.as_str()) {
            let sid = SessionId::auto_generate(self.next_auto_id);
            self.next_auto_id += 1;
            Ok(sid)
        } else {
            Ok(sid)
        }
    }

    pub fn reserve_session_start(
        &mut self,
        set_id: Option<&str>,
        requested_profile: Option<&str>,
        resolved_profile: &str,
        mode: Mode,
        headless: bool,
    ) -> Result<SessionId, CliError> {
        if mode == Mode::Local
            && let Some(existing_id) = self
                .find_local_session_by_profile(resolved_profile, mode)
                .map(|entry| entry.id.to_string())
        {
            return Err(CliError::SessionAlreadyExists(existing_id));
        }

        let session_id = self.generate_session_id(set_id, requested_profile)?;
        self.insert(SessionEntry::starting(
            session_id.clone(),
            mode,
            headless,
            resolved_profile.to_string(),
        ));
        Ok(session_id)
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

    /// Get url and title for a tab.
    pub fn get_tab_url_title(
        &self,
        session_id: &str,
        tab_id: &str,
    ) -> (Option<String>, Option<String>) {
        self.get(session_id)
            .and_then(|entry| entry.tabs.iter().find(|t| t.id.0 == tab_id))
            .map(|tab| (Some(tab.url.clone()), Some(tab.title.clone())))
            .unwrap_or((None, None))
    }

    /// Get or create a tab-scoped RefCache for stable snapshot refs.
    pub fn take_ref_cache(&mut self, session_id: &str, tab_id: &str) -> RefCache {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.remove(&key).unwrap_or_default()
    }

    /// Store a tab-scoped RefCache back after snapshot.
    pub fn put_ref_cache(&mut self, session_id: &str, tab_id: &str, cache: RefCache) {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.insert(key, cache);
    }

    /// Read-only access to a tab-scoped RefCache (no take/put needed).
    pub fn peek_ref_cache(&self, session_id: &str, tab_id: &str) -> Option<&RefCache> {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.get(&key)
    }

    /// Clear the RefCache for a tab (call on navigation/reload/back/forward).
    /// When the page changes, old backendNodeIds are no longer valid.
    pub fn clear_ref_cache(&mut self, session_id: &str, tab_id: &str) {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.remove(&key);
    }

    /// Clear all RefCaches for a session (call on session close/restart).
    pub fn clear_session_ref_caches(&mut self, session_id: &str) {
        let prefix = format!("{}\0", session_id);
        self.ref_caches.retain(|k, _| !k.starts_with(&prefix));
    }

    /// Store the cursor position for a tab.
    pub fn set_cursor_position(&mut self, session_id: &str, tab_id: &str, x: f64, y: f64) {
        let key = format!("{}\0{}", session_id, tab_id);
        self.cursor_positions.insert(key, (x, y));
    }

    /// Get the cursor position for a tab.
    pub fn get_cursor_position(&self, session_id: &str, tab_id: &str) -> Option<(f64, f64)> {
        let key = format!("{}\0{}", session_id, tab_id);
        self.cursor_positions.get(&key).copied()
    }
}

pub type SharedRegistry = Arc<Mutex<SessionRegistry>>;

pub fn new_shared_registry() -> SharedRegistry {
    Arc::new(Mutex::new(SessionRegistry::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_session_start_rejects_second_placeholder_for_same_local_profile() {
        let mut registry = SessionRegistry::new();

        let session_id = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true)
            .expect("reserve first placeholder");

        let entry = registry
            .get(session_id.as_str())
            .expect("placeholder entry should exist");
        assert_eq!(entry.status, SessionState::Starting);

        let err = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true)
            .expect_err("second placeholder should be rejected");

        assert_eq!(err.error_code(), "SESSION_ALREADY_EXISTS");
    }

    #[test]
    fn reserve_session_start_allows_retry_after_placeholder_cleanup() {
        let mut registry = SessionRegistry::new();

        let first = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true)
            .expect("reserve first placeholder");
        registry.remove(first.as_str());

        let second = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true)
            .expect("retry after cleanup should succeed");

        assert_eq!(
            registry.get(second.as_str()).map(|entry| entry.status),
            Some(SessionState::Starting)
        );
    }

    #[test]
    fn reserve_session_start_rejects_set_id_when_profile_occupied() {
        let mut registry = SessionRegistry::new();

        // Create first session with profile "myprofile"
        registry
            .reserve_session_start(
                Some("first-id"),
                Some("myprofile"),
                "myprofile",
                Mode::Local,
                true,
            )
            .expect("reserve first session");

        // Try to create second session with SAME profile but DIFFERENT set-session-id
        let err = registry
            .reserve_session_start(
                Some("second-id"),
                Some("myprofile"),
                "myprofile",
                Mode::Local,
                true,
            )
            .expect_err("should reject: profile already occupied");

        assert_eq!(err.error_code(), "SESSION_ALREADY_EXISTS");
    }

    #[test]
    fn reserve_session_start_ignores_closed_sessions_for_uniqueness() {
        let mut registry = SessionRegistry::new();
        let session_id = SessionId::new("testrace").expect("valid session id");
        let mut entry = SessionEntry::starting(
            session_id.clone(),
            Mode::Local,
            true,
            "testrace".to_string(),
        );
        entry.status = SessionState::Closed;
        registry.insert(entry);

        let next = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true)
            .expect("closed entry should not block new start");

        assert_eq!(next.as_str(), session_id.as_str());
        assert_eq!(
            registry.get(next.as_str()).map(|entry| entry.status),
            Some(SessionState::Starting)
        );
    }
}
