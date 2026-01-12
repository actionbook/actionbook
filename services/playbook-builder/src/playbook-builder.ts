/**
 * PlaybookBuilder - Main class for building playbooks
 *
 * Orchestrates the playbook building process:
 * 1. Page discovery - Find all pages on the website
 * 2. Playbook generation - Generate 7-section Playbook for each page
 * 3. Write to database - Save playbooks (document + chunk) with embeddings
 *
 * Each page produces one document with one chunk containing the full Playbook.
 */

import 'dotenv/config';

import { createBrowserAuto, type BrowserAdapter } from '@actionbookdev/browser';
import { AIClient, createEmbeddingProvider, type EmbeddingProvider } from './brain/index.js';
import { Storage, createStorage } from './storage/index.js';
import { log, fileLogger, normalizeUrl, isSameDomain, buildChunkContent } from './utils/index.js';
import type {
  PlaybookBuilderConfig,
  PlaybookBuildResult,
  DiscoveredPage,
} from './types/index.js';

// Import discoverers
import { PageDiscoverer, CapabilitiesDiscoverer } from './discoverer/index.js';

/**
 * PlaybookBuilder - Build playbooks for a website
 */
export class PlaybookBuilder {
  private config: Required<Omit<PlaybookBuilderConfig, 'llmProvider' | 'customPrompt'>> & Pick<PlaybookBuilderConfig, 'llmProvider' | 'customPrompt'>;
  private browser: BrowserAdapter;
  private ai: AIClient;
  private embedding: EmbeddingProvider | null = null;
  private storage: Storage;

  // Components
  private pageDiscoverer: PageDiscoverer;
  private capabilitiesDiscoverer: CapabilitiesDiscoverer;

  constructor(config: PlaybookBuilderConfig) {
    this.config = {
      sourceId: config.sourceId,
      startUrl: config.startUrl,
      headless: config.headless ?? (process.env.HEADLESS === 'true'),
      maxPages: config.maxPages ?? 10,
      maxDepth: config.maxDepth ?? 1,
      sourceVersionId: config.sourceVersionId ?? 0,
      llmProvider: config.llmProvider,
      customPrompt: config.customPrompt,
    };

    // Auto-detect browser: AgentCoreBrowser in AWS, StagehandBrowser locally
    this.browser = createBrowserAuto({ headless: this.config.headless });
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

    // Initialize components with optional custom prompt
    this.pageDiscoverer = new PageDiscoverer(this.ai, this.config.customPrompt);
    this.capabilitiesDiscoverer = new CapabilitiesDiscoverer(this.ai, this.config.customPrompt);
  }

  /**
   * Build playbooks for the configured website
   */
  async build(): Promise<PlaybookBuildResult> {
    // Initialize logging
    fileLogger.initialize('.', 'playbook-builder');
    log('info', `[PlaybookBuilder] Starting build for source ${this.config.sourceId}`);
    log('info', `[PlaybookBuilder] Start URL: ${this.config.startUrl}`);
    log('info', `[PlaybookBuilder] Config: maxPages=${this.config.maxPages}, maxDepth=${this.config.maxDepth}`);

    let sourceVersionId = this.config.sourceVersionId;

    try {
      // Initialize browser
      await this.browser.initialize();

      // Create or get source version
      if (!sourceVersionId) {
        const version = await this.storage.createVersion({
          sourceId: this.config.sourceId,
        });
        sourceVersionId = version.id;
      }

      // Discover pages with depth control
      const allPages = await this.discoverPagesRecursively();
      log('info', `[PlaybookBuilder] Total pages discovered: ${allPages.length}`);

      // Limit pages
      const pagesToProcess = allPages.slice(0, this.config.maxPages);
      log('info', `[PlaybookBuilder] Pages to process: ${pagesToProcess.length}`);

      const playbookIds: number[] = [];

      // Process each page
      for (const page of pagesToProcess) {
        log('info', `[PlaybookBuilder] Processing page: ${page.name} (depth=${page.depth}, ${page.semanticId})`);

        try {
          // Navigate to the page
          await this.browser.navigate(page.url);
          const pageScreenshot = await this.browser.screenshot();
          const pageContent = await this.browser.getContent();

          // Generate Playbook for the page (7-section format)
          const currentUrl = this.browser.getUrl();
          const capabilities = await this.capabilitiesDiscoverer.discover(
            pageScreenshot,
            pageContent,
            page.name,
            currentUrl
          );
          log('info', `[PlaybookBuilder] Generated Playbook with ${capabilities.capabilities.length} capabilities`);

          // Use playbook as chunk content (or fallback to buildChunkContent)
          const chunkContent = capabilities.playbook || buildChunkContent(page.name, capabilities);
          let embedding: number[] | undefined;
          if (this.embedding) {
            try {
              const result = await this.embedding.embed(chunkContent);
              embedding = result.embedding;
              log('info', `[PlaybookBuilder] Generated embedding for ${page.name}`);
            } catch (error) {
              log('warn', `[PlaybookBuilder] Failed to generate embedding:`, error);
            }
          }

          // Create playbook (document + chunk)
          const playbook = await this.storage.createPlaybook({
            sourceId: this.config.sourceId,
            sourceVersionId,
            url: this.browser.getUrl(),
            title: page.name,
            description: capabilities.description,
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

      // Note: publishVersion is NOT called here.
      // It should be called after action-builder completes the full pipeline:
      // init -> playbook builder -> action-builder -> completed (publish)

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
   * Recursively discover pages up to maxDepth
   * Uses BFS (breadth-first search) to explore pages level by level
   */
  private async discoverPagesRecursively(): Promise<DiscoveredPage[]> {
    const visitedUrls = new Set<string>();
    const allPages: DiscoveredPage[] = [];

    // Queue of pages to discover from: [url, depth]
    const queue: Array<{ url: string; depth: number }> = [
      { url: this.config.startUrl, depth: 0 }
    ];

    // Add startUrl as first page
    const startUrlNormalized = normalizeUrl(this.config.startUrl);
    visitedUrls.add(startUrlNormalized);

    // Create a page entry for startUrl
    allPages.push({
      url: this.config.startUrl,
      semanticId: 'start',
      name: 'Start Page',
      description: 'Starting page for exploration',
      depth: 0,
    });

    while (queue.length > 0 && allPages.length < this.config.maxPages) {
      const current = queue.shift()!;

      // Skip if we've reached max depth for discovery
      if (current.depth >= this.config.maxDepth) {
        continue;
      }

      log('info', `[PlaybookBuilder] Discovering pages from: ${current.url} (depth=${current.depth})`);

      try {
        // Navigate to the page
        await this.browser.navigate(current.url);
        const screenshot = await this.browser.screenshot();
        const content = await this.browser.getContent();

        // Discover pages from this page
        const discoveredPages = await this.pageDiscoverer.discover(screenshot, content, current.url);
        log('info', `[PlaybookBuilder] Found ${discoveredPages.length} pages at depth ${current.depth}`);

        // Add new pages to queue and results
        for (const page of discoveredPages) {
          const normalizedUrl = normalizeUrl(page.url);

          // Skip if already visited or external
          if (visitedUrls.has(normalizedUrl)) {
            continue;
          }

          // Skip external URLs
          if (!isSameDomain(page.url, this.config.startUrl)) {
            continue;
          }

          visitedUrls.add(normalizedUrl);

          const pageWithDepth: DiscoveredPage = {
            ...page,
            depth: current.depth + 1,
          };

          allPages.push(pageWithDepth);

          // Add to queue for further discovery if not at max depth
          if (current.depth + 1 < this.config.maxDepth) {
            queue.push({ url: page.url, depth: current.depth + 1 });
          }

          // Stop if we have enough pages
          if (allPages.length >= this.config.maxPages) {
            break;
          }
        }

      } catch (error) {
        log('error', `[PlaybookBuilder] Error discovering pages from ${current.url}:`, error);
        // Continue with next page in queue
      }
    }

    return allPages;
  }
}
