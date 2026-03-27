/// Readability extraction JavaScript (borrowed from pinchtab/readability.js)
///
/// Extracts meaningful text content from a page, stripping navigation,
/// ads, modals, and other non-content elements. Returns ~800 tokens/page
/// vs 10,000+ for full snapshot.
#[allow(dead_code)]
pub const READABILITY_JS: &str = r#"
(function() {
    function extractReadable() {
        // 1. Prefer semantic content containers
        var root = document.querySelector('article')
            || document.querySelector('[role="main"]')
            || document.querySelector('main');

        if (!root) {
            // 2. Clone body and strip junk
            root = document.body.cloneNode(true);

            // Remove navigation, footer, aside, header
            var junkSelectors = [
                'nav', 'footer', 'aside', 'header',
                '[role="navigation"]', '[role="banner"]', '[role="contentinfo"]',
                '[role="complementary"]',
                // Ads and popups
                '[class*="ad-"]', '[class*="ads"]', '[class*="popup"]',
                '[class*="modal"]', '[class*="overlay"]', '[class*="sidebar"]',
                '[class*="cookie"]', '[class*="consent"]', '[class*="banner"]',
                '[class*="language-selector"]', '[class*="lang-"]',
                // Common junk IDs
                '[id*="cookie"]', '[id*="consent"]', '[id*="popup"]',
                '[id*="modal"]', '[id*="sidebar"]', '[id*="ad-"]'
            ];

            for (var i = 0; i < junkSelectors.length; i++) {
                var els = root.querySelectorAll(junkSelectors[i]);
                for (var j = 0; j < els.length; j++) {
                    els[j].remove();
                }
            }
        }

        // 3. Remove scripts, styles, hidden elements, and obvious chrome
        var removeSelectors = ['nav', 'footer', 'aside', 'header',
                               '[role="navigation"]', '[role="banner"]',
                               '[role="contentinfo"]', '[role="complementary"]',
                               'script', 'style', 'noscript', 'template', 'svg',
                               '[hidden]', '[aria-hidden="true"]',
                               '[style*="display:none"]', '[style*="display: none"]'];
        for (var k = 0; k < removeSelectors.length; k++) {
            var toRemove = root.querySelectorAll(removeSelectors[k]);
            for (var l = 0; l < toRemove.length; l++) {
                toRemove[l].remove();
            }
        }

        // 4. Get text and normalize whitespace
        var text = root.innerText || root.textContent || '';
        return text.replace(/\n{3,}/g, '\n\n').replace(/[ \t]+/g, ' ').trim();
    }
    return extractReadable();
})()
"#;
