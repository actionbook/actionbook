import type { EmbeddingProviderType, EmbeddingProviderFactory, EmbeddingConfig, EmbeddingProvider } from './types.js';
import { OpenAIEmbeddingProvider } from './providers/openai.js';

/**
 * Registry of embedding provider factories
 */
const embeddingProviderRegistry = new Map<EmbeddingProviderType, EmbeddingProviderFactory>();

/**
 * Register a provider factory
 */
export function registerEmbeddingProvider(
  type: EmbeddingProviderType,
  factory: EmbeddingProviderFactory
): void {
  embeddingProviderRegistry.set(type, factory);
}

/**
 * Get a provider factory by type
 */
export function getEmbeddingProviderFactory(
  type: EmbeddingProviderType
): EmbeddingProviderFactory | undefined {
  return embeddingProviderRegistry.get(type);
}

/**
 * List all registered provider types
 */
export function listEmbeddingProviders(): EmbeddingProviderType[] {
  return Array.from(embeddingProviderRegistry.keys());
}

/**
 * Check if a provider type is registered
 */
export function hasEmbeddingProvider(type: EmbeddingProviderType): boolean {
  return embeddingProviderRegistry.has(type);
}

// Register built-in providers
registerEmbeddingProvider('openai', (config: EmbeddingConfig): EmbeddingProvider => {
  return new OpenAIEmbeddingProvider(config);
});

// Placeholder for future providers
// registerEmbeddingProvider('cohere', (config) => new CohereEmbeddingProvider(config));
// registerEmbeddingProvider('voyager', (config) => new VoyagerEmbeddingProvider(config));
// registerEmbeddingProvider('local', (config) => new LocalEmbeddingProvider(config));
