/**
 * Coordinator Performance Benchmark Tests
 *
 * Phase 2 Day 5: Performance benchmark tests
 *
 * Test scenarios:
 * 1. Throughput test: 50 simulated tasks (30s each), concurrency 5-10, target <2 hours
 * 2. Concurrency test: Average concurrent execution count ≈ concurrency config value
 *
 * Note: Uses Mock TaskExecutor with configurable delay, real DB
 */

import { describe, test, expect, beforeAll, afterAll, beforeEach, vi } from 'vitest';
import { getDb } from '@actionbookdev/db';
import type { Database } from '@actionbookdev/db';
import { buildTasks, recordingTasks } from '@actionbookdev/db';
import { eq, inArray } from 'drizzle-orm';

// Mock TaskExecutor with configurable delay for performance testing
vi.mock('../src/task-worker/task-executor', async () => {
  const { recordingTasks } = await import('@actionbookdev/db');
  const { eq } = await import('drizzle-orm');

  return {
    TaskExecutor: class MockTaskExecutor {
      constructor(private db: any, private config: any) {}

      async execute(task: any) {
        // Simulate task execution with configurable delay (default 30s for benchmark)
        const delay = process.env.BENCHMARK_TASK_DELAY_MS
          ? parseInt(process.env.BENCHMARK_TASK_DELAY_MS)
          : 30000;

        await new Promise((resolve) => setTimeout(resolve, delay));

        // Update task status to completed (like real TaskExecutor does)
        await this.db
          .update(recordingTasks)
          .set({
            status: 'completed',
            progress: 100,
            completedAt: new Date(),
            attemptCount: task.attemptCount + 1,
            durationMs: delay,
            tokensUsed: 0,
            updatedAt: new Date(),
          })
          .where(eq(recordingTasks.id, task.id));

        return {
          success: true,
          actions_created: 5,
          duration_ms: delay,
        };
      }
    },
  };
});

import { Coordinator } from '../src/task-worker/coordinator';
import {
  createTestSource,
  createTestDocument,
  createTestChunks,
  cleanupTestData,
  cleanupTestSource,
  waitForCondition,
} from './helpers/test-helpers';

describe('Coordinator Performance Benchmark Tests', () => {
  let db: Database;
  let testSourceId: number;
  let testDocumentId: number;
  let testChunkIds: number[];
  let createdBuildTaskIds: number[] = [];

  beforeAll(async () => {
    db = getDb();
    testSourceId = await createTestSource(db, 'benchmark_test');
    testDocumentId = await createTestDocument(db, testSourceId);
    testChunkIds = await createTestChunks(db, testDocumentId, 50); // 50 chunks for benchmark
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

    await db.delete(recordingTasks).where(eq(recordingTasks.sourceId, testSourceId));
    await db.delete(buildTasks).where(eq(buildTasks.sourceId, testSourceId));
  });

  test('BM-01: Throughput test - 50 tasks (30s each), concurrency 10, target <2 hours', async () => {
    // Override task delay for faster testing (use 100ms instead of 30s)
    process.env.BENCHMARK_TASK_DELAY_MS = '100';

    // 1. Create 1 build_task with 50 recording_tasks
    const result = await db
      .insert(buildTasks)
      .values({
        sourceId: testSourceId,
        sourceUrl: 'https://bm01-test.com',
        sourceName: 'bm01_test',
        sourceCategory: 'help',
        stage: 'knowledge_build',
        stageStatus: 'completed',
        config: {},
      })
      .returning({ id: buildTasks.id });
    const buildTaskId = result[0].id;
    createdBuildTaskIds.push(buildTaskId);

    // 2. Start Coordinator with concurrency 10
    const coordinator = new Coordinator(db, {
      maxConcurrentBuildTasks: 1,
      buildTaskPollIntervalSeconds: 1,
      queueWorker: {
        concurrency: 10,
        databaseUrl: process.env.DATABASE_URL!,
        headless: true,
        outputDir: './test-output',
        idleWaitMs: 500,
      },
    });

    const startTime = Date.now();
    const coordinatorPromise = coordinator.start();

    // 3. Wait for all recording_tasks to be generated
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        return tasks.length === 50;
      },
      { timeout: 10000, interval: 500 }
    );

    console.log(`[BM-01] All 50 recording_tasks generated`);

    // 4. Wait for all tasks to complete
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        return tasks.every((t) => t.status === 'completed' || t.status === 'failed');
      },
      { timeout: 120000, interval: 1000 } // 2 minutes max
    );

    const endTime = Date.now();
    const durationSeconds = (endTime - startTime) / 1000;

    console.log(`[BM-01] All 50 tasks completed in ${durationSeconds.toFixed(1)}s`);

    // 5. Calculate expected time based on delay and concurrency
    const taskDelay = parseInt(process.env.BENCHMARK_TASK_DELAY_MS!);
    const concurrency = 10;
    const expectedMinTime = (50 * taskDelay) / concurrency / 1000; // seconds

    // Actual time should be close to expected (allow 4x overhead for startup/shutdown/polling)
    expect(durationSeconds).toBeLessThan(expectedMinTime * 4);
    console.log(`[BM-01] Expected min time: ${expectedMinTime.toFixed(1)}s, actual: ${durationSeconds.toFixed(1)}s`);

    // 6. Stop Coordinator
    await coordinator.stop(5000);
    await coordinatorPromise;

    // 7. Verify all tasks completed successfully
    const finalTasks = await db.select().from(recordingTasks).where(
      eq(recordingTasks.buildTaskId, buildTaskId)
    );
    const completedCount = finalTasks.filter((t) => t.status === 'completed').length;
    expect(completedCount).toBe(50);

    // Reset env var
    delete process.env.BENCHMARK_TASK_DELAY_MS;
  }, 180000); // 3 minutes timeout

  test('BM-02: Concurrency test - Average concurrent execution ≈ concurrency config', async () => {
    // Use 3000ms delay for better concurrency sampling (longer tasks = easier to sample)
    process.env.BENCHMARK_TASK_DELAY_MS = '3000';

    // 1. Create 1 build_task with 30 recording_tasks
    const result = await db
      .insert(buildTasks)
      .values({
        sourceId: testSourceId,
        sourceUrl: 'https://bm02-test.com',
        sourceName: 'bm02_test',
        sourceCategory: 'help',
        stage: 'knowledge_build',
        stageStatus: 'completed',
        config: {},
      })
      .returning({ id: buildTasks.id });
    const buildTaskId = result[0].id;
    createdBuildTaskIds.push(buildTaskId);

    // 2. Start Coordinator with concurrency 5
    const targetConcurrency = 5;
    const coordinator = new Coordinator(db, {
      maxConcurrentBuildTasks: 1,
      buildTaskPollIntervalSeconds: 1,
      queueWorker: {
        concurrency: targetConcurrency,
        databaseUrl: process.env.DATABASE_URL!,
        headless: true,
        outputDir: './test-output',
        idleWaitMs: 500,
      },
    });

    const coordinatorPromise = coordinator.start();

    // 3. Wait for recording_tasks to be generated
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        return tasks.length === 30;
      },
      { timeout: 10000, interval: 500 }
    );

    // 4. Sample concurrent execution count every 100ms (more frequent sampling)
    const concurrencySamples: number[] = [];
    const samplingInterval = setInterval(async () => {
      const tasks = await db.select().from(recordingTasks).where(
        eq(recordingTasks.buildTaskId, buildTaskId)
      );
      const runningCount = tasks.filter((t) => t.status === 'running').length;
      concurrencySamples.push(runningCount);
    }, 100);

    // 5. Wait for all tasks to complete
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          eq(recordingTasks.buildTaskId, buildTaskId)
        );
        return tasks.every((t) => t.status === 'completed' || t.status === 'failed');
      },
      { timeout: 120000, interval: 1000 } // 2 minutes timeout for 30 tasks × 3s
    );

    clearInterval(samplingInterval);

    // 6. Calculate average concurrent execution
    const validSamples = concurrencySamples.filter((s) => s > 0);
    const avgConcurrency = validSamples.length > 0
      ? validSamples.reduce((a, b) => a + b, 0) / validSamples.length
      : 0;

    console.log(`[BM-02] Average concurrency: ${avgConcurrency.toFixed(2)} (target: ${targetConcurrency})`);
    console.log(`[BM-02] Concurrency samples: ${concurrencySamples.join(', ')}`);

    // 7. Average should be close to target (allow 30% variance due to startup/shutdown)
    expect(avgConcurrency).toBeGreaterThan(targetConcurrency * 0.7);
    expect(avgConcurrency).toBeLessThanOrEqual(targetConcurrency);

    // 8. Stop Coordinator
    await coordinator.stop(5000);
    await coordinatorPromise;

    // Reset env var
    delete process.env.BENCHMARK_TASK_DELAY_MS;
  }, 180000); // 3 minutes timeout

  test('BM-03: Scalability test - 5 build_tasks with 10 tasks each, concurrency 10', async () => {
    // Use 200ms delay for faster testing
    process.env.BENCHMARK_TASK_DELAY_MS = '200';

    // 1. Create 5 build_tasks, each with separate source
    const buildTaskIds: number[] = [];
    const timestamp = Date.now();
    for (let i = 0; i < 5; i++) {
      const sourceId = await createTestSource(db, `bm03_test${timestamp}_${i}`);
      const documentId = await createTestDocument(db, sourceId);
      await createTestChunks(db, documentId, 10); // 10 tasks per build_task

      const result = await db
        .insert(buildTasks)
        .values({
          sourceId,
          sourceUrl: `https://bm03-test${i}.com`,
          sourceName: `bm03_test${i}`,
          sourceCategory: 'help',
          stage: 'knowledge_build',
          stageStatus: 'completed',
          config: {},
        })
        .returning({ id: buildTasks.id });
      buildTaskIds.push(result[0].id);
      createdBuildTaskIds.push(result[0].id);
    }

    // 2. Start Coordinator with maxConcurrentBuildTasks=3, queueWorker concurrency=10
    const coordinator = new Coordinator(db, {
      maxConcurrentBuildTasks: 3,
      buildTaskPollIntervalSeconds: 1,
      queueWorker: {
        concurrency: 10,
        databaseUrl: process.env.DATABASE_URL!,
        headless: true,
        outputDir: './test-output',
        idleWaitMs: 500,
      },
    });

    const startTime = Date.now();
    const coordinatorPromise = coordinator.start();

    // 3. Wait for all recording_tasks to complete (5 × 10 = 50 tasks)
    await waitForCondition(
      async () => {
        const tasks = await db.select().from(recordingTasks).where(
          inArray(recordingTasks.buildTaskId, buildTaskIds)
        );
        return tasks.length === 50 && tasks.every((t) => t.status === 'completed' || t.status === 'failed');
      },
      { timeout: 120000, interval: 1000 }
    );

    // 4. Wait for all build_tasks to be completed (BuildTaskRunner polling needs time to detect completion)
    await waitForCondition(
      async () => {
        const buildTasksStatus = await db.select().from(buildTasks).where(
          inArray(buildTasks.id, buildTaskIds)
        );
        return buildTasksStatus.every((t) => t.stageStatus === 'completed' || t.stageStatus === 'error');
      },
      { timeout: 30000, interval: 1000 } // Wait up to 30s for BuildTaskRunner to mark completed
    );

    const endTime = Date.now();
    const durationSeconds = (endTime - startTime) / 1000;

    console.log(`[BM-03] All 50 tasks (5 build_tasks × 10) completed in ${durationSeconds.toFixed(1)}s`);

    // 5. Verify all build_tasks completed
    const finalBuildTasks = await db.select().from(buildTasks).where(
      inArray(buildTasks.id, buildTaskIds)
    );
    const completedBuildTaskCount = finalBuildTasks.filter(
      (t) => t.stageStatus === 'completed'
    ).length;
    expect(completedBuildTaskCount).toBe(5);

    // 6. Stop Coordinator
    await coordinator.stop(5000);
    await coordinatorPromise;

    // 7. Clean up test sources
    for (const buildTaskId of buildTaskIds) {
      const buildTask = await db.select().from(buildTasks).where(eq(buildTasks.id, buildTaskId));
      if (buildTask.length > 0) {
        await cleanupTestSource(db, buildTask[0].sourceId);
      }
    }

    // Reset env var
    delete process.env.BENCHMARK_TASK_DELAY_MS;
  }, 180000); // 3 minutes timeout
});
