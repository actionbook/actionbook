import OpenAI from 'openai';
import { HttpsProxyAgent } from 'https-proxy-agent';
import type { EmbeddingProvider, EmbeddingResult, EmbeddingConfig } from '../types.js';

/**
 * OpenAI embedding dimension by model
 */
const MODEL_DIMENSIONS: Record<string, number> = {
  'text-embedding-3-small': 1536,
  'text-embedding-3-large': 3072,
  'text-embedding-ada-002': 1536,
};

const DEFAULT_MODEL = 'text-embedding-3-small';
const DEFAULT_BATCH_SIZE = 100; // OpenAI supports up to 2048
const DEFAULT_MAX_TOKENS = 8000;

/**
 * OpenAI Embedding Provider
 *
 * Supports models:
 * - text-embedding-3-small (1536 dim, recommended)
 * - text-embedding-3-large (3072 dim)
 * - text-embedding-ada-002 (1536 dim, legacy)
 */
export class OpenAIEmbeddingProvider implements EmbeddingProvider {
  readonly name = 'openai';
  readonly model: string;
  readonly dimension: number;

  private client: OpenAI;
  private batchSize: number;
  private maxTokens: number;

  constructor(config: EmbeddingConfig) {
    if (!config.apiKey) {
      throw new Error('[OpenAIEmbeddingProvider] API key is required');
    }

    this.model = config.model || DEFAULT_MODEL;
    this.dimension = MODEL_DIMENSIONS[this.model] || 1536;
    this.batchSize = (config.options?.batchSize as number) || DEFAULT_BATCH_SIZE;
    this.maxTokens = (config.options?.maxTokens as number) || DEFAULT_MAX_TOKENS;

    const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;

    if (proxyUrl) {
      console.log(`[OpenAIEmbeddingProvider] Using proxy: ${proxyUrl}`);
    }
    if (config.baseUrl) {
      console.log(`[OpenAIEmbeddingProvider] Using custom baseURL: ${config.baseUrl}`);
    }

    this.client = new OpenAI({
      apiKey: config.apiKey,
      baseURL: config.baseUrl || process.env.OPENAI_BASE_URL,
      timeout: config.timeout || 60000,
      maxRetries: config.maxRetries || 3,
      httpAgent: proxyUrl ? new HttpsProxyAgent(proxyUrl) : undefined,
    });

    console.log(`[OpenAIEmbeddingProvider] Initialized with model: ${this.model} (${this.dimension} dim)`);
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
    // Filter and validate texts
    const validTexts = texts.map((t) => t.trim()).filter((t) => t.length > 0);
    if (validTexts.length === 0) return [];

    // Check token limits
    validTexts.forEach((text, i) => this.checkTokenLimit(text, i));

    console.log(
      `[OpenAIEmbeddingProvider] Processing ${validTexts.length} texts (filtered from ${texts.length})`
    );

    const results: EmbeddingResult[] = [];

    // Process in batches
    for (let i = 0; i < validTexts.length; i += this.batchSize) {
      const batch = validTexts.slice(i, i + this.batchSize);

      const response = await this.client.embeddings.create({
        model: this.model,
        input: batch,
      });

      // Calculate average tokens per text
      const avgTokens = Math.ceil(response.usage.total_tokens / batch.length);

      for (const data of response.data) {
        results.push({
          embedding: data.embedding,
          tokenCount: avgTokens,
        });
      }

      // Log progress for large batches
      if (validTexts.length > this.batchSize) {
        console.log(
          `[OpenAIEmbeddingProvider] Processed ${Math.min(i + this.batchSize, validTexts.length)}/${validTexts.length}`
        );
      }
    }

    return results;
  }

  /**
   * Check if text exceeds token limit
   */
  private checkTokenLimit(text: string, index: number): void {
    // Rough estimation: ~4 chars per token
    const estimatedTokens = Math.ceil(text.length / 4);
    if (estimatedTokens > this.maxTokens) {
      throw new Error(
        `[OpenAIEmbeddingProvider] Text at index ${index} exceeds token limit: ~${estimatedTokens} tokens (max: ${this.maxTokens}). ` +
          `Text preview: "${text.slice(0, 100)}..."`
      );
    }
  }
}
