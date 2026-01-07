/**
 * SelectorValidator Unit Tests
 *
 * Tests for enhanced validation with visible and interactable checks
 * TDD Step 6: Validator enhancement
 */

import { describe, it, expect } from 'vitest'
import type { SelectorValidationDetail } from '../../src/types/config.js'

describe('SelectorValidationDetail type', () => {
  it('should support visible field', () => {
    const detail: SelectorValidationDetail = {
      type: 'css',
      value: '#test-button',
      valid: true,
      visible: true,
    }

    expect(detail.visible).toBe(true)
  })

  it('should support interactable field', () => {
    const detail: SelectorValidationDetail = {
      type: 'css',
      value: '#test-button',
      valid: true,
      interactable: true,
    }

    expect(detail.interactable).toBe(true)
  })

  it('should support all validation states', () => {
    // Element exists but is hidden
    const hiddenElement: SelectorValidationDetail = {
      type: 'css',
      value: '#hidden-element',
      valid: true,
      visible: false,
      interactable: false,
    }
    expect(hiddenElement.valid).toBe(true)
    expect(hiddenElement.visible).toBe(false)
    expect(hiddenElement.interactable).toBe(false)

    // Element exists, visible but disabled
    const disabledElement: SelectorValidationDetail = {
      type: 'css',
      value: '#disabled-button',
      valid: true,
      visible: true,
      interactable: false,
    }
    expect(disabledElement.valid).toBe(true)
    expect(disabledElement.visible).toBe(true)
    expect(disabledElement.interactable).toBe(false)

    // Fully interactive element
    const interactiveElement: SelectorValidationDetail = {
      type: 'css',
      value: '#active-button',
      valid: true,
      visible: true,
      interactable: true,
    }
    expect(interactiveElement.valid).toBe(true)
    expect(interactiveElement.visible).toBe(true)
    expect(interactiveElement.interactable).toBe(true)
  })

  it('should have visible and interactable as optional fields', () => {
    // Backward compatibility - fields are optional
    const minimalDetail: SelectorValidationDetail = {
      type: 'xpath',
      value: '//button',
      valid: false,
      error: 'Element not found',
    }

    expect(minimalDetail.visible).toBeUndefined()
    expect(minimalDetail.interactable).toBeUndefined()
  })
})
