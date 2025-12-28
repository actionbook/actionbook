/**
 * Storage Layer - Type definitions
 *
 * Provides types for storing and retrieving knowledge base data.
 *
 * Entity types use Pick from @actionbookdev/db to select only fields
 * needed by the knowledge-builder, avoiding coupling to Action-Builder fields.
 */

import type {
  // Full entity types from Drizzle schema
  Source as DbSource,
  SourceVersion as DbSourceVersion,
  CrawlLog as DbCrawlLog,
  // JSON column types
  BreadcrumbItem,
  CrawlConfig,
  CrawlStatus,
  HeadingItem,
} from '@actionbookdev/db'

/**
 * Source entity - only fields needed by knowledge-builder
 * (excludes Action-Builder fields: domain, tags, healthScore, lastRecordedAt)
 */
export type Source = Pick<
  DbSource,
  | 'id'
  | 'name'
  | 'baseUrl'
  | 'description'
  | 'crawlConfig'
  | 'lastCrawledAt'
  | 'createdAt'
  | 'updatedAt'
  | 'currentVersionId'
>

/**
 * SourceVersion entity
 */
export type SourceVersion = DbSourceVersion

/**
 * CrawlLog entity - with simplified errorDetails type
 * (db uses CrawlError[], but we use Record for flexibility)
 */
export type CrawlLog = Omit<DbCrawlLog, 'errorDetails'> & {
  errorDetails: Record<string, unknown> | null
}

/**
 * Input for creating a new source
 */
export interface CreateSourceInput {
  name: string
  baseUrl: string
  description?: string
  crawlConfig?: CrawlConfig
}

/**
 * Input for creating a new version
 */
export interface CreateVersionInput {
  sourceId: number
  commitMessage?: string
  createdBy?: string
}

/**
 * Input for creating/updating a document
 */
export interface UpsertDocumentInput {
  sourceId: number
  /** Version ID for Blue/Green deployment (required for new documents) */
  sourceVersionId?: number
  url: string
  urlHash: string
  title: string
  description?: string
  contentText: string
  contentHtml: string
  contentMd?: string
  parentId?: number
  depth: number
  breadcrumb?: BreadcrumbItem[]
  wordCount?: number
  language?: string
  contentHash: string
  status: string
  version: number
}

/**
 * Input for creating a chunk
 */
export interface CreateChunkInput {
  documentId: number
  /** Source version ID (redundant for query optimization) */
  sourceVersionId?: number
  content: string
  contentHash: string
  chunkIndex: number
  startChar: number
  endChar: number
  heading?: string
  headingHierarchy?: HeadingItem[]
  tokenCount: number
  embedding?: number[]
  embeddingModel?: string
}

/**
 * Input for updating crawl log
 */
export interface UpdateCrawlLogInput {
  status?: CrawlStatus
  pagesCrawled?: number
  pagesNew?: number
  pagesUpdated?: number
  errorCount?: number
  errorDetails?: Record<string, unknown>
}

/**
 * Minimal document result (just what processor needs)
 */
export interface DocumentResult {
  id: number
}
