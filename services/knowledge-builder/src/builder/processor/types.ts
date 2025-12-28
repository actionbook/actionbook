/**
 * Processor Layer - Knowledge building pipeline orchestration
 *
 * Coordinates the entire knowledge building process:
 * Load → Convert → Chunk → Embed → Store
 */

import type { CrawlConfig, BreadcrumbItem } from '@actionbookdev/db'
import type { PageLoadError } from '../page-loader.js'

/**
 * Processor configuration
 */
export interface ProcessorConfig {
  /** Source name for adapter resolution */
  sourceName: string

  /** Base URL to crawl */
  baseUrl: string

  /** Crawl configuration */
  crawlConfig: CrawlConfig

  /** Skip embedding generation */
  skipEmbeddings?: boolean

  /** Chunker options */
  chunkerOptions?: {
    chunkSize?: number
    chunkOverlap?: number
    minChunkSize?: number
    splitHeadingLevel?: number
  }

  /**
   * Version management options (Blue/Green deployment)
   */
  versionOptions?: {
    /** Commit message for the version */
    commitMessage?: string
    /** Creator identifier */
    createdBy?: string
    /** Auto-publish version after successful crawl (default: true) */
    autoPublish?: boolean
  }
}

/**
 * Processing result for a single document
 */
export interface ProcessedDocument {
  url: string
  urlHash: string
  title: string
  description?: string
  contentHash: string
  depth: number
  breadcrumb: BreadcrumbItem[]
  chunkCount: number
  isNew: boolean
  isUpdated: boolean
  skipped: boolean
}

/**
 * Overall processing result
 */
export interface ProcessingResult {
  /** Source ID */
  sourceId: number

  /** Version ID (Blue/Green deployment) */
  versionId: number

  /** Version number */
  versionNumber: number

  /** Whether version was published */
  published: boolean

  /** Crawl log ID */
  crawlLogId: number

  /** Total pages crawled */
  totalPages: number

  /** New pages added */
  newPages: number

  /** Pages updated */
  updatedPages: number

  /** Pages skipped (unchanged) */
  skippedPages: number

  /** Error count */
  errorCount: number

  /** Failed URLs with errors */
  failedUrls: PageLoadError[]

  /** Processing duration in milliseconds */
  durationMs: number
}

/**
 * Progress callback for monitoring processing
 */
export interface ProcessingProgress {
  /** Current phase */
  phase: 'crawling' | 'chunking' | 'embedding' | 'storing'

  /** Current URL being processed */
  currentUrl?: string

  /** Pages processed so far */
  pagesProcessed: number

  /** Total pages discovered */
  totalDiscovered: number

  /** Current document info */
  document?: ProcessedDocument
}

/**
 * Progress callback function type
 */
export type ProgressCallback = (progress: ProcessingProgress) => void

/**
 * Result of prepare() call - contains source and version info
 */
export interface PrepareResult {
  /** Source ID */
  sourceId: number

  /** Source name */
  sourceName: string

  /** Version ID */
  versionId: number

  /** Version number */
  versionNumber: number
}

/**
 * Processor interface
 *
 * Orchestrates the knowledge building pipeline
 */
export interface Processor {
  /**
   * Prepare for processing - creates source and version
   * Call this before process() to get source/version info for external tracking
   *
   * @param config - Processing configuration
   * @returns Prepare result with source and version info
   */
  prepare(config: ProcessorConfig): Promise<PrepareResult>

  /**
   * Process a documentation source
   * Must call prepare() first
   *
   * @param config - Processing configuration
   * @param onProgress - Optional progress callback
   * @returns Processing result
   */
  process(
    config: ProcessorConfig,
    onProgress?: ProgressCallback
  ): Promise<ProcessingResult>

  /**
   * Stop processing (for graceful shutdown)
   */
  stop(): Promise<void>
}
