import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import YAML from 'yaml';
import type { YamlSiteConfig } from './types.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/**
 * Default configs directory (relative to package root)
 */
const DEFAULT_CONFIGS_DIR = path.resolve(__dirname, '../../../configs/sites');

/**
 * Cache for loaded YAML configs
 */
const configCache = new Map<string, YamlSiteConfig>();

/**
 * Load a YAML site config by name
 */
export function loadYamlConfig(name: string, configsDir?: string): YamlSiteConfig | null {
  // Check cache first
  const cacheKey = `${configsDir || 'default'}:${name}`;
  if (configCache.has(cacheKey)) {
    return configCache.get(cacheKey)!;
  }

  const dir = configsDir || DEFAULT_CONFIGS_DIR;
  const filePath = path.join(dir, `${name}.yaml`);

  // Try .yaml first, then .yml
  let actualPath = filePath;
  if (!fs.existsSync(actualPath)) {
    actualPath = path.join(dir, `${name}.yml`);
  }

  if (!fs.existsSync(actualPath)) {
    return null;
  }

  try {
    const content = fs.readFileSync(actualPath, 'utf-8');
    const config = YAML.parse(content) as YamlSiteConfig;

    // Validate required fields
    if (!config.name || !config.displayName) {
      console.warn(`[YamlLoader] Invalid config ${name}: missing required fields`);
      return null;
    }

    // Cache the config
    configCache.set(cacheKey, config);

    return config;
  } catch (error) {
    console.error(`[YamlLoader] Failed to load config ${name}:`, error);
    return null;
  }
}

/**
 * List all available YAML configs
 */
export function listYamlConfigs(configsDir?: string): string[] {
  const dir = configsDir || DEFAULT_CONFIGS_DIR;

  if (!fs.existsSync(dir)) {
    return [];
  }

  try {
    const files = fs.readdirSync(dir);
    return files
      .filter((f) => f.endsWith('.yaml') || f.endsWith('.yml'))
      .map((f) => f.replace(/\.ya?ml$/, ''));
  } catch {
    return [];
  }
}

/**
 * Check if a YAML config exists
 */
export function hasYamlConfig(name: string, configsDir?: string): boolean {
  const dir = configsDir || DEFAULT_CONFIGS_DIR;
  return (
    fs.existsSync(path.join(dir, `${name}.yaml`)) ||
    fs.existsSync(path.join(dir, `${name}.yml`))
  );
}

/**
 * Clear the config cache
 */
export function clearConfigCache(): void {
  configCache.clear();
}
