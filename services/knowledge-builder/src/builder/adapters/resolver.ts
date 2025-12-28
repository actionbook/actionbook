import type { SiteAdapter, AdapterCrawlConfig } from './types.js';
import { ADAPTER_REGISTRY, DEFAULT_ADAPTER } from './config.js';
import { YamlAdapter } from './yaml-adapter.js';
import { SmartAdapter } from './smart-adapter.js';
import { loadYamlConfig, hasYamlConfig, listYamlConfigs } from './yaml-loader.js';

/**
 * Adapter resolution result
 */
export interface ResolvedAdapter {
  adapter: SiteAdapter;
  source: 'code' | 'yaml' | 'smart';
  crawlConfig?: AdapterCrawlConfig;
}

/**
 * AdapterResolver - Three-layer adapter resolution
 *
 * Priority:
 * 1. Code Adapter (highest) - Custom TypeScript adapters
 * 2. YAML Adapter - Configuration-driven adapters
 * 3. Smart Adapter (lowest) - Zero-config automatic extraction
 */
export class AdapterResolver {
  private smartAdapter: SmartAdapter;
  private configsDir?: string;

  constructor(configsDir?: string) {
    this.smartAdapter = new SmartAdapter();
    this.configsDir = configsDir;
  }

  /**
   * Resolve adapter by name or URL
   *
   * @param nameOrUrl - Adapter name or URL to resolve
   * @returns Resolved adapter with metadata
   */
  resolve(nameOrUrl: string): ResolvedAdapter {
    // If it looks like a URL, extract domain for lookup
    let name = nameOrUrl;
    if (nameOrUrl.startsWith('http://') || nameOrUrl.startsWith('https://')) {
      try {
        const url = new URL(nameOrUrl);
        // Try domain without www
        name = url.hostname.replace(/^www\./, '').split('.')[0];
      } catch {
        // Invalid URL, use as-is
      }
    }

    // Layer 1: Check Code Adapters (highest priority)
    const codeEntry = ADAPTER_REGISTRY[name];
    if (codeEntry) {
      console.log(`[AdapterResolver] Using Code Adapter: ${name}`);
      return {
        adapter: codeEntry.adapter,
        source: 'code',
        crawlConfig: codeEntry.crawlConfig,
      };
    }

    // Layer 2: Check YAML Configs
    if (hasYamlConfig(name, this.configsDir)) {
      const yamlConfig = loadYamlConfig(name, this.configsDir);
      if (yamlConfig) {
        console.log(`[AdapterResolver] Using YAML Adapter: ${name}`);
        const adapter = new YamlAdapter(yamlConfig);
        return {
          adapter,
          source: 'yaml',
          crawlConfig: adapter.getCrawlConfig(),
        };
      }
    }

    // Layer 3: Use Smart Adapter (zero-config fallback)
    console.log(`[AdapterResolver] Using Smart Adapter for: ${name}`);
    return {
      adapter: this.smartAdapter,
      source: 'smart',
    };
  }

  /**
   * Get adapter by name (backward compatible)
   */
  getAdapter(name: string): SiteAdapter {
    return this.resolve(name).adapter;
  }

  /**
   * List all available adapters
   */
  listAdapters(): Array<{
    name: string;
    type: 'code' | 'yaml' | 'smart';
    description: string;
  }> {
    const adapters: Array<{
      name: string;
      type: 'code' | 'yaml' | 'smart';
      description: string;
    }> = [];

    // Code adapters
    for (const [name, entry] of Object.entries(ADAPTER_REGISTRY)) {
      adapters.push({
        name,
        type: 'code',
        description: entry.description,
      });
    }

    // YAML adapters
    const yamlConfigs = listYamlConfigs(this.configsDir);
    for (const name of yamlConfigs) {
      // Skip if already have code adapter
      if (ADAPTER_REGISTRY[name]) continue;

      const config = loadYamlConfig(name, this.configsDir);
      if (config) {
        adapters.push({
          name,
          type: 'yaml',
          description: config.description,
        });
      }
    }

    // Smart adapter is always available but not listed
    // (it's the fallback, not a named option)

    return adapters;
  }

  /**
   * Check if an adapter exists (code or yaml)
   */
  hasAdapter(name: string): boolean {
    return name in ADAPTER_REGISTRY || hasYamlConfig(name, this.configsDir);
  }

  /**
   * Validate adapter name and return error if invalid
   * Returns null if valid, or uses smart adapter
   */
  validateAdapterName(name: string): string | null {
    // If adapter exists, it's valid
    if (this.hasAdapter(name)) {
      return null;
    }

    // Smart adapter will be used - return info message
    const available = this.listAdapters();
    if (available.length === 0) {
      return null; // No specific adapters, smart will be used
    }

    // Return warning that smart adapter will be used
    const adapterList = available
      .map((a) => `  - ${a.name} (${a.type}): ${a.description}`)
      .join('\n');

    return (
      `No specific adapter found for "${name}". Smart Adapter will be used.\n\n` +
      `Available adapters:\n${adapterList}`
    );
  }
}

/**
 * Default resolver instance
 */
let defaultResolver: AdapterResolver | null = null;

/**
 * Get or create the default resolver
 */
export function getResolver(configsDir?: string): AdapterResolver {
  if (!defaultResolver || configsDir) {
    defaultResolver = new AdapterResolver(configsDir);
  }
  return defaultResolver;
}

/**
 * Convenience function to resolve adapter
 */
export function resolveAdapter(nameOrUrl: string, configsDir?: string): ResolvedAdapter {
  return getResolver(configsDir).resolve(nameOrUrl);
}
