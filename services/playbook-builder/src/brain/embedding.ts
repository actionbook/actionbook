/**
 * OpenAI Embedding Provider
 *
 * Generates embeddings using OpenAI's text-embedding models.
 */

import OpenAI from 'openai';
import { HttpsProxyAgent } from 'https-proxy-agent';
import { log } from '../utils/index.js';
import type { EmbeddingProvider, EmbeddingResult, EmbeddingConfig } from './types.js';

/**
 * OpenAI embedding dimension by model
 */
const MODEL_DIMENSIONS: Record<string, number> = {
  'text-embedding-3-small': 1536,
  'text-embedding-3-large': 3072,
  'text-embedding-ada-002': 1536,
};

const DEFAULT_MODEL = 'text-embedding-3-small';
const DEFAULT_BATCH_SIZE = 100;
const DEFAULT_MAX_TOKENS = 8000;

/**
 * OpenAI Embedding Provider
 */
export class OpenAIEmbeddingProvider implements EmbeddingProvider {
  readonly name = 'openai';
  readonly model: string;
  readonly dimension: number;

  private client: OpenAI;
  private batchSize: number;
  private maxTokens: number;

  constructor(config: EmbeddingConfig = { provider: 'openai' }) {
    const apiKey = config.apiKey || process.env.OPENAI_API_KEY;
    if (!apiKey) {
      throw new Error('[OpenAIEmbeddingProvider] API key is required. Set OPENAI_API_KEY or pass apiKey in config.');
    }

    this.model = config.model || DEFAULT_MODEL;
    this.dimension = MODEL_DIMENSIONS[this.model] || 1536;
    this.batchSize = (config.options?.batchSize as number) || DEFAULT_BATCH_SIZE;
    this.maxTokens = (config.options?.maxTokens as number) || DEFAULT_MAX_TOKENS;

    const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;

    if (proxyUrl) {
      log('info', `[OpenAIEmbeddingProvider] Using proxy: ${proxyUrl}`);
    }

    this.client = new OpenAI({
      apiKey,
      baseURL: config.baseUrl || process.env.OPENAI_BASE_URL,
      timeout: config.timeout || 60000,
      maxRetries: config.maxRetries || 3,
      httpAgent: proxyUrl ? new HttpsProxyAgent(proxyUrl) : undefined,
    });

    log('info', `[OpenAIEmbeddingProvider] Initialized with model: ${this.model} (${this.dimension} dim)`);
  }

  /**
   * Generate embedding for a single text
   */
  async embed(text: string): Promise<EmbeddingResult> {
    const trimmed = text.trim();
    if (!trimmed) {
      throw new Error('[OpenAIEmbeddingProvider] Cannot embed empty text');
    }

    this.checkTokenLimit(trimmed, 0);

    const response = await this.client.embeddings.create({
      model: this.model,
      input: trimmed,
    });

    return {
      embedding: response.data[0].embedding,
      tokenCount: response.usage.total_tokens,
    };
  }

  /**
   * Generate embeddings for multiple texts in batch
   */
  async embedBatch(texts: string[]): Promise<EmbeddingResult[]> {
    const validTexts = texts.map((t) => t.trim()).filter((t) => t.length > 0);
    if (validTexts.length === 0) return [];

    validTexts.forEach((text, i) => this.checkTokenLimit(text, i));

    log('info', `[OpenAIEmbeddingProvider] Processing ${validTexts.length} texts`);

    const results: EmbeddingResult[] = [];

    for (let i = 0; i < validTexts.length; i += this.batchSize) {
      const batch = validTexts.slice(i, i + this.batchSize);

      const response = await this.client.embeddings.create({
        model: this.model,
        input: batch,
      });

      const avgTokens = Math.ceil(response.usage.total_tokens / batch.length);

      for (const data of response.data) {
        results.push({
          embedding: data.embedding,
          tokenCount: avgTokens,
        });
      }

      if (validTexts.length > this.batchSize) {
        log('info', `[OpenAIEmbeddingProvider] Processed ${Math.min(i + this.batchSize, validTexts.length)}/${validTexts.length}`);
      }
    }

    return results;
  }

  /**
   * Check if text exceeds token limit
   */
  private checkTokenLimit(text: string, index: number): void {
    const estimatedTokens = Math.ceil(text.length / 4);
    if (estimatedTokens > this.maxTokens) {
      throw new Error(
        `[OpenAIEmbeddingProvider] Text at index ${index} exceeds token limit: ~${estimatedTokens} tokens (max: ${this.maxTokens})`
      );
    }
  }
}

/**
 * Create an embedding provider based on config
 */
export function createEmbeddingProvider(config?: EmbeddingConfig): EmbeddingProvider {
  const providerType = config?.provider || 'openai';

  switch (providerType) {
    case 'openai':
      return new OpenAIEmbeddingProvider(config);
    default:
      throw new Error(`Unknown embedding provider: ${providerType}`);
  }
}
