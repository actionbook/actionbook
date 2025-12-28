/**
 * Task Mapper - Convert BuildTask to ProcessorConfig
 *
 * Maps database BuildTask configuration to Processor configuration
 */

import type { ProcessorConfig } from '../builder/index.js';
import type { BuildTask } from './types.js';

/**
 * Default crawl configuration values
 */
const DEFAULT_CONFIG = {
  maxDepth: 3,
  rateLimit: 1000,
  maxPages: 500,
};

/**
 * Extract domain name from URL for use as source name
 */
function extractSourceName(url: string): string {
  try {
    const urlObj = new URL(url);
    // Remove 'www.' prefix if present
    return urlObj.hostname.replace(/^www\./, '');
  } catch {
    // Fallback: use URL as-is or generate a hash
    return url.replace(/[^a-zA-Z0-9]/g, '-').substring(0, 50);
  }
}

/**
 * Map BuildTask to ProcessorConfig
 *
 * @param task - The build task from database
 * @returns ProcessorConfig for the knowledge builder
 */
export function mapTaskToProcessorConfig(task: BuildTask): ProcessorConfig {
  const config = task.config || {};

  // Use sourceName from task, or extract from URL
  const sourceName = task.sourceName || extractSourceName(task.sourceUrl);

  // Extract typed values from config (which has index signature [key: string]: unknown)
  const maxDepth = typeof config.maxDepth === 'number' ? config.maxDepth : DEFAULT_CONFIG.maxDepth;
  const maxPages = typeof config.maxPages === 'number' ? config.maxPages : DEFAULT_CONFIG.maxPages;
  const rateLimit = typeof config.rateLimit === 'number' ? config.rateLimit : DEFAULT_CONFIG.rateLimit;
  const includePatterns = Array.isArray(config.includePatterns) ? config.includePatterns as string[] : [];
  const excludePatterns = Array.isArray(config.excludePatterns) ? config.excludePatterns as string[] : [];
  const urls = Array.isArray(config.urls) ? config.urls as string[] : undefined;

  return {
    sourceName,
    baseUrl: task.sourceUrl,
    crawlConfig: {
      maxDepth,
      maxPages,
      includePatterns,
      excludePatterns,
      rateLimit,
      urls,
    },
    skipEmbeddings: false,
    versionOptions: {
      commitMessage: `Auto-build from task #${task.id}`,
      createdBy: 'build-task-controller',
      autoPublish: false,
    },
  };
}

/**
 * Validate that a task has the required fields for processing
 *
 * @param task - The build task to validate
 * @returns Error message if invalid, null if valid
 */
export function validateTask(task: BuildTask): string | null {
  if (!task.sourceUrl) {
    return 'Task is missing sourceUrl';
  }

  try {
    new URL(task.sourceUrl);
  } catch {
    return `Invalid sourceUrl: ${task.sourceUrl}`;
  }

  if (task.sourceCategory !== 'help') {
    return `Unsupported sourceCategory: ${task.sourceCategory}`;
  }

  return null;
}
