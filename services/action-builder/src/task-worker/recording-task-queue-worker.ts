/**
 * RecordingTaskQueueWorker - 记录任务队列消费者
 *
 * 职责:
 * - 持续消费 DB 中的 pending 任务（全局队列）
 * - 执行 recording_task
 * - 更新任务状态到 DB
 * - 维护任务心跳
 * - 恢复 stale 任务
 *
 * 特性:
 * - 全局队列: 不区分 build_task，统一消费所有 pending 任务
 * - 并发控制: 配置 concurrency 参数控制最大并发数
 * - 心跳机制: 定期更新 lastHeartbeat，防止被误认为 stale
 * - Stale 恢复: 启动时和运行中定期检查并恢复 stale 任务
 */

import type { Database } from '@actionbookdev/db';
import { recordingTasks } from '@actionbookdev/db';
import { eq, and, sql } from 'drizzle-orm';
import { TaskExecutor } from './task-executor';
import type { TaskExecutorConfig, RecordingTask } from './types';

export interface RecordingTaskQueueWorkerConfig extends TaskExecutorConfig {
  /** 最大并发执行数 */
  concurrency?: number;
  /** 无任务时等待间隔（毫秒）*/
  idleWaitMs?: number;
  /** 任务心跳间隔（毫秒）*/
  heartbeatIntervalMs?: number;
  /** 最大重试次数（用于 stale 恢复） */
  maxAttempts?: number;
  /** Stale 任务判定阈值（分钟）*/
  staleTimeoutMinutes?: number;
}

interface RunningTask {
  id: number;
  executor: TaskExecutor;
  heartbeatTimer: NodeJS.Timeout;
  promise: Promise<void>;
}

export class RecordingTaskQueueWorker {
  private db: Database;
  private config: Required<RecordingTaskQueueWorkerConfig>;
  private running = false;
  private runningTasks = new Map<number, RunningTask>();
  private gracefulShutdownTimeout?: number;

  constructor(db: Database, config: RecordingTaskQueueWorkerConfig = {}) {
    this.db = db;
    this.config = {
      databaseUrl: config.databaseUrl ?? process.env.DATABASE_URL!,
      concurrency: config.concurrency ?? 3,
      idleWaitMs: config.idleWaitMs ?? 1000,
      heartbeatIntervalMs: config.heartbeatIntervalMs ?? 5000,
      taskTimeoutMinutes: config.taskTimeoutMinutes ?? 10,
      maxAttempts: config.maxAttempts ?? 3,
      staleTimeoutMinutes: config.staleTimeoutMinutes ?? 15,
      headless: config.headless ?? true,
      maxTurns: config.maxTurns ?? 30,
      outputDir: config.outputDir ?? './output',
      profileEnabled: config.profileEnabled ?? false,
      profileDir: config.profileDir ?? '.browser-profile',
      llmApiKey: config.llmApiKey,
      llmBaseURL: config.llmBaseURL,
      llmModel: config.llmModel,
    };
  }

  /**
   * 启动队列消费
   */
  async start(): Promise<void> {
    if (this.running) {
      console.log('[QueueWorker] Already running');
      return;
    }

    this.running = true;
    console.log(
      `[QueueWorker] Starting with concurrency=${this.config.concurrency}`
    );

    // 启动时恢复 stale 任务
    await this.recoverStaleTasks();

    // 进入主循环
    await this.mainLoop();
  }

  /**
   * 停止队列消费（优雅关闭）
   */
  async stop(timeoutMs?: number): Promise<void> {
    if (!this.running) {
      return;
    }

    this.gracefulShutdownTimeout = timeoutMs;
    console.log(
      `[QueueWorker] Stopping gracefully (timeout=${timeoutMs ?? 'none'}ms)...`
    );
    this.running = false;

    // 等待所有执行中的任务完成
    const startTime = Date.now();
    while (this.runningTasks.size > 0) {
      if (
        this.gracefulShutdownTimeout &&
        Date.now() - startTime > this.gracefulShutdownTimeout
      ) {
        console.log(
          `[QueueWorker] Graceful shutdown timeout, forcing stop. ` +
            `${this.runningTasks.size} tasks still running`
        );

        // 记录未完成任务
        const incompleteTasks = Array.from(this.runningTasks.keys());
        console.log(
          `[QueueWorker] Incomplete tasks: ${incompleteTasks.join(', ')}`
        );

        // 停止所有心跳
        for (const task of this.runningTasks.values()) {
          clearInterval(task.heartbeatTimer);
        }
        break;
      }

      await this.sleep(100);
    }

    console.log('[QueueWorker] Stopped');
  }

  /**
   * 主循环
   */
  private async mainLoop(): Promise<void> {
    while (this.running) {
      try {
        // 1. 恢复 stale 任务
        await this.recoverStaleTasks();

        // 2. 填充执行槽位
        while (
          this.running &&
          this.runningTasks.size < this.config.concurrency
        ) {
          // 2.1 原子性领取一个 pending 任务
          const task = await this.claimTask();

          if (!task) {
            // 无任务可领取，跳出填充循环
            break;
          }

          // 2.2 启动执行（非阻塞）
          await this.startExecution(task);
        }

        // 3. 如果无执行中任务，等待后继续
        if (this.runningTasks.size === 0) {
          await this.sleep(this.config.idleWaitMs);
          continue;
        }

        // 4. 等待任意一个任务完成
        await Promise.race(
          Array.from(this.runningTasks.values()).map((t) => t.promise)
        );
      } catch (error) {
        console.error('[QueueWorker] Main loop error:', error);
        await this.sleep(1000);
      }
    }
  }

  /**
   * 原子性领取一个 pending 任务
   * 使用 FOR UPDATE SKIP LOCKED 确保并发安全
   */
  private async claimTask(): Promise<RecordingTask | null> {
    try {
      const result = await this.db.execute(sql`
        UPDATE ${recordingTasks}
        SET
          status = 'running',
          started_at = NOW(),
          last_heartbeat = NOW(),
          updated_at = NOW()
        WHERE id = (
          SELECT id
          FROM ${recordingTasks}
          WHERE status = 'pending'
          ORDER BY updated_at DESC, id  -- 重试任务优先（刚被更新）
          LIMIT 1
          FOR UPDATE SKIP LOCKED  -- 跳过已锁定行
        )
        RETURNING *
      `);

      if (result.rows.length === 0) {
        return null;
      }

      const row = result.rows[0] as any;
      return {
        id: row.id,
        sourceId: row.source_id,
        chunkId: row.chunk_id,
        startUrl: row.start_url,
        status: row.status,
        progress: row.progress,
        config: row.config,
        attemptCount: row.attempt_count,
        errorMessage: row.error_message,
        completedAt: row.completed_at,
        lastHeartbeat: row.last_heartbeat,
        createdAt: row.created_at,
        updatedAt: row.updated_at,
      };
    } catch (error) {
      console.error('[QueueWorker] Failed to claim task:', error);
      return null;
    }
  }

  /**
   * 启动任务执行
   */
  private async startExecution(task: RecordingTask): Promise<void> {
    console.log(`[QueueWorker] Starting task #${task.id}`);

    // 创建 TaskExecutor
    const executor = new TaskExecutor(this.db, this.config);

    // 启动心跳
    const heartbeatTimer = setInterval(() => {
      this.updateTaskHeartbeat(task.id).catch((error) => {
        console.error(`[QueueWorker] Task #${task.id} heartbeat error:`, error);
      });
    }, this.config.heartbeatIntervalMs);

    // 执行任务（非阻塞）
    const promise = executor
      .execute(task)
      .then(() => {
        console.log(`[QueueWorker] Task #${task.id} completed`);
      })
      .catch((error) => {
        console.error(`[QueueWorker] Task #${task.id} failed:`, error);
      })
      .finally(() => {
        // 清理
        clearInterval(heartbeatTimer);
        this.runningTasks.delete(task.id);
      });

    // 保存到运行任务列表
    this.runningTasks.set(task.id, {
      id: task.id,
      executor,
      heartbeatTimer,
      promise,
    });
  }

  /**
   * 更新任务心跳
   */
  private async updateTaskHeartbeat(taskId: number): Promise<void> {
    await this.db
      .update(recordingTasks)
      .set({
        lastHeartbeat: new Date(),
        updatedAt: new Date(),
      })
      .where(eq(recordingTasks.id, taskId));
  }

  /**
   * 恢复 stale 任务
   * 查找 running 但 lastHeartbeat 超时的任务
   */
  private async recoverStaleTasks(): Promise<void> {
    try {
      const staleThresholdMs = this.config.staleTimeoutMinutes * 60 * 1000;
      const staleThreshold = new Date(Date.now() - staleThresholdMs);

      const maxAttempts = this.config.maxAttempts;
      const result = await this.db.execute(sql`
        WITH stale_tasks AS (
          SELECT
            id,
            attempt_count,
            CASE
              WHEN attempt_count < ${maxAttempts} THEN 'pending'
              ELSE 'failed'
            END AS new_status
          FROM ${recordingTasks}
          WHERE
            status = 'running'
            AND last_heartbeat < ${staleThreshold}
        )
        UPDATE ${recordingTasks}
        SET
          status = stale_tasks.new_status,
          attempt_count = CASE
            WHEN stale_tasks.new_status = 'pending' THEN "recording_tasks".attempt_count + 1
            ELSE "recording_tasks".attempt_count
          END,
          error_message = CASE
            WHEN stale_tasks.new_status = 'failed' THEN 'Task stale: max attempts reached'
            ELSE NULL
          END,
          updated_at = NOW()
        FROM stale_tasks
        WHERE ${recordingTasks.id} = stale_tasks.id
        RETURNING ${recordingTasks.id}, ${recordingTasks.status}
      `);

      if (result.rows.length > 0) {
        console.log(
          `[QueueWorker] Recovered ${result.rows.length} stale tasks`
        );
        for (const row of result.rows as any[]) {
          console.log(`  - Task #${row.id}: ${row.status}`);
        }
      }
    } catch (error) {
      console.error('[QueueWorker] Failed to recover stale tasks:', error);
    }
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
    runningTaskCount: number;
    runningTaskIds: number[];
  } {
    return {
      running: this.running,
      runningTaskCount: this.runningTasks.size,
      runningTaskIds: Array.from(this.runningTasks.keys()),
    };
  }
}
