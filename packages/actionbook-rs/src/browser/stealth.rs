//! Stealth browser automation support using chaser-oxide.
//!
//! Enable with `--features stealth` for:
//! - No automation banner
//! - Anti-detection measures
//! - Hardware fingerprint spoofing
//! - Human-like mouse/keyboard simulation

#[cfg(feature = "stealth")]
use chaser_oxide::profiles::{ChaserProfile, ChaserProfileBuilder, Gpu};

#[cfg(feature = "stealth")]
use crate::error::{ActionbookError, Result};

/// Stealth profile configuration
#[derive(Debug, Clone)]
pub struct StealthProfile {
    /// Operating system to emulate
    pub os: StealthOs,
    /// GPU to emulate
    pub gpu: StealthGpu,
    /// Chrome version to emulate
    pub chrome_version: u32,
    /// Memory in GB
    pub memory_gb: u32,
    /// CPU cores
    pub cpu_cores: u32,
    /// Locale (e.g., "en-US", "zh-CN")
    pub locale: String,
    /// Timezone (e.g., "America/New_York", "Asia/Shanghai")
    pub timezone: String,
}

impl Default for StealthProfile {
    fn default() -> Self {
        Self {
            os: StealthOs::MacOsArm,
            gpu: StealthGpu::AppleM4Max,
            chrome_version: 130,
            memory_gb: 16,
            cpu_cores: 8,
            locale: "en-US".to_string(),
            timezone: "America/Los_Angeles".to_string(),
        }
    }
}

/// Operating system options for stealth profile
#[derive(Debug, Clone, Copy)]
pub enum StealthOs {
    Windows,
    MacOsIntel,
    MacOsArm,
    Linux,
}

/// GPU options for stealth profile
#[derive(Debug, Clone, Copy)]
pub enum StealthGpu {
    // NVIDIA
    NvidiaRtx4080,
    NvidiaRtx3080,
    NvidiaGtx1660,
    // AMD
    AmdRadeonRx6800,
    // Intel
    IntelUhd630,
    IntelIrisXe,
    // Apple
    AppleM1Pro,
    AppleM2Max,
    AppleM4Max,
}

#[cfg(feature = "stealth")]
impl StealthProfile {
    /// Build a chaser-oxide profile from this configuration
    pub fn to_chaser_profile(&self) -> ChaserProfile {
        let builder: ChaserProfileBuilder = match self.os {
            StealthOs::Windows => ChaserProfile::windows(),
            StealthOs::MacOsIntel => ChaserProfile::macos_intel(),
            StealthOs::MacOsArm => ChaserProfile::macos_arm(),
            StealthOs::Linux => ChaserProfile::linux(),
        };

        let gpu = match self.gpu {
            StealthGpu::NvidiaRtx4080 => Gpu::NvidiaRTX4080,
            StealthGpu::NvidiaRtx3080 => Gpu::NvidiaRTX3080,
            StealthGpu::NvidiaGtx1660 => Gpu::NvidiaGTX1660,
            StealthGpu::AmdRadeonRx6800 => Gpu::AmdRadeonRX6800,
            StealthGpu::IntelUhd630 => Gpu::IntelUHD630,
            StealthGpu::IntelIrisXe => Gpu::IntelIrisXe,
            StealthGpu::AppleM1Pro => Gpu::AppleM1Pro,
            StealthGpu::AppleM2Max => Gpu::AppleM2Max,
            StealthGpu::AppleM4Max => Gpu::AppleM4Max,
        };

        builder
            .chrome_version(self.chrome_version)
            .gpu(gpu)
            .memory_gb(self.memory_gb)
            .cpu_cores(self.cpu_cores)
            .locale(&self.locale)
            .timezone(&self.timezone)
            .build()
    }
}

/// Apply stealth profile to a chromiumoxide page
/// This wraps the page in ChaserPage and applies anti-detection measures
#[cfg(feature = "stealth")]
pub async fn apply_stealth_to_page(
    page: &chromiumoxide::Page,
    profile: &StealthProfile,
) -> Result<()> {
    use chromiumoxide::Page;

    // Convert to ChaserPage and apply profile
    // Note: ChaserPage requires ownership, so we clone the page handle
    let chaser_profile = profile.to_chaser_profile();

    // Apply stealth via CDP commands directly
    // This is done via JavaScript injection to avoid ChaserPage ownership issues

    // 1. Override navigator properties
    let navigator_override = format!(
        r#"
        Object.defineProperty(navigator, 'webdriver', {{ get: () => undefined }});
        Object.defineProperty(navigator, 'platform', {{ get: () => '{}' }});
        Object.defineProperty(navigator, 'hardwareConcurrency', {{ get: () => {} }});
        Object.defineProperty(navigator, 'deviceMemory', {{ get: () => {} }});
        Object.defineProperty(navigator, 'language', {{ get: () => '{}' }});
        Object.defineProperty(navigator, 'languages', {{ get: () => ['{}', 'en'] }});
        "#,
        match profile.os {
            StealthOs::Windows => "Win32",
            StealthOs::MacOsIntel | StealthOs::MacOsArm => "MacIntel",
            StealthOs::Linux => "Linux x86_64",
        },
        profile.cpu_cores,
        profile.memory_gb,
        &profile.locale,
        &profile.locale,
    );

    page.evaluate(navigator_override)
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to apply navigator override: {}", e)))?;

    // 2. Override WebGL renderer
    let webgl_override = format!(
        r#"
        const getParameter = WebGLRenderingContext.prototype.getParameter;
        WebGLRenderingContext.prototype.getParameter = function(parameter) {{
            if (parameter === 37445) return '{}';
            if (parameter === 37446) return '{}';
            return getParameter.apply(this, arguments);
        }};
        "#,
        match profile.gpu {
            StealthGpu::NvidiaRtx4080 | StealthGpu::NvidiaRtx3080 | StealthGpu::NvidiaGtx1660 => "NVIDIA Corporation",
            StealthGpu::AmdRadeonRx6800 => "AMD",
            StealthGpu::IntelUhd630 | StealthGpu::IntelIrisXe => "Intel Inc.",
            StealthGpu::AppleM1Pro | StealthGpu::AppleM2Max | StealthGpu::AppleM4Max => "Apple Inc.",
        },
        match profile.gpu {
            StealthGpu::NvidiaRtx4080 => "NVIDIA GeForce RTX 4080",
            StealthGpu::NvidiaRtx3080 => "NVIDIA GeForce RTX 3080",
            StealthGpu::NvidiaGtx1660 => "NVIDIA GeForce GTX 1660",
            StealthGpu::AmdRadeonRx6800 => "AMD Radeon RX 6800",
            StealthGpu::IntelUhd630 => "Intel UHD Graphics 630",
            StealthGpu::IntelIrisXe => "Intel Iris Xe Graphics",
            StealthGpu::AppleM1Pro => "Apple M1 Pro",
            StealthGpu::AppleM2Max => "Apple M2 Max",
            StealthGpu::AppleM4Max => "Apple M4 Max",
        },
    );

    page.evaluate(webgl_override)
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to apply WebGL override: {}", e)))?;

    // 3. Hide automation indicators
    let automation_hide = r#"
        // Remove webdriver property
        delete navigator.__proto__.webdriver;

        // Override permissions
        const originalQuery = window.navigator.permissions.query;
        window.navigator.permissions.query = (parameters) => (
            parameters.name === 'notifications' ?
                Promise.resolve({ state: Notification.permission }) :
                originalQuery(parameters)
        );

        // Fix chrome object
        window.chrome = { runtime: {} };

        // Fix plugins
        Object.defineProperty(navigator, 'plugins', {
            get: () => [
                { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer' },
                { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai' },
                { name: 'Native Client', filename: 'internal-nacl-plugin' }
            ]
        });
    "#;

    page.evaluate(automation_hide)
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to hide automation: {}", e)))?;

    tracing::debug!("Applied stealth profile to page: {:?}", profile.os);
    Ok(())
}

/// Check if stealth mode is enabled
pub fn is_stealth_enabled() -> bool {
    cfg!(feature = "stealth")
}

/// Get stealth mode status string
pub fn stealth_status() -> &'static str {
    if cfg!(feature = "stealth") {
        "enabled (chaser-oxide)"
    } else {
        "disabled (use --features stealth to enable)"
    }
}

/// Parse OS string from CLI into StealthOs
pub fn parse_stealth_os(s: &str) -> Option<StealthOs> {
    match s.to_lowercase().as_str() {
        "windows" | "win" => Some(StealthOs::Windows),
        "macos-intel" | "mac-intel" | "osx-intel" => Some(StealthOs::MacOsIntel),
        "macos-arm" | "mac-arm" | "osx-arm" | "macos" | "mac" => Some(StealthOs::MacOsArm),
        "linux" => Some(StealthOs::Linux),
        _ => None,
    }
}

/// Parse GPU string from CLI into StealthGpu
pub fn parse_stealth_gpu(s: &str) -> Option<StealthGpu> {
    match s.to_lowercase().replace(['-', '_', ' '], "").as_str() {
        "nvidiartx4080" | "rtx4080" | "4080" => Some(StealthGpu::NvidiaRtx4080),
        "nvidiartx3080" | "rtx3080" | "3080" => Some(StealthGpu::NvidiaRtx3080),
        "nvidiagtx1660" | "gtx1660" | "1660" => Some(StealthGpu::NvidiaGtx1660),
        "amdradeonrx6800" | "rx6800" | "6800" => Some(StealthGpu::AmdRadeonRx6800),
        "inteluhd630" | "uhd630" => Some(StealthGpu::IntelUhd630),
        "intelirisxe" | "irisxe" => Some(StealthGpu::IntelIrisXe),
        "applem1pro" | "m1pro" | "m1" => Some(StealthGpu::AppleM1Pro),
        "applem2max" | "m2max" | "m2" => Some(StealthGpu::AppleM2Max),
        "applem4max" | "m4max" | "m4" => Some(StealthGpu::AppleM4Max),
        _ => None,
    }
}

/// Build a StealthProfile from optional CLI parameters
pub fn build_stealth_profile(os: Option<&str>, gpu: Option<&str>) -> StealthProfile {
    let mut profile = StealthProfile::default();

    if let Some(os_str) = os {
        if let Some(os_val) = parse_stealth_os(os_str) {
            profile.os = os_val;
        }
    }

    if let Some(gpu_str) = gpu {
        if let Some(gpu_val) = parse_stealth_gpu(gpu_str) {
            profile.gpu = gpu_val;
        }
    }

    profile
}
