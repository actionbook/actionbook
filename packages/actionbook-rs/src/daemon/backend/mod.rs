//! Browser backend abstraction layer.
//!
//! Defines the [`BrowserBackendFactory`] and [`BackendSession`] traits that
//! abstract over different browser connection modes (Local, Extension, Cloud).
//!
//! The daemon's session actor interacts exclusively through these traits;
//! concrete implementations live in submodules:
//! - [`local`] — Launch and control a local Chrome process via CDP over `ws://`
//! - [`extension`] — Connect to user's Chrome via extension bridge WS
//! - [`cloud`] — Connect to remote browser via `wss://`

pub mod cloud;
pub mod extension;
pub mod local;
pub mod types;

pub use types::*;

use async_trait::async_trait;
use futures::stream::BoxStream;

use super::backend_op::BackendOp;
use crate::error::Result;

// ---------------------------------------------------------------------------
// BrowserBackendFactory
// ---------------------------------------------------------------------------

/// Factory for creating backend sessions.
///
/// Each backend kind (Local, Extension, Cloud) provides one factory instance.
/// The daemon uses it to start new sessions, attach to existing ones, or
/// resume from a persisted checkpoint after a crash.
#[async_trait]
pub trait BrowserBackendFactory: Send + Sync {
    /// Which backend kind this factory produces.
    #[allow(dead_code)]
    fn kind(&self) -> BackendKind;

    /// Declare capabilities of this backend.
    #[allow(dead_code)]
    fn capabilities(&self) -> Capabilities;

    /// Start a new browser session (launch a process, connect WS, etc.).
    async fn start(&self, spec: StartSpec) -> Result<Box<dyn BackendSession>>;

    /// Attach to an already-running browser by WS URL.
    async fn attach(&self, spec: AttachSpec) -> Result<Box<dyn BackendSession>>;

    /// Resume a previously checkpointed session after daemon restart.
    #[allow(dead_code)]
    async fn resume(&self, cp: Checkpoint) -> Result<Box<dyn BackendSession>>;
}

// ---------------------------------------------------------------------------
// BackendSession
// ---------------------------------------------------------------------------

/// A live connection to a browser instance.
///
/// Held by a session actor for the lifetime of the session. All methods are
/// `&mut self` — the session actor serializes calls through its channel.
///
/// # Cancellation safety
///
/// All async methods must be cancellation-safe: dropping the future before
/// completion must not leave the session in an inconsistent state.
#[async_trait]
pub trait BackendSession: Send {
    /// Stream of backend events (disconnect, target created/destroyed, dialog).
    ///
    /// The caller should `take()` this once and poll it in a select loop.
    /// Returns an empty stream if already taken.
    #[allow(dead_code)]
    fn events(&mut self) -> BoxStream<'static, BackendEvent>;

    /// Execute a CDP-level operation and return the raw result.
    async fn exec(&mut self, op: BackendOp) -> Result<OpResult>;

    /// List all CDP targets (tabs, service workers, etc.).
    async fn list_targets(&self) -> Result<Vec<TargetInfo>>;

    /// Produce a checkpoint for crash recovery.
    #[allow(dead_code)]
    async fn checkpoint(&self) -> Result<Checkpoint>;

    /// Check whether the browser connection is alive.
    #[allow(dead_code)]
    async fn health(&self) -> Result<Health>;

    /// Shut down the browser according to the given policy.
    async fn shutdown(&mut self, policy: ShutdownPolicy) -> Result<()>;
}
