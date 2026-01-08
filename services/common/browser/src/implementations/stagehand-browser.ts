/**
 * StagehandBrowser - Local Playwright + Stagehand AI implementation
 *
 * Implements AIBrowserAdapter interface with full AI capabilities
 * for element observation and intelligent action execution.
 */

import type { Page, BrowserContext } from 'playwright';
import type { AIBrowserAdapter } from '../adapters/ai-browser-adapter.js';
import type {
  BrowserConfig,
  NavigateOptions,
  ScreenshotOptions,
  WaitForSelectorOptions,
  ScrollDirection,
  ObserveResult,
  ActionObject,
  ElementAttributes,
  TokenStats,
} from '../types/index.js';
import {
  log,
  filterStateDataAttributes,
  createIdSelector,
  generateOptimizedXPath,
  filterCssClasses,
} from '../utils/index.js';

// Dynamic imports for optional dependencies
let Stagehand: any;
let AISdkClient: any;
let BrowserProfileManager: any;

/**
 * Error class for element not found scenarios
 */
export class ElementNotFoundError extends Error {
  constructor(
    message: string,
    public readonly selector?: string
  ) {
    super(message);
    this.name = 'ElementNotFoundError';
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
    this.name = 'ActionExecutionError';
  }
}

/**
 * Stagehand metrics snapshot for tracking LLM usage
 */
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

/**
 * StagehandBrowser - Full-featured browser with AI capabilities
 */
export class StagehandBrowser implements AIBrowserAdapter {
  private stagehand: any = null;
  private page: Page | null = null;
  private config: BrowserConfig;
  private lastMetrics: StagehandMetricsSnapshot | null = null;
  private accumulatedInputTokens: number = 0;
  private accumulatedOutputTokens: number = 0;

  constructor(config: BrowserConfig = {}) {
    this.config = {
      headless: config.headless ?? process.env.HEADLESS === 'true',
      proxy: config.proxy ?? process.env.HTTPS_PROXY ?? process.env.HTTP_PROXY,
      timeout: config.timeout ?? 60000,
      ...config,
    };
  }

  // ============================================
  // Lifecycle
  // ============================================

  async initialize(): Promise<void> {
    if (this.stagehand && this.page) {
      return;
    }

    // Dynamic import of Stagehand
    try {
      const stagehandModule = await import('@browserbasehq/stagehand');
      Stagehand = stagehandModule.Stagehand;
      AISdkClient = stagehandModule.AISdkClient;
    } catch {
      throw new Error(
        'Stagehand is not installed. Install with: npm install @browserbasehq/stagehand'
      );
    }

    // Dynamic import of BrowserProfileManager
    try {
      const profileModule = await import('@actionbookdev/browser-profile');
      BrowserProfileManager = profileModule.BrowserProfileManager;
    } catch {
      log('warn', '[StagehandBrowser] browser-profile not available, profiles disabled');
    }

    log('info', '[StagehandBrowser] Initializing Stagehand...');

    const { modelConfig, llmClient } = await this.buildLLMConfig();
    const localBrowserLaunchOptions = this.buildBrowserLaunchOptions();

    // Create Stagehand instance
    const stagehandOptions: any = {
      env: 'LOCAL',
      localBrowserLaunchOptions,
      verbose: 1,
      logger: this.createStagehandLogger(),
    };

    if (llmClient) {
      stagehandOptions.llmClient = llmClient;
    } else if (modelConfig) {
      stagehandOptions.model = modelConfig;
    }

    this.stagehand = new Stagehand(stagehandOptions);
    await this.stagehand.init();

    // Get page from context
    this.page = this.stagehand.context.pages()[0] as Page;

    // Inject storage state if configured
    await this.injectStorageState();

    // Initialize metrics baseline
    await this.initializeMetricsBaseline();

    log('info', '[StagehandBrowser] Initialized successfully');
  }

  async close(): Promise<void> {
    if (this.stagehand) {
      log('info', '[StagehandBrowser] Closing browser');
      await this.stagehand.close();
      this.stagehand = null;
      this.page = null;
    }
  }

  // ============================================
  // Navigation
  // ============================================

  async navigate(url: string, options?: NavigateOptions): Promise<void> {
    const page = this.getPageOrThrow();
    try {
      await page.goto(url, {
        waitUntil: options?.waitUntil ?? 'domcontentloaded',
        timeout: options?.timeout ?? this.config.timeout,
      });
    } catch (error) {
      // If navigation times out, check if we're still on the page
      const currentUrl = page.url();
      if (currentUrl && currentUrl.includes(new URL(url).hostname)) {
        log('info', `[StagehandBrowser] Page loaded (partial): ${currentUrl}`);
      } else {
        throw error;
      }
    }
    await this.wait(2000);
  }

  async goBack(): Promise<void> {
    const page = this.getPageOrThrow();
    await page.goBack({ waitUntil: 'domcontentloaded', timeout: 30000 });
    await this.wait(1000);
    log('info', `[StagehandBrowser] Navigated back to: ${page.url()}`);
  }

  async goForward(): Promise<void> {
    const page = this.getPageOrThrow();
    await page.goForward({ waitUntil: 'domcontentloaded', timeout: 30000 });
    await this.wait(1000);
  }

  async reload(): Promise<void> {
    const page = this.getPageOrThrow();
    await page.reload({ waitUntil: 'domcontentloaded', timeout: 30000 });
    await this.wait(1000);
  }

  // ============================================
  // Page Information
  // ============================================

  getUrl(): string {
    return this.getPageOrThrow().url();
  }

  async getTitle(): Promise<string> {
    return await this.getPageOrThrow().title();
  }

  async getContent(): Promise<string> {
    return await this.getPageOrThrow().content();
  }

  // ============================================
  // Screenshot
  // ============================================

  async screenshot(options?: ScreenshotOptions): Promise<Buffer> {
    const page = this.getPageOrThrow();
    const format = options?.format === 'webp' ? 'png' : (options?.format ?? 'png');
    return await page.screenshot({
      fullPage: options?.fullPage ?? false,
      type: format,
      quality: format === 'jpeg' ? options?.quality : undefined,
    });
  }

  // ============================================
  // Waiting
  // ============================================

  async waitForSelector(
    selector: string,
    options?: WaitForSelectorOptions
  ): Promise<void> {
    const page = this.getPageOrThrow();
    await page.waitForSelector(selector, {
      timeout: options?.timeout ?? 30000,
      state: options?.hidden ? 'hidden' : options?.visible ? 'visible' : 'attached',
    });
  }

  async waitForNavigation(timeout?: number): Promise<void> {
    const page = this.getPageOrThrow();
    await page.waitForLoadState('domcontentloaded', {
      timeout: timeout ?? 30000,
    });
  }

  async wait(ms: number): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, ms));
  }

  // ============================================
  // Scrolling
  // ============================================

  async scroll(direction: ScrollDirection, amount: number = 300): Promise<void> {
    const page = this.getPageOrThrow();
    const delta = direction === 'down' ? amount : -amount;
    try {
      await page.mouse.wheel(0, delta);
    } catch {
      const key = direction === 'down' ? 'PageDown' : 'PageUp';
      await page.keyboard.press(key);
    }
  }

  async scrollToBottom(waitAfterMs: number = 1000): Promise<void> {
    const page = this.getPageOrThrow();

    try {
      let lastScrollHeight = await page.evaluate(() => document.body.scrollHeight);
      let scrollAttempts = 0;
      const maxAttempts = 10;

      while (scrollAttempts < maxAttempts) {
        await page.evaluate(() => {
          window.scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' });
        });

        await this.wait(500);

        const newScrollHeight = await page.evaluate(() => document.body.scrollHeight);

        if (newScrollHeight === lastScrollHeight) {
          break;
        }

        lastScrollHeight = newScrollHeight;
        scrollAttempts++;
      }

      await this.wait(waitAfterMs);
      log('info', `[StagehandBrowser] Scrolled to bottom (${scrollAttempts} iterations)`);
    } catch (error) {
      log('warn', `[StagehandBrowser] scrollToBottom failed: ${error}`);
      for (let i = 0; i < 5; i++) {
        await page.keyboard.press('End');
        await this.wait(200);
      }
      await this.wait(waitAfterMs);
    }
  }

  // ============================================
  // AI Capabilities
  // ============================================

  async observe(instruction: string, timeoutMs: number = 30000): Promise<ObserveResult[]> {
    if (!this.stagehand) {
      throw new Error('Browser not initialized. Call initialize() first.');
    }

    const startTime = Date.now();
    try {
      const timeoutPromise = new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error(`observe timeout after ${timeoutMs}ms`)),
          timeoutMs
        )
      );

      const result = await Promise.race([
        this.stagehand.observe(instruction),
        timeoutPromise,
      ]);

      await this.logStagehandMetrics('observe', startTime);
      return result;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      log('error', `[StagehandBrowser] observe() failed: ${errorMessage}`);
      throw error;
    }
  }

  async act(instructionOrAction: string | ActionObject): Promise<unknown> {
    if (!this.stagehand) {
      throw new Error('Browser not initialized. Call initialize() first.');
    }

    const startTime = Date.now();
    try {
      const result = await this.stagehand.act(instructionOrAction);
      await this.logStagehandMetrics('act', startTime);
      return result;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);

      if (
        errorMessage.includes('No object generated') ||
        errorMessage.includes('response did not match schema') ||
        errorMessage.includes('Could not find element')
      ) {
        const selector =
          typeof instructionOrAction === 'string'
            ? undefined
            : instructionOrAction.selector;
        throw new ElementNotFoundError(
          'Element not found or action could not be performed.',
          selector
        );
      }

      if (errorMessage.includes('timeout') || errorMessage.includes('Timeout')) {
        throw new ActionExecutionError(
          'Action timed out. The element may be loading or not interactive.',
          typeof instructionOrAction === 'string'
            ? instructionOrAction
            : instructionOrAction.method,
          error instanceof Error ? error : undefined
        );
      }

      throw new ActionExecutionError(
        `Failed to execute action: ${errorMessage}`,
        typeof instructionOrAction === 'string'
          ? instructionOrAction
          : instructionOrAction.method,
        error instanceof Error ? error : undefined
      );
    }
  }

  async actWithSelector(action: ActionObject): Promise<unknown> {
    log(
      'info',
      `[StagehandBrowser] Acting with selector: ${action.method} on ${action.selector}`
    );
    return this.act(action);
  }

  // ============================================
  // Element Inspection
  // ============================================

  async getElementAttributes(xpathSelector: string): Promise<ElementAttributes | null> {
    const page = this.getPageOrThrow();

    try {
      // Check if XPath contains iframe reference
      const iframeMatch = xpathSelector.match(
        /^(.+?\/iframe\[\d+\])\/html\[\d+\]\/body\[\d+\](.*)$/i
      );

      let attrs: any = null;

      if (iframeMatch) {
        // Use frames() API for iframe elements
        const elementPath = `/html[1]/body[1]${iframeMatch[2]}`;
        const frames = page.frames();

        for (let i = 1; i < frames.length; i++) {
          try {
            attrs = await frames[i].evaluate(this.extractElementAttributes, elementPath);
            if (attrs) break;
          } catch {
            continue;
          }
        }
      } else {
        attrs = await page.evaluate(this.extractElementAttributes, xpathSelector);
      }

      if (!attrs) {
        log('warn', `[StagehandBrowser] Element not found: ${xpathSelector}`);
        return null;
      }

      // Filter state attributes
      attrs.dataAttributes = filterStateDataAttributes(attrs.dataAttributes);

      // Build CSS selector
      const cssSelector = this.buildCssSelector(attrs);

      // Generate optimized XPath
      const optimizedXPathResult = generateOptimizedXPath(attrs, xpathSelector);

      return {
        tagName: attrs.tagName,
        id: attrs.id,
        className: attrs.className,
        dataTestId: attrs.dataTestId,
        ariaLabel: attrs.ariaLabel,
        placeholder: attrs.placeholder,
        name: attrs.name,
        textContent: attrs.textContent,
        dataAttributes: attrs.dataAttributes,
        cssSelector,
        optimizedXPath: optimizedXPathResult.xpath,
      };
    } catch (error) {
      log('warn', `[StagehandBrowser] Failed to get element attributes: ${error}`);
      return null;
    }
  }

  /**
   * Alias for getElementAttributes (backward compatibility)
   * @deprecated Use getElementAttributes instead
   */
  async getElementAttributesFromXPath(xpath: string): Promise<ElementAttributes | null> {
    return this.getElementAttributes(xpath);
  }

  /**
   * Get the underlying Playwright Page instance
   */
  async getPage(): Promise<Page> {
    return this.getPageOrThrow();
  }

  /**
   * Wait for text to appear on the page
   */
  async waitForText(text: string, timeout: number = 30000): Promise<void> {
    const page = this.getPageOrThrow();
    await page.getByText(text, { exact: false }).first().waitFor({
      state: 'visible',
      timeout,
    });
  }

  // ============================================
  // Automation Helpers
  // ============================================

  async autoClosePopups(): Promise<number> {
    if (!this.stagehand) {
      return 0;
    }

    let closedCount = 0;
    const popupInstructions = [
      'click the close button on any popup or modal',
      'click accept or dismiss on cookie consent banner',
      'click close on any overlay dialog',
    ];

    for (const instruction of popupInstructions) {
      try {
        const actions = await this.stagehand.observe(instruction);
        if (actions.length > 0) {
          await this.stagehand.act(actions[0]);
          closedCount++;
          log('info', `[StagehandBrowser] Closed popup with: ${instruction}`);
          await this.wait(500);
        }
      } catch {
        // Ignore - no popup found
      }
    }

    if (closedCount > 0) {
      log('info', `[StagehandBrowser] Total popups closed: ${closedCount}`);
    }

    return closedCount;
  }

  // ============================================
  // Metrics
  // ============================================

  getTokenStats(): TokenStats {
    return {
      input: this.accumulatedInputTokens,
      output: this.accumulatedOutputTokens,
      total: this.accumulatedInputTokens + this.accumulatedOutputTokens,
    };
  }

  // ============================================
  // Private Methods
  // ============================================

  private getPageOrThrow(): Page {
    if (!this.page) {
      throw new Error('Browser not initialized. Call initialize() first.');
    }
    return this.page;
  }

  private async buildLLMConfig(): Promise<{
    modelConfig?: any;
    llmClient?: any;
  }> {
    const openrouterKey = process.env.OPENROUTER_API_KEY;
    const openaiKey = process.env.OPENAI_API_KEY;
    const anthropicKey = process.env.ANTHROPIC_API_KEY;
    const bedrockAccessKey = process.env.AWS_ACCESS_KEY_ID;
    const bedrockSecretKey = process.env.AWS_SECRET_ACCESS_KEY;
    const stagehandModel = process.env.STAGEHAND_MODEL;
    const hasProxy = !!this.config.proxy;

    if (openrouterKey) {
      const model = stagehandModel || 'gpt-4o';
      return {
        modelConfig: {
          modelName: model,
          apiKey: openrouterKey,
          baseURL: 'https://openrouter.ai/api/v1',
        },
      };
    }

    if (openaiKey) {
      const model = stagehandModel || 'gpt-4o';
      if (hasProxy) {
        return {
          modelConfig: {
            modelName: model,
            apiKey: openaiKey,
            baseURL: process.env.OPENAI_BASE_URL || 'https://api.openai.com/v1',
          },
        };
      }
      return { modelConfig: model };
    }

    if (bedrockAccessKey && bedrockSecretKey) {
      const { createAmazonBedrock } = await import('@ai-sdk/amazon-bedrock');
      const region = process.env.AWS_REGION || 'us-east-1';
      const bedrockModel =
        stagehandModel || 'anthropic.claude-3-5-sonnet-20241022-v2:0';

      const bedrock = createAmazonBedrock({
        region,
        accessKeyId: bedrockAccessKey,
        secretAccessKey: bedrockSecretKey,
        sessionToken: process.env.AWS_SESSION_TOKEN,
      });

      return {
        llmClient: new AISdkClient({
          model: bedrock(bedrockModel),
        }),
      };
    }

    if (anthropicKey) {
      if (hasProxy) {
        throw new Error(
          'Anthropic SDK does not support HTTP proxy. Use OPENROUTER_API_KEY or AWS Bedrock instead.'
        );
      }
      return { modelConfig: stagehandModel || 'claude-sonnet-4-20250514' };
    }

    throw new Error(
      'No LLM API key found. Set OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY, or AWS credentials.'
    );
  }

  private buildBrowserLaunchOptions(): Record<string, any> {
    const options: Record<string, any> = {
      headless: this.config.headless,
    };

    if (this.config.proxy) {
      options.proxy = { server: this.config.proxy };
      log('info', `[StagehandBrowser] Using proxy: ${this.config.proxy}`);
    }

    if (this.config.profile?.enabled && BrowserProfileManager) {
      const profileDir = this.config.profile.profileDir || '.browser-profile';
      const profileManager = new BrowserProfileManager({ baseDir: profileDir });
      const profilePath = profileManager.getProfilePath();

      profileManager.ensureDir();
      profileManager.cleanupStaleLocks?.();

      options.userDataDir = profilePath;
      options.preserveUserDataDir = true;
      options.args = [
        '--disable-blink-features=AutomationControlled',
        '--no-first-run',
      ];
      options.ignoreDefaultArgs = ['--enable-automation'];

      log('info', `[StagehandBrowser] Using profile: ${profilePath}`);
    }

    return options;
  }

  private createStagehandLogger() {
    return (logLine: {
      message: string;
      level?: number;
      auxiliary?: Record<string, unknown>;
    }) => {
      const level = logLine.level === 0 ? 'error' : logLine.level === 2 ? 'debug' : 'info';
      let auxStr = '';
      if (logLine.auxiliary && Object.keys(logLine.auxiliary).length > 0) {
        auxStr =
          '\n    ' +
          Object.entries(logLine.auxiliary)
            .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
            .join('\n    ');
      }
      log(level as any, `[Stagehand] ${logLine.message}${auxStr}`);
    };
  }

  private async injectStorageState(): Promise<void> {
    if (!this.config.storageStatePath || !this.stagehand) return;

    try {
      const fs = await import('fs');
      if (fs.existsSync(this.config.storageStatePath)) {
        const stateData = JSON.parse(
          fs.readFileSync(this.config.storageStatePath, 'utf-8')
        );
        const context = this.stagehand.context as BrowserContext;

        if (stateData.cookies?.length) {
          await context.addCookies(stateData.cookies);
          log(
            'info',
            `[StagehandBrowser] Injected ${stateData.cookies.length} cookies`
          );
        }

        if (stateData.origins?.length) {
          await context.addInitScript((storageState: any) => {
            if (window.location.href === 'about:blank') return;
            const originState = storageState.origins.find(
              (o: any) => o.origin === window.location.origin
            );
            if (originState?.localStorage) {
              for (const { name, value } of originState.localStorage) {
                window.localStorage.setItem(name, value);
              }
            }
          }, stateData);
          log(
            'info',
            `[StagehandBrowser] Injected localStorage for ${stateData.origins.length} origins`
          );
        }
      }
    } catch (error) {
      log('error', `[StagehandBrowser] Failed to inject storage state: ${error}`);
    }
  }

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
    } catch {
      this.lastMetrics = {
        observePromptTokens: 0,
        observeCompletionTokens: 0,
        observeReasoningTokens: 0,
        observeCachedInputTokens: 0,
        observeInferenceTimeMs: 0,
        actPromptTokens: 0,
        actCompletionTokens: 0,
        actReasoningTokens: 0,
        actCachedInputTokens: 0,
        actInferenceTimeMs: 0,
      };
    }
  }

  private async logStagehandMetrics(
    operation: 'observe' | 'act',
    startTime: number
  ): Promise<void> {
    if (!this.stagehand) return;

    try {
      const metrics = await this.stagehand.metrics;
      const e2eLatencyMs = Date.now() - startTime;
      const prev = this.lastMetrics || {
        observePromptTokens: 0,
        observeCompletionTokens: 0,
        observeReasoningTokens: 0,
        observeCachedInputTokens: 0,
        observeInferenceTimeMs: 0,
        actPromptTokens: 0,
        actCompletionTokens: 0,
        actReasoningTokens: 0,
        actCachedInputTokens: 0,
        actInferenceTimeMs: 0,
      };

      let inputTokens: number, outputTokens: number;

      if (operation === 'observe') {
        inputTokens = metrics.observePromptTokens - prev.observePromptTokens;
        outputTokens = metrics.observeCompletionTokens - prev.observeCompletionTokens;
      } else {
        inputTokens = metrics.actPromptTokens - prev.actPromptTokens;
        outputTokens = metrics.actCompletionTokens - prev.actCompletionTokens;
      }

      // Update metrics snapshot
      this.lastMetrics = { ...metrics };

      if (inputTokens > 0 || outputTokens > 0) {
        this.accumulatedInputTokens += inputTokens;
        this.accumulatedOutputTokens += outputTokens;

        const totalTokens = inputTokens + outputTokens;
        log(
          'info',
          `[LLM] stagehand/${operation} | tokens: in=${inputTokens}, out=${outputTokens}, total=${totalTokens} | latency=${e2eLatencyMs}ms`
        );
      }
    } catch {
      // Ignore metrics errors
    }
  }

  private extractElementAttributes = (xpathStr: string) => {
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
      if (attr.name.startsWith('data-')) {
        dataAttributes[attr.name] = attr.value;
      }
    }

    const rawText = el.textContent?.trim() || '';
    const textContent = rawText.length > 50 ? rawText.substring(0, 50) : rawText;

    return {
      tagName: el.tagName.toLowerCase(),
      id: el.id || undefined,
      className: (el as HTMLElement).className || undefined,
      placeholder: (el as HTMLInputElement).placeholder || undefined,
      ariaLabel: el.getAttribute('aria-label') || undefined,
      dataTestId: el.getAttribute('data-testid') || undefined,
      dataAttributes: Object.keys(dataAttributes).length > 0 ? dataAttributes : undefined,
      name: el.getAttribute('name') || undefined,
      textContent: textContent || undefined,
    };
  };

  private buildCssSelector(attrs: any): string | undefined {
    if (attrs.dataTestId) {
      return `[data-testid="${attrs.dataTestId}"]`;
    }

    if (attrs.dataAttributes) {
      const preferredAttrs = [
        'data-id',
        'data-component',
        'data-element',
        'data-action',
        'data-section',
        'data-name',
      ];
      const dataKeys = Object.keys(attrs.dataAttributes);
      const preferredKey = preferredAttrs.find((k) => dataKeys.includes(k)) || dataKeys[0];
      if (preferredKey) {
        return `[${preferredKey}="${attrs.dataAttributes[preferredKey]}"]`;
      }
    }

    if (attrs.id) {
      return createIdSelector(attrs.id);
    }

    if (attrs.ariaLabel) {
      return `[aria-label="${attrs.ariaLabel}"]`;
    }

    if (attrs.placeholder) {
      return `${attrs.tagName}[placeholder="${attrs.placeholder}"]`;
    }

    if (attrs.className) {
      const classes = filterCssClasses(attrs.className);
      if (classes.length > 0) {
        const bemClass = classes.find((c) => c.includes('__') || c.includes('--'));
        return `${attrs.tagName}.${bemClass || classes[0]}`;
      }
    }

    return undefined;
  }
}
