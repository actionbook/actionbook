//! Enhanced stealth demonstration
//!
//! This example shows how to use enhanced stealth features learned from Camoufox
//! to bypass bot detection on various websites.
//!
//! Usage:
//! ```bash
//! cargo run --example enhanced_stealth_demo
//! ```

use actionbook_rs::browser::{apply_enhanced_stealth, BrowserLauncher, EnhancedStealthProfile};
use actionbook_rs::error::Result;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::ScreenshotParams;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Enhanced Stealth Demo (Camoufox-inspired)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Step 1: Launch browser with enhanced stealth flags
    println!("📦 Step 1: Launching Chrome with enhanced stealth flags...");

    let launcher = BrowserLauncher::new()?.with_stealth(true); // Enable enhanced stealth

    let browser_process = launcher.launch()?;
    println!("✅ Browser launched with PID: {:?}\n", browser_process.id());

    sleep(Duration::from_secs(2)).await;

    // Step 2: Connect via CDP
    println!("🔌 Step 2: Connecting to browser via CDP...");

    let (browser, mut handler) = Browser::connect(format!("http://127.0.0.1:9222"))
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    // Spawn handler task
    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                eprintln!("Browser handler error: {}", e);
            }
        }
    });

    println!("✅ Connected to browser\n");

    // Step 3: Create new page
    println!("📄 Step 3: Creating new page...");

    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    println!("✅ Page created\n");

    // Step 4: Apply enhanced stealth profile
    println!("🛡️  Step 4: Applying enhanced stealth profile (Camoufox-inspired)...");

    let profile = EnhancedStealthProfile {
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
        platform: "MacIntel".to_string(),
        hardware_concurrency: 10,
        device_memory: 16,
        language: "en-US".to_string(),
        languages: vec!["en-US".to_string(), "en".to_string()],
        screen_width: 1920,
        screen_height: 1080,
        avail_width: 1920,
        avail_height: 1055,
        webgl_vendor: "Apple Inc.".to_string(),
        webgl_renderer: "Apple M4 Max".to_string(),
        timezone: "America/Los_Angeles".to_string(),
        latitude: Some(34.0522),  // Los Angeles
        longitude: Some(-118.2437),
        color_depth: 24,
    };

    apply_enhanced_stealth(&page, &profile).await?;

    println!("✅ Enhanced stealth applied:");
    println!("   • Platform: {}", profile.platform);
    println!("   • User-Agent: {}...", &profile.user_agent[..50]);
    println!("   • Hardware: {} cores, {}GB RAM", profile.hardware_concurrency, profile.device_memory);
    println!("   • Screen: {}x{}", profile.screen_width, profile.screen_height);
    println!("   • WebGL: {} - {}", profile.webgl_vendor, profile.webgl_renderer);
    println!("   • Timezone: {}\n", profile.timezone);

    // Step 5: Test on bot.sannysoft.com
    println!("🧪 Step 5: Testing on bot.sannysoft.com...");

    page.goto("https://bot.sannysoft.com")
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    println!("✅ Navigated to bot.sannysoft.com");

    // Wait for page to load
    sleep(Duration::from_secs(3)).await;

    // Take screenshot
    println!("📸 Taking screenshot...");

    let screenshot_params = ScreenshotParams::builder().build();
    let screenshot = page
        .screenshot(screenshot_params)
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    let screenshot_path = "enhanced_stealth_bot_test.png";
    std::fs::write(screenshot_path, screenshot)?;
    println!("✅ Screenshot saved: {}\n", screenshot_path);

    // Check if webdriver is detected
    println!("🔍 Checking navigator.webdriver...");

    let webdriver_result = page
        .evaluate("navigator.webdriver")
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    if let Some(value) = webdriver_result.value() {
        if value.is_null() {
            println!("✅ navigator.webdriver = undefined (GOOD!)");
        } else {
            println!("❌ navigator.webdriver = {} (DETECTED!)", value);
        }
    }

    // Check CDP traces
    println!("🔍 Checking CDP traces...");

    let cdc_result = page
        .evaluate("window.cdc_adoQpoasnfa76pfcZLmcfl_Array !== undefined")
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    if let Some(value) = cdc_result.value() {
        if value.as_bool() == Some(false) {
            println!("✅ CDP traces removed (GOOD!)");
        } else {
            println!("❌ CDP traces detected!");
        }
    }

    // Check Playwright traces
    println!("🔍 Checking Playwright traces...");

    let playwright_result = page
        .evaluate("window.__playwright !== undefined")
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    if let Some(value) = playwright_result.value() {
        if value.as_bool() == Some(false) {
            println!("✅ Playwright traces removed (GOOD!)");
        } else {
            println!("❌ Playwright traces detected!");
        }
    }

    // Check WebGL
    println!("🔍 Checking WebGL vendor...");

    let webgl_vendor = page
        .evaluate(
            r#"
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext('webgl');
            gl.getParameter(gl.getExtension('WEBGL_debug_renderer_info').UNMASKED_VENDOR_WEBGL);
        "#,
        )
        .await
        .map_err(|e| actionbook_rs::error::ActionbookError::BrowserOperation(e.to_string()))?;

    if let Some(vendor) = webgl_vendor.value() {
        println!("✅ WebGL Vendor: {}", vendor);
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Enhanced stealth demo completed successfully!");
    println!("\n📊 Results:");
    println!("   • Open {} to see the bot detection test", screenshot_path);
    println!("   • Green checks = undetected");
    println!("   • Red X's = detected");
    println!("\nExpected results with enhanced stealth:");
    println!("   ✅ Webdriver: Not detected");
    println!("   ✅ Chrome: Properly configured");
    println!("   ✅ Permissions: Normal");
    println!("   ✅ Plugins: Present");
    println!("   ✅ WebGL: Spoofed");
    println!("\nPress Ctrl+C to exit (browser will stay open)...");

    // Keep running
    tokio::signal::ctrl_c().await?;

    println!("\n👋 Shutting down...");

    Ok(())
}
