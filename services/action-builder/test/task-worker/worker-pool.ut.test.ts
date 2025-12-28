/**
 * WorkerPool Unit Tests
 *
 * Tests for concurrent execution of recording tasks using a worker pool.
 * WorkerPool manages multiple TaskExecutors to process tasks in parallel.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { RecordingTask, ExecutionResult } from '../../src/task-worker/types/index';

// Helper to create a sample recording task
function createSampleTask(overrides: Partial<RecordingTask> = {}): RecordingTask {
  const now = new Date();
  return {
    id: 1,
    sourceId: 100,
    chunkId: 200,
    startUrl: 'https://example.com',
    status: 'running',
    progress: 0,
    config: { chunk_type: 'exploratory' },
    attemptCount: 0,
    errorMessage: null,
    completedAt: null,
    lastHeartbeat: now,
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

// Helper to create a successful execution result
function createSuccessResult(actionsCreated: number = 5): ExecutionResult {
  return {
    success: true,
    actions_created: actionsCreated,
    duration_ms: 1000,
    error: undefined,
  };
}

// Helper to create a failed execution result
function createFailResult(error: string): ExecutionResult {
  return {
    success: false,
    actions_created: 0,
    duration_ms: 500,
    error,
  };
}

// Mock TaskScheduler
const mockTaskScheduler = {
  claimNextTask: vi.fn(),
  updateHeartbeat: vi.fn(),
  markFailed: vi.fn(),
};

// Mock TaskExecutor
const mockExecute = vi.fn();
const mockTaskExecutor = {
  execute: mockExecute,
};

// Mock the modules
vi.mock('../../src/task-worker/task-scheduler', () => ({
  TaskScheduler: vi.fn().mockImplementation(() => mockTaskScheduler),
}));

vi.mock('../../src/task-worker/task-executor', () => ({
  TaskExecutor: vi.fn().mockImplementation(() => mockTaskExecutor),
}));

describe('WorkerPool', () => {
  let WorkerPool: typeof import('../../src/task-worker/worker-pool').WorkerPool;
  const mockDb = {} as any;

  beforeEach(async () => {
    vi.clearAllMocks();

    // Reset mock implementations
    mockTaskScheduler.claimNextTask.mockReset();
    mockTaskScheduler.updateHeartbeat.mockResolvedValue(undefined);
    mockTaskScheduler.markFailed.mockResolvedValue(undefined);
    mockExecute.mockReset();

    // Dynamically import after mocks are set up
    const module = await import('../../src/task-worker/worker-pool');
    WorkerPool = module.WorkerPool;
  });

  describe('constructor', () => {
    // UT-WP-01: 按配置创建指定数量 workers
    it('should create pool with specified concurrency config', () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 5,
      });

      expect(pool).toBeDefined();
    });

    it('should use default concurrency of 3 when not specified', () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
      });

      expect(pool).toBeDefined();
    });
  });

  describe('executeAll', () => {
    // UT-WP-02: Executes multiple tasks concurrently
    it('should execute multiple tasks', async () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 3,
      });

      const tasks = [
        createSampleTask({ id: 1 }),
        createSampleTask({ id: 2 }),
        createSampleTask({ id: 3 }),
      ];

      let claimCallCount = 0;
      mockTaskScheduler.claimNextTask.mockImplementation(async () => {
        if (claimCallCount < tasks.length) {
          return tasks[claimCallCount++];
        }
        return null;
      });

      mockExecute.mockResolvedValue(createSuccessResult(5));

      const result = await pool.executeAll(100, {
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
        heartbeatIntervalMs: 60000,
      });

      expect(result.completed).toBe(3);
      expect(result.failed).toBe(0);
      expect(result.elements).toBe(15); // 3 tasks * 5 elements each
    });

    // UT-WP-04: Returns correct stats after all tasks complete
    it('should return correct stats after all tasks complete', async () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 3,
      });

      const tasks = [
        createSampleTask({ id: 1 }),
        createSampleTask({ id: 2 }),
      ];

      let claimCallCount = 0;
      mockTaskScheduler.claimNextTask.mockImplementation(async () => {
        if (claimCallCount < tasks.length) {
          return tasks[claimCallCount++];
        }
        return null;
      });

      mockExecute
        .mockResolvedValueOnce(createSuccessResult(10))
        .mockResolvedValueOnce(createSuccessResult(5));

      const result = await pool.executeAll(100, {
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
        heartbeatIntervalMs: 60000,
      });

      expect(result.completed).toBe(2);
      expect(result.failed).toBe(0);
      expect(result.elements).toBe(15); // 10 + 5
    });

    // UT-WP-05: Single task failure does not affect others
    it('should isolate failures between workers', async () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 3,
      });

      const tasks = [
        createSampleTask({ id: 1 }),
        createSampleTask({ id: 2 }),
        createSampleTask({ id: 3 }),
      ];

      let claimCallCount = 0;
      mockTaskScheduler.claimNextTask.mockImplementation(async () => {
        if (claimCallCount < tasks.length) {
          return tasks[claimCallCount++];
        }
        return null;
      });

      // Second task fails
      mockExecute
        .mockResolvedValueOnce(createSuccessResult(5))
        .mockResolvedValueOnce(createFailResult('Task 2 failed'))
        .mockResolvedValueOnce(createSuccessResult(5));

      const result = await pool.executeAll(100, {
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
        heartbeatIntervalMs: 60000,
      });

      expect(result.completed).toBe(2); // Tasks 1 and 3 succeeded
      expect(result.failed).toBe(1);     // Task 2 failed
      expect(result.elements).toBe(10);  // Only from successful tasks
    });

    it('should return empty stats when no tasks available', async () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 3,
      });

      mockTaskScheduler.claimNextTask.mockResolvedValue(null);

      const result = await pool.executeAll(100, {
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
        heartbeatIntervalMs: 60000,
      });

      expect(result.completed).toBe(0);
      expect(result.failed).toBe(0);
      expect(result.elements).toBe(0);
    });

    it('should handle executor throwing exception', async () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 3,
      });

      const tasks = [
        createSampleTask({ id: 1 }),
        createSampleTask({ id: 2 }),
      ];

      let claimCallCount = 0;
      mockTaskScheduler.claimNextTask.mockImplementation(async () => {
        if (claimCallCount < tasks.length) {
          return tasks[claimCallCount++];
        }
        return null;
      });

      // First task throws, second succeeds
      mockExecute
        .mockRejectedValueOnce(new Error('Executor crashed'))
        .mockResolvedValueOnce(createSuccessResult(5));

      const result = await pool.executeAll(100, {
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
        heartbeatIntervalMs: 60000,
      });

      // First task crashes (counted as failed), second succeeds
      expect(result.completed).toBe(1);
      expect(result.failed).toBe(1);
    });

    // UT-WP-06: Waits for running tasks when claimNextTask throws
    it('should wait for running tasks when claimNextTask throws', async () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 3,
      });

      let taskExecuted = false;
      let claimCallCount = 0;

      // First call returns a task, second call throws
      mockTaskScheduler.claimNextTask.mockImplementation(async () => {
        claimCallCount++;
        if (claimCallCount === 1) {
          return createSampleTask({ id: 1 });
        }
        // Simulate database connection error on second call
        throw new Error('Database connection lost');
      });

      // Task execution takes some time
      mockExecute.mockImplementation(async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
        taskExecuted = true;
        return createSuccessResult(5);
      });

      // executeAll should throw, but only after waiting for running task
      await expect(
        pool.executeAll(100, {
          staleTimeoutMinutes: 10,
          maxAttempts: 3,
          heartbeatIntervalMs: 60000,
        })
      ).rejects.toThrow('Database connection lost');

      // The running task should have completed before the error was thrown
      expect(taskExecuted).toBe(true);
    });
  });

  describe('shutdown', () => {
    it('should stop all heartbeats and cleanup', async () => {
      const pool = new WorkerPool(mockDb, {
        databaseUrl: 'postgres://test',
        concurrency: 2,
      });

      // No errors should be thrown
      await expect(pool.shutdown()).resolves.not.toThrow();
    });
  });
});
