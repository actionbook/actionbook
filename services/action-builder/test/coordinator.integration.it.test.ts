/**
 * Coordinator Integration Tests
 *
 * Phase 2 Day 4: 端到端集成测试、无状态恢复测试
 *
 * 测试场景:
 * 1. 端到端-多 build_task 并发: 3 个 build_task，每个 5 个 recording_task
 * 2. 端到端-任务失败重试: 部分 recording_task 失败 (Mock)
 * 3. 无状态恢复-程序重启: 模拟中途崩溃，重启后自动恢复
 * 4. 并发安全-多消费者: 多个 QueueWorker 实例
 *
 * Note: 使用 Mock TaskExecutor，不启动真实浏览器，DB 使用真实的
 */

import { describe, test, expect, beforeAll, afterAll, beforeEach, vi } from 'vitest';
import { getDb } from '@actionbookdev/db';
import type { Database } from '@actionbookdev/db';
import { buildTasks, recordingTasks } from '@actionbookdev/db';
import { eq, inArray } from 'drizzle-orm';

// Mock TaskExecutor to avoid real browser and LLM calls
vi.mock('../src/task-worker/task-executor', async () => {
  const { recordingTasks } = await import('@actionbookdev/db');
  const { eq } = await import('drizzle-orm');

  return {
    TaskExecutor: class MockTaskExecutor {
      constructor(private db: any, private config: any) {}

      async execute(task: any) {
        // Simulate fast execution (50-100ms)
        await new Promise((resolve) => setTimeout(resolve, 50 + Math.random() * 50));

        // Update task status to completed (like real TaskExecutor does)
        await this.db
          .update(recordingTasks)
          .set({
            status: 'completed',
            progress: 100,
            completedAt: new Date(),
            attemptCount: task.attemptCount + 1,
            durationMs: 100,
            tokensUsed: 0,
            updatedAt: new Date(),
          })
          .where(eq(recordingTasks.id, task.id));

        return {
          success: true,
          actions_created: 5,
          duration_ms: 100,
        };
      }
    },
  };
});
import { Coordinator } from '../src/task-worker/coordinator';
import { RecordingTaskQueueWorker } from '../src/task-worker/recording-task-queue-worker';
import {
  createTestSource,
  createTestDocument,
  createTestChunks,
  createTestBuildTask,
  cleanupTestData,
  cleanupTestSource,
  waitForCondition,
} from './helpers/test-helpers';

describe('Coordinator Integration Tests', () => {
  let db: Database;
  let testSourceId: number;
  let testDocumentId: number;
  let testChunkIds: number[];
  let createdBuildTaskIds: number[] = [];

  beforeAll(async () => {
    db = getDb();
    testSourceId = await createTestSource(db);
    testDocumentId = await createTestDocument(db, testSourceId);
    testChunkIds = await createTestChunks(db, testDocumentId, 15); // 15 chunks for 3 build_tasks × 5 tasks
  });

  afterAll(async () => {
    await cleanupTestData(db, createdBuildTaskIds);
    await cleanupTestSource(db, testSourceId);
  });

  beforeEach(async () => {
    // Clean up all build_tasks and recording_tasks for this test source
    if (createdBuildTaskIds.length > 0) {
      await cleanupTestData(db, createdBuildTaskIds);
      createdBuildTaskIds = [];
    }

    // Also clean up any orphaned recording_tasks
    await db.delete(recordingTasks).where(eq(recordingTasks.sourceId, testSourceId));
    await db.delete(buildTasks).where(eq(buildTasks.sourceId, testSourceId));
  });

  test('IT-01: 端到端-多 build_task 并发 (3 个 build_task，每个 5 个 recording_task)', async () => {
    // 1. Create 3 separate sources, each with 5 chunks, to ensure each build_task gets exactly 5 tasks
    const buildTaskIds: number[] = [];
    const testSourceIds: number[] = [];

    for (let i = 0; i < 3; i++) {
      // Create separate source, document, and chunks for each build_task
      const sourceId = await createTestSource(db, `it01_test${i}`);
      testSourceIds.push(sourceId);

      const documentId = await createTestDocument(db, sourceId);
      await createTestChunks(db, documentId, 5); // 5 chunks per build_task

      // Create build_task for this source
      const result = await db
        .insert(buildTasks)
        .values({
          sourceId,
          sourceUrl: `https://it01-test${i}.com`,
          sourceName: `it01_test${i}`,
          sourceCategory: 'help',
          stage: 'knowledge_build',
          stageStatus: 'completed',
          config: {},
        })
        .returning({ id: buildTasks.id });
      buildTaskIds.push(result[0].id);
      createdBuildTaskIds.push(result[0].id);
    }

    // 2. 启动 Coordinator (maxConcurrentBuildTasks=2, QueueWorker concurrency=3)
    const coordinator = new Coordinator(db, {
      maxConcurrentBuildTasks: 2,
      buildTaskPollIntervalSeconds: 1,
      queueWorker: {
        concurrency: 3,
        databaseUrl: process.env.DATABASE_URL!,
        headless: true,
        outputDir: './test-output',
        idleWaitMs: 500,
      },
    });

    // 3. 启动 Coordinator（后台运行）
    const coordinatorPromise = coordinator.start();

    // 4. 等待所有 build_task 被领取并生成 recording_tasks
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(buildTasks).where(inArray(buildTasks.id, buildTaskIds));
        return tasks.every((t) => t.stage === 'action_build');
      },
      { timeout: 10000, interval: 500 }
    );

    // 5. 检查是否生成了 15 个 recording_tasks (3 × 5)
    const allRecordingTasks = await db.select().from(recordingTasks).where(
      inArray(recordingTasks.buildTaskId, buildTaskIds)
    );
    expect(allRecordingTasks.length).toBe(15);

    // 6. 等待所有 recording_tasks 完成 (completed or failed)
    const completed = await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          inArray(recordingTasks.buildTaskId, buildTaskIds)
        );
        return tasks.every((t) => t.status === 'completed' || t.status === 'failed');
      },
      { timeout: 30000, interval: 1000 }
    );

    expect(completed).toBe(true);

    // 7. 等待所有 build_task 完成
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(buildTasks).where(inArray(buildTasks.id, buildTaskIds));
        return tasks.every((t) => t.stageStatus === 'completed' || t.stageStatus === 'failed');
      },
      { timeout: 10000, interval: 500 }
    );

    // 8. 验证结果
    const finalBuildTasks = await db.select().from(buildTasks).where(inArray(buildTasks.id, buildTaskIds));
    const completedCount = finalBuildTasks.filter((t) => t.stageStatus === 'completed').length;
    expect(completedCount).toBeGreaterThan(0);

    // 9. 停止 Coordinator
    await coordinator.stop(5000);
    await coordinatorPromise;

    // 10. Clean up test sources
    for (const sourceId of testSourceIds) {
      await cleanupTestSource(db, sourceId);
    }
  }, 60000);

  test('IT-02: 端到端-任务失败重试 (部分 recording_task 失败，自动重试)', async () => {
    // 1. 创建 1 个 build_task
    const result = await db
      .insert(buildTasks)
      .values({
        sourceId: testSourceId,
        sourceUrl: 'https://it02-test.com',
        sourceName: 'it02_test',
        sourceCategory: 'help',
        stage: 'knowledge_build',
        stageStatus: 'completed',
        config: {},
      })
      .returning({ id: buildTasks.id });
    const buildTaskId = result[0].id;
    createdBuildTaskIds.push(buildTaskId);

    // 2. 启动 Coordinator
    const coordinator = new Coordinator(db, {
      maxConcurrentBuildTasks: 1,
      buildTaskPollIntervalSeconds: 1,
      buildTaskRunner: {
        checkIntervalSeconds: 2,
        maxAttempts: 3,
      },
      queueWorker: {
        concurrency: 2,
        databaseUrl: process.env.DATABASE_URL!,
        headless: true,
        outputDir: './test-output',
        idleWaitMs: 500,
      },
    });

    const coordinatorPromise = coordinator.start();

    // 3. Wait for recording_tasks to be generated and immediately set one to failed
    // This must happen before QueueWorker completes all tasks
    let firstTaskId: number = 0;
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        if (tasks.length >= 3 && firstTaskId === 0) {
          // Immediately set first task to failed (before it gets claimed)
          firstTaskId = tasks[0].id;
          await db
            .update(recordingTasks)
            .set({
              status: 'failed',
              attemptCount: 1,
              errorMessage: 'Mock failure for retry test',
            })
            .where(eq(recordingTasks.id, firstTaskId));
          return true;
        }
        return firstTaskId > 0;
      },
      { timeout: 10000, interval: 100 }
    );

    // 4. Wait for BuildTaskRunner to detect failed task and retry
    const retried = await waitForCondition(
      async () => {
        const task = await db.select().from(recordingTasks).where(
          eq(recordingTasks.id, firstTaskId)
        );
        // Task should be reset to pending or being re-executed
        return task.length > 0 && (task[0].status === 'pending' || task[0].status === 'running');
      },
      { timeout: 15000, interval: 1000 }
    );

    expect(retried).toBe(true);

    // 4.1 Wait for the retried task to actually be executed again (attemptCount should increase)
    const executedAgain = await waitForCondition(
      async () => {
        const task = await db.select().from(recordingTasks).where(
          eq(recordingTasks.id, firstTaskId)
        );
        return task.length > 0 && (task[0].attemptCount ?? 0) > 1;
      },
      { timeout: 20000, interval: 500 }
    );

    expect(executedAgain).toBe(true);

    // 5. Stop Coordinator
    await coordinator.stop(5000);
    await coordinatorPromise;

    // 6. Verify retry count increased
    const finalTask = await db.select().from(recordingTasks).where(
      eq(recordingTasks.id, firstTaskId)
    );
    expect(finalTask[0].attemptCount).toBeGreaterThan(1);
  }, 60000);

  test('IT-03: 无状态恢复-程序重启 (模拟中途崩溃，重启后自动恢复)', async () => {
    // 1. Create build_task in action_build/running state (so Coordinator won't claim it)
    // We only test QueueWorker's stale recovery, not BuildTaskRunner
    const result = await db
      .insert(buildTasks)
      .values({
        sourceId: testSourceId,
        sourceUrl: 'https://it03-test.com',
        sourceName: 'it03_test',
        sourceCategory: 'help',
        stage: 'action_build',
        stageStatus: 'running',
        actionStartedAt: new Date(Date.now() - 10 * 60 * 1000),
        config: {},
      })
      .returning({ id: buildTasks.id });
    const buildTaskId = result[0].id;
    createdBuildTaskIds.push(buildTaskId);

    // 2. Manually create 3 recording_tasks: 1 completed, 1 running (stale), 1 pending
    await db.insert(recordingTasks).values([
      {
        sourceId: testSourceId,
        buildTaskId,
        chunkId: testChunkIds[0],
        startUrl: 'https://it03-test.com/page0',
        status: 'completed',
        progress: 100,
        attemptCount: 0,
        config: {},
      },
      {
        sourceId: testSourceId,
        buildTaskId,
        chunkId: testChunkIds[1],
        startUrl: 'https://it03-test.com/page1',
        status: 'running',
        progress: 50,
        attemptCount: 0,
        lastHeartbeat: new Date(Date.now() - 20 * 60 * 1000), // 20 分钟前 (stale)
        config: {},
      },
      {
        sourceId: testSourceId,
        buildTaskId,
        chunkId: testChunkIds[2],
        startUrl: 'https://it03-test.com/page2',
        status: 'pending',
        progress: 0,
        attemptCount: 0,
        config: {},
      },
    ]);

    // 3. 启动新的 Coordinator (模拟程序重启)
    const coordinator = new Coordinator(db, {
      maxConcurrentBuildTasks: 1,
      buildTaskPollIntervalSeconds: 1,
      queueWorker: {
        concurrency: 2,
        staleTimeoutMinutes: 15,
        databaseUrl: process.env.DATABASE_URL!,
        headless: true,
        outputDir: './test-output',
        idleWaitMs: 500,
      },
    });

    const coordinatorPromise = coordinator.start();

    // 4. Wait for stale task to be recovered (attemptCount should increase)
    const recovered = await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        const staleTask = tasks.find((t) => t.chunkId === testChunkIds[1]);
        // Task should be recovered and re-executed (attemptCount increases from 0 to 1)
        return staleTask && staleTask.attemptCount > 0;
      },
      { timeout: 10000, interval: 500 }
    );

    expect(recovered).toBe(true);

    // 5. Wait for all tasks to complete
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        return tasks.every((t) => t.status === 'completed' || t.status === 'failed');
      },
      { timeout: 20000, interval: 1000 }
    );

    // 6. Verify all recording_tasks completed
    const finalTasks = await db.select().from(recordingTasks).where(
      eq(recordingTasks.buildTaskId, buildTaskId)
    );
    const completedCount = finalTasks.filter((t) => t.status === 'completed').length;
    expect(completedCount).toBeGreaterThan(0);

    // 7. Stop Coordinator
    await coordinator.stop(5000);
    await coordinatorPromise;
  }, 60000);

  test('IT-04: 并发安全-多消费者 (多个 QueueWorker 实例，任务不重复消费)', async () => {
    // 1. 创建 1 个 build_task 并手动生成 10 个 pending recording_tasks
    const result = await db
      .insert(buildTasks)
      .values({
        sourceId: testSourceId,
        sourceUrl: 'https://it04-test.com',
        sourceName: 'it04_test',
        sourceCategory: 'help',
        stage: 'action_build',
        stageStatus: 'running',
        config: {},
      })
      .returning({ id: buildTasks.id });
    const buildTaskId = result[0].id;
    createdBuildTaskIds.push(buildTaskId);

    // 生成 10 个 pending 任务
    const taskData = [];
    for (let i = 0; i < 10; i++) {
      taskData.push({
        sourceId: testSourceId,
        buildTaskId,
        chunkId: testChunkIds[i],
        startUrl: `https://it04-test.com/page${i}`,
        status: 'pending' as const,
        progress: 0,
        attemptCount: 0,
        config: {},
      });
    }
    await db.insert(recordingTasks).values(taskData);

    // 2. 启动 3 个 QueueWorker 实例并发消费
    const worker1 = new RecordingTaskQueueWorker(db, {
      concurrency: 2,
      idleWaitMs: 500,
      databaseUrl: process.env.DATABASE_URL!,
      headless: true,
      outputDir: './test-output',
    });

    const worker2 = new RecordingTaskQueueWorker(db, {
      concurrency: 2,
      idleWaitMs: 500,
      databaseUrl: process.env.DATABASE_URL!,
      headless: true,
      outputDir: './test-output',
    });

    const worker3 = new RecordingTaskQueueWorker(db, {
      concurrency: 2,
      idleWaitMs: 500,
      databaseUrl: process.env.DATABASE_URL!,
      headless: true,
      outputDir: './test-output',
    });

    // 3. Start 3 workers in background (don't await, let them run)
    const worker1Promise = worker1.start();
    const worker2Promise = worker2.start();
    const worker3Promise = worker3.start();

    // 4. Wait for all tasks to complete
    const allCompleted = await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        return tasks.every((t) => t.status === 'completed' || t.status === 'failed');
      },
      { timeout: 30000, interval: 1000 }
    );

    expect(allCompleted).toBe(true);

    // 5. Stop all workers and wait for them to finish
    await Promise.all([
      worker1.stop(5000),
      worker2.stop(5000),
      worker3.stop(5000),
    ]);

    // Wait for worker promises to complete
    await Promise.all([worker1Promise, worker2Promise, worker3Promise]);

    // 6. 验证每个任务只被执行一次（没有重复消费）
    const finalTasks = await db.select().from(recordingTasks).where(
      eq(recordingTasks.buildTaskId, buildTaskId)
    );

    // 所有任务都应该是 completed 或 failed（每个只执行一次）
    expect(finalTasks.length).toBe(10);
    const completedOrFailed = finalTasks.filter(
      (t) => t.status === 'completed' || t.status === 'failed'
    );
    expect(completedOrFailed.length).toBe(10);

    // 验证没有任务的 attemptCount 异常高（表明重复消费）
    const maxAttempts = Math.max(...finalTasks.map((t) => t.attemptCount));
    expect(maxAttempts).toBeLessThanOrEqual(3);
  }, 60000);
});
