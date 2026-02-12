//! Enhanced stealth browser automation based on Camoufox techniques.
//!
//! This module implements anti-detection techniques learned from Camoufox:
//! - Navigator properties spoofing
//! - WebGL fingerprint protection
//! - Canvas noise injection
//! - WebRTC IP leak prevention
//! - Automation trace removal
//! - CDP native emulation commands

use crate::error::{ActionbookError, Result};
use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetTimezoneOverrideParams,
    SetUserAgentOverrideParams,
};
use chromiumoxide::Page;

/// Enhanced stealth profile with Camoufox-inspired features
#[derive(Debug, Clone)]
pub struct EnhancedStealthProfile {
    // Navigator properties
    pub user_agent: String,
    pub platform: String,
    pub hardware_concurrency: u32,
    pub device_memory: u32,
    pub language: String,
    pub languages: Vec<String>,

    // Screen dimensions
    pub screen_width: u32,
    pub screen_height: u32,
    pub avail_width: u32,
    pub avail_height: u32,

    // WebGL
    pub webgl_vendor: String,
    pub webgl_renderer: String,

    // Location & Time
    pub timezone: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    // Misc
    pub color_depth: u32,
}

impl Default for EnhancedStealthProfile {
    fn default() -> Self {
        Self {
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
            platform: "MacIntel".to_string(),
            hardware_concurrency: 8,
            device_memory: 8,
            language: "en-US".to_string(),
            languages: vec!["en-US".to_string(), "en".to_string()],
            screen_width: 1920,
            screen_height: 1080,
            avail_width: 1920,
            avail_height: 1055, // Typical macOS with menu bar
            webgl_vendor: "Apple Inc.".to_string(),
            webgl_renderer: "Apple M4 Max".to_string(),
            timezone: "America/Los_Angeles".to_string(),
            latitude: None,
            longitude: None,
            color_depth: 24,
        }
    }
}

impl EnhancedStealthProfile {
    /// Generate a script that removes all automation traces
    /// This must run before page load via addScriptToEvaluateOnNewDocument
    fn generate_stealth_script(&self) -> String {
        format!(
            r#"
// ========== CRITICAL: Remove Automation Traces ==========

// 1. Remove navigator.webdriver (MOST IMPORTANT!)
Object.defineProperty(navigator, 'webdriver', {{
  get: () => undefined,
  configurable: true
}});

// 2. Remove CDP runtime descriptors
delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;

// 3. Remove Playwright traces
delete window.__playwright;
delete window.__pw_manual;
delete window.__PW_inspect;

// 4. Remove Puppeteer traces
if (window.navigator.constructor) {{
  delete window.navigator.constructor.prototype;
}}

// ========== Navigator Properties Spoofing ==========

Object.defineProperty(navigator, 'platform', {{
  get: () => '{}',
  configurable: true
}});

Object.defineProperty(navigator, 'hardwareConcurrency', {{
  get: () => {},
  configurable: true
}});

Object.defineProperty(navigator, 'deviceMemory', {{
  get: () => {},
  configurable: true
}});

Object.defineProperty(navigator, 'language', {{
  get: () => '{}',
  configurable: true
}});

Object.defineProperty(navigator, 'languages', {{
  get: () => {},
  configurable: true
}});

Object.defineProperty(navigator, 'maxTouchPoints', {{
  get: () => 0,
  configurable: true
}});

// ========== Chrome Object Fix ==========

if (!window.chrome) {{
  window.chrome = {{}};
}}

window.chrome.runtime = {{
  connect: function() {{}},
  sendMessage: function() {{}}
}};

window.chrome.loadTimes = function() {{
  return {{
    requestTime: Date.now() / 1000,
    startLoadTime: Date.now() / 1000,
    commitLoadTime: Date.now() / 1000,
    finishDocumentLoadTime: Date.now() / 1000,
    finishLoadTime: Date.now() / 1000,
    firstPaintTime: Date.now() / 1000,
    firstPaintAfterLoadTime: 0,
    navigationType: 'Other',
    wasFetchedViaSpdy: false,
    wasNpnNegotiated: false,
    npnNegotiatedProtocol: 'unknown',
    wasAlternateProtocolAvailable: false,
    connectionInfo: 'http/1.1'
  }};
}};

window.chrome.csi = function() {{
  return {{
    startE: Date.now(),
    onloadT: Date.now(),
    pageT: Date.now(),
    tran: 15
  }};
}};

// ========== Plugins Array ==========

Object.defineProperty(navigator, 'plugins', {{
  get: () => [
    {{
      0: {{ type: "application/pdf", suffixes: "pdf", description: "Portable Document Format" }},
      description: "Portable Document Format",
      filename: "internal-pdf-viewer",
      length: 1,
      name: "Chrome PDF Plugin"
    }},
    {{
      0: {{ type: "application/x-google-chrome-pdf", suffixes: "pdf", description: "Portable Document Format" }},
      description: "Portable Document Format",
      filename: "internal-pdf-viewer",
      length: 1,
      name: "Chrome PDF Viewer"
    }},
    {{
      0: {{ type: "application/x-nacl", suffixes: "", description: "Native Client Executable" }},
      1: {{ type: "application/x-pnacl", suffixes: "", description: "Portable Native Client Executable" }},
      description: "",
      filename: "internal-nacl-plugin",
      length: 2,
      name: "Native Client"
    }}
  ],
  configurable: true
}});

// ========== Permissions API Override ==========

const originalQuery = window.navigator.permissions.query;
window.navigator.permissions.query = function(parameters) {{
  if (parameters.name === 'notifications') {{
    return Promise.resolve({{ state: 'default', onchange: null }});
  }}
  return originalQuery.call(this, parameters);
}};

// ========== WebGL Fingerprinting Protection ==========

const getParameter = WebGLRenderingContext.prototype.getParameter;
WebGLRenderingContext.prototype.getParameter = function(parameter) {{
  // UNMASKED_VENDOR_WEBGL
  if (parameter === 37445) {{
    return '{}';
  }}
  // UNMASKED_RENDERER_WEBGL
  if (parameter === 37446) {{
    return '{}';
  }}
  return getParameter.apply(this, arguments);
}};

// Also override WebGL2
if (window.WebGL2RenderingContext) {{
  const getParameter2 = WebGL2RenderingContext.prototype.getParameter;
  WebGL2RenderingContext.prototype.getParameter = function(parameter) {{
    if (parameter === 37445) return '{}';
    if (parameter === 37446) return '{}';
    return getParameter2.apply(this, arguments);
  }};
}}

// ========== Canvas Fingerprinting Protection ==========

const toDataURL = HTMLCanvasElement.prototype.toDataURL;
const toBlob = HTMLCanvasElement.prototype.toBlob;
const getImageData = CanvasRenderingContext2D.prototype.getImageData;

// Add imperceptible noise to canvas output
HTMLCanvasElement.prototype.toDataURL = function(...args) {{
  const context = this.getContext('2d');
  if (context) {{
    const imageData = context.getImageData(0, 0, this.width, this.height);
    // Add minimal noise (1 pixel value change every ~10 pixels)
    for (let i = 0; i < imageData.data.length; i += 40) {{
      imageData.data[i] = imageData.data[i] + (Math.random() < 0.5 ? -1 : 1);
    }}
    context.putImageData(imageData, 0, 0);
  }}
  return toDataURL.apply(this, args);
}};

HTMLCanvasElement.prototype.toBlob = function(...args) {{
  const context = this.getContext('2d');
  if (context) {{
    const imageData = context.getImageData(0, 0, this.width, this.height);
    for (let i = 0; i < imageData.data.length; i += 40) {{
      imageData.data[i] = imageData.data[i] + (Math.random() < 0.5 ? -1 : 1);
    }}
    context.putImageData(imageData, 0, 0);
  }}
  return toBlob.apply(this, args);
}};

// ========== WebRTC IP Leak Prevention ==========

const RTCPeerConnection = window.RTCPeerConnection || window.webkitRTCPeerConnection || window.mozRTCPeerConnection;

if (RTCPeerConnection) {{
  const originalCreateOffer = RTCPeerConnection.prototype.createOffer;
  const originalCreateAnswer = RTCPeerConnection.prototype.createAnswer;

  RTCPeerConnection.prototype.createOffer = function(...args) {{
    return originalCreateOffer.apply(this, args).then(offer => {{
      // Replace real IPs with 0.0.0.0 in SDP
      if (offer.sdp) {{
        offer.sdp = offer.sdp.replace(/(\r\n|\r|\n)c=IN\s+(IP4|IP6)\s+[\d.a-f:]+/gi, '$1c=IN $2 0.0.0.0');
      }}
      return offer;
    }});
  }};

  RTCPeerConnection.prototype.createAnswer = function(...args) {{
    return originalCreateAnswer.apply(this, args).then(answer => {{
      if (answer.sdp) {{
        answer.sdp = answer.sdp.replace(/(\r\n|\r|\n)c=IN\s+(IP4|IP6)\s+[\d.a-f:]+/gi, '$1c=IN $2 0.0.0.0');
      }}
      return answer;
    }});
  }};
}}

// ========== Battery API Spoofing ==========

if (navigator.getBattery) {{
  const originalGetBattery = navigator.getBattery;
  navigator.getBattery = function() {{
    return Promise.resolve({{
      charging: true,
      chargingTime: 0,
      dischargingTime: Infinity,
      level: 1.0,
      addEventListener: function() {{}},
      removeEventListener: function() {{}},
      dispatchEvent: function() {{ return true; }}
    }});
  }};
}}

// ========== Screen Properties ==========

Object.defineProperty(screen, 'colorDepth', {{
  get: () => {},
  configurable: true
}});

Object.defineProperty(screen, 'pixelDepth', {{
  get: () => {},
  configurable: true
}});

console.log('✅ Enhanced stealth script loaded (Camoufox-inspired)');
"#,
            self.platform,
            self.hardware_concurrency,
            self.device_memory,
            self.language,
            serde_json::to_string(&self.languages).unwrap(),
            self.webgl_vendor,
            self.webgl_renderer,
            self.webgl_vendor,
            self.webgl_renderer,
            self.color_depth,
            self.color_depth,
        )
    }
}

/// Apply enhanced stealth profile to a page using CDP native commands + JavaScript
pub async fn apply_enhanced_stealth(
    page: &Page,
    profile: &EnhancedStealthProfile,
) -> Result<()> {
    // ========== PHASE 1: CDP Native Commands (Less Detectable) ==========

    // 1. Set User-Agent via CDP
    let user_agent_params = SetUserAgentOverrideParams::builder()
        .user_agent(&profile.user_agent)
        .build()
        .map_err(|e| ActionbookError::Other(format!("Failed to build user agent params: {}", e)))?;

    page.execute(user_agent_params)
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to set user agent: {}", e)))?;

    tracing::debug!("✅ Set User-Agent via CDP");

    // 2. Set Screen Metrics via CDP
    let device_metrics = SetDeviceMetricsOverrideParams::builder()
        .width(profile.screen_width as i64)
        .height(profile.screen_height as i64)
        .device_scale_factor(1.0)
        .mobile(false)
        .screen_width(profile.screen_width as i64)
        .screen_height(profile.screen_height as i64)
        .build()
        .map_err(|e| ActionbookError::Other(format!("Failed to build device metrics params: {}", e)))?;

    page.execute(device_metrics)
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to set device metrics: {}", e)))?;

    tracing::debug!("✅ Set screen metrics via CDP: {}x{}", profile.screen_width, profile.screen_height);

    // 3. Set Timezone via CDP
    let timezone_params = SetTimezoneOverrideParams::new(profile.timezone.clone());
    page.execute(timezone_params)
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to set timezone: {}", e)))?;

    tracing::debug!("✅ Set timezone via CDP: {}", profile.timezone);

    // 4. Set Geolocation if provided
    if let (Some(lat), Some(lon)) = (profile.latitude, profile.longitude) {
        // Note: SetGeolocationOverrideParams doesn't have a builder pattern
        // We'll use JavaScript injection as a fallback
        let geolocation_script = format!(
            r#"
            navigator.geolocation.getCurrentPosition = function(success) {{
                const position = {{
                    coords: {{
                        latitude: {},
                        longitude: {},
                        accuracy: 100,
                        altitude: null,
                        altitudeAccuracy: null,
                        heading: null,
                        speed: null
                    }},
                    timestamp: Date.now()
                }};
                success(position);
            }};
            "#,
            lat, lon
        );

        page.evaluate(geolocation_script)
            .await
            .map_err(|e| ActionbookError::Other(format!("Failed to set geolocation: {}", e)))?;

        tracing::debug!("✅ Set geolocation via JavaScript: {}, {}", lat, lon);
    }

    // ========== PHASE 2: JavaScript Injection (Critical for Automation Detection) ==========

    // IMPORTANT: Use addScriptToEvaluateOnNewDocument instead of evaluate
    // This runs BEFORE page load, making it much harder to detect
    let stealth_script = profile.generate_stealth_script();

    page.evaluate_on_new_document(stealth_script)
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to inject stealth script: {}", e)))?;

    tracing::info!(
        "✅ Enhanced stealth profile applied (Camoufox-inspired): {} on {}",
        profile.platform,
        profile.user_agent
    );

    Ok(())
}

/// Get Chrome launch args with enhanced stealth flags (Camoufox-inspired)
pub fn get_enhanced_stealth_args() -> Vec<String> {
    vec![
        // ========== Critical Anti-Detection Flags ==========
        "--disable-blink-features=AutomationControlled".to_string(), // CRITICAL!

        // ========== WebRTC Protection ==========
        "--force-webrtc-ip-handling-policy=disable_non_proxied_udp".to_string(),

        // ========== Stability & Performance ==========
        "--disable-dev-shm-usage".to_string(),
        "--disable-setuid-sandbox".to_string(),

        // ========== Remove Automation UI ==========
        "--disable-infobars".to_string(),
        "--disable-save-password-bubble".to_string(),
        "--disable-translate".to_string(),

        // ========== Disable Features That Leak Automation ==========
        "--disable-features=IsolateOrigins,site-per-process".to_string(),
        "--disable-site-isolation-trials".to_string(),

        // ========== Window Size ==========
        "--window-size=1920,1080".to_string(),

        // ========== Extensions Support ==========
        "--disable-extensions-except".to_string(), // Will be followed by extension paths
        "--load-extension".to_string(),            // Will be followed by extension paths
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enhanced_profile_default() {
        let profile = EnhancedStealthProfile::default();
        assert_eq!(profile.platform, "MacIntel");
        assert_eq!(profile.hardware_concurrency, 8);
        assert!(profile.user_agent.contains("Chrome"));
    }

    #[test]
    fn test_stealth_script_generation() {
        let profile = EnhancedStealthProfile::default();
        let script = profile.generate_stealth_script();

        // Verify critical components are present
        assert!(script.contains("navigator.webdriver"));
        assert!(script.contains("cdc_adoQpoasnfa76pfcZLmcfl"));
        assert!(script.contains("__playwright"));
        assert!(script.contains("WebGLRenderingContext"));
        assert!(script.contains("HTMLCanvasElement"));
        assert!(script.contains("RTCPeerConnection"));
    }

    #[test]
    fn test_enhanced_stealth_args() {
        let args = get_enhanced_stealth_args();
        assert!(args.contains(&"--disable-blink-features=AutomationControlled".to_string()));
        assert!(args
            .contains(&"--force-webrtc-ip-handling-policy=disable_non_proxied_udp".to_string()));
    }
}
