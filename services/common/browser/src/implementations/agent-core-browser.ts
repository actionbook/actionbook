/**
 * AgentCoreBrowser - AWS Agent Core Browser Tool implementation
 *
 * Implements BrowserAdapter interface using AWS Bedrock AgentCore
 * Browser Tool for cloud-based browser automation.
 *
 * Benefits:
 * - No local browser installation needed
 * - Auto-scaling and session isolation
 * - Built-in session recording
 * - Enterprise-grade security
 */

import type { BrowserAdapter } from '../adapters/browser-adapter.js';
import type {
  BrowserConfig,
  NavigateOptions,
  ScreenshotOptions,
  WaitForSelectorOptions,
  ScrollDirection,
} from '../types/index.js';
import { log } from '../utils/index.js';

// Dynamic import for optional dependency
let PlaywrightBrowser: any;

/**
 * AgentCoreBrowser configuration
 */
export interface AgentCoreBrowserConfig extends BrowserConfig {
  /** Session timeout in minutes (default: 15, max: 480 for 8 hours) */
  sessionTimeoutMinutes?: number;
  /** AWS region for AgentCore */
  region?: string;
}

/**
 * AgentCoreBrowser - Cloud-based browser using AWS AgentCore
 */
export class AgentCoreBrowser implements BrowserAdapter {
  private client: any = null;
  private initialized: boolean = false;
  private config: AgentCoreBrowserConfig;
  private currentUrl: string = 'about:blank';

  constructor(config: AgentCoreBrowserConfig = {}) {
    this.config = {
      sessionTimeoutMinutes: config.sessionTimeoutMinutes ?? 15,
      region: config.region ?? process.env.AWS_REGION ?? 'us-east-1',
      timeout: config.timeout ?? 60000,
      ...config,
    };
  }

  // ============================================
  // Lifecycle
  // ============================================

  async initialize(): Promise<void> {
    if (this.initialized && this.client) {
      return;
    }

    // Dynamic import of AgentCore Browser SDK
    try {
      const browserModule = await import('bedrock-agentcore/browser/playwright');
      PlaywrightBrowser = browserModule.PlaywrightBrowser;
    } catch {
      throw new Error(
        'bedrock-agentcore is not installed. Install with: npm install bedrock-agentcore'
      );
    }

    log('info', '[AgentCoreBrowser] Initializing AgentCore Browser...');
    log('info', `[AgentCoreBrowser] Region: ${this.config.region}`);
    log(
      'info',
      `[AgentCoreBrowser] Session timeout: ${this.config.sessionTimeoutMinutes} minutes`
    );

    try {
      // Create PlaywrightBrowser instance
      // Session will be created automatically on first method call (e.g., navigate)
      this.client = new PlaywrightBrowser({
        region: this.config.region,
        sessionTimeout: this.config.sessionTimeoutMinutes,
      });

      this.initialized = true;
      log('info', '[AgentCoreBrowser] Browser client created (session will be created on first use)');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log('error', `[AgentCoreBrowser] Failed to create browser client: ${message}`);
      throw new Error(`Failed to initialize AgentCore Browser: ${message}`);
    }
  }

  async close(): Promise<void> {
    if (this.client && this.initialized) {
      log('info', '[AgentCoreBrowser] Stopping session...');
      try {
        await this.client.stopSession();
        log('info', '[AgentCoreBrowser] Session stopped');
      } catch (error) {
        log('warn', `[AgentCoreBrowser] Error stopping session: ${error}`);
      }
      this.client = null;
      this.initialized = false;
      this.currentUrl = 'about:blank';
    }
  }

  // ============================================
  // Navigation
  // ============================================

  async navigate(url: string, options?: NavigateOptions): Promise<void> {
    this.ensureInitialized();

    log('info', `[AgentCoreBrowser] Navigating to: ${url}`);
    try {
      // SDK requires object parameter: { url, timeout?, waitUntil? }
      await this.client.navigate({
        url,
        timeout: options?.timeout ?? this.config.timeout,
        waitUntil: options?.waitUntil ?? 'domcontentloaded',
      });
      this.currentUrl = url;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log('error', `[AgentCoreBrowser] Navigation failed: ${message}`);
      throw error;
    }
  }

  async goBack(): Promise<void> {
    this.ensureInitialized();
    log('info', '[AgentCoreBrowser] Navigating back');
    await this.client.back();
  }

  async goForward(): Promise<void> {
    this.ensureInitialized();
    log('info', '[AgentCoreBrowser] Navigating forward');
    await this.client.forward();
  }

  async reload(): Promise<void> {
    this.ensureInitialized();
    log('info', '[AgentCoreBrowser] Reloading page');
    await this.navigate(this.currentUrl);
  }

  // ============================================
  // Page Information
  // ============================================

  getUrl(): string {
    return this.currentUrl;
  }

  async getTitle(): Promise<string> {
    this.ensureInitialized();
    try {
      // SDK requires object parameter: { script, args? }
      const title = await this.client.evaluate({ script: 'document.title' });
      return title || '';
    } catch {
      return '';
    }
  }

  async getContent(): Promise<string> {
    this.ensureInitialized();
    try {
      const html = await this.client.getHtml();
      return html || '';
    } catch (error) {
      log('warn', `[AgentCoreBrowser] Failed to get content: ${error}`);
      return '';
    }
  }

  // ============================================
  // Screenshot
  // ============================================

  async screenshot(options?: ScreenshotOptions): Promise<Buffer> {
    this.ensureInitialized();

    log('info', '[AgentCoreBrowser] Taking screenshot');
    try {
      const screenshot = await this.client.screenshot({
        fullPage: options?.fullPage ?? false,
        type: options?.format ?? 'png',
      });

      // AgentCore returns base64 string, convert to Buffer
      if (typeof screenshot === 'string') {
        return Buffer.from(screenshot, 'base64');
      }
      return screenshot;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log('error', `[AgentCoreBrowser] Screenshot failed: ${message}`);
      throw error;
    }
  }

  // ============================================
  // Waiting
  // ============================================

  async waitForSelector(
    selector: string,
    options?: WaitForSelectorOptions
  ): Promise<void> {
    this.ensureInitialized();

    log('info', `[AgentCoreBrowser] Waiting for selector: ${selector}`);
    try {
      // SDK requires object parameter: { selector, timeout?, state? }
      await this.client.waitForSelector({
        selector,
        timeout: options?.timeout ?? 30000,
        state: options?.hidden ? 'hidden' : options?.visible ? 'visible' : 'attached',
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log('warn', `[AgentCoreBrowser] waitForSelector failed: ${message}`);
      throw error;
    }
  }

  async waitForNavigation(_timeout?: number): Promise<void> {
    this.ensureInitialized();
    // AgentCore handles navigation waiting internally
    await this.wait(1000);
  }

  async wait(ms: number): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, ms));
  }

  // ============================================
  // Scrolling
  // ============================================

  async scroll(direction: ScrollDirection, amount: number = 300): Promise<void> {
    this.ensureInitialized();

    const delta = direction === 'down' ? amount : -amount;
    try {
      // SDK requires object parameter: { script, args? }
      await this.client.evaluate({
        script: `window.scrollBy(0, ${delta})`,
      });
    } catch (error) {
      log('warn', `[AgentCoreBrowser] Scroll failed: ${error}`);
    }
  }

  async scrollToBottom(waitAfterMs: number = 1000): Promise<void> {
    this.ensureInitialized();

    log('info', '[AgentCoreBrowser] Scrolling to bottom');
    try {
      let lastHeight = 0;
      let attempts = 0;
      const maxAttempts = 10;

      while (attempts < maxAttempts) {
        // SDK requires object parameter: { script, args? }
        const currentHeight = await this.client.evaluate({
          script: 'document.body.scrollHeight',
        });

        await this.client.evaluate({
          script: 'window.scrollTo(0, document.body.scrollHeight)',
        });

        await this.wait(500);

        if (currentHeight === lastHeight) {
          break;
        }

        lastHeight = currentHeight;
        attempts++;
      }

      await this.wait(waitAfterMs);
      log('info', `[AgentCoreBrowser] Scrolled to bottom (${attempts} iterations)`);
    } catch (error) {
      log('warn', `[AgentCoreBrowser] scrollToBottom failed: ${error}`);
    }
  }

  // ============================================
  // Additional AgentCore-specific methods
  // ============================================

  /**
   * Click an element
   */
  async click(selector: string): Promise<void> {
    this.ensureInitialized();
    // SDK requires object parameter: { selector, timeout? }
    await this.client.click({ selector });
  }

  /**
   * Fill a text input
   */
  async fill(selector: string, value: string): Promise<void> {
    this.ensureInitialized();
    // SDK requires object parameter: { selector, value, timeout? }
    await this.client.fill({ selector, value });
  }

  /**
   * Type text (character by character)
   */
  async type(selector: string, text: string): Promise<void> {
    this.ensureInitialized();
    // SDK requires object parameter: { selector, text, delay? }
    await this.client.type({ selector, text });
  }

  /**
   * Get text content of an element
   */
  async getText(selector: string): Promise<string> {
    this.ensureInitialized();
    // SDK requires object parameter: { selector }
    return await this.client.getText({ selector });
  }

  /**
   * Execute JavaScript in the browser
   * SDK requires object parameter: { script: string, args?: any[] }
   */
  async evaluate<T>(fn: () => T): Promise<T>;
  async evaluate<T, A>(fn: (arg: A) => T, arg: A): Promise<T>;
  async evaluate<T, A>(fn: ((arg: A) => T) | (() => T), arg?: A): Promise<T> {
    this.ensureInitialized();
    // Convert function to string for SDK
    const fnString = fn.toString();
    if (arg !== undefined) {
      // Wrap function call with argument
      const script = `(${fnString})(${JSON.stringify(arg)})`;
      return await this.client.evaluate({ script });
    }
    // Wrap function call without argument
    const script = `(${fnString})()`;
    return await this.client.evaluate({ script });
  }

  /**
   * Get session ID (from SDK's internal state)
   */
  getSessionId(): string | null {
    return this.client?._session?.sessionId ?? null;
  }

  // ============================================
  // Private Methods
  // ============================================

  private ensureInitialized(): void {
    if (!this.client || !this.initialized) {
      throw new Error('Browser not initialized. Call initialize() first.');
    }
  }
}
