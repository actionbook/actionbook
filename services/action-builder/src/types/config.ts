/**
 * Step event for real-time feedback during recording
 */
export interface StepEvent {
  /** Current step/turn number */
  stepNumber: number;
  /** Total turns executed so far */
  totalTurns: number;
  /** Maximum allowed turns */
  maxTurns: number;
  /** Tool that was called */
  toolName: string;
  /** Arguments passed to the tool */
  toolArgs: Record<string, unknown>;
  /** Result from the tool execution */
  toolResult?: unknown;
  /** Whether the step succeeded */
  success: boolean;
  /** Error message if failed */
  error?: string;
  /** Duration of this step in milliseconds */
  durationMs: number;
  /** Current page type context */
  pageType?: string;
  /** Timestamp when step completed */
  timestamp: Date;
}

/**
 * Callback function for step completion events
 */
export type OnStepFinishCallback = (event: StepEvent) => void | Promise<void>;

/**
 * LLM Provider type
 */
export type LLMProvider = 'openrouter' | 'openai' | 'anthropic';

/**
 * Termination reason for recording task
 */
export type TerminationReason =
  | 'completed'                 // Normal completion
  | 'max_turns_reached'         // Reached maximum turns
  | 'task_timeout'              // Task total timeout
  | 'max_tokens_reached'        // Token limit exceeded
  | 'element_threshold_reached' // Element threshold reached
  | 'low_observe_efficiency'    // Low observe efficiency
  | 'max_pages_visited';        // Maximum pages visited

/**
 * ActionBuilder configuration
 */
export interface ActionBuilderConfig {
  headless?: boolean;
  maxTurns?: number;
  outputDir?: string;
  /** Custom log file path. If not provided, auto-generated in ./logs/ */
  logFile?: string;
  /**
   * LLM API key. If not provided, auto-detected from environment variables.
   * Priority: OPENROUTER_API_KEY > OPENAI_API_KEY > ANTHROPIC_API_KEY
   */
  llmApiKey?: string;
  /**
   * LLM provider. If not provided, auto-detected from available API keys.
   * @deprecated Use llmApiKey with environment variables for auto-detection
   */
  llmProvider?: LLMProvider;
  /**
   * LLM model name. Provider-specific default used if not specified.
   * - OpenRouter: anthropic/claude-sonnet-4
   * - OpenAI: gpt-4o
   * - Anthropic: claude-sonnet-4-5
   */
  llmModel?: string;
  /**
   * @deprecated No longer needed with Vercel AI SDK
   */
  llmBaseURL?: string;
  /** PostgreSQL connection string. If provided, enables dual-write to both YAML and database */
  databaseUrl?: string;
  /** Optional callback for real-time step feedback during recording */
  onStepFinish?: OnStepFinishCallback;
  /** Enable browser profile for persistent login state */
  profileEnabled?: boolean;
  /** Profile directory path, default: '.browser-profile' */
  profileDir?: string;
  /** Build timeout in milliseconds (includes init, build, cleanup), default: 10 minutes */
  buildTimeoutMs?: number;
  /** Build-level retry config for browser connection errors (retries entire buildInternal flow including LLM) */
  browserRetryConfig?: {
    maxAttempts?: number;      // default: 3
    baseDelayMs?: number;       // default: 2000 (exponential backoff: delay = baseDelayMs * attempt)
    retryableErrors?: string[]; // default: ECONNREFUSED, Target closed, etc.
  };
}

/**
 * Browser profile configuration
 */
export interface ProfileConfig {
  /** Enable browser profile for persistent login state */
  enabled: boolean;
  /** Profile directory path, default: '.browser-profile' */
  profileDir?: string;
}

/**
 * Browser configuration
 */
export interface BrowserConfig {
  headless: boolean;
  llmApiKey: string;
  llmBaseURL?: string;
  llmModel?: string;
  /** Browser profile configuration for persistent login state */
  profile?: ProfileConfig;
  /** Path to storage state file (cookies/localStorage) for session injection */
  storageStatePath?: string;
}

/**
 * LLM client configuration
 */
export interface LLMConfig {
  apiKey: string;
  baseURL: string;
  model: string;
}

/**
 * Recorder configuration
 */
export interface RecorderConfig {
  maxTurns: number;
  outputDir: string;
  /** Optional callback for real-time step feedback */
  onStepFinish?: OnStepFinishCallback;

  // === Task-level controls ===
  /** Task total timeout in milliseconds, default: 15 * 60 * 1000 (15 minutes) */
  taskTimeoutMs?: number;
  /** Token limit, -1 = unlimited, default: -1 */
  maxTokens?: number;
  /** Maximum visited pages, default: 5 */
  maxVisitedPages?: number;

  // === Operation timeout config ===
  operationTimeouts?: {
    /** Navigate timeout in milliseconds, default: 60000 */
    navigate?: number;
    /** Observe_page timeout in milliseconds, default: 30000 */
    observe?: number;
  };

  // === Retry config ===
  retryConfig?: {
    /** Maximum retry attempts, default: 3 */
    maxAttempts?: number;
    /** Base delay in milliseconds, default: 1000 */
    baseDelayMs?: number;
  };

  // === Termination strategy config ===
  terminationConfig?: {
    /** Element count threshold, terminates when exceeded, default: 80 */
    elementThreshold?: number;
    /** Observe efficiency threshold (elements/call), default: 3 */
    minObserveEfficiency?: number;
    /** Minimum observe calls before efficiency check, default: 3 */
    minObserveCallsForCheck?: number;
  };

  // === Selector optimization ===
  /**
   * Enable LLM-based selector optimization before saving to database.
   * The optimizer analyzes all discovered selectors and:
   * - Filters out unstable selectors (e.g., dynamic counters, timestamps)
   * - Reorders selectors by reliability
   * - Adjusts confidence scores
   * Default: true
   */
  enableSelectorOptimization?: boolean;

  // === Playbook mode options ===
  /**
   * Target page URL pattern (regex). Only elements on pages matching this pattern will be recorded.
   * If not provided, elements on all pages will be recorded.
   * @example '^/search' - only record elements on /search/* pages
   */
  targetUrlPattern?: string;

  /**
   * Whether to auto-scroll to bottom before observe_page to load lazy elements.
   * Default: true
   */
  autoScrollToBottom?: boolean;
}

/**
 * Validator configuration
 */
export interface ValidatorConfig {
  timeout?: number;
  verbose?: boolean;
  /** Filter validation to specific page types */
  pageFilter?: string[];
  /** Template parameters for validating template selectors */
  templateParams?: Record<string, string>;
}

/**
 * Build options for ActionBuilder.build()
 */
export interface BuildOptions {
  siteName?: string;
  siteDescription?: string;
  /** Detailed scenario description (e.g., chunk_content from knowledge base) */
  scenarioDescription?: string;
  focusAreas?: string[];
  pageTypes?: string[];
  /** Custom system prompt (overrides default CAPABILITY_RECORDER_SYSTEM_PROMPT) */
  customSystemPrompt?: string;
  /** Custom user prompt (overrides auto-generated prompt) */
  customUserPrompt?: string;
  /** Existing task ID to use instead of creating a new one (for TaskWorker mode) */
  taskId?: number;

  // === Playbook mode options ===
  /**
   * Target page URL pattern (regex). Only elements on pages matching this pattern will be recorded.
   * If not provided, elements on all pages will be recorded.
   * @example '^/search' - only record elements on /search/* pages
   * @example '/products/\\d+' - only record elements on /products/123 style pages
   */
  targetUrlPattern?: string;

  /**
   * Enable auto-scroll to bottom before observing page elements.
   * This ensures lazy-loaded elements are loaded before recording.
   * @default true
   */
  autoScrollToBottom?: boolean;
}

/**
 * Build result from ActionBuilder.build()
 */
export interface BuildResult {
  success: boolean;
  message: string;
  turns: number;
  totalDuration: number;
  /** Token usage breakdown */
  tokens: {
    input: number;
    output: number;
    total: number;
    /** Planning LLM token usage (AIClient) */
    planning?: { input: number; output: number };
    /** Browser automation token usage (Stagehand) */
    browser?: { input: number; output: number };
  };
  siteCapability?: import("./capability.js").SiteCapability;
  savedPath?: string;
  /** Error message if database save failed (dual-write mode) */
  dbSaveError?: string;
  /** Indicates this is a partial result from timeout */
  partialResult?: boolean;
}

/**
 * Record result from ActionRecorder.record()
 */
export interface RecordResult {
  success: boolean;
  message: string;
  turns: number;
  /** Number of tool calls executed */
  steps: number;
  totalDuration: number;
  /** Token usage breakdown */
  tokens: {
    input: number;
    output: number;
    total: number;
    /** Planning LLM token usage (AIClient) */
    planning?: { input: number; output: number };
    /** Browser automation token usage (Stagehand) */
    browser?: { input: number; output: number };
  };
  /** Number of elements discovered */
  elementsDiscovered: number;
  siteCapability?: import("./capability.js").SiteCapability;
  savedPath?: string;
  /** Error message if database save failed (dual-write mode) */
  dbSaveError?: string;

  // === Termination-related fields ===
  /** Termination reason */
  terminationReason?: TerminationReason;
  /** Whether partially completed (early termination with results) */
  partialComplete?: boolean;
  /** Observe statistics */
  observeStats?: {
    totalCalls: number;
    totalElements: number;
    avgEfficiency: number;
  };
  /** Number of visited pages */
  visitedPagesCount?: number;
}

/**
 * Single selector validation detail
 */
export interface SelectorValidationDetail {
  type: string;
  value: string;
  valid: boolean;
  error?: string;
  isTemplate?: boolean;
  /** Whether the element is visible on the page */
  visible?: boolean;
  /** Whether the element is interactable (enabled and not obstructed) */
  interactable?: boolean;
}

/**
 * Element validation result
 */
export interface ElementValidationResult {
  elementId: string;
  pageType: string;
  valid: boolean;
  selector: {
    css?: { valid: boolean; error?: string };
    xpath?: { valid: boolean; error?: string };
  };
  /** Detailed validation results for each selector in the selectors array */
  selectorsDetail?: SelectorValidationDetail[];
}

/**
 * Validation result from ActionBuilder.validate()
 */
export interface ValidationResult {
  success: boolean;
  domain: string;
  totalElements: number;
  validElements: number;
  invalidElements: number;
  validationRate: number;
  details: ElementValidationResult[];
}
