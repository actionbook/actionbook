//! Human-like mouse movement and typing simulation (borrowed from pinchtab/human.go)
//!
//! Generates bezier-curve mouse paths and natural typing patterns
//! to avoid detection by anti-bot systems.

use rand::Rng;

/// Generate a bezier curve mouse path from start to end point
pub fn bezier_mouse_path(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> Vec<(f64, f64)> {
    let mut rng = rand::thread_rng();

    let dx = end_x - start_x;
    let dy = end_y - start_y;
    let distance = (dx * dx + dy * dy).sqrt();

    // Number of steps based on distance
    let steps = ((distance / 20.0).clamp(5.0, 30.0)) as usize;

    // Random control points for bezier curve (perpendicular offset)
    let offset1 = rng.gen_range(-50.0..50.0);
    let offset2 = rng.gen_range(-50.0..50.0);

    let cp1_x = start_x + dx * 0.3 + dy.signum() * offset1;
    let cp1_y = start_y + dy * 0.3 - dx.signum() * offset1;
    let cp2_x = start_x + dx * 0.7 + dy.signum() * offset2;
    let cp2_y = start_y + dy * 0.7 - dx.signum() * offset2;

    let mut points = Vec::with_capacity(steps);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let it = 1.0 - t;

        // Cubic bezier
        let x = it * it * it * start_x
            + 3.0 * it * it * t * cp1_x
            + 3.0 * it * t * t * cp2_x
            + t * t * t * end_x;
        let y = it * it * it * start_y
            + 3.0 * it * it * t * cp1_y
            + 3.0 * it * t * t * cp2_y
            + t * t * t * end_y;

        // Add small jitter (±2px)
        let jx = x + rng.gen_range(-2.0..2.0);
        let jy = y + rng.gen_range(-2.0..2.0);
        points.push((jx, jy));
    }

    points
}

/// Generate a random start position offset from the target
pub fn random_start_offset(target_x: f64, target_y: f64) -> (f64, f64) {
    let mut rng = rand::thread_rng();
    let offset_x = rng.gen_range(50.0..250.0) * if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
    let offset_y = rng.gen_range(50.0..250.0) * if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
    (
        (target_x + offset_x).max(0.0),
        (target_y + offset_y).max(0.0),
    )
}

/// Generate typing delays for each character (milliseconds)
pub fn typing_delays(text: &str, fast: bool) -> Vec<(char, u64)> {
    let mut rng = rand::thread_rng();
    let base_delay: u64 = if fast { 40 } else { 80 };

    let chars: Vec<char> = text.chars().collect();
    let mut result = Vec::with_capacity(chars.len());
    let mut prev_char = '\0';

    for &ch in &chars {
        let mut delay = base_delay + rng.gen_range(0..40);

        // Faster for repeated characters
        if ch == prev_char {
            delay = delay / 2;
        }

        // Occasional pause (5% chance)
        if rng.gen_range(0..100) < 5 {
            delay += rng.gen_range(200..500);
        }

        // Simulate typo (3% chance, ASCII letters only to avoid garbage chars)
        if ch.is_ascii_alphabetic() && rng.gen_range(0..100) < 3 {
            // Pick a nearby key on QWERTY layout
            let neighbors: &[u8] = match ch.to_ascii_lowercase() as u8 {
                b'a' => b"sq",
                b'b' => b"vn",
                b'c' => b"xv",
                b'd' => b"sf",
                b'e' => b"wr",
                b'f' => b"dg",
                b'g' => b"fh",
                b'h' => b"gj",
                b'i' => b"uo",
                b'j' => b"hk",
                b'k' => b"jl",
                b'l' => b"k;",
                b'm' => b"n,",
                b'n' => b"bm",
                b'o' => b"ip",
                b'p' => b"o[",
                b'q' => b"wa",
                b'r' => b"et",
                b's' => b"ad",
                b't' => b"ry",
                b'u' => b"yi",
                b'v' => b"cb",
                b'w' => b"qe",
                b'x' => b"zc",
                b'y' => b"tu",
                b'z' => b"x",
                _ => b"",
            };
            if !neighbors.is_empty() {
                let wrong = neighbors[rng.gen_range(0..neighbors.len())] as char;
                let wrong = if ch.is_ascii_uppercase() {
                    wrong.to_ascii_uppercase()
                } else {
                    wrong
                };
                result.push((wrong, delay));
                result.push(('\u{0008}', rng.gen_range(50..100))); // backspace
                delay = rng.gen_range(30..60);
            }
        }

        result.push((ch, delay));
        prev_char = ch;
    }

    result
}

/// Pre-click delay (human pause before clicking)
pub fn pre_click_delay_ms() -> u64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(50..200)
}

/// Click hold duration
#[allow(dead_code)]
pub fn click_hold_ms() -> u64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(30..120)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_path_has_expected_length() {
        let path = bezier_mouse_path(0.0, 0.0, 200.0, 200.0);
        assert!(path.len() >= 5);
        assert!(path.len() <= 31);
    }

    #[test]
    fn typing_delays_generates_entries() {
        let delays = typing_delays("hello", false);
        // At least one entry per character (maybe more due to typos)
        assert!(delays.len() >= 5);
    }

    #[test]
    fn random_start_offset_is_away_from_target() {
        let (sx, sy) = random_start_offset(100.0, 100.0);
        let dist = ((sx - 100.0).powi(2) + (sy - 100.0).powi(2)).sqrt();
        assert!(dist >= 50.0);
    }
}
