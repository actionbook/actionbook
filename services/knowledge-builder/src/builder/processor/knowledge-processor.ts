/**
 * KnowledgeProcessor - Orchestrates the knowledge building pipeline
 *
 * Flow: Load → Convert → Chunk → Embed → Store
 *
 * Supports Blue/Green deployment with version management:
 * 1. Creates a new version in 'building' state
 * 2. Writes all documents to that version
 * 3. Publishes version atomically on success (optional)
 *
 * Crawl scheduling is handled here (not in PageLoader):
 * - URL queue management
 * - Visited URL tracking
 * - Depth control
 * - Rate limiting
 */

import PQueue from 'p-queue';
import { PageLoader, hashUrl, hashContent, type PageContent, type PageLoadError } from '../page-loader.js';
import { Converter } from '../converter.js';
import { DocumentChunker, hashChunk, type ChunkData } from '../chunker.js';
import { createEmbeddingProviderFromEnv, type EmbeddingProvider } from '../brain/index.js';
import { createStorage, type Storage, type CreateChunkInput } from '../storage/index.js';
import type { Source, SourceVersion, CrawlLog } from '../storage/types.js';
import { resolveAdapter } from '../adapters/index.js';
import type {
  Processor,
  ProcessorConfig,
  ProcessingResult,
  ProcessedDocument,
  ProgressCallback,
  PrepareResult,
} from './types.js';

/**
 * Default chunker options
 */
const DEFAULT_CHUNKER_OPTIONS = {
  chunkSize: 2000,
  chunkOverlap: 50,
  minChunkSize: 100,
  splitHeadingLevel: 2,
};

/**
 * Internal crawl state
 */
interface CrawlState {
  visitedUrls: Set<string>;
  failedUrls: PageLoadError[];
  queue: PQueue;
  totalPages: number;
  newPages: number;
  updatedPages: number;
  skippedPages: number;
  errorCount: number;
}

/**
 * Processing context passed to page processing
 */
interface ProcessingContext {
  source: Source;
  version: SourceVersion;
  chunker: DocumentChunker;
  crawlLog: CrawlLog;
  onProgress?: ProgressCallback;
  /** Minimum chunk content length to be stored (default: 100) */
  minChunkSize: number;
}

/**
 * KnowledgeProcessor
 *
 * Coordinates the entire knowledge building process:
 * 1. Resolves adapter for the source
 * 2. Manages crawl queue and scheduling
 * 3. Uses PageLoader to fetch individual pages
 * 4. Chunks content using DocumentChunker
 * 5. Generates embeddings using Brain layer
 * 6. Stores data using Storage layer
 */
export class KnowledgeProcessor implements Processor {
  private storage: Storage;
  private embedder: EmbeddingProvider | null = null;
  private pageLoader: PageLoader | null = null;
  private converter: Converter | null = null;
  private stopped = false;

  // Prepared state (set by prepare(), used by process())
  private preparedSource: Source | null = null;
  private preparedVersion: SourceVersion | null = null;
  private preparedCrawlLog: CrawlLog | null = null;

  constructor(storage?: Storage) {
    this.storage = storage || createStorage();
  }

  /**
   * Prepare for processing - creates source and version
   * Call this before process() to get source/version info for external tracking
   */
  async prepare(config: ProcessorConfig): Promise<PrepareResult> {
    // Initialize components first
    await this.initializeComponents(config);

    // Setup source and version
    const { source, version, crawlLog } = await this.setupSourceAndVersion(config);

    // Store prepared state for process() to use
    this.preparedSource = source;
    this.preparedVersion = version;
    this.preparedCrawlLog = crawlLog;

    console.log(`[Processor] Prepared: source=${source.id}, version=${version.id}`);

    return {
      sourceId: source.id,
      sourceName: source.name,
      versionId: version.id,
      versionNumber: version.versionNumber,
    };
  }

  /**
   * Process a documentation source
   * If prepare() was called, uses the prepared source/version
   * Otherwise, creates new source/version automatically
   */
  async process(
    config: ProcessorConfig,
    onProgress?: ProgressCallback
  ): Promise<ProcessingResult> {
    const startTime = Date.now();
    this.stopped = false;

    let source: Source;
    let version: SourceVersion;
    let crawlLog: CrawlLog;

    // Check if prepare() was called
    if (this.preparedSource && this.preparedVersion && this.preparedCrawlLog) {
      // Use prepared state
      source = this.preparedSource;
      version = this.preparedVersion;
      crawlLog = this.preparedCrawlLog;
      console.log(`[Processor] Using prepared state: source=${source.id}, version=${version.id}`);
    } else {
      // Initialize components and setup source/version
      await this.initializeComponents(config);
      const setup = await this.setupSourceAndVersion(config);
      source = setup.source;
      version = setup.version;
      crawlLog = setup.crawlLog;
    }

    const chunkerOptions = {
      ...DEFAULT_CHUNKER_OPTIONS,
      ...config.chunkerOptions,
    };
    const chunker = new DocumentChunker(chunkerOptions);

    // Initialize crawl state
    const state = this.createCrawlState(config);

    // Create processing context
    const context: ProcessingContext = {
      source,
      version,
      chunker,
      crawlLog,
      onProgress,
      minChunkSize: chunkerOptions.minChunkSize,
    };

    // Execute crawl
    await this.executeCrawl(config, state, context);

    // Cleanup and finalize
    await this.cleanup();
    const result = await this.finalize(config, state, context, startTime);

    // Clear prepared state after processing
    this.preparedSource = null;
    this.preparedVersion = null;
    this.preparedCrawlLog = null;

    return result;
  }

  /**
   * Stop processing gracefully
   */
  async stop(): Promise<void> {
    this.stopped = true;
    await this.cleanup();
  }

  // ============================================================================
  // Initialization
  // ============================================================================

  /**
   * Initialize processing components (embedder, page loader, converter)
   */
  private async initializeComponents(config: ProcessorConfig): Promise<void> {
    // Initialize embedder if needed
    if (!config.skipEmbeddings) {
      try {
        this.embedder = createEmbeddingProviderFromEnv();
      } catch (error) {
        throw new Error(`Failed to create embedding provider: ${error}`);
      }
    }

    // Resolve adapter and log
    const resolved = resolveAdapter(config.sourceName);
    console.log(`[Processor] Using adapter: ${resolved.adapter.name} (${resolved.source})`);

    // Initialize page loader
    this.pageLoader = new PageLoader({
      baseUrl: config.baseUrl,
      crawlConfig: config.crawlConfig,
      adapterName: config.sourceName,
    });
    await this.pageLoader.init();

    // Initialize converter
    this.converter = new Converter();
  }

  /**
   * Setup source, version, and crawl log in database
   */
  private async setupSourceAndVersion(config: ProcessorConfig): Promise<{
    source: Source;
    version: SourceVersion;
    crawlLog: CrawlLog;
  }> {
    // Get or create source
    let source = await this.storage.getSourceByName(config.sourceName);
    if (!source) {
      source = await this.storage.createSource({
        name: config.sourceName,
        baseUrl: config.baseUrl,
        crawlConfig: config.crawlConfig,
      });
      console.log(`[Processor] Created new source: ${source.name} (ID: ${source.id})`);
    } else {
      console.log(`[Processor] Using existing source: ${source.name} (ID: ${source.id})`);
    }

    // Create new version for Blue/Green deployment
    const versionOptions = config.versionOptions || {};
    const version = await this.storage.createVersion({
      sourceId: source.id,
      commitMessage: versionOptions.commitMessage || `Crawl at ${new Date().toISOString()}`,
      createdBy: versionOptions.createdBy || 'knowledge-builder',
    });
    console.log(`[Processor] Created version: v${version.versionNumber} (ID: ${version.id}, status: ${version.status})`);

    // Create crawl log
    const crawlLog = await this.storage.createCrawlLog(source.id);
    console.log(`[Processor] Started crawl (Log ID: ${crawlLog.id})`);

    return { source, version, crawlLog };
  }

  /**
   * Create initial crawl state
   */
  private createCrawlState(config: ProcessorConfig): CrawlState {
    return {
      visitedUrls: new Set<string>(),
      failedUrls: [],
      queue: new PQueue({
        concurrency: 1,
        interval: config.crawlConfig.rateLimit || 1000,
        intervalCap: 1,
      }),
      totalPages: 0,
      newPages: 0,
      updatedPages: 0,
      skippedPages: 0,
      errorCount: 0,
    };
  }

  // ============================================================================
  // Crawl Execution
  // ============================================================================

  /**
   * Execute the crawl starting from base URL or explicit URLs list
   */
  private async executeCrawl(
    config: ProcessorConfig,
    state: CrawlState,
    context: ProcessingContext
  ): Promise<void> {
    // Check if explicit URLs are provided
    const explicitUrls = config.crawlConfig.urls;
    if (explicitUrls && explicitUrls.length > 0) {
      // URLs-only mode: crawl only the specified URLs, no link following
      console.log(`[Processor] URLs-only mode: crawling ${explicitUrls.length} specified URLs`);
      await this.executeCrawlUrlsOnly(explicitUrls, state, context);
      return;
    }

    // Standard recursive crawl mode
    const processUrl = async (url: string, depth: number): Promise<void> => {
      if (this.stopped) return;

      // Normalize and validate URL
      const normalizedUrl = this.pageLoader!.normalizeUrl(url);
      if (!this.shouldProcessUrl(normalizedUrl, depth, config, state)) {
        return;
      }

      state.visitedUrls.add(normalizedUrl);

      try {
        // Load and process the page
        const pageContent = await this.loadPage(normalizedUrl, state);
        if (!pageContent) return;

        state.totalPages++;

        // Process page content
        await this.processPageContent(pageContent, depth, state, context);

        // Recursively process child links
        for (const link of pageContent.links) {
          await processUrl(link, depth + 1);
        }
      } catch (error) {
        this.handleCrawlError(normalizedUrl, error, state);
      }
    };

    await processUrl(config.baseUrl, 0);
  }

  /**
   * Execute crawl for explicit URLs only (no link following)
   */
  private async executeCrawlUrlsOnly(
    urls: string[],
    state: CrawlState,
    context: ProcessingContext
  ): Promise<void> {
    for (const url of urls) {
      if (this.stopped) break;

      // Normalize URL
      const normalizedUrl = this.pageLoader!.normalizeUrl(url);

      // Skip if already visited
      if (state.visitedUrls.has(normalizedUrl)) {
        console.log(`[Processor] Skipping duplicate URL: ${normalizedUrl}`);
        continue;
      }

      state.visitedUrls.add(normalizedUrl);

      try {
        // Load and process the page
        const pageContent = await this.loadPage(normalizedUrl, state);
        if (!pageContent) continue;

        state.totalPages++;

        // Process page content (depth=0 for all explicit URLs)
        await this.processPageContent(pageContent, 0, state, context);
      } catch (error) {
        this.handleCrawlError(normalizedUrl, error, state);
      }
    }
  }

  /**
   * Check if URL should be processed
   */
  private shouldProcessUrl(
    url: string,
    depth: number,
    config: ProcessorConfig,
    state: CrawlState
  ): boolean {
    // Skip if already visited
    if (state.visitedUrls.has(url)) return false;

    // Check depth limit
    const maxDepth = config.crawlConfig.maxDepth ?? 3;
    if (depth > maxDepth) return false;

    // Check URL patterns
    if (!this.pageLoader!.shouldCrawl(url)) return false;

    return true;
  }

  /**
   * Load a page with rate limiting
   */
  private async loadPage(
    url: string,
    state: CrawlState
  ): Promise<PageContent | null> {
    const pageContent = await state.queue.add(() =>
      this.pageLoader!.loadPage(url)
    );

    if (!pageContent) {
      state.errorCount++;
      state.failedUrls.push({
        url,
        error: 'Page failed to load or returned empty content',
        timestamp: new Date(),
      });
      return null;
    }

    return pageContent;
  }

  /**
   * Handle crawl error
   */
  private handleCrawlError(
    url: string,
    error: unknown,
    state: CrawlState
  ): void {
    const errorMessage = error instanceof Error ? error.message : String(error);
    console.error(`[Processor] Error crawling ${url}:`, errorMessage);
    state.errorCount++;
    state.failedUrls.push({
      url,
      error: errorMessage,
      timestamp: new Date(),
    });
  }

  // ============================================================================
  // Page Processing
  // ============================================================================

  /**
   * Process a single page: check changes, convert, chunk, embed, store
   */
  private async processPageContent(
    pageContent: PageContent,
    depth: number,
    state: CrawlState,
    context: ProcessingContext
  ): Promise<void> {
    const { source, version, chunker, crawlLog, onProgress, minChunkSize } = context;

    // Notify progress - crawling phase
    this.notifyProgress(onProgress, 'crawling', pageContent.url, state);

    // Check if content changed
    const urlHash = hashUrl(pageContent.url);
    const contentHash = hashContent(pageContent.contentText);
    const { isNew, isUpdated } = await this.checkContentChanges(
      source,
      urlHash,
      contentHash
    );

    // Skip if unchanged
    if (!isNew && !isUpdated) {
      this.handleUnchangedPage(pageContent, urlHash, contentHash, depth, state, onProgress);
      return;
    }

    // Convert HTML to Markdown
    const contentMd = this.converter!.convert(pageContent.contentHtml);

    // Skip if content is too short (no need to chunk/embed)
    if (contentMd.trim().length < minChunkSize) {
      this.handleShortContentPage(pageContent, urlHash, contentHash, depth, state, onProgress);
      return;
    }

    // Chunk content
    this.notifyProgress(onProgress, 'chunking', pageContent.url, state);
    const allChunks = chunker.chunk(contentMd);

    // Filter out chunks with content shorter than minChunkSize
    const chunkDataList = allChunks.filter(
      (chunk) => chunk.content.trim().length >= minChunkSize
    );

    // Generate embeddings
    const embeddings = await this.generateEmbeddings(
      chunkDataList,
      pageContent.url,
      state,
      onProgress
    );

    // Store document and chunks
    this.notifyProgress(onProgress, 'storing', pageContent.url, state);
    await this.storeDocumentAndChunks(
      pageContent,
      contentMd,
      urlHash,
      contentHash,
      depth,
      chunkDataList,
      embeddings,
      source,
      version
    );

    // Update stats and log
    this.logPageProcessed(pageContent.title, isNew, chunkDataList.length);
    this.updateStats(state, isNew, isUpdated);

    // Notify completion
    this.notifyPageComplete(
      pageContent,
      urlHash,
      contentHash,
      depth,
      chunkDataList.length,
      isNew,
      isUpdated,
      state,
      onProgress
    );

    // Periodic crawl log update
    await this.updateCrawlLogPeriodically(crawlLog.id, state);
  }

  /**
   * Check if content has changed compared to existing version
   */
  private async checkContentChanges(
    source: Source,
    urlHash: string,
    contentHash: string
  ): Promise<{ isNew: boolean; isUpdated: boolean }> {
    const existingHash = source.currentVersionId
      ? await this.storage.getDocumentContentHashByVersion(source.currentVersionId, urlHash)
      : await this.storage.getDocumentContentHash(source.id, urlHash);

    const isNew = !existingHash;
    const isUpdated = !!(existingHash && existingHash !== contentHash);

    return { isNew, isUpdated };
  }

  /**
   * Handle unchanged page (skip processing)
   */
  private handleUnchangedPage(
    pageContent: PageContent,
    urlHash: string,
    contentHash: string,
    depth: number,
    state: CrawlState,
    onProgress?: ProgressCallback
  ): void {
    console.log(`  [SKIP] ${pageContent.title} (unchanged)`);
    state.skippedPages++;

    const doc: ProcessedDocument = {
      url: pageContent.url,
      urlHash,
      title: pageContent.title,
      description: pageContent.description,
      contentHash,
      depth,
      breadcrumb: pageContent.breadcrumb,
      chunkCount: 0,
      isNew: false,
      isUpdated: false,
      skipped: true,
    };

    onProgress?.({
      phase: 'crawling',
      currentUrl: pageContent.url,
      pagesProcessed: state.totalPages,
      totalDiscovered: state.totalPages,
      document: doc,
    });
  }

  /**
   * Handle page with content too short (skip processing)
   */
  private handleShortContentPage(
    pageContent: PageContent,
    urlHash: string,
    contentHash: string,
    depth: number,
    state: CrawlState,
    onProgress?: ProgressCallback
  ): void {
    console.log(`  [SKIP] ${pageContent.title} (content too short)`);
    state.skippedPages++;

    const doc: ProcessedDocument = {
      url: pageContent.url,
      urlHash,
      title: pageContent.title,
      description: pageContent.description,
      contentHash,
      depth,
      breadcrumb: pageContent.breadcrumb,
      chunkCount: 0,
      isNew: false,
      isUpdated: false,
      skipped: true,
    };

    onProgress?.({
      phase: 'crawling',
      currentUrl: pageContent.url,
      pagesProcessed: state.totalPages,
      totalDiscovered: state.totalPages,
      document: doc,
    });
  }

  /**
   * Generate embeddings for chunks
   */
  private async generateEmbeddings(
    chunkDataList: ChunkData[],
    url: string,
    state: CrawlState,
    onProgress?: ProgressCallback
  ): Promise<number[][]> {
    if (!this.embedder || chunkDataList.length === 0) {
      return [];
    }

    this.notifyProgress(onProgress, 'embedding', url, state);
    console.log(`    Generating embeddings...`);

    const embeddingResults = await this.embedder.embedBatch(
      chunkDataList.map((c) => c.content)
    );

    return embeddingResults.map((r) => r.embedding);
  }

  /**
   * Store document and chunks in database
   */
  private async storeDocumentAndChunks(
    pageContent: PageContent,
    contentMd: string,
    urlHash: string,
    contentHash: string,
    depth: number,
    chunkDataList: ChunkData[],
    embeddings: number[][],
    source: Source,
    version: SourceVersion
  ): Promise<void> {
    const chunkInserts: CreateChunkInput[] = chunkDataList.map((chunk, i) => ({
      documentId: 0, // Will be set after document is created
      sourceVersionId: version.id, // Redundant for query optimization
      content: chunk.content,
      contentHash: hashChunk(chunk.content),
      chunkIndex: chunk.chunkIndex,
      startChar: chunk.startChar,
      endChar: chunk.endChar,
      heading: chunk.heading,
      headingHierarchy: chunk.headingHierarchy,
      tokenCount: chunk.tokenCount,
      embedding: embeddings[i],
      embeddingModel: this.embedder?.model,
    }));

    await this.storage.withTransaction(async (txStorage) => {
      const doc = await txStorage.upsertDocument({
        sourceId: source.id,
        sourceVersionId: version.id,
        url: pageContent.url,
        urlHash,
        title: pageContent.title,
        description: pageContent.description,
        contentText: pageContent.contentText,
        contentHtml: pageContent.contentHtml,
        contentMd,
        depth,
        breadcrumb: pageContent.breadcrumb,
        wordCount: pageContent.contentText.split(/\s+/).length,
        language: 'en',
        contentHash,
        status: 'active',
        version: 1,
      });

      await txStorage.deleteChunksByDocument(doc.id);

      for (const chunk of chunkInserts) {
        chunk.documentId = doc.id;
      }
      await txStorage.insertChunks(chunkInserts);
    });
  }

  // ============================================================================
  // Progress & Logging
  // ============================================================================

  /**
   * Notify progress callback
   */
  private notifyProgress(
    onProgress: ProgressCallback | undefined,
    phase: 'crawling' | 'chunking' | 'embedding' | 'storing',
    url: string,
    state: CrawlState
  ): void {
    onProgress?.({
      phase,
      currentUrl: url,
      pagesProcessed: state.totalPages,
      totalDiscovered: state.totalPages,
    });
  }

  /**
   * Log page processed
   */
  private logPageProcessed(title: string, isNew: boolean, chunkCount: number): void {
    console.log(`  [${isNew ? 'NEW' : 'UPDATE'}] ${title}`);
    console.log(`    Chunked into ${chunkCount} parts`);
  }

  /**
   * Update processing stats
   */
  private updateStats(state: CrawlState, isNew: boolean, isUpdated: boolean): void {
    if (isNew) state.newPages++;
    if (isUpdated) state.updatedPages++;
  }

  /**
   * Notify page processing complete
   */
  private notifyPageComplete(
    pageContent: PageContent,
    urlHash: string,
    contentHash: string,
    depth: number,
    chunkCount: number,
    isNew: boolean,
    isUpdated: boolean,
    state: CrawlState,
    onProgress?: ProgressCallback
  ): void {
    const processedDoc: ProcessedDocument = {
      url: pageContent.url,
      urlHash,
      title: pageContent.title,
      description: pageContent.description,
      contentHash,
      depth,
      breadcrumb: pageContent.breadcrumb,
      chunkCount,
      isNew,
      isUpdated,
      skipped: false,
    };

    onProgress?.({
      phase: 'storing',
      currentUrl: pageContent.url,
      pagesProcessed: state.totalPages,
      totalDiscovered: state.totalPages,
      document: processedDoc,
    });
  }

  /**
   * Update crawl log periodically
   */
  private async updateCrawlLogPeriodically(
    crawlLogId: number,
    state: CrawlState
  ): Promise<void> {
    if (state.totalPages % 10 === 0) {
      await this.storage.updateCrawlLog(crawlLogId, {
        pagesCrawled: state.totalPages,
        pagesNew: state.newPages,
        pagesUpdated: state.updatedPages,
      });
    }
  }

  // ============================================================================
  // Cleanup & Finalization
  // ============================================================================

  /**
   * Cleanup resources
   */
  private async cleanup(): Promise<void> {
    if (this.pageLoader) {
      await this.pageLoader.close();
      this.pageLoader = null;
    }
    this.converter = null;
  }

  /**
   * Finalize crawl and return result
   */
  private async finalize(
    config: ProcessorConfig,
    state: CrawlState,
    context: ProcessingContext,
    startTime: number
  ): Promise<ProcessingResult> {
    const { source, version, crawlLog } = context;
    const versionOptions = config.versionOptions || {};

    // Finalize crawl log
    const crawlSuccess = !(state.errorCount > 0 && state.totalPages === 0);
    await this.storage.updateCrawlLog(crawlLog.id, {
      status: crawlSuccess ? 'completed' : 'failed',
      pagesCrawled: state.totalPages,
      pagesNew: state.newPages,
      pagesUpdated: state.updatedPages,
      errorCount: state.errorCount,
      errorDetails:
        state.failedUrls.length > 0
          ? {
              failedUrls: state.failedUrls.map((f) => ({
                url: f.url,
                error: f.error,
                timestamp: f.timestamp.toISOString(),
              })),
            }
          : undefined,
    });

    await this.storage.updateSourceLastCrawled(source.id);

    // Publish version if successful
    const published = await this.publishVersionIfNeeded(
      version,
      versionOptions,
      crawlSuccess,
      state.totalPages
    );

    return {
      sourceId: source.id,
      versionId: version.id,
      versionNumber: version.versionNumber,
      published,
      crawlLogId: crawlLog.id,
      totalPages: state.totalPages,
      newPages: state.newPages,
      updatedPages: state.updatedPages,
      skippedPages: state.skippedPages,
      errorCount: state.errorCount,
      failedUrls: state.failedUrls,
      durationMs: Date.now() - startTime,
    };
  }

  /**
   * Publish version if conditions are met
   */
  private async publishVersionIfNeeded(
    version: SourceVersion,
    versionOptions: ProcessorConfig['versionOptions'],
    crawlSuccess: boolean,
    totalPages: number
  ): Promise<boolean> {
    const autoPublish = versionOptions?.autoPublish !== false;

    if (crawlSuccess && autoPublish && totalPages > 0) {
      await this.storage.publishVersion(version.id);
      console.log(`[Processor] Published version: v${version.versionNumber} (ID: ${version.id})`);
      return true;
    }

    if (!crawlSuccess) {
      console.log(`[Processor] Version not published due to crawl failure (ID: ${version.id})`);
    } else if (!autoPublish) {
      console.log(`[Processor] Version not published (autoPublish disabled, ID: ${version.id})`);
    }

    return false;
  }
}

/**
 * Create a KnowledgeProcessor instance
 */
export function createProcessor(storage?: Storage): Processor {
  return new KnowledgeProcessor(storage);
}
