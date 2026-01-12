/**
 * Types for Playbook Builder
 */

/**
 * Configuration for PlaybookBuilder
 */
export interface PlaybookBuilderConfig {
  /** Source ID to build playbook for */
  sourceId: number;
  /** Starting URL for exploration */
  startUrl: string;
  /** Whether to run browser in headless mode */
  headless?: boolean;
  /** Maximum number of pages to explore (default: 10) */
  maxPages?: number;
  /** Maximum depth for recursive page discovery (default: 1)
   *  - 0: Only process startUrl
   *  - 1: Discover pages from startUrl only (single-level)
   *  - 2+: Recursively discover pages from discovered pages
   */
  maxDepth?: number;
  /** Source version ID (optional, creates new version if not provided) */
  sourceVersionId?: number;
  /** LLM provider for page exploration (auto-detected if not specified) */
  llmProvider?: 'openrouter' | 'openai' | 'anthropic' | 'bedrock';
  /**
   * Custom prompt for site-specific optimization
   * Appended to user prompts in page discovery, analysis, and capabilities discovery
   * Example: "Focus on the main navigation menu and ignore promotional banners"
   */
  customPrompt?: string;
}

/**
 * Discovered page info from LLM exploration
 */
export interface DiscoveredPage {
  /** URL of the page */
  url: string;
  /** Semantic ID (e.g., 'home', 'search', 'listing_detail') */
  semanticId: string;
  /** Human-readable name */
  name: string;
  /** Page description */
  description: string;
  /** How to navigate to this page */
  navigation?: string;
  /** Discovery depth (0 = startUrl, 1 = discovered from startUrl, etc.) */
  depth?: number;
}

/**
 * Analyzed page with capabilities
 */
export interface AnalyzedPage extends DiscoveredPage {
  /** List of capabilities/features on this page */
  capabilities: string[];
  /** Prerequisites for accessing this page */
  prerequisites?: string[];
  /** URL pattern for matching this page type */
  urlPattern?: string;
  /** Selector to wait for before page is ready */
  waitFor?: string;
}

/**
 * User scenario/flow on the page
 * Describes WHAT users do, not HOW (element details are action-builder's job)
 */
export interface UserScenario {
  /** Scenario name (e.g., "Search for accommodation", "User login") */
  name: string;
  /** What this scenario accomplishes */
  goal: string;
  /** High-level steps described in natural language */
  steps: string[];
  /** Expected outcome after completing the scenario */
  outcome: string;
}

/**
 * Page capabilities - describes what actions can be performed on a page
 * Now includes full 7-section Playbook markdown for comprehensive documentation
 */
export interface PageCapabilities {
  /**
   * Comprehensive description of the page's purpose and main functionality
   * (Extracted from Playbook Section 1: Page Overview)
   */
  description: string;

  /**
   * High-level capabilities as action phrases (e.g., "Search for products", "Add item to cart")
   * (Extracted from Playbook Section 2: Page Function Summary)
   */
  capabilities: string[];

  /**
   * Full 7-section Playbook markdown document
   * Contains:
   * - Section 0: Page URL (parameters, dynamic params)
   * - Section 1: Page Overview (core business objective)
   * - Section 2: Page Function Summary (function list)
   * - Section 3: Page Structure Summary (layout modules + CSS selectors)
   * - Section 4: DOM Structure Instance (pattern recognition, HTML snippets)
   * - Section 5: Parsing & Processing Summary (data retrieval scenarios)
   * - Section 6: Operation Summary (interactive operations)
   */
  playbook?: string;

  /**
   * Key functional areas on this page (e.g., "Search form", "Navigation menu", "Product filters")
   * @deprecated Use playbook Section 3 instead
   */
  functionalAreas?: string[];

  /**
   * Common user scenarios/workflows that can be performed on this page
   * @deprecated Use playbook Section 6 instead
   */
  scenarios?: UserScenario[];

  /**
   * Prerequisites or conditions for using this page
   * @deprecated Use playbook Section 1 instead
   */
  prerequisites?: string[];
}


/**
 * Result of playbook building
 */
export interface PlaybookBuildResult {
  /** Number of playbooks (pages) created */
  playbookCount: number;
  /** Source version ID */
  sourceVersionId: number;
  /** List of created playbook IDs (document IDs) */
  playbookIds: number[];
}
