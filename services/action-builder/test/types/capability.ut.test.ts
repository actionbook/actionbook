/**
 * Capability Types Unit Tests
 *
 * Tests for PageModule type and ElementCapability.module field
 * TDD Step 1: Type definitions
 */

import { describe, it, expect } from 'vitest'
import type {
  PageModule,
  ElementCapability,
  ElementType,
  AllowMethod,
} from '../../src/types/capability.js'

describe('PageModule type', () => {
  it('should accept valid module values', () => {
    const validModules: PageModule[] = [
      'header',
      'footer',
      'sidebar',
      'navibar',
      'main',
      'modal',
      'breadcrumb',
      'tab',
      'unknown',
    ]

    // Each value should be assignable to PageModule
    validModules.forEach((module) => {
      const assigned: PageModule = module
      expect(assigned).toBe(module)
    })
  })

  it('should have all expected module values', () => {
    const expectedModules = [
      'header',
      'footer',
      'sidebar',
      'navibar',
      'main',
      'modal',
      'breadcrumb',
      'tab',
      'unknown',
    ]

    // Verify all expected values are valid PageModule
    expectedModules.forEach((module) => {
      const valid: PageModule = module as PageModule
      expect(valid).toBeDefined()
    })
  })
})

describe('ElementCapability.module', () => {
  it('should be optional - element without module is valid', () => {
    const element: ElementCapability = {
      id: 'test_button',
      selectors: [],
      description: 'A test button',
      element_type: 'button' as ElementType,
      allow_methods: ['click'] as AllowMethod[],
      discovered_at: new Date().toISOString(),
    }

    // module should be undefined when not set
    expect(element.module).toBeUndefined()
  })

  it('should accept valid module value', () => {
    const element: ElementCapability = {
      id: 'header_logo',
      selectors: [{ type: 'css', value: '#logo', priority: 1, confidence: 0.9 }],
      description: 'Header logo',
      element_type: 'link' as ElementType,
      allow_methods: ['click'] as AllowMethod[],
      discovered_at: new Date().toISOString(),
      module: 'header',
    }

    expect(element.module).toBe('header')
  })

  it('should accept all valid module values', () => {
    const modules: PageModule[] = [
      'header',
      'footer',
      'sidebar',
      'navibar',
      'main',
      'modal',
      'breadcrumb',
      'tab',
      'unknown',
    ]

    modules.forEach((module) => {
      const element: ElementCapability = {
        id: `test_${module}`,
        selectors: [],
        description: `Element in ${module}`,
        element_type: 'button' as ElementType,
        allow_methods: ['click'] as AllowMethod[],
        discovered_at: new Date().toISOString(),
        module,
      }

      expect(element.module).toBe(module)
    })
  })
})
