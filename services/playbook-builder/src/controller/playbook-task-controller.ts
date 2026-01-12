/**
 * PlaybookTaskController - Polls and executes playbook-builder tasks
 *
 * Features:
 * - Polls database for tasks ready for playbook building
 *   (stage='init' or 'knowledge_build', stageStatus='pending')
 * - Claims tasks with optimistic locking to prevent duplicate execution
 * - Executes playbook-builder pipeline
 * - Updates task status with heartbeat mechanism
 * - Supports retry on failure
 */

import {
  getDb,
  buildTasks,
  sources,
  eq,
  and,
  or,
  sql,
  type BuildTask as DbBuildTask,
} from '@actionbookdev/db';
import { PlaybookBuilder } from '../playbook-builder.js';
import { log } from '../utils/index.js';
import type {
  PlaybookTaskController as IPlaybookTaskController,
  ControllerOptions,
  ControllerState,
  BuildTask,
  ProcessingResult,
} from './types.js';

/**
 * Default controller options
 */
const DEFAULT_OPTIONS: Required<
  Omit<
    ControllerOptions,
    'onProgress' | 'onTaskStart' | 'onTaskComplete' | 'onTaskError'
  >
> = {
  pollInterval: 30000, // 30 seconds
  taskTimeout: 0, // No timeout
  heartbeatInterval: 60000, // 1 minute
  maxRetries: 3,
};

/**
 * PlaybookTaskController implementation
 */
export class PlaybookTaskControllerImpl implements IPlaybookTaskController {
  private state: ControllerState = 'idle';
  private options: Required<
    Omit<
      ControllerOptions,
      'onProgress' | 'onTaskStart' | 'onTaskComplete' | 'onTaskError'
    >
  > &
    Pick<
      ControllerOptions,
      'onProgress' | 'onTaskStart' | 'onTaskComplete' | 'onTaskError'
    >;
  private pollTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private currentTaskId: number | null = null;
  private currentBuilder: PlaybookBuilder | null = null;
  private stopPromiseResolve: (() => void) | null = null;

  constructor() {
    this.options = { ...DEFAULT_OPTIONS };
  }

  // ============================================================================
  // Public Interface
  // ============================================================================

  async start(options?: ControllerOptions): Promise<void> {
    if (this.state !== 'idle' && this.state !== 'stopped') {
      throw new Error(`Cannot start controller in state: ${this.state}`);
    }

    this.options = { ...DEFAULT_OPTIONS, ...options };
    this.state = 'polling';

    log('info', '[Controller] Starting with options:', {
      pollInterval: this.options.pollInterval,
      heartbeatInterval: this.options.heartbeatInterval,
      maxRetries: this.options.maxRetries,
    });

    // Start polling loop
    await this.poll();
  }

  async stop(reason?: string): Promise<void> {
    if (this.state === 'stopped' || this.state === 'idle') {
      return;
    }

    log('info', '[Controller] Stopping...');
    this.state = 'stopping';

    // Clear poll timer
    if (this.pollTimer) {
      clearTimeout(this.pollTimer);
      this.pollTimer = null;
    }

    // If currently processing, mark task as error and stop
    if (this.currentTaskId !== null) {
      const taskId = this.currentTaskId;
      const errorMessage = reason || 'Task stopped manually';

      log('info', `[Controller] Stopping task #${taskId}: ${errorMessage}`);

      // Update task status to error
      await this.updateTaskStopped(taskId, errorMessage);

      // Wait for the processing to finish
      await new Promise<void>((resolve) => {
        this.stopPromiseResolve = resolve;
      });
    }

    // Clear heartbeat timer
    this.clearHeartbeat();

    this.state = 'stopped';
    log('info', '[Controller] Stopped');
  }

  async checkOnce(): Promise<boolean> {
    const task = await this.pollTask();
    if (!task) {
      return false;
    }

    await this.executeTask(task);
    return true;
  }

  getState(): ControllerState {
    return this.state;
  }

  // ============================================================================
  // Polling Logic
  // ============================================================================

  private async poll(): Promise<void> {
    while (this.state === 'polling') {
      try {
        log('info', '[Controller] Polling for pending playbook tasks...');
        const task = await this.pollTask();

        if (task) {
          log('info', `[Controller] Found task #${task.id}: ${task.sourceUrl}`);
          await this.executeTask(task);
        } else {
          log('info', '[Controller] No pending tasks found');
        }
      } catch (error) {
        log('error', '[Controller] Poll error:', error);
      }

      // Schedule next poll if still running
      if (this.state === 'polling') {
        log('info', `[Controller] Next poll in ${this.options.pollInterval / 1000}s`);
        await this.sleep(this.options.pollInterval);
      }
    }
  }

  private async pollTask(): Promise<BuildTask | null> {
    const db = getDb();

    // Poll for:
    // 1. New tasks: stage='init', stageStatus='pending'
    // 2. Retry tasks: stage='knowledge_build', stageStatus='pending' (set by handleTaskError for retry)
    const tasks = await db
      .select()
      .from(buildTasks)
      .where(
        sql`(
          (
            ${buildTasks.stage} = 'init'
            AND ${buildTasks.stageStatus} = 'pending'
          )
          OR (
            ${buildTasks.stage} = 'knowledge_build'
            AND ${buildTasks.stageStatus} = 'pending'
          )
        )`
      )
      .orderBy(buildTasks.createdAt)
      .limit(1);

    if (tasks.length === 0) {
      return null;
    }

    return this.mapDbTask(tasks[0]);
  }

  // ============================================================================
  // Task Execution
  // ============================================================================

  private async executeTask(task: BuildTask): Promise<void> {
    // Try to claim the task first (optimistic lock to prevent race conditions)
    // This must happen BEFORE findOrCreateSource to avoid multiple workers
    // racing to create/update sources for the same task
    const claimed = await this.claimTask(task.id);
    if (!claimed) {
      log('info', `[Controller] Task #${task.id} already claimed by another worker`);
      return;
    }

    log('info', `[Controller] Claimed task #${task.id}: ${task.sourceUrl}`);

    // Now that we've claimed the task, safely find or create source
    if (!task.sourceId) {
      log('info', `[Controller] Task #${task.id} has no sourceId, trying to find or create source...`);
      const sourceId = await this.findOrCreateSource(task);
      if (!sourceId) {
        log('error', `[Controller] Task #${task.id} failed to find or create source`);
        await this.updateTaskError(task.id, new Error('Failed to find or create source'), 0);
        return;
      }
      task.sourceId = sourceId;
      log('info', `[Controller] Task #${task.id} assigned sourceId: ${sourceId}`);
    }
    this.state = 'processing';
    this.currentTaskId = task.id;

    // Start heartbeat
    this.startHeartbeat(task.id);

    // Notify task start
    this.options.onTaskStart?.(task.id);

    const startTime = Date.now();

    try {
      // Create PlaybookBuilder
      const config = task.config;
      this.currentBuilder = new PlaybookBuilder({
        sourceId: task.sourceId,
        startUrl: task.sourceUrl,
        headless: config.playbookHeadless ?? true,
        maxPages: config.playbookMaxPages ?? 10,
        maxDepth: config.playbookMaxDepth ?? 1,
        customPrompt: config.playbookBuilderPrompt,
      });

      log('info', `[Controller] Processing task #${task.id} with config:`, {
        sourceId: task.sourceId,
        startUrl: task.sourceUrl,
        maxPages: config.playbookMaxPages ?? 10,
        maxDepth: config.playbookMaxDepth ?? 1,
      });

      // Execute playbook builder
      const buildResult = await this.currentBuilder.build();

      const result: ProcessingResult = {
        playbookCount: buildResult.playbookCount,
        sourceVersionId: buildResult.sourceVersionId,
        playbookIds: buildResult.playbookIds,
        durationMs: Date.now() - startTime,
      };

      // Update task as success
      await this.updateTaskSuccess(task.id, result);
      this.options.onTaskComplete?.(task.id, result);

      log('info', `[Controller] Task #${task.id} completed:`, {
        playbookCount: result.playbookCount,
        durationMs: result.durationMs,
      });
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      const retryCount = ((task.config as Record<string, unknown>)._retryCount as number ?? 0) + 1;

      log('error', `[Controller] Task #${task.id} failed (attempt ${retryCount}/${this.options.maxRetries}):`, err.message);

      await this.handleTaskError(task.id, err, retryCount);
      this.options.onTaskError?.(task.id, err, retryCount);
    } finally {
      this.clearHeartbeat();
      this.currentTaskId = null;
      this.currentBuilder = null;

      // If we were stopping, signal completion
      const currentState = this.getState();
      if (currentState === 'stopping' && this.stopPromiseResolve) {
        this.stopPromiseResolve();
        this.stopPromiseResolve = null;
      } else if (currentState === 'processing') {
        this.state = 'polling';
      }
    }
  }

  // ============================================================================
  // Task State Management
  // ============================================================================

  private async claimTask(taskId: number): Promise<boolean> {
    const db = getDb();

    // Optimistic lock: only update if status is 'pending'
    const result = await db
      .update(buildTasks)
      .set({
        stage: 'knowledge_build',
        stageStatus: 'running',
        knowledgeStartedAt: new Date(),
        updatedAt: new Date(),
      })
      .where(
        and(
          eq(buildTasks.id, taskId),
          eq(buildTasks.stageStatus, 'pending')
        )
      )
      .returning({ id: buildTasks.id });

    return result.length > 0;
  }

  private async updateTaskSuccess(
    taskId: number,
    _result: ProcessingResult
  ): Promise<void> {
    const db = getDb();

    await db
      .update(buildTasks)
      .set({
        stage: 'knowledge_build',
        stageStatus: 'completed',
        knowledgeCompletedAt: new Date(),
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, taskId));
  }

  private async handleTaskError(
    taskId: number,
    error: Error,
    retryCount: number
  ): Promise<void> {
    const db = getDb();

    if (retryCount < this.options.maxRetries) {
      // Reset to pending for retry
      await db
        .update(buildTasks)
        .set({
          stage: 'knowledge_build',
          stageStatus: 'pending',
          config: sql`${buildTasks.config} || ${JSON.stringify({
            _retryCount: retryCount,
            _lastError: error.message,
          })}::jsonb`,
          updatedAt: new Date(),
        })
        .where(eq(buildTasks.id, taskId));

      log('info', `[Controller] Task #${taskId} queued for retry (${retryCount}/${this.options.maxRetries})`);
    } else {
      // Max retries exceeded, mark as error
      await this.updateTaskError(taskId, error, retryCount);
    }
  }

  private async updateTaskError(
    taskId: number,
    error: Error,
    retryCount: number
  ): Promise<void> {
    const db = getDb();

    await db
      .update(buildTasks)
      .set({
        stage: 'error',
        stageStatus: 'error',
        errorMessage: error.message,
        config: sql`${buildTasks.config} || ${JSON.stringify({
          _retryCount: retryCount,
          _lastError: error.message,
          _errorAt: new Date().toISOString(),
        })}::jsonb`,
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, taskId));

    log('error', `[Controller] Task #${taskId} permanently failed after ${retryCount} attempts`);
  }

  private async updateTaskStopped(
    taskId: number,
    reason: string
  ): Promise<void> {
    const db = getDb();

    await db
      .update(buildTasks)
      .set({
        stage: 'error',
        stageStatus: 'error',
        errorMessage: reason,
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, taskId));

    log('info', `[Controller] Task #${taskId} stopped: ${reason}`);
  }

  // ============================================================================
  // Heartbeat
  // ============================================================================

  private startHeartbeat(taskId: number): void {
    this.heartbeatTimer = setInterval(async () => {
      try {
        await this.sendHeartbeat(taskId);
      } catch (error) {
        log('error', `[Controller] Heartbeat error for task #${taskId}:`, error);
      }
    }, this.options.heartbeatInterval);
  }

  private async sendHeartbeat(taskId: number): Promise<void> {
    const db = getDb();

    await db
      .update(buildTasks)
      .set({ updatedAt: new Date() })
      .where(eq(buildTasks.id, taskId));

    log('info', `[Controller] Heartbeat sent for task #${taskId}`);
  }

  private clearHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  // ============================================================================
  // Source Management
  // ============================================================================

  private async findOrCreateSource(task: BuildTask): Promise<number | null> {
    const db = getDb();

    // Extract domain from URL for matching
    let domain: string | null = null;
    try {
      const url = new URL(task.sourceUrl);
      domain = url.hostname;
    } catch {
      // Invalid URL, skip domain extraction
    }

    // Try to find existing source by baseUrl or name
    const existingSources = await db
      .select()
      .from(sources)
      .where(
        or(
          eq(sources.baseUrl, task.sourceUrl),
          task.sourceName ? eq(sources.name, task.sourceName) : sql`false`,
          domain ? eq(sources.domain, domain) : sql`false`
        )
      )
      .limit(1);

    if (existingSources.length > 0) {
      const source = existingSources[0];
      log('info', `[Controller] Found existing source: ${source.name} (id=${source.id})`);

      // Update task with sourceId
      await db
        .update(buildTasks)
        .set({ sourceId: source.id, updatedAt: new Date() })
        .where(eq(buildTasks.id, task.id));

      return source.id;
    }

    // Create new source
    const sourceName = task.sourceName || domain || task.sourceUrl;
    log('info', `[Controller] Creating new source: ${sourceName}`);

    try {
      const [newSource] = await db
        .insert(sources)
        .values({
          name: sourceName,
          baseUrl: task.sourceUrl,
          domain: domain,
          appUrl: task.sourceUrl,
          description: `Auto-created by playbook-builder for ${task.sourceUrl}`,
        })
        .returning({ id: sources.id });

      // Update task with sourceId
      await db
        .update(buildTasks)
        .set({ sourceId: newSource.id, updatedAt: new Date() })
        .where(eq(buildTasks.id, task.id));

      log('info', `[Controller] Created new source: ${sourceName} (id=${newSource.id})`);
      return newSource.id;
    } catch (error) {
      log('error', `[Controller] Failed to create source:`, error);
      return null;
    }
  }

  // ============================================================================
  // Utilities
  // ============================================================================

  private mapDbTask(dbTask: DbBuildTask): BuildTask {
    return {
      id: dbTask.id,
      sourceId: dbTask.sourceId,
      sourceUrl: dbTask.sourceUrl,
      sourceName: dbTask.sourceName,
      sourceCategory: dbTask.sourceCategory,
      stage: dbTask.stage,
      stageStatus: dbTask.stageStatus,
      config: (dbTask.config || {}) as BuildTask['config'],
      createdAt: dbTask.createdAt,
      updatedAt: dbTask.updatedAt,
      knowledgeStartedAt: dbTask.knowledgeStartedAt,
      knowledgeCompletedAt: dbTask.knowledgeCompletedAt,
      actionStartedAt: dbTask.actionStartedAt,
      actionCompletedAt: dbTask.actionCompletedAt,
    };
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => {
      this.pollTimer = setTimeout(resolve, ms);
    });
  }
}

/**
 * Create a new PlaybookTaskController instance
 */
export function createPlaybookTaskController(): IPlaybookTaskController {
  return new PlaybookTaskControllerImpl();
}
