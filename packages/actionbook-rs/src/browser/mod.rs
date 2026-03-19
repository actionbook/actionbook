pub mod app_discovery; // Electron app discovery
pub mod auto_connect; // Auto-discover running Chrome instances
mod backend;
pub mod bridge_lifecycle;
#[cfg(feature = "camoufox")]
pub mod camofox;
#[cfg(feature = "camoufox")]
pub mod camofox_webdriver;
pub mod cdp_types; // CDP typed message structures (Phase 2a optimization)
pub mod content;
mod discovery;
pub mod extension_backend;
pub mod extension_bridge;
pub mod extension_installer;
pub mod fingerprint_generator; // Statistical fingerprint generation (Phase 2)
pub mod http_fetch; // HTTP-first content fetching (I2)
pub mod human_behavior; // Human behavior simulation (Phase 3)
pub mod human_input; // Human-like mouse/typing (F5, borrowed from pinchtab)
pub mod launcher;
pub mod native_messaging;
pub mod readability; // Readability text extraction (F4, borrowed from pinchtab)
mod router;
mod session;
pub mod snapshot; // CDP Accessibility Tree (F1, borrowed from pinchtab)
pub mod stealth;
pub mod stealth_enhanced;
pub mod url_rewrite; // Privacy-frontend URL rewriting (I4)
pub mod wait_hints; // Domain-aware wait strategies (I5) // Enhanced stealth based on Camoufox techniques (Phase 1)

#[allow(unused_imports)]
pub use app_discovery::{discover_electron_apps, ElectronAppInfo};
pub use backend::BrowserBackend;
#[allow(unused_imports)]
pub use discovery::{discover_all_browsers, BrowserInfo, BrowserType};
pub use router::BrowserDriver;
pub use session::{ResourceBlockLevel, TextExtractionMode};
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
