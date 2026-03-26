/// Privacy-frontend URL rewriting.
///
/// Rewrites URLs to use privacy-friendly frontends that are less likely
/// to block automated access. Inspired by x-tweet-fetcher's Nitter pattern.
///
/// Attempt to rewrite a URL to a privacy-friendly frontend.
///
/// Returns `(possibly_rewritten_url, was_rewritten)`.
/// Only rewrites when the host matches a known rule.
pub fn maybe_rewrite(url: &str) -> (String, bool) {
    // Parse scheme + host + rest
    let (scheme, after_scheme) = if let Some(rest) = url.strip_prefix("https://") {
        ("https://", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http://", rest)
    } else {
        // No scheme — treat as-is, but still try to match
        ("https://", url)
    };

    // Split host from path+query
    let (host_port, path_and_rest) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => (after_scheme, ""),
    };

    // Strip port for matching
    let host = host_port.split(':').next().unwrap_or(host_port);
    let host_lower = host.to_ascii_lowercase();

    // Apply rewrite rules
    if let Some(new_host) = rewrite_host(&host_lower) {
        let rewritten = format!("{}{}{}", scheme, new_host, path_and_rest);
        (rewritten, true)
    } else {
        (url.to_string(), false)
    }
}

/// Map a host to its privacy-friendly replacement.
fn rewrite_host(host: &str) -> Option<&'static str> {
    match host {
        "x.com" | "www.x.com" | "twitter.com" | "www.twitter.com" | "mobile.twitter.com" => {
            Some("xcancel.com")
        }
        "reddit.com" | "www.reddit.com" | "new.reddit.com" => Some("old.reddit.com"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_twitter_rewrite() {
        let (url, rewritten) = maybe_rewrite("https://x.com/elonmusk");
        assert!(rewritten);
        assert_eq!(url, "https://xcancel.com/elonmusk");
    }

    #[test]
    fn test_twitter_com_rewrite() {
        let (url, rewritten) = maybe_rewrite("https://twitter.com/user/status/123");
        assert!(rewritten);
        assert_eq!(url, "https://xcancel.com/user/status/123");
    }

    #[test]
    fn test_www_twitter_rewrite() {
        let (url, rewritten) = maybe_rewrite("https://www.twitter.com/user");
        assert!(rewritten);
        assert_eq!(url, "https://xcancel.com/user");
    }

    #[test]
    fn test_reddit_rewrite() {
        let (url, rewritten) = maybe_rewrite("https://www.reddit.com/r/rust");
        assert!(rewritten);
        assert_eq!(url, "https://old.reddit.com/r/rust");
    }

    #[test]
    fn test_new_reddit_rewrite() {
        let (url, rewritten) = maybe_rewrite("https://new.reddit.com/r/rust");
        assert!(rewritten);
        assert_eq!(url, "https://old.reddit.com/r/rust");
    }

    #[test]
    fn test_old_reddit_no_rewrite() {
        let (url, rewritten) = maybe_rewrite("https://old.reddit.com/r/rust");
        assert!(!rewritten);
        assert_eq!(url, "https://old.reddit.com/r/rust");
    }

    #[test]
    fn test_github_no_rewrite() {
        let (url, rewritten) = maybe_rewrite("https://github.com/rust-lang/rust");
        assert!(!rewritten);
        assert_eq!(url, "https://github.com/rust-lang/rust");
    }

    #[test]
    fn test_http_scheme_preserved() {
        let (url, rewritten) = maybe_rewrite("http://x.com/user");
        assert!(rewritten);
        assert_eq!(url, "http://xcancel.com/user");
    }

    #[test]
    fn test_preserves_path_and_query() {
        let (url, rewritten) = maybe_rewrite("https://x.com/user/status/123?s=20&t=abc");
        assert!(rewritten);
        assert_eq!(url, "https://xcancel.com/user/status/123?s=20&t=abc");
    }
}
