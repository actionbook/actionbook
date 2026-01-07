/**
 * RecorderTools Unit Tests
 *
 * Tests for tool definitions including new scroll_to_bottom and go_back tools
 * TDD Step 4: New tool definitions
 * TDD Step 5: observe_page module parameter
 */

import { describe, it, expect } from 'vitest'
import { getRecorderTools } from '../../src/recorder/RecorderTools.js'

describe('getRecorderTools', () => {
  const tools = getRecorderTools()

  describe('existing tools', () => {
    it('should include navigate tool', () => {
      const tool = tools.find((t) => t.function.name === 'navigate')
      expect(tool).toBeDefined()
      expect(tool?.function.parameters.properties).toHaveProperty('url')
    })

    it('should include observe_page tool', () => {
      const tool = tools.find((t) => t.function.name === 'observe_page')
      expect(tool).toBeDefined()
      expect(tool?.function.parameters.properties).toHaveProperty('focus')
    })

    it('should include scroll tool', () => {
      const tool = tools.find((t) => t.function.name === 'scroll')
      expect(tool).toBeDefined()
    })
  })

  describe('register_element module parameter', () => {
    it('should have module parameter', () => {
      const tool = tools.find((t) => t.function.name === 'register_element')
      expect(tool?.function.parameters.properties).toHaveProperty('module')
    })

    it('should have correct module enum values for register_element', () => {
      const tool = tools.find((t) => t.function.name === 'register_element')
      const params = tool?.function.parameters.properties as Record<
        string,
        { type: string; enum?: string[] }
      >
      // register_element uses 'unknown' instead of 'all'
      const expectedModules = ['header', 'footer', 'sidebar', 'navibar', 'main', 'modal', 'breadcrumb', 'tab', 'unknown']
      expect(params.module.enum).toEqual(expect.arrayContaining(expectedModules))
    })

    it('should not require module parameter (optional)', () => {
      const tool = tools.find((t) => t.function.name === 'register_element')
      const required = tool?.function.parameters.required as string[]
      expect(required).not.toContain('module')
    })
  })

  describe('observe_page module parameter', () => {
    it('should have module parameter', () => {
      const tool = tools.find((t) => t.function.name === 'observe_page')
      expect(tool?.function.parameters.properties).toHaveProperty('module')
    })

    it('should have module as string type with enum', () => {
      const tool = tools.find((t) => t.function.name === 'observe_page')
      const params = tool?.function.parameters.properties as Record<
        string,
        { type: string; enum?: string[] }
      >
      expect(params.module.type).toBe('string')
      expect(params.module.enum).toBeDefined()
    })

    it('should have correct module enum values', () => {
      const tool = tools.find((t) => t.function.name === 'observe_page')
      const params = tool?.function.parameters.properties as Record<
        string,
        { type: string; enum?: string[] }
      >
      const expectedModules = ['header', 'footer', 'sidebar', 'navibar', 'main', 'modal', 'breadcrumb', 'tab', 'all']
      expect(params.module.enum).toEqual(expect.arrayContaining(expectedModules))
      expect(params.module.enum?.length).toBe(expectedModules.length)
    })

    it('should not require module parameter (optional)', () => {
      const tool = tools.find((t) => t.function.name === 'observe_page')
      const required = tool?.function.parameters.required as string[]
      expect(required).not.toContain('module')
    })
  })

  describe('scroll_to_bottom tool', () => {
    it('should include scroll_to_bottom tool', () => {
      const tool = tools.find((t) => t.function.name === 'scroll_to_bottom')
      expect(tool).toBeDefined()
    })

    it('should have correct description', () => {
      const tool = tools.find((t) => t.function.name === 'scroll_to_bottom')
      expect(tool?.function.description).toContain('bottom')
    })

    it('should have wait_after_scroll parameter', () => {
      const tool = tools.find((t) => t.function.name === 'scroll_to_bottom')
      expect(tool?.function.parameters.properties).toHaveProperty('wait_after_scroll')
    })

    it('should have wait_after_scroll as number type', () => {
      const tool = tools.find((t) => t.function.name === 'scroll_to_bottom')
      const params = tool?.function.parameters.properties as Record<string, { type: string }>
      expect(params.wait_after_scroll.type).toBe('number')
    })

    it('should not require wait_after_scroll (optional with default)', () => {
      const tool = tools.find((t) => t.function.name === 'scroll_to_bottom')
      const required = tool?.function.parameters.required as string[] | undefined
      expect(required).not.toContain('wait_after_scroll')
    })
  })

  describe('go_back tool', () => {
    it('should include go_back tool', () => {
      const tool = tools.find((t) => t.function.name === 'go_back')
      expect(tool).toBeDefined()
    })

    it('should have correct description', () => {
      const tool = tools.find((t) => t.function.name === 'go_back')
      expect(tool?.function.description).toContain('back')
    })

    it('should have no required parameters', () => {
      const tool = tools.find((t) => t.function.name === 'go_back')
      const required = tool?.function.parameters.required as string[] | undefined
      expect(required ?? []).toHaveLength(0)
    })
  })

  describe('tool count', () => {
    it('should have expected number of tools', () => {
      // Original: navigate, observe_page, set_page_context, register_element, interact, wait, scroll
      // New: scroll_to_bottom, go_back
      // Total: 9
      expect(tools.length).toBe(9)
    })
  })
})
