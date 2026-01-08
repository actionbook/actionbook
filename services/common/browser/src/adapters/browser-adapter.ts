/**
 * BrowserAdapter - Base interface for browser automation
 *
 * This interface defines the minimal set of operations needed for
 * basic browser automation tasks like navigation, screenshots, and
 * content extraction.
 *
 * Used by: playbook-builder (basic web crawling)
 */

import type {
  NavigateOptions,
  ScreenshotOptions,
  WaitForSelectorOptions,
  ScrollDirection,
} from '../types/index.js';

/**
 * Base browser adapter interface
 *
 * Implementations:
 * - StagehandBrowser: Local Playwright + Stagehand (also implements AIBrowserAdapter)
 * - AgentCoreBrowser: AWS Agent Core Browser Tool (cloud-based)
 * - PlaywrightBrowser: Pure Playwright (lightweight, no AI)
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

  /**
   * Navigate forward in browser history
   */
  goForward(): Promise<void>;

  /**
   * Reload the current page
   */
  reload(): Promise<void>;

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
   * Wait for navigation to complete
   * @param timeout - Timeout in milliseconds
   */
  waitForNavigation(timeout?: number): Promise<void>;

  /**
   * Wait for a specified duration
   * @param ms - Duration in milliseconds
   */
  wait(ms: number): Promise<void>;

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
}
