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

#[allow(unused_imports)]
pub use client::CamofoxClient;
#[allow(unused_imports)]
pub use session::CamofoxSession;
#[allow(unused_imports)]
pub use snapshot::AccessibilityTreeExt;
#[allow(unused_imports)]
pub use types::{
    AccessibilityNode, ClickRequest, CreateTabRequest, CreateTabResponse, NavigateRequest,
    SnapshotResponse, TypeTextRequest,
};
