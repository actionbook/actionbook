//! Human behavior simulation demonstration
//!
//! This example shows how to use the human behavior simulation module
//! to create realistic user interactions.
//!
//! Usage:
//! ```bash
//! cargo run --example human_behavior_demo
//! ```

use actionbook::browser::{
    calculate_movement_delays, generate_mouse_trajectory, generate_scroll_delays,
    generate_typing_delays, reading_time, HumanBehaviorConfig, Point,
};

fn main() {
    println!("🤖 Human Behavior Simulation Demo");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // ========== Test 1: Mouse Trajectory ==========
    println!("📊 Test 1: Mouse trajectory generation");
    println!("─────────────────────────────────────────\n");

    let start = Point::new(100.0, 100.0);
    let end = Point::new(800.0, 600.0);
    let trajectory = generate_mouse_trajectory(start, end, 20);

    println!("Generated trajectory with {} points", trajectory.len());
    println!("Start: ({:.1}, {:.1})", start.x, start.y);
    println!("End:   ({:.1}, {:.1})", end.x, end.y);
    println!("\nSample points:");
    for (i, point) in trajectory.iter().enumerate().step_by(5) {
        println!("  Point {}: ({:.1}, {:.1})", i, point.x, point.y);
    }

    let delays = calculate_movement_delays(&trajectory, 1.0);
    let total_ms: u64 = delays.iter().map(|d| d.as_millis() as u64).sum();
    println!("\nMovement timing:");
    println!("  Total duration: {}ms", total_ms);
    println!("  Average delay: {}ms", total_ms / delays.len() as u64);

    // ========== Test 2: Typing Delays ==========
    println!("\n📊 Test 2: Typing delay generation");
    println!("─────────────────────────────────────────\n");

    let test_texts = vec![
        ("Hello, World!", 60),
        ("The quick brown fox jumps over the lazy dog.", 45),
        ("Testing: 1, 2, 3! Ready?", 80),
    ];

    for (text, wpm) in test_texts {
        println!("Text: \"{}\"", text);
        println!("WPM: {}", wpm);

        let delays = generate_typing_delays(text, wpm);
        let total_ms: u64 = delays.iter().map(|d| d.as_millis() as u64).sum();

        println!("  Total time: {}ms", total_ms);
        println!("  Avg per char: {}ms", total_ms / delays.len() as u64);
        println!("  Characters: {}", text.len());
        println!();
    }

    // ========== Test 3: Typing Delay Details ==========
    println!("📊 Test 3: Typing delay breakdown");
    println!("─────────────────────────────────────────\n");

    let text = "Hello. World!";
    let delays = generate_typing_delays(text, 60);

    println!("Text: \"{}\"", text);
    println!("\nPer-character delays:");
    for (i, (ch, delay)) in text.chars().zip(delays.iter()).enumerate() {
        let ch_display = if ch == ' ' { "␣" } else { &ch.to_string() };
        println!("  [{}] '{}' → {}ms", i, ch_display, delay.as_millis());
    }

    // ========== Test 4: Reading Time ==========
    println!("\n📊 Test 4: Reading time calculation");
    println!("─────────────────────────────────────────\n");

    let word_counts = vec![10, 50, 100, 500];

    for word_count in word_counts {
        let duration = reading_time(word_count);
        println!(
            "{} words → {:.1}s (± 20% variation)",
            word_count,
            duration.as_secs_f64()
        );
    }

    // ========== Test 5: Scroll Delays ==========
    println!("\n📊 Test 5: Scroll delay generation");
    println!("─────────────────────────────────────────\n");

    println!("Without momentum:");
    let delays_no_momentum = generate_scroll_delays(10, false);
    print_scroll_delays(&delays_no_momentum);

    println!("\nWith momentum (ease-in-out):");
    let delays_with_momentum = generate_scroll_delays(10, true);
    print_scroll_delays(&delays_with_momentum);

    // ========== Test 6: Configuration Presets ==========
    println!("\n📊 Test 6: Behavior configuration presets");
    println!("─────────────────────────────────────────\n");

    let configs = vec![
        ("Fast", HumanBehaviorConfig::fast()),
        ("Normal", HumanBehaviorConfig::normal()),
        ("Slow", HumanBehaviorConfig::slow()),
    ];

    for (name, config) in configs {
        println!("{}:", name);
        println!("  Mouse speed: {:.1}x", config.mouse_speed);
        println!("  Typing WPM: {}", config.typing_wpm);
        println!("  Random pauses: {}", config.enable_random_pauses);
        if config.enable_random_pauses {
            println!(
                "  Pause range: {}-{}ms",
                config.pause_min_ms, config.pause_max_ms
            );
        }
        println!("  Scroll momentum: {}", config.enable_scroll_momentum);
        println!();
    }

    // ========== Test 7: Speed Comparison ==========
    println!("📊 Test 7: Speed multiplier effect");
    println!("─────────────────────────────────────────\n");

    let points = vec![
        Point::new(0.0, 0.0),
        Point::new(100.0, 100.0),
        Point::new(200.0, 100.0),
        Point::new(300.0, 200.0),
    ];

    let speeds = vec![0.5, 1.0, 2.0];

    for speed in speeds {
        let delays = calculate_movement_delays(&points, speed);
        let total_ms: u64 = delays.iter().map(|d| d.as_millis() as u64).sum();
        println!("Speed {:.1}x: {}ms total", speed, total_ms);
    }

    // ========== Test 8: Real-world simulation ==========
    println!("\n📊 Test 8: Real-world interaction simulation");
    println!("─────────────────────────────────────────\n");

    println!("Scenario: User filling a login form\n");

    println!("1. Move to username field:");
    let username_trajectory = generate_mouse_trajectory(
        Point::new(400.0, 300.0),  // Current position
        Point::new(650.0, 420.0),  // Username field
        15,
    );
    let username_delays = calculate_movement_delays(&username_trajectory, 1.0);
    let username_move_time: u64 = username_delays.iter().map(|d| d.as_millis() as u64).sum();
    println!("   Mouse movement: {}ms", username_move_time);

    println!("\n2. Type username:");
    let username = "john.doe@example.com";
    let username_typing_delays = generate_typing_delays(username, 65);
    let username_type_time: u64 = username_typing_delays
        .iter()
        .map(|d| d.as_millis() as u64)
        .sum();
    println!("   Typing '{}': {}ms", username, username_type_time);

    println!("\n3. Move to password field:");
    let password_trajectory = generate_mouse_trajectory(
        Point::new(650.0, 420.0),  // Username field
        Point::new(650.0, 480.0),  // Password field (below)
        8,
    );
    let password_delays = calculate_movement_delays(&password_trajectory, 1.0);
    let password_move_time: u64 = password_delays.iter().map(|d| d.as_millis() as u64).sum();
    println!("   Mouse movement: {}ms", password_move_time);

    println!("\n4. Type password:");
    let password = "MySecureP@ssw0rd";
    let password_typing_delays = generate_typing_delays(password, 55); // Slightly slower for passwords
    let password_type_time: u64 = password_typing_delays
        .iter()
        .map(|d| d.as_millis() as u64)
        .sum();
    println!("   Typing password: {}ms", password_type_time);

    println!("\n5. Move to submit button:");
    let submit_trajectory = generate_mouse_trajectory(
        Point::new(650.0, 480.0),  // Password field
        Point::new(650.0, 560.0),  // Submit button
        10,
    );
    let submit_delays = calculate_movement_delays(&submit_trajectory, 1.0);
    let submit_move_time: u64 = submit_delays.iter().map(|d| d.as_millis() as u64).sum();
    println!("   Mouse movement: {}ms", submit_move_time);

    let total_time = username_move_time
        + username_type_time
        + password_move_time
        + password_type_time
        + submit_move_time;

    println!("\n📈 Total interaction time: {}ms ({:.1}s)", total_time, total_time as f64 / 1000.0);

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ All demonstrations completed!\n");

    println!("💡 Real-world usage example:");
    println!("```rust");
    println!("// Generate humanized mouse movement");
    println!("let config = HumanBehaviorConfig::normal();");
    println!("let trajectory = generate_mouse_trajectory(start, end, 20);");
    println!("let delays = calculate_movement_delays(&trajectory, config.mouse_speed);");
    println!();
    println!("// Execute movement with delays");
    println!("for (point, delay) in trajectory.iter().zip(delays.iter()) {{");
    println!("    page.mouse_move(point.x, point.y).await?;");
    println!("    tokio::time::sleep(*delay).await;");
    println!("}}");
    println!();
    println!("// Type text with human-like delays");
    println!("let text = \"Hello, World!\";");
    println!("let delays = generate_typing_delays(text, config.typing_wpm);");
    println!("for (ch, delay) in text.chars().zip(delays.iter()) {{");
    println!("    page.keyboard_type(&ch.to_string()).await?;");
    println!("    tokio::time::sleep(*delay).await;");
    println!("}}");
    println!("```");
}

fn print_scroll_delays(delays: &[std::time::Duration]) {
    print!("  Delays: ");
    for (i, delay) in delays.iter().enumerate() {
        print!("{}ms", delay.as_millis());
        if i < delays.len() - 1 {
            print!(", ");
        }
    }
    println!();

    let total: u64 = delays.iter().map(|d| d.as_millis() as u64).sum();
    let avg = total / delays.len() as u64;
    println!("  Total: {}ms, Average: {}ms", total, avg);
}
