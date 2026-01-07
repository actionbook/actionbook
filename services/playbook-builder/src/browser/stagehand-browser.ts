/**
 * StagehandBrowser - Wrapper for Stagehand browser automation
 */

import { Stagehand } from '@browserbasehq/stagehand';
import type { Page } from 'playwright';
import { BrowserProfileManager } from '@actionbookdev/browser-profile';
import { log } from '../utils/logger.js';

export interface StagehandBrowserConfig {
  /** Whether to run in headless mode */
  headless?: boolean;
  /** Proxy server URL */
  proxy?: string;
  /** Base directory for browser profile */
  profileDir?: string;
}

/**
 * StagehandBrowser - Manages Stagehand instance with persistent profile
 */
export class StagehandBrowser {
  private stagehand: Stagehand | null = null;
  private profileManager: BrowserProfileManager;
  private config: StagehandBrowserConfig;

  constructor(config: StagehandBrowserConfig = {}) {
    this.config = {
      headless: config.headless ?? (process.env.HEADLESS === 'true'),
      proxy: config.proxy ?? process.env.HTTPS_PROXY ?? process.env.HTTP_PROXY,
      profileDir: config.profileDir ?? '.browser-profile',
    };
    this.profileManager = new BrowserProfileManager({ baseDir: this.config.profileDir });
  }

  /**
   * Initialize the browser
   */
  async init(): Promise<void> {
    if (this.stagehand) {
      log('warn', '[StagehandBrowser] Already initialized');
      return;
    }

    // Ensure profile directory exists
    this.profileManager.ensureDir();

    // Build launch options
    const launchOptions = this.profileManager.getLaunchOptions({
      headless: this.config.headless,
      proxy: this.config.proxy ? { server: this.config.proxy } : undefined,
    });

    log('info', `[StagehandBrowser] Initializing with headless=${this.config.headless}`);

    this.stagehand = new Stagehand({
      env: 'LOCAL',
      localBrowserLaunchOptions: launchOptions,
      verbose: 0,
    });

    await this.stagehand.init();
    log('info', '[StagehandBrowser] Initialized successfully');
  }

  /**
   * Get the current page
   */
  getPage(): Page {
    if (!this.stagehand) {
      throw new Error('Browser not initialized. Call init() first.');
    }
    return this.stagehand.page;
  }

  /**
   * Get all pages
   */
  getPages(): Page[] {
    if (!this.stagehand) {
      throw new Error('Browser not initialized. Call init() first.');
    }
    return this.stagehand.context.pages();
  }

  /**
   * Navigate to a URL
   */
  async goto(url: string, options?: { timeout?: number; waitUntil?: 'load' | 'domcontentloaded' | 'networkidle' }): Promise<void> {
    const page = this.getPage();
    log('info', `[StagehandBrowser] Navigating to ${url}`);
    await page.goto(url, {
      timeout: options?.timeout ?? 60000,
      waitUntil: options?.waitUntil ?? 'domcontentloaded',
    });
  }

  /**
   * Take a screenshot
   */
  async screenshot(options?: { fullPage?: boolean }): Promise<Buffer> {
    const page = this.getPage();
    return await page.screenshot({
      fullPage: options?.fullPage ?? false,
      type: 'png',
    });
  }

  /**
   * Get page content (HTML)
   */
  async getContent(): Promise<string> {
    const page = this.getPage();
    return await page.content();
  }

  /**
   * Get page title
   */
  async getTitle(): Promise<string> {
    const page = this.getPage();
    return await page.title();
  }

  /**
   * Get current URL
   */
  getUrl(): string {
    const page = this.getPage();
    return page.url();
  }

  /**
   * Wait for a selector
   */
  async waitForSelector(selector: string, options?: { timeout?: number }): Promise<void> {
    const page = this.getPage();
    await page.waitForSelector(selector, {
      timeout: options?.timeout ?? 30000,
    });
  }

  /**
   * Check if profile exists (for login state)
   */
  hasProfile(): boolean {
    return this.profileManager.exists();
  }

  /**
   * Get profile info
   */
  getProfileInfo() {
    return this.profileManager.getInfo();
  }

  /**
   * Close the browser
   */
  async close(): Promise<void> {
    if (this.stagehand) {
      log('info', '[StagehandBrowser] Closing browser');
      await this.stagehand.close();
      this.stagehand = null;
    }
  }
}
