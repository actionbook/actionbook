/**
 * @actionbookdev/browser
 *
 * Browser adapters for Actionbook - supports multiple browser backends:
 * - StagehandBrowser: Local Playwright + Stagehand AI (full AI capabilities)
 * - AgentCoreBrowser: AWS Agent Core Browser Tool (cloud-based)
 *
 * @example
 * // Using factory function with auto-detection
 * import { createBrowser } from '@actionbookdev/browser';
 *
 * const browser = createBrowser('auto');
 * await browser.initialize();
 * await browser.navigate('https://example.com');
 * const screenshot = await browser.screenshot();
 * await browser.close();
 *
 * @example
 * // Using specific implementation
 * import { StagehandBrowser } from '@actionbookdev/browser';
 *
 * const browser = new StagehandBrowser({ headless: true });
 * await browser.initialize();
 * const elements = await browser.observe('find the login button');
 * await browser.close();
 */

// ============================================
// Types
// ============================================

export type {
  // Browser types
  BrowserConfig,
  ScreenshotOptions,
  NavigateOptions,
  WaitForSelectorOptions,
  ScrollDirection,
  BrowserType,
  // AI browser types
  ObserveResult,
  ActionObject,
  ActionMethod,
  ElementAttributes,
  TokenStats,
  AIBrowserConfig,
} from './types/index.js';

// ============================================
// Adapters (Interfaces)
// ============================================

export type { BrowserAdapter } from './adapters/index.js';

// ============================================
// Implementations
// ============================================

export {
  StagehandBrowser,
  ElementNotFoundError,
  ActionExecutionError,
} from './implementations/index.js';

export {
  AgentCoreBrowser,
  type AgentCoreBrowserConfig,
} from './implementations/index.js';

// ============================================
// Utilities
// ============================================

export {
  log,
  setLogger,
  resetLogger,
  type LogLevel,
  type LogFunction,
} from './utils/index.js';

export {
  filterStateDataAttributes,
  createIdSelector,
  generateOptimizedXPath,
  filterCssClasses,
} from './utils/index.js';

// ============================================
// Factory Functions
// ============================================

import type { BrowserAdapter } from './adapters/index.js';
import type { BrowserConfig, BrowserType } from './types/index.js';
import { StagehandBrowser } from './implementations/index.js';
import { AgentCoreBrowser } from './implementations/index.js';

/**
 * Create a browser instance of the specified type
 *
 * @param type - Browser type: 'stagehand', 'agentcore', or 'auto'
 * @param config - Browser configuration
 * @returns BrowserAdapter instance
 *
 * @example
 * const browser = createBrowser('stagehand', { headless: true });
 * await browser.initialize();
 */
export function createBrowser(
  type: BrowserType | 'auto',
  config?: BrowserConfig
): BrowserAdapter {
  switch (type) {
    case 'stagehand':
      return new StagehandBrowser(config);

    case 'agentcore':
      return new AgentCoreBrowser(config);

    case 'playwright':
      // For now, fall back to StagehandBrowser without AI features
      // Could implement a pure Playwright adapter in the future
      return new StagehandBrowser(config);

    case 'auto':
      return createBrowserAuto(config);

    default:
      throw new Error(`Unknown browser type: ${type}`);
  }
}

/**
 * Auto-detect the best browser implementation based on environment
 *
 * Detection logic:
 * 1. If AWS_AGENTCORE_RUNTIME is set, use AgentCoreBrowser
 * 2. Otherwise, use StagehandBrowser (local)
 *
 * @param config - Browser configuration
 * @returns BrowserAdapter instance
 */
export function createBrowserAuto(config?: BrowserConfig): BrowserAdapter {
  // Check if running in AgentCore Runtime environment
  if (process.env.AWS_AGENTCORE_RUNTIME === 'true') {
    return new AgentCoreBrowser(config);
  }

  // Check if AgentCore Browser is explicitly requested
  if (process.env.BROWSER_TYPE === 'agentcore') {
    return new AgentCoreBrowser(config);
  }

  // Default to StagehandBrowser (local Playwright)
  return new StagehandBrowser(config);
}

/**
 * Create a StagehandBrowser instance (convenience function)
 */
export function createStagehandBrowser(config?: BrowserConfig): StagehandBrowser {
  return new StagehandBrowser(config);
}

/**
 * Create an AgentCoreBrowser instance (convenience function)
 */
export function createAgentCoreBrowser(config?: BrowserConfig): AgentCoreBrowser {
  return new AgentCoreBrowser(config);
}
