/**
 * Controller Layer - Type definitions
 *
 * Types for BuildTaskController that polls and executes knowledge-builder tasks
 */

import type { ProcessingProgress, ProcessingResult } from '../builder/index.js';

/**
 * Controller options for configuring the polling behavior
 */
export interface ControllerOptions {
  /** Polling interval in milliseconds (default: 30000) */
  pollInterval?: number;

  /** Task execution timeout in milliseconds (default: no timeout) */
  taskTimeout?: number;

  /** Heartbeat interval in milliseconds (default: 60000) */
  heartbeatInterval?: number;

  /** Maximum retry attempts for failed tasks (default: 3) */
  maxRetries?: number;

  /** Progress callback */
  onProgress?: (taskId: number, progress: ProcessingProgress) => void;

  /** Task started callback */
  onTaskStart?: (taskId: number) => void;

  /** Task completed callback */
  onTaskComplete?: (taskId: number, result: ProcessingResult) => void;

  /** Task error callback */
  onTaskError?: (taskId: number, error: Error, retryCount: number) => void;
}

/**
 * Controller state
 */
export type ControllerState = 'idle' | 'polling' | 'processing' | 'stopping' | 'stopped';

/**
 * Build task from database
 */
export interface BuildTask {
  id: number;
  sourceId: number | null;
  sourceUrl: string;
  sourceName: string | null;
  sourceCategory: 'help' | 'unknown' | 'any';
  stage: 'init' | 'knowledge_build' | 'action_build' | 'completed' | 'error';
  stageStatus: 'pending' | 'running' | 'completed' | 'error';
  config: BuildTaskConfig;
  createdAt: Date;
  updatedAt: Date;
  knowledgeStartedAt: Date | null;
  knowledgeCompletedAt: Date | null;
  actionStartedAt: Date | null;
  actionCompletedAt: Date | null;
}

/**
 * Build task configuration
 */
export interface BuildTaskConfig {
  /** Maximum number of pages to crawl */
  maxPages?: number;
  /** Crawl depth */
  maxDepth?: number;
  /** Include URL patterns */
  includePatterns?: string[];
  /** Exclude URL patterns */
  excludePatterns?: string[];
  /** Rate limit in milliseconds */
  rateLimit?: number;
  /** Retry count (managed by controller) */
  _retryCount?: number;
  /** Last error message (managed by controller) */
  _lastError?: string;
  /** Additional configuration */
  [key: string]: unknown;
}

/**
 * BuildTaskController interface
 */
export interface BuildTaskController {
  /**
   * Start the controller, begin polling for tasks
   * @param options - Controller options
   */
  start(options?: ControllerOptions): Promise<void>;

  /**
   * Stop the controller gracefully
   * If a task is currently running, it will be marked as error with the given reason
   * @param reason - Error message to record when stopping a running task
   */
  stop(reason?: string): Promise<void>;

  /**
   * Check for and execute one task immediately
   * Useful for testing or manual triggering
   */
  checkOnce(): Promise<boolean>;

  /**
   * Get current controller state
   */
  getState(): ControllerState;
}
