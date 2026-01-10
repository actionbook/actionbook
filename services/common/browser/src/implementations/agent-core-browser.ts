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

import type { Page } from 'playwright';
import type { BrowserAdapter } from '../adapters/browser-adapter.js';
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
import { log } from '../utils/index.js';
import {
  BedrockRuntimeClient,
  InvokeModelCommand,
} from '@aws-sdk/client-bedrock-runtime';

// Static import for AgentCore PlaywrightBrowser (external in tsup config)
import { PlaywrightBrowser } from 'bedrock-agentcore/browser/playwright';

/**
 * PlaywrightBrowser instance type
 */
type AgentCoreBrowserClient = InstanceType<typeof PlaywrightBrowser>;

/**
 * Raw observe response item from AI
 */
interface RawObserveItem {
  selector?: string;
  description?: string;
  method?: string;
  arguments?: string[];
}

/** Default Bedrock Claude model for observe() - using cross-region inference profile */
const DEFAULT_BEDROCK_MODEL = 'us.anthropic.claude-3-5-sonnet-20241022-v2:0';

/**
 * AgentCoreBrowser configuration
 */
export interface AgentCoreBrowserConfig extends BrowserConfig {
  /** Session timeout in minutes (default: 15, max: 480 for 8 hours) */
  sessionTimeoutMinutes?: number;
  /** AWS region for AgentCore */
  region?: string;
  /** Bedrock model ID for AI operations (default: claude-3-5-sonnet) */
  bedrockModelId?: string;
}

/**
 * AgentCoreBrowser - Cloud-based browser using AWS AgentCore
 */
export class AgentCoreBrowser implements BrowserAdapter {
  private client: AgentCoreBrowserClient | null = null;
  private bedrockClient: BedrockRuntimeClient | null = null;
  private initialized: boolean = false;
  private config: AgentCoreBrowserConfig;
  private currentUrl: string = 'about:blank';

  constructor(config: AgentCoreBrowserConfig = {}) {
    this.config = {
      sessionTimeoutMinutes: config.sessionTimeoutMinutes ?? 15,
      region: config.region ?? process.env.AWS_REGION ?? 'us-east-1',
      timeout: config.timeout ?? 60000,
      bedrockModelId:
        config.bedrockModelId ??
        process.env.AGENTCORE_BROWSER_MODEL_ID ??
        DEFAULT_BEDROCK_MODEL,
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

    log('info', '[AgentCoreBrowser] Initializing AgentCore Browser...');
    log('info', `[AgentCoreBrowser] Region: ${this.config.region}`);
    log(
      'info',
      `[AgentCoreBrowser] Session timeout: ${this.config.sessionTimeoutMinutes} minutes`
    );

    try {
      // Create PlaywrightBrowser instance
      // Note: session timeout is set when calling navigate/startSession, not in constructor
      this.client = new PlaywrightBrowser({
        region: this.config.region,
      });

      // Initialize Bedrock client for observe() AI capability
      this.bedrockClient = new BedrockRuntimeClient({
        region: this.config.region,
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
      this.bedrockClient = null;
      this.initialized = false;
      this.currentUrl = 'about:blank';
    }
  }

  // ============================================
  // Navigation
  // ============================================

  async navigate(url: string, options?: NavigateOptions): Promise<void> {
    const client = this.getClient();

    log('info', `[AgentCoreBrowser] Navigating to: ${url}`);
    try {
      // SDK requires object parameter: { url, timeout?, waitUntil? }
      await client.navigate({
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
    const client = this.getClient();
    log('info', '[AgentCoreBrowser] Navigating back');
    await client.back();
  }

  // ============================================
  // Page Information
  // ============================================

  getUrl(): string {
    return this.currentUrl;
  }

  async getTitle(): Promise<string> {
    const client = this.getClient();
    try {
      // SDK requires object parameter: { script, args? }
      const title = await client.evaluate({ script: 'document.title' });
      return title || '';
    } catch {
      return '';
    }
  }

  async getContent(): Promise<string> {
    const client = this.getClient();
    try {
      const html = await client.getHtml();
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
    const client = this.getClient();

    log('info', '[AgentCoreBrowser] Taking screenshot');
    try {
      // AgentCore SDK only supports 'png' | 'jpeg', default to 'png' for 'webp'
      const format = options?.format === 'webp' ? 'png' : (options?.format ?? 'png');
      const screenshot = await client.screenshot({
        fullPage: options?.fullPage ?? false,
        type: format,
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
    const client = this.getClient();

    log('info', `[AgentCoreBrowser] Waiting for selector: ${selector}`);
    try {
      // SDK requires object parameter: { selector, timeout?, state? }
      await client.waitForSelector({
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

  async wait(ms: number): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, ms));
  }

  // ============================================
  // Scrolling
  // ============================================

  async scroll(direction: ScrollDirection, amount: number = 300): Promise<void> {
    const client = this.getClient();

    const delta = direction === 'down' ? amount : -amount;
    try {
      // SDK requires object parameter: { script, args? }
      await client.evaluate({
        script: `window.scrollBy(0, ${delta})`,
      });
    } catch (error) {
      log('warn', `[AgentCoreBrowser] Scroll failed: ${error}`);
    }
  }

  async scrollToBottom(waitAfterMs: number = 1000): Promise<void> {
    const client = this.getClient();

    log('info', '[AgentCoreBrowser] Scrolling to bottom');
    try {
      let lastHeight = 0;
      let attempts = 0;
      const maxAttempts = 10;

      while (attempts < maxAttempts) {
        // SDK requires object parameter: { script, args? }
        const currentHeight = await client.evaluate({
          script: 'document.body.scrollHeight',
        });

        await client.evaluate({
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
    const client = this.getClient();
    // SDK requires object parameter: { selector, timeout? }
    await client.click({ selector });
  }

  /**
   * Fill a text input
   */
  async fill(selector: string, value: string): Promise<void> {
    const client = this.getClient();
    // SDK requires object parameter: { selector, value, timeout? }
    await client.fill({ selector, value });
  }

  /**
   * Type text (character by character)
   */
  async type(selector: string, text: string): Promise<void> {
    const client = this.getClient();
    // SDK requires object parameter: { selector, text, delay? }
    await client.type({ selector, text });
  }

  /**
   * Get text content of an element
   */
  async getText(selector: string): Promise<string> {
    const client = this.getClient();
    // SDK requires object parameter: { selector }
    return await client.getText({ selector });
  }

  /**
   * Execute JavaScript in the browser
   * SDK requires object parameter: { script: string, args?: unknown[] }
   */
  async evaluate<T>(fn: () => T): Promise<T>;
  async evaluate<T, A>(fn: (arg: A) => T, arg: A): Promise<T>;
  async evaluate<T, A>(fn: ((arg: A) => T) | (() => T), arg?: A): Promise<T> {
    const client = this.getClient();
    // Convert function to string for SDK
    const fnString = fn.toString();
    if (arg !== undefined) {
      // Wrap function call with argument
      const script = `(${fnString})(${JSON.stringify(arg)})`;
      return await client.evaluate({ script });
    }
    // Wrap function call without argument
    const script = `(${fnString})()`;
    return await client.evaluate({ script });
  }

  /**
   * Get session ID (from SDK's internal state)
   * Note: This uses internal SDK state which may not be publicly accessible
   */
  getSessionId(): string | null {
    // Type assertion needed because _session is protected in the SDK
    const clientWithSession = this.client as unknown as { _session?: { sessionId?: string } };
    return clientWithSession?._session?.sessionId ?? null;
  }

  // ============================================
  // AI Capabilities (via Bedrock Claude)
  // ============================================

  /**
   * Observe page elements using AI vision
   *
   * Takes a screenshot and uses Bedrock Claude to identify elements
   * matching the given instruction.
   *
   * @param instruction - Natural language description of elements to find
   * @param timeoutMs - Timeout in milliseconds (default: 30000)
   * @returns Array of observed elements with selectors
   */
  async observe(
    instruction: string,
    timeoutMs: number = 30000
  ): Promise<ObserveResult[]> {
    this.ensureInitialized();

    if (!this.bedrockClient) {
      throw new Error('Bedrock client not initialized');
    }

    log('info', `[AgentCoreBrowser] Observing page: ${instruction}`);
    const startTime = Date.now();

    try {
      // Take screenshot for visual analysis
      const screenshot = await this.screenshot();
      const base64Image = screenshot.toString('base64');

      // Build prompt for Claude
      const prompt = this.buildObservePrompt(instruction);

      // Call Bedrock Claude with timeout
      const timeoutPromise = new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error(`observe timeout after ${timeoutMs}ms`)),
          timeoutMs
        )
      );

      const invokePromise = this.bedrockClient.send(
        new InvokeModelCommand({
          modelId: this.config.bedrockModelId!,
          contentType: 'application/json',
          body: JSON.stringify({
            anthropic_version: 'bedrock-2023-05-31',
            max_tokens: 4096,
            messages: [
              {
                role: 'user',
                content: [
                  {
                    type: 'image',
                    source: {
                      type: 'base64',
                      media_type: 'image/png',
                      data: base64Image,
                    },
                  },
                  {
                    type: 'text',
                    text: prompt,
                  },
                ],
              },
            ],
          }),
        })
      );

      const response = await Promise.race([invokePromise, timeoutPromise]);

      // Parse response
      const responseBody = JSON.parse(new TextDecoder().decode(response.body));
      const content = responseBody.content?.[0]?.text || '';

      const results = this.parseObserveResponse(content);

      const duration = Date.now() - startTime;
      log(
        'info',
        `[AgentCoreBrowser] Observe completed: ${results.length} elements found in ${duration}ms`
      );

      return results;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log('error', `[AgentCoreBrowser] observe() failed: ${message}`);
      throw error;
    }
  }

  /**
   * Execute an action using AI or direct selector
   *
   * @param instructionOrAction - Natural language instruction or ActionObject
   */
  async act(instructionOrAction: string | ActionObject): Promise<unknown> {
    this.ensureInitialized();

    // If it's an ActionObject, use actWithSelector
    if (typeof instructionOrAction === 'object') {
      return this.actWithSelector(instructionOrAction);
    }

    // Natural language instruction - use observe to find element then act
    log('info', `[AgentCoreBrowser] Acting on instruction: ${instructionOrAction}`);
    const elements = await this.observe(instructionOrAction);

    if (elements.length === 0) {
      throw new Error(`No elements found matching: ${instructionOrAction}`);
    }

    // Execute action on the first matching element
    const element = elements[0];
    return this.actWithSelector({
      selector: element.selector,
      method: (element.method as any) || 'click',
      description: element.description,
      arguments: element.arguments,
    });
  }

  /**
   * Execute an action using a predefined selector
   */
  async actWithSelector(action: ActionObject): Promise<unknown> {
    const client = this.getClient();

    const { selector, method, arguments: args } = action;
    log('info', `[AgentCoreBrowser] Acting with selector: ${selector} (${method})`);

    try {
      switch (method) {
        case 'click':
          await this.click(selector);
          return { success: true, action: 'click' };

        case 'type':
        case 'fill':
          if (!args || args.length === 0) {
            throw new Error(`${method} action requires text argument`);
          }
          if (method === 'fill') {
            await this.fill(selector, args[0]);
          } else {
            await this.type(selector, args[0]);
          }
          return { success: true, action: method, text: args[0] };

        case 'hover':
          // Hover via JavaScript
          await client.evaluate({
            script: `document.evaluate("${selector.replace('xpath=', '')}", document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))`,
          });
          return { success: true, action: 'hover' };

        case 'select':
          if (!args || args.length === 0) {
            throw new Error('select action requires value argument');
          }
          await client.evaluate({
            script: `(() => {
              const el = document.evaluate("${selector.replace('xpath=', '')}", document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue;
              if (el) { el.value = "${args[0]}"; el.dispatchEvent(new Event('change', { bubbles: true })); }
            })()`,
          });
          return { success: true, action: 'select', value: args[0] };

        case 'scroll':
          await this.scroll('down', 300);
          return { success: true, action: 'scroll' };

        case 'wait': {
          const waitMs = args?.[0] ? parseInt(args[0], 10) : 1000;
          await this.wait(waitMs);
          return { success: true, action: 'wait', ms: waitMs };
        }

        case 'press':
          if (!args || args.length === 0) {
            throw new Error('press action requires key argument');
          }
          await client.pressKey(args[0]);
          return { success: true, action: 'press', key: args[0] };

        default:
          throw new Error(`Unsupported action method: ${method}`);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log('error', `[AgentCoreBrowser] actWithSelector failed: ${message}`);
      throw error;
    }
  }

  /**
   * Extract attributes from an element by XPath
   */
  async getElementAttributesFromXPath(xpath: string): Promise<ElementAttributes | null> {
    const client = this.getClient();

    try {
      // Normalize xpath (remove 'xpath=' prefix if present)
      const normalizedXpath = xpath.replace(/^xpath=/, '');

      const attrs = await client.evaluate({
        script: `(() => {
          const el = document.evaluate("${normalizedXpath}", document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue;
          if (!el) return null;

          // Collect data-* attributes
          const dataAttributes = {};
          for (const attr of el.attributes) {
            if (attr.name.startsWith('data-')) {
              dataAttributes[attr.name] = attr.value;
            }
          }

          return {
            tagName: el.tagName.toLowerCase(),
            id: el.id || undefined,
            className: el.className || undefined,
            dataTestId: el.getAttribute('data-testid') || undefined,
            ariaLabel: el.getAttribute('aria-label') || undefined,
            placeholder: el.getAttribute('placeholder') || undefined,
            name: el.getAttribute('name') || undefined,
            textContent: (el.textContent || '').trim().substring(0, 100),
            dataAttributes: Object.keys(dataAttributes).length > 0 ? dataAttributes : undefined,
          };
        })()`,
      });

      return attrs || null;
    } catch (error) {
      log('warn', `[AgentCoreBrowser] getElementAttributesFromXPath failed: ${error}`);
      return null;
    }
  }

  /**
   * Get the underlying Playwright Page instance
   * Note: This uses internal SDK state which may not be publicly accessible
   */
  async getPage(): Promise<Page> {
    this.ensureInitialized();
    // Type assertion needed because _playwrightPage is private in the SDK
    const clientWithPage = this.client as unknown as { _playwrightPage?: Page };
    const page = clientWithPage._playwrightPage;
    if (!page) {
      throw new Error('Playwright Page not available');
    }
    return page;
  }

  /**
   * Wait for text to appear on the page
   */
  async waitForText(text: string, timeout: number = 30000): Promise<void> {
    this.ensureInitialized();

    log('info', `[AgentCoreBrowser] Waiting for text: "${text}"`);
    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      try {
        const content = await this.getContent();
        if (content.includes(text)) {
          log('info', `[AgentCoreBrowser] Text found: "${text}"`);
          return;
        }
      } catch {
        // Ignore errors during polling
      }
      await this.wait(500);
    }

    throw new Error(`Timeout waiting for text: "${text}"`);
  }

  /**
   * Auto-detect and close popups/overlays
   */
  async autoClosePopups(): Promise<number> {
    this.ensureInitialized();

    let closedCount = 0;

    // Use same prompts as StagehandBrowser for consistency
    const popupInstructions = [
      'click the close button on any popup or modal',
      'click accept or dismiss on cookie consent banner',
      'click close on any overlay dialog',
    ];

    for (const instruction of popupInstructions) {
      try {
        const elements = await this.observe(instruction);
        if (elements.length > 0) {
          await this.actWithSelector({
            selector: elements[0].selector,
            method: 'click',
            description: elements[0].description,
          });
          closedCount++;
          log('info', `[AgentCoreBrowser] Closed popup with: ${instruction}`);
          await this.wait(500);
        }
      } catch {
        // Ignore errors - popup might not exist
      }
    }

    if (closedCount > 0) {
      log('info', `[AgentCoreBrowser] Total popups closed: ${closedCount}`);
    }

    return closedCount;
  }

  /**
   * Get accumulated token usage statistics
   * AgentCoreBrowser doesn't track tokens (Bedrock billing is separate)
   */
  getTokenStats(): TokenStats | undefined {
    return undefined;
  }

  // ============================================
  // Private Methods
  // ============================================

  private ensureInitialized(): void {
    if (!this.client || !this.initialized) {
      throw new Error('Browser not initialized. Call initialize() first.');
    }
  }

  /**
   * Get the client with null check (throws if not initialized)
   */
  private getClient(): AgentCoreBrowserClient {
    if (!this.client || !this.initialized) {
      throw new Error('Browser not initialized. Call initialize() first.');
    }
    return this.client;
  }

  /**
   * Build the prompt for observe() AI analysis
   * Inspired by Stagehand's observe prompt but adapted for screenshot-based observation
   * Returns format matching ObserveResult: { selector, description, method, arguments }
   */
  private buildObservePrompt(instruction: string): string {
    return `You are helping the user automate the browser by finding elements based on what the user wants to observe in the page.

You will be given:
1. An instruction describing elements to find
2. A screenshot of the current webpage

Your task: Find elements that EXACTLY match the instruction. If the requested elements do not exist on the page, return an empty array [].

Instruction: "${instruction}"

CRITICAL RULES:
- ONLY return elements that actually exist and match the instruction
- If no matching elements exist (e.g., no popup, no cookie banner, no modal), return []
- Do NOT invent or guess elements that are not clearly visible
- Return ONLY a valid JSON array, no other text

Response format (if elements found):
[{"selector": "xpath=//button[@id='close']", "description": "Close button", "method": "click"}]

Response format (if NO elements found):
[]`;
  }

  /**
   * Parse Claude's response into ObserveResult array
   */
  private parseObserveResponse(content: string): ObserveResult[] {
    try {
      // Try to extract JSON from the response
      // Claude might wrap it in markdown code blocks
      let jsonStr = content.trim();

      // Remove markdown code blocks if present
      const jsonMatch = jsonStr.match(/```(?:json)?\s*([\s\S]*?)```/);
      if (jsonMatch) {
        jsonStr = jsonMatch[1].trim();
      }

      // Find the JSON array in the response
      const arrayMatch = jsonStr.match(/\[[\s\S]*\]/);
      if (!arrayMatch) {
        log('warn', '[AgentCoreBrowser] No JSON array found in response');
        return [];
      }

      const parsed = JSON.parse(arrayMatch[0]);

      if (!Array.isArray(parsed)) {
        log('warn', '[AgentCoreBrowser] Parsed result is not an array');
        return [];
      }

      // Validate and normalize each result
      return (parsed as RawObserveItem[])
        .filter(
          (item): item is RawObserveItem & { selector: string; description: string } =>
            !!item &&
            typeof item.selector === 'string' &&
            typeof item.description === 'string'
        )
        .map((item) => ({
          selector: item.selector,
          description: item.description,
          method: item.method || 'click',
          arguments: item.arguments,
        }));
    } catch (error) {
      log(
        'warn',
        `[AgentCoreBrowser] Failed to parse observe response: ${error}`
      );
      return [];
    }
  }
}
