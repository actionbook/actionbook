/**
 * RecorderToolExecutor Unit Tests
 *
 * Tests navigate handling with external domain detection and URL tracking.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { RecorderToolExecutor, type NavigateResult } from '../../src/recorder/RecorderToolExecutor';
import type { BrowserAdapter } from '../../src/browser/BrowserAdapter';
import type { ElementCapability } from '../../src/types/index';

// Mock browser adapter
const createMockBrowser = (): BrowserAdapter => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  getPage: vi.fn().mockResolvedValue({ url: () => 'https://test.com' }),
  navigate: vi.fn().mockResolvedValue(undefined),
  observe: vi.fn().mockResolvedValue([]),
  act: vi.fn().mockResolvedValue({ success: true }),
  actWithSelector: vi.fn().mockResolvedValue({ success: true }),
  autoClosePopups: vi.fn().mockResolvedValue(0),
  getElementAttributesFromXPath: vi.fn().mockResolvedValue(null),
  wait: vi.fn().mockResolvedValue(undefined),
  waitForText: vi.fn().mockResolvedValue(undefined),
  scroll: vi.fn().mockResolvedValue(undefined),
  close: vi.fn().mockResolvedValue(undefined),
});

describe('RecorderToolExecutor', () => {
  let mockBrowser: BrowserAdapter;
  let mockHandlers: {
    ensureSiteCapability: ReturnType<typeof vi.fn>;
    registerElement: ReturnType<typeof vi.fn>;
    setPageContext: ReturnType<typeof vi.fn>;
    onNavigate: ReturnType<typeof vi.fn>;
    getCurrentUrl: ReturnType<typeof vi.fn>;
    getPreviousUrl: ReturnType<typeof vi.fn>;
    extractMultipleSelectors: ReturnType<typeof vi.fn>;
    detectTemplatePattern: ReturnType<typeof vi.fn>;
    inferElementType: ReturnType<typeof vi.fn>;
    inferAllowMethods: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    mockBrowser = createMockBrowser();
    mockHandlers = {
      ensureSiteCapability: vi.fn(),
      registerElement: vi.fn(),
      setPageContext: vi.fn(),
      onNavigate: vi.fn().mockReturnValue({ isNew: true }),
      getCurrentUrl: vi.fn().mockReturnValue('https://test.com'),
      getPreviousUrl: vi.fn().mockReturnValue(null),
      extractMultipleSelectors: vi.fn().mockReturnValue([]),
      detectTemplatePattern: vi.fn().mockReturnValue(null),
      inferElementType: vi.fn().mockReturnValue('button'),
      inferAllowMethods: vi.fn().mockReturnValue(['click']),
    };
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  // ==========================================================================
  // Navigate Tool Tests
  // ==========================================================================
  describe('navigate tool', () => {
    // UT-RTE-01: Successful navigation to new URL
    it('UT-RTE-01: Successful navigation to new URL', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      mockHandlers.onNavigate.mockReturnValue({ isNew: true });

      const result = await executor.execute('navigate', { url: 'https://test.com/page1' });

      expect(mockBrowser.navigate).toHaveBeenCalledWith('https://test.com/page1');
      expect(mockBrowser.autoClosePopups).toHaveBeenCalled();
      expect(result.output).toEqual({ success: true, url: 'https://test.com/page1' });
    });

    // UT-RTE-02: External domain blocks navigation without loading the page
    it('UT-RTE-02: External domain blocks navigation without loading the page', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      mockHandlers.onNavigate.mockReturnValue({ isNew: false, reason: 'external_domain' });
      mockHandlers.getCurrentUrl.mockReturnValue('https://primary.com/page2');

      const result = await executor.execute('navigate', { url: 'https://external.com/page' });

      // Should NOT navigate at all - navigation is blocked before loading
      expect(mockBrowser.navigate).not.toHaveBeenCalled();
      // Result should indicate blocked external domain
      expect((result.output as any).success).toBe(false);
      expect((result.output as any).external_domain).toBe(true);
      expect((result.output as any).blocked).toBe(true);
      expect((result.output as any).url).toBe('https://primary.com/page2');
      expect((result.output as any).message).toContain('external domain');
    });

    // UT-RTE-03: External domain without current URL returns target URL
    it('UT-RTE-03: External domain without current URL returns target URL', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      mockHandlers.onNavigate.mockReturnValue({ isNew: false, reason: 'external_domain' });
      mockHandlers.getCurrentUrl.mockReturnValue(null);

      const result = await executor.execute('navigate', { url: 'https://external.com/page' });

      // Should NOT navigate at all
      expect(mockBrowser.navigate).not.toHaveBeenCalled();
      expect((result.output as any).success).toBe(false);
      expect((result.output as any).external_domain).toBe(true);
      expect((result.output as any).blocked).toBe(true);
      expect((result.output as any).url).toBe('https://external.com/page');
    });

    // UT-RTE-04: Already visited URL blocks navigation without loading the page
    it('UT-RTE-04: Already visited URL blocks navigation without loading the page', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      mockHandlers.onNavigate.mockReturnValue({ isNew: false, reason: 'already_visited' });
      mockHandlers.getCurrentUrl.mockReturnValue('https://test.com/page2');

      const result = await executor.execute('navigate', { url: 'https://test.com/' });

      // Should NOT navigate at all - navigation is blocked before loading
      expect(mockBrowser.navigate).not.toHaveBeenCalled();
      expect((result.output as any).success).toBe(false);
      expect((result.output as any).already_visited).toBe(true);
      expect((result.output as any).blocked).toBe(true);
      expect((result.output as any).message).toContain('already visited');
    });

    // UT-RTE-05: Already visited URL without current URL returns target URL
    it('UT-RTE-05: Already visited URL without current URL returns target URL', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      mockHandlers.onNavigate.mockReturnValue({ isNew: false, reason: 'already_visited' });
      mockHandlers.getCurrentUrl.mockReturnValue(null);

      const result = await executor.execute('navigate', { url: 'https://test.com/' });

      // Should NOT navigate at all
      expect(mockBrowser.navigate).not.toHaveBeenCalled();
      expect((result.output as any).success).toBe(false);
      expect((result.output as any).already_visited).toBe(true);
      expect((result.output as any).blocked).toBe(true);
    });

    // UT-RTE-06: ensureSiteCapability only called for successful navigation
    it('UT-RTE-06: ensureSiteCapability only called for successful navigation', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      mockHandlers.onNavigate.mockReturnValue({ isNew: true });

      await executor.execute('navigate', { url: 'https://example.com/path?query=1' });

      expect(mockBrowser.navigate).toHaveBeenCalledWith('https://example.com/path?query=1');
      expect(mockHandlers.ensureSiteCapability).toHaveBeenCalledWith('example.com');
    });
  });

  // ==========================================================================
  // observe_page Tool Tests
  // ==========================================================================
  describe('observe_page tool', () => {
    // UT-RTE-07: observe_page passes timeout to browser
    it('UT-RTE-07: observe_page passes timeout to browser', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      (mockBrowser.observe as any).mockResolvedValue([
        { description: 'Button', selector: '//button', method: 'click' },
      ]);

      await executor.execute('observe_page', { focus: 'buttons', _timeoutMs: 5000 });

      expect(mockBrowser.observe).toHaveBeenCalledWith('buttons', 5000);
    });

    // UT-RTE-08: observe_page returns elements found
    it('UT-RTE-08: observe_page returns elements found', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      (mockBrowser.observe as any).mockResolvedValue([
        { description: 'Button 1', selector: '//button[1]', method: 'click' },
        { description: 'Button 2', selector: '//button[2]', method: 'click' },
        { description: 'Input', selector: '//input', method: 'type' },
      ]);

      const result = await executor.execute('observe_page', { focus: 'interactive elements' });

      expect((result.output as any).elements_found).toBe(3);
      expect((result.output as any).elements).toHaveLength(3);
    });

    // UT-RTE-09: observe_page limits to 20 elements
    it('UT-RTE-09: observe_page limits to 20 elements', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      // Return 30 elements
      const manyElements = Array.from({ length: 30 }, (_, i) => ({
        description: `Element ${i}`,
        selector: `//el[${i}]`,
        method: 'click',
      }));
      (mockBrowser.observe as any).mockResolvedValue(manyElements);

      const result = await executor.execute('observe_page', { focus: 'all' });

      expect((result.output as any).elements_found).toBe(30);
      expect((result.output as any).elements).toHaveLength(20); // Capped at 20
    });
  });

  // ==========================================================================
  // register_element Tool Tests
  // ==========================================================================
  describe('register_element tool', () => {
    // UT-RTE-10: register_element calls registerElement handler
    it('UT-RTE-10: register_element calls registerElement handler', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      await executor.execute('register_element', {
        element_id: 'search_button',
        description: 'Search button',
        element_type: 'button',
        allow_methods: ['click'],
        xpath_selector: '//button[@id="search"]',
      });

      expect(mockHandlers.registerElement).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 'search_button',
          description: 'Search button',
          element_type: 'button',
          allow_methods: ['click'],
        })
      );
    });

    // UT-RTE-11: register_element extracts attributes from XPath
    it('UT-RTE-11: register_element extracts attributes from XPath', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      (mockBrowser.getElementAttributesFromXPath as any).mockResolvedValue({
        id: 'btn-submit',
        dataTestId: 'submit-button',
        ariaLabel: 'Submit form',
        cssSelector: '#btn-submit',
      });

      await executor.execute('register_element', {
        element_id: 'submit_btn',
        description: 'Submit button',
        element_type: 'button',
        allow_methods: ['click'],
        xpath_selector: '//button[@type="submit"]',
      });

      // Should call getElementAttributesFromXPath
      expect(mockBrowser.getElementAttributesFromXPath).toHaveBeenCalledWith('//button[@type="submit"]');

      // Should register with extracted selectors
      const registeredElement = mockHandlers.registerElement.mock.calls[0][0] as ElementCapability;
      const selectorTypes = registeredElement.selectors.map(s => s.type);

      expect(selectorTypes).toContain('id');
      expect(selectorTypes).toContain('data-testid');
      expect(selectorTypes).toContain('aria-label');
    });

    // UT-RTE-12: register_element falls back to observe when no XPath
    it('UT-RTE-12: register_element falls back to observe when no XPath', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      (mockBrowser.observe as any).mockResolvedValue([
        { description: 'Found element', selector: 'xpath=//div[@class="target"]' },
      ]);

      await executor.execute('register_element', {
        element_id: 'target_div',
        description: 'Target div element',
        element_type: 'other',
        allow_methods: ['click'],
        // No xpath_selector provided
      });

      // Should fall back to observe
      expect(mockBrowser.observe).toHaveBeenCalledWith('Target div element');
    });
  });

  // ==========================================================================
  // set_page_context Tool Tests
  // ==========================================================================
  describe('set_page_context tool', () => {
    // UT-RTE-13: set_page_context calls setPageContext handler
    it('UT-RTE-13: set_page_context calls setPageContext handler', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      (mockBrowser.getPage as any).mockResolvedValue({
        url: () => 'https://test.com/dashboard',
      });

      await executor.execute('set_page_context', {
        page_type: 'dashboard',
        page_name: 'User Dashboard',
        page_description: 'Main dashboard for users',
        url_pattern: '/dashboard',
      });

      expect(mockHandlers.setPageContext).toHaveBeenCalledWith({
        pageType: 'dashboard',
        pageName: 'User Dashboard',
        pageDescription: 'Main dashboard for users',
        urlPattern: '/dashboard',
        concreteUrl: 'https://test.com/dashboard',
      });
    });
  });

  // ==========================================================================
  // Other Tools Tests
  // ==========================================================================
  describe('other tools', () => {
    // UT-RTE-14: wait tool with seconds
    it('UT-RTE-14: wait tool with seconds', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      await executor.execute('wait', { seconds: 2 });

      expect(mockBrowser.wait).toHaveBeenCalledWith(2000);
    });

    // UT-RTE-15: wait tool with forText
    it('UT-RTE-15: wait tool with forText', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      await executor.execute('wait', { forText: 'Loading complete' });

      expect(mockBrowser.waitForText).toHaveBeenCalledWith('Loading complete');
    });

    // UT-RTE-16: scroll tool
    it('UT-RTE-16: scroll tool', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      await executor.execute('scroll', { direction: 'down', amount: 500 });

      expect(mockBrowser.scroll).toHaveBeenCalledWith('down', 500);
    });

    // UT-RTE-17: unknown tool returns error
    it('UT-RTE-17: unknown tool returns error', async () => {
      const executor = new RecorderToolExecutor(mockBrowser, mockHandlers as any);

      const result = await executor.execute('unknown_tool', {});

      expect((result.output as any).error).toContain('Unknown tool');
    });
  });
});
