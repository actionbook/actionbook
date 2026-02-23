//! Fingerprint generator demonstration
//!
//! This example shows how to use the statistical fingerprint generator
//! to create realistic browser profiles.
//!
//! Usage:
//! ```bash
//! cargo run --example fingerprint_generator_demo
//! ```

use actionbook::browser::{generate_with_os, FingerprintGenerator, OperatingSystem};

fn main() {
    println!("🎲 Fingerprint Generator Demo");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // ========== Random Generation ==========
    println!("📊 Test 1: Generate 5 random fingerprints");
    println!("─────────────────────────────────────────\n");

    let mut generator = FingerprintGenerator::new();

    for i in 1..=5 {
        let profile = generator.generate();

        println!("Fingerprint #{}:", i);
        println!("  Platform:    {}", profile.platform);
        println!("  User-Agent:  {}...", &profile.user_agent[..60]);
        println!(
            "  Hardware:    {} cores, {}GB RAM",
            profile.hardware_concurrency, profile.device_memory
        );
        println!(
            "  Screen:      {}x{} (avail: {}x{})",
            profile.screen_width,
            profile.screen_height,
            profile.avail_width,
            profile.avail_height
        );
        println!(
            "  WebGL:       {} - {}",
            profile.webgl_vendor, profile.webgl_renderer
        );
        println!("  Timezone:    {}", profile.timezone);
        println!();
    }

    // ========== OS-Specific Generation ==========
    println!("\n📊 Test 2: Generate OS-specific fingerprints");
    println!("─────────────────────────────────────────\n");

    let os_list = vec![
        OperatingSystem::Windows,
        OperatingSystem::MacOsArm,
        OperatingSystem::MacOsIntel,
        OperatingSystem::Linux,
    ];

    for os in os_list {
        let profile = generate_with_os(os);

        println!("OS: {:?}", os);
        println!("  Platform:    {}", profile.platform);
        println!(
            "  WebGL:       {} - {}",
            profile.webgl_vendor, profile.webgl_renderer
        );
        println!(
            "  Screen:      {}x{}",
            profile.screen_width, profile.screen_height
        );
        println!();
    }

    // ========== Verify Consistency ==========
    println!("\n🔍 Test 3: Verify internal consistency");
    println!("─────────────────────────────────────────\n");

    // Test Mac consistency
    let mac_profile = generate_with_os(OperatingSystem::MacOsArm);
    assert!(
        mac_profile.platform == "MacIntel",
        "Mac should have MacIntel platform"
    );
    assert!(
        mac_profile.webgl_vendor.contains("Apple"),
        "Mac should have Apple GPU"
    );
    assert!(
        mac_profile.user_agent.contains("Macintosh"),
        "Mac should have Macintosh in UA"
    );
    println!("✅ Mac profile is internally consistent");

    // Test Windows consistency
    let win_profile = generate_with_os(OperatingSystem::Windows);
    assert!(
        win_profile.platform == "Win32",
        "Windows should have Win32 platform"
    );
    assert!(
        win_profile.user_agent.contains("Windows"),
        "Windows should have Windows in UA"
    );
    assert!(
        !win_profile.webgl_vendor.contains("Apple"),
        "Windows should not have Apple GPU"
    );
    println!("✅ Windows profile is internally consistent");

    // Test Linux consistency
    let linux_profile = generate_with_os(OperatingSystem::Linux);
    assert!(
        linux_profile.platform == "Linux x86_64",
        "Linux should have Linux x86_64 platform"
    );
    assert!(
        linux_profile.user_agent.contains("Linux"),
        "Linux should have Linux in UA"
    );
    println!("✅ Linux profile is internally consistent");

    // ========== Reproducibility Test ==========
    println!("\n🔁 Test 4: Test reproducibility with seed");
    println!("─────────────────────────────────────────\n");

    let mut gen1 = FingerprintGenerator::with_seed(42);
    let mut gen2 = FingerprintGenerator::with_seed(42);

    let profile1 = gen1.generate();
    let profile2 = gen2.generate();

    println!("Profile 1:");
    println!("  Platform:    {}", profile1.platform);
    println!(
        "  Screen:      {}x{}",
        profile1.screen_width, profile1.screen_height
    );
    println!(
        "  Hardware:    {} cores",
        profile1.hardware_concurrency
    );

    println!("\nProfile 2 (same seed):");
    println!("  Platform:    {}", profile2.platform);
    println!(
        "  Screen:      {}x{}",
        profile2.screen_width, profile2.screen_height
    );
    println!(
        "  Hardware:    {} cores",
        profile2.hardware_concurrency
    );

    assert_eq!(
        profile1.platform, profile2.platform,
        "Same seed should produce same platform"
    );
    assert_eq!(
        profile1.screen_width, profile2.screen_width,
        "Same seed should produce same screen width"
    );
    println!("\n✅ Reproducibility confirmed");

    // ========== Distribution Test ==========
    println!("\n📈 Test 5: Distribution analysis (100 samples)");
    println!("─────────────────────────────────────────\n");

    let mut generator = FingerprintGenerator::new();
    let mut platform_counts = std::collections::HashMap::new();
    let mut resolution_counts = std::collections::HashMap::new();

    for _ in 0..100 {
        let profile = generator.generate();
        *platform_counts.entry(profile.platform.clone()).or_insert(0) += 1;
        *resolution_counts
            .entry((profile.screen_width, profile.screen_height))
            .or_insert(0) += 1;
    }

    println!("Platform distribution:");
    for (platform, count) in platform_counts.iter() {
        println!("  {:<15} {}% ({})", platform, count, count);
    }

    println!("\nTop 5 screen resolutions:");
    let mut resolutions: Vec<_> = resolution_counts.iter().collect();
    resolutions.sort_by(|a, b| b.1.cmp(a.1));
    for ((width, height), count) in resolutions.iter().take(5) {
        println!("  {}x{:<6} {}% ({})", width, height, count, count);
    }

    // ========== Real-world usage example ==========
    println!("\n💡 Real-world usage example:");
    println!("─────────────────────────────────────────\n");

    println!("```rust");
    println!("// Generate a random fingerprint");
    println!("let mut generator = FingerprintGenerator::new();");
    println!("let profile = generator.generate();");
    println!();
    println!("// Apply to browser page");
    println!("apply_enhanced_stealth(&page, &profile).await?;");
    println!();
    println!("// Now browse with a realistic, consistent fingerprint!");
    println!("page.goto(\"https://example.com\").await?;");
    println!("```");

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ All tests passed!");
    println!("\nKey benefits:");
    println!("  • Statistically accurate: Based on real-world market share");
    println!("  • Internally consistent: OS, GPU, and screen match");
    println!("  • Reproducible: Use seeds for testing");
    println!("  • Diverse: Each profile is unique");
}
