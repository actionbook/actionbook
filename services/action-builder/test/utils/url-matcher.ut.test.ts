/**
 * URL Matcher Unit Tests
 *
 * Tests for isTargetPage function that matches URLs against patterns
 * TDD Step 3: isTargetPage logic
 */

import { describe, it, expect } from 'vitest'
import { isTargetPage } from '../../src/utils/url-matcher.js'

describe('isTargetPage', () => {
  describe('when no pattern configured', () => {
    it('should return true for any URL', () => {
      expect(isTargetPage('https://example.com/any/path', undefined)).toBe(true)
      expect(isTargetPage('https://example.com/', undefined)).toBe(true)
      expect(isTargetPage('https://example.com/deep/nested/path', undefined)).toBe(true)
    })

    it('should return true when pattern is empty string', () => {
      expect(isTargetPage('https://example.com/any/path', '')).toBe(true)
    })
  })

  describe('when pattern is configured', () => {
    it('should match URL pathname against simple pattern', () => {
      const pattern = '^/search'

      expect(isTargetPage('https://example.com/search', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/search/results', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/home', pattern)).toBe(false)
      expect(isTargetPage('https://example.com/', pattern)).toBe(false)
    })

    it('should match exact path pattern', () => {
      const pattern = '^/products$'

      expect(isTargetPage('https://example.com/products', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/products/', pattern)).toBe(false)
      expect(isTargetPage('https://example.com/products/123', pattern)).toBe(false)
    })

    it('should handle regex patterns with digits', () => {
      const pattern = '/products/\\d+'

      expect(isTargetPage('https://example.com/products/123', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/products/456789', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/products/abc', pattern)).toBe(false)
      expect(isTargetPage('https://example.com/products/', pattern)).toBe(false)
    })

    it('should handle pattern with query params in URL', () => {
      const pattern = '^/search'

      // Query params should not affect pathname matching
      expect(isTargetPage('https://example.com/search?q=test', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/search?q=test&page=1', pattern)).toBe(true)
    })

    it('should handle pattern with hash in URL', () => {
      const pattern = '^/app'

      // Hash should not affect pathname matching
      expect(isTargetPage('https://example.com/app#section', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/app#/route', pattern)).toBe(true)
    })

    it('should handle complex regex patterns', () => {
      const pattern = '^/(products|categories)/\\d+/details$'

      expect(isTargetPage('https://example.com/products/123/details', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/categories/456/details', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/products/abc/details', pattern)).toBe(false)
      expect(isTargetPage('https://example.com/products/123', pattern)).toBe(false)
    })
  })

  describe('edge cases', () => {
    it('should handle root path', () => {
      const pattern = '^/$'

      expect(isTargetPage('https://example.com/', pattern)).toBe(true)
      expect(isTargetPage('https://example.com', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/home', pattern)).toBe(false)
    })

    it('should handle invalid URL gracefully', () => {
      // Invalid URLs should return false
      expect(isTargetPage('not-a-valid-url', '^/test')).toBe(false)
      expect(isTargetPage('', '^/test')).toBe(false)
    })

    it('should handle case-sensitive matching', () => {
      const pattern = '/Search'

      expect(isTargetPage('https://example.com/Search', pattern)).toBe(true)
      expect(isTargetPage('https://example.com/search', pattern)).toBe(false)
    })
  })
})
