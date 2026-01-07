/**
 * URL Matcher utility for Playbook mode
 *
 * Provides URL pattern matching functionality to determine if a page
 * is a target page for element recording.
 */

/**
 * Check if a URL matches the target page pattern
 *
 * @param url - Full URL to check
 * @param pattern - Regex pattern to match against URL pathname (undefined = match all)
 * @returns true if URL matches the pattern or pattern is not set
 *
 * @example
 * // Match all pages (no pattern)
 * isTargetPage('https://example.com/any', undefined) // true
 *
 * @example
 * // Match search pages
 * isTargetPage('https://example.com/search', '^/search') // true
 * isTargetPage('https://example.com/home', '^/search') // false
 *
 * @example
 * // Match product detail pages
 * isTargetPage('https://example.com/products/123', '/products/\\d+') // true
 * isTargetPage('https://example.com/products/abc', '/products/\\d+') // false
 */
export function isTargetPage(url: string, pattern: string | undefined): boolean {
  // No pattern = match all pages
  if (!pattern) {
    return true
  }

  try {
    const urlObj = new URL(url)
    // Handle URLs without trailing slash (e.g., 'https://example.com' -> '/')
    const pathname = urlObj.pathname || '/'
    const regex = new RegExp(pattern)
    return regex.test(pathname)
  } catch {
    // Invalid URL - return false
    return false
  }
}
