/**
 * Coordinator
 *
 * Responsibilities:
 * - Start and manage RecordingTaskQueueWorker
 * - Continuously claim new build_tasks and start BuildTaskRunners
 * - Control max concurrent build_tasks
 * - Handle graceful shutdown
 */

import type { Database } from '@actionbookdev/db';
import { buildTasks, recordingTasks } from '@actionbookdev/db';
import { sql, eq } from 'drizzle-orm';
import { BuildTaskRunner, type BuildTaskRunnerConfig } from './build-task-runner.js';
import {
  RecordingTaskQueueWorker,
  type RecordingTaskQueueWorkerConfig,
} from './recording-task-queue-worker.js';

export interface CoordinatorConfig {
  /** Max concurrent build_tasks */
  maxConcurrentBuildTasks?: number;
  /** Build task polling interval (seconds) */
  buildTaskPollIntervalSeconds?: number;
  /** Stale build_task timeout (minutes) for crash recovery of action_build/running tasks */
  buildTaskStaleTimeoutMinutes?: number;
  /** BuildTaskRunner configuration */
  buildTaskRunner?: BuildTaskRunnerConfig;
  /** RecordingTaskQueueWorker configuration */
  queueWorker?: RecordingTaskQueueWorkerConfig;
}

interface RunningBuildTask {
  id: number;
  runner: BuildTaskRunner;
  promise: Promise<void>;
}

export class Coordinator {
  private db: Database;
  private config: Required<CoordinatorConfig>;
  private queueWorker: RecordingTaskQueueWorker;
  private runningBuildTasks = new Map<number, RunningBuildTask>();
  private running = false;
  private metricsTimer?: NodeJS.Timeout;
  private lastMetricsTime = Date.now();
  private metricsIntervalMs = 30000; // 30 seconds
  // Error tracking for exponential backoff
  private consecutiveErrors = 0;
  private maxBackoffMs = 30000; // Max 30 seconds backoff

  constructor(db: Database, config: CoordinatorConfig = {}) {
    this.db = db;
    this.config = {
      maxConcurrentBuildTasks: config.maxConcurrentBuildTasks ?? 5,
      buildTaskPollIntervalSeconds: config.buildTaskPollIntervalSeconds ?? 5,
      buildTaskStaleTimeoutMinutes: config.buildTaskStaleTimeoutMinutes ?? 15,
      buildTaskRunner: config.buildTaskRunner ?? {},
      queueWorker: config.queueWorker ?? {} as RecordingTaskQueueWorkerConfig,
    };

    // Create QueueWorker
    this.queueWorker = new RecordingTaskQueueWorker(
      db,
      this.config.queueWorker
    );
  }

  /**
   * Start coordinator
   */
  async start(): Promise<void> {
    if (this.running) {
      console.log('[Coordinator] Already running');
      return;
    }

    this.running = true;
    console.log(
      `[Coordinator] Starting with maxConcurrentBuildTasks=${this.config.maxConcurrentBuildTasks}`
    );

    // 1. Start QueueWorker (background)
    this.queueWorker.start().catch((error: unknown) => {
      console.error('[Coordinator] QueueWorker error:', error);
    });

    // 2. Start metrics output
    this.startMetrics();

    // 3. Enter main loop
    await this.mainLoop();
  }

  /**
   * Stop coordinator (graceful shutdown)
   */
  async stop(timeoutMs?: number): Promise<void> {
    if (!this.running) {
      return;
    }

    console.log('[Coordinator] Stopping gracefully...');
    this.running = false;

    // 1. Stop metrics output
    this.stopMetrics();

    // 2. Stop QueueWorker
    await this.queueWorker.stop(timeoutMs);

    // 3. Wait for all BuildTaskRunners to complete
    const startTime = Date.now();
    while (this.runningBuildTasks.size > 0) {
      if (timeoutMs && Date.now() - startTime > timeoutMs) {
        console.log(
          `[Coordinator] Graceful shutdown timeout. ` +
            `${this.runningBuildTasks.size} build tasks still running`
        );
        break;
      }
      await this.sleep(100);
    }

    console.log('[Coordinator] Stopped');
  }

  /**
   * Main loop
   */
  private async mainLoop(): Promise<void> {
    while (this.running) {
      try {
        // 1. Cleanup completed build_tasks
        this.cleanupCompletedTasks();

        // 2. Claim new build_tasks if slots available
        while (
          this.running &&
          this.runningBuildTasks.size < this.config.maxConcurrentBuildTasks
        ) {
          const buildTask = await this.claimBuildTask();

          if (!buildTask) {
            // No claimable build_task
            break;
          }

          // Start BuildTaskRunner (non-blocking)
          this.startBuildTaskRunner(buildTask.id);
        }

        // Reset error counter on successful iteration
        this.consecutiveErrors = 0;

        // 3. Wait before continuing
        await this.sleep(this.config.buildTaskPollIntervalSeconds * 1000);
      } catch (error) {
        this.consecutiveErrors++;

        // Exponential backoff: 2^n seconds, capped at maxBackoffMs
        const backoffMs = Math.min(
          Math.pow(2, this.consecutiveErrors) * 1000,
          this.maxBackoffMs
        );

        console.error(
          `[Coordinator] Main loop error (${this.consecutiveErrors} consecutive):`,
          error instanceof Error ? error.message : error
        );
        console.log(`[Coordinator] Backing off for ${backoffMs / 1000}s before retry...`);

        await this.sleep(backoffMs);
      }
    }
  }

  /**
   * Claim a build_task
   * Find tasks with stage=knowledge_build, stage_status=completed
   */
  private async claimBuildTask(): Promise<{ id: number } | null> {
    try {
      const staleMs = this.config.buildTaskStaleTimeoutMinutes * 60 * 1000;
      const staleThreshold = new Date(Date.now() - staleMs);
      const result = await this.db.execute(sql`
        UPDATE ${buildTasks}
        SET
          stage = 'action_build',
          stage_status = 'running',
          action_started_at = COALESCE(action_started_at, NOW()),
          updated_at = NOW()
        WHERE id = (
          SELECT id
          FROM ${buildTasks}
          WHERE
            (
              (stage = 'knowledge_build' AND stage_status = 'completed')
              OR
              (stage = 'action_build' AND stage_status = 'running' AND updated_at < ${staleThreshold})
            )
          ORDER BY
            CASE
              WHEN stage = 'action_build' AND stage_status = 'running' THEN 0
              ELSE 1
            END,
            id
          LIMIT 1
          FOR UPDATE SKIP LOCKED
        )
        RETURNING id
      `);

      if (result.rows.length === 0) {
        return null;
      }

      const id = (result.rows[0] as any).id as number;
      // If this was a stale recovery, it will be in action_build/running already; BuildTaskRunner will re-upsert tasks idempotently.
      return { id };
    } catch (error) {
      // Check if it's a connection error
      const isConnectionError =
        error instanceof Error && (
          error.message.includes('Connection terminated') ||
          error.message.includes('connection timeout') ||
          error.message.includes('ECONNREFUSED') ||
          error.message.includes('ETIMEDOUT')
        );

      if (isConnectionError) {
        console.error(
          '[Coordinator] Database connection error when claiming build_task:',
          error instanceof Error ? error.message : error
        );
      } else {
        console.error('[Coordinator] Failed to claim build_task:', error);
      }
      return null;
    }
  }

  /**
   * Start BuildTaskRunner (non-blocking)
   */
  private startBuildTaskRunner(buildTaskId: number): void {
    console.log(`[Coordinator] Starting BuildTaskRunner #${buildTaskId}`);

    const runner = new BuildTaskRunner(
      this.db,
      buildTaskId,
      this.config.buildTaskRunner
    );

    const promise = runner
      .run()
      .then(() => {
        console.log(`[Coordinator] BuildTaskRunner #${buildTaskId} completed`);
      })
      .catch((error: unknown) => {
        console.error(
          `[Coordinator] BuildTaskRunner #${buildTaskId} error:`,
          error
        );
      })
      .finally(() => {
        this.runningBuildTasks.delete(buildTaskId);
      });

    this.runningBuildTasks.set(buildTaskId, {
      id: buildTaskId,
      runner,
      promise,
    });
  }

  /**
   * Cleanup completed build_tasks
   */
  private cleanupCompletedTasks(): void {
    // Promise's finally already handles deletion, no additional operation needed here
  }

  /**
   * Start metrics output
   */
  private startMetrics(): void {
    this.metricsTimer = setInterval(() => {
      this.outputMetrics().catch((error: unknown) => {
        console.error('[Coordinator] Metrics error:', error);
      });
    }, this.metricsIntervalMs);

    // Output immediately once
    this.outputMetrics().catch((error: unknown) => {
      console.error('[Coordinator] Metrics error:', error);
    });
  }

  /**
   * Stop metrics output
   */
  private stopMetrics(): void {
    if (this.metricsTimer) {
      clearInterval(this.metricsTimer);
      this.metricsTimer = undefined;
    }
  }

  /**
   * Output metrics
   */
  private async outputMetrics(): Promise<void> {
    const queueStatus = this.queueWorker.getStatus();
    const now = Date.now();
    const elapsedSeconds = (now - this.lastMetricsTime) / 1000;

    console.log(
      `[Metrics] ` +
        `build_tasks=${this.runningBuildTasks.size}/${this.config.maxConcurrentBuildTasks}, ` +
        `recording_tasks=${queueStatus.runningTaskCount}/${this.config.queueWorker.concurrency ?? 3}, ` +
        `elapsed=${elapsedSeconds.toFixed(1)}s`
    );

    // Output detailed status for each build_task
    if (this.runningBuildTasks.size > 0) {
      for (const [buildTaskId] of this.runningBuildTasks) {
        try {
          const details = await this.getBuildTaskDetails(buildTaskId);
          if (details) {
            const progress = details.total > 0
              ? ((details.completed + details.failed) / details.total * 100).toFixed(1)
              : '0.0';

            const elapsed = details.startedAt
              ? ((now - details.startedAt.getTime()) / 1000 / 60).toFixed(1)
              : '0.0';

            console.log(
              `  #${buildTaskId} [${details.sourceName}] ` +
                `tasks=${details.completed}+${details.failed}/${details.total} (${progress}%) ` +
                `elapsed=${elapsed}min`
            );
          }
        } catch (error) {
          // Ignore errors in metrics collection
        }
      }
    }

    this.lastMetricsTime = now;
  }

  /**
   * Get build_task detailed information
   */
  private async getBuildTaskDetails(buildTaskId: number): Promise<{
    sourceName: string;
    total: number;
    completed: number;
    failed: number;
    pending: number;
    running: number;
    startedAt: Date | null;
  } | null> {
    try {
      // 1. Get build_task info
      const buildTaskResult = await this.db
        .select({
          sourceName: buildTasks.sourceName,
          startedAt: buildTasks.actionStartedAt,
        })
        .from(buildTasks)
        .where(eq(buildTasks.id, buildTaskId))
        .limit(1);

      if (buildTaskResult.length === 0) {
        return null;
      }

      // 2. Get recording_tasks stats
      const statsResult = await this.db.execute(sql`
        SELECT
          COUNT(*) as total,
          SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as completed,
          SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed,
          SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as pending,
          SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END) as running
        FROM ${recordingTasks}
        WHERE build_task_id = ${buildTaskId}
      `);

      const stats = statsResult.rows[0] as any;

      return {
        sourceName: buildTaskResult[0].sourceName || 'unknown',
        total: parseInt(stats.total || '0'),
        completed: parseInt(stats.completed || '0'),
        failed: parseInt(stats.failed || '0'),
        pending: parseInt(stats.pending || '0'),
        running: parseInt(stats.running || '0'),
        startedAt: buildTaskResult[0].startedAt,
      };
    } catch (error) {
      return null;
    }
  }

  /**
   * Sleep utility function
   */
  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Get current running status
   */
  getStatus(): {
    running: boolean;
    runningBuildTaskCount: number;
    runningBuildTaskIds: number[];
    queueWorkerStatus: ReturnType<RecordingTaskQueueWorker['getStatus']>;
  } {
    return {
      running: this.running,
      runningBuildTaskCount: this.runningBuildTasks.size,
      runningBuildTaskIds: Array.from(this.runningBuildTasks.keys()),
      queueWorkerStatus: this.queueWorker.getStatus(),
    };
  }
}
