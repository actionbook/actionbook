/**
 * AI Browser Types - Types for AI-powered browser interactions
 */

/**
 * Result from observe() - AI-detected element
 */
export interface ObserveResult {
  /** Element selector (usually XPath) */
  selector: string;
  /** Human-readable description of the element */
  description: string;
  /** Suggested interaction method */
  method?: string;
  /** Suggested arguments for the method */
  arguments?: string[];
}

/**
 * Action object for direct element interaction
 */
export interface ActionObject {
  /** Element selector (CSS or XPath) */
  selector: string;
  /** Human-readable description */
  description?: string;
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
