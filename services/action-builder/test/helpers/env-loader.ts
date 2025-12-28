/**
 * Shared environment loader for e2e tests
 *
 * Loads environment variables from .env file and validates required API keys.
 * Supports multi-provider auto-detection: OpenRouter > OpenAI > Anthropic > Bedrock
 */

import * as fs from 'fs';
import * as path from 'path';

/**
 * Load environment variables from .env file
 * Priority: process.cwd()/.env > services/action-builder/.env
 */
export function loadEnv(): void {
  const possiblePaths = [
    path.resolve(process.cwd(), '.env'),
    path.resolve(process.cwd(), '../.env'),
  ];

  for (const envPath of possiblePaths) {
    if (fs.existsSync(envPath)) {
      const envContent = fs.readFileSync(envPath, 'utf-8');
      for (const line of envContent.split('\n')) {
        // Skip comments and empty lines
        if (line.startsWith('#') || !line.trim()) continue;

        const match = line.match(/^([^=]+)=(.*)$/);
        if (match && !process.env[match[1]]) {
          process.env[match[1]] = match[2];
        }
      }
      console.log(`Loaded environment from: ${envPath}`);
      return;
    }
  }

  console.log('No .env file found, using existing environment variables');
}

/**
 * Check if at least one LLM API key is available
 * Priority: OpenRouter > OpenAI > Anthropic > Bedrock
 */
export function hasLLMApiKey(): boolean {
  const hasBedrock = !!(
    (process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY) ||
    process.env.AWS_BEARER_TOKEN_BEDROCK
  );
  return !!(
    process.env.OPENROUTER_API_KEY ||
    process.env.OPENAI_API_KEY ||
    process.env.ANTHROPIC_API_KEY ||
    hasBedrock
  );
}

/**
 * Get detected LLM provider info for logging
 */
export function getDetectedProvider(): { provider: string; model: string } {
  if (process.env.OPENROUTER_API_KEY) {
    return {
      provider: 'OpenRouter',
      model: process.env.OPENROUTER_MODEL || 'anthropic/claude-sonnet-4',
    };
  }
  if (process.env.OPENAI_API_KEY) {
    return {
      provider: 'OpenAI',
      model: process.env.OPENAI_MODEL || 'gpt-4o',
    };
  }
  if (process.env.ANTHROPIC_API_KEY) {
    return {
      provider: 'Anthropic',
      model: process.env.ANTHROPIC_MODEL || 'claude-sonnet-4-5',
    };
  }
  const hasBedrock = !!(
    (process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY) ||
    process.env.AWS_BEARER_TOKEN_BEDROCK
  );
  if (hasBedrock) {
    return {
      provider: 'Bedrock',
      model: process.env.AWS_BEDROCK_MODEL || 'anthropic.claude-3-5-sonnet-20241022-v2:0',
    };
  }
  return { provider: 'none', model: 'none' };
}

/**
 * Validate required environment and exit if missing
 */
export function requireLLMApiKey(): void {
  if (!hasLLMApiKey()) {
    console.error('Error: No LLM API key found.');
    console.error('Set one of: OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY, or AWS credentials for Bedrock');
    process.exit(1);
  }

  const { provider, model } = getDetectedProvider();
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);
}

/**
 * Get Stagehand provider info
 * Stagehand also auto-detects from the same API keys
 */
export function getStagehandInfo(): string {
  const model = process.env.STAGEHAND_MODEL;
  if (process.env.OPENROUTER_API_KEY) {
    return `OpenRouter (${model || 'gpt-4o'})`;
  }
  if (process.env.OPENAI_API_KEY) {
    return `OpenAI (${model || 'gpt-4o'})`;
  }
  const hasBedrock = !!(
    (process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY) ||
    process.env.AWS_BEARER_TOKEN_BEDROCK
  );
  if (hasBedrock) {
    return `Bedrock (${model || process.env.AWS_BEDROCK_MODEL || 'anthropic.claude-3-5-sonnet-20241022-v2:0'})`;
  }
  if (process.env.ANTHROPIC_API_KEY) {
    return `Anthropic (${model || 'claude-sonnet-4-20250514'})`;
  }
  return 'No API key';
}
