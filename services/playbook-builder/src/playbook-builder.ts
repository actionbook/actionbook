/**
 * PlaybookBuilder - Main class for building playbooks
 *
 * Orchestrates the playbook building process:
 * 1. Page discovery - Find all pages on the website
 * 2. Page analysis - Analyze each page for basic info
 * 3. Capabilities discovery - Discover what each page can do
 * 4. Write to database - Save playbooks (document + chunk) with embeddings
 *
 * Each page produces one document with one chunk containing capability descriptions.
 */

import 'dotenv/config';

import { StagehandBrowser } from './browser/index.js';
import { AIClient, createEmbeddingProvider, type EmbeddingProvider } from './brain/index.js';
import { Storage, createStorage } from './storage/index.js';
import { log, fileLogger } from './utils/index.js';
import type {
  PlaybookBuilderConfig,
  PlaybookBuildResult,
  PageCapabilities,
} from './types/index.js';

// Import discoverers and analyzers
import { PageDiscoverer, CapabilitiesDiscoverer } from './discoverer/index.js';
import { PageAnalyzer } from './analyzer/index.js';

/**
 * PlaybookBuilder - Build playbooks for a website
 */
export class PlaybookBuilder {
  private config: Required<Omit<PlaybookBuilderConfig, 'llmProvider'>> & Pick<PlaybookBuilderConfig, 'llmProvider'>;
  private browser: StagehandBrowser;
  private ai: AIClient;
  private embedding: EmbeddingProvider | null = null;
  private storage: Storage;

  // Components
  private pageDiscoverer: PageDiscoverer;
  private pageAnalyzer: PageAnalyzer;
  private capabilitiesDiscoverer: CapabilitiesDiscoverer;

  constructor(config: PlaybookBuilderConfig) {
    this.config = {
      sourceId: config.sourceId,
      startUrl: config.startUrl,
      headless: config.headless ?? (process.env.HEADLESS === 'true'),
      maxPages: config.maxPages ?? 10,
      sourceVersionId: config.sourceVersionId ?? 0,
      llmProvider: config.llmProvider,
    };

    this.browser = new StagehandBrowser({ headless: this.config.headless });
    // AIClient: Use specified provider, env var, or auto-detect
    const llmProvider = this.config.llmProvider ||
      (process.env.LLM_PROVIDER as 'openrouter' | 'openai' | 'anthropic' | 'bedrock' | undefined);
    this.ai = new AIClient({ provider: llmProvider });
    log('info', `[PlaybookBuilder] LLM provider: ${this.ai.getProvider()}/${this.ai.getModel()}`);
    this.storage = createStorage();

    // Initialize embedding provider if OPENAI_API_KEY is available
    if (process.env.OPENAI_API_KEY) {
      try {
        this.embedding = createEmbeddingProvider({ provider: 'openai' });
        log('info', `[PlaybookBuilder] Embedding provider: openai/${this.embedding.model}`);
      } catch (error) {
        log('warn', '[PlaybookBuilder] Failed to initialize embedding provider:', error);
      }
    } else {
      log('warn', '[PlaybookBuilder] No OPENAI_API_KEY found, embedding generation disabled');
    }

    // Initialize components
    this.pageDiscoverer = new PageDiscoverer(this.ai);
    this.pageAnalyzer = new PageAnalyzer(this.ai);
    this.capabilitiesDiscoverer = new CapabilitiesDiscoverer(this.ai);
  }

  /**
   * Build playbooks for the configured website
   */
  async build(): Promise<PlaybookBuildResult> {
    // Initialize logging
    fileLogger.initialize('.', 'playbook-builder');
    log('info', `[PlaybookBuilder] Starting build for source ${this.config.sourceId}`);
    log('info', `[PlaybookBuilder] Start URL: ${this.config.startUrl}`);

    let sourceVersionId = this.config.sourceVersionId;

    try {
      // Initialize browser
      await this.browser.init();

      // Create or get source version
      if (!sourceVersionId) {
        const version = await this.storage.createVersion({
          sourceId: this.config.sourceId,
        });
        sourceVersionId = version.id;
      }

      // Step 1: Navigate to start URL and discover pages
      await this.browser.goto(this.config.startUrl);
      const screenshot = await this.browser.screenshot();
      const content = await this.browser.getContent();

      const discoveredPages = await this.pageDiscoverer.discover(screenshot, content, this.config.startUrl);
      log('info', `[PlaybookBuilder] Discovered ${discoveredPages.length} pages`);

      // Limit pages
      const pagesToProcess = discoveredPages.slice(0, this.config.maxPages);

      const playbookIds: number[] = [];

      // Step 2: Process each page
      for (const page of pagesToProcess) {
        log('info', `[PlaybookBuilder] Processing page: ${page.name} (${page.semanticId})`);

        try {
          // Navigate to the page
          await this.browser.goto(page.url);
          const pageScreenshot = await this.browser.screenshot();
          const pageContent = await this.browser.getContent();

          // Step 2a: Analyze page for basic info
          const analyzedPage = await this.pageAnalyzer.analyze(pageScreenshot, pageContent, page);
          log('info', `[PlaybookBuilder] Analyzed page: ${analyzedPage.name}`);

          // Step 2b: Discover page capabilities
          const capabilities = await this.capabilitiesDiscoverer.discover(
            pageScreenshot,
            pageContent,
            analyzedPage.name
          );
          log('info', `[PlaybookBuilder] Discovered ${capabilities.capabilities.length} capabilities`);

          // Step 2c: Build chunk content and generate embedding
          const chunkContent = this.buildChunkContent(analyzedPage.name, capabilities);
          let embedding: number[] | undefined;
          if (this.embedding) {
            try {
              const result = await this.embedding.embed(chunkContent);
              embedding = result.embedding;
              log('info', `[PlaybookBuilder] Generated embedding for ${analyzedPage.name}`);
            } catch (error) {
              log('warn', `[PlaybookBuilder] Failed to generate embedding:`, error);
            }
          }

          // Step 2d: Create playbook (document + chunk)
          const playbook = await this.storage.createPlaybook({
            sourceId: this.config.sourceId,
            sourceVersionId,
            url: this.browser.getUrl(),
            title: analyzedPage.name,
            description: analyzedPage.description,
            chunkContent,
            embedding,
            embeddingModel: embedding ? this.embedding?.model : undefined,
          });
          playbookIds.push(playbook.documentId);

        } catch (pageError) {
          log('error', `[PlaybookBuilder] Error processing page ${page.name}:`, pageError);
          // Continue with next page
        }
      }

      // Publish version
      await this.storage.publishVersion(sourceVersionId, this.config.sourceId);

      const result: PlaybookBuildResult = {
        playbookCount: playbookIds.length,
        sourceVersionId,
        playbookIds,
      };

      log('info', `[PlaybookBuilder] Build complete: ${result.playbookCount} playbooks`);
      return result;

    } finally {
      await this.browser.close();
      fileLogger.close();
    }
  }

  /**
   * Build chunk content from page capabilities
   * This content is stored in chunks.content and used for embedding/search
   * Focuses on capabilities and scenarios - element details are action-builder's job
   */
  private buildChunkContent(pageName: string, capabilities: PageCapabilities): string {
    const parts: string[] = [
      `# ${pageName}`,
      '',
      capabilities.description,
    ];

    // Capabilities as action phrases
    if (capabilities.capabilities.length > 0) {
      parts.push('');
      parts.push('## Capabilities');
      capabilities.capabilities.forEach((cap) => {
        parts.push(`- ${cap}`);
      });
    }

    // Functional areas
    if (capabilities.functionalAreas && capabilities.functionalAreas.length > 0) {
      parts.push('');
      parts.push('## Functional Areas');
      capabilities.functionalAreas.forEach((area) => {
        parts.push(`- ${area}`);
      });
    }

    // User scenarios/workflows
    if (capabilities.scenarios && capabilities.scenarios.length > 0) {
      parts.push('');
      parts.push('## Scenarios');
      capabilities.scenarios.forEach((scenario) => {
        parts.push('');
        parts.push(`### ${scenario.name}`);
        parts.push(`**Goal:** ${scenario.goal}`);
        parts.push('');
        parts.push('**Steps:**');
        scenario.steps.forEach((step, idx) => {
          parts.push(`${idx + 1}. ${step}`);
        });
        parts.push('');
        parts.push(`**Outcome:** ${scenario.outcome}`);
      });
    }

    // Prerequisites
    if (capabilities.prerequisites && capabilities.prerequisites.length > 0) {
      parts.push('');
      parts.push('## Prerequisites');
      capabilities.prerequisites.forEach((prereq) => {
        parts.push(`- ${prereq}`);
      });
    }

    return parts.join('\n');
  }
}
