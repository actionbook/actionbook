/**
 * Coordinator - 协调器
 *
 * 职责:
 * - 启动并管理 RecordingTaskQueueWorker
 * - 持续获取新的 build_task 并启动 BuildTaskRunner
 * - 控制最大并发 build_task 数量
 * - 处理优雅关闭
 */

import type { Database } from '@actionbookdev/db';
import { buildTasks } from '@actionbookdev/db';
import { eq, and, sql } from 'drizzle-orm';
import { BuildTaskRunner, type BuildTaskRunnerConfig } from './build-task-runner';
import {
  RecordingTaskQueueWorker,
  type RecordingTaskQueueWorkerConfig,
} from './recording-task-queue-worker';

export interface CoordinatorConfig {
  /** 最大并发 build_task 数量 */
  maxConcurrentBuildTasks?: number;
  /** build_task 轮询间隔（秒）*/
  buildTaskPollIntervalSeconds?: number;
  /** build_task stale 判定阈值（分钟），用于重启恢复 action_build/running 的任务 */
  buildTaskStaleTimeoutMinutes?: number;
  /** BuildTaskRunner 配置 */
  buildTaskRunner?: BuildTaskRunnerConfig;
  /** RecordingTaskQueueWorker 配置 */
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
  private metricsIntervalMs = 30000; // 30 秒

  constructor(db: Database, config: CoordinatorConfig = {}) {
    this.db = db;
    this.config = {
      maxConcurrentBuildTasks: config.maxConcurrentBuildTasks ?? 5,
      buildTaskPollIntervalSeconds: config.buildTaskPollIntervalSeconds ?? 5,
      buildTaskStaleTimeoutMinutes: config.buildTaskStaleTimeoutMinutes ?? 15,
      buildTaskRunner: config.buildTaskRunner ?? {},
      queueWorker: config.queueWorker ?? {},
    };

    // 创建 QueueWorker
    this.queueWorker = new RecordingTaskQueueWorker(
      db,
      this.config.queueWorker
    );
  }

  /**
   * 启动协调器
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

    // 1. 启动 QueueWorker（后台运行）
    this.queueWorker.start().catch((error) => {
      console.error('[Coordinator] QueueWorker error:', error);
    });

    // 2. 启动监控指标输出
    this.startMetrics();

    // 3. 进入主循环
    await this.mainLoop();
  }

  /**
   * 停止协调器（优雅关闭）
   */
  async stop(timeoutMs?: number): Promise<void> {
    if (!this.running) {
      return;
    }

    console.log('[Coordinator] Stopping gracefully...');
    this.running = false;

    // 1. 停止监控指标
    this.stopMetrics();

    // 2. 停止 QueueWorker
    await this.queueWorker.stop(timeoutMs);

    // 3. 等待所有 BuildTaskRunner 完成
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
   * 主循环
   */
  private async mainLoop(): Promise<void> {
    while (this.running) {
      try {
        // 1. 清理已完成的 build_task
        this.cleanupCompletedTasks();

        // 2. 如果有空闲槽位，领取新的 build_task
        while (
          this.running &&
          this.runningBuildTasks.size < this.config.maxConcurrentBuildTasks
        ) {
          const buildTask = await this.claimBuildTask();

          if (!buildTask) {
            // 无可领取的 build_task
            break;
          }

          // 启动 BuildTaskRunner（非阻塞）
          await this.startBuildTaskRunner(buildTask.id);
        }

        // 3. 等待后继续
        await this.sleep(this.config.buildTaskPollIntervalSeconds * 1000);
      } catch (error) {
        console.error('[Coordinator] Main loop error:', error);
        await this.sleep(1000);
      }
    }
  }

  /**
   * 领取一个 build_task
   * 查找 stage=knowledge_build, stage_status=completed 的任务
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
      console.error('[Coordinator] Failed to claim build_task:', error);
      return null;
    }
  }

  /**
   * 启动 BuildTaskRunner
   */
  private async startBuildTaskRunner(buildTaskId: number): Promise<void> {
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
      .catch((error) => {
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
   * 清理已完成的 build_task
   */
  private cleanupCompletedTasks(): void {
    // Promise 的 finally 已经处理了删除，这里不需要额外操作
  }

  /**
   * 启动监控指标输出
   */
  private startMetrics(): void {
    this.metricsTimer = setInterval(() => {
      this.outputMetrics();
    }, this.metricsIntervalMs);

    // 立即输出一次
    this.outputMetrics();
  }

  /**
   * 停止监控指标输出
   */
  private stopMetrics(): void {
    if (this.metricsTimer) {
      clearInterval(this.metricsTimer);
      this.metricsTimer = undefined;
    }
  }

  /**
   * 输出监控指标
   */
  private outputMetrics(): void {
    const queueStatus = this.queueWorker.getStatus();
    const now = Date.now();
    const elapsedSeconds = (now - this.lastMetricsTime) / 1000;

    console.log(
      `[Metrics] ` +
        `build_tasks=${this.runningBuildTasks.size}/${this.config.maxConcurrentBuildTasks}, ` +
        `recording_tasks=${queueStatus.runningTaskCount}/${this.config.queueWorker.concurrency ?? 3}, ` +
        `elapsed=${elapsedSeconds.toFixed(1)}s`
    );

    this.lastMetricsTime = now;
  }

  /**
   * Sleep 工具函数
   */
  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * 获取当前运行状态
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
