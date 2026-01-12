/**
 * BuildTaskRunner - Build Task Runner
 *
 * Responsibilities:
 * - Handle lifecycle of a single build_task
 * - Generate recording_tasks and write to DB
 * - Poll recording_tasks status (read from DB)
 * - Handle retry logic
 * - Update build_task status after all complete
 *
 * Features:
 * - Non-blocking: Does not wait for recording_task execution, only generates and polls
 * - Stateless: Reads state from DB each time, does not retain in memory
 * - Concurrent: Multiple Runners can run simultaneously, handling different build_tasks
 */

import type { Database } from '@actionbookdev/db';
import { eq, and, inArray, sql } from 'drizzle-orm';
import { buildTasks, recordingTasks, chunks, documents, sourceVersions } from '@actionbookdev/db';
import type { BuildTaskInfo } from './types/index.js';
import type { SourceVersionStatus } from '@actionbookdev/db';

export interface BuildTaskRunnerConfig {
  /** Status check interval (seconds) */
  checkIntervalSeconds?: number;
  /** Max retry attempts */
  maxAttempts?: number;
  /** Heartbeat interval (milliseconds) */
  heartbeatIntervalMs?: number;
}

interface RecordingTaskStatus {
  pending: number;
  running: number;
  completed: number;
  failed: number;
}

export class BuildTaskRunner {
  private db: Database;
  private buildTaskId: number;
  private config: Required<BuildTaskRunnerConfig>;
  private heartbeatTimer?: NodeJS.Timeout;
  private running = false;

  constructor(db: Database, buildTaskId: number, config: BuildTaskRunnerConfig = {}) {
    this.db = db;
    this.buildTaskId = buildTaskId;
    this.config = {
      checkIntervalSeconds: config.checkIntervalSeconds ?? 5,
      maxAttempts: config.maxAttempts ?? 3,
      heartbeatIntervalMs: config.heartbeatIntervalMs ?? 5000,
    };
  }

  /**
   * Run complete build_task lifecycle
   */
  async run(): Promise<void> {
    this.running = true;

    try {
      // 1. Get build_task details
      const buildTask = await this.getBuildTask();
      if (!buildTask) {
        throw new Error(`Build task ${this.buildTaskId} not found`);
      }

      // 2. Generate recording_tasks
      console.log(
        `[BuildTaskRunner #${this.buildTaskId}] Generating recording tasks...`
      );
      const tasksCreated = await this.generateRecordingTasks(buildTask);
      console.log(
        `[BuildTaskRunner #${this.buildTaskId}] Generated ${tasksCreated} recording tasks`
      );

      // 3. If no tasks generated, mark as completed directly
      if (tasksCreated === 0) {
        await this.completeBuildTask();
        console.log(
          `[BuildTaskRunner #${this.buildTaskId}] No tasks to process, marked as completed`
        );
        return;
      }

      // 4. Start heartbeat
      this.startHeartbeat();

      // 5. Enter polling loop
      await this.pollUntilComplete();

      // 6. Stop heartbeat
      this.stopHeartbeat();

      // 7. Publish new version (Blue-Green deployment)
      await this.publishVersion(buildTask);

      // 8. Update build_task to completed
      await this.completeBuildTask();
      console.log(`[BuildTaskRunner #${this.buildTaskId}] Completed successfully`);
    } catch (error) {
      this.stopHeartbeat();
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(
        `[BuildTaskRunner #${this.buildTaskId}] Error:`,
        error instanceof Error ? error.stack : errorMessage
      );
      await this.failBuildTask(errorMessage);
      throw error;
    } finally {
      this.running = false;
    }
  }

  /**
   * Get build_task information
   */
  private async getBuildTask(): Promise<BuildTaskInfo | null> {
    const results = await this.db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, this.buildTaskId))
      .limit(1);

    if (results.length === 0) {
      return null;
    }

    const task = results[0];
    return {
      id: task.id,
      sourceId: task.sourceId,
      sourceVersionId: task.sourceVersionId, // Include sourceVersionId
      sourceUrl: task.sourceUrl,
      sourceName: task.sourceName,
      sourceCategory: task.sourceCategory,
      stage: task.stage,
      stageStatus: task.stageStatus,
      config: task.config as any,
      knowledgeStartedAt: task.knowledgeStartedAt,
      knowledgeCompletedAt: task.knowledgeCompletedAt,
      actionStartedAt: task.actionStartedAt,
      actionCompletedAt: task.actionCompletedAt,
      createdAt: task.createdAt,
      updatedAt: task.updatedAt,
    };
  }

  /**
   * Generate recording_tasks
   * Use atomic transaction + UPSERT (ON CONFLICT DO UPDATE)
   */
  private async generateRecordingTasks(buildTask: BuildTaskInfo): Promise<number> {
    // 1. Get all chunks associated with this build_task (linked via documents table using sourceVersionId)
    const chunkResults = await this.db
      .select({
        id: chunks.id,
        documentId: chunks.documentId,
        url: documents.url,
      })
      .from(chunks)
      .innerJoin(documents, eq(chunks.documentId, documents.id))
      .where(
        and(
          eq(documents.sourceId, buildTask.sourceId!),
          eq(documents.sourceVersionId, buildTask.sourceVersionId!)
        )
      )
      .orderBy(chunks.id);

    if (chunkResults.length === 0) {
      return 0;
    }

    // 2. Prepare recording_tasks data
    // Pass actionBuilderPrompt from build task config to recording task config
    const actionBuilderPrompt = (buildTask.config as any)?.actionBuilderPrompt;
    const recordingTasksData = chunkResults.map((chunk) => ({
      sourceId: buildTask.sourceId!,
      buildTaskId: this.buildTaskId,
      chunkId: chunk.id,
      startUrl: chunk.url,
      status: 'pending' as const,
      progress: 0,
      attemptCount: 0,
      config: {
        chunk_type: 'task_driven' as const,
        ...(actionBuilderPrompt ? { actionBuilderPrompt } : {}),
      },
    }));

    // 3. Atomic insert (ON CONFLICT DO UPDATE)
    // Based on status: skip completed/failed, reset pending/running
    await this.db.transaction(async (tx) => {
      for (const data of recordingTasksData) {
        await tx
          .insert(recordingTasks)
          .values(data)
          .onConflictDoUpdate({
            target: [recordingTasks.chunkId, recordingTasks.buildTaskId],
            set: {
              // Only reset when status is pending/running
              status: sql`CASE
                WHEN ${recordingTasks.status} IN ('pending', 'running') THEN 'pending'
                ELSE ${recordingTasks.status}
              END`,
              updatedAt: new Date(),
            },
          });
      }

      // Update build_task status to running
      await tx
        .update(buildTasks)
        .set({
          stage: 'action_build',
          stageStatus: 'running',
          actionStartedAt: new Date(),
          updatedAt: new Date(),
        })
        .where(eq(buildTasks.id, this.buildTaskId));
    });

    return recordingTasksData.length;
  }

  /**
   * Poll and check all recording_tasks status
   */
  private async pollUntilComplete(): Promise<void> {
    while (this.running) {
      // 1. Query all recording_tasks status from DB
      const status = await this.getRecordingTasksStatus();

      console.log(
        `[BuildTaskRunner #${this.buildTaskId}] Status: ` +
          `pending=${status.pending}, running=${status.running}, ` +
          `completed=${status.completed}, failed=${status.failed}`
      );

      // 2. Retry failed tasks (retriable only). If retried any, we must NOT finish.
      const retriedCount = await this.retryFailedTasks();

      // 3. Check if all finished:
      // - pending=0 and running=0
      // - and no retriable failures were just re-queued
      //
      // Note: permanent failures (attemptCount >= maxAttempts) do NOT block completion.
      const allFinished =
        status.pending === 0 && status.running === 0 && retriedCount === 0;

      if (allFinished) {
        console.log(
          `[BuildTaskRunner #${this.buildTaskId}] All recording tasks finished`
        );
        return;
      }

      // 4. Wait before next poll
      await this.sleep(this.config.checkIntervalSeconds * 1000);
    }
  }

  /**
   * Get recording_tasks status statistics
   */
  private async getRecordingTasksStatus(): Promise<RecordingTaskStatus> {
    const results = await this.db
      .select({
        status: recordingTasks.status,
        count: sql<number>`COUNT(*)::int`,
      })
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, this.buildTaskId))
      .groupBy(recordingTasks.status);

    const status: RecordingTaskStatus = {
      pending: 0,
      running: 0,
      completed: 0,
      failed: 0,
    };

    for (const row of results) {
      if (row.status in status) {
        status[row.status as keyof RecordingTaskStatus] = row.count;
      }
    }

    return status;
  }

  /**
   * Retry failed tasks
   * Only reset tasks where attemptCount < maxAttempts
   */
  private async retryFailedTasks(): Promise<number> {
    // 1. Find retriable failed tasks
    const failedTasks = await this.db
      .select()
      .from(recordingTasks)
      .where(
        and(
          eq(recordingTasks.buildTaskId, this.buildTaskId),
          eq(recordingTasks.status, 'failed'),
          sql`${recordingTasks.attemptCount} < ${this.config.maxAttempts}`
        )
      );

    if (failedTasks.length === 0) {
      return 0;
    }

    console.log(
      `[BuildTaskRunner #${this.buildTaskId}] Retrying ${failedTasks.length} failed tasks`
    );

    // 2. Reset to pending
    const taskIds = failedTasks.map((t) => t.id);
    await this.db
      .update(recordingTasks)
      .set({
        status: 'pending',
        progress: 0,
        errorMessage: null,
        startedAt: null,
        completedAt: null,
        durationMs: null,
        tokensUsed: 0,
        updatedAt: new Date(),
      })
      .where(inArray(recordingTasks.id, taskIds));

    return failedTasks.length;
  }

  /**
   * Start heartbeat
   */
  private startHeartbeat(): void {
    this.heartbeatTimer = setInterval(() => {
      this.updateHeartbeat().catch((error) => {
        console.error(
          `[BuildTaskRunner #${this.buildTaskId}] Heartbeat error:`,
          error
        );
      });
    }, this.config.heartbeatIntervalMs);
  }

  /**
   * Stop heartbeat
   */
  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = undefined;
    }
  }

  /**
   * Update heartbeat time
   */
  private async updateHeartbeat(): Promise<void> {
    await this.db
      .update(buildTasks)
      .set({
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, this.buildTaskId));
  }

  /**
   * Mark build_task as completed
   */
  private async completeBuildTask(): Promise<void> {
    await this.db
      .update(buildTasks)
      .set({
        stageStatus: 'completed',
        actionCompletedAt: new Date(),
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, this.buildTaskId));
  }

  /**
   * Publish version (Blue-Green deployment)
   * 1. Get the source_version linked to this build_task
   * 2. Get current active version and archive it
   * 3. Update the linked source_version status to active (building → active)
   */
  private async publishVersion(buildTask: BuildTaskInfo): Promise<void> {
    try {
      // 1. Get the source_version linked to this build_task
      const linkedVersion = await this.db
        .select()
        .from(buildTasks)
        .where(eq(buildTasks.id, this.buildTaskId))
        .limit(1);

      const sourceVersionId = linkedVersion[0]?.sourceVersionId;

      if (!sourceVersionId) {
        console.warn(
          `[BuildTaskRunner #${this.buildTaskId}] No source_version linked to this build_task, skipping version publish`
        );
        return;
      }

      // 2. Get current active version (if any) and archive it
      const currentActiveVersions = await this.db
        .select()
        .from(sourceVersions)
        .where(
          and(
            eq(sourceVersions.sourceId, buildTask.sourceId!),
            eq(sourceVersions.status, 'active' as SourceVersionStatus)
          )
        )
        .limit(1);

      // Archive current active version
      if (currentActiveVersions.length > 0) {
        await this.db
          .update(sourceVersions)
          .set({
            status: 'archived' as SourceVersionStatus,
          })
          .where(eq(sourceVersions.id, currentActiveVersions[0].id));

        console.log(
          `[BuildTaskRunner #${this.buildTaskId}] Archived previous active version #${currentActiveVersions[0].id} (v${currentActiveVersions[0].versionNumber})`
        );
      }

      // 3. Update linked source_version status: building → active
      const updatedVersion = await this.db
        .update(sourceVersions)
        .set({
          status: 'active' as SourceVersionStatus,
          commitMessage: `Action build completed (build_task #${this.buildTaskId})`,
          publishedAt: new Date(),
        })
        .where(eq(sourceVersions.id, sourceVersionId))
        .returning({ id: sourceVersions.id, versionNumber: sourceVersions.versionNumber });

      console.log(
        `[BuildTaskRunner #${this.buildTaskId}] Published version #${updatedVersion[0].id} (v${updatedVersion[0].versionNumber}) - status: building → active`
      );
    } catch (error) {
      console.error(
        `[BuildTaskRunner #${this.buildTaskId}] Failed to publish version:`,
        error
      );
      // Don't throw - version publishing failure should not block build_task completion
    }
  }

  /**
   * Mark build_task as error
   */
  private async failBuildTask(errorMessage: string): Promise<void> {
    await this.db
      .update(buildTasks)
      .set({
        stageStatus: 'error',
        config: sql`jsonb_set(
          COALESCE(${buildTasks.config}, '{}'::jsonb),
          '{lastError}',
          ${JSON.stringify(errorMessage)}::jsonb
        )`,
        updatedAt: new Date(),
      })
      .where(eq(buildTasks.id, this.buildTaskId));
  }

  /**
   * Sleep utility function
   */
  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Stop running
   */
  stop(): void {
    this.running = false;
    this.stopHeartbeat();
  }
}
