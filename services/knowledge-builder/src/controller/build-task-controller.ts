/**
 * BuildTaskController - Polls and executes knowledge-builder tasks
 *
 * Features:
 * - Polls database for pending tasks (stage='init', source_category='help')
 * - Claims tasks with optimistic locking to prevent duplicate execution
 * - Executes knowledge-builder pipeline
 * - Updates task status with heartbeat mechanism
 * - Supports retry on failure
 */

import {
  getDb,
  buildTasks,
  eq,
  and,
  sql,
  type BuildTask as DbBuildTask,
} from '@actionbookdev/db'
import {
  createProcessor,
  type ProcessingResult,
  type PrepareResult,
} from '../builder/index.js'
import { mapTaskToProcessorConfig, validateTask } from './task-mapper.js'
import type {
  BuildTaskController as IBuildTaskController,
  ControllerOptions,
  ControllerState,
  BuildTask,
} from './types.js'

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
}

/**
 * BuildTaskController implementation
 */
export class BuildTaskControllerImpl implements IBuildTaskController {
  private state: ControllerState = 'idle'
  private options: Required<
    Omit<
      ControllerOptions,
      'onProgress' | 'onTaskStart' | 'onTaskComplete' | 'onTaskError'
    >
  > &
    Pick<
      ControllerOptions,
      'onProgress' | 'onTaskStart' | 'onTaskComplete' | 'onTaskError'
    >
  private pollTimer: ReturnType<typeof setTimeout> | null = null
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null
  private currentTaskId: number | null = null
  private processor = createProcessor()
  private stopPromiseResolve: (() => void) | null = null

  constructor() {
    this.options = { ...DEFAULT_OPTIONS }
  }

  // ============================================================================
  // Public Interface
  // ============================================================================

  async start(options?: ControllerOptions): Promise<void> {
    if (this.state !== 'idle' && this.state !== 'stopped') {
      throw new Error(`Cannot start controller in state: ${this.state}`)
    }

    this.options = { ...DEFAULT_OPTIONS, ...options }
    this.state = 'polling'

    console.log('[Controller] Starting with options:', {
      pollInterval: this.options.pollInterval,
      heartbeatInterval: this.options.heartbeatInterval,
      maxRetries: this.options.maxRetries,
    })

    // Start polling loop
    await this.poll()
  }

  async stop(reason?: string): Promise<void> {
    if (this.state === 'stopped' || this.state === 'idle') {
      return
    }

    console.log('[Controller] Stopping...')
    this.state = 'stopping'

    // Clear poll timer
    if (this.pollTimer) {
      clearTimeout(this.pollTimer)
      this.pollTimer = null
    }

    // If currently processing, mark task as error and stop
    if (this.currentTaskId !== null) {
      const taskId = this.currentTaskId
      const errorMessage = reason || '任务被手动停止'

      console.log(`[Controller] Stopping task #${taskId}: ${errorMessage}`)

      // Update task status to error
      await this.updateTaskStopped(taskId, errorMessage)

      // Stop the processor
      await this.processor.stop()

      // Wait for the processing to finish
      await new Promise<void>((resolve) => {
        this.stopPromiseResolve = resolve
      })
    }

    // Clear heartbeat timer
    this.clearHeartbeat()

    this.state = 'stopped'
    console.log('[Controller] Stopped')
  }

  async checkOnce(): Promise<boolean> {
    const task = await this.pollTask()
    if (!task) {
      return false
    }

    await this.executeTask(task)
    return true
  }

  getState(): ControllerState {
    return this.state
  }

  // ============================================================================
  // Polling Logic
  // ============================================================================

  private async poll(): Promise<void> {
    while (this.state === 'polling') {
      try {
        console.log('[Controller] Polling for pending tasks...')
        const task = await this.pollTask()

        if (task) {
          console.log(`[Controller] Found task #${task.id}: ${task.sourceUrl}`)
          await this.executeTask(task)
        } else {
          console.log('[Controller] No pending tasks found')
        }
      } catch (error) {
        console.error('[Controller] Poll error:', error)
      }

      // Schedule next poll if still running
      if (this.state === 'polling') {
        console.log(
          `[Controller] Next poll in ${this.options.pollInterval / 1000}s`
        )
        await this.sleep(this.options.pollInterval)
      }
    }
  }

  private async pollTask(): Promise<BuildTask | null> {
    const db = getDb()

    // Query for pending tasks
    const tasks = await db
      .select()
      .from(buildTasks)
      .where(
        and(
          eq(buildTasks.stage, 'init'),
          eq(buildTasks.sourceCategory, 'help'),
          eq(buildTasks.stageStatus, 'pending')
        )
      )
      .orderBy(buildTasks.createdAt)
      .limit(1)

    if (tasks.length === 0) {
      return null
    }

    return this.mapDbTask(tasks[0])
  }

  // ============================================================================
  // Task Execution
  // ============================================================================

  private async executeTask(task: BuildTask): Promise<void> {
    // Validate task
    const validationError = validateTask(task)
    if (validationError) {
      console.error(
        `[Controller] Task #${task.id} validation failed: ${validationError}`
      )
      await this.updateTaskError(task.id, new Error(validationError), 0)
      return
    }

    // Try to claim the task
    const claimed = await this.claimTask(task.id)
    if (!claimed) {
      console.log(
        `[Controller] Task #${task.id} already claimed by another worker`
      )
      return
    }

    console.log(`[Controller] Claimed task #${task.id}: ${task.sourceUrl}`)
    this.state = 'processing'
    this.currentTaskId = task.id

    // Start heartbeat
    this.startHeartbeat(task.id)

    // Notify task start
    this.options.onTaskStart?.(task.id)

    try {
      // Map task to processor config
      const config = mapTaskToProcessorConfig(task)
      console.log(`[Controller] Processing task #${task.id} with config:`, {
        sourceName: config.sourceName,
        baseUrl: config.baseUrl,
        maxDepth: config.crawlConfig.maxDepth,
      })

      // Prepare: create source and version in database
      const prepareResult = await this.processor.prepare(config)
      console.log(
        `[Controller] Task #${task.id} prepared: sourceId=${prepareResult.sourceId}, versionId=${prepareResult.versionId}`
      )

      // Update task with sourceId early (before processing starts)
      await this.updateTaskSourceId(task.id, prepareResult.sourceId)

      // Execute processor (uses prepared source/version)
      const result = await this.processor.process(config, (progress) => {
        this.options.onProgress?.(task.id, progress)
      })

      // Update task as success
      await this.updateTaskSuccess(task.id, result)
      this.options.onTaskComplete?.(task.id, result)

      console.log(`[Controller] Task #${task.id} completed:`, {
        sourceId: result.sourceId,
        versionId: result.versionId,
        totalPages: result.totalPages,
        durationMs: result.durationMs,
      })
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error))
      const retryCount = (task.config._retryCount ?? 0) + 1

      console.error(
        `[Controller] Task #${task.id} failed (attempt ${retryCount}/${this.options.maxRetries}):`,
        err.message
      )

      await this.handleTaskError(task.id, err, retryCount)
      this.options.onTaskError?.(task.id, err, retryCount)
    } finally {
      this.clearHeartbeat()
      this.currentTaskId = null

      // If we were stopping (state changed by stop() during processing), signal completion
      // Using getState() to avoid TypeScript narrowing issues with async state changes
      const currentState = this.getState()
      if (currentState === 'stopping' && this.stopPromiseResolve) {
        this.stopPromiseResolve()
        this.stopPromiseResolve = null
      } else if (currentState === 'processing') {
        this.state = 'polling'
      }
    }
  }

  // ============================================================================
  // Task State Management
  // ============================================================================

  private async claimTask(taskId: number): Promise<boolean> {
    const db = getDb()

    // Optimistic lock: only update if still pending
    const result = await db
      .update(buildTasks)
      .set({
        stage: 'knowledge_build',
        stageStatus: 'running',
        knowledgeStartedAt: new Date(),
        updatedAt: new Date(),
      })
      .where(
        and(eq(buildTasks.id, taskId), eq(buildTasks.stageStatus, 'pending'))
      )
      .returning({ id: buildTasks.id })

    return result.length > 0
  }

  private async updateTaskSourceId(
    taskId: number,
    sourceId: number
  ): Promise<void> {
    const db = getDb()

    await db
      .update(buildTasks)
      .set({
        sourceId,
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, taskId))
  }

  private async updateTaskSuccess(
    taskId: number,
    result: ProcessingResult
  ): Promise<void> {
    const db = getDb()

    await db
      .update(buildTasks)
      .set({
        sourceId: result.sourceId,
        stageStatus: 'completed',
        knowledgeCompletedAt: new Date(),
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, taskId))
  }

  private async handleTaskError(
    taskId: number,
    error: Error,
    retryCount: number
  ): Promise<void> {
    const db = getDb()

    if (retryCount < this.options.maxRetries) {
      // Reset to pending for retry
      await db
        .update(buildTasks)
        .set({
          stage: 'init',
          stageStatus: 'pending',
          config: sql`${buildTasks.config} || ${JSON.stringify({
            _retryCount: retryCount,
            _lastError: error.message,
          })}::jsonb`,
          updatedAt: new Date(),
        })
        .where(eq(buildTasks.id, taskId))

      console.log(
        `[Controller] Task #${taskId} queued for retry (${retryCount}/${this.options.maxRetries})`
      )
    } else {
      // Max retries exceeded, mark as error
      await this.updateTaskError(taskId, error, retryCount)
    }
  }

  private async updateTaskError(
    taskId: number,
    error: Error,
    retryCount: number
  ): Promise<void> {
    const db = getDb()

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
      .where(eq(buildTasks.id, taskId))

    console.error(
      `[Controller] Task #${taskId} permanently failed after ${retryCount} attempts`
    )
  }

  private async updateTaskStopped(
    taskId: number,
    reason: string
  ): Promise<void> {
    const db = getDb()

    await db
      .update(buildTasks)
      .set({
        stage: 'error',
        stageStatus: 'error',
        errorMessage: reason,
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, taskId))

    console.log(`[Controller] Task #${taskId} stopped: ${reason}`)
  }

  // ============================================================================
  // Heartbeat
  // ============================================================================

  private startHeartbeat(taskId: number): void {
    this.heartbeatTimer = setInterval(async () => {
      try {
        await this.sendHeartbeat(taskId)
      } catch (error) {
        console.error(
          `[Controller] Heartbeat error for task #${taskId}:`,
          error
        )
      }
    }, this.options.heartbeatInterval)
  }

  private async sendHeartbeat(taskId: number): Promise<void> {
    const db = getDb()

    await db
      .update(buildTasks)
      .set({ updatedAt: new Date() })
      .where(eq(buildTasks.id, taskId))

    console.log(`[Controller] Heartbeat sent for task #${taskId}`)
  }

  private clearHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer)
      this.heartbeatTimer = null
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
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => {
      this.pollTimer = setTimeout(resolve, ms)
    })
  }
}

/**
 * Create a new BuildTaskController instance
 */
export function createBuildTaskController(): IBuildTaskController {
  return new BuildTaskControllerImpl()
}
