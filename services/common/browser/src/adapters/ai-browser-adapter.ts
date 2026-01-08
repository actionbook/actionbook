/**
 * AIBrowserAdapter - Extended interface for AI-powered browser automation
 *
 * This interface extends BrowserAdapter with AI capabilities like
 * element observation and intelligent action execution.
 *
 * Used by: action-builder (AI-driven element discovery and interaction)
 */

import type { BrowserAdapter } from './browser-adapter.js';
import type {
  ObserveResult,
  ActionObject,
  ElementAttributes,
  TokenStats,
} from '../types/index.js';

/**
 * AI-powered browser adapter interface
 *
 * Extends BrowserAdapter with:
 * - observe(): AI-powered element discovery
 * - act(): AI-powered action execution
 * - getElementAttributes(): Extract element metadata
 *
 * Implementations:
 * - StagehandBrowser: Local Playwright + Stagehand AI
 */
export interface AIBrowserAdapter extends BrowserAdapter {
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
  getElementAttributes(xpath: string): Promise<ElementAttributes | null>;

  /**
   * Alias for getElementAttributes (backward compatibility)
   * @deprecated Use getElementAttributes instead
   */
  getElementAttributesFromXPath(xpath: string): Promise<ElementAttributes | null>;

  /**
   * Get the underlying Playwright Page instance
   *
   * Provides direct access to Playwright Page for advanced operations.
   * Use with caution - prefer high-level methods when possible.
   *
   * @returns Playwright Page instance
   */
  getPage(): Promise<unknown>;

  /**
   * Wait for text to appear on the page
   *
   * @param text - Text to wait for
   * @param timeout - Timeout in milliseconds (default: 30000)
   */
  waitForText(text: string, timeout?: number): Promise<void>;

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
  getTokenStats?(): TokenStats;
}

/**
 * Type guard to check if a BrowserAdapter is an AIBrowserAdapter
 */
export function isAIBrowserAdapter(
  adapter: BrowserAdapter
): adapter is AIBrowserAdapter {
  return (
    'observe' in adapter &&
    'act' in adapter &&
    typeof (adapter as AIBrowserAdapter).observe === 'function' &&
    typeof (adapter as AIBrowserAdapter).act === 'function'
  );
}
