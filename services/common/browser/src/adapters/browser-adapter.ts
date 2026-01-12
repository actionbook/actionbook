/**
 * BrowserAdapter - Unified interface for browser automation
 *
 * This interface defines the complete set of operations for browser automation,
 * including basic operations, AI-powered element discovery, and action execution.
 *
 * Implementations:
 * - StagehandBrowser: Local Playwright + Stagehand AI
 * - AgentCoreBrowser: AWS Agent Core Browser Tool (cloud-based)
 */

import type {
  NavigateOptions,
  ScreenshotOptions,
  WaitForSelectorOptions,
  ScrollDirection,
  ObserveResult,
  ActionObject,
  ElementAttributes,
  TokenStats,
} from '../types/index.js';

/**
 * Unified browser adapter interface
 *
 * Combines basic browser automation with AI capabilities:
 * - Lifecycle: initialize, close
 * - Navigation: navigate, goBack
 * - Page info: getUrl, getTitle, getContent
 * - Screenshot: screenshot
 * - Waiting: waitForSelector, wait, waitForText
 * - Scrolling: scroll, scrollToBottom
 * - AI: observe, act, actWithSelector
 * - Element: getElementAttributesFromXPath, getPage
 * - Helpers: autoClosePopups
 * - Metrics: getTokenStats (optional)
 */
export interface BrowserAdapter {
  // ============================================
  // Lifecycle
  // ============================================

  /**
   * Initialize the browser instance
   * Must be called before any other operations
   */
  initialize(): Promise<void>;

  /**
   * Close the browser and release resources
   */
  close(): Promise<void>;

  // ============================================
  // Navigation
  // ============================================

  /**
   * Navigate to a URL
   * @param url - Target URL
   * @param options - Navigation options
   */
  navigate(url: string, options?: NavigateOptions): Promise<void>;

  /**
   * Navigate back in browser history
   */
  goBack(): Promise<void>;

  // ============================================
  // Page Information
  // ============================================

  /**
   * Get the current page URL
   */
  getUrl(): string;

  /**
   * Get the current page title
   */
  getTitle(): Promise<string>;

  /**
   * Get the page HTML content
   */
  getContent(): Promise<string>;

  // ============================================
  // Screenshot
  // ============================================

  /**
   * Take a screenshot of the page
   * @param options - Screenshot options
   * @returns Screenshot as Buffer (PNG/JPEG)
   */
  screenshot(options?: ScreenshotOptions): Promise<Buffer>;

  // ============================================
  // Waiting
  // ============================================

  /**
   * Wait for a selector to appear on the page
   * @param selector - CSS or XPath selector
   * @param options - Wait options
   */
  waitForSelector(selector: string, options?: WaitForSelectorOptions): Promise<void>;

  /**
   * Wait for a specified duration
   * @param ms - Duration in milliseconds
   */
  wait(ms: number): Promise<void>;

  /**
   * Wait for text to appear on the page
   * @param text - Text to wait for
   * @param timeout - Timeout in milliseconds (default: 30000)
   */
  waitForText(text: string, timeout?: number): Promise<void>;

  // ============================================
  // Scrolling
  // ============================================

  /**
   * Scroll the page
   * @param direction - Scroll direction ('up' or 'down')
   * @param amount - Scroll amount in pixels
   */
  scroll(direction: ScrollDirection, amount?: number): Promise<void>;

  /**
   * Scroll to the bottom of the page
   * Useful for loading lazy-loaded content
   * @param waitAfterMs - Time to wait after reaching bottom
   */
  scrollToBottom(waitAfterMs?: number): Promise<void>;

  // ============================================
  // AI Capabilities
  // ============================================

  /**
   * Observe page elements using AI
   *
   * Uses LLM to analyze the page and find elements matching
   * the natural language instruction.
   *
   * @param instruction - Natural language description of what to find
   *   e.g., "find the search button", "locate the login form"
   * @param timeoutMs - Timeout in milliseconds (default: 30000)
   * @returns Array of observed elements with selectors
   *
   * @example
   * const elements = await browser.observe('find all navigation links');
   * console.log(elements[0].selector); // xpath=//nav//a[1]
   */
  observe(instruction: string, timeoutMs?: number): Promise<ObserveResult[]>;

  /**
   * Execute an action using AI or direct selector
   *
   * Can accept either:
   * - Natural language instruction (AI inference)
   * - ActionObject with explicit selector (direct, faster)
   *
   * @param instructionOrAction - Instruction string or ActionObject
   * @returns Action result
   *
   * @example
   * // Natural language mode (AI inference)
   * await browser.act('click the submit button');
   *
   * // Selector mode (direct, faster)
   * await browser.act({
   *   selector: '#submit-btn',
   *   method: 'click',
   *   description: 'Submit button'
   * });
   */
  act(instructionOrAction: string | ActionObject): Promise<unknown>;

  /**
   * Execute an action using a predefined selector
   *
   * Convenience method for selector-based actions.
   * Clearer semantics than act() with ActionObject.
   *
   * @param action - ActionObject with selector and method
   * @returns Action result
   */
  actWithSelector(action: ActionObject): Promise<unknown>;

  // ============================================
  // Element Inspection
  // ============================================

  /**
   * Extract attributes from an element by XPath
   *
   * Retrieves comprehensive element metadata for
   * selector generation and validation.
   *
   * @param xpath - XPath selector to the element
   * @returns Element attributes or null if not found
   */
  getElementAttributesFromXPath(xpath: string): Promise<ElementAttributes | null>;

  /**
   * Get the underlying Page instance
   *
   * Returns the browser-specific Page object:
   * - StagehandBrowser: Stagehand Page (limited Playwright-like API)
   * - AgentCoreBrowser: Playwright Page (full API)
   *
   * Use with caution - prefer high-level methods when possible.
   *
   * @returns Page instance (type varies by implementation)
   */
  getPage(): Promise<unknown>;

  // ============================================
  // Automation Helpers
  // ============================================

  /**
   * Auto-detect and close popups/overlays
   *
   * Uses AI to find common popup patterns and close them:
   * - Cookie consent banners
   * - Newsletter signup modals
   * - Notification permission dialogs
   *
   * @returns Number of popups closed
   */
  autoClosePopups(): Promise<number>;

  // ============================================
  // Metrics (Optional)
  // ============================================

  /**
   * Get accumulated token usage statistics
   *
   * Returns the total tokens consumed by AI operations
   * (observe, act) during this browser session.
   *
   * @returns Token statistics or undefined if not tracked
   */
  getTokenStats?(): TokenStats | undefined;
}