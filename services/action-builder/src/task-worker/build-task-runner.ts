/**
 * BuildTaskRunner - 构建任务运行器
 *
 * 职责:
 * - 处理单个 build_task 的生命周期
 * - 生成 recording_tasks 并写入 DB
 * - 轮询检查 recording_tasks 状态（从 DB 读取）
 * - 处理重试逻辑
 * - 所有完成后更新 build_task 状态
 *
 * 特性:
 * - 非阻塞: 不等待 recording_task 执行，只生成和轮询
 * - 无状态: 每次从 DB 读取状态，不在内存保留
 * - 可并发: 多个 Runner 可同时运行，处理不同 build_task
 */

import type { Database } from '@actionbookdev/db';
import { eq, and, inArray, sql } from 'drizzle-orm';
import { buildTasks, recordingTasks, chunks, documents } from '@actionbookdev/db';
import type { BuildTaskInfo } from './types';

export interface BuildTaskRunnerConfig {
  /** 状态检查间隔（秒）*/
  checkIntervalSeconds?: number;
  /** 最大重试次数 */
  maxAttempts?: number;
  /** 心跳间隔（毫秒）*/
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
   * 运行 build_task 完整生命周期
   */
  async run(): Promise<void> {
    this.running = true;

    try {
      // 1. 获取 build_task 详情
      const buildTask = await this.getBuildTask();
      if (!buildTask) {
        throw new Error(`Build task ${this.buildTaskId} not found`);
      }

      // 2. 生成 recording_tasks
      console.log(
        `[BuildTaskRunner #${this.buildTaskId}] Generating recording tasks...`
      );
      const tasksCreated = await this.generateRecordingTasks(buildTask);
      console.log(
        `[BuildTaskRunner #${this.buildTaskId}] Generated ${tasksCreated} recording tasks`
      );

      // 3. 如果没有生成任何任务，直接标记为 completed
      if (tasksCreated === 0) {
        await this.completeBuildTask();
        console.log(
          `[BuildTaskRunner #${this.buildTaskId}] No tasks to process, marked as completed`
        );
        return;
      }

      // 4. 启动心跳
      this.startHeartbeat();

      // 5. 进入轮询循环
      await this.pollUntilComplete();

      // 6. 停止心跳
      this.stopHeartbeat();

      // 7. 更新 build_task 为 completed
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
   * 获取 build_task 信息
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
   * 生成 recording_tasks
   * 使用原子事务 + UPSERT (ON CONFLICT DO UPDATE)
   */
  private async generateRecordingTasks(buildTask: BuildTaskInfo): Promise<number> {
    // 1. 获取该 build_task 关联的所有 chunks（通过 documents 表关联 sourceId）
    const chunkResults = await this.db
      .select({
        id: chunks.id,
        documentId: chunks.documentId,
        url: documents.url,
      })
      .from(chunks)
      .innerJoin(documents, eq(chunks.documentId, documents.id))
      .where(eq(documents.sourceId, buildTask.sourceId!))
      .orderBy(chunks.id);

    if (chunkResults.length === 0) {
      return 0;
    }

    // 2. 准备 recording_tasks 数据
    const recordingTasksData = chunkResults.map((chunk) => ({
      sourceId: buildTask.sourceId!,
      buildTaskId: this.buildTaskId,
      chunkId: chunk.id,
      startUrl: chunk.url,
      status: 'pending' as const,
      progress: 0,
      attemptCount: 0,
      config: {
        chunk_type: 'task_driven',
      },
    }));

    // 3. 原子插入 (ON CONFLICT DO UPDATE)
    // 根据状态判断: completed/failed 跳过，pending/running 重置
    await this.db.transaction(async (tx) => {
      for (const data of recordingTasksData) {
        await tx
          .insert(recordingTasks)
          .values(data)
          .onConflictDoUpdate({
            target: [recordingTasks.chunkId, recordingTasks.buildTaskId],
            set: {
              // 只在 pending/running 状态时重置
              status: sql`CASE
                WHEN ${recordingTasks.status} IN ('pending', 'running') THEN 'pending'
                ELSE ${recordingTasks.status}
              END`,
              updatedAt: new Date(),
            },
          });
      }

      // 更新 build_task 状态为 running
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
   * 轮询检查所有 recording_tasks 状态
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
   * 获取 recording_tasks 状态统计
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
   * 重试失败的任务
   * 只重置 attemptCount < maxAttempts 的任务
   */
  private async retryFailedTasks(): Promise<number> {
    // 1. 查找可重试的失败任务
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

    // 2. 重置为 pending
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
   * 启动心跳
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
   * 停止心跳
   */
  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = undefined;
    }
  }

  /**
   * 更新心跳时间
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
   * 标记 build_task 为 completed
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
   * 标记 build_task 为 error
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
   * Sleep 工具函数
   */
  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * 停止运行
   */
  stop(): void {
    this.running = false;
    this.stopHeartbeat();
  }
}
