import { defineConfig } from 'tsup';

export default defineConfig({
  entry: [
    'src/index.ts',
    'src/types/index.ts',
    'src/adapters/index.ts',
    'src/implementations/stagehand-browser.ts',
    'src/implementations/agent-core-browser.ts',
  ],
  format: ['esm'],
  dts: true,
  sourcemap: true,
  clean: true,
  splitting: false,
  external: [
    '@browserbasehq/stagehand',
    'playwright',
    'bedrock-agentcore',
    '@ai-sdk/amazon-bedrock',
    '@ai-sdk/openai',
    '@ai-sdk/anthropic',
    '@openrouter/ai-sdk-provider',
  ],
});
