//! v2 daemon protocol types.
//!
//! This module defines the typed protocol for CLI/MCP/AI SDK communication
//! with the actionbook daemon. It replaces the v1 raw-CDP pass-through
//! protocol with a structured Action enum, classified error results, and
//! length-prefixed framing.
//!
//! # Module layout
//!
//! - [`types`] — Core newtypes (`SessionId`, `TabId`, `WindowId`) and `Mode` enum
//! - [`action`] — The `Action` enum (CLI-to-daemon Layer 1 protocol)
//! - [`action_result`] — `ActionResult` with recovery-strategy classification
//! - [`backend_op`] — `BackendOp` CDP primitives (daemon-internal, Layer 2)
//! - [`backend`] — `BrowserBackendFactory`/`BackendSession` traits + `LocalBackend`
//! - [`wire`] — `Request`/`Response` structs and length-prefix framing (Layer 3)
//! - [`router`] — Request router: dispatches Actions to global handlers or session actors
//! - [`server`] — UDS server: accepts connections, reads/writes frames, calls router
//! - [`daemon_main`] — Daemon entry point: wires router + server + persistence + recovery

pub mod action;
pub mod action_handler;
pub mod action_result;
pub mod backend;
pub mod backend_op;
pub mod cli_v2;
pub mod client;
pub mod daemon_main;
pub mod formatter;
pub mod persistence;
pub mod recovery;
pub mod registry;
pub mod router;
pub mod server;
pub mod session_actor;
pub mod types;
pub mod wire;
