//! Camoufox browser backend integration
//!
//! This module provides integration with the Camoufox browser via its REST API.
//! Camoufox is a Firefox-based browser optimized for anti-bot circumvention with:
//! - C++-level fingerprint spoofing (105+ properties)
//! - Juggler protocol isolation (completely hides automation)
//! - Accessibility tree responses (5KB vs 500KB HTML)
//! - Stable element refs (e1, e2, e3) instead of brittle CSS selectors

mod client;
mod session;
mod snapshot;
pub mod types;

pub use client::CamofoxClient;
pub use session::CamofoxSession;
pub use snapshot::AccessibilityTreeExt;
pub use types::{
    AccessibilityNode, ClickRequest, CreateTabRequest, CreateTabResponse, NavigateRequest,
    SnapshotResponse, TypeTextRequest,
};
