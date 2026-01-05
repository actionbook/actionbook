/**
 * TaskScheduler - Task Scheduler
 *
 * Responsible for fetching pending tasks and updating task status
 * M1 version: Simplified FIFO queue (no concurrency support)
 *
 * Stale Task Recovery:
 * - Tasks stuck in 'running' state (e.g., after crash) can be recovered
 * - Stale timeout: tasks running longer than threshold are considered stale
 * - Retry limit: stale tasks are retried up to maxAttempts times
 */

import type { Database, RecordingTaskStatus } from '@actionbookdev/db'
import { recordingTasks, eq, and, asc, or, lt, sql } from '@actionbookdev/db'
import type { RecordingTask, TaskConfig } from './types/index.js'

export interface GetTaskOptions {
  sourceId?: number
  buildTaskId?: number
  /** Stale timeout in minutes. Running tasks older than this are considered stale. Default: 30 */
  staleTimeoutMinutes?: number
  /** Max retry attempts for stale tasks. Default: 3 */
  maxAttempts?: number
}

export class TaskScheduler {
  constructor(private db: Database) {}

  /**
   * Get next pending task
   * M1: Simplified FIFO, no concurrency consideration
   *
   * @param sourceId - Optional source ID filter
   * @returns Task object or null
   */
  async getNextTask(sourceId?: number): Promise<RecordingTask | null> {
    const whereClause =
      sourceId === undefined
        ? eq(recordingTasks.status, 'pending')
        : and(
            eq(recordingTasks.status, 'pending'),
            eq(recordingTasks.sourceId, sourceId)
          )

    const result = await this.db
      .select()
      .from(recordingTasks)
      .where(whereClause)
      .orderBy(asc(recordingTasks.id))
      .limit(1)

    if (result.length === 0) {
      return null
    }

    return this.mapToRecordingTask(result[0])
  }

  /**
   * Get next task with stale task recovery
   *
   * Priority:
   * 1. Pending tasks (FIFO)
   * 2. Stale running tasks (older than staleTimeoutMinutes)
   *
   * Stale tasks are automatically reset to pending with incremented attemptCount.
   * Tasks exceeding maxAttempts are marked as failed.
   *
   * Uses while-loop instead of recursion to avoid stack overflow with many stale tasks.
   *
   * @param options - Options including sourceId, staleTimeoutMinutes, maxAttempts
   * @returns Task object or null
   */
  async getNextTaskWithRecovery(
    options: GetTaskOptions = {}
  ): Promise<RecordingTask | null> {
    const { sourceId, staleTimeoutMinutes = 10, maxAttempts = 3 } = options

    // 1. Try to get a pending task first
    const pendingTask = await this.getNextTask(sourceId)
    if (pendingTask) {
      return pendingTask
    }

    // 2. Look for stale running tasks (use while-loop to avoid recursion)
    while (true) {
      const staleTask = await this.getStaleRunningTask(
        sourceId,
        staleTimeoutMinutes
      )
      if (!staleTask) {
        return null
      }

      // 3. Check if task has exceeded max attempts
      if (staleTask.attemptCount >= maxAttempts) {
        console.log(
          `[TaskScheduler] Task ${staleTask.id} exceeded max attempts (${maxAttempts}), marking as failed`
        )
        await this.markFailed(
          staleTask.id,
          `Exceeded max attempts (${maxAttempts}) after timeout`
        )
        // Continue to find another stale task (using loop instead of recursion)
        continue
      }

      // 4. Reset stale task to pending with incremented attemptCount
      console.log(
        `[TaskScheduler] Recovering stale task ${staleTask.id} (attempt ${
          staleTask.attemptCount + 1
        }/${maxAttempts})`
      )
      await this.resetStaleTask(staleTask.id)

      // Return the task with updated attemptCount
      return {
        ...staleTask,
        status: 'pending',
        attemptCount: staleTask.attemptCount + 1,
      }
    }
  }

  /**
   * Get a stale running task (running longer than timeout)
   */
  private async getStaleRunningTask(
    sourceId: number | undefined,
    staleTimeoutMinutes: number
  ): Promise<RecordingTask | null> {
    const staleThreshold = new Date(
      Date.now() - staleTimeoutMinutes * 60 * 1000
    )

    // Build where clause: running AND (lastHeartbeat < threshold OR updatedAt < threshold)
    const baseCondition = and(
      eq(recordingTasks.status, 'running'),
      or(
        lt(recordingTasks.lastHeartbeat, staleThreshold),
        and(
          sql`${recordingTasks.lastHeartbeat} IS NULL`,
          lt(recordingTasks.updatedAt, staleThreshold)
        )
      )
    )

    const whereClause =
      sourceId === undefined
        ? baseCondition
        : and(baseCondition, eq(recordingTasks.sourceId, sourceId))

    const result = await this.db
      .select()
      .from(recordingTasks)
      .where(whereClause!)
      .orderBy(asc(recordingTasks.id))
      .limit(1)

    if (result.length === 0) {
      return null
    }

    return this.mapToRecordingTask(result[0])
  }

  /**
   * Reset a stale task to pending status with incremented attemptCount
   */
  private async resetStaleTask(taskId: number): Promise<void> {
    await this.db
      .update(recordingTasks)
      .set({
        status: 'pending',
        progress: 0,
        attemptCount: sql`${recordingTasks.attemptCount} + 1`,
        errorMessage: null,
        lastHeartbeat: null,
        updatedAt: new Date(),
      })
      .where(eq(recordingTasks.id, taskId))
  }

  /**
   * Map database row to RecordingTask type
   */
  private mapToRecordingTask(
    task: typeof recordingTasks.$inferSelect
  ): RecordingTask {
    const config = (task.config as TaskConfig | null) ?? {
      chunk_type: 'exploratory',
    }

    return {
      id: task.id,
      sourceId: task.sourceId,
      chunkId: task.chunkId,
      startUrl: task.startUrl,
      status: task.status as RecordingTaskStatus,
      progress: task.progress,
      config,
      attemptCount: task.attemptCount,
      errorMessage: task.errorMessage,
      completedAt: task.completedAt,
      lastHeartbeat: task.lastHeartbeat,
      createdAt: task.createdAt,
      updatedAt: task.updatedAt,
    }
  }

  /**
   * Mark task as running
   */
  async markRunning(taskId: number): Promise<void> {
    const now = new Date()
    await this.db
      .update(recordingTasks)
      .set({
        status: 'running',
        startedAt: now,
        lastHeartbeat: now,
        updatedAt: now,
      })
      .where(eq(recordingTasks.id, taskId))
  }

  /**
   * Update task heartbeat (call periodically during long-running tasks)
   */
  async updateHeartbeat(taskId: number): Promise<void> {
    const now = new Date()
    await this.db
      .update(recordingTasks)
      .set({
        lastHeartbeat: now,
        updatedAt: now,
      })
      .where(eq(recordingTasks.id, taskId))
  }

  /**
   * Reset all recording tasks for a specific build_task to pending state
   *
   * This allows a build_task to be re-executed from the beginning.
   * Only resets tasks that are not currently running to avoid conflicts.
   *
   * @param buildTaskId - The build task ID to reset tasks for
   * @returns Number of tasks reset
   */
  async resetRecordingTasksForBuildTask(buildTaskId: number): Promise<number> {
    const now = new Date()

    // Reset all non-running tasks to pending
    // Running tasks are left alone to avoid interfering with active executions
    const result = await this.db.execute<{ id: number }>(sql`
      UPDATE recording_tasks
      SET
        status = 'pending',
        started_at = NULL,
        completed_at = NULL,
        last_heartbeat = NULL,
        progress = 0,
        elements_discovered = 0,
        pages_discovered = 0,
        tokens_used = 0,
        attempt_count = 0,
        duration_ms = NULL,
        error_message = NULL,
        updated_at = ${now}
      WHERE build_task_id = ${buildTaskId}
        AND status != 'running'
      RETURNING id
    `)

    const resetCount = result.rows.length

    if (resetCount > 0) {
      console.log(
        `[TaskScheduler] Reset ${resetCount} recording task(s) for build_task ${buildTaskId}`
      )
    }

    return resetCount
  }

  /**
   * Mark task as completed
   */
  async markCompleted(taskId: number): Promise<void> {
    await this.db
      .update(recordingTasks)
      .set({
        status: 'completed',
        progress: 100,
        completedAt: new Date(),
        updatedAt: new Date(),
      })
      .where(eq(recordingTasks.id, taskId))
  }

  /**
   * Mark task as failed
   */
  async markFailed(taskId: number, errorMessage: string): Promise<void> {
    await this.db
      .update(recordingTasks)
      .set({
        status: 'failed',
        errorMessage,
        completedAt: new Date(),
        updatedAt: new Date(),
      })
      .where(eq(recordingTasks.id, taskId))
  }

  // =========================================================================
  // Concurrent-Safe Task Claiming (M2)
  // =========================================================================

  /**
   * Raw recording task type (snake_case from raw SQL)
   */
  private static readonly RawRecordingTaskType = {} as {
    id: number
    source_id: number
    chunk_id: number | null
    start_url: string
    status: string
    progress: number
    config: TaskConfig | null
    attempt_count: number
    error_message: string | null
    completed_at: Date | null
    last_heartbeat: Date | null
    started_at: Date | null
    created_at: Date
    updated_at: Date
  }

  /**
   * Atomically claim the next available task (concurrent-safe)
   *
   * Uses FOR UPDATE SKIP LOCKED to prevent race conditions.
   * Combines SELECT + UPDATE into single atomic operation.
   *
   * Task Selection Priority:
   * 1. Pending tasks (FIFO by id)
   * 2. Stale running tasks (last_heartbeat older than staleTimeoutMinutes)
   *
   * Stale Task Recovery:
   * - Tasks stuck in 'running' state (e.g., after process crash) are recovered
   * - Tasks exceeding maxAttempts are marked as failed and skipped
   * - attemptCount is incremented for recovered tasks
   *
   * This is the preferred method for concurrent workers. Each worker
   * will get a different task, and no task will be claimed twice.
   *
   * @param options - Options including sourceId, staleTimeoutMinutes, maxAttempts
   * @returns Claimed task (already marked as running) or null if no tasks available
   */
  async claimNextTask(
    options: GetTaskOptions = {}
  ): Promise<RecordingTask | null> {
    const {
      sourceId,
      buildTaskId,
      staleTimeoutMinutes = 10,
      maxAttempts = 3,
    } = options
    const now = new Date()
    const staleThreshold = new Date(
      now.getTime() - staleTimeoutMinutes * 60 * 1000
    )

    type RawRecordingTask = typeof TaskScheduler.RawRecordingTaskType

    // Build filter conditions
    const sourceFilter =
      sourceId !== undefined ? sql`AND source_id = ${sourceId}` : sql``
    const buildTaskFilter =
      buildTaskId !== undefined
        ? sql`AND build_task_id = ${buildTaskId}`
        : sql``

    // First, try to claim a pending task (most common case)
    const pendingResult = await this.db.execute<RawRecordingTask>(sql`
      UPDATE recording_tasks
      SET
        status = 'running',
        started_at = ${now},
        last_heartbeat = ${now},
        updated_at = ${now}
      WHERE id = (
        SELECT id FROM recording_tasks
        WHERE status = 'pending'
          ${sourceFilter}
          ${buildTaskFilter}
        ORDER BY id ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
      )
      RETURNING *
    `)

    if (pendingResult.rows.length > 0) {
      return this.mapRawToRecordingTask(pendingResult.rows[0])
    }

    // No pending tasks - try to recover a stale running task
    // Use a loop to handle tasks that exceed maxAttempts
    while (true) {
      const staleResult = await this.db.execute<RawRecordingTask>(sql`
        UPDATE recording_tasks
        SET
          status = 'running',
          started_at = ${now},
          last_heartbeat = ${now},
          attempt_count = attempt_count + 1,
          error_message = NULL,
          updated_at = ${now}
        WHERE id = (
          SELECT id FROM recording_tasks
          WHERE status = 'running'
            AND (
              last_heartbeat < ${staleThreshold}
              OR (last_heartbeat IS NULL AND updated_at < ${staleThreshold})
            )
            ${sourceFilter}
            ${buildTaskFilter}
          ORDER BY id ASC
          LIMIT 1
          FOR UPDATE SKIP LOCKED
        )
        RETURNING *
      `)

      if (staleResult.rows.length === 0) {
        return null // No stale tasks either
      }

      const task = this.mapRawToRecordingTask(staleResult.rows[0])

      // Check if task has exceeded max attempts (attemptCount was just incremented)
      if (task.attemptCount > maxAttempts) {
        console.log(
          `[TaskScheduler] Task ${task.id} exceeded max attempts (${maxAttempts}), marking as failed`
        )
        await this.markFailed(
          task.id,
          `Exceeded max attempts (${maxAttempts}) after timeout`
        )
        // Continue loop to find another stale task
        continue
      }

      console.log(
        `[TaskScheduler] Recovering stale task ${task.id} (attempt ${task.attemptCount}/${maxAttempts})`
      )
      return task
    }
  }

  /**
   * Map raw SQL result (snake_case) to RecordingTask (camelCase)
   */
  private mapRawToRecordingTask(
    row: typeof TaskScheduler.RawRecordingTaskType
  ): RecordingTask {
    const config = (row.config ?? { chunk_type: 'exploratory' }) as TaskConfig
    return {
      id: row.id,
      sourceId: row.source_id,
      chunkId: row.chunk_id,
      startUrl: row.start_url,
      status: row.status as RecordingTaskStatus,
      progress: row.progress,
      config,
      attemptCount: row.attempt_count,
      errorMessage: row.error_message,
      completedAt: row.completed_at,
      lastHeartbeat: row.last_heartbeat,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }
  }
}
