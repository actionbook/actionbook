/**
 * WorkerPool - Concurrent Task Execution Pool
 *
 * Manages multiple TaskExecutors to process recording tasks in parallel.
 * Each worker has its own TaskExecutor instance.
 *
 * Key features:
 * - Bounded concurrency: Never exceed N parallel executions
 * - Atomic task claiming: Uses FOR UPDATE SKIP LOCKED via TaskScheduler
 * - Error isolation: One worker failure does not affect others
 * - Heartbeat management: Prevents tasks from being marked stale
 * - Graceful error handling: Waits for running tasks on unexpected errors
 *
 * Resource Management:
 * - TaskExecutor instances are lightweight (no browser in constructor)
 * - Browsers are created lazily per-task in TaskExecutor.execute()
 * - Each TaskExecutor.execute() handles its own browser cleanup via finally block
 */

import type { Database } from '@actionbookdev/db'
import { TaskScheduler } from './task-scheduler.js'
import { TaskExecutor } from './task-executor.js'
import type {
  RecordingTask,
  ExecutionResult,
  TaskExecutorConfig,
} from './types/index.js'

/**
 * WorkerPool configuration
 */
export interface WorkerPoolConfig extends TaskExecutorConfig {
  /** Number of concurrent workers (default: 3) */
  concurrency?: number
  /** Task execution timeout in minutes (default: 10) */
  taskTimeoutMinutes?: number
}

/**
 * Execution options for executeAll
 */
export interface ExecuteAllOptions {
  /** Stale timeout in minutes */
  staleTimeoutMinutes: number
  /** Max retry attempts for stale tasks */
  maxAttempts: number
  /** Heartbeat interval in milliseconds */
  heartbeatIntervalMs: number
}

/**
 * Execution statistics
 */
export interface ExecutionStats {
  /** Number of successfully completed tasks */
  completed: number
  /** Number of failed tasks */
  failed: number
  /** Total elements created */
  elements: number
}

/**
 * Worker status statistics
 */
export interface WorkerStats {
  /** Number of busy workers */
  busy: number
  /** Number of idle workers */
  idle: number
  /** Total number of workers */
  total: number
}

/**
 * Internal worker state
 */
interface WorkerState {
  id: number
  executor: TaskExecutor
  busy: boolean
  currentTaskId?: number
}

/**
 * WorkerPool - Manages concurrent task execution
 */
export class WorkerPool {
  private workers: WorkerState[] = []
  private concurrency: number
  private taskScheduler: TaskScheduler
  private stopHeartbeats: Map<number, () => void> = new Map()
  private taskTimeoutMs: number

  constructor(db: Database, config: WorkerPoolConfig) {
    this.concurrency = config.concurrency ?? 3
    this.taskTimeoutMs = (config.taskTimeoutMinutes ?? 10) * 60 * 1000
    this.taskScheduler = new TaskScheduler(db)

    // Pre-create worker slots with executors
    for (let i = 0; i < this.concurrency; i++) {
      this.workers.push({
        id: i,
        executor: new TaskExecutor(db, config),
        busy: false,
      })
    }
  }

  /**
   * Get current worker status
   *
   * @returns Worker statistics (busy, idle, total)
   */
  getWorkerStats(): WorkerStats {
    const busy = this.workers.filter((w) => w.busy).length
    return {
      busy,
      idle: this.concurrency - busy,
      total: this.concurrency,
    }
  }

  /**
   * Execute all pending tasks for a build_task with N concurrent workers
   *
   * Uses a loop that:
   * 1. Finds an available worker
   * 2. Atomically claims a task
   * 3. Starts execution (non-blocking)
   * 4. Repeats until no more tasks
   *
   * Error Handling:
   * - If claimNextTask or other operations fail, waits for running tasks to complete
   * - Running tasks continue to completion even if the main loop errors
   * - Stats from completed tasks are preserved in the returned result
   *
   * @param buildTaskId - Build task ID to filter tasks
   * @param sourceId - Source ID to filter tasks
   * @param options - Execution options
   * @returns Execution statistics
   */
  async executeAll(
    buildTaskId: number,
    sourceId: number,
    options: ExecuteAllOptions
  ): Promise<ExecutionStats> {
    let completed = 0
    let failed = 0
    let elements = 0

    // Track running promises
    const runningPromises: Map<number, Promise<void>> = new Map()

    /**
     * Get worker status summary
     */
    const getWorkerStatus = (): { busy: number; idle: number } => {
      const busy = this.workers.filter((w) => w.busy).length
      return { busy, idle: this.concurrency - busy }
    }

    /**
     * Wait for all running promises to complete
     * Used both for normal completion and error cleanup
     */
    const waitForRunningTasks = async (): Promise<void> => {
      if (runningPromises.size > 0) {
        const { busy, idle } = getWorkerStatus()
        console.log(
          `[WorkerPool] Waiting for ${runningPromises.size} running task(s) to complete... ` +
            `(workers: ${busy} busy, ${idle} idle)`
        )
        await Promise.all(runningPromises.values())
      }
    }

    console.log(
      `[WorkerPool] Starting execution for build_task ${buildTaskId} (source ${sourceId}) with ${this.concurrency} workers`
    )

    try {
      while (true) {
        // 1. Find available worker
        const availableWorker = this.workers.find((w) => !w.busy)

        if (!availableWorker) {
          // All workers busy - wait for one to complete
          if (runningPromises.size > 0) {
            console.log(
              `[WorkerPool] All workers busy (${this.concurrency}/${this.concurrency}), waiting for one to complete...`
            )
            await Promise.race(runningPromises.values())
            continue
          }
          break // No workers and no running tasks - shouldn't happen
        }

        // 2. Atomically claim next task for this build_task
        const task = await this.taskScheduler.claimNextTask({
          buildTaskId,
          sourceId,
          staleTimeoutMinutes: options.staleTimeoutMinutes,
          maxAttempts: options.maxAttempts,
        })

        if (!task) {
          // No more tasks - wait for running tasks to complete
          await waitForRunningTasks()
          break
        }

        // 3. Mark worker as busy and start execution
        availableWorker.busy = true
        availableWorker.currentTaskId = task.id

        const { busy, idle } = getWorkerStatus()
        console.log(
          `[WorkerPool] Worker ${availableWorker.id} claiming task ${task.id} ` +
            `(workers: ${busy} busy, ${idle} idle)`
        )

        // Start heartbeat for this task
        const stopHeartbeat = this.createHeartbeat(
          () => this.taskScheduler.updateHeartbeat(task.id),
          options.heartbeatIntervalMs
        )
        this.stopHeartbeats.set(task.id, stopHeartbeat)

        // 4. Execute task in background
        const promise = this.executeTaskWithWorker(availableWorker, task)
          .then((result) => {
            if (result.success) {
              completed++
              elements += result.actions_created
              console.log(
                `[WorkerPool] Worker ${availableWorker.id} completed task ${task.id} ` +
                  `(${result.actions_created} elements, ${completed} completed, ${failed} failed)`
              )
            } else {
              failed++
              console.log(
                `[WorkerPool] Worker ${availableWorker.id} task ${task.id} failed: ${result.error} ` +
                  `(${completed} completed, ${failed} failed)`
              )
            }
          })
          .catch((error) => {
            failed++
            console.error(
              `[WorkerPool] Worker ${availableWorker.id} error on task ${task.id}:`,
              error
            )
          })
          .finally(() => {
            availableWorker.busy = false
            availableWorker.currentTaskId = undefined
            runningPromises.delete(task.id)

            // Stop heartbeat
            const stop = this.stopHeartbeats.get(task.id)
            if (stop) {
              stop()
              this.stopHeartbeats.delete(task.id)
            }
          })

        runningPromises.set(task.id, promise)
      }
    } catch (error) {
      // On unexpected error (e.g., database connection issue in claimNextTask),
      // wait for running tasks to complete before re-throwing
      const { busy, idle } = getWorkerStatus()
      console.error(
        `[WorkerPool] Error in executeAll loop (workers: ${busy} busy, ${idle} idle), ` +
          `waiting for ${runningPromises.size} running task(s):`,
        error
      )
      await waitForRunningTasks()
      throw error
    }

    console.log(
      `[WorkerPool] Execution complete for source ${sourceId}: ` +
        `${completed} completed, ${failed} failed, ${elements} elements created`
    )

    return { completed, failed, elements }
  }

  /**
   * Execute a single task with a specific worker
   *
   * Includes timeout protection to prevent tasks from hanging indefinitely.
   * If a task exceeds the timeout, it will be marked as failed and the worker
   * will be released.
   */
  private async executeTaskWithWorker(
    worker: WorkerState,
    task: RecordingTask
  ): Promise<ExecutionResult> {
    console.log(`[WorkerPool] Worker ${worker.id} executing task ${task.id}`)

    // Maximum execution time (configurable, default: 10 minutes)
    // This prevents tasks from hanging indefinitely due to unhandled Promise deadlocks
    const timeoutPromise = new Promise<ExecutionResult>((_, reject) => {
      setTimeout(() => {
        reject(
          new Error(
            `Task execution timeout after ${this.taskTimeoutMs / 1000 / 60} minutes`
          )
        )
      }, this.taskTimeoutMs)
    })

    try {
      return await Promise.race([
        worker.executor.execute(task),
        timeoutPromise,
      ])
    } catch (error) {
      // If timeout or other error occurs, ensure task is marked as failed
      const errorMessage = error instanceof Error ? error.message : String(error)

      console.error(
        `[WorkerPool] Worker ${worker.id} task ${task.id} failed:`,
        errorMessage
      )

      // Update task status to failed (best-effort - TaskExecutor may have already updated)
      try {
        await this.taskScheduler.markFailed(task.id, errorMessage)
      } catch (updateError) {
        console.warn(
          `[WorkerPool] Failed to update task ${task.id} status:`,
          updateError
        )
      }

      // Return failure result
      return {
        success: false,
        actions_created: 0,
        error: errorMessage,
        duration_ms: this.taskTimeoutMs,
      }
    }
  }

  /**
   * Create a self-scheduling heartbeat
   *
   * Uses setTimeout instead of setInterval to ensure previous heartbeat
   * completes before scheduling the next one.
   */
  private createHeartbeat(
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
          console.warn(`[WorkerPool] Heartbeat failed: ${error}`)
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

  /**
   * Gracefully shutdown the worker pool
   *
   * Stops all heartbeats. Note that workers (TaskExecutors) are lightweight
   * and don't need explicit cleanup - they create/close browsers per execution.
   */
  async shutdown(): Promise<void> {
    // Stop all heartbeats
    for (const stop of this.stopHeartbeats.values()) {
      stop()
    }
    this.stopHeartbeats.clear()
  }
}
