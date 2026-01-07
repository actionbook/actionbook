/**
 * Brain Layer - AI capability abstraction
 *
 * Provides unified interfaces for AI capabilities (embedding, chat, etc.)
 * allowing easy swapping of underlying providers.
 */

/**
 * Result of an embedding operation
 */
export interface EmbeddingResult {
  embedding: number[];
  tokenCount: number;
}

/**
 * Embedding provider interface
 */
export interface EmbeddingProvider {
  /** Provider name (e.g., 'openai') */
  readonly name: string;

  /** Model name being used */
  readonly model: string;

  /** Embedding dimension for this model */
  readonly dimension: number;

  /**
   * Generate embedding for a single text
   */
  embed(text: string): Promise<EmbeddingResult>;

  /**
   * Generate embeddings for multiple texts in batch
   */
  embedBatch(texts: string[]): Promise<EmbeddingResult[]>;
}

/**
 * Supported embedding providers
 */
export type EmbeddingProviderType = 'openai';

/**
 * Configuration for creating an embedding provider
 */
export interface EmbeddingConfig {
  /** Provider type */
  provider: EmbeddingProviderType;

  /** API key (required for most providers) */
  apiKey?: string;

  /** Model to use (provider-specific default if not specified) */
  model?: string;

  /** Custom base URL for API */
  baseUrl?: string;

  /** Request timeout in milliseconds */
  timeout?: number;

  /** Maximum retries for failed requests */
  maxRetries?: number;

  /** Provider-specific options */
  options?: Record<string, unknown>;
}
