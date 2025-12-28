/**
 * BuildTaskScheduler - Build Task Scheduler
 *
 * Manages build_tasks table operations for the action_build stage.
 * Queries for tasks ready for action building and updates their status.
 *
 * Stale Task Recovery:
 * - Tasks stuck in 'action_build/running' state (e.g., after crash) can be recovered
 * - Stale timeout: tasks running longer than threshold are considered stale
 * - Retry limit: stale tasks are retried up to maxAttempts times
 * - Priority: stale running tasks > new pending tasks
 */

import type {
  Database,
  BuildTaskConfig,
  SourceVersionStatus,
} from '@actionbookdev/db'
import {
  buildTasks,
  sourceVersions,
  sources,
  eq,
  and,
  asc,
  isNotNull,
  sql,
} from '@actionbookdev/db'
import type {
  BuildTaskInfo,
  BuildTaskStats,
  BuildTaskSchedulerConfig,
} from './types/index.js'

/**
 * Publish version result type
 */
export interface PublishVersionResult {
  success: boolean
  versionId?: number
  archivedVersionId?: number | null
  error?: string
}

export class BuildTaskScheduler {
  private maxAttempts: number
  private staleTimeoutMinutes: number

  constructor(private db: Database, config?: BuildTaskSchedulerConfig) {
    this.maxAttempts = config?.maxAttempts ?? 3
    this.staleTimeoutMinutes = config?.staleTimeoutMinutes ?? 10
  }

  /**
   * Atomically claim the next build_task for action_build stage.
   *
   * Priority:
   *   1. Stale running tasks (action_build/running, timed out) - resume interrupted work
   *   2. New pending tasks (knowledge_build/completed) - start new work
   *
   * Stale tasks that exceed maxAttempts are marked as failed and skipped.
   *
   * @returns The claimed task, or null if no tasks available
   */
  async claimNextActionTask(): Promise<BuildTaskInfo | null> {
    // 1. First try to claim a stale running task (priority)
    const staleTask = await this.claimStaleRunningTask()
    if (staleTask) {
      return staleTask
    }

    // 2. Then try to claim a new pending task
    return this.claimNewPendingTask()
  }

  /**
   * Claim a stale running task (action_build/running that has timed out)
   *
   * Uses atomic UPDATE...WHERE...RETURNING for concurrency safety.
   * If the task has exceeded maxAttempts, it will be marked as failed and skipped.
   *
   * Uses while-loop instead of recursion to avoid stack overflow with many stale tasks.
   */
  private async claimStaleRunningTask(): Promise<BuildTaskInfo | null> {
    type RawBuildTask = {
      id: number
      source_id: number | null
      source_url: string
      source_name: string | null
      source_category: string
      stage: string
      stage_status: string
      config: BuildTaskConfig | null
      created_at: Date
      updated_at: Date
      knowledge_started_at: Date | null
      knowledge_completed_at: Date | null
      action_started_at: Date | null
      action_completed_at: Date | null
      [key: string]: unknown
    }

    // Use while-loop instead of recursion to avoid stack overflow
    while (true) {
      // Calculate now and staleThreshold each iteration for accurate timestamps
      const now = new Date()
      const staleThreshold = new Date(
        now.getTime() - this.staleTimeoutMinutes * 60 * 1000
      )

      // Atomic UPDATE...WHERE...RETURNING to claim stale task
      // This ensures only one worker can claim each stale task
      const claimResult = await this.db.execute<RawBuildTask>(sql`
        UPDATE build_tasks
        SET
          updated_at = ${now}
        WHERE id = (
          SELECT id FROM build_tasks
          WHERE stage = 'action_build'
            AND stage_status = 'running'
            AND updated_at < ${staleThreshold}
          ORDER BY updated_at ASC
          LIMIT 1
          FOR UPDATE SKIP LOCKED
        )
        RETURNING *
      `)

      if (claimResult.rows.length === 0) {
        return null
      }

      const staleRow = claimResult.rows[0]
      const config = (staleRow.config ?? {}) as BuildTaskConfig
      const attemptCount =
        typeof config.attemptCount === 'number' ? config.attemptCount : 0

      // Check if max attempts exceeded
      if (attemptCount >= this.maxAttempts) {
        // Mark as failed (permanent error)
        console.log(
          `[BuildTaskScheduler] Task ${staleRow.id} exceeded max attempts (${this.maxAttempts}), marking as failed`
        )
        await this.db.execute(sql`
          UPDATE build_tasks
          SET
            stage = 'error',
            stage_status = 'error',
            updated_at = ${now},
            config = ${JSON.stringify({
              ...config,
              attemptCount: attemptCount + 1,
              lastError: `Exceeded max attempts (${this.maxAttempts}) after timeout`,
            })}::jsonb
          WHERE id = ${staleRow.id}
        `)
        // Continue to find another stale task (using loop instead of recursion)
        continue
      }

      // Claim the stale task - update config with incremented attemptCount
      console.log(
        `[BuildTaskScheduler] Recovering stale task ${staleRow.id} (attempt ${
          attemptCount + 1
        }/${this.maxAttempts})`
      )

      const newConfig = {
        ...config,
        attemptCount: attemptCount + 1,
        lastError: `Recovered from stale running state (timeout: ${this.staleTimeoutMinutes}min)`,
      }

      await this.db.execute(sql`
        UPDATE build_tasks
        SET
          action_started_at = ${now},
          config = ${JSON.stringify(newConfig)}::jsonb
        WHERE id = ${staleRow.id}
      `)

      return this.mapRawToTaskInfo({
        ...staleRow,
        stage: 'action_build',
        stage_status: 'running',
        action_started_at: now,
        updated_at: now,
        config: newConfig,
      })
    }
  }

  /**
   * Claim a new pending task (knowledge_build/completed)
   */
  private async claimNewPendingTask(): Promise<BuildTaskInfo | null> {
    const now = new Date()

    type RawBuildTask = {
      id: number
      source_id: number | null
      source_url: string
      source_name: string | null
      source_category: string
      stage: string
      stage_status: string
      config: BuildTaskConfig | null
      created_at: Date
      updated_at: Date
      knowledge_started_at: Date | null
      knowledge_completed_at: Date | null
      action_started_at: Date | null
      action_completed_at: Date | null
      [key: string]: unknown
    }

    // Atomic UPDATE...WHERE...RETURNING using raw SQL for concurrency safety
    // Only one worker will successfully claim each task
    const result = await this.db.execute<RawBuildTask>(sql`
      UPDATE build_tasks
      SET
        stage = 'action_build',
        stage_status = 'running',
        action_started_at = ${now},
        updated_at = ${now}
      WHERE id = (
        SELECT id FROM build_tasks
        WHERE stage = 'knowledge_build'
          AND stage_status = 'completed'
          AND source_id IS NOT NULL
        ORDER BY created_at ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
      )
      RETURNING *
    `)

    if (result.rows.length === 0) {
      return null
    }

    // Map snake_case raw result to camelCase BuildTaskInfo
    const row = result.rows[0]
    return this.mapRawToTaskInfo(row)
  }

  /**
   * Map raw SQL result (snake_case) to BuildTaskInfo (camelCase)
   */
  private mapRawToTaskInfo(row: {
    id: number
    source_id: number | null
    source_url: string
    source_name: string | null
    source_category: string
    stage: string
    stage_status: string
    config: BuildTaskConfig | null
    created_at: Date
    updated_at: Date
    knowledge_started_at: Date | null
    knowledge_completed_at: Date | null
    action_started_at: Date | null
    action_completed_at: Date | null
  }): BuildTaskInfo {
    const config = (row.config ?? {}) as BuildTaskConfig
    return {
      id: row.id,
      sourceId: row.source_id,
      sourceUrl: row.source_url,
      sourceName: row.source_name,
      sourceCategory: row.source_category as 'help' | 'unknown',
      stage: row.stage as BuildTaskInfo['stage'],
      stageStatus: row.stage_status as BuildTaskInfo['stageStatus'],
      config,
      knowledgeStartedAt: row.knowledge_started_at,
      knowledgeCompletedAt: row.knowledge_completed_at,
      actionStartedAt: row.action_started_at,
      actionCompletedAt: row.action_completed_at,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }
  }

  /**
   * Get next build_task ready for action_build stage (read-only, for inspection)
   *
   * WARNING: This method is NOT safe for concurrent use. Use claimNextActionTask()
   * for actual task claiming in production with multiple workers.
   *
   * Conditions:
   *   - stage = 'knowledge_build' AND stageStatus = 'completed'
   *   - sourceId IS NOT NULL (source must be created by knowledge builder)
   *
   * Order by: createdAt ASC (FIFO)
   */
  async getNextActionTask(): Promise<BuildTaskInfo | null> {
    const result = await this.db
      .select()
      .from(buildTasks)
      .where(
        and(
          eq(buildTasks.stage, 'knowledge_build'),
          eq(buildTasks.stageStatus, 'completed'),
          isNotNull(buildTasks.sourceId)
        )
      )
      .orderBy(asc(buildTasks.createdAt))
      .limit(1)

    if (result.length === 0) {
      return null
    }

    return this.mapToTaskInfo(result[0])
  }

  /**
   * Get build_task by ID
   */
  async getTaskById(taskId: number): Promise<BuildTaskInfo | null> {
    const result = await this.db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, taskId))
      .limit(1)

    if (result.length === 0) {
      return null
    }

    return this.mapToTaskInfo(result[0])
  }

  /**
   * Start action_build stage
   *
   * Updates:
   *   - stage = 'action_build'
   *   - stageStatus = 'running'
   *   - actionStartedAt = NOW()
   *   - updatedAt = NOW()
   */
  async startActionStage(taskId: number): Promise<void> {
    const now = new Date()
    await this.db
      .update(buildTasks)
      .set({
        stage: 'action_build',
        stageStatus: 'running',
        actionStartedAt: now,
        updatedAt: now,
      })
      .where(eq(buildTasks.id, taskId))
  }

  /**
   * Update task heartbeat (call periodically during long-running tasks)
   *
   * This updates updated_at to prevent the task from being considered stale
   * during long-running execution.
   */
  async updateHeartbeat(taskId: number): Promise<void> {
    const now = new Date()
    await this.db
      .update(buildTasks)
      .set({
        updatedAt: now,
      })
      .where(eq(buildTasks.id, taskId))
  }

  /**
   * Complete build_task successfully
   *
   * Updates:
   *   - stage = 'completed'
   *   - stageStatus = 'completed'
   *   - actionCompletedAt = NOW()
   *   - updatedAt = NOW()
   *   - config.stats = stats (optional)
   */
  async completeTask(taskId: number, stats?: BuildTaskStats): Promise<void> {
    const now = new Date()
    const task = await this.getTaskById(taskId)
    const currentConfig = (task?.config ?? {}) as BuildTaskConfig

    const newConfig: BuildTaskConfig = stats
      ? { ...currentConfig, stats }
      : currentConfig

    await this.db
      .update(buildTasks)
      .set({
        stage: 'completed',
        stageStatus: 'completed',
        actionCompletedAt: now,
        updatedAt: now,
        config: newConfig,
      })
      .where(eq(buildTasks.id, taskId))
  }

  /**
   * Handle task failure with retry logic
   *
   * If retries exhausted:
   *   - stage = 'error'
   *   - stageStatus = 'error'
   *   - config.lastError = errorMessage
   *
   * Otherwise (still has retries):
   *   - Keep stage = 'knowledge_build', stageStatus = 'completed' so it can be retried
   *   - config.attemptCount++
   *   - config.lastError = errorMessage
   */
  async failTask(taskId: number, errorMessage: string): Promise<void> {
    const now = new Date()
    const task = await this.getTaskById(taskId)
    const currentConfig = (task?.config ?? {}) as BuildTaskConfig
    const currentAttemptCount =
      typeof currentConfig.attemptCount === 'number'
        ? currentConfig.attemptCount
        : 0
    const newAttemptCount = currentAttemptCount + 1

    if (newAttemptCount >= this.maxAttempts) {
      // Max retries reached - mark as permanent error
      await this.db
        .update(buildTasks)
        .set({
          stage: 'error',
          stageStatus: 'error',
          updatedAt: now,
          config: {
            ...currentConfig,
            attemptCount: newAttemptCount,
            lastError: errorMessage,
          },
        })
        .where(eq(buildTasks.id, taskId))
    } else {
      // Still has retries - explicitly reset to knowledge_build/completed so it can be retried
      // (This is important if the failure happened after startActionStage and the task is in action_build/running.)
      await this.db
        .update(buildTasks)
        .set({
          stage: 'knowledge_build',
          stageStatus: 'completed',
          updatedAt: now,
          config: {
            ...currentConfig,
            attemptCount: newAttemptCount,
            lastError: errorMessage,
          },
        })
        .where(eq(buildTasks.id, taskId))
    }
  }

  /**
   * Map database row to BuildTaskInfo
   */
  private mapToTaskInfo(row: typeof buildTasks.$inferSelect): BuildTaskInfo {
    const config = (row.config ?? {}) as BuildTaskConfig
    return {
      id: row.id,
      sourceId: row.sourceId,
      sourceUrl: row.sourceUrl,
      sourceName: row.sourceName,
      sourceCategory: row.sourceCategory,
      stage: row.stage,
      stageStatus: row.stageStatus,
      config,
      knowledgeStartedAt: row.knowledgeStartedAt,
      knowledgeCompletedAt: row.knowledgeCompletedAt,
      actionStartedAt: row.actionStartedAt,
      actionCompletedAt: row.actionCompletedAt,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    }
  }

  // =========================================================================
  // Version Publishing
  // =========================================================================

  /**
   * Publish the 'building' version for a source (Blue-Green deployment)
   *
   * This method performs an atomic switch:
   *   1. Find the 'building' version for the given sourceId
   *   2. Archive the current 'active' version (if exists)
   *   3. Set the 'building' version to 'active'
   *   4. Update sources.currentVersionId
   *
   * @param sourceId - The source ID to publish version for
   * @returns PublishVersionResult with success status and version info
   */
  async publishVersion(sourceId: number): Promise<PublishVersionResult> {
    // 1. Find the latest 'building' version for this source
    // Order by id DESC to get the most recent building version
    const buildingVersionResult = await this.db
      .select()
      .from(sourceVersions)
      .where(
        and(
          eq(sourceVersions.sourceId, sourceId),
          eq(sourceVersions.status, 'building' as SourceVersionStatus)
        )
      )
      .orderBy(sql`${sourceVersions.id} DESC`)
      .limit(1)

    if (buildingVersionResult.length === 0) {
      return {
        success: false,
        error: `No 'building' version found for source ${sourceId}`,
      }
    }

    const versionToPublish = buildingVersionResult[0]
    const publishedAt = new Date()

    // 2. Get current active version (to archive it)
    const sourceResult = await this.db
      .select({ currentVersionId: sources.currentVersionId })
      .from(sources)
      .where(eq(sources.id, sourceId))
      .limit(1)

    if (sourceResult.length === 0) {
      return {
        success: false,
        error: `Source ${sourceId} not found`,
      }
    }

    const oldActiveVersionId = sourceResult[0].currentVersionId

    // 3. Perform atomic switch in transaction
    await this.db.transaction(async (tx) => {
      // 3a. Archive old active version (if exists)
      if (oldActiveVersionId) {
        await tx
          .update(sourceVersions)
          .set({ status: 'archived' as SourceVersionStatus })
          .where(eq(sourceVersions.id, oldActiveVersionId))
      }

      // 3b. Set new version to active
      await tx
        .update(sourceVersions)
        .set({
          status: 'active' as SourceVersionStatus,
          publishedAt,
        })
        .where(eq(sourceVersions.id, versionToPublish.id))

      // 3c. Update source's currentVersionId
      await tx
        .update(sources)
        .set({
          currentVersionId: versionToPublish.id,
          updatedAt: publishedAt,
        })
        .where(eq(sources.id, sourceId))
    })

    return {
      success: true,
      versionId: versionToPublish.id,
      archivedVersionId: oldActiveVersionId,
    }
  }

  // =========================================================================
  // Task Statistics
  // =========================================================================

  /**
   * Get current task statistics for monitoring
   *
   * Returns counts of tasks in various stages/statuses for both
   * build_tasks and recording_tasks.
   */
  async getTaskStats(): Promise<TaskStatsResult> {
    // Query build_tasks stats
    const buildTaskStats = await this.db.execute<{
      stage: string
      stage_status: string
      count: string
    }>(sql`
      SELECT stage, stage_status, COUNT(*)::text as count
      FROM build_tasks
      WHERE stage IN ('knowledge_build', 'action_build')
      GROUP BY stage, stage_status
    `)

    // Query recording_tasks stats
    const recordingTaskStats = await this.db.execute<{
      status: string
      count: string
    }>(sql`
      SELECT status, COUNT(*)::text as count
      FROM recording_tasks
      WHERE status IN ('pending', 'running', 'failed')
      GROUP BY status
    `)

    // Parse build_tasks results
    const buildTasks = {
      knowledge_build: { pending: 0, running: 0, completed: 0 },
      action_build: { pending: 0, running: 0, completed: 0 },
    }

    for (const row of buildTaskStats.rows) {
      const stage = row.stage as keyof typeof buildTasks
      const status = row.stage_status as 'pending' | 'running' | 'completed'
      if (buildTasks[stage] && status in buildTasks[stage]) {
        buildTasks[stage][status] = parseInt(row.count, 10)
      }
    }

    // Parse recording_tasks results
    const recordingTaskCounts = { pending: 0, running: 0, failed: 0 }
    for (const row of recordingTaskStats.rows) {
      const status = row.status as keyof typeof recordingTaskCounts
      if (status in recordingTaskCounts) {
        recordingTaskCounts[status] = parseInt(row.count, 10)
      }
    }

    return {
      buildTasks,
      recordingTasks: recordingTaskCounts,
    }
  }
}

/**
 * Task statistics result type
 */
export interface TaskStatsResult {
  buildTasks: {
    knowledge_build: { pending: number; running: number; completed: number }
    action_build: { pending: number; running: number; completed: number }
  }
  recordingTasks: {
    pending: number
    running: number
    failed: number
  }
}
