//! In-memory session registry — maps [`SessionId`] to live session handles.
//!
//! The registry is owned by the daemon's request router. It tracks all active
//! sessions, their metadata, and holds the `mpsc::Sender` used to dispatch
//! actions to each session's actor task.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::session_actor::ActionRequest;
use super::types::{Mode, SessionId};

// ---------------------------------------------------------------------------
// SessionState
// ---------------------------------------------------------------------------

/// Lifecycle state of a session, matching the design doc state machine.
///
/// ```text
/// Starting -> Ready <-> Executing
///                 \-> Recovering -> Ready | Lost
///                                          \-> Closed
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Backend is starting (browser launching, WS connecting).
    Starting,
    /// Ready to accept commands.
    Ready,
    /// Currently executing a command.
    Executing,
    /// Attempting to reconnect after a disconnect.
    Recovering,
    /// Connection permanently lost (browser gone, reconnect failed).
    Lost,
    /// Session has been closed (terminal state).
    Closed,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Starting => write!(f, "starting"),
            SessionState::Ready => write!(f, "ready"),
            SessionState::Executing => write!(f, "executing"),
            SessionState::Recovering => write!(f, "recovering"),
            SessionState::Lost => write!(f, "lost"),
            SessionState::Closed => write!(f, "closed"),
        }
    }
}

// ---------------------------------------------------------------------------
// SessionHandle
// ---------------------------------------------------------------------------

/// A handle to a live session actor, held in the registry.
///
/// Contains the channel sender for dispatching actions plus metadata
/// for listing/inspection without sending a message to the actor.
pub struct SessionHandle {
    /// Channel to send actions to the session actor task.
    pub tx: mpsc::Sender<ActionRequest>,
    /// Profile name used to create this session.
    pub profile: String,
    /// Backend mode (Local/Extension/Cloud).
    pub mode: Mode,
    /// Current lifecycle state.
    pub state: SessionState,
    /// Number of tabs in this session (updated by the actor via callback).
    pub tab_count: usize,
    /// When this session was created.
    pub created_at: Instant,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("profile", &self.profile)
            .field("mode", &self.mode)
            .field("state", &self.state)
            .field("tab_count", &self.tab_count)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// SessionSummary
// ---------------------------------------------------------------------------

/// Lightweight snapshot of a session for list/status responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub profile: String,
    pub mode: Mode,
    pub state: SessionState,
    pub tab_count: usize,
    /// Seconds since session creation.
    pub uptime_secs: u64,
}

// ---------------------------------------------------------------------------
// SessionRegistry
// ---------------------------------------------------------------------------

/// In-memory registry of all active sessions.
///
/// Owned by the daemon's main task. Provides CRUD operations and
/// monotonically-increasing [`SessionId`] allocation.
pub struct SessionRegistry {
    sessions: HashMap<SessionId, SessionHandle>,
    next_id: u32,
}

impl SessionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        SessionRegistry {
            sessions: HashMap::new(),
            next_id: 0,
        }
    }

    /// Create a registry with a custom starting ID (useful for crash recovery).
    pub fn with_next_id(next_id: u32) -> Self {
        SessionRegistry {
            sessions: HashMap::new(),
            next_id,
        }
    }

    /// Allocate the next [`SessionId`] without registering a session.
    pub fn next_session_id(&mut self) -> SessionId {
        let id = SessionId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Register a session handle under a specific ID.
    ///
    /// Typically called right after `next_session_id()` once the actor is spawned.
    pub fn register(&mut self, id: SessionId, handle: SessionHandle) {
        self.sessions.insert(id, handle);
    }

    /// Register a session, allocating an ID automatically.
    ///
    /// Returns the assigned [`SessionId`].
    pub fn register_session(&mut self, handle: SessionHandle) -> SessionId {
        let id = self.next_session_id();
        self.sessions.insert(id, handle);
        id
    }

    /// Look up a session by ID.
    pub fn get(&self, id: SessionId) -> Option<&SessionHandle> {
        self.sessions.get(&id)
    }

    /// Get a mutable reference to a session handle.
    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut SessionHandle> {
        self.sessions.get_mut(&id)
    }

    /// Remove a session from the registry, returning it if it existed.
    ///
    /// When the registry becomes empty, `next_id` resets to 0 so the next
    /// session starts at `s0` again.  This keeps IDs predictable for agents
    /// and test harnesses that expect deterministic numbering.
    pub fn remove(&mut self, id: SessionId) -> Option<SessionHandle> {
        let removed = self.sessions.remove(&id);
        if self.sessions.is_empty() {
            self.next_id = 0;
        }
        removed
    }

    /// List all sessions as lightweight summaries.
    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        let now = Instant::now();
        let mut summaries: Vec<_> = self
            .sessions
            .iter()
            .map(|(&id, handle)| SessionSummary {
                id,
                profile: handle.profile.clone(),
                mode: handle.mode,
                state: handle.state,
                tab_count: handle.tab_count,
                uptime_secs: now.duration_since(handle.created_at).as_secs(),
            })
            .collect();
        // Sort by ID for deterministic output.
        summaries.sort_by_key(|s| s.id.0);
        summaries
    }

    /// Number of registered sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn make_handle(profile: &str, mode: Mode) -> (SessionHandle, mpsc::Receiver<ActionRequest>) {
        let (tx, rx) = mpsc::channel(32);
        let handle = SessionHandle {
            tx,
            profile: profile.into(),
            mode,
            state: SessionState::Ready,
            tab_count: 0,
            created_at: Instant::now(),
        };
        (handle, rx)
    }

    #[test]
    fn next_session_id_is_monotonic() {
        let mut reg = SessionRegistry::new();
        assert_eq!(reg.next_session_id(), SessionId(0));
        assert_eq!(reg.next_session_id(), SessionId(1));
        assert_eq!(reg.next_session_id(), SessionId(2));
    }

    #[test]
    fn register_and_get() {
        let mut reg = SessionRegistry::new();
        let (handle, _rx) = make_handle("default", Mode::Local);
        let id = reg.register_session(handle);
        assert_eq!(id, SessionId(0));
        assert!(reg.get(id).is_some());
        assert_eq!(reg.get(id).unwrap().profile, "default");
    }

    #[test]
    fn register_with_explicit_id() {
        let mut reg = SessionRegistry::new();
        let id = reg.next_session_id();
        let (handle, _rx) = make_handle("work", Mode::Cloud);
        reg.register(id, handle);
        assert!(reg.get(id).is_some());
        assert_eq!(reg.get(id).unwrap().mode, Mode::Cloud);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let reg = SessionRegistry::new();
        assert!(reg.get(SessionId(99)).is_none());
    }

    #[test]
    fn remove_session() {
        let mut reg = SessionRegistry::new();
        let (handle, _rx) = make_handle("default", Mode::Local);
        let id = reg.register_session(handle);
        assert_eq!(reg.len(), 1);

        let removed = reg.remove(id);
        assert!(removed.is_some());
        assert_eq!(reg.len(), 0);
        assert!(reg.get(id).is_none());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut reg = SessionRegistry::new();
        assert!(reg.remove(SessionId(0)).is_none());
    }

    #[test]
    fn list_sessions_sorted_by_id() {
        let mut reg = SessionRegistry::new();

        let (h1, _r1) = make_handle("default", Mode::Local);
        let (h2, _r2) = make_handle("ext", Mode::Extension);
        let (h3, _r3) = make_handle("cloud", Mode::Cloud);

        reg.register_session(h1); // s0
        reg.register_session(h2); // s1
        reg.register_session(h3); // s2

        let summaries = reg.list_sessions();
        assert_eq!(summaries.len(), 3);
        assert_eq!(summaries[0].id, SessionId(0));
        assert_eq!(summaries[1].id, SessionId(1));
        assert_eq!(summaries[2].id, SessionId(2));
        assert_eq!(summaries[0].profile, "default");
        assert_eq!(summaries[1].mode, Mode::Extension);
    }

    #[test]
    fn list_empty_registry() {
        let reg = SessionRegistry::new();
        assert!(reg.list_sessions().is_empty());
        assert!(reg.is_empty());
    }

    #[test]
    fn get_mut_updates_state() {
        let mut reg = SessionRegistry::new();
        let (handle, _rx) = make_handle("default", Mode::Local);
        let id = reg.register_session(handle);

        reg.get_mut(id).unwrap().state = SessionState::Executing;
        assert_eq!(reg.get(id).unwrap().state, SessionState::Executing);

        reg.get_mut(id).unwrap().tab_count = 5;
        assert_eq!(reg.get(id).unwrap().tab_count, 5);
    }

    #[test]
    fn with_next_id_starts_from_offset() {
        let mut reg = SessionRegistry::with_next_id(10);
        assert_eq!(reg.next_session_id(), SessionId(10));
        assert_eq!(reg.next_session_id(), SessionId(11));
    }

    #[test]
    fn session_state_display() {
        assert_eq!(SessionState::Ready.to_string(), "ready");
        assert_eq!(SessionState::Executing.to_string(), "executing");
        assert_eq!(SessionState::Lost.to_string(), "lost");
    }

    #[test]
    fn session_state_serde_round_trip() {
        for state in [
            SessionState::Starting,
            SessionState::Ready,
            SessionState::Executing,
            SessionState::Recovering,
            SessionState::Lost,
            SessionState::Closed,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let decoded: SessionState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, decoded);
        }
    }

    #[test]
    fn session_summary_serde_round_trip() {
        let summary = SessionSummary {
            id: SessionId(3),
            profile: "test".into(),
            mode: Mode::Local,
            state: SessionState::Ready,
            tab_count: 2,
            uptime_secs: 120,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: SessionSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, SessionId(3));
        assert_eq!(decoded.tab_count, 2);
        assert_eq!(decoded.uptime_secs, 120);
    }
}
