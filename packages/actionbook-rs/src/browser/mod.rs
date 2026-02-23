mod backend;
pub mod bridge_lifecycle;
pub mod camofox;
pub mod camofox_webdriver;
mod discovery;
pub mod extension_bridge;
pub mod extension_installer;
pub mod fingerprint_generator; // Statistical fingerprint generation (Phase 2)
pub mod human_behavior; // Human behavior simulation (Phase 3)
pub mod launcher;
pub mod native_messaging;
mod router;
mod session;
pub mod stealth;
pub mod stealth_enhanced; // Enhanced stealth based on Camoufox techniques (Phase 1)

pub use backend::BrowserBackend;
#[allow(unused_imports)]
pub use discovery::{discover_all_browsers, BrowserInfo, BrowserType};
pub use router::BrowserDriver;
pub use session::{SessionManager, SessionStatus, StealthConfig};
pub use stealth::{build_stealth_profile, stealth_status};

// Re-export stealth page application for external use
#[cfg(feature = "stealth")]
pub use stealth::apply_stealth_to_page;

// Re-export enhanced stealth (Phase 1)
#[allow(unused_imports)]
pub use stealth_enhanced::{
    apply_enhanced_stealth, get_enhanced_stealth_args, EnhancedStealthProfile,
};

// Re-export fingerprint generator (Phase 2)
#[allow(unused_imports)]
pub use fingerprint_generator::{
    generate_with_os, FingerprintGenerator, HardwareConfig, OperatingSystem, ScreenResolution, GPU,
};

// Re-export human behavior simulation (Phase 3)
#[allow(unused_imports)]
pub use human_behavior::{
    calculate_movement_delays, generate_mouse_trajectory, generate_scroll_delays,
    generate_typing_delays, humanized_pause, reading_time, simulate_reading, HumanBehaviorConfig,
    Point,
};
