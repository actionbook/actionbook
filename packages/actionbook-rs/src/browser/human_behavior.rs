//! Human behavior simulation module
//!
//! This module implements realistic human interaction patterns to avoid
//! behavioral detection by anti-bot systems. Inspired by Camoufox's
//! MouseTrajectories.hpp and timing analysis.

#![allow(dead_code)]
//!
//! Key features:
//! - Bezier curve mouse movements
//! - Humanized typing patterns with variable delays
//! - Natural scrolling with momentum simulation
//! - Random pause and read-time simulation

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for human behavior simulation
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HumanBehaviorConfig {
    /// Mouse movement speed multiplier (1.0 = normal, 0.5 = slower, 2.0 = faster)
    pub mouse_speed: f64,

    /// Typing speed in words per minute (WPM)
    pub typing_wpm: u32,

    /// Enable random pauses between actions
    pub enable_random_pauses: bool,

    /// Minimum pause duration in milliseconds
    pub pause_min_ms: u64,

    /// Maximum pause duration in milliseconds
    pub pause_max_ms: u64,

    /// Enable scroll momentum simulation
    pub enable_scroll_momentum: bool,
}

impl Default for HumanBehaviorConfig {
    fn default() -> Self {
        Self {
            mouse_speed: 1.0,
            typing_wpm: 60,
            enable_random_pauses: true,
            pause_min_ms: 100,
            pause_max_ms: 800,
            enable_scroll_momentum: true,
        }
    }
}

impl HumanBehaviorConfig {
    /// Create a fast behavior profile (for testing)
    pub fn fast() -> Self {
        Self {
            mouse_speed: 2.0,
            typing_wpm: 120,
            enable_random_pauses: false,
            pause_min_ms: 0,
            pause_max_ms: 0,
            enable_scroll_momentum: false,
        }
    }

    /// Create a slow/careful behavior profile
    pub fn slow() -> Self {
        Self {
            mouse_speed: 0.5,
            typing_wpm: 30,
            enable_random_pauses: true,
            pause_min_ms: 300,
            pause_max_ms: 1500,
            enable_scroll_momentum: true,
        }
    }

    /// Create a normal human behavior profile
    pub fn normal() -> Self {
        Self::default()
    }
}

/// A 2D point for mouse trajectory
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Calculate distance to another point
    pub fn distance_to(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Generate a Bezier curve mouse trajectory
///
/// Uses cubic Bezier curves to simulate natural mouse movement.
/// Based on Camoufox's MouseTrajectories.hpp implementation.
///
/// # Arguments
/// * `start` - Starting point
/// * `end` - Ending point
/// * `steps` - Number of intermediate points to generate
///
/// # Returns
/// Vector of points representing the mouse path
pub fn generate_mouse_trajectory(start: Point, end: Point, steps: usize) -> Vec<Point> {
    if steps == 0 {
        return vec![start, end];
    }

    let mut rng = rand::thread_rng();

    // Generate control points for cubic Bezier curve
    // Add some randomness to make it look more human
    let distance = start.distance_to(&end);
    let randomness = (distance * 0.2).max(20.0).min(100.0);

    let control1 = Point::new(
        start.x + (end.x - start.x) * 0.25 + rng.gen_range(-randomness..randomness),
        start.y + (end.y - start.y) * 0.25 + rng.gen_range(-randomness..randomness),
    );

    let control2 = Point::new(
        start.x + (end.x - start.x) * 0.75 + rng.gen_range(-randomness..randomness),
        start.y + (end.y - start.y) * 0.75 + rng.gen_range(-randomness..randomness),
    );

    // Generate points along the Bezier curve
    let mut points = Vec::with_capacity(steps + 2);
    points.push(start);

    for i in 1..=steps {
        let t = i as f64 / (steps + 1) as f64;
        let point = cubic_bezier(t, start, control1, control2, end);
        points.push(point);
    }

    points.push(end);
    points
}

/// Calculate a point on a cubic Bezier curve
fn cubic_bezier(t: f64, p0: Point, p1: Point, p2: Point, p3: Point) -> Point {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;

    Point::new(
        mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x,
        mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y,
    )
}

/// Calculate delays between mouse movements
///
/// Uses easing function to simulate acceleration and deceleration
pub fn calculate_movement_delays(
    points: &[Point],
    speed_multiplier: f64,
) -> Vec<Duration> {
    if points.len() <= 1 {
        return Vec::new();
    }

    let mut delays = Vec::with_capacity(points.len() - 1);
    let total_distance: f64 = points
        .windows(2)
        .map(|w| w[0].distance_to(&w[1]))
        .sum();

    // Base duration: ~500ms for typical mouse movements
    let base_duration_ms = (total_distance * 0.5 / speed_multiplier).max(100.0).min(2000.0);

    for i in 0..points.len() - 1 {
        let progress = i as f64 / (points.len() - 1) as f64;

        // Ease-in-out: slower at start and end, faster in middle
        let easing = if progress < 0.5 {
            2.0 * progress * progress
        } else {
            1.0 - (-2.0 * progress + 2.0).powi(2) / 2.0
        };

        // Distribute total duration with easing
        let delay_ms = (base_duration_ms / points.len() as f64) * (1.0 + easing * 0.5);
        delays.push(Duration::from_millis(delay_ms as u64));
    }

    delays
}

/// Generate humanized typing delays
///
/// Simulates realistic typing patterns with:
/// - Variable keystroke delays based on WPM
/// - Longer pauses for punctuation
/// - Random variations
///
/// # Arguments
/// * `text` - The text to be typed
/// * `wpm` - Words per minute (typical human: 40-80 WPM)
///
/// # Returns
/// Vector of delays (one per character)
pub fn generate_typing_delays(text: &str, wpm: u32) -> Vec<Duration> {
    let mut rng = rand::thread_rng();
    let mut delays = Vec::with_capacity(text.len());

    // Average delay between keystrokes (milliseconds)
    // Formula: (60 seconds/min * 1000 ms/s) / (WPM * 5 chars/word)
    let avg_delay_ms = (60_000.0 / (wpm as f64 * 5.0)) as u64;

    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        let base_delay = avg_delay_ms;

        // Longer pauses for certain characters
        let char_multiplier = match ch {
            ' ' => 1.5,  // Slight pause at spaces
            '.' | '!' | '?' => 2.5,  // Longer pause at sentence end
            ',' | ';' | ':' => 1.8,  // Medium pause at punctuation
            '\n' => 3.0,  // Long pause at line breaks
            _ => 1.0,
        };

        // Add randomness: ±30% variation
        let randomness = rng.gen_range(0.7..1.3);

        // Occasional longer pauses (thinking time)
        let thinking_pause = if i > 0 && i % 20 == 0 && rng.gen_bool(0.3) {
            rng.gen_range(300..800)
        } else {
            0
        };

        let total_delay = ((base_delay as f64 * char_multiplier * randomness) as u64)
            .max(30)  // Minimum 30ms
            + thinking_pause;

        delays.push(Duration::from_millis(total_delay));
    }

    delays
}

/// Generate a random pause duration
pub fn random_pause(config: &HumanBehaviorConfig) -> Duration {
    if !config.enable_random_pauses {
        return Duration::from_millis(0);
    }

    let mut rng = rand::thread_rng();
    let ms = rng.gen_range(config.pause_min_ms..=config.pause_max_ms);
    Duration::from_millis(ms)
}

/// Simulate reading time based on content length
///
/// Uses average reading speed of 200-250 words per minute
pub fn reading_time(word_count: usize) -> Duration {
    // Average reading speed: 225 WPM
    // Add some randomness: ±20%
    let mut rng = rand::thread_rng();
    let base_seconds = (word_count as f64 / 225.0) * 60.0;
    let randomness = rng.gen_range(0.8..1.2);
    let total_seconds = (base_seconds * randomness).max(1.0);

    Duration::from_secs_f64(total_seconds)
}

/// Generate scroll step delays with momentum simulation
///
/// # Arguments
/// * `steps` - Number of scroll steps
/// * `momentum` - Whether to simulate momentum (slower at start/end)
pub fn generate_scroll_delays(steps: usize, momentum: bool) -> Vec<Duration> {
    if steps == 0 {
        return Vec::new();
    }

    let mut delays = Vec::with_capacity(steps);
    let base_delay_ms = 30; // ~30fps scrolling

    for i in 0..steps {
        let delay_ms = if momentum {
            let progress = i as f64 / steps as f64;

            // Ease-in-out: slower at start and end
            let easing = if progress < 0.3 {
                // Ease in: accelerating
                2.0 - (progress / 0.3)
            } else if progress > 0.7 {
                // Ease out: decelerating
                2.0 - ((1.0 - progress) / 0.3)
            } else {
                // Middle: constant speed
                1.0
            };

            (base_delay_ms as f64 * easing) as u64
        } else {
            base_delay_ms
        };

        delays.push(Duration::from_millis(delay_ms));
    }

    delays
}

/// Async helper: perform a humanized pause
pub async fn humanized_pause(config: &HumanBehaviorConfig) {
    let duration = random_pause(config);
    if duration.as_millis() > 0 {
        sleep(duration).await;
    }
}

/// Async helper: simulate reading time
pub async fn simulate_reading(word_count: usize) {
    let duration = reading_time(word_count);
    sleep(duration).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_trajectory_generation() {
        let start = Point::new(100.0, 100.0);
        let end = Point::new(500.0, 300.0);
        let trajectory = generate_mouse_trajectory(start, end, 10);

        assert_eq!(trajectory.len(), 12); // start + 10 steps + end
        assert_eq!(trajectory[0].x, 100.0);
        assert_eq!(trajectory[0].y, 100.0);
        assert_eq!(trajectory[11].x, 500.0);
        assert_eq!(trajectory[11].y, 300.0);
    }

    #[test]
    fn test_movement_delays() {
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 100.0),
            Point::new(200.0, 150.0),
        ];

        let delays = calculate_movement_delays(&points, 1.0);
        assert_eq!(delays.len(), 2);

        // All delays should be positive
        for delay in delays {
            assert!(delay.as_millis() > 0);
        }
    }

    #[test]
    fn test_typing_delays() {
        let text = "Hello, World!";
        let delays = generate_typing_delays(text, 60);

        assert_eq!(delays.len(), text.len());

        // All delays should be positive
        for delay in delays {
            assert!(delay.as_millis() >= 30); // Minimum 30ms
        }
    }

    #[test]
    fn test_typing_delays_punctuation() {
        let text = "Hello. World!";
        let delays = generate_typing_delays(text, 60);

        // Find the delay after the period
        let period_index = text.find('.').unwrap();
        let period_delay = delays[period_index];

        // Find a regular character delay
        let h_index = text.find('H').unwrap();
        let regular_delay = delays[h_index];

        // Period should have longer delay
        assert!(period_delay > regular_delay);
    }

    #[test]
    fn test_reading_time() {
        let duration = reading_time(100);

        // 100 words at ~225 WPM ≈ 26 seconds
        // With randomness: 21-32 seconds
        assert!(duration.as_secs() >= 20 && duration.as_secs() <= 35);
    }

    #[test]
    fn test_scroll_delays_with_momentum() {
        let delays = generate_scroll_delays(10, true);
        assert_eq!(delays.len(), 10);

        // First delay should be longer (ease-in)
        // Middle delays should be shorter
        // Last delay should be longer (ease-out)
        assert!(delays[0] > delays[5]);
        assert!(delays[9] > delays[5]);
    }

    #[test]
    fn test_scroll_delays_without_momentum() {
        let delays = generate_scroll_delays(10, false);
        assert_eq!(delays.len(), 10);

        // All delays should be roughly equal
        let first = delays[0].as_millis();
        let last = delays[9].as_millis();
        assert_eq!(first, last);
    }

    #[test]
    fn test_config_presets() {
        let fast = HumanBehaviorConfig::fast();
        assert_eq!(fast.mouse_speed, 2.0);
        assert_eq!(fast.typing_wpm, 120);
        assert!(!fast.enable_random_pauses);

        let slow = HumanBehaviorConfig::slow();
        assert_eq!(slow.mouse_speed, 0.5);
        assert_eq!(slow.typing_wpm, 30);
        assert!(slow.enable_random_pauses);

        let normal = HumanBehaviorConfig::normal();
        assert_eq!(normal.mouse_speed, 1.0);
        assert_eq!(normal.typing_wpm, 60);
    }
}
