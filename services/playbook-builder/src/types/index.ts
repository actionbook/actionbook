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
  /** Maximum number of pages to explore */
  maxPages?: number;
  /** Source version ID (optional, creates new version if not provided) */
  sourceVersionId?: number;
  /** LLM provider for page exploration (auto-detected if not specified) */
  llmProvider?: 'openrouter' | 'openai' | 'anthropic' | 'bedrock';
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
 * Focuses on CAPABILITIES and SCENARIOS, not element details
 * Element discovery is action-builder's responsibility
 */
export interface PageCapabilities {
  /**
   * Comprehensive description of the page's purpose and main functionality
   */
  description: string;

  /**
   * High-level capabilities as action phrases (e.g., "Search for products", "Add item to cart")
   */
  capabilities: string[];

  /**
   * Key functional areas on this page (e.g., "Search form", "Navigation menu", "Product filters")
   */
  functionalAreas?: string[];

  /**
   * Common user scenarios/workflows that can be performed on this page
   */
  scenarios?: UserScenario[];

  /**
   * Prerequisites or conditions for using this page
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
