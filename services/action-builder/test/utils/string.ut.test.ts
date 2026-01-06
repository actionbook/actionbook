/**
 * String Utility Unit Tests
 *
 * Tests for CSS selector utilities: hasSpecialCssChars, createIdSelector
 */

import { describe, it, expect } from 'vitest'
import { hasSpecialCssChars, createIdSelector } from '../../src/utils/string.js'

describe('hasSpecialCssChars', () => {
  describe('should return true for IDs with CSS special characters', () => {
    it('UT-STR-01: detects dot in ID', () => {
      expect(hasSpecialCssChars('cs.AI')).toBe(true)
      expect(hasSpecialCssChars('section.name')).toBe(true)
      expect(hasSpecialCssChars('a.b.c')).toBe(true)
    })

    it('UT-STR-02: detects colon in ID', () => {
      expect(hasSpecialCssChars('my:id')).toBe(true)
      expect(hasSpecialCssChars('namespace:element')).toBe(true)
    })

    it('UT-STR-03: detects brackets in ID', () => {
      expect(hasSpecialCssChars('arr[0]')).toBe(true)
      expect(hasSpecialCssChars('data[key]')).toBe(true)
    })

    it('UT-STR-04: detects other special characters', () => {
      expect(hasSpecialCssChars('id#hash')).toBe(true)
      expect(hasSpecialCssChars('path/to')).toBe(true)
      expect(hasSpecialCssChars('key=value')).toBe(true)
      expect(hasSpecialCssChars('question?')).toBe(true)
      expect(hasSpecialCssChars('star*')).toBe(true)
      expect(hasSpecialCssChars('plus+')).toBe(true)
      expect(hasSpecialCssChars('caret^')).toBe(true)
      expect(hasSpecialCssChars('dollar$')).toBe(true) // $ is CSS special char (used in [attr$=value])
    })
  })

  describe('should return false for safe IDs', () => {
    it('UT-STR-05: allows simple alphanumeric IDs', () => {
      expect(hasSpecialCssChars('simple')).toBe(false)
      expect(hasSpecialCssChars('button123')).toBe(false)
      expect(hasSpecialCssChars('myElement')).toBe(false)
    })

    it('UT-STR-06: allows IDs with hyphens and underscores', () => {
      expect(hasSpecialCssChars('my-id')).toBe(false)
      expect(hasSpecialCssChars('my_id')).toBe(false)
      expect(hasSpecialCssChars('my-long_id-123')).toBe(false)
    })

    it('UT-STR-07: allows empty string', () => {
      expect(hasSpecialCssChars('')).toBe(false)
    })
  })
})

describe('createIdSelector', () => {
  describe('should use attribute selector for IDs with special characters', () => {
    it('UT-STR-08: converts dot-containing ID to attribute selector', () => {
      expect(createIdSelector('cs.AI')).toBe('[id="cs.AI"]')
      expect(createIdSelector('section.name')).toBe('[id="section.name"]')
    })

    it('UT-STR-09: converts colon-containing ID to attribute selector', () => {
      expect(createIdSelector('my:id')).toBe('[id="my:id"]')
    })

    it('UT-STR-10: converts bracket-containing ID to attribute selector', () => {
      expect(createIdSelector('arr[0]')).toBe('[id="arr[0]"]')
    })

    it('UT-STR-11: escapes quotes in ID value', () => {
      expect(createIdSelector('id"quoted')).toBe('[id="id\\"quoted"]')
      expect(createIdSelector('double""quote')).toBe('[id="double\\"\\"quote"]')
    })
  })

  describe('should use standard ID selector for safe IDs', () => {
    it('UT-STR-12: returns #id format for simple IDs', () => {
      expect(createIdSelector('simple')).toBe('#simple')
      expect(createIdSelector('button123')).toBe('#button123')
    })

    it('UT-STR-13: returns #id format for IDs with hyphens/underscores', () => {
      expect(createIdSelector('my-id')).toBe('#my-id')
      expect(createIdSelector('my_id')).toBe('#my_id')
    })
  })

  describe('edge cases', () => {
    it('UT-STR-14: returns empty string for empty input', () => {
      expect(createIdSelector('')).toBe('')
    })

    it('UT-STR-15: handles real-world arxiv.org IDs', () => {
      // These are real IDs from arxiv.org that caused validation failures
      expect(createIdSelector('cs.AI')).toBe('[id="cs.AI"]')
      expect(createIdSelector('cs.CL')).toBe('[id="cs.CL"]')
      expect(createIdSelector('math.CO')).toBe('[id="math.CO"]')
      expect(createIdSelector('stat.ML')).toBe('[id="stat.ML"]')
    })
  })
})
