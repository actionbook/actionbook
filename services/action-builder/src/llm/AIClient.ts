import { generateText, tool as aiTool, jsonSchema, type LanguageModel } from 'ai';
import { createOpenAI } from '@ai-sdk/openai';
import { createAnthropic } from '@ai-sdk/anthropic';
import { createOpenRouter } from '@openrouter/ai-sdk-provider';
import { createAmazonBedrock } from '@ai-sdk/amazon-bedrock';
import type OpenAI from 'openai';
import { ProxyAgent, fetch as undiciFetch } from 'undici';
import { log } from '../utils/logger.js';

/**
 * LLM call metrics for observability
 */
export interface LLMMetrics {
  // Token statistics
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cacheCreationInputTokens?: number;
  cacheReadInputTokens?: number;
  reasoningTokens?: number;

  // Performance statistics
  e2eLatencyMs: number;      // End-to-end latency in milliseconds
  tps: number;               // Tokens per second (output tokens / e2e latency)

  // Request info
  provider: string;
  model: string;
  success: boolean;
  errorType?: string;        // 'rate_limit' | 'server_error' | 'timeout' | 'other'
}

/**
 * Create a fetch function with proxy support
 * Reads from HTTPS_PROXY or HTTP_PROXY environment variables
 */
function createProxyFetch(): typeof fetch | undefined {
  const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;
  if (!proxyUrl) {
    return undefined;
  }

  log('info', `[AIClient] Using proxy: ${proxyUrl}`);
  const proxyAgent = new ProxyAgent(proxyUrl);

  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    const response = await undiciFetch(url, {
      ...init,
      dispatcher: proxyAgent,
    } as Parameters<typeof undiciFetch>[1]);
    return response as unknown as Response;
  };
}

export type LLMProvider = 'openrouter' | 'openai' | 'anthropic' | 'bedrock';

export interface AIClientConfig {
  /** LLM provider. Auto-detected from API keys if not specified */
  provider?: LLMProvider;
  /** Model name. Uses provider-specific default if not specified */
  model?: string;
  /** API key. Auto-detected from environment variables if not specified */
  apiKey?: string;
}

interface ResolvedConfig {
  provider: LLMProvider;
  model: string;
  apiKey: string;
}

/**
 * AI Client using Vercel AI SDK with multi-provider support
 *
 * Supports three providers with auto-detection priority:
 * 1. OpenRouter (OPENROUTER_API_KEY) - recommended, access to all models
 * 2. OpenAI (OPENAI_API_KEY)
 * 3. Anthropic (ANTHROPIC_API_KEY)
 *
 * Returns OpenAI-compatible response format for backward compatibility.
 */
export class AIClient {
  private model: LanguageModel;
  private provider: LLMProvider;
  private modelName: string;

  constructor(config: AIClientConfig = {}) {
    const resolved = this.resolveConfig(config);
    this.provider = resolved.provider;
    this.modelName = resolved.model;
    this.model = this.createModel(resolved.provider, resolved.model, resolved.apiKey);

    log('info', `[AIClient] Initialized with ${resolved.provider}/${resolved.model}`);
  }

  /**
   * Auto-detect provider based on available API keys
   * Priority: OpenRouter > OpenAI > Anthropic > Bedrock
   */
  private resolveConfig(config: AIClientConfig): ResolvedConfig {
    const openrouterKey = process.env.OPENROUTER_API_KEY;
    const openaiKey = process.env.OPENAI_API_KEY;
    const anthropicKey = process.env.ANTHROPIC_API_KEY;
    // Bedrock uses AWS credentials (access key + secret key) or IAM role
    const bedrockAccessKey = process.env.AWS_ACCESS_KEY_ID;
    const bedrockSecretKey = process.env.AWS_SECRET_ACCESS_KEY;
    const hasBedrock = bedrockAccessKey && bedrockSecretKey;

    if (config.provider) {
      // Explicit provider specified
      if (config.provider === 'bedrock') {
        if (!hasBedrock) {
          throw new Error('AWS credentials not found for Bedrock. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY');
        }
        return {
          provider: 'bedrock',
          model: config.model || process.env.AWS_BEDROCK_MODEL || this.getDefaultModel('bedrock'),
          apiKey: 'bedrock', // Placeholder, Bedrock uses AWS credentials
        };
      }
      const keyMap: Record<Exclude<LLMProvider, 'bedrock'>, string | undefined> = {
        openrouter: openrouterKey,
        openai: openaiKey,
        anthropic: anthropicKey,
      };
      const apiKey = keyMap[config.provider] || config.apiKey;
      if (!apiKey) {
        throw new Error(`API key not found for provider: ${config.provider}`);
      }
      return {
        provider: config.provider,
        model: config.model || this.getDefaultModel(config.provider),
        apiKey,
      };
    }

    // Auto-detect based on available keys
    if (openrouterKey || config.apiKey) {
      return {
        provider: 'openrouter',
        model: config.model || process.env.OPENROUTER_MODEL || 'anthropic/claude-sonnet-4',
        apiKey: openrouterKey || config.apiKey!,
      };
    }
    if (openaiKey) {
      return {
        provider: 'openai',
        model: config.model || process.env.OPENAI_MODEL || 'gpt-4o',
        apiKey: openaiKey,
      };
    }
    if (anthropicKey) {
      return {
        provider: 'anthropic',
        model: config.model || process.env.ANTHROPIC_MODEL || 'claude-sonnet-4-5',
        apiKey: anthropicKey,
      };
    }
    if (hasBedrock) {
      return {
        provider: 'bedrock',
        model: config.model || process.env.AWS_BEDROCK_MODEL || 'anthropic.claude-3-5-sonnet-20241022-v2:0',
        apiKey: 'bedrock', // Placeholder
      };
    }

    throw new Error(
      'No LLM API key found. Set OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY, or AWS credentials for Bedrock'
    );
  }

  private getDefaultModel(provider: LLMProvider): string {
    const defaults: Record<LLMProvider, string> = {
      openrouter: 'anthropic/claude-sonnet-4',
      openai: 'gpt-4o',
      anthropic: 'claude-sonnet-4-5',
      bedrock: 'anthropic.claude-3-5-sonnet-20241022-v2:0',
    };
    return defaults[provider];
  }

  private createModel(provider: LLMProvider, model: string, apiKey: string): LanguageModel {
    // Create proxy fetch if HTTPS_PROXY or HTTP_PROXY is set
    const proxyFetch = createProxyFetch();

    switch (provider) {
      case 'openrouter': {
        const openrouter = createOpenRouter({
          apiKey,
          fetch: proxyFetch,
        });
        return openrouter(model);
      }
      case 'openai': {
        const openai = createOpenAI({
          apiKey,
          fetch: proxyFetch,
        });
        return openai(model);
      }
      case 'anthropic': {
        const anthropic = createAnthropic({
          apiKey,
          fetch: proxyFetch,
        });
        return anthropic(model);
      }
      case 'bedrock': {
        const bedrock = createAmazonBedrock({
          region: process.env.AWS_REGION || process.env.AWS_BEDROCK_REGION || 'us-east-1',
          accessKeyId: process.env.AWS_ACCESS_KEY_ID,
          secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY,
          sessionToken: process.env.AWS_SESSION_TOKEN,
          fetch: proxyFetch,
        });
        return bedrock(model);
      }
    }
  }

  /**
   * Convert OpenAI messages format to Vercel AI SDK format
   */
  private convertMessages(
    messages: OpenAI.Chat.Completions.ChatCompletionMessageParam[]
  ): Array<{ role: string; content: unknown }> {
    // Build a map of toolCallId -> toolName from assistant messages
    const toolCallIdToName: Map<string, string> = new Map();
    for (const msg of messages) {
      if (msg.role === 'assistant') {
        const assistantMsg = msg as OpenAI.Chat.Completions.ChatCompletionAssistantMessageParam;
        if (assistantMsg.tool_calls) {
          for (const tc of assistantMsg.tool_calls) {
            toolCallIdToName.set(tc.id, tc.function.name);
          }
        }
      }
    }

    return messages.map((msg) => {
      if (msg.role === 'system') {
        return { role: 'system', content: msg.content as string };
      }
      if (msg.role === 'user') {
        return { role: 'user', content: msg.content as string };
      }
      if (msg.role === 'assistant') {
        const assistantMsg = msg as OpenAI.Chat.Completions.ChatCompletionAssistantMessageParam;
        if (assistantMsg.tool_calls && assistantMsg.tool_calls.length > 0) {
          // Assistant message with tool calls
          const content: unknown[] = [];

          if (assistantMsg.content) {
            content.push({ type: 'text', text: assistantMsg.content as string });
          }

          for (const tc of assistantMsg.tool_calls) {
            // Parse args, handling empty or malformed arguments
            // AI SDK v5 uses 'input' instead of 'args' for tool calls
            let input: unknown = {};
            if (tc.function.arguments) {
              try {
                input = JSON.parse(tc.function.arguments);
              } catch {
                input = {};
              }
            }
            content.push({
              type: 'tool-call',
              toolCallId: tc.id,
              toolName: tc.function.name,
              input,
            });
          }

          return { role: 'assistant', content };
        }
        return { role: 'assistant', content: assistantMsg.content as string || '' };
      }
      if (msg.role === 'tool') {
        const toolMsg = msg as OpenAI.Chat.Completions.ChatCompletionToolMessageParam;
        // Parse the content as JSON if possible
        // AI SDK v5 requires 'output' to be a discriminated union with 'type' field
        // Valid types: { type: "text", value: string } | { type: "json", value: any } | etc.
        let output: { type: string; value: unknown };
        try {
          const parsed = JSON.parse(toolMsg.content as string);
          // Wrap in AI SDK v5 output format
          output = { type: 'json', value: parsed };
        } catch {
          // If not valid JSON, use text format
          output = { type: 'text', value: toolMsg.content as string };
        }
        // Get the tool name from our map
        const toolName = toolCallIdToName.get(toolMsg.tool_call_id) || 'unknown';
        return {
          role: 'tool',
          content: [
            {
              type: 'tool-result',
              toolCallId: toolMsg.tool_call_id,
              toolName,
              output,
            },
          ],
        };
      }
      // Fallback
      return { role: 'user', content: String((msg as unknown as Record<string, unknown>).content || '') };
    });
  }

  /**
   * Convert OpenAI tools format to Vercel AI SDK tools format
   * Uses jsonSchema() for direct JSON Schema passthrough
   */
  private convertTools(
    tools: OpenAI.Chat.Completions.ChatCompletionTool[]
  ): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    for (const tool of tools) {
      if (tool.type === 'function') {
        const fn = tool.function;
        // Use AI SDK's jsonSchema() for direct JSON Schema passthrough
        // This avoids Zod conversion issues
        result[fn.name] = aiTool({
          description: fn.description || '',
          inputSchema: jsonSchema(fn.parameters as Record<string, unknown>),
          // No execute - we want the tool calls returned, not executed
        });
      }
    }

    return result;
  }

  /**
   * Chat with tool calling support
   * Returns OpenAI-compatible response format for backward compatibility
   */
  async chat(
    messages: OpenAI.Chat.Completions.ChatCompletionMessageParam[],
    tools: OpenAI.Chat.Completions.ChatCompletionTool[]
  ): Promise<OpenAI.Chat.Completions.ChatCompletion> {
    const modelMessages = this.convertMessages(messages);
    const aiTools = this.convertTools(tools);

    const startTime = Date.now();
    let metrics: LLMMetrics;

    try {
      // Use AI SDK's built-in retry mechanism with exponential backoff
      // maxRetries: 3 means 4 total attempts (1 initial + 3 retries)
      // For rate limit errors, AI SDK will automatically use exponential backoff
      const result = await generateText({
        model: this.model,
        messages: modelMessages,
        tools: aiTools,
        maxOutputTokens: 4096,
        maxRetries: 5, // Increased from 3 to 5 for better rate limit handling
      } as Parameters<typeof generateText>[0]);

      const e2eLatencyMs = Date.now() - startTime;

      // Extract usage metrics from result
      const usage = result.usage as Record<string, unknown> | undefined;
      const inputTokens = (usage?.inputTokens ?? usage?.promptTokens ?? 0) as number;
      const outputTokens = (usage?.outputTokens ?? usage?.completionTokens ?? 0) as number;

      // Extract provider-specific metrics (cache tokens, reasoning tokens)
      const providerMetadata = (result as unknown as Record<string, unknown>).providerMetadata as Record<string, unknown> | undefined;
      const anthropicMeta = providerMetadata?.anthropic as Record<string, unknown> | undefined;
      const cacheCreationInputTokens = (anthropicMeta?.cacheCreationInputTokens ?? usage?.cacheCreationInputTokens) as number | undefined;
      const cacheReadInputTokens = (anthropicMeta?.cacheReadInputTokens ?? usage?.cacheReadInputTokens) as number | undefined;

      // Reasoning tokens (for o1/o3 models)
      const reasoningTokens = (usage?.reasoningTokens) as number | undefined;

      // Calculate TPS (tokens per second)
      const tps = e2eLatencyMs > 0 ? (outputTokens / (e2eLatencyMs / 1000)) : 0;

      metrics = {
        inputTokens,
        outputTokens,
        totalTokens: inputTokens + outputTokens,
        cacheCreationInputTokens,
        cacheReadInputTokens,
        reasoningTokens,
        e2eLatencyMs,
        tps: Math.round(tps * 10) / 10,
        provider: this.provider,
        model: this.modelName,
        success: true,
      };

      this.logMetrics(metrics);

      // Convert Vercel AI SDK result to OpenAI-compatible format
      return this.convertToOpenAIResponse(result as Awaited<ReturnType<typeof generateText>>);
    } catch (error) {
      const e2eLatencyMs = Date.now() - startTime;
      const errorType = this.classifyError(error);

      metrics = {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        e2eLatencyMs,
        tps: 0,
        provider: this.provider,
        model: this.modelName,
        success: false,
        errorType,
      };

      this.logMetrics(metrics);
      throw error;
    }
  }

  /**
   * Classify error type for metrics
   */
  private classifyError(error: unknown): string {
    const errorMsg = error instanceof Error ? error.message : String(error);
    const errorStr = errorMsg.toLowerCase();

    if (errorStr.includes('429') || errorStr.includes('rate limit')) {
      return 'rate_limit';
    }
    if (errorStr.includes('5') && errorStr.includes('00')) {
      return 'server_error';
    }
    if (errorStr.includes('timeout') || errorStr.includes('timed out')) {
      return 'timeout';
    }
    return 'other';
  }

  /**
   * Log LLM metrics in a structured format
   */
  private logMetrics(metrics: LLMMetrics): void {
    // Build token stats string
    const tokenParts = [
      `in=${metrics.inputTokens}`,
      `out=${metrics.outputTokens}`,
    ];
    if (metrics.cacheReadInputTokens) {
      tokenParts.push(`cache_read=${metrics.cacheReadInputTokens}`);
    }
    if (metrics.cacheCreationInputTokens) {
      tokenParts.push(`cache_create=${metrics.cacheCreationInputTokens}`);
    }
    if (metrics.reasoningTokens) {
      tokenParts.push(`reasoning=${metrics.reasoningTokens}`);
    }
    tokenParts.push(`total=${metrics.totalTokens}`);

    // Build perf stats string
    const perfParts = [
      `latency=${metrics.e2eLatencyMs}ms`,
      `tps=${metrics.tps}`,
    ];

    const status = metrics.success ? '✓' : `✗ ${metrics.errorType}`;

    log('info', `[LLM] ${status} | ${metrics.provider}/${metrics.model} | tokens: ${tokenParts.join(', ')} | perf: ${perfParts.join(', ')}`);
  }

  /**
   * Convert Vercel AI SDK result to OpenAI ChatCompletion format
   */
  private convertToOpenAIResponse(
    result: Awaited<ReturnType<typeof generateText>>
  ): OpenAI.Chat.Completions.ChatCompletion {
    // Extract tool calls from result - handle different property names
    const rawToolCalls = (result as unknown as Record<string, unknown>).toolCalls as Array<{
      toolCallId?: string;
      toolName: string;
      input?: unknown;
      args?: unknown;
    }> | undefined;

    const toolCalls = rawToolCalls?.map((tc, index) => ({
      id: tc.toolCallId || `call_${index}`,
      type: 'function' as const,
      function: {
        name: tc.toolName,
        // AI SDK v5 uses 'input', but some versions use 'args'
        arguments: JSON.stringify(tc.input ?? tc.args ?? {}),
      },
    }));

    const message: OpenAI.Chat.Completions.ChatCompletionMessage = {
      role: 'assistant',
      content: result.text || null,
      refusal: null,
      ...(toolCalls && toolCalls.length > 0 ? { tool_calls: toolCalls } : {}),
    };

    // Extract usage - handle different property names
    const usage = result.usage as Record<string, unknown> | undefined;
    const promptTokens = (usage?.inputTokens ?? usage?.promptTokens ?? 0) as number;
    const completionTokens = (usage?.outputTokens ?? usage?.completionTokens ?? 0) as number;

    return {
      id: `chatcmpl-${Date.now()}`,
      object: 'chat.completion',
      created: Math.floor(Date.now() / 1000),
      model: this.modelName,
      choices: [
        {
          index: 0,
          message,
          finish_reason: toolCalls && toolCalls.length > 0 ? 'tool_calls' : 'stop',
          logprobs: null,
        },
      ],
      usage: {
        prompt_tokens: promptTokens,
        completion_tokens: completionTokens,
        total_tokens: promptTokens + completionTokens,
      },
    };
  }

  /**
   * Get the current provider
   */
  getProvider(): LLMProvider {
    return this.provider;
  }

  /**
   * Get the current model name
   */
  getModel(): string {
    return this.modelName;
  }
}
