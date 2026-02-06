mod discovery;
mod launcher;
mod session;
pub mod stealth;

pub use discovery::{discover_all_browsers, BrowserInfo};
pub use session::{PageInfo, SessionManager, SessionStatus, StealthConfig};
pub use stealth::{stealth_status, build_stealth_profile, parse_stealth_os, parse_stealth_gpu};
pub use stealth::{StealthGpu, StealthOs, StealthProfile};

// Re-export stealth page application for external use
#[cfg(feature = "stealth")]
pub use stealth::apply_stealth_to_page;
