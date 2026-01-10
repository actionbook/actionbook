import fs from "fs";
import { Stagehand, AISdkClient, type LLMClient } from "@browserbasehq/stagehand";
import type { Page, BrowserContext } from "playwright";
import { createAmazonBedrock } from "@ai-sdk/amazon-bedrock";
import { ProxyAgent, fetch as undiciFetch } from "undici";
import { log } from "../utils/logger.js";
import { createIdSelector } from "../utils/string.js";
import type { BrowserConfig, ObserveResultItem, ActionObject } from "../types/index.js";
import type { BrowserAdapter } from "./BrowserAdapter.js";
import { BrowserProfileManager, DEFAULT_PROFILE_DIR, type ProfileLogger } from "./BrowserProfileManager.js";

/**
 * State-related data attributes that should be filtered out.
 * These attributes change based on user interaction and are not stable selectors.
 */
const STATE_DATA_ATTRS = new Set([
  'data-state',
  'data-checked',
  'data-disabled',
  'data-active',
  'data-selected',
  'data-expanded',
  'data-open',
  'data-closed',
  'data-focus',
  'data-focus-visible',
  'data-hover',
  'data-pressed',
  'data-visible',
  'data-hidden',
  'data-loading',
  'data-readonly',
  'data-invalid',
  'data-valid',
  'data-highlighted',
  'data-orientation',
]);

/**
 * State-related values that indicate the attribute is a state attribute.
 */
const STATE_VALUES = new Set([
  'open', 'closed',
  'on', 'off',
  'true', 'false',
  'active', 'inactive',
  'enabled', 'disabled',
  'visible', 'hidden',
  'expanded', 'collapsed',
  'checked', 'unchecked',
  'selected', 'unselected',
  'pressed', 'unpressed',
  'valid', 'invalid',
  'loading', 'loaded',
  'horizontal', 'vertical',
]);

/**
 * Filter out state-related data attributes that are not stable for selectors.
 */
function filterStateDataAttributes(
  dataAttributes: Record<string, string> | undefined
): Record<string, string> | undefined {
  if (!dataAttributes) return undefined;

  const filtered: Record<string, string> = {};
  for (const [name, value] of Object.entries(dataAttributes)) {
    // Skip if attribute name is a known state attribute
    if (STATE_DATA_ATTRS.has(name)) {
      log("debug", `[StagehandBrowser] Filtering state attr: ${name}="${value}"`);
      continue;
    }
    // Skip if value is a known state value
    if (STATE_VALUES.has(value.toLowerCase())) {
      log("debug", `[StagehandBrowser] Filtering state value: ${name}="${value}"`);
      continue;
    }
    filtered[name] = value;
  }

  return Object.keys(filtered).length > 0 ? filtered : undefined;
}

/**
 * Generate an optimized XPath selector based on element attributes.
 * Priority (highest to lowest stability):
 * 1. @id / @data-testid - Most stable, unique identifiers
 * 2. @name / @aria-label - Stable, semantic attributes
 * 3. @class + tag - Moderately stable
 * 4. text() content - Can change with i18n
 * 5. Fallback to original absolute path - Least stable
 *
 * @param attrs - Element attributes extracted from the page
 * @param originalXPath - The original absolute XPath as fallback
 * @returns Optimized XPath string
 */
function generateOptimizedXPath(
  attrs: {
    tagName: string;
    id?: string;
    dataTestId?: string;
    name?: string;
    ariaLabel?: string;
    className?: string;
    textContent?: string;
    placeholder?: string;
    dataAttributes?: Record<string, string>;
  },
  originalXPath: string
): { xpath: string; source: string } {
  const tag = attrs.tagName;

  // Priority 1: @id (most stable, unique)
  if (attrs.id) {
    return {
      xpath: `//${tag}[@id="${attrs.id}"]`,
      source: "id",
    };
  }

  // Priority 1: @data-testid (most stable, designed for testing)
  if (attrs.dataTestId) {
    return {
      xpath: `//${tag}[@data-testid="${attrs.dataTestId}"]`,
      source: "data-testid",
    };
  }

  // Priority 1: Other stable data-* attributes (data-id, data-component, etc.)
  if (attrs.dataAttributes) {
    const stableDataAttrs = ['data-id', 'data-component', 'data-element', 'data-action', 'data-section', 'data-name'];
    for (const attr of stableDataAttrs) {
      if (attrs.dataAttributes[attr]) {
        return {
          xpath: `//${tag}[@${attr}="${attrs.dataAttributes[attr]}"]`,
          source: attr,
        };
      }
    }
  }

  // Priority 2: @name (stable for form elements - use tag for specificity)
  if (attrs.name) {
    return {
      xpath: `//${tag}[@name="${attrs.name}"]`,
      source: "name",
    };
  }

  // Priority 2: @aria-label (stable, semantic - use tag for specificity)
  if (attrs.ariaLabel) {
    return {
      xpath: `//${tag}[@aria-label="${attrs.ariaLabel}"]`,
      source: "aria-label",
    };
  }

  // Priority 2: @placeholder (for inputs - use tag for specificity)
  if (attrs.placeholder) {
    return {
      xpath: `//${tag}[@placeholder="${attrs.placeholder}"]`,
      source: "placeholder",
    };
  }

  // Priority 3: @class (moderately stable, filter out hash-like classes)
  if (attrs.className) {
    const classes = attrs.className.split(" ").filter((c: string) => {
      if (!c) return false;
      // Keep BEM-style classes
      if (/^[a-z][a-z0-9]*(-[a-z0-9]+)*(__[a-z0-9]+(-[a-z0-9]+)*)?(--[a-z0-9]+(-[a-z0-9]+)*)?$/i.test(c)) {
        return true;
      }
      // Filter out hash-like classes
      if (/^[a-z]{1,3}-[a-zA-Z0-9]{4,}$/.test(c)) return false;
      if (/^[a-zA-Z]{2,}[A-Z][a-z]+$/.test(c)) return false;
      if (/[A-Z].*[A-Z]/.test(c) && c.length < 12) return false;
      return true;
    });

    if (classes.length > 0) {
      // Prefer BEM-style classes
      const bemClass = classes.find((c: string) => c.includes('__') || c.includes('--'));
      const selectedClass = bemClass || classes[0];
      return {
        xpath: `//${tag}[contains(@class, "${selectedClass}")]`,
        source: `class(${selectedClass})`,
      };
    }
  }

  // Priority 4: text() content (can change with i18n, use contains for partial match)
  if (attrs.textContent && attrs.textContent.length > 2 && attrs.textContent.length <= 30) {
    // Escape quotes in text content
    const escapedText = attrs.textContent.replace(/"/g, '\\"');
    // Use normalize-space to handle whitespace variations
    return {
      xpath: `//${tag}[normalize-space()="${escapedText}"]`,
      source: "text",
    };
  }

  // Priority 5: Fallback to original absolute path (least stable)
  return {
    xpath: originalXPath,
    source: "absolute-path",
  };
}

/**
 * Create a proxy-enabled fetch function for Bedrock requests
 */
function createProxyFetchForBedrock(): typeof globalThis.fetch | undefined {
  const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;
  if (!proxyUrl) {
    return undefined;
  }

  log("info", `[StagehandBrowser] Using proxy for Bedrock: ${proxyUrl}`);

  const proxyAgent = new ProxyAgent(proxyUrl);
  return async (url: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const response = await undiciFetch(url.toString(), {
      ...init,
      dispatcher: proxyAgent,
    } as any);
    return response as unknown as Response;
  };
}

/**
 * Error class for element not found scenarios
 */
export class ElementNotFoundError extends Error {
  constructor(message: string, public readonly selector?: string) {
    super(message);
    this.name = "ElementNotFoundError";
  }
}

/**
 * Error class for action execution failures
 */
export class ActionExecutionError extends Error {
  constructor(
    message: string,
    public readonly action?: string,
    public readonly originalError?: Error
  ) {
    super(message);
    this.name = "ActionExecutionError";
  }
}

/**
 * Stagehand browser wrapper for managing browser lifecycle and operations
 */
/** Stagehand metrics for tracking LLM usage */
interface StagehandMetricsSnapshot {
  observePromptTokens: number;
  observeCompletionTokens: number;
  observeReasoningTokens: number;
  observeCachedInputTokens: number;
  observeInferenceTimeMs: number;
  actPromptTokens: number;
  actCompletionTokens: number;
  actReasoningTokens: number;
  actCachedInputTokens: number;
  actInferenceTimeMs: number;
}

export class StagehandBrowser implements BrowserAdapter {
  private stagehand: Stagehand | null = null;
  private page: Page | null = null;
  private config: BrowserConfig;
  private lastMetrics: StagehandMetricsSnapshot | null = null;

  // Accumulated token usage for Stagehand operations
  private accumulatedInputTokens: number = 0;
  private accumulatedOutputTokens: number = 0;

  constructor(config: BrowserConfig) {
    this.config = config;
  }

  /**
   * Initialize metrics baseline after Stagehand init
   * This captures any tokens consumed during initialization so they don't get
   * incorrectly attributed to the first actual operation
   */
  private async initializeMetricsBaseline(): Promise<void> {
    if (!this.stagehand) return;

    try {
      const metrics = await this.stagehand.metrics;
      this.lastMetrics = {
        observePromptTokens: metrics.observePromptTokens,
        observeCompletionTokens: metrics.observeCompletionTokens,
        observeReasoningTokens: metrics.observeReasoningTokens,
        observeCachedInputTokens: metrics.observeCachedInputTokens,
        observeInferenceTimeMs: metrics.observeInferenceTimeMs,
        actPromptTokens: metrics.actPromptTokens,
        actCompletionTokens: metrics.actCompletionTokens,
        actReasoningTokens: metrics.actReasoningTokens,
        actCachedInputTokens: metrics.actCachedInputTokens,
        actInferenceTimeMs: metrics.actInferenceTimeMs,
      };
      log("debug", `[StagehandBrowser] Metrics baseline initialized`);
    } catch {
      // If metrics are not available, initialize to zero
      this.lastMetrics = {
        observePromptTokens: 0, observeCompletionTokens: 0, observeReasoningTokens: 0,
        observeCachedInputTokens: 0, observeInferenceTimeMs: 0,
        actPromptTokens: 0, actCompletionTokens: 0, actReasoningTokens: 0,
        actCachedInputTokens: 0, actInferenceTimeMs: 0,
      };
    }
  }

  /**
   * Log Stagehand LLM metrics after an operation
   */
  private async logStagehandMetrics(operation: 'observe' | 'act', startTime: number): Promise<void> {
    if (!this.stagehand) return;

    try {
      const metrics = await this.stagehand.metrics;
      const e2eLatencyMs = Date.now() - startTime;

      // Calculate delta from last metrics
      const prev = this.lastMetrics || {
        observePromptTokens: 0, observeCompletionTokens: 0, observeReasoningTokens: 0,
        observeCachedInputTokens: 0, observeInferenceTimeMs: 0,
        actPromptTokens: 0, actCompletionTokens: 0, actReasoningTokens: 0,
        actCachedInputTokens: 0, actInferenceTimeMs: 0,
      };

      let inputTokens: number, outputTokens: number, reasoningTokens: number, cachedTokens: number, inferenceTimeMs: number;

      if (operation === 'observe') {
        inputTokens = metrics.observePromptTokens - prev.observePromptTokens;
        outputTokens = metrics.observeCompletionTokens - prev.observeCompletionTokens;
        reasoningTokens = metrics.observeReasoningTokens - prev.observeReasoningTokens;
        cachedTokens = metrics.observeCachedInputTokens - prev.observeCachedInputTokens;
        inferenceTimeMs = metrics.observeInferenceTimeMs - prev.observeInferenceTimeMs;
      } else {
        inputTokens = metrics.actPromptTokens - prev.actPromptTokens;
        outputTokens = metrics.actCompletionTokens - prev.actCompletionTokens;
        reasoningTokens = metrics.actReasoningTokens - prev.actReasoningTokens;
        cachedTokens = metrics.actCachedInputTokens - prev.actCachedInputTokens;
        inferenceTimeMs = metrics.actInferenceTimeMs - prev.actInferenceTimeMs;
      }

      // Update last metrics snapshot
      this.lastMetrics = {
        observePromptTokens: metrics.observePromptTokens,
        observeCompletionTokens: metrics.observeCompletionTokens,
        observeReasoningTokens: metrics.observeReasoningTokens,
        observeCachedInputTokens: metrics.observeCachedInputTokens,
        observeInferenceTimeMs: metrics.observeInferenceTimeMs,
        actPromptTokens: metrics.actPromptTokens,
        actCompletionTokens: metrics.actCompletionTokens,
        actReasoningTokens: metrics.actReasoningTokens,
        actCachedInputTokens: metrics.actCachedInputTokens,
        actInferenceTimeMs: metrics.actInferenceTimeMs,
      };

      // Only log if there were actual tokens used
      if (inputTokens > 0 || outputTokens > 0) {
        // Accumulate tokens for aggregation
        this.accumulatedInputTokens += inputTokens;
        this.accumulatedOutputTokens += outputTokens;

        const totalTokens = inputTokens + outputTokens;
        const tps = inferenceTimeMs > 0 ? Math.round((outputTokens / (inferenceTimeMs / 1000)) * 10) / 10 : 0;

        // Build token stats
        const tokenParts = [`in=${inputTokens}`, `out=${outputTokens}`];
        if (cachedTokens > 0) tokenParts.push(`cache_read=${cachedTokens}`);
        if (reasoningTokens > 0) tokenParts.push(`reasoning=${reasoningTokens}`);
        tokenParts.push(`total=${totalTokens}`);

        log('info', `[LLM] ✓ | stagehand/${operation} | tokens: ${tokenParts.join(', ')} | perf: latency=${e2eLatencyMs}ms, inference=${inferenceTimeMs}ms, tps=${tps}`);
      }
    } catch {
      // Ignore metrics errors - don't break the flow
    }
  }

  /**
   * Initialize Stagehand and browser
   */
  async initialize(): Promise<void> {
    if (this.stagehand && this.page) {
      return;
    }

    log("info", "[StagehandBrowser] Initializing Stagehand...");

    // Check if proxy is configured
    const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;
    const hasProxy = !!proxyUrl;

    // Stagehand V3 API: supports multiple LLM providers
    // Priority: OpenRouter > OpenAI > Anthropic (same as AIClient)
    const openrouterKey = process.env.OPENROUTER_API_KEY;
    const openaiKey = process.env.OPENAI_API_KEY;
    const anthropicKey = process.env.ANTHROPIC_API_KEY;

    // Use unified STAGEHAND_MODEL env var, with provider-specific defaults
    const stagehandModel = process.env.STAGEHAND_MODEL;

    let modelConfig: string | { modelName: string; apiKey: string; baseURL?: string } | undefined;
    let llmClient: LLMClient | undefined;

    if (openrouterKey) {
      // Use OpenRouter with custom baseURL (OpenAI-compatible)
      const model = stagehandModel || "gpt-4o";
      modelConfig = {
        modelName: model,
        apiKey: openrouterKey,
        baseURL: "https://openrouter.ai/api/v1",
      };
      log("info", `[StagehandBrowser] Using OpenRouter with ${model}`);
    } else if (openaiKey) {
      // Use OpenAI directly (via custom baseURL if proxy is set)
      const model = stagehandModel || "gpt-4o";
      if (hasProxy) {
        // When proxy is configured, we need to use custom baseURL approach
        // Stagehand's internal OpenAI client may not respect HTTPS_PROXY
        // Use explicit config to ensure proxy compatibility
        modelConfig = {
          modelName: model,
          apiKey: openaiKey,
          baseURL: process.env.OPENAI_BASE_URL || "https://api.openai.com/v1",
        };
        log("info", `[StagehandBrowser] Using OpenAI model ${model} (with explicit baseURL for proxy)`);
      } else {
        modelConfig = model;
        log("info", `[StagehandBrowser] Using OpenAI model ${model}`);
      }
    } else {
      // Check for AWS Bedrock credentials
      const bedrockAccessKey = process.env.AWS_ACCESS_KEY_ID;
      const bedrockSecretKey = process.env.AWS_SECRET_ACCESS_KEY;
      const hasBedrock = bedrockAccessKey && bedrockSecretKey;

      if (hasBedrock) {
        // Use AWS Bedrock via AISdkClient (bypasses model name validation)
        // This approach uses Vercel AI SDK as middleware to bridge Stagehand and Bedrock
        const region = process.env.AWS_REGION || process.env.AWS_BEDROCK_REGION || "us-east-1";
        const bedrockModel = stagehandModel || process.env.AWS_BEDROCK_MODEL || "anthropic.claude-3-5-sonnet-20241022-v2:0";

        log("info", `[StagehandBrowser] Using AWS Bedrock via AISdkClient`);
        log("info", `[StagehandBrowser] Bedrock region: ${region}`);
        log("info", `[StagehandBrowser] Bedrock model: ${bedrockModel}`);

        // Create proxy-enabled fetch if proxy is configured
        const proxyFetch = createProxyFetchForBedrock();

        // Create Bedrock provider with AWS credentials and optional proxy
        const bedrock = createAmazonBedrock({
          region,
          accessKeyId: bedrockAccessKey,
          secretAccessKey: bedrockSecretKey,
          sessionToken: process.env.AWS_SESSION_TOKEN,
          fetch: proxyFetch,
        });

        // Create AISdkClient wrapping the Bedrock model
        // This bypasses Stagehand's model name whitelist validation
        llmClient = new AISdkClient({
          model: bedrock(bedrockModel),
        });

        log("info", `[StagehandBrowser] AISdkClient created for Bedrock`);
      } else if (anthropicKey) {
        // Anthropic SDK does NOT support HTTP proxy natively
        // When proxy is configured, we cannot use Anthropic directly with Stagehand
        if (hasProxy) {
          log("warn", `[StagehandBrowser] Anthropic SDK does not support HTTP proxy.`);
          log("warn", `[StagehandBrowser] Please set OPENROUTER_API_KEY, OPENAI_API_KEY, or AWS Bedrock credentials for proxy support.`);
          throw new Error(
            "Anthropic SDK does not support HTTP proxy. " +
            "Stagehand requires direct API access. " +
            "Please use OPENROUTER_API_KEY, OPENAI_API_KEY, or AWS Bedrock credentials instead when HTTPS_PROXY is set."
          );
        }
        // Use Anthropic directly (no proxy)
        const model = stagehandModel || "claude-sonnet-4-20250514";
        modelConfig = model;
        log("info", `[StagehandBrowser] Using Anthropic model ${model}`);
      } else {
        throw new Error("No LLM API key found. Set OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY, or AWS credentials for Bedrock");
      }
    }

    // Custom logger to forward Stagehand logs to our unified logger
    const stagehandLogger = (logLine: { message: string; level?: number; auxiliary?: Record<string, unknown> }) => {
      const level = logLine.level === 0 ? "error" : logLine.level === 2 ? "debug" : "info";

      // Format auxiliary data if present
      let auxStr = "";
      if (logLine.auxiliary && Object.keys(logLine.auxiliary).length > 0) {
        auxStr = "\n    " + Object.entries(logLine.auxiliary)
          .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
          .join("\n    ");
      }

      log(level as "info" | "warn" | "error" | "debug", `[Stagehand] ${logLine.message}${auxStr}`);
    };

    // Build localBrowserLaunchOptions
    const localBrowserLaunchOptions: Record<string, unknown> = {
      headless: this.config.headless,
    };

    // Add browser proxy support if system proxy is configured
    if (proxyUrl) {
      localBrowserLaunchOptions.proxy = {
        server: proxyUrl,
      };
      log("info", `[StagehandBrowser] Using browser proxy: ${proxyUrl}`);
    }

    // Add profile support if enabled
    if (this.config.profile?.enabled) {
      const profileDir = this.config.profile.profileDir || DEFAULT_PROFILE_DIR;
      // Create custom logger that uses action-builder's log function
      const profileLogger: ProfileLogger = (level: "info" | "warn" | "error" | "debug", message: string) => {
        log(level, `[BrowserProfileManager] ${message}`);
      };
      const profileManager = new BrowserProfileManager({ baseDir: profileDir, logger: profileLogger });
      const profilePath = profileManager.getProfilePath();

      // Ensure profile directory exists before use
      profileManager.ensureDir();

      // Clean up stale lock files from previous crashed sessions
      profileManager.cleanupStaleLocks();

      localBrowserLaunchOptions.userDataDir = profilePath;
      localBrowserLaunchOptions.preserveUserDataDir = true;

      // Anti-detection args
      localBrowserLaunchOptions.args = [
        "--disable-blink-features=AutomationControlled",
        "--no-first-run",
      ];
      localBrowserLaunchOptions.ignoreDefaultArgs = ["--enable-automation"];

      log("info", `[StagehandBrowser] Using browser profile: ${profilePath}`);
    }

    // Create Stagehand with either model config or llmClient
    const stagehandOptions: any = {
      env: "LOCAL",
      localBrowserLaunchOptions,
      verbose: 1,
      logger: stagehandLogger,
    };

    if (llmClient) {
      // Use custom llmClient (for Bedrock via AISdkClient)
      stagehandOptions.llmClient = llmClient;
    } else if (modelConfig) {
      // Use model config (for OpenRouter, OpenAI, Anthropic)
      stagehandOptions.model = modelConfig;
    }

    this.stagehand = new Stagehand(stagehandOptions);

    await this.stagehand.init();

    // Stagehand V3: access page via context.pages()[0]
    this.page = this.stagehand.context.pages()[0] as unknown as Page;

    // Inject storage state (cookies/localStorage) if configured
    if (this.config.storageStatePath) {
      try {
        if (fs.existsSync(this.config.storageStatePath)) {
          const stateData = JSON.parse(fs.readFileSync(this.config.storageStatePath, 'utf-8'));
          const context = this.stagehand.context as unknown as BrowserContext;

          // 1. Inject Cookies
          if (stateData.cookies && Array.isArray(stateData.cookies)) {
            await context.addCookies(stateData.cookies);
            log("info", `[StagehandBrowser] Injected ${stateData.cookies.length} cookies from ${this.config.storageStatePath}`);
          }

          // 2. Inject LocalStorage
          if (stateData.origins && Array.isArray(stateData.origins)) {
            await context.addInitScript((storageState) => {
              if (window.location.href === 'about:blank') return; // Skip for about:blank
              
              const originState = storageState.origins.find((o: any) => o.origin === window.location.origin);
              if (originState && originState.localStorage) {
                for (const { name, value } of originState.localStorage) {
                  window.localStorage.setItem(name, value);
                }
              }
            }, stateData);
            log("info", `[StagehandBrowser] Injected localStorage scripts for ${stateData.origins.length} origins`);
          }
        } else {
          log("warn", `[StagehandBrowser] Storage state file not found: ${this.config.storageStatePath}`);
        }
      } catch (error) {
        log("error", `[StagehandBrowser] Failed to inject storage state: ${error}`);
      }
    }

    // Initialize metrics baseline to avoid counting any tokens consumed during init
    // This ensures subsequent delta calculations are accurate for actual operations
    await this.initializeMetricsBaseline();

    log("info", "[StagehandBrowser] Initialized successfully.");
  }

  /**
   * Get the current page instance
   */
  async getPage(): Promise<Page> {
    if (!this.page) {
      throw new Error("Browser not initialized. Call initialize() first.");
    }
    return this.page;
  }

  /**
   * Get the Stagehand context (V3 API)
   */
  getContext(): unknown {
    return this.stagehand?.context || null;
  }

  /**
   * Navigate to a URL (Stagehand V3 API)
   */
  async navigate(url: string): Promise<void> {
    const page = await this.getPage();
    try {
      // V3 page.goto() is similar to Playwright
      await (page as any).goto(url, { waitUntil: "domcontentloaded", timeout: 60000 });
    } catch (error) {
      // If navigation times out, check if we're still on the page
      const currentUrl = (page as any).url();
      if (currentUrl && currentUrl.includes(new URL(url).hostname)) {
        log("info", `[StagehandBrowser] Page loaded (partial): ${currentUrl}`);
      } else {
        throw error;
      }
    }
    // V3 uses setTimeout instead of page.waitForTimeout
    await new Promise(resolve => setTimeout(resolve, 3000));
  }

  /**
   * Check if error is a rate limit error
   */
  private isRateLimitError(error: unknown): boolean {
    const errorMsg = error instanceof Error ? error.message : String(error);
    return errorMsg.toLowerCase().includes('too many tokens') ||
           errorMsg.toLowerCase().includes('rate limit') ||
           errorMsg.toLowerCase().includes('429');
  }

  /**
   * Observe page elements using Stagehand AI (V3 API) with rate limit retry
   * In V3, observe() is on the stagehand instance, not page
   *
   * @param instruction - Natural language instruction for what to observe
   * @param timeoutMs - Timeout in milliseconds, default: 30000 (30 seconds)
   */
  async observe(instruction: string, timeoutMs: number = 30000): Promise<ObserveResultItem[]> {
    if (!this.stagehand) {
      throw new Error("Browser not initialized. Call initialize() first.");
    }

    const maxRetries = 3;
    let lastError: unknown;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      const startTime = Date.now();
      try {
        // Use Promise.race to implement timeout
        const timeoutPromise = new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error(`observe_page timeout after ${timeoutMs}ms`)), timeoutMs)
        );

        const result = await Promise.race([
          this.stagehand.observe(instruction),
          timeoutPromise
        ]);

        await this.logStagehandMetrics('observe', startTime);

        // Log retry success if this was a retry
        if (attempt > 0) {
          log("info", `[StagehandBrowser] observe() succeeded on retry ${attempt}/${maxRetries}`);
        }

        return result;
      } catch (error) {
        lastError = error;
        const errorMessage = error instanceof Error ? error.message : String(error);

        // Check if this is a rate limit error
        const isRateLimit = this.isRateLimitError(error);

        // If rate limit and not last attempt, retry with exponential backoff
        if (isRateLimit && attempt < maxRetries) {
          const delayMs = Math.min(1000 * Math.pow(2, attempt), 30000); // Cap at 30s
          log("warn", `[StagehandBrowser] Rate limit detected on attempt ${attempt + 1}/${maxRetries + 1}, retrying in ${delayMs}ms...`);
          await new Promise(resolve => setTimeout(resolve, delayMs));
          continue;
        }

        // Log error
        log("error", `[StagehandBrowser] observe() failed: ${errorMessage}`);
        // Log full error for debugging
        if (error instanceof Error && error.stack) {
          log("error", `[StagehandBrowser] Stack: ${error.stack.substring(0, 500)}`);
        }

        // If not rate limit or last attempt, throw immediately
        if (!isRateLimit || attempt >= maxRetries) {
          throw error;
        }
      }
    }

    // Should never reach here, but throw last error just in case
    throw lastError;
  }

  /**
   * Perform an action using Stagehand AI (V3 API)
   * In V3, act() is on the stagehand instance, not page
   * Supports both natural language instructions and predefined action objects
   *
   * @param instructionOrAction - Natural language instruction string OR ActionObject with selector
   * @returns Action result from Stagehand
   * @throws ElementNotFoundError if element cannot be found
   * @throws ActionExecutionError if action fails to execute
   *
   * @example
   * // Natural language mode (AI inference)
   * await browser.act("click the search button");
   *
   * @example
   * // Selector mode (direct, faster)
   * await browser.act({
   *   selector: "#search-btn",
   *   description: "Search button",
   *   method: "click"
   * });
   */
  async act(instructionOrAction: string | ActionObject): Promise<unknown> {
    if (!this.stagehand) {
      throw new Error("Browser not initialized. Call initialize() first.");
    }

    const maxRetries = 3;
    let lastError: unknown;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      const startTime = Date.now();
      try {
        // V3 API: stagehand.act() accepts string instruction or Action object
        const result = await this.stagehand.act(instructionOrAction as any);
        await this.logStagehandMetrics('act', startTime);

        // Log retry success if this was a retry
        if (attempt > 0) {
          log("info", `[StagehandBrowser] act() succeeded on retry ${attempt}/${maxRetries}`);
        }

        return result;
      } catch (error) {
        lastError = error;
        const errorMessage = error instanceof Error ? error.message : String(error);

        // Check if this is a rate limit error
        const isRateLimit = this.isRateLimitError(error);

        // If rate limit and not last attempt, retry with exponential backoff
        if (isRateLimit && attempt < maxRetries) {
          const delayMs = Math.min(1000 * Math.pow(2, attempt), 30000); // Cap at 30s
          log("warn", `[StagehandBrowser] Rate limit detected in act() on attempt ${attempt + 1}/${maxRetries + 1}, retrying in ${delayMs}ms...`);
          await new Promise(resolve => setTimeout(resolve, delayMs));
          continue;
        }

        // Handle "Element not found" type errors
        if (
          errorMessage.includes("No object generated") ||
          errorMessage.includes("response did not match schema") ||
          errorMessage.includes("Could not find element")
        ) {
          const selector = typeof instructionOrAction === "string"
            ? undefined
            : instructionOrAction.selector;
          throw new ElementNotFoundError(
            `Element not found or action could not be performed. The page may have changed or the element is not visible.`,
            selector
          );
        }

        // Handle timeout errors
        if (errorMessage.includes("timeout") || errorMessage.includes("Timeout")) {
          throw new ActionExecutionError(
            `Action timed out. The element may be loading or not interactive.`,
            typeof instructionOrAction === "string" ? instructionOrAction : instructionOrAction.method,
            error instanceof Error ? error : undefined
          );
        }

        // If not rate limit or last attempt, throw with better context
        if (!isRateLimit || attempt >= maxRetries) {
          throw new ActionExecutionError(
            `Failed to execute action: ${errorMessage}`,
            typeof instructionOrAction === "string" ? instructionOrAction : instructionOrAction.method,
            error instanceof Error ? error : undefined
          );
        }
      }
    }

    // Should never reach here, but throw last error just in case
    throw lastError;
  }

  /**
   * Perform an action using a predefined selector (faster, no AI inference)
   * This is a convenience method that provides clearer semantics for selector-based actions
   *
   * @param action - ActionObject with selector, description, method, and optional arguments
   * @returns Action result from Stagehand
   */
  async actWithSelector(action: ActionObject): Promise<unknown> {
    log("info", `[StagehandBrowser] Acting with selector: ${action.method} on ${action.selector}`);
    return this.act(action);
  }

  /**
   * Auto-detect and close common popups/overlays using Stagehand AI (V3 API)
   */
  async autoClosePopups(): Promise<number> {
    if (!this.stagehand) {
      return 0;
    }

    let closedCount = 0;

    // Use Stagehand AI to find and close popups
    const popupInstructions = [
      "click the close button on any popup or modal",
      "click accept or dismiss on cookie consent banner",
      "click close on any overlay dialog",
    ];

    for (const instruction of popupInstructions) {
      try {
        const actions = await this.stagehand.observe(instruction);
        if (actions.length > 0) {
          await this.stagehand.act(actions[0]);
          closedCount++;
          log("info", `[StagehandBrowser] Closed popup with: ${instruction}`);
          await new Promise(resolve => setTimeout(resolve, 500));
        }
      } catch {
        // Ignore - no popup found
      }
    }

    if (closedCount > 0) {
      log("info", `[StagehandBrowser] Total popups closed: ${closedCount}`);
    }

    return closedCount;
  }

  /**
   * Get element attributes from XPath (V3 API)
   * Uses Playwright frameLocator for iframe elements, page.evaluate for regular elements
   * Supports cross-iframe XPath (e.g., /html/body/div/iframe[1]/html/body/...)
   */
  async getElementAttributesFromXPath(xpathSelector: string): Promise<{
    id?: string;
    dataTestId?: string;
    ariaLabel?: string;
    placeholder?: string;
    cssSelector?: string;
    tagName?: string;
    dataAttributes?: Record<string, string>;
    name?: string;
    textContent?: string;
    optimizedXPath?: string;
  } | null> {
    const page = await this.getPage();

    try {
      // Check if XPath contains iframe reference (e.g., /iframe[1]/html[1]/body[1]/)
      // Pattern: match the point where iframe content starts (after /iframe[n]/html[n]/body[n])
      const iframeMatch = xpathSelector.match(/^(.+?\/iframe\[\d+\])\/html\[\d+\]\/body\[\d+\](.*)$/i);

      let attrs: {
        tagName: string;
        id?: string;
        className?: string;
        placeholder?: string;
        ariaLabel?: string;
        dataTestId?: string;
        dataAttributes?: Record<string, string>;
        name?: string;
        textContent?: string;
      } | null = null;

      if (iframeMatch) {
        // Use page.frames() API for iframe elements
        const iframePath = iframeMatch[1];
        const elementPath = `/html[1]/body[1]${iframeMatch[2]}`;

        log("info", `[StagehandBrowser] Using frames() API for iframe element: iframePath=${iframePath}, elementPath=${elementPath}`);

        try {
          // Get all frames from the page
          const frames = (page as any).frames();
          log("debug", `[StagehandBrowser] Found ${frames.length} frames`);

          // Try each frame (skip the main frame at index 0)
          let foundAttrs = null;
          for (let i = 1; i < frames.length; i++) {
            const frame = frames[i];
            try {
              foundAttrs = await frame.evaluate((xpath: string) => {
                const result = document.evaluate(
                  xpath,
                  document,
                  null,
                  XPathResult.FIRST_ORDERED_NODE_TYPE,
                  null
                );
                const el = result.singleNodeValue as Element;
                if (!el) return null;

                const dataAttributes: Record<string, string> = {};
                for (const attr of Array.from(el.attributes)) {
                  if (attr.name.startsWith("data-")) {
                    dataAttributes[attr.name] = attr.value;
                  }
                }

                // Get text content (trim and limit length)
                const rawText = el.textContent?.trim() || "";
                const textContent = rawText.length > 50 ? rawText.substring(0, 50) : rawText;

                return {
                  tagName: el.tagName.toLowerCase(),
                  id: el.id || undefined,
                  className: (el as HTMLElement).className || undefined,
                  placeholder: (el as HTMLInputElement).placeholder || undefined,
                  ariaLabel: el.getAttribute("aria-label") || undefined,
                  dataTestId: el.getAttribute("data-testid") || undefined,
                  dataAttributes: Object.keys(dataAttributes).length > 0 ? dataAttributes : undefined,
                  name: el.getAttribute("name") || undefined,
                  textContent: textContent || undefined,
                };
              }, elementPath);

              if (foundAttrs) {
                log("info", `[StagehandBrowser] Found element in frame ${i}`);
                break;
              }
            } catch {
              // This frame doesn't have the element, try next
              continue;
            }
          }

          attrs = foundAttrs;

          if (attrs) {
            log("info", `[StagehandBrowser] Successfully extracted attrs from iframe element`);
          } else {
            log("warn", `[StagehandBrowser] Element not found in any frame: ${elementPath}`);
          }
        } catch (frameError) {
          log("warn", `[StagehandBrowser] frames() API failed: ${frameError}`);
          return null;
        }
      } else {
        // Regular element (not in iframe) - use page.evaluate
        attrs = await (page as any).evaluate((xpathStr: string) => {
          const result = document.evaluate(
            xpathStr,
            document,
            null,
            XPathResult.FIRST_ORDERED_NODE_TYPE,
            null
          );
          const el = result.singleNodeValue as Element;
          if (!el) return null;

          const dataAttributes: Record<string, string> = {};
          for (const attr of Array.from(el.attributes)) {
            if (attr.name.startsWith("data-")) {
              dataAttributes[attr.name] = attr.value;
            }
          }

          // Get text content (trim and limit length)
          const rawText = el.textContent?.trim() || "";
          const textContent = rawText.length > 50 ? rawText.substring(0, 50) : rawText;

          return {
            tagName: el.tagName.toLowerCase(),
            id: el.id || undefined,
            className: (el as HTMLElement).className || undefined,
            placeholder: (el as HTMLInputElement).placeholder || undefined,
            ariaLabel: el.getAttribute("aria-label") || undefined,
            dataTestId: el.getAttribute("data-testid") || undefined,
            dataAttributes: Object.keys(dataAttributes).length > 0 ? dataAttributes : undefined,
            name: el.getAttribute("name") || undefined,
            textContent: textContent || undefined,
          };
        }, xpathSelector);
      }

      if (!attrs) {
        log("warn", `[StagehandBrowser] getElementAttributesFromXPath: Element not found for xpath=${xpathSelector}`);
        return null;
      }

      // Filter out state-related data attributes
      attrs.dataAttributes = filterStateDataAttributes(attrs.dataAttributes);

      // Log all extracted attributes for debugging
      log("info", `[StagehandBrowser] getElementAttributesFromXPath: xpath=${xpathSelector}`);
      log("info", `[StagehandBrowser] Extracted attrs: tagName=${attrs.tagName}, id=${attrs.id}, className=${attrs.className}, dataTestId=${attrs.dataTestId}, ariaLabel=${attrs.ariaLabel}, placeholder=${attrs.placeholder}, dataAttributes=${JSON.stringify(attrs.dataAttributes)}`);

      // Build CSS selector from best available attribute
      let cssSelector: string | undefined;
      let cssSelectorSource: string = "none";

      if (attrs.dataTestId) {
        cssSelector = `[data-testid="${attrs.dataTestId}"]`;
        cssSelectorSource = "dataTestId";
      } else if (attrs.dataAttributes && Object.keys(attrs.dataAttributes).length > 0) {
        // Use other data-* attributes (data-id, data-component, etc.) - stable and i18n safe
        // Prefer semantic data attributes over random ones
        const preferredAttrs = ['data-id', 'data-component', 'data-element', 'data-action', 'data-section', 'data-name', 'data-type'];
        const dataKeys = Object.keys(attrs.dataAttributes);
        const preferredKey = preferredAttrs.find(k => dataKeys.includes(k)) || dataKeys[0];
        cssSelector = `[${preferredKey}="${attrs.dataAttributes[preferredKey]}"]`;
        cssSelectorSource = `dataAttr(${preferredKey})`;
      } else if (attrs.id) {
        // Use createIdSelector to handle special characters like dots (e.g., "cs.AI" -> '[id="cs.AI"]')
        cssSelector = createIdSelector(attrs.id);
        cssSelectorSource = "id";
      } else if (attrs.ariaLabel) {
        cssSelector = `[aria-label="${attrs.ariaLabel}"]`;
        cssSelectorSource = "ariaLabel";
      } else if (attrs.placeholder) {
        cssSelector = `${attrs.tagName}[placeholder="${attrs.placeholder}"]`;
        cssSelectorSource = "placeholder";
      } else if (attrs.className && typeof attrs.className === "string") {
        // Filter out hash-like class names (CSS Modules, styled-components, etc.)
        // but keep BEM naming (block__element, block--modifier)
        const allClasses = attrs.className.split(" ");
        log("info", `[StagehandBrowser] Processing className: allClasses=${JSON.stringify(allClasses)}`);

        const classes = allClasses.filter((c: string) => {
          if (!c) return false;
          // Keep BEM-style classes (e.g., msg-form__contenteditable, btn--primary)
          const isBEM = /^[a-z][a-z0-9]*(-[a-z0-9]+)*(__[a-z0-9]+(-[a-z0-9]+)*)?(--[a-z0-9]+(-[a-z0-9]+)*)?$/i.test(c);
          if (isBEM) {
            log("debug", `[StagehandBrowser] Class "${c}" matched BEM pattern, keeping`);
            return true;
          }
          // Filter out hash-like classes (e.g., sc-bdVaJa, css-1abc23, iKqMuZ)
          // These typically have: random mix of upper/lower case, or end with hash
          if (/^[a-z]{1,3}-[a-zA-Z0-9]{4,}$/.test(c)) {
            log("debug", `[StagehandBrowser] Class "${c}" matched hash pattern 1, filtering out`);
            return false;
          }
          if (/^[a-zA-Z]{2,}[A-Z][a-z]+$/.test(c)) {
            log("debug", `[StagehandBrowser] Class "${c}" matched hash pattern 2, filtering out`);
            return false;
          }
          if (/[A-Z].*[A-Z]/.test(c) && c.length < 12) {
            log("debug", `[StagehandBrowser] Class "${c}" matched hash pattern 3, filtering out`);
            return false;
          }
          // Keep other meaningful classes
          log("debug", `[StagehandBrowser] Class "${c}" kept as meaningful class`);
          return true;
        });

        log("info", `[StagehandBrowser] Filtered classes: ${JSON.stringify(classes)}`);

        if (classes.length > 0) {
          // Prefer BEM-style classes (with __ or --)
          const bemClass = classes.find((c: string) => c.includes('__') || c.includes('--'));
          cssSelector = `${attrs.tagName}.${bemClass || classes[0]}`;
          cssSelectorSource = bemClass ? `className(BEM: ${bemClass})` : `className(first: ${classes[0]})`;
        } else {
          log("warn", `[StagehandBrowser] All classes filtered out, no CSS selector from className`);
        }
      } else {
        log("warn", `[StagehandBrowser] No usable attributes for CSS selector: dataTestId=${attrs.dataTestId}, id=${attrs.id}, ariaLabel=${attrs.ariaLabel}, placeholder=${attrs.placeholder}, className=${attrs.className}`);
      }

      log("info", `[StagehandBrowser] Final CSS selector: ${cssSelector || "undefined"} (source: ${cssSelectorSource})`);

      // Generate optimized XPath based on element attributes
      const optimizedXPathResult = generateOptimizedXPath(attrs, xpathSelector);
      log("info", `[StagehandBrowser] Optimized XPath: ${optimizedXPathResult.xpath} (source: ${optimizedXPathResult.source})`);

      return {
        id: attrs.id,
        dataTestId: attrs.dataTestId,
        ariaLabel: attrs.ariaLabel,
        placeholder: attrs.placeholder,
        cssSelector,
        tagName: attrs.tagName,
        dataAttributes: attrs.dataAttributes,
        name: attrs.name,
        textContent: attrs.textContent,
        optimizedXPath: optimizedXPathResult.xpath,
      };
    } catch (error) {
      log("warn", `[StagehandBrowser] Failed to get element attributes: ${error}`);
      return null;
    }
  }

  /**
   * Try to get a CSS selector for an element given its XPath (V3 API)
   * @deprecated Use getElementAttributesFromXPath instead for full attribute access
   */
  async tryGetCssSelector(xpathSelector: string): Promise<string | undefined> {
    const result = await this.getElementAttributesFromXPath(xpathSelector);
    return result?.cssSelector;
  }

  /**
   * Wait for a specified time (V3 API - uses native setTimeout)
   */
  async wait(ms: number): Promise<void> {
    await new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Wait for text to appear on the page (V3 API)
   */
  async waitForText(text: string, timeout: number = 30000): Promise<void> {
    const page = await this.getPage();
    // V3 page may have different API
    try {
      await (page as any).waitForSelector(`text=${text}`, { timeout });
    } catch {
      // Fallback: poll for text
      const startTime = Date.now();
      while (Date.now() - startTime < timeout) {
        const content = await (page as any).content?.() || "";
        if (content.includes(text)) return;
        await this.wait(500);
      }
      throw new Error(`Text "${text}" not found within ${timeout}ms`);
    }
  }

  /**
   * Scroll the page (V3 API)
   */
  async scroll(direction: "up" | "down", amount: number = 300): Promise<void> {
    const page = await this.getPage();
    const delta = direction === "down" ? amount : -amount;
    try {
      // Try V3 page.mouse.wheel if available
      await (page as any).mouse?.wheel(0, delta);
    } catch {
      // Fallback: use keyboard scroll
      const key = direction === "down" ? "PageDown" : "PageUp";
      await (page as any).keyboard?.press(key);
    }
  }

  /**
   * Scroll to the bottom of the page to load lazy-loaded content (V3 API)
   * Performs incremental scrolling to trigger lazy load events
   *
   * @param waitAfterMs - Time to wait after scrolling completes (default: 1000ms)
   */
  async scrollToBottom(waitAfterMs: number = 1000): Promise<void> {
    const page = await this.getPage();

    try {
      // Get initial scroll height
      let lastScrollHeight = await (page as any).evaluate(() => document.body.scrollHeight);
      let scrollAttempts = 0;
      const maxAttempts = 10; // Prevent infinite scroll on infinite scroll pages

      while (scrollAttempts < maxAttempts) {
        // Scroll to current bottom
        await (page as any).evaluate(() => {
          window.scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' });
        });

        // Wait for content to load
        await this.wait(500);

        // Check if new content was loaded
        const newScrollHeight = await (page as any).evaluate(() => document.body.scrollHeight);

        if (newScrollHeight === lastScrollHeight) {
          // No new content, we've reached the bottom
          break;
        }

        lastScrollHeight = newScrollHeight;
        scrollAttempts++;
      }

      // Final wait for any remaining lazy-loaded content
      await this.wait(waitAfterMs);
      log("info", `[StagehandBrowser] Scrolled to bottom (${scrollAttempts} scroll iterations)`);
    } catch (error) {
      log("warn", `[StagehandBrowser] scrollToBottom failed: ${error}`);
      // Fallback: simple keyboard scroll
      for (let i = 0; i < 5; i++) {
        await (page as any).keyboard?.press("End");
        await this.wait(200);
      }
      await this.wait(waitAfterMs);
    }
  }

  /**
   * Navigate back to the previous page in browser history (V3 API)
   */
  async goBack(): Promise<void> {
    const page = await this.getPage();

    try {
      await (page as any).goBack({ waitUntil: 'domcontentloaded', timeout: 30000 });
      // Wait for page to stabilize
      await this.wait(1000);
      log("info", `[StagehandBrowser] Navigated back to: ${(page as any).url?.()}`);
    } catch (error) {
      log("warn", `[StagehandBrowser] goBack failed: ${error}`);
      throw error;
    }
  }

  /**
   * Get element attributes for better selector generation (V3 API)
   * Uses Stagehand observe to find the element, then extracts attributes via page.evaluate
   */
  async getElementAttributes(
    instruction: string
  ): Promise<{
    id?: string;
    dataTestId?: string;
    ariaLabel?: string;
    placeholder?: string;
  } | null> {
    const page = await this.getPage();

    try {
      // First, use observe to find the element
      const observeResults = await this.observe(instruction);
      if (observeResults.length === 0 || !observeResults[0].selector) {
        return null;
      }

      const selector = observeResults[0].selector;
      const xpath = selector.replace(/^xpath=/, "");

      // Use page.evaluate with document.evaluate for XPath
      const attributes = await (page as any).evaluate((xpathStr: string) => {
        const result = document.evaluate(
          xpathStr,
          document,
          null,
          XPathResult.FIRST_ORDERED_NODE_TYPE,
          null
        );
        const el = result.singleNodeValue as Element;
        if (!el) return null;

        return {
          id: el.id || undefined,
          dataTestId: el.getAttribute("data-testid") || undefined,
          ariaLabel: el.getAttribute("aria-label") || undefined,
          placeholder: (el as HTMLInputElement).placeholder || undefined,
        };
      }, xpath);

      return attributes;
    } catch (error) {
      log("warn", `[StagehandBrowser] Failed to get element attributes: ${error}`);
      return null;
    }
  }

  /**
   * Get accumulated token usage statistics from Stagehand operations
   */
  getTokenStats(): { input: number; output: number; total: number } {
    return {
      input: this.accumulatedInputTokens,
      output: this.accumulatedOutputTokens,
      total: this.accumulatedInputTokens + this.accumulatedOutputTokens,
    };
  }

  /**
   * Close the browser
   */
  async close(): Promise<void> {
    if (this.stagehand) {
      await this.stagehand.close();
      this.stagehand = null;
      this.page = null;
      log("info", "[StagehandBrowser] Browser closed.");
    }
  }
}


