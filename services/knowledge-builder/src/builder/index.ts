/**
 * Builder Layer - Unified entry point
 *
 * This module provides the main entry point for the knowledge builder.
 * The primary interface is the Processor, which orchestrates the entire pipeline.
 *
 * Usage:
 * ```typescript
 * import { createProcessor } from './builder';
 *
 * const processor = createProcessor();
 * await processor.process({
 *   sourceName: 'my-docs',
 *   baseUrl: 'https://docs.example.com',
 *   crawlConfig: { maxDepth: 3 },
 * });
 * ```
 */

// ============================================================================
// Processor Layer (Primary Entry Point)
// ============================================================================
export {
  // Factory
  createProcessor,
  KnowledgeProcessor,
  // Types
  type Processor,
  type ProcessorConfig,
  type ProcessingResult,
  type ProcessedDocument,
  type ProcessingProgress,
  type ProgressCallback,
  type PrepareResult,
} from './processor/index.js';

// ============================================================================
// PageLoader (Internal - used by Processor)
// ============================================================================
export {
  PageLoader,
  hashUrl,
  hashContent,
  type PageContent,
  type PageLoadError,
  type PageLoaderConfig,
} from './page-loader.js';

// ============================================================================
// Converter (Internal - used by Processor)
// ============================================================================
export {
  Converter,
  createConverter,
  type ConverterConfig,
} from './converter.js';

// ============================================================================
// Chunker
// ============================================================================
export {
  DocumentChunker,
  hashChunk,
  type ChunkData,
  type ChunkerOptions,
} from './chunker.js';

// ============================================================================
// Storage Layer
// ============================================================================
export {
  // Factory
  createStorage,
  Storage,
  // Types
  type Source,
  type SourceVersion,
  type CrawlLog,
  type DocumentResult,
  type CreateSourceInput,
  type CreateVersionInput,
  type UpsertDocumentInput,
  type CreateChunkInput,
  type UpdateCrawlLogInput,
} from './storage/index.js';

// ============================================================================
// Brain Layer (AI Capabilities)
// ============================================================================
export {
  // Factory functions
  createEmbeddingProvider,
  createEmbeddingProviderFromEnv,
  getEmbeddingDimension,
  // Registry functions
  registerEmbeddingProvider,
  listEmbeddingProviders,
  hasEmbeddingProvider,
  // Provider implementations
  OpenAIEmbeddingProvider,
  // Types
  type EmbeddingProvider,
  type EmbeddingConfig,
  type EmbeddingProviderType,
  type EmbeddingResult,
} from './brain/index.js';

// ============================================================================
// Adapters Layer (Content Extraction)
// ============================================================================
export {
  // Adapter classes
  DefaultAdapter,
  YamlAdapter,
  SmartAdapter,
  // Resolver
  AdapterResolver,
  getResolver,
  resolveAdapter,
  // YAML utilities
  loadYamlConfig,
  listYamlConfigs,
  hasYamlConfig,
  // Legacy functions
  getAdapter,
  getAdapterConfig,
  getAdapterCrawlConfig,
  validateSourceName,
  listAllAdapters,
  // Types
  type SiteAdapter,
  type ExtractedContent,
  type AdapterConfig,
  type AdapterCrawlConfig,
  type YamlSiteConfig,
  type ResolvedAdapter,
} from './adapters/index.js';

