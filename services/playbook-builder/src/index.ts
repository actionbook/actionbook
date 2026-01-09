/**
 * Playbook Builder - LLM-powered page exploration and capability discovery
 *
 * @example
 * ```typescript
 * import { PlaybookBuilder } from '@actionbookdev/playbook-builder';
 *
 * const builder = new PlaybookBuilder({
 *   sourceId: 1,
 *   startUrl: 'https://airbnb.com',
 * });
 *
 * const result = await builder.build();
 * console.log(`Created ${result.playbookCount} playbooks`);
 * ```
 */

// Main class
export { PlaybookBuilder } from './playbook-builder.js';

// Types
export type {
  PlaybookBuilderConfig,
  PlaybookBuildResult,
  DiscoveredPage,
  AnalyzedPage,
  PageCapabilities,
  UserScenario,
} from './types/index.js';

// Browser utilities (re-exported from shared packages)
export { createBrowserAuto, StagehandBrowser, AgentCoreBrowser } from '@actionbookdev/browser';
export type { BrowserAdapter } from '@actionbookdev/browser';
export { BrowserProfileManager } from '@actionbookdev/browser-profile';

// Brain (AI capabilities)
export { AIClient, type AIClientConfig, type LLMProvider } from './brain/index.js';
export {
  OpenAIEmbeddingProvider,
  createEmbeddingProvider,
  type EmbeddingProvider,
  type EmbeddingResult,
  type EmbeddingConfig,
} from './brain/index.js';

// Storage
export { Storage, createStorage } from './storage/index.js';
export type {
  CreatePlaybookInput,
  CreateVersionInput,
} from './storage/index.js';

// Logging
export { log, fileLogger } from './utils/index.js';

// Controller
export {
  createPlaybookTaskController,
  PlaybookTaskControllerImpl,
} from './controller/index.js';
export type {
  PlaybookTaskController,
  ControllerOptions,
  ControllerState,
} from './controller/index.js';
