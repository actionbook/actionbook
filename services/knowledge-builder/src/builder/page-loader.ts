/**
 * PageLoader - Loads web pages and extracts structured content
 *
 * Responsibilities:
 * - Browser lifecycle management (init/close)
 * - Page loading with Playwright
 * - Content extraction using adapters
 *
 * Does NOT handle:
 * - HTML to Markdown conversion (handled by Converter)
 * - Recursive crawling (handled by Processor)
 * - URL queue management (handled by Processor)
 */

import { chromium, Browser, BrowserContext } from 'playwright'
import * as cheerio from 'cheerio'
import crypto from 'crypto'
import type { CrawlConfig, BreadcrumbItem } from '@actionbookdev/db'
import { getAdapter, type SiteAdapter } from './adapters/index.js'
import {
  BrowserProfileManager,
  DEFAULT_PROFILE_DIR,
  ANTI_DETECTION_ARGS,
  IGNORE_DEFAULT_ARGS,
} from '@actionbookdev/browser-profile'

/**
 * Raw page content extracted from a URL
 */
export interface PageContent {
  url: string
  title: string
  description?: string
  contentHtml: string
  contentText: string
  links: string[]
  breadcrumb: BreadcrumbItem[]
}

/**
 * Error from loading a page
 */
export interface PageLoadError {
  url: string
  error: string
  timestamp: Date
}

/**
 * PageLoader configuration
 */
export interface PageLoaderConfig {
  /** Base URL for the site */
  baseUrl: string
  /** Crawl configuration */
  crawlConfig: CrawlConfig
  /** Adapter name to use */
  adapterName?: string
}

/**
 * PageLoader - loads and extracts content from web pages
 */
export class PageLoader {
  private browser: Browser | null = null
  private persistentContext: BrowserContext | null = null
  private config: CrawlConfig
  private baseUrl: string
  private adapter: SiteAdapter

  constructor(config: PageLoaderConfig) {
    this.baseUrl = config.baseUrl
    this.config = config.crawlConfig
    this.adapter = getAdapter(config.adapterName || 'default')

    console.log(`[PageLoader] Using adapter: ${this.adapter.name}`)
  }

  /**
   * Initialize the browser
   *
   * When BROWSER_PROFILE_ENABLED=true, the browser will use a persistent profile
   * to maintain login state across sessions. This is useful for sites that require
   * authentication or have CAPTCHA challenges.
   *
   * Environment variables:
   * - BROWSER_PROFILE_ENABLED: Set to "true" to enable persistent profile
   * - BROWSER_PROFILE_DIR: Custom profile directory (default: .browser-profile)
   * - BROWSER_HEADLESS: Set to "false" to show browser window (default: true)
   */
  async init(): Promise<void> {
    if (this.browser || this.persistentContext) return

    const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY
    const profileEnabled = process.env.BROWSER_PROFILE_ENABLED === 'true'
    const profileDir = process.env.BROWSER_PROFILE_DIR || DEFAULT_PROFILE_DIR
    const headless = process.env.BROWSER_HEADLESS !== 'false'

    // Build launch options
    const launchOptions: Parameters<typeof chromium.launch>[0] = {
      headless,
    }

    // Add proxy if configured
    if (proxyUrl) {
      console.log(`[PageLoader] Using proxy: ${proxyUrl}`)
      launchOptions.proxy = { server: proxyUrl }
    } else {
      console.log('[PageLoader] No proxy configured')
    }

    // Add profile support if enabled
    if (profileEnabled) {
      const profileManager = new BrowserProfileManager({ baseDir: profileDir })
      const profilePath = profileManager.getProfilePath()

      // Clean up stale lock files from previous crashed sessions
      profileManager.cleanupStaleLocks()

      // Ensure profile directory exists
      profileManager.ensureDir()

      // Show profile info
      const info = profileManager.getInfo()
      if (info.exists) {
        console.log(`[PageLoader] Using browser profile: ${info.path} (${info.size})`)
      } else {
        console.warn(`[PageLoader] No browser profile found. Run 'pnpm run login' first to save login state.`)
      }

      // Configure persistent context options
      // Note: For persistent profile, we use launchPersistentContext instead of launch
      // This returns a BrowserContext directly, not a Browser
      this.persistentContext = await chromium.launchPersistentContext(profilePath, {
        headless,
        args: ANTI_DETECTION_ARGS,
        ignoreDefaultArgs: IGNORE_DEFAULT_ARGS,
        proxy: proxyUrl ? { server: proxyUrl } : undefined,
      })

      console.log('[PageLoader] Browser initialized with persistent profile')
      return
    }

    // Standard browser launch (no profile)
    this.browser = await chromium.launch(launchOptions)
    console.log('[PageLoader] Browser initialized')
  }

  /**
   * Close the browser
   */
  async close(): Promise<void> {
    if (this.persistentContext) {
      await this.persistentContext.close()
      this.persistentContext = null
    }
    if (this.browser) {
      await this.browser.close()
      this.browser = null
    }
  }

  /**
   * Load and extract content from a page
   *
   * @param url - URL to load
   * @returns Page content or null if failed
   */
  async loadPage(url: string): Promise<PageContent | null> {
    if (!this.browser && !this.persistentContext) {
      await this.init()
    }

    // Create a new page from either persistent context or browser
    const page = this.persistentContext
      ? await this.persistentContext.newPage()
      : await this.browser!.newPage()

    try {
      console.log(`[PageLoader] Loading: ${url}`)

      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 })

      // Wait for content selector if specified
      const waitForSelector =
        this.adapter.waitForSelector || this.config.waitForSelector
      if (waitForSelector) {
        await page.waitForSelector(waitForSelector, { timeout: 10000 })
      }

      // Wait additional time for dynamic content
      const waitTime = this.config.waitTime ?? 1000
      if (waitTime > 0) {
        await page.waitForTimeout(waitTime)
      }

      // Remove screen-reader-only elements
      await this.removeScreenReaderElements(page)

      // Get the page HTML
      const html = await page.content()
      const $ = cheerio.load(html)

      // Apply removeSelectors from config
      const configRemoveSelectors = this.config.removeSelectors || []
      for (const selector of configRemoveSelectors) {
        $(selector).remove()
      }

      // Use adapter to extract content
      const extracted = this.adapter.extractContent($, url)

      // Filter links based on crawl config patterns
      const filteredLinks = extracted.links.filter((link) =>
        this.shouldCrawl(link)
      )

      return {
        url,
        title: extracted.title,
        description: extracted.description,
        contentHtml: extracted.contentHtml,
        contentText: extracted.contentText,
        links: filteredLinks,
        breadcrumb: extracted.breadcrumb,
      }
    } catch (error) {
      console.error(`[PageLoader] Failed to load ${url}:`, error)
      return null
    } finally {
      await page.close()
    }
  }

  /**
   * Get the base URL
   */
  getBaseUrl(): string {
    return this.baseUrl
  }

  /**
   * Normalize a URL (remove hash, normalize trailing slash)
   */
  normalizeUrl(url: string): string {
    const urlObj = new URL(url)
    urlObj.hash = ''
    if (urlObj.pathname !== '/' && urlObj.pathname.endsWith('/')) {
      urlObj.pathname = urlObj.pathname.slice(0, -1)
    }
    return urlObj.href
  }

  /**
   * Check if a URL should be crawled based on include/exclude patterns
   */
  shouldCrawl(url: string): boolean {
    try {
      const path = new URL(url).pathname
      const includePatterns = this.config.includePatterns || []
      const excludePatterns = this.config.excludePatterns || []

      // Check exclude patterns
      for (const pattern of excludePatterns) {
        if (this.matchPattern(path, pattern)) {
          return false
        }
      }

      // Check include patterns (if any)
      if (includePatterns.length > 0) {
        for (const pattern of includePatterns) {
          if (this.matchPattern(path, pattern)) {
            return true
          }
        }
        return false
      }

      return true
    } catch {
      return false
    }
  }

  // Private methods

  private async removeScreenReaderElements(
    page: import('playwright').Page
  ): Promise<void> {
    await page.evaluate(() => {
      const elements = document.querySelectorAll('*')
      elements.forEach((el) => {
        const style = window.getComputedStyle(el)
        const isSrOnly =
          (style.clip === 'rect(0px, 0px, 0px, 0px)' ||
            style.clipPath === 'inset(100%)') &&
          (style.width === '1px' || style.height === '1px')
        if (isSrOnly) {
          el.remove()
        }
      })
    })
  }

  private matchPattern(path: string, pattern: string): boolean {
    const regexPattern = pattern
      .replace(/\*/g, '.*')
      .replace(/\?/g, '.')
      .replace(/\//g, '\\/')
    return new RegExp(`^${regexPattern}`).test(path)
  }
}

// Helper functions

/**
 * Generate a hash for a URL
 */
export function hashUrl(url: string): string {
  return crypto.createHash('sha256').update(url).digest('hex').substring(0, 16)
}

/**
 * Generate a hash for content
 */
export function hashContent(content: string): string {
  return crypto.createHash('sha256').update(content).digest('hex')
}
