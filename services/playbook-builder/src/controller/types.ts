/**
 * Controller Types for Playbook Builder
 */

import type { BuildTaskConfig } from '@actionbookdev/db';

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
  sourceCategory: string;
  stage: string;
  stageStatus: string;
  config: BuildTaskConfig;
  createdAt: Date;
  updatedAt: Date;
  knowledgeStartedAt: Date | null;
  knowledgeCompletedAt: Date | null;
  actionStartedAt: Date | null;
  actionCompletedAt: Date | null;
}

/**
 * Processing result from PlaybookBuilder
 */
export interface ProcessingResult {
  playbookCount: number;
  sourceVersionId: number;
  playbookIds: number[];
  durationMs: number;
}

/**
 * Progress callback
 */
export interface ProgressInfo {
  phase: string;
  pagesProcessed: number;
  currentUrl?: string;
}

/**
 * Controller options
 */
export interface ControllerOptions {
  /** Poll interval in milliseconds (default: 30000) */
  pollInterval?: number;
  /** Task timeout in milliseconds (default: 0 = no timeout) */
  taskTimeout?: number;
  /** Heartbeat interval in milliseconds (default: 60000) */
  heartbeatInterval?: number;
  /** Max retries on failure (default: 3) */
  maxRetries?: number;
  /** Callback when task starts */
  onTaskStart?: (taskId: number) => void;
  /** Callback when task completes */
  onTaskComplete?: (taskId: number, result: ProcessingResult) => void;
  /** Callback when task fails */
  onTaskError?: (taskId: number, error: Error, retryCount: number) => void;
  /** Callback for progress updates */
  onProgress?: (taskId: number, progress: ProgressInfo) => void;
}

/**
 * Controller interface
 */
export interface PlaybookTaskController {
  /** Start polling for tasks */
  start(options?: ControllerOptions): Promise<void>;
  /** Stop the controller gracefully */
  stop(reason?: string): Promise<void>;
  /** Check for and execute one task */
  checkOnce(): Promise<boolean>;
  /** Get current state */
  getState(): ControllerState;
}
