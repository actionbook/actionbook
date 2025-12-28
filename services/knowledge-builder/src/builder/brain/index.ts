/**
 * Brain Layer - AI capability abstraction
 *
 * Usage:
 * ```typescript
 * import { createEmbeddingProvider } from './brain/index.js';
 *
 * // Create provider from config
 * const embedder = createEmbeddingProvider({
 *   provider: 'openai',
 *   apiKey: process.env.OPENAI_API_KEY,
 *   model: 'text-embedding-3-small',
 * });
 *
 * // Or use environment-based factory
 * const embedder = createEmbeddingProviderFromEnv();
 *
 * // Use the provider
 * const result = await embedder.embed('Hello world');
 * const results = await embedder.embedBatch(['Hello', 'World']);
 * ```
 */

import type {
  EmbeddingProvider,
  EmbeddingConfig,
  EmbeddingProviderType,
  EmbeddingResult,
} from './types.js';
import {
  getEmbeddingProviderFactory,
  listEmbeddingProviders,
  hasEmbeddingProvider,
  registerEmbeddingProvider,
} from './registry.js';

// Re-export types
export type {
  EmbeddingProvider,
  EmbeddingConfig,
  EmbeddingProviderType,
  EmbeddingResult,
};

// Re-export registry functions for extensibility
export {
  registerEmbeddingProvider,
  listEmbeddingProviders,
  hasEmbeddingProvider,
};

// Re-export provider implementations
export { OpenAIEmbeddingProvider } from './providers/index.js';

/**
 * Create an embedding provider from configuration
 *
 * @param config - Provider configuration
 * @returns Configured embedding provider
 * @throws Error if provider type is not registered
 */
export function createEmbeddingProvider(config: EmbeddingConfig): EmbeddingProvider {
  const factory = getEmbeddingProviderFactory(config.provider);

  if (!factory) {
    const available = listEmbeddingProviders().join(', ');
    throw new Error(
      `Unknown embedding provider: "${config.provider}". Available providers: ${available}`
    );
  }

  return factory(config);
}

/**
 * Create an embedding provider from environment variables
 *
 * Environment variables:
 * - BRAIN_EMBEDDING_PROVIDER: Provider type (default: 'openai')
 * - BRAIN_EMBEDDING_MODEL: Model name (provider-specific default)
 * - OPENAI_API_KEY: API key for OpenAI
 * - OPENAI_BASE_URL: Custom base URL for OpenAI
 * - COHERE_API_KEY: API key for Cohere (when supported)
 *
 * @returns Configured embedding provider
 */
export function createEmbeddingProviderFromEnv(): EmbeddingProvider {
  const provider = (process.env.BRAIN_EMBEDDING_PROVIDER || 'openai') as EmbeddingProviderType;
  const model = process.env.BRAIN_EMBEDDING_MODEL;

  // Get API key based on provider
  let apiKey: string | undefined;
  let baseUrl: string | undefined;

  switch (provider) {
    case 'openai':
      apiKey = process.env.OPENAI_API_KEY;
      baseUrl = process.env.OPENAI_BASE_URL;
      break;
    case 'cohere':
      apiKey = process.env.COHERE_API_KEY;
      break;
    case 'voyager':
      apiKey = process.env.VOYAGER_API_KEY;
      break;
    case 'local':
      // Local provider may not need API key
      break;
    default:
      throw new Error(`Unknown provider: ${provider}`);
  }

  if (!apiKey && provider !== 'local') {
    throw new Error(
      `API key not found for provider "${provider}". ` +
        `Set the appropriate environment variable (e.g., OPENAI_API_KEY)`
    );
  }

  return createEmbeddingProvider({
    provider,
    apiKey,
    model,
    baseUrl,
  });
}

/**
 * Get default embedding dimension for a provider/model
 *
 * @param provider - Provider type
 * @param model - Model name (optional)
 * @returns Embedding dimension
 */
export function getEmbeddingDimension(
  provider: EmbeddingProviderType,
  model?: string
): number {
  // Default dimensions by provider
  const defaults: Record<EmbeddingProviderType, number> = {
    openai: 1536, // text-embedding-3-small
    cohere: 1024, // embed-english-v3.0
    voyager: 1024,
    local: 384, // all-MiniLM-L6-v2
  };

  // Model-specific dimensions for OpenAI
  if (provider === 'openai' && model) {
    const openaiDims: Record<string, number> = {
      'text-embedding-3-small': 1536,
      'text-embedding-3-large': 3072,
      'text-embedding-ada-002': 1536,
    };
    return openaiDims[model] || defaults.openai;
  }

  return defaults[provider] || 1536;
}
