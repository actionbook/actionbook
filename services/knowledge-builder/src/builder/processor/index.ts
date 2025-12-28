/**
 * Processor Layer
 *
 * Orchestrates the knowledge building pipeline:
 * Crawl → Chunk → Embed → Store
 */

// Types
export type {
  Processor,
  ProcessorConfig,
  ProcessingResult,
  ProcessedDocument,
  ProcessingProgress,
  ProgressCallback,
  PrepareResult,
} from './types.js';

// Implementation
export { KnowledgeProcessor, createProcessor } from './knowledge-processor.js';
