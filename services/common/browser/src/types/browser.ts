/**
 * Browser Types - Basic configuration and options
 */

/**
 * Browser configuration options
 */
export interface BrowserConfig {
  /** Whether to run browser in headless mode */
  headless?: boolean;
  /** Proxy server URL (e.g., http://proxy:8080) */
  proxy?: string;
  /** Directory for browser profile/user data */
  profileDir?: string;
  /** Default navigation timeout in milliseconds */
  timeout?: number;
  /** Storage state file path for cookies/localStorage */
  storageStatePath?: string;
  /** Browser profile configuration */
  profile?: {
    enabled: boolean;
    profileDir?: string;
  };
}

/**
 * Screenshot options
 */
export interface ScreenshotOptions {
  /** Capture full scrollable page */
  fullPage?: boolean;
  /** Image format */
  format?: 'png' | 'jpeg' | 'webp';
  /** JPEG/WebP quality (0-100) */
  quality?: number;
}

/**
 * Navigation options
 */
export interface NavigateOptions {
  /** Navigation timeout in milliseconds */
  timeout?: number;
  /** When to consider navigation complete */
  waitUntil?: 'load' | 'domcontentloaded' | 'networkidle';
}

/**
 * Wait for selector options
 */
export interface WaitForSelectorOptions {
  /** Timeout in milliseconds */
  timeout?: number;
  /** Wait for element to be visible */
  visible?: boolean;
  /** Wait for element to be hidden */
  hidden?: boolean;
}

/**
 * Scroll direction
 */
export type ScrollDirection = 'up' | 'down';

/**
 * Browser type identifier
 */
export type BrowserType = 'stagehand' | 'agentcore' | 'playwright';
