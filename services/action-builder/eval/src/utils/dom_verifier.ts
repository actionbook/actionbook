/**
 * DOM Verifier
 *
 * Uses Playwright to verify that recorded selectors match reference selectors.
 * Supports pre-actions for elements that require interaction to become visible.
 *
 * Matching rules:
 * - Relaxed matching (A ⊆ B): If recorded selector finds N elements including
 *   the reference element, it's considered a match (selector works, just not unique)
 * - Pre-actions use golden element ref_selectors, NOT recorded capability selectors
 */

import { chromium, type Browser, type Page, type Locator } from "playwright";
import type {
  PreActionStep,
  GoldenElement,
  SiteCapability,
  TestEnv,
} from "../types.js";

// Default test environment
const DEFAULT_ENV: TestEnv = {
  id: "desktop_en",
  viewport: { width: 1920, height: 1080 },
  locale: "en-US",
};

// Default timeouts
const DEFAULT_TIMEOUT = 5000;
const ACTION_DELAY = 300;

export interface DOMMatchResult {
  /** Whether the selectors match the same element */
  matched: boolean;
  /** Whether the recorded selector found an element */
  recordedFound: boolean;
  /** Whether the reference selector found an element */
  refFound: boolean;
  /** Number of elements found by recorded selector */
  recordedCount: number;
  /** Number of elements found by reference selector */
  refCount: number;
  /** Whether the element is unique (count === 1) */
  isUnique: boolean;
  /** Whether the element is visible */
  isVisible: boolean;
  /** Error message if any */
  error?: string;
}

export interface DOMVerifierOptions {
  /** Test environment configuration */
  env?: TestEnv;
  /** Timeout for element operations (ms) */
  timeout?: number;
  /** Whether to run in headless mode */
  headless?: boolean;
  /** Enable verbose logging */
  verbose?: boolean;
}

/**
 * DOM Verifier class for validating selectors using Playwright
 */
export class DOMVerifier {
  private browser: Browser | null = null;
  private page: Page | null = null;
  private options: Required<DOMVerifierOptions>;

  constructor(options: DOMVerifierOptions = {}) {
    this.options = {
      env: options.env || DEFAULT_ENV,
      timeout: options.timeout || DEFAULT_TIMEOUT,
      headless: options.headless ?? true,
      verbose: options.verbose ?? false,
    };
  }

  /**
   * Initialize browser and page
   */
  async init(): Promise<void> {
    if (this.browser) return;

    this.log("Initializing browser...");
    this.browser = await chromium.launch({
      headless: this.options.headless,
    });

    const context = await this.browser.newContext({
      viewport: this.options.env.viewport,
      locale: this.options.env.locale,
    });

    this.page = await context.newPage();
    this.log("Browser initialized");
  }

  /**
   * Close browser
   */
  async close(): Promise<void> {
    if (this.browser) {
      await this.browser.close();
      this.browser = null;
      this.page = null;
      this.log("Browser closed");
    }
  }

  /**
   * Navigate to URL
   */
  async navigate(url: string): Promise<void> {
    if (!this.page) throw new Error("Browser not initialized");

    this.log(`Navigating to: ${url}`);
    await this.page.goto(url, { waitUntil: "domcontentloaded" });
    await this.page.waitForTimeout(1000); // Wait for initial render
  }

  /**
   * Execute pre-actions sequence
   *
   * @param preActions - List of pre-action steps
   * @param goldenElements - Golden elements with ref_selectors (preferred for element resolution)
   * @param capability - Recorded capability (fallback for element resolution)
   */
  async executePreActions(
    preActions: PreActionStep[],
    goldenElements?: GoldenElement[],
    capability?: SiteCapability
  ): Promise<void> {
    if (!this.page) throw new Error("Browser not initialized");

    for (const step of preActions) {
      await this.executeAction(step, goldenElements, capability);
      await this.page.waitForTimeout(ACTION_DELAY);
    }
  }

  /**
   * Execute a single pre-action step
   * Uses golden ref_selector when available, falls back to capability
   */
  private async executeAction(
    step: PreActionStep,
    goldenElements?: GoldenElement[],
    capability?: SiteCapability
  ): Promise<void> {
    if (!this.page) throw new Error("Browser not initialized");

    const { action, element_id, value } = step;
    this.log(`Executing action: ${action}${element_id ? ` on ${element_id}` : ""}`);

    switch (action) {
      case "wait": {
        const ms = value ? parseInt(value, 10) : 500;
        await this.page.waitForTimeout(ms);
        break;
      }

      case "goto": {
        if (value) {
          await this.page.goto(value, { waitUntil: "domcontentloaded" });
        }
        break;
      }

      case "go_back": {
        await this.page.goBack();
        break;
      }

      case "go_forward": {
        await this.page.goForward();
        break;
      }

      case "press": {
        if (value) {
          await this.page.keyboard.press(value);
        }
        break;
      }

      case "scroll": {
        if (element_id) {
          const locator = await this.getLocatorForElementWithGolden(element_id, goldenElements, capability);
          if (locator) {
            await locator.scrollIntoViewIfNeeded({ timeout: this.options.timeout });
          }
        } else if (value === "up") {
          await this.page.mouse.wheel(0, -300);
        } else if (value === "down") {
          await this.page.mouse.wheel(0, 300);
        }
        break;
      }

      case "click": {
        if (element_id) {
          const locator = await this.getLocatorForElementWithGolden(element_id, goldenElements, capability);
          if (locator) {
            await locator.click({ timeout: this.options.timeout });
          }
        }
        break;
      }

      case "hover": {
        if (element_id) {
          const locator = await this.getLocatorForElementWithGolden(element_id, goldenElements, capability);
          if (locator) {
            await locator.hover({ timeout: this.options.timeout });
          }
        }
        break;
      }

      case "type": {
        if (element_id && value) {
          const locator = await this.getLocatorForElementWithGolden(element_id, goldenElements, capability);
          if (locator) {
            await locator.fill(value, { timeout: this.options.timeout });
          }
        }
        break;
      }

      default:
        this.log(`Unknown action: ${action}`);
    }
  }

  /**
   * Get locator for an element, preferring golden ref_selector over capability
   */
  private async getLocatorForElementWithGolden(
    elementId: string,
    goldenElements?: GoldenElement[],
    capability?: SiteCapability
  ): Promise<Locator | null> {
    if (!this.page) return null;

    // First, try golden ref_selector (preferred)
    if (goldenElements) {
      const golden = goldenElements.find((g) => g.id === elementId);
      if (golden?.ref_selector) {
        const locator = this.buildLocatorFromSelector(golden.ref_selector);
        const count = await locator.count();
        if (count > 0) {
          this.log(`Found element ${elementId} via golden ref_selector`);
          return locator;
        }
        // Try fallback selectors
        if (golden.ref_selector_fallbacks) {
          for (const fallback of golden.ref_selector_fallbacks) {
            const fbLocator = this.buildLocatorFromSelector(fallback);
            const fbCount = await fbLocator.count();
            if (fbCount > 0) {
              this.log(`Found element ${elementId} via golden fallback selector`);
              return fbLocator;
            }
          }
        }
      }
    }

    // Fallback to capability (for backwards compatibility)
    if (capability) {
      return this.getLocatorForElement(elementId, capability);
    }

    this.log(`Element not found: ${elementId}`);
    return null;
  }

  /**
   * Get Playwright locator for an element from capability
   */
  private async getLocatorForElement(
    elementId: string,
    capability: SiteCapability
  ): Promise<Locator | null> {
    if (!this.page) return null;

    // Search in all pages
    for (const page of Object.values(capability.pages)) {
      const element = page.elements[elementId];
      if (element && element.selectors.length > 0) {
        // Try selectors in priority order
        for (const sel of element.selectors) {
          const locator = this.buildLocator(sel.type, sel.value);
          const count = await locator.count();
          if (count > 0) {
            return locator;
          }
        }
      }
    }

    // Search in global elements
    const globalElement = capability.global_elements[elementId];
    if (globalElement && globalElement.selectors.length > 0) {
      for (const sel of globalElement.selectors) {
        const locator = this.buildLocator(sel.type, sel.value);
        const count = await locator.count();
        if (count > 0) {
          return locator;
        }
      }
    }

    this.log(`Element not found in capability: ${elementId}`);
    return null;
  }

  /**
   * Build Playwright locator from selector type and value
   */
  private buildLocator(type: string, value: string): Locator {
    if (!this.page) throw new Error("Browser not initialized");

    switch (type) {
      case "xpath":
        return this.page.locator(`xpath=${value}`);
      case "text":
        return this.page.locator(`text=${value}`);
      default:
        // css, id, data-testid, aria-label, placeholder all use CSS-like selectors
        return this.page.locator(value);
    }
  }

  /**
   * Verify if recorded selectors match the reference selector
   *
   * Uses relaxed matching (A ⊆ B):
   * - If recorded selector finds N elements and the reference element is one of them,
   *   it's considered a match (the selector works, though it may not be unique)
   *
   * @param recordedSelectors - Selectors from recorded capability
   * @param refSelector - Reference selector from golden element
   * @param preActions - Pre-actions to execute before verification
   * @param goldenElements - Golden elements for pre-action element resolution
   * @param capability - Capability for fallback element resolution (deprecated)
   */
  async verifyMatch(
    recordedSelectors: Array<{ type: string; value: string }>,
    refSelector: string,
    preActions?: PreActionStep[],
    goldenElements?: GoldenElement[],
    capability?: SiteCapability
  ): Promise<DOMMatchResult> {
    if (!this.page) throw new Error("Browser not initialized");

    try {
      // Execute pre-actions if provided (using golden ref_selectors)
      if (preActions && preActions.length > 0) {
        await this.executePreActions(preActions, goldenElements, capability);
      }

      // Find reference element
      const refLocator = this.buildLocatorFromSelector(refSelector);
      const refCount = await refLocator.count();
      const refFound = refCount > 0;

      if (!refFound) {
        return {
          matched: false,
          recordedFound: false,
          refFound: false,
          recordedCount: 0,
          refCount: 0,
          isUnique: false,
          isVisible: false,
          error: "Reference selector not found",
        };
      }

      // Get reference element handle
      const refHandle = await refLocator.first().elementHandle();

      // Try recorded selectors
      for (const sel of recordedSelectors) {
        const recordedLocator = this.buildLocator(sel.type, sel.value);
        const recordedCount = await recordedLocator.count();

        if (recordedCount > 0 && refHandle) {
          // Relaxed matching: Check if ref element is ANY of the recorded elements
          // (not just the first one)
          for (let i = 0; i < recordedCount; i++) {
            const recordedHandle = await recordedLocator.nth(i).elementHandle();

            if (recordedHandle) {
              const isSame = await this.page.evaluate(
                ([a, b]) => a === b,
                [refHandle, recordedHandle]
              );

              if (isSame) {
                const isVisible = await recordedLocator.nth(i).isVisible();
                return {
                  matched: true,
                  recordedFound: true,
                  refFound: true,
                  recordedCount,
                  refCount,
                  isUnique: recordedCount === 1,
                  isVisible,
                };
              }
            }
          }
        }
      }

      // No match found
      return {
        matched: false,
        recordedFound: false,
        refFound: true,
        recordedCount: 0,
        refCount,
        isUnique: false,
        isVisible: false,
        error: "Recorded selectors do not match reference element",
      };
    } catch (error) {
      return {
        matched: false,
        recordedFound: false,
        refFound: false,
        recordedCount: 0,
        refCount: 0,
        isUnique: false,
        isVisible: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Build locator from a selector string (auto-detect type)
   */
  private buildLocatorFromSelector(selector: string): Locator {
    if (!this.page) throw new Error("Browser not initialized");

    if (selector.startsWith("xpath=")) {
      return this.page.locator(selector);
    }
    if (selector.startsWith("text=")) {
      return this.page.locator(selector);
    }
    // Default to CSS
    return this.page.locator(selector);
  }

  /**
   * Verify a golden element against recorded capability
   *
   * @param golden - The golden element to verify
   * @param capability - Recorded capability
   * @param allGoldenElements - All golden elements (for pre-action resolution)
   */
  async verifyGoldenElement(
    golden: GoldenElement,
    capability: SiteCapability,
    allGoldenElements?: GoldenElement[]
  ): Promise<DOMMatchResult> {
    // Find recorded element
    const recordedElement = this.findRecordedElement(golden.id, capability);

    if (!recordedElement) {
      return {
        matched: false,
        recordedFound: false,
        refFound: false,
        recordedCount: 0,
        refCount: 0,
        isUnique: false,
        isVisible: false,
        error: `Element ${golden.id} not found in recorded capability`,
      };
    }

    return this.verifyMatch(
      recordedElement.selectors,
      golden.ref_selector,
      golden.pre_actions,
      allGoldenElements,
      capability
    );
  }

  /**
   * Find recorded element by ID in capability
   */
  private findRecordedElement(
    elementId: string,
    capability: SiteCapability
  ): { selectors: Array<{ type: string; value: string }> } | null {
    // Search in pages
    for (const page of Object.values(capability.pages)) {
      const element = page.elements[elementId];
      if (element) {
        return { selectors: element.selectors };
      }
    }

    // Search in global elements
    const globalElement = capability.global_elements[elementId];
    if (globalElement) {
      return { selectors: globalElement.selectors };
    }

    return null;
  }

  private log(message: string): void {
    if (this.options.verbose) {
      console.log(`[DOMVerifier] ${message}`);
    }
  }
}

/**
 * Convenience function to verify elements without managing browser lifecycle
 */
export async function verifyElements(
  url: string,
  goldenElements: GoldenElement[],
  capability: SiteCapability,
  options?: DOMVerifierOptions
): Promise<Map<string, DOMMatchResult>> {
  const verifier = new DOMVerifier(options);
  const results = new Map<string, DOMMatchResult>();

  try {
    await verifier.init();
    await verifier.navigate(url);

    for (const golden of goldenElements) {
      // Reset page state for elements with pre-actions
      if (golden.pre_actions && golden.pre_actions.length > 0) {
        await verifier.navigate(url);
      }

      // Pass all golden elements for pre-action resolution
      const result = await verifier.verifyGoldenElement(golden, capability, goldenElements);
      results.set(golden.id, result);
    }
  } finally {
    await verifier.close();
  }

  return results;
}
