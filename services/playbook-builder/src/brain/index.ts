/**
 * Brain Layer - AI capabilities
 *
 * Provides:
 * - AIClient: Chat/completion with tool calling
 * - EmbeddingProvider: Text embeddings for vector search
 */

// Chat/Completion
export { AIClient } from './ai-client.js';
export type { AIClientConfig, LLMProvider, LLMMetrics } from './ai-client.js';

// Embedding
export { OpenAIEmbeddingProvider, createEmbeddingProvider } from './embedding.js';
export type {
  EmbeddingProvider,
  EmbeddingResult,
  EmbeddingConfig,
  EmbeddingProviderType,
} from './types.js';
