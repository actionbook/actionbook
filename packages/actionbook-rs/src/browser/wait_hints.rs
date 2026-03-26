/// Domain-aware wait strategies for browser navigation.
///
/// Different websites have vastly different JS rendering times.
/// This module provides sensible defaults per domain so that
/// `browser fetch` and other one-shot commands wait just long enough.
///
/// Wait hint categories with associated millisecond durations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitHint {
    /// Static pages, no JS needed (0 ms extra wait)
    Instant,
    /// Light SPAs, fast servers (3 000 ms)
    Fast,
    /// Average sites (5 000 ms) — default
    Normal,
    /// Heavy SPAs, aggressive anti-bot (10 000 ms)
    Slow,
    /// Extremely heavy JS, TikTok-class (15 000 ms)
    Heavy,
}

impl WaitHint {
    pub fn as_millis(self) -> u64 {
        match self {
            WaitHint::Instant => 0,
            WaitHint::Fast => 3_000,
            WaitHint::Normal => 5_000,
            WaitHint::Slow => 10_000,
            WaitHint::Heavy => 15_000,
        }
    }
}

/// Built-in domain → hint mapping.
/// Domains are matched by suffix so `en.wikipedia.org` matches `wikipedia.org`.
const DOMAIN_HINTS: &[(&[&str], WaitHint)] = &[
    (&["x.com", "twitter.com", "xcancel.com"], WaitHint::Slow),
    (&["douyin.com", "tiktok.com"], WaitHint::Heavy),
    (&["github.com", "gitlab.com"], WaitHint::Fast),
    (&["wikipedia.org"], WaitHint::Instant),
    (&["old.reddit.com"], WaitHint::Fast),
    (&["reddit.com"], WaitHint::Slow),
    (&["youtube.com"], WaitHint::Slow),
    (&["linkedin.com"], WaitHint::Slow),
    (&["medium.com"], WaitHint::Fast),
    (&["stackoverflow.com"], WaitHint::Fast),
];

/// Parse a CLI hint string into a `WaitHint`.
fn parse_hint(s: &str) -> Option<WaitHint> {
    match s.to_ascii_lowercase().as_str() {
        "instant" => Some(WaitHint::Instant),
        "fast" => Some(WaitHint::Fast),
        "normal" => Some(WaitHint::Normal),
        "slow" => Some(WaitHint::Slow),
        "heavy" => Some(WaitHint::Heavy),
        _ => None,
    }
}

/// Extract the host from a URL string (cheap, no full URL parser needed).
fn extract_host(url: &str) -> Option<&str> {
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = after_scheme.split('/').next()?;
    // Strip port
    let host = host.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// Check if `host` ends with `.domain` or equals `domain`.
fn host_matches(host: &str, domain: &str) -> bool {
    if host == domain {
        return true;
    }
    host.ends_with(&format!(".{}", domain))
}

/// Resolve the wait duration in milliseconds for a given URL.
///
/// Priority:
/// 1. `cli_override` (e.g. `--wait-hint fast`) — highest
/// 2. Domain match from `DOMAIN_HINTS`
/// 3. Default: `WaitHint::Normal` (5 000 ms)
pub fn resolve_wait_ms(url: &str, cli_override: Option<&str>) -> u64 {
    // CLI override wins
    if let Some(hint_str) = cli_override {
        // Allow raw milliseconds too
        if let Ok(ms) = hint_str.parse::<u64>() {
            return ms;
        }
        if let Some(hint) = parse_hint(hint_str) {
            return hint.as_millis();
        }
        tracing::warn!("Unknown wait hint '{}', using default (normal)", hint_str);
    }

    // Domain matching
    if let Some(host) = extract_host(url) {
        // More-specific rules first: check `old.reddit.com` before `reddit.com`
        for (domains, hint) in DOMAIN_HINTS {
            for domain in *domains {
                if host_matches(host, domain) {
                    return hint.as_millis();
                }
            }
        }
    }

    WaitHint::Normal.as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hint_millis() {
        assert_eq!(WaitHint::Instant.as_millis(), 0);
        assert_eq!(WaitHint::Fast.as_millis(), 3_000);
        assert_eq!(WaitHint::Normal.as_millis(), 5_000);
        assert_eq!(WaitHint::Slow.as_millis(), 10_000);
        assert_eq!(WaitHint::Heavy.as_millis(), 15_000);
    }

    #[test]
    fn test_domain_matching() {
        assert_eq!(resolve_wait_ms("https://x.com/elonmusk", None), 10_000);
        assert_eq!(resolve_wait_ms("https://twitter.com/user", None), 10_000);
        assert_eq!(resolve_wait_ms("https://github.com/repo", None), 3_000);
        assert_eq!(
            resolve_wait_ms("https://en.wikipedia.org/wiki/Rust", None),
            0
        );
        assert_eq!(
            resolve_wait_ms("https://old.reddit.com/r/rust", None),
            3_000
        );
        assert_eq!(
            resolve_wait_ms("https://www.reddit.com/r/rust", None),
            10_000
        );
        assert_eq!(resolve_wait_ms("https://tiktok.com/@user", None), 15_000);
    }

    #[test]
    fn test_cli_override() {
        assert_eq!(resolve_wait_ms("https://github.com", Some("slow")), 10_000);
        assert_eq!(resolve_wait_ms("https://github.com", Some("instant")), 0);
        // Raw millis
        assert_eq!(resolve_wait_ms("https://github.com", Some("7500")), 7_500);
    }

    #[test]
    fn test_unknown_domain_gets_default() {
        assert_eq!(
            resolve_wait_ms("https://some-random-site.io/page", None),
            5_000
        );
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(extract_host("https://github.com/repo"), Some("github.com"));
        assert_eq!(
            extract_host("http://localhost:3000/test"),
            Some("localhost")
        );
        assert_eq!(
            extract_host("https://en.wikipedia.org/wiki/Rust"),
            Some("en.wikipedia.org")
        );
    }
}
