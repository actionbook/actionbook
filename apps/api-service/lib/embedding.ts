import OpenAI from 'openai';
import { HttpsProxyAgent } from 'https-proxy-agent';

let openai: OpenAI | null = null;

function getOpenAI(): OpenAI {
  if (!openai) {
    const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;

    openai = new OpenAI({
      apiKey: process.env.OPENAI_API_KEY,
      baseURL: process.env.OPENAI_BASE_URL,
      timeout: 60000,
      maxRetries: 3,
      httpAgent: proxyUrl ? new HttpsProxyAgent(proxyUrl) : undefined,
    });
  }
  return openai;
}

/**
 * Get embedding vector for text using OpenAI API (or OpenRouter)
 */
export async function getEmbedding(text: string): Promise<number[]> {
  const client = getOpenAI();

  try {
    const response = await client.embeddings.create({
      model: process.env.EMBEDDING_MODEL || 'text-embedding-3-small',
      input: text,
    });

    if (!response.data || !response.data[0] || !response.data[0].embedding) {
      throw new Error('Invalid embedding response format');
    }

    return response.data[0].embedding;
  } catch (error) {
    console.error('Embedding error:', error);
    throw new Error(`Failed to generate embedding: ${error instanceof Error ? error.message : 'Unknown error'}`);
  }
}
