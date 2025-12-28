import type { CheerioAPI } from 'cheerio'
import type { BreadcrumbItem } from '@actionbookdev/db'

/**
 * Content extraction result
 */
export interface ExtractedContent {
  title: string
  description?: string
  contentHtml: string
  contentText: string
  links: string[]
  breadcrumb: BreadcrumbItem[]
  /** Quality score from 0-1, indicating extraction confidence */
  qualityScore?: number
}

/**
 * Site adapter interface for extracting content from different websites
 */
export interface SiteAdapter {
  /** Adapter name for identification */
  name: string

  /** Adapter type: code, yaml, or smart */
  type: 'code' | 'yaml' | 'smart'

  /** Selectors to remove from the page before extraction */
  removeSelectors: string[]

  /** Optional: selector to wait for before extraction */
  waitForSelector?: string

  /** Extract content from the page */
  extractContent($: CheerioAPI, url: string): ExtractedContent
}

/**
 * Crawl configuration defaults for an adapter
 */
export interface AdapterCrawlConfig {
  /** Max crawl depth (default: 3) */
  maxDepth?: number
  /** Include URL patterns */
  includePatterns?: string[]
  /** Exclude URL patterns */
  excludePatterns?: string[]
  /** Rate limit in ms (default: 1000) */
  rateLimit?: number
}

/**
 * Adapter configuration entry
 */
export interface AdapterConfig {
  /** Display name for the adapter */
  name: string
  /** Description of what this adapter is for */
  description: string
  /** Default crawl configuration */
  crawlConfig?: AdapterCrawlConfig
}

/**
 * YAML site configuration schema
 */
export interface YamlSiteConfig {
  /** Unique identifier for the site */
  name: string
  /** Display name */
  displayName: string
  /** Description of the site */
  description: string

  /** CSS selectors configuration */
  selectors?: {
    /** Main content selector (default: "article, main, .content") */
    content?: string
    /** Title selector (default: "h1") */
    title?: string
    /** Description selector (default: "meta[name='description']") */
    description?: string
    /** Breadcrumb selector (default: "nav[aria-label='breadcrumb']") */
    breadcrumb?: string
  }

  /** Elements to remove before extraction */
  remove?: string[]

  /** Wait configuration for SPA sites */
  wait?: {
    /** Selector to wait for */
    selector?: string
    /** Timeout in ms (default: 5000) */
    timeout?: number
  }

  /** Crawl rules */
  crawl?: {
    /** Max crawl depth */
    maxDepth?: number
    /** Rate limit in ms */
    rateLimit?: number
    /** URL patterns to include */
    include?: string[]
    /** URL patterns to exclude */
    exclude?: string[]
  }

  /** Content cleaning rules */
  cleaning?: {
    /** Headings text to skip (remove section) */
    skipSections?: string[]
    /** Regex replacements */
    replacements?: Array<{
      pattern: string
      replacement: string
    }>
  }

  /** Link extraction rules */
  links?: {
    /** URL patterns to include */
    includePatterns?: string[]
    /** Whether to follow external links */
    followExternal?: boolean
  }
}

/**
 * Structured data extracted from page (JSON-LD, OpenGraph, etc.)
 */
export interface StructuredData {
  title?: string
  description?: string
  author?: string
  publishDate?: string
  modifiedDate?: string
  type?: string
  image?: string
  url?: string
}
