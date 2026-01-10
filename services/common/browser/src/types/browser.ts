/**
 * Browser Types - Configuration, options, and AI-related types
 */

// ============================================
// Browser Configuration
// ============================================

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
 * AI Browser configuration extending base config
 */
export interface AIBrowserConfig {
  /** LLM provider for AI operations */
  llmProvider?: 'openrouter' | 'openai' | 'anthropic' | 'bedrock';
  /** Model name/identifier */
  modelName?: string;
  /** Verbose logging level (0-2) */
  verbose?: number;
}

// ============================================
// Navigation & Screenshot Options
// ============================================

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

// ============================================
// AI Capabilities
// ============================================

/**
 * Result from observe() - AI-detected element
 * Aligned with Stagehand's ObserveResult type
 */
export interface ObserveResult {
  /** Element selector (usually XPath) */
  selector?: string;
  /** Human-readable description of the element */
  description?: string;
  /** Suggested interaction method */
  method?: string;
  /** Suggested arguments for the method */
  arguments?: unknown[];
}

/**
 * Action object for direct element interaction
 * Aligned with main branch - description is required
 */
export interface ActionObject {
  /** Element selector (CSS or XPath) */
  selector: string;
  /** Human-readable description of the action */
  description: string;
  /** Interaction method */
  method: ActionMethod;
  /** Arguments for the method (e.g., text to type) */
  arguments?: string[];
}

/**
 * Supported action methods
 */
export type ActionMethod =
  | 'click'
  | 'type'
  | 'fill'
  | 'select'
  | 'hover'
  | 'press'
  | 'scroll'
  | 'wait';

// ============================================
// Element Attributes
// ============================================

/**
 * Element attributes extracted from the page
 */
export interface ElementAttributes {
  /** HTML tag name (lowercase) */
  tagName: string;
  /** Element id attribute */
  id?: string;
  /** Element class attribute */
  className?: string;
  /** data-testid attribute */
  dataTestId?: string;
  /** aria-label attribute */
  ariaLabel?: string;
  /** placeholder attribute (for inputs) */
  placeholder?: string;
  /** name attribute (for form elements) */
  name?: string;
  /** Text content (truncated) */
  textContent?: string;
  /** Generated CSS selector */
  cssSelector?: string;
  /** Optimized XPath selector */
  optimizedXPath?: string;
  /** All data-* attributes */
  dataAttributes?: Record<string, string>;
}

// ============================================
// Metrics
// ============================================

/**
 * Token usage statistics for AI operations
 */
export interface TokenStats {
  /** Input tokens consumed */
  input: number;
  /** Output tokens generated */
  output: number;
  /** Total tokens (input + output) */
  total: number;
}
