import { DefaultAdapter } from './default.js';
import type { SiteAdapter, AdapterConfig } from './types.js';

/**
 * Adapter registry configuration
 * Maps source name to adapter config and class
 *
 * Note: Code adapters (like AirbnbAdapter) can be added here when needed.
 * For most sites, use YAML configuration in adapters/sites/*.yaml instead.
 */
export const ADAPTER_REGISTRY: Record<
  string,
  AdapterConfig & { adapter: SiteAdapter }
> = {
  // Example: Add code-based adapters here
  // airbnb: {
  //   name: 'Airbnb',
  //   description: 'Airbnb Help Center documentation',
  //   adapter: new AirbnbAdapter(),
  //   crawlConfig: { ... },
  // },
};

/**
 * Default adapter instance for fallback
 */
export const DEFAULT_ADAPTER = new DefaultAdapter();
