import {
  pgTable,
  serial,
  varchar,
  text,
  jsonb,
  timestamp,
  integer,
} from 'drizzle-orm/pg-core';

/**
 * Sources table - Data sources table
 * Stores crawled website source information, also serves as the site table for action-builder
 */
export const sources = pgTable('sources', {
  id: serial('id').primaryKey(),
  name: varchar('name', { length: 255 }).notNull(),
  baseUrl: text('base_url').notNull().unique(),
  description: text('description'),
  crawlConfig: jsonb('crawl_config').$type<CrawlConfig>().notNull().default({}),
  lastCrawledAt: timestamp('last_crawled_at', { withTimezone: true }),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),

  // ========== Blue/Green deployment fields ==========
  /** Current active version ID (points to source_versions.id, no FK to avoid circular reference) */
  currentVersionId: integer('current_version_id'),

  // ========== Action-Builder extended fields ==========
  /** App/Product URL - the main application URL for action building (e.g., https://www.linkedin.com) */
  appUrl: text('app_url'),
  /** Site domain (e.g., www.airbnb.com) */
  domain: varchar('domain', { length: 255 }),
  /** Tags array */
  tags: jsonb('tags').$type<string[]>().default([]),
  /** Health score (0-100), reflects selector validity rate */
  healthScore: integer('health_score'),
  /** Last recording time */
  lastRecordedAt: timestamp('last_recorded_at', { withTimezone: true }),
});

/**
 * CrawlConfig - Crawl configuration
 */
export interface CrawlConfig {
  maxDepth?: number;
  maxPages?: number;
  includePatterns?: string[];
  excludePatterns?: string[];
  waitTime?: number;
  userAgent?: string;
  /** Rate limit between requests in milliseconds */
  rateLimit?: number;
  /** CSS selectors for content extraction */
  selectors?: {
    content?: string;
    title?: string;
    description?: string;
    nav?: string;
  };
  /** Selector to wait for before extracting content */
  waitForSelector?: string;
  /** CSS selectors for elements to remove */
  removeSelectors?: string[];
  /**
   * Explicit list of URLs to crawl.
   * When specified, only these URLs will be crawled (no link following).
   * This overrides baseUrl and maxDepth settings.
   */
  urls?: string[];
  [key: string]: unknown;
}
