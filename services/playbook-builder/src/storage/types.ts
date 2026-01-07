/**
 * Storage Layer - Type definitions
 *
 * Provides types for storing playbook and chunk data.
 */

/**
 * Input for creating a playbook document with its chunk
 */
export interface CreatePlaybookInput {
  sourceId: number;
  sourceVersionId: number;
  url: string;
  title: string;
  description?: string;
  /** Page capabilities content for the chunk */
  chunkContent: string;
  /** Embedding vector for the chunk */
  embedding?: number[];
  /** Embedding model used */
  embeddingModel?: string;
}

/**
 * Result of creating a playbook (document + chunk)
 */
export interface PlaybookResult {
  documentId: number;
  chunkId: number;
}

/**
 * Input for creating a source version
 */
export interface CreateVersionInput {
  sourceId: number;
  commitMessage?: string;
  createdBy?: string;
}

/**
 * Source version result
 */
export interface VersionResult {
  id: number;
  versionNumber: number;
}
