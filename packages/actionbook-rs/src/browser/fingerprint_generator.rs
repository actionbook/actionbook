//! Fingerprint generator for creating statistically accurate browser fingerprints.
//!
//! This module generates realistic browser fingerprints based on real-world distribution
//! of devices, ensuring internal consistency between OS, GPU, screen resolution, etc.
//!

#![allow(dead_code)]
//! Inspired by BrowserForge (https://github.com/daijro/browserforge)

use crate::browser::stealth_enhanced::EnhancedStealthProfile;
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// Operating system with distribution weights
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum OperatingSystem {
    Windows,
    MacOsIntel,
    MacOsArm,
    Linux,
}

impl OperatingSystem {
    /// Get the platform string for navigator.platform
    pub fn platform(&self) -> &'static str {
        match self {
            Self::Windows => "Win32",
            Self::MacOsIntel | Self::MacOsArm => "MacIntel",
            Self::Linux => "Linux x86_64",
        }
    }

    /// Get a realistic User-Agent string for this OS
    pub fn user_agent(&self, chrome_version: u32) -> String {
        match self {
            Self::Windows => {
                format!(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36",
                    chrome_version
                )
            }
            Self::MacOsIntel => {
                format!(
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36",
                    chrome_version
                )
            }
            Self::MacOsArm => {
                format!(
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36",
                    chrome_version
                )
            }
            Self::Linux => {
                format!(
                    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36",
                    chrome_version
                )
            }
        }
    }

    /// Get typical languages for this OS
    pub fn typical_languages(&self) -> Vec<String> {
        match self {
            Self::Windows | Self::MacOsIntel | Self::MacOsArm => {
                vec!["en-US".to_string(), "en".to_string()]
            }
            Self::Linux => vec!["en-US".to_string(), "en".to_string()],
        }
    }

    /// Get typical timezone for this OS (based on most common usage)
    pub fn typical_timezone(&self) -> &'static str {
        match self {
            Self::Windows => "America/New_York",     // Most Windows users in US
            Self::MacOsIntel | Self::MacOsArm => "America/Los_Angeles", // Silicon Valley
            Self::Linux => "America/New_York",       // Developer timezone
        }
    }
}

/// Screen resolution configuration
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ScreenResolution {
    pub width: u32,
    pub height: u32,
    pub avail_width: u32,
    pub avail_height: u32,
}

impl ScreenResolution {
    /// Create a new screen resolution with typical available dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            // Typical available dimensions accounting for taskbar/menu bar
            avail_width: width,
            avail_height: height.saturating_sub(if height > 1000 { 40 } else { 30 }),
        }
    }

    /// Common screen resolutions with their market share (2024 data)
    pub fn common_resolutions() -> Vec<(Self, f64)> {
        vec![
            (Self::new(1920, 1080), 23.0), // Full HD - 23%
            (Self::new(1366, 768), 19.0),  // HD - 19%
            (Self::new(1440, 900), 9.0),   // WXGA+ - 9%
            (Self::new(1536, 864), 8.0),   // - 8%
            (Self::new(1280, 720), 7.0),   // HD - 7%
            (Self::new(2560, 1440), 6.0),  // 2K - 6%
            (Self::new(1600, 900), 5.0),   // HD+ - 5%
            (Self::new(1280, 1024), 4.0),  // SXGA - 4%
            (Self::new(1920, 1200), 3.5),  // WUXGA - 3.5%
            (Self::new(3840, 2160), 2.5),  // 4K - 2.5%
            (Self::new(2560, 1600), 2.0),  // WQXGA - 2%
            (Self::new(1680, 1050), 2.0),  // WSXGA+ - 2%
        ]
    }

    /// Get resolutions typical for a specific OS
    pub fn for_os(os: OperatingSystem) -> Vec<(Self, f64)> {
        match os {
            OperatingSystem::Windows => {
                // Windows users have more diverse resolutions
                Self::common_resolutions()
            }
            OperatingSystem::MacOsIntel | OperatingSystem::MacOsArm => {
                // Mac users tend to have higher resolution displays
                vec![
                    (Self::new(2560, 1600), 30.0), // MacBook Pro 16" (Retina)
                    (Self::new(2880, 1800), 20.0), // MacBook Pro 15" (Retina)
                    (Self::new(3024, 1964), 15.0), // MacBook Pro 16" (M1 Max)
                    (Self::new(1920, 1080), 15.0), // External display
                    (Self::new(2560, 1440), 10.0), // 27" iMac
                    (Self::new(3840, 2160), 10.0), // 4K external
                ]
            }
            OperatingSystem::Linux => {
                // Linux users often use higher resolutions
                vec![
                    (Self::new(1920, 1080), 40.0),
                    (Self::new(2560, 1440), 25.0),
                    (Self::new(1366, 768), 15.0),
                    (Self::new(3840, 2160), 10.0),
                    (Self::new(1680, 1050), 10.0),
                ]
            }
        }
    }
}

/// GPU vendor and model
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GPU {
    pub vendor: String,
    pub renderer: String,
}

impl GPU {
    /// Common GPUs with market share for Windows
    pub fn windows_gpus() -> Vec<(Self, f64)> {
        vec![
            (
                Self {
                    vendor: "NVIDIA Corporation".to_string(),
                    renderer: "NVIDIA GeForce RTX 3060".to_string(),
                },
                12.0,
            ),
            (
                Self {
                    vendor: "NVIDIA Corporation".to_string(),
                    renderer: "NVIDIA GeForce RTX 4060".to_string(),
                },
                10.0,
            ),
            (
                Self {
                    vendor: "NVIDIA Corporation".to_string(),
                    renderer: "NVIDIA GeForce RTX 3070".to_string(),
                },
                8.0,
            ),
            (
                Self {
                    vendor: "Intel Inc.".to_string(),
                    renderer: "Intel UHD Graphics 630".to_string(),
                },
                15.0,
            ),
            (
                Self {
                    vendor: "Intel Inc.".to_string(),
                    renderer: "Intel Iris Xe Graphics".to_string(),
                },
                12.0,
            ),
            (
                Self {
                    vendor: "AMD".to_string(),
                    renderer: "AMD Radeon RX 6800".to_string(),
                },
                8.0,
            ),
            (
                Self {
                    vendor: "AMD".to_string(),
                    renderer: "AMD Radeon RX 7800 XT".to_string(),
                },
                7.0,
            ),
            (
                Self {
                    vendor: "NVIDIA Corporation".to_string(),
                    renderer: "NVIDIA GeForce GTX 1660".to_string(),
                },
                6.0,
            ),
        ]
    }

    /// Common GPUs for Mac
    pub fn mac_gpus() -> Vec<(Self, f64)> {
        vec![
            (
                Self {
                    vendor: "Apple Inc.".to_string(),
                    renderer: "Apple M3 Pro".to_string(),
                },
                25.0,
            ),
            (
                Self {
                    vendor: "Apple Inc.".to_string(),
                    renderer: "Apple M3 Max".to_string(),
                },
                20.0,
            ),
            (
                Self {
                    vendor: "Apple Inc.".to_string(),
                    renderer: "Apple M2 Pro".to_string(),
                },
                15.0,
            ),
            (
                Self {
                    vendor: "Apple Inc.".to_string(),
                    renderer: "Apple M2 Max".to_string(),
                },
                12.0,
            ),
            (
                Self {
                    vendor: "Apple Inc.".to_string(),
                    renderer: "Apple M1 Pro".to_string(),
                },
                10.0,
            ),
            (
                Self {
                    vendor: "Apple Inc.".to_string(),
                    renderer: "Apple M1 Max".to_string(),
                },
                8.0,
            ),
            (
                Self {
                    vendor: "Apple Inc.".to_string(),
                    renderer: "Apple M4 Max".to_string(),
                },
                10.0,
            ),
        ]
    }

    /// Common GPUs for Linux
    pub fn linux_gpus() -> Vec<(Self, f64)> {
        vec![
            (
                Self {
                    vendor: "NVIDIA Corporation".to_string(),
                    renderer: "NVIDIA GeForce RTX 3060".to_string(),
                },
                20.0,
            ),
            (
                Self {
                    vendor: "Intel Inc.".to_string(),
                    renderer: "Intel UHD Graphics 630".to_string(),
                },
                25.0,
            ),
            (
                Self {
                    vendor: "AMD".to_string(),
                    renderer: "AMD Radeon RX 6800".to_string(),
                },
                15.0,
            ),
            (
                Self {
                    vendor: "NVIDIA Corporation".to_string(),
                    renderer: "NVIDIA GeForce RTX 4070".to_string(),
                },
                12.0,
            ),
            (
                Self {
                    vendor: "Intel Inc.".to_string(),
                    renderer: "Intel Iris Xe Graphics".to_string(),
                },
                10.0,
            ),
        ]
    }
}

/// Hardware configuration (CPU, memory)
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct HardwareConfig {
    pub cpu_cores: u32,
    pub device_memory: u32,
}

impl HardwareConfig {
    /// Common hardware configurations with market share
    pub fn common_configs() -> Vec<(Self, f64)> {
        vec![
            (
                Self {
                    cpu_cores: 8,
                    device_memory: 16,
                },
                25.0,
            ),
            (
                Self {
                    cpu_cores: 6,
                    device_memory: 8,
                },
                20.0,
            ),
            (
                Self {
                    cpu_cores: 4,
                    device_memory: 8,
                },
                18.0,
            ),
            (
                Self {
                    cpu_cores: 12,
                    device_memory: 32,
                },
                12.0,
            ),
            (
                Self {
                    cpu_cores: 8,
                    device_memory: 32,
                },
                10.0,
            ),
            (
                Self {
                    cpu_cores: 16,
                    device_memory: 64,
                },
                5.0,
            ),
            (
                Self {
                    cpu_cores: 4,
                    device_memory: 4,
                },
                10.0,
            ),
        ]
    }
}

/// Fingerprint generator that creates statistically accurate configurations
#[allow(dead_code)]
pub enum FingerprintGenerator {
    /// Random generator using thread RNG
    Random(ThreadRng),
    /// Seeded generator for reproducibility
    Seeded(StdRng),
}

impl Default for FingerprintGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl FingerprintGenerator {
    /// Create a new fingerprint generator with random seed
    pub fn new() -> Self {
        Self::Random(thread_rng())
    }

    /// Create a fingerprint generator with a specific seed (for reproducibility)
    pub fn with_seed(seed: u64) -> Self {
        Self::Seeded(StdRng::seed_from_u64(seed))
    }

    /// Generate a complete fingerprint profile with statistical accuracy
    pub fn generate(&mut self) -> EnhancedStealthProfile {
        // Step 1: Choose OS based on market share
        let os = self.choose_os();

        // Step 2: Choose screen resolution appropriate for the OS
        let screen = self.choose_screen_for_os(os);

        // Step 3: Choose GPU appropriate for the OS
        let gpu = self.choose_gpu_for_os(os);

        // Step 4: Choose hardware config
        let hardware = self.choose_hardware();

        // Step 5: Generate Chrome version (recent versions)
        let chrome_version = self.choose_chrome_version();

        // Step 6: Assemble the profile
        EnhancedStealthProfile {
            user_agent: os.user_agent(chrome_version),
            platform: os.platform().to_string(),
            hardware_concurrency: hardware.cpu_cores,
            device_memory: hardware.device_memory,
            language: "en-US".to_string(),
            languages: os.typical_languages(),
            screen_width: screen.width,
            screen_height: screen.height,
            avail_width: screen.avail_width,
            avail_height: screen.avail_height,
            webgl_vendor: gpu.vendor,
            webgl_renderer: gpu.renderer,
            timezone: os.typical_timezone().to_string(),
            latitude: None,  // User can set if needed
            longitude: None,
            color_depth: 24, // Standard for modern displays
        }
    }

    /// Choose an OS based on real-world market share
    fn choose_os(&mut self) -> OperatingSystem {
        // Market share as of 2024 (desktop/laptop)
        let choices = vec![
            (OperatingSystem::Windows, 75.0),     // 75%
            (OperatingSystem::MacOsArm, 12.0),    // 12% (M-series Macs)
            (OperatingSystem::MacOsIntel, 8.0),   // 8% (Intel Macs declining)
            (OperatingSystem::Linux, 5.0),        // 5%
        ];

        self.weighted_choice(&choices)
    }

    /// Choose screen resolution for the given OS
    fn choose_screen_for_os(&mut self, os: OperatingSystem) -> ScreenResolution {
        let resolutions = ScreenResolution::for_os(os);
        self.weighted_choice(&resolutions)
    }

    /// Choose GPU for the given OS
    fn choose_gpu_for_os(&mut self, os: OperatingSystem) -> GPU {
        let gpus = match os {
            OperatingSystem::Windows => GPU::windows_gpus(),
            OperatingSystem::MacOsIntel | OperatingSystem::MacOsArm => GPU::mac_gpus(),
            OperatingSystem::Linux => GPU::linux_gpus(),
        };

        self.weighted_choice(&gpus)
    }

    /// Choose hardware configuration
    fn choose_hardware(&mut self) -> HardwareConfig {
        let configs = HardwareConfig::common_configs();
        self.weighted_choice(&configs)
    }

    /// Choose Chrome version (recent versions only)
    fn choose_chrome_version(&mut self) -> u32 {
        // Chrome versions 127-132 (2024-2025)
        let versions = vec![
            (131, 30.0), // Latest stable
            (130, 25.0),
            (129, 20.0),
            (128, 15.0),
            (127, 10.0),
        ];

        self.weighted_choice(&versions)
    }

    /// Helper function to make a weighted random choice
    fn weighted_choice<T: Clone>(&mut self, choices: &[(T, f64)]) -> T {
        let weights: Vec<f64> = choices.iter().map(|(_, weight)| *weight).collect();
        let dist = WeightedIndex::new(&weights).expect("Invalid weights");

        let idx = match self {
            Self::Random(rng) => dist.sample(rng),
            Self::Seeded(rng) => dist.sample(rng),
        };

        choices[idx].0.clone()
    }
}

/// Generate a fingerprint with custom OS constraint
pub fn generate_with_os(os: OperatingSystem) -> EnhancedStealthProfile {
    let mut gen = FingerprintGenerator::new();
    let screen = gen.choose_screen_for_os(os);
    let gpu = gen.choose_gpu_for_os(os);
    let hardware = gen.choose_hardware();
    let chrome_version = gen.choose_chrome_version();

    EnhancedStealthProfile {
        user_agent: os.user_agent(chrome_version),
        platform: os.platform().to_string(),
        hardware_concurrency: hardware.cpu_cores,
        device_memory: hardware.device_memory,
        language: "en-US".to_string(),
        languages: os.typical_languages(),
        screen_width: screen.width,
        screen_height: screen.height,
        avail_width: screen.avail_width,
        avail_height: screen.avail_height,
        webgl_vendor: gpu.vendor,
        webgl_renderer: gpu.renderer,
        timezone: os.typical_timezone().to_string(),
        latitude: None,
        longitude: None,
        color_depth: 24,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_fingerprint() {
        let mut gen = FingerprintGenerator::new();
        let profile = gen.generate();

        // Verify all fields are set
        assert!(!profile.user_agent.is_empty());
        assert!(!profile.platform.is_empty());
        assert!(profile.hardware_concurrency > 0);
        assert!(profile.device_memory > 0);
        assert!(profile.screen_width > 0);
        assert!(profile.screen_height > 0);
        assert!(!profile.webgl_vendor.is_empty());
        assert!(!profile.webgl_renderer.is_empty());
    }

    #[test]
    fn test_os_consistency() {
        // Test that Mac OS always gets Apple GPUs
        let profile = generate_with_os(OperatingSystem::MacOsArm);
        assert!(profile.webgl_vendor.contains("Apple"));
        assert!(profile.platform == "MacIntel");
    }

    #[test]
    fn test_windows_consistency() {
        let profile = generate_with_os(OperatingSystem::Windows);
        assert!(profile.platform == "Win32");
        assert!(profile.user_agent.contains("Windows"));
    }

    #[test]
    fn test_reproducibility_with_seed() {
        let mut gen1 = FingerprintGenerator::with_seed(42);
        let mut gen2 = FingerprintGenerator::with_seed(42);

        let profile1 = gen1.generate();
        let profile2 = gen2.generate();

        // Same seed should produce same results
        assert_eq!(profile1.platform, profile2.platform);
        assert_eq!(profile1.screen_width, profile2.screen_width);
        assert_eq!(profile1.hardware_concurrency, profile2.hardware_concurrency);
    }

    #[test]
    fn test_diversity() {
        let mut gen = FingerprintGenerator::new();

        // Generate multiple profiles
        let mut platforms = std::collections::HashSet::new();
        let mut resolutions = std::collections::HashSet::new();

        for _ in 0..20 {
            let profile = gen.generate();
            platforms.insert(profile.platform.clone());
            resolutions.insert((profile.screen_width, profile.screen_height));
        }

        // Should have some diversity
        assert!(platforms.len() > 1, "Should generate different platforms");
        assert!(
            resolutions.len() > 3,
            "Should generate different resolutions"
        );
    }
}
