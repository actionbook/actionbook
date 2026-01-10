import type OpenAI from "openai";
import type { BrowserAdapter } from "@actionbookdev/browser";
import { AIClient } from "../llm/AIClient.js";
import { YamlWriter } from "../writers/YamlWriter.js";
import { DbWriter } from "../writers/DbWriter.js";
import { SelectorOptimizer } from "../optimizer/SelectorOptimizer.js";
import { log } from "../utils/logger.js";
import { truncate, humanDelay, createIdSelector } from "../utils/index.js";
import { isTargetPage } from "../utils/url-matcher.js";
import { getRecorderTools } from "./RecorderTools.js";
import { RecorderToolExecutor } from "./RecorderToolExecutor.js";
import type {
  RecorderConfig,
  RecordResult,
  SiteCapability,
  ElementCapability,
  ElementType,
  AllowMethod,
  ObserveResultItem,
  SelectorItem,
  SelectorType,
  TemplateParam,
  StepEvent,
  TerminationReason,
} from "../types/index.js";
import { sleep } from "../utils/retry.js";

/**
 * Action Recorder - Records website UI element capabilities
 */
export class ActionRecorder {
  private browser: BrowserAdapter;
  private llmClient: AIClient;
  private config: RecorderConfig;
  private yamlWriter: YamlWriter;
  private dbWriter: DbWriter | null = null;
  private toolExecutor: RecorderToolExecutor;

  // Recording state
  private siteCapability: SiteCapability | null = null;
  private currentPageType: string = "unknown";
  private currentUrl: string = "";
  private previousUrl: string = "";  // Track previous URL for going back
  private primaryDomain: string = "";  // Primary domain for the recording session
  private discoveredElements: Map<string, ElementCapability> = new Map();
  private visitedUrls: Set<string> = new Set();  // Track visited URLs (pathname + query params)

  // Token tracking
  private inputTokens: number = 0;
  private outputTokens: number = 0;

  // DB task tracking
  private currentTaskId: number | null = null;
  private stepOrder: number = 0;
  private taskStatusUpdated: boolean = false; // Prevent race condition between timeout and normal completion

  // Termination tracking
  private taskStartTime: number = 0;
  private observeCallCount: number = 0;
  private observeElementsTotal: number = 0;
  private visitedPagesCount: number = 0;
  private currentTurn: number = 0;

  // P0-2: Track if we've scrolled on current page for autoScrollToBottom
  private hasScrolledCurrentPage: boolean = false;

  constructor(
    browser: BrowserAdapter,
    llmClient: AIClient,
    config: RecorderConfig,
    dbWriter?: DbWriter
  ) {
    this.browser = browser;
    this.llmClient = llmClient;
    this.config = config;
    this.yamlWriter = new YamlWriter(config.outputDir);
    this.dbWriter = dbWriter || null;
    this.toolExecutor = new RecorderToolExecutor(
      this.browser,
      {
      ensureSiteCapability: (domain: string) => {
        if (!this.siteCapability) this.initializeSiteCapability(domain);
      },
      registerElement: (element: ElementCapability) => this.registerElement(element),
      setPageContext: ({ pageType, pageName, pageDescription, urlPattern, concreteUrl }) => {
        this.currentPageType = pageType;
        if (concreteUrl) this.currentUrl = concreteUrl;

        if (this.siteCapability && !this.siteCapability.pages[pageType]) {
          this.siteCapability.pages[pageType] = {
            page_type: pageType,
            name: pageName,
            description: pageDescription || pageName,
            url_patterns: urlPattern ? [urlPattern] : [],
            elements: {},
            concrete_url: concreteUrl,
          } as any;
        }

        log("info", `[ActionRecorder] Set page context: ${pageType}`);
      },
      onNavigate: (url: string) => {
        const urlObj = new URL(url);

        // Set primary domain on first navigation
        if (!this.primaryDomain) {
          this.primaryDomain = urlObj.hostname;
          log("info", `[ActionRecorder] Primary domain set: ${this.primaryDomain}`);
        }

        // Check for external domain
        if (urlObj.hostname !== this.primaryDomain) {
          log("info", `[ActionRecorder] External domain detected: ${urlObj.hostname} (primary: ${this.primaryDomain})`);
          return { isNew: false, reason: 'external_domain' as const };
        }

        // Normalize URL: pathname + sorted query parameters + hash
        // This ensures ?q=apple&page=1 and ?page=1&q=apple are treated as the same URL
        // Hash is included to support SPA hash routing (e.g., /#/user vs /#/settings)
        const sortedParams = new URLSearchParams(urlObj.searchParams);
        sortedParams.sort();
        const urlKey = urlObj.pathname + (sortedParams.toString() ? '?' + sortedParams.toString() : '') + urlObj.hash;

        // Check if already visited
        if (this.visitedUrls.has(urlKey)) {
          return { isNew: false, reason: 'already_visited' as const };
        }
        this.visitedUrls.add(urlKey);

        // Track visited pages count
        this.visitedPagesCount++;

        // Store previous URL before updating
        this.previousUrl = this.currentUrl;
        this.currentUrl = url;
        // P0-2: Reset scroll flag for new page
        this.hasScrolledCurrentPage = false;
        return { isNew: true };  // New URL
      },
      getCurrentUrl: () => {
        return this.currentUrl || null;
      },
      getPreviousUrl: () => {
        return this.previousUrl || null;
      },
      extractMultipleSelectors: (observeResult, cssSelector, attrs) =>
        this.extractMultipleSelectors(observeResult, cssSelector, attrs),
      detectTemplatePattern: (cssSelector: string) => this.detectTemplatePattern(cssSelector),
      inferElementType: (action: string, instruction: string) => this.inferElementType(action, instruction),
      inferAllowMethods: (action: string) => this.inferAllowMethods(action),
    });
  }

  /**
   * Update playbook-specific configuration at runtime
   * Called before record() to set targetUrlPattern and autoScrollToBottom
   */
  updatePlaybookConfig(options: {
    targetUrlPattern?: string;
    autoScrollToBottom?: boolean;
  }): void {
    if (options.targetUrlPattern !== undefined) {
      this.config.targetUrlPattern = options.targetUrlPattern;
      log("info", `[ActionRecorder] Target URL pattern set: ${options.targetUrlPattern}`);
    }
    if (options.autoScrollToBottom !== undefined) {
      this.config.autoScrollToBottom = options.autoScrollToBottom;
      log("info", `[ActionRecorder] Auto scroll to bottom: ${options.autoScrollToBottom}`);
    }
  }

  /**
   * Emit a step event to the configured callback
   */
  private async emitStepEvent(
    turns: number,
    maxTurns: number,
    toolName: string,
    toolArgs: Record<string, unknown>,
    toolResult: unknown,
    success: boolean,
    durationMs: number,
    error?: string
  ): Promise<void> {
    if (!this.config.onStepFinish) return;

    const event: StepEvent = {
      stepNumber: this.stepOrder,
      totalTurns: turns,
      maxTurns,
      toolName,
      toolArgs,
      toolResult,
      success,
      error,
      durationMs,
      pageType: this.currentPageType !== "unknown" ? this.currentPageType : undefined,
      timestamp: new Date(),
    };

    try {
      await this.config.onStepFinish(event);
    } catch (err) {
      log("warn", `[ActionRecorder] onStepFinish callback error: ${err}`);
    }
  }

  /**
   * Detect if a selector contains a dynamic value that should be templated
   * Returns template info if detected, null otherwise
   */
  private detectTemplatePattern(selector: string): {
    template: string;
    params: TemplateParam[]
  } | null {
    // Pattern: data-state--date-string='YYYY-MM-DD' or similar date patterns
    const datePatterns = [
      // Airbnb date selector: button[data-state--date-string='2025-12-10']
      {
        regex: /(\[data-state--date-string=')(\d{4}-\d{2}-\d{2})('\])/,
        paramName: 'date',
        paramType: 'date' as const,
        format: 'YYYY-MM-DD',
        description: 'Date in YYYY-MM-DD format',
      },
      // Generic date attribute patterns
      {
        regex: /(\[data-date=')(\d{4}-\d{2}-\d{2})('\])/,
        paramName: 'date',
        paramType: 'date' as const,
        format: 'YYYY-MM-DD',
        description: 'Date in YYYY-MM-DD format',
      },
      // aria-label with date
      {
        regex: /(\[aria-label='[^']*?)(\d{1,2}\/\d{1,2}\/\d{4}|\d{4}-\d{2}-\d{2})([^']*'\])/,
        paramName: 'date',
        paramType: 'date' as const,
        format: 'YYYY-MM-DD',
        description: 'Date value',
      },
    ];

    for (const pattern of datePatterns) {
      const match = selector.match(pattern.regex);
      if (match) {
        const template = selector.replace(pattern.regex, `$1{{${pattern.paramName}}}$3`);
        return {
          template,
          params: [{
            name: pattern.paramName,
            type: pattern.paramType,
            format: pattern.format,
            description: pattern.description,
          }],
        };
      }
    }

    return null;
  }

  /**
   * Extract multiple selectors from observe result (new format)
   * Returns array of SelectorItem sorted by priority
   */
  private extractMultipleSelectors(
    observeResult: ObserveResultItem,
    cssSelector?: string,
    additionalInfo?: {
      ariaLabel?: string;
      dataTestId?: string;
      placeholder?: string;
      id?: string;
      dataAttributes?: Record<string, string>;
    }
  ): SelectorItem[] {
    const selectors: SelectorItem[] = [];
    let priority = 1;

    // 1. ID selector (highest priority)
    // Use createIdSelector to handle special characters like dots (e.g., "cs.AI" -> '[id="cs.AI"]')
    if (additionalInfo?.id) {
      selectors.push({
        type: 'id' as SelectorType,
        value: createIdSelector(additionalInfo.id),
        priority: priority++,
        confidence: 0.95,
      });
    }

    // 2. data-testid (very stable)
    if (additionalInfo?.dataTestId) {
      selectors.push({
        type: 'data-testid' as SelectorType,
        value: `[data-testid="${additionalInfo.dataTestId}"]`,
        priority: priority++,
        confidence: 0.9,
      });
    }

    // 3. aria-label (semantic, stable)
    if (additionalInfo?.ariaLabel) {
      selectors.push({
        type: 'aria-label' as SelectorType,
        value: `[aria-label="${additionalInfo.ariaLabel}"]`,
        priority: priority++,
        confidence: 0.85,
      });
    }

    // 4. data-* attributes (including date strings)
    const addedDataAttrs = new Set<string>();
    if (additionalInfo?.dataAttributes) {
      for (const [attrName, attrValue] of Object.entries(additionalInfo.dataAttributes)) {
        const attrSelector = `[${attrName}="${attrValue}"]`;
        addedDataAttrs.add(attrSelector);
        const templateInfo = this.detectTemplatePattern(attrSelector);
        selectors.push({
          type: 'css' as SelectorType,
          value: templateInfo ? templateInfo.template : attrSelector,
          priority: priority++,
          confidence: 0.8,
          isTemplate: templateInfo?.params ? true : undefined,
          templateParams: templateInfo?.params,
        });
      }
    }

    // 5. CSS selector (skip if already added as data-* attribute)
    if (cssSelector && !addedDataAttrs.has(cssSelector)) {
      const templateInfo = this.detectTemplatePattern(cssSelector);
      if (templateInfo) {
        selectors.push({
          type: 'css' as SelectorType,
          value: templateInfo.template,
          priority: priority++,
          isTemplate: true,
          templateParams: templateInfo.params,
          confidence: 0.8,
        });
      } else {
        selectors.push({
          type: 'css' as SelectorType,
          value: cssSelector,
          priority: priority++,
          confidence: 0.75,
        });
      }
    }

    // 6. XPath selector (from observe result)
    if (observeResult.selector) {
      const sel = observeResult.selector;
      if (sel.startsWith("xpath=") || sel.startsWith("/")) {
        const xpathValue = sel.replace(/^xpath=/, "");
        selectors.push({
          type: 'xpath' as SelectorType,
          value: xpathValue,
          priority: priority++,
          confidence: 0.6,
        });
      } else if (!cssSelector) {
        // It's a CSS selector from observe
        const templateInfo = this.detectTemplatePattern(sel);
        if (templateInfo) {
          selectors.push({
            type: 'css' as SelectorType,
            value: templateInfo.template,
            priority: priority++,
            isTemplate: true,
            templateParams: templateInfo.params,
            confidence: 0.8,
          });
        } else {
          selectors.push({
            type: 'css' as SelectorType,
            value: sel,
            priority: priority++,
            confidence: 0.7,
          });
        }
      }
    }

    // 7. Placeholder (for inputs)
    if (additionalInfo?.placeholder) {
      selectors.push({
        type: 'placeholder' as SelectorType,
        value: `[placeholder="${additionalInfo.placeholder}"]`,
        priority: priority++,
        confidence: 0.65,
      });
    }

    return selectors;
  }

  /**
   * Infer element type from action and instruction
   */
  private inferElementType(action: string, instruction: string): ElementType {
    const lower = instruction.toLowerCase();
    if (
      action === "type" ||
      lower.includes("input") ||
      lower.includes("field") ||
      lower.includes("textbox")
    ) {
      return "input";
    }
    if (lower.includes("button") || lower.includes("submit")) {
      return "button";
    }
    if (lower.includes("link") || lower.includes("navigate")) {
      return "link";
    }
    if (lower.includes("select") || lower.includes("dropdown")) {
      return "select";
    }
    if (lower.includes("checkbox")) {
      return "checkbox";
    }
    return "other";
  }

  /**
   * Infer allowed methods from action
   */
  private inferAllowMethods(action: string): AllowMethod[] {
    switch (action) {
      case "type":
        return ["click", "type", "clear"];
      case "click":
        return ["click"];
      case "hover":
        return ["hover", "click"];
      default:
        return ["click"];
    }
  }

  /**
   * Initialize site capability structure
   */
  private initializeSiteCapability(domain: string): void {
    this.siteCapability = {
      domain,
      name: domain,
      description: "",
      version: "1.0.0",
      recorded_at: new Date().toISOString(),
      scenario: "",
      global_elements: {},
      pages: {},
    };
  }

  /**
   * Register an element capability
   */
  private registerElement(element: ElementCapability): void {
    if (!this.siteCapability) return;

    // P0-1: Check if current page matches targetUrlPattern
    // Skip elements on non-target pages when pattern is specified
    if (this.config.targetUrlPattern && this.currentUrl) {
      if (!isTargetPage(this.currentUrl, this.config.targetUrlPattern)) {
        log("info", `[ActionRecorder] Skipping element ${element.id} - page does not match pattern: ${this.config.targetUrlPattern}`);
        return;
      }
    }

    if (
      this.currentPageType &&
      this.siteCapability.pages[this.currentPageType]
    ) {
      this.siteCapability.pages[this.currentPageType].elements[element.id] =
        element;
    } else {
      this.siteCapability.global_elements[element.id] = element;
    }

    this.discoveredElements.set(element.id, element);
    const urlObj = this.currentUrl ? new URL(this.currentUrl) : null;
    const urlInfo = urlObj ? `${urlObj.origin}${urlObj.pathname} (${urlObj.pathname})` : 'N/A';
    log("info", `[ActionRecorder] Registered element: ${element.id} | Page: ${this.currentPageType} | URL: ${urlInfo}`);
  }

  /**
   * Record a step to the database (if dbWriter is available)
   */
  private async recordStep(
    toolName: string,
    toolInput: Record<string, unknown>,
    toolOutput: unknown,
    status: 'success' | 'failed',
    durationMs: number,
    errorMessage?: string
  ): Promise<void> {
    if (!this.dbWriter || !this.currentTaskId) return;

    try {
      this.stepOrder++;
      await this.dbWriter.addStep(this.currentTaskId, {
        stepOrder: this.stepOrder,
        toolName,
        toolInput,
        toolOutput,
        pageType: this.currentPageType !== 'unknown' ? this.currentPageType : undefined,
        durationMs,
        status,
        errorMessage,
      });
    } catch (err) {
      log('error', `[ActionRecorder] Failed to record step: ${err}`);
    }
  }

  /**
   * Get total tokens used
   */
  private get totalTokens(): number {
    return this.inputTokens + this.outputTokens;
  }

  /**
   * Print execution statistics (simplified - detailed output handled by caller)
   */
  private printStatistics(
    totalDuration: number,
    success: boolean,
    browserTokens?: { input: number; output: number }
  ): void {
    const status = success ? "✅ SUCCESS" : "❌ FAILED";
    const domain = this.siteCapability?.domain || "unknown";
    const pages = Object.keys(this.siteCapability?.pages || {}).length;
    const elements = this.discoveredElements.size;
    const durationSec = (totalDuration / 1000).toFixed(1);

    // Calculate combined tokens
    const browserIn = browserTokens?.input || 0;
    const browserOut = browserTokens?.output || 0;
    const totalIn = this.inputTokens + browserIn;
    const totalOut = this.outputTokens + browserOut;
    const totalTokens = totalIn + totalOut;

    log("info", `[ActionRecorder] ${status} | Domain: ${domain} | Pages: ${pages} | Elements: ${elements} | Duration: ${durationSec}s | Tokens: in=${totalIn}, out=${totalOut}, total=${totalTokens} (planning: ${this.inputTokens}/${this.outputTokens}, browser: ${browserIn}/${browserOut}) | Steps: ${this.stepOrder}`);
  }

  /**
   * Check if task should be terminated based on configured limits
   * @returns Object with shouldTerminate flag and reason
   */
  private checkTermination(): { shouldTerminate: boolean; reason: TerminationReason | null } {
    const config = this.config;
    const termConfig = config.terminationConfig || {};

    // 1. Task total timeout check (default: 15 minutes)
    const taskTimeoutMs = config.taskTimeoutMs ?? 15 * 60 * 1000;
    const elapsed = Date.now() - this.taskStartTime;
    if (elapsed >= taskTimeoutMs) {
      return { shouldTerminate: true, reason: 'task_timeout' };
    }

    // 2. Token limit check (-1 = unlimited)
    const maxTokens = config.maxTokens ?? -1;
    if (maxTokens > 0 && this.totalTokens >= maxTokens) {
      return { shouldTerminate: true, reason: 'max_tokens_reached' };
    }

    // 3. Element threshold check (default: 80)
    const elementThreshold = termConfig.elementThreshold ?? 80;
    if (this.discoveredElements.size >= elementThreshold) {
      return { shouldTerminate: true, reason: 'element_threshold_reached' };
    }

    // 4. Observe efficiency check (efficiency < 3 and 3+ calls)
    const minCalls = termConfig.minObserveCallsForCheck ?? 3;
    const minEfficiency = termConfig.minObserveEfficiency ?? 3;
    if (this.observeCallCount >= minCalls) {
      const avgEfficiency = this.observeElementsTotal / this.observeCallCount;
      if (avgEfficiency < minEfficiency) {
        return { shouldTerminate: true, reason: 'low_observe_efficiency' };
      }
    }

    // 5. Maximum pages visited check (default: 5)
    const maxPages = config.maxVisitedPages ?? 5;
    if (this.visitedPagesCount >= maxPages) {
      return { shouldTerminate: true, reason: 'max_pages_visited' };
    }

    return { shouldTerminate: false, reason: null };
  }

  /**
   * Execute a tool with timeout and retry logic
   * @returns Tool execution result or error result if all retries failed
   */
  private async executeToolWithRetry(
    toolName: string,
    toolArgs: Record<string, unknown>
  ): Promise<{ output: unknown; content: string; skipped?: boolean }> {
    const maxAttempts = this.config.retryConfig?.maxAttempts ?? 3;
    const baseDelay = this.config.retryConfig?.baseDelayMs ?? 1000;

    const timeouts = this.config.operationTimeouts || {};
    const observeTimeout = timeouts.observe ?? 30000;

    // P0-2: Auto-scroll before observe_page if enabled and not yet scrolled
    if (toolName === 'observe_page' && !this.hasScrolledCurrentPage) {
      const autoScroll = this.config.autoScrollToBottom !== false; // Default: true
      if (autoScroll) {
        try {
          log("info", `[ActionRecorder] Auto-scrolling to bottom before observe_page...`);
          await this.browser.scrollToBottom(1000);
          this.hasScrolledCurrentPage = true;
          log("info", `[ActionRecorder] Auto-scroll complete`);
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          log("warn", `[ActionRecorder] Auto-scroll failed: ${errMsg}`);
          // Continue with observe_page even if scroll fails
        }
      }
    }

    // Track manual scroll_to_bottom calls
    if (toolName === 'scroll_to_bottom') {
      this.hasScrolledCurrentPage = true;
    }

    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      try {
        // For observe_page, pass timeout to toolExecutor
        if (toolName === 'observe_page') {
          // Add timeout to toolArgs for observe_page
          const argsWithTimeout = { ...toolArgs, _timeoutMs: observeTimeout };
          return await this.toolExecutor.execute(toolName, argsWithTimeout);
        }
        return await this.toolExecutor.execute(toolName, toolArgs);
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        log("warn", `[ActionRecorder] Tool ${toolName} attempt ${attempt}/${maxAttempts} failed: ${errorMessage}`);

        if (attempt === maxAttempts) {
          // All retries exhausted, return error result and mark as skipped
          log("warn", `[ActionRecorder] Tool ${toolName} failed after ${maxAttempts} attempts, skipping`);
          return {
            output: { error: 'max_retries_exceeded', message: errorMessage },
            content: JSON.stringify({ error: 'skipped', reason: `Failed after ${maxAttempts} attempts: ${errorMessage}` }),
            skipped: true,
          };
        }

        // Exponential backoff
        const delay = baseDelay * Math.pow(2, attempt - 1);
        await sleep(delay);
      }
    }

    // Should never reach here, but TypeScript needs this
    return { output: { error: 'unexpected' }, content: JSON.stringify({ error: 'unexpected' }), skipped: true };
  }

  /**
   * Calculate combined token statistics from planning and browser usage
   * @returns Token statistics object with input, output, total, and breakdown by source
   */
  private calculateTokenStats(): {
    input: number;
    output: number;
    total: number;
    planning: { input: number; output: number };
    browser: { input: number; output: number };
  } {
    // Get browser (Stagehand) token stats if available
    let browserInputTokens = 0;
    let browserOutputTokens = 0;
    if (this.browser.getTokenStats) {
      const stats = this.browser.getTokenStats();
      if (stats) {
        browserInputTokens = stats.input;
        browserOutputTokens = stats.output;
      }
    }

    // Calculate combined totals
    const totalInputTokens = this.inputTokens + browserInputTokens;
    const totalOutputTokens = this.outputTokens + browserOutputTokens;
    const combinedTotalTokens = totalInputTokens + totalOutputTokens;

    return {
      input: totalInputTokens,
      output: totalOutputTokens,
      total: combinedTotalTokens,
      planning: { input: this.inputTokens, output: this.outputTokens },
      browser: { input: browserInputTokens, output: browserOutputTokens },
    };
  }

  /**
   * Optimize selectors for all discovered elements using LLM
   * Updates siteCapability in-place with optimized selectors
   */
  private async optimizeAndUpdateSelectors(): Promise<void> {
    if (this.config.enableSelectorOptimization === false) {
      return;
    }

    if (!this.siteCapability || this.discoveredElements.size === 0) {
      return;
    }

    try {
      log("info", `[ActionRecorder] Optimizing selectors for ${this.discoveredElements.size} elements...`);
      const optimizer = new SelectorOptimizer();
      const optimizationResult = await optimizer.optimizeSelectors(this.discoveredElements);

      if (optimizationResult.success) {
        log("info", `[ActionRecorder] Selector optimization complete: ${optimizationResult.optimizedCount}/${optimizationResult.totalElements} elements optimized`);

        // Update siteCapability with optimized selectors
        for (const optElement of optimizationResult.elements) {
          const element = this.discoveredElements.get(optElement.elementId);
          if (element) {
            element.selectors = optElement.optimizedSelectors;

            // Update in siteCapability pages
            for (const page of Object.values(this.siteCapability.pages)) {
              if (page.elements[optElement.elementId]) {
                page.elements[optElement.elementId].selectors = optElement.optimizedSelectors;
              }
            }
            // Update in global_elements
            if (this.siteCapability.global_elements[optElement.elementId]) {
              this.siteCapability.global_elements[optElement.elementId].selectors = optElement.optimizedSelectors;
            }
          }
        }
      } else {
        log("warn", `[ActionRecorder] Selector optimization failed: ${optimizationResult.error}`);
      }
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      log("warn", `[ActionRecorder] Selector optimization error: ${errMsg}`);
    }
  }

  /**
   * Save siteCapability to YAML file and database
   * @returns Object with savedPath and optional dbSaveError
   */
  private async saveToYamlAndDb(): Promise<{
    savedPath?: string;
    dbSaveError?: string;
  }> {
    if (!this.siteCapability || this.discoveredElements.size === 0) {
      return {};
    }

    let savedPath: string | undefined;
    let dbSaveError: string | undefined;

    // Save to YAML
    savedPath = this.yamlWriter.save(this.siteCapability);
    log("info", `[ActionRecorder] Saved ${this.discoveredElements.size} elements to YAML: ${savedPath}`);

    // Save to database (dual-write)
    if (this.dbWriter) {
      try {
        const sourceId = await this.dbWriter.save(this.siteCapability);
        log("info", `[ActionRecorder] Saved to database, sourceId: ${sourceId}`);
      } catch (err) {
        const errMsg = err instanceof Error ? err.message : String(err);
        log("error", `[ActionRecorder] Failed to save to database: ${errMsg}`);
        dbSaveError = errMsg;
      }
    }

    return { savedPath, dbSaveError };
  }

  /**
   * Finalize recording and save results
   * Unified method for saving to YAML/DB and building RecordResult
   */
  private async finalizeResult(options: {
    reason: TerminationReason;
    scenario: string;
    siteName?: string;
    siteDescription?: string;
    message?: string;
    partialComplete?: boolean;
  }): Promise<RecordResult> {
    const { reason, scenario, siteName, siteDescription, partialComplete } = options;
    const totalDuration = Date.now() - this.taskStartTime;
    const hasElements = this.discoveredElements.size > 0;

    // Calculate token statistics
    const tokenStats = this.calculateTokenStats();

    if (reason !== 'completed') {
      log("warn", `[ActionRecorder] Task terminated: ${reason}`);
    }

    // Update site info
    if (this.siteCapability) {
      this.siteCapability.scenario = scenario;
      if (siteName) this.siteCapability.name = siteName;
      if (siteDescription) this.siteCapability.description = siteDescription;
    }

    // Save to YAML and DB
    let savedPath: string | undefined;
    let dbSaveError: string | undefined;

    if (hasElements && this.siteCapability) {
      // Optimize selectors using LLM before saving
      await this.optimizeAndUpdateSelectors();

      // Save to YAML and database
      const saveResult = await this.saveToYamlAndDb();
      savedPath = saveResult.savedPath;
      dbSaveError = saveResult.dbSaveError;
    }

    // Update task status in database
    // Skip if already updated by timeout handler (prevent race condition)
    if (this.dbWriter && this.currentTaskId && !this.taskStatusUpdated) {
      try {
        const status = hasElements ? 'completed' : 'failed';
        const statusMessage = options.message || `Task ${reason}. Discovered ${this.discoveredElements.size} elements.`;
        await this.dbWriter.completeTask(
          this.currentTaskId,
          status,
          totalDuration,
          tokenStats.total,
          status === 'failed' ? statusMessage : undefined
        );
        this.taskStatusUpdated = true;
      } catch (err) {
        log("error", `[ActionRecorder] Failed to update task status: ${err}`);
      }
    }

    this.printStatistics(totalDuration, hasElements, { input: tokenStats.browser.input, output: tokenStats.browser.output });

    // Build result message
    const resultMessage = options.message || (hasElements
      ? `Task ${reason}. Discovered ${this.discoveredElements.size} elements.`
      : `Task ${reason} without discovering elements.`);

    return {
      success: hasElements,
      message: resultMessage,
      turns: this.currentTurn,
      steps: this.stepOrder,
      totalDuration,
      tokens: tokenStats,
      elementsDiscovered: this.discoveredElements.size,
      siteCapability: this.siteCapability || undefined,
      savedPath,
      dbSaveError,
      terminationReason: reason,
      partialComplete,
      observeStats: {
        totalCalls: this.observeCallCount,
        totalElements: this.observeElementsTotal,
        avgEfficiency: this.observeCallCount > 0 ? this.observeElementsTotal / this.observeCallCount : 0,
      },
      visitedPagesCount: this.visitedPagesCount,
    };
  }

  /**
   * Save partial result when timeout occurs
   * Optimizes and saves discovered elements even if recording is incomplete
   *
   * @returns Object with element count and siteCapability, or null if nothing to save
   */
  async savePartialResult(): Promise<{
    elements: number;
    siteCapability: any;
    turns: number;
    steps: number;
    tokens: {
      input: number;
      output: number;
      total: number;
      planning: { input: number; output: number };
      browser: { input: number; output: number };
    };
  } | null> {
    const hasElements = this.discoveredElements.size > 0;

    if (!hasElements || !this.siteCapability) {
      log("warn", "[ActionRecorder] No elements to save in partial result");
      return null;
    }

    try {
      log("info", `[ActionRecorder] Saving partial result with ${this.discoveredElements.size} elements...`);

      // Optimize selectors using LLM before saving
      await this.optimizeAndUpdateSelectors();

      // Save to YAML and database
      await this.saveToYamlAndDb();

      // Calculate token statistics
      const tokenStats = this.calculateTokenStats();

      // Mark task status as updated to prevent race condition with finalizeResult()
      this.taskStatusUpdated = true;

      return {
        elements: this.discoveredElements.size,
        siteCapability: this.siteCapability,
        turns: this.currentTurn,
        steps: this.stepOrder,
        tokens: tokenStats,
      };
    } catch (error) {
      const errMsg = error instanceof Error ? error.message : String(error);
      log("error", `[ActionRecorder] savePartialResult failed: ${errMsg}`);
      return null;
    }
  }

  /**
   * Record capabilities from a scenario
   */
  async record(
    scenario: string,
    systemPrompt: string,
    userMessage: string,
    siteName?: string,
    siteDescription?: string,
    startUrl?: string,
    existingTaskId?: number
  ): Promise<RecordResult> {
    const startTime = Date.now();
    this.taskStartTime = startTime;

    // Reset state
    this.discoveredElements.clear();
    this.visitedUrls.clear();
    this.currentPageType = "unknown";
    this.currentUrl = "";
    this.previousUrl = "";
    this.primaryDomain = "";
    this.inputTokens = 0;
    this.outputTokens = 0;
    this.siteCapability = null;
    this.currentTaskId = null;
    this.stepOrder = 0;
    this.taskStatusUpdated = false; // Reset race condition flag

    // Reset termination tracking
    this.observeCallCount = 0;
    this.observeElementsTotal = 0;
    this.visitedPagesCount = 0;
    this.currentTurn = 0;
    // P0-2: Reset scroll tracking
    this.hasScrolledCurrentPage = false;

    const tools = getRecorderTools();
    const messages: OpenAI.Chat.Completions.ChatCompletionMessageParam[] = [
      { role: "system", content: systemPrompt },
      { role: "user", content: userMessage },
    ];

    const maxTurns = this.config.maxTurns;

    log("info", `[ActionRecorder] Starting capability recording: ${scenario}`);
    log("info", `[ActionRecorder] Task: ${truncate(userMessage, 100)}`);

    // Create recording task in database if dbWriter is available
    // Note: We need a sourceId to create a task, but we don't have domain yet.
    // We'll create the task after the first navigate call when we have the domain.
    let taskCreated = false;

    while (this.currentTurn < maxTurns) {
      // Check termination conditions at the start of each turn
      const termCheck = this.checkTermination();
      if (termCheck.shouldTerminate) {
        return this.finalizeResult({
          reason: termCheck.reason!,
          scenario,
          siteName,
          siteDescription,
          partialComplete: true,
        });
      }

      this.currentTurn++;
      const urlObj = this.currentUrl ? new URL(this.currentUrl) : null;
      const urlInfo = urlObj ? `${urlObj.origin}${urlObj.pathname} (${urlObj.pathname})` : 'N/A';
      const totalTokens = this.inputTokens + this.outputTokens;
      log("info", `[ActionRecorder] --- Turn ${this.currentTurn}/${maxTurns} --- URL: ${urlInfo} | Page: ${this.currentPageType} | Tokens: in=${this.inputTokens}, out=${this.outputTokens}, total=${totalTokens} | Elements: ${this.discoveredElements.size} | Pages: ${this.visitedPagesCount}`);

      if (this.currentTurn > 1) {
        await humanDelay(500, 1500);
      }

      const response = await this.llmClient.chat(messages, tools);

      // Track tokens separately
      const promptTokens = response.usage?.prompt_tokens || 0;
      const completionTokens = response.usage?.completion_tokens || 0;
      this.inputTokens += promptTokens;
      this.outputTokens += completionTokens;

      // Handle empty response
      if (!response.choices || response.choices.length === 0) {
        log("error", "[ActionRecorder] Empty response from LLM");
        continue;
      }

      const choice = response.choices[0];
      const assistantMessage = choice.message;

      if (assistantMessage.content) {
        log(
          "info",
          `[ActionRecorder] Assistant: ${truncate(assistantMessage.content, 200)}`
        );
      }

      // Check if completed (no more tool calls)
      if (!assistantMessage.tool_calls || assistantMessage.tool_calls.length === 0) {
        return this.finalizeResult({
          reason: 'completed',
          scenario,
          siteName,
          siteDescription,
          message: assistantMessage.content || "Capability recording completed.",
        });
      }

      messages.push(assistantMessage);

      // Execute tool calls
      for (const toolCall of assistantMessage.tool_calls) {
        if (toolCall.type !== "function") continue;

        const toolName = toolCall.function.name;
        let toolArgs: Record<string, unknown>;

        try {
          toolArgs = JSON.parse(toolCall.function.arguments);
        } catch {
          toolArgs = {};
        }

        const argsPreview = Object.entries(toolArgs)
          .map(([k, v]) => `${k}=${truncate(String(v), 30)}`)
          .join(", ");
        log("info", `[ActionRecorder] Executing: ${toolName}(${argsPreview})`);

        const toolStartTime = Date.now();
        try {
          // Use executeToolWithRetry for navigate and observe_page
          const shouldRetry = toolName === 'navigate' || toolName === 'observe_page';
          const result = shouldRetry
            ? await this.executeToolWithRetry(toolName, toolArgs)
            : await this.toolExecutor.execute(toolName, toolArgs);

          const { output, content, skipped } = result as { output: unknown; content: string; skipped?: boolean };
          const toolDuration = Date.now() - toolStartTime;
          log("info", `[ActionRecorder] Result: ${truncate(content, 200)}${skipped ? ' (skipped after retries)' : ''}`);

          // Track observe statistics
          if (toolName === 'observe_page' && !skipped) {
            this.observeCallCount++;
            const outputObj = output as { elements_found?: number } | null;
            if (outputObj && typeof outputObj.elements_found === 'number') {
              this.observeElementsTotal += outputObj.elements_found;
            }
          }

          // Record step to database
          await this.recordStep(toolName, toolArgs, output, skipped ? 'failed' : 'success', toolDuration, skipped ? 'Skipped after max retries' : undefined);

          // Emit step event for successful execution
          await this.emitStepEvent(
            this.currentTurn,
            maxTurns,
            toolName,
            toolArgs,
            content,
            true,
            toolDuration
          );

          // Create or use recording task after first navigate (when we have domain)
          if (!taskCreated && this.dbWriter && this.siteCapability !== null) {
            const cap = this.siteCapability as SiteCapability;
            try {
              // First ensure we have a source record
              const sourceId = await this.dbWriter.save(cap);

              // Use existing task ID if provided (TaskWorker mode), otherwise create new
              if (existingTaskId) {
                this.currentTaskId = existingTaskId;
                log("info", `[ActionRecorder] Using existing task: ${this.currentTaskId}`);
              } else {
                // Then create the recording task with the actual startUrl
                this.currentTaskId = await this.dbWriter.createTask(
                  sourceId,
                  scenario,
                  startUrl || `https://${cap.domain}`  // Use provided startUrl or fallback
                );
                log("info", `[ActionRecorder] Created recording task: ${this.currentTaskId}`);
              }
              taskCreated = true;
            } catch (err) {
              log("error", `[ActionRecorder] Failed to setup recording task: ${err}`);
            }
          }

          await humanDelay(300, 800);

          messages.push({
            role: "tool",
            tool_call_id: toolCall.id,
            content,
          });
        } catch (error) {
          const errorMessage =
            error instanceof Error ? error.message : String(error);
          log("error", `[ActionRecorder] Tool ${toolName} failed: ${errorMessage}`);

          // Record failed step
          const toolDuration = Date.now() - toolStartTime;
          await this.recordStep(toolName, toolArgs, null, 'failed', toolDuration, errorMessage);

          // Emit step event for failed execution
          await this.emitStepEvent(
            this.currentTurn,
            maxTurns,
            toolName,
            toolArgs,
            null,
            false,
            toolDuration,
            errorMessage
          );

          messages.push({
            role: "tool",
            tool_call_id: toolCall.id,
            content: JSON.stringify({ error: errorMessage }),
          });
        }
      }
    }

    // Max turns reached
    const hasElements = this.discoveredElements.size > 0;
    return this.finalizeResult({
      reason: 'max_turns_reached',
      scenario,
      siteName,
      siteDescription,
      message: hasElements
        ? `Reached max turns (${maxTurns}) but recorded ${this.discoveredElements.size} elements.`
        : `Recording reached maximum turns (${maxTurns}) without completing.`,
    });
  }
}
