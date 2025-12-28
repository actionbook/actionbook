import { ADAPTER_REGISTRY, DEFAULT_ADAPTER } from './config.js';
import type { SiteAdapter, AdapterConfig, AdapterCrawlConfig } from './types.js';
import { AdapterResolver, getResolver, resolveAdapter } from './resolver.js';

// Re-export types
export type {
  SiteAdapter,
  ExtractedContent,
  AdapterConfig,
  AdapterCrawlConfig,
  YamlSiteConfig,
  StructuredData,
} from './types.js';

// Re-export adapters
export { DefaultAdapter } from './default.js';
export { YamlAdapter } from './yaml-adapter.js';
export { SmartAdapter } from './smart-adapter.js';

// Re-export resolver
export { AdapterResolver, getResolver, resolveAdapter } from './resolver.js';
export type { ResolvedAdapter } from './resolver.js';

// Re-export YAML loader utilities
export { loadYamlConfig, listYamlConfigs, hasYamlConfig } from './yaml-loader.js';

// Legacy exports for backward compatibility
export { ADAPTER_REGISTRY } from './config.js';

/**
 * Get all registered source names (code adapters only)
 * @deprecated Use AdapterResolver.listAdapters() for all adapters
 */
export function getRegisteredSources(): string[] {
  return Object.keys(ADAPTER_REGISTRY);
}

/**
 * Check if a source name is valid (code adapter)
 * @deprecated Use AdapterResolver.hasAdapter() for all adapters
 */
export function isValidSource(name: string): boolean {
  return name in ADAPTER_REGISTRY;
}

/**
 * Get adapter config by source name
 */
export function getAdapterConfig(name: string): AdapterConfig | undefined {
  const entry = ADAPTER_REGISTRY[name];
  return entry ? { name: entry.name, description: entry.description } : undefined;
}

/**
 * Get crawl config defaults for an adapter
 * Now supports both code and YAML adapters
 */
export function getAdapterCrawlConfig(name: string): AdapterCrawlConfig | undefined {
  // First check code adapters
  const codeEntry = ADAPTER_REGISTRY[name];
  if (codeEntry) {
    return codeEntry.crawlConfig;
  }

  // Then check YAML configs via resolver
  const resolved = resolveAdapter(name);
  return resolved.crawlConfig;
}

/**
 * Get adapter by source name
 * Now uses three-layer resolution: Code > YAML > Smart
 */
export function getAdapter(name: string): SiteAdapter {
  return resolveAdapter(name).adapter;
}

/**
 * Validate source name and return error message if invalid
 * Now returns warning instead of error (Smart Adapter will be used as fallback)
 */
export function validateSourceName(name: string): string | null {
  const resolver = getResolver();
  return resolver.validateAdapterName(name);
}

/**
 * List all available adapters (code + yaml)
 */
export function listAllAdapters() {
  return getResolver().listAdapters();
}
