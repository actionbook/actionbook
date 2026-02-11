pub mod backend;
mod discovery;
pub mod extension_backend;
pub mod extension_bridge;
pub mod extension_installer;
pub mod isolated_backend;
pub mod launcher;
pub mod native_messaging;
mod session;
pub mod stealth;

#[allow(unused_imports)]
pub use discovery::{discover_all_browsers, BrowserInfo, BrowserType};
pub use session::{SessionManager, SessionStatus, StealthConfig};
pub use stealth::{build_stealth_profile, stealth_status};

// Re-export stealth page application for external use
#[cfg(feature = "stealth")]
pub use stealth::apply_stealth_to_page;
