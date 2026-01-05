/**
 * BuildTaskWorker - Build Task Worker
 *
 * Orchestrates the action_build stage of the build pipeline.
 * Picks up build_tasks where knowledge_build is completed,
 * creates recording_tasks from chunks, and executes them concurrently.
 *
 * Uses WorkerPool for concurrent recording task execution with N workers,
 * each having its own browser instance.
 */

import type { Database } from '@actionbookdev/db'
import { BuildTaskScheduler } from './build-task-scheduler.js'
import { TaskGenerator } from './task-generator.js'
import { TaskScheduler } from './task-scheduler.js'
import { WorkerPool, type WorkerStats } from './worker-pool.js'
import type {
  BuildTaskWorkerConfig,
  BuildTaskResult,
  BuildTaskStats,
} from './types/index.js'

/** Default heartbeat interval in milliseconds (30 seconds) */
const DEFAULT_HEARTBEAT_INTERVAL_MS = 30 * 1000

/**
 * Create a self-scheduling heartbeat that avoids async re-entry.
 * Uses setTimeout instead of setInterval to ensure previous heartbeat
 * completes before scheduling the next one.
 *
 * @param fn - The async heartbeat function to call
 * @param intervalMs - The interval between heartbeats
 * @returns A stop function to cancel the heartbeat
 */
function createHeartbeat(
  fn: () => Promise<void>,
  intervalMs: number
): () => void {
  let stopped = false
  let timeoutId: ReturnType<typeof setTimeout> | null = null

  const scheduleNext = () => {
    if (stopped) return
    timeoutId = setTimeout(async () => {
      if (stopped) return
      try {
        await fn()
      } catch (error) {
        // Log but don't fail - heartbeat is best effort
        console.warn(`[Heartbeat] Update failed: ${error}`)
      }
      scheduleNext()
    }, intervalMs)
  }

  scheduleNext()

  return () => {
    stopped = true
    if (timeoutId) {
      clearTimeout(timeoutId)
      timeoutId = null
    }
  }
}

export class BuildTaskWorker {
  private buildTaskScheduler: BuildTaskScheduler
  private taskScheduler: TaskScheduler
  private taskGenerator: TaskGenerator
  private workerPool: WorkerPool
  private recordingTaskLimit: number
  private staleTimeoutMinutes: number
  private maxAttempts: number
  private heartbeatIntervalMs: number
  private taskTimeoutMinutes: number
  private buildTimeoutMinutes: number

  constructor(db: Database, config: BuildTaskWorkerConfig) {
    this.staleTimeoutMinutes = config.staleTimeoutMinutes ?? 10
    this.maxAttempts = config.maxAttempts ?? 3
    this.recordingTaskLimit = config.recordingTaskLimit ?? 100
    this.taskTimeoutMinutes = config.taskTimeoutMinutes ?? 10
    this.buildTimeoutMinutes = config.buildTimeoutMinutes ?? 8

    // Heartbeat interval should be less than stale timeout
    // Clamp to at most half of stale timeout to ensure at least 2 heartbeats before stale detection
    const staleTimeoutMs = this.staleTimeoutMinutes * 60 * 1000
    const maxHeartbeatMs = Math.floor(staleTimeoutMs / 2)
    const configuredHeartbeatMs =
      config.heartbeatIntervalMs ?? DEFAULT_HEARTBEAT_INTERVAL_MS

    if (configuredHeartbeatMs > maxHeartbeatMs) {
      console.warn(
        `[BuildTaskWorker] heartbeatIntervalMs (${configuredHeartbeatMs}ms) is greater than ` +
          `half of staleTimeoutMinutes (${maxHeartbeatMs}ms). Clamping to ${maxHeartbeatMs}ms.`
      )
    }
    this.heartbeatIntervalMs = Math.min(configuredHeartbeatMs, maxHeartbeatMs)

    this.buildTaskScheduler = new BuildTaskScheduler(db, {
      maxAttempts: this.maxAttempts,
      staleTimeoutMinutes: this.staleTimeoutMinutes,
    })
    this.taskScheduler = new TaskScheduler(db)
    this.taskGenerator = new TaskGenerator(db)

    const concurrency = config.concurrency ?? 1
    this.workerPool = new WorkerPool(db, {
      ...config,
      concurrency,
      taskTimeoutMinutes: this.taskTimeoutMinutes,
      buildTimeoutMinutes: this.buildTimeoutMinutes,
    })

    console.log(
      `[BuildTaskWorker] Initialized with concurrency=${concurrency}, ` +
        `staleTimeout=${this.staleTimeoutMinutes}min, maxAttempts=${this.maxAttempts}, ` +
        `heartbeat=${this.heartbeatIntervalMs}ms, ` +
        `taskTimeout=${this.taskTimeoutMinutes}min, buildTimeout=${this.buildTimeoutMinutes}min`
    )
  }

  /**
   * Get current worker pool status
   *
   * @returns Worker statistics (busy, idle, total)
   */
  getWorkerStats(): WorkerStats {
    return this.workerPool.getWorkerStats()
  }

  /**
   * Gracefully shutdown the worker
   * Stops heartbeats and closes all browser instances
   */
  async shutdown(): Promise<void> {
    console.log('[BuildTaskWorker] Shutting down...')
    await this.workerPool.shutdown()
    console.log('[BuildTaskWorker] Shutdown complete')
  }

  /**
   * Run one complete cycle:
   *
   * 1. Atomically claim next build_task (concurrent-safe)
   * 2. Verify sourceId exists
   * 3. Generate recording_tasks from chunks
   * 4. Execute all pending recording_tasks concurrently via WorkerPool
   * 5. Complete or fail the build_task
   *
   * Heartbeat is updated periodically throughout the entire execution
   * to prevent the task from being considered stale during long-running operations.
   *
   * @returns BuildTaskResult or null if no tasks available
   */
  async runOnce(): Promise<BuildTaskResult | null> {
    const startTime = Date.now()

    // 1. Atomically claim next task (already transitions to action_build/running)
    // Uses FOR UPDATE SKIP LOCKED to prevent concurrent workers from claiming the same task
    const buildTask = await this.buildTaskScheduler.claimNextActionTask()
    if (!buildTask) {
      return null // No tasks available
    }

    // Start heartbeat immediately after claiming the task
    // Uses self-scheduling setTimeout to avoid async re-entry issues
    const stopBuildTaskHeartbeat = createHeartbeat(
      () => this.buildTaskScheduler.updateHeartbeat(buildTask.id),
      this.heartbeatIntervalMs
    )

    try {
      let tasksReset = 0
      let tasksCreated = 0

      // 2. Verify sourceId exists (should always be true due to WHERE clause, but defensive check)
      if (!buildTask.sourceId) {
        const errorMessage =
          'sourceId is null - knowledge_build may have failed'
        await this.buildTaskScheduler.failTask(buildTask.id, errorMessage)
        return {
          success: false,
          taskId: buildTask.id,
          recordingTasksReset: 0,
          recordingTasksCreated: 0,
          recordingTasksCompleted: 0,
          recordingTasksFailed: 0,
          elementsCreated: 0,
          duration_ms: Date.now() - startTime,
          error: errorMessage,
        }
      }

      const sourceId = buildTask.sourceId

      // 3. Reset existing recording_tasks to allow re-execution
      // This allows failed/completed tasks to be retried when build_task is re-run
      tasksReset = await this.taskScheduler.resetRecordingTasksForBuildTask(buildTask.id)

      // 4. Generate new recording_tasks from chunks for this specific build_task
      tasksCreated = await this.taskGenerator.generate(
        buildTask.id,
        sourceId,
        this.recordingTaskLimit
      )

      // 5. Execute all pending recording_tasks for this build_task concurrently via WorkerPool
      // WorkerPool manages N concurrent TaskExecutors, each with its own browser
      const executionStats = await this.workerPool.executeAll(
        buildTask.id,
        sourceId,
        {
          staleTimeoutMinutes: this.staleTimeoutMinutes,
          maxAttempts: this.maxAttempts,
          heartbeatIntervalMs: this.heartbeatIntervalMs,
        }
      )

      // 6. Complete the build_task
      const stats: BuildTaskStats = {
        recordingTasksReset: tasksReset,
        recordingTasksCreated: tasksCreated,
        recordingTasksCompleted: executionStats.completed,
        recordingTasksFailed: executionStats.failed,
        elementsCreated: executionStats.elements,
        duration_ms: Date.now() - startTime,
      }

      await this.buildTaskScheduler.completeTask(buildTask.id, stats)

      // 6. Publish the version (Blue-Green deployment)
      const publishResult = await this.buildTaskScheduler.publishVersion(
        sourceId
      )
      if (publishResult.success) {
        console.log(
          `[BuildTaskWorker] Published version ${publishResult.versionId} for source ${sourceId}` +
            (publishResult.archivedVersionId
              ? `, archived version ${publishResult.archivedVersionId}`
              : '')
        )
      } else {
        // Log warning but don't fail the task - version publish is best-effort
        console.warn(
          `[BuildTaskWorker] Failed to publish version for source ${sourceId}: ${publishResult.error}`
        )
      }

      return {
        success: true,
        taskId: buildTask.id,
        ...stats,
        publishedVersionId: publishResult.versionId,
      }
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : String(error)
      await this.buildTaskScheduler.failTask(buildTask.id, errorMessage)

      return {
        success: false,
        taskId: buildTask.id,
        recordingTasksReset: 0,
        recordingTasksCreated: 0,
        recordingTasksCompleted: 0,
        recordingTasksFailed: 0,
        elementsCreated: 0,
        duration_ms: Date.now() - startTime,
        error: errorMessage,
      }
    } finally {
      // Always stop the build task heartbeat and shutdown worker pool
      stopBuildTaskHeartbeat()
      await this.workerPool.shutdown()
    }
  }
}
