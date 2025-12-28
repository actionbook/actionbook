/**
 * Storage Layer
 *
 * Provides data persistence for the knowledge builder.
 */

// Types
export type {
  Source,
  SourceVersion,
  CrawlLog,
  DocumentResult,
  CreateSourceInput,
  CreateVersionInput,
  UpsertDocumentInput,
  CreateChunkInput,
  UpdateCrawlLogInput,
} from './types.js';

// Storage class and factory
export { Storage, createStorage } from './postgres.js';
