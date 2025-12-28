/**
 * BuildTaskWorker Unit Tests
 *
 * Tests for the build task worker that orchestrates the action_build stage.
 * Uses WorkerPool for concurrent recording task execution.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BuildTaskWorker } from '../../src/task-worker/build-task-worker';
import {
  createMockDatabase,
  createMockBuildTaskScheduler,
  createMockTaskGenerator,
  createMockWorkerPool,
  createSampleBuildTaskInfo,
} from '../helpers/mock-factory';

// Mock all dependencies
vi.mock('../../src/task-worker/build-task-scheduler', () => ({
  BuildTaskScheduler: vi.fn(),
}));

vi.mock('../../src/task-worker/task-generator', () => ({
  TaskGenerator: vi.fn(),
}));

vi.mock('../../src/task-worker/worker-pool', () => ({
  WorkerPool: vi.fn(),
}));

describe('BuildTaskWorker', () => {
  let worker: BuildTaskWorker;
  let mockDb: ReturnType<typeof createMockDatabase>;
  let mockBuildTaskScheduler: ReturnType<typeof createMockBuildTaskScheduler>;
  let mockTaskGenerator: ReturnType<typeof createMockTaskGenerator>;
  let mockWorkerPool: ReturnType<typeof createMockWorkerPool>;

  const mockConfig = {
    llmApiKey: 'test-api-key',
    llmBaseURL: 'https://api.test.com/v1',
    llmModel: 'test-model',
    databaseUrl: 'postgres://test:test@localhost:5432/test',
    headless: true,
    maxTurns: 30,
    outputDir: './output',
    concurrency: 3,
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockDb = createMockDatabase();
    mockBuildTaskScheduler = createMockBuildTaskScheduler();
    mockTaskGenerator = createMockTaskGenerator();
    mockWorkerPool = createMockWorkerPool();

    // Create worker with injected mocks
    worker = new BuildTaskWorker(mockDb as any, mockConfig);

    // Replace internal dependencies with mocks
    (worker as any).buildTaskScheduler = mockBuildTaskScheduler;
    (worker as any).taskGenerator = mockTaskGenerator;
    (worker as any).workerPool = mockWorkerPool;
  });

  describe('runOnce', () => {
    // UT-BTW-01: Returns null when no tasks available
    it('should return null when no tasks available', async () => {
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(null);

      const result = await worker.runOnce();

      expect(result).toBeNull();
      expect(mockBuildTaskScheduler.claimNextActionTask).toHaveBeenCalled();
      expect(mockTaskGenerator.generate).not.toHaveBeenCalled();
    });

    // UT-BTW-02: Fails task when sourceId is null (defensive check)
    it('should fail task when sourceId is null', async () => {
      const taskWithNullSourceId = createSampleBuildTaskInfo({ sourceId: null });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(taskWithNullSourceId);

      const result = await worker.runOnce();

      expect(result).not.toBeNull();
      expect(result?.success).toBe(false);
      expect(result?.error).toContain('sourceId');
      expect(mockBuildTaskScheduler.failTask).toHaveBeenCalledWith(
        taskWithNullSourceId.id,
        expect.stringContaining('sourceId')
      );
    });

    // UT-BTW-03: Completes full workflow successfully (atomic claim + generate + execute + complete)
    it('should complete full workflow: claim -> generate -> execute -> complete', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockResolvedValue(5); // 5 recording tasks created

      // Mock WorkerPool.executeAll to return stats for 3 completed tasks
      mockWorkerPool.executeAll.mockResolvedValue({
        completed: 3,
        failed: 0,
        elements: 15, // 3 tasks * 5 elements each
      });
      mockBuildTaskScheduler.completeTask.mockResolvedValue(undefined);

      const result = await worker.runOnce();

      // Verify workflow order (atomic claim already transitions state)
      expect(mockBuildTaskScheduler.claimNextActionTask).toHaveBeenCalled();
      expect(mockTaskGenerator.generate).toHaveBeenCalledWith(sampleTask.sourceId, expect.any(Number));
      expect(mockWorkerPool.executeAll).toHaveBeenCalledWith(
        sampleTask.sourceId,
        expect.objectContaining({
          staleTimeoutMinutes: expect.any(Number),
          maxAttempts: expect.any(Number),
          heartbeatIntervalMs: expect.any(Number),
        })
      );
      expect(mockBuildTaskScheduler.completeTask).toHaveBeenCalledWith(
        sampleTask.id,
        expect.objectContaining({
          recordingTasksCreated: 5,
          recordingTasksCompleted: 3,
          recordingTasksFailed: 0,
        })
      );

      // Verify result
      expect(result).not.toBeNull();
      expect(result?.success).toBe(true);
      expect(result?.taskId).toBe(sampleTask.id);
      expect(result?.recordingTasksCreated).toBe(5);
      expect(result?.recordingTasksCompleted).toBe(3);
    });

    // UT-BTW-04: Calls failTask when execution throws error
    it('should call failTask when taskGenerator throws error', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockRejectedValue(new Error('Generator failed'));

      const result = await worker.runOnce();

      expect(result).not.toBeNull();
      expect(result?.success).toBe(false);
      expect(result?.error).toBe('Generator failed');
      expect(mockBuildTaskScheduler.failTask).toHaveBeenCalledWith(
        sampleTask.id,
        'Generator failed'
      );
      expect(mockBuildTaskScheduler.completeTask).not.toHaveBeenCalled();
    });

    // UT-BTW-05: Tracks failed recording tasks via WorkerPool stats
    it('should track failed recording tasks in stats', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockResolvedValue(3);

      // Mock WorkerPool.executeAll: 2 succeed, 1 fails
      mockWorkerPool.executeAll.mockResolvedValue({
        completed: 2,
        failed: 1,
        elements: 8, // 5 + 3 from successful tasks
      });
      mockBuildTaskScheduler.completeTask.mockResolvedValue(undefined);

      const result = await worker.runOnce();

      expect(result?.success).toBe(true); // Build task still succeeds
      expect(result?.recordingTasksCompleted).toBe(2);
      expect(result?.recordingTasksFailed).toBe(1);
      expect(result?.elementsCreated).toBe(8);
    });

    // UT-BTW-06: Returns correct duration_ms
    it('should return correct duration_ms in result', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockResolvedValue(0); // No tasks created
      mockWorkerPool.executeAll.mockResolvedValue({ completed: 0, failed: 0, elements: 0 });
      mockBuildTaskScheduler.completeTask.mockResolvedValue(undefined);

      const startTime = Date.now();
      const result = await worker.runOnce();
      const endTime = Date.now();

      expect(result).not.toBeNull();
      expect(result?.duration_ms).toBeGreaterThanOrEqual(0);
      expect(result?.duration_ms).toBeLessThanOrEqual(endTime - startTime + 100); // Allow some margin
    });

    // UT-BTW-07: Handles WorkerPool.executeAll throwing exception
    it('should handle workerPool.executeAll throwing exception', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockResolvedValue(2);

      // WorkerPool throws exception
      mockWorkerPool.executeAll.mockRejectedValue(new Error('WorkerPool crashed'));

      const result = await worker.runOnce();

      // Should fail the build task
      expect(result?.success).toBe(false);
      expect(result?.error).toBe('WorkerPool crashed');
      expect(mockBuildTaskScheduler.failTask).toHaveBeenCalledWith(
        sampleTask.id,
        'WorkerPool crashed'
      );
    });

    // UT-BTW-08: Uses default recordingTaskLimit of 100
    it('should use default recordingTaskLimit of 100', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockResolvedValue(0);
      mockWorkerPool.executeAll.mockResolvedValue({ completed: 0, failed: 0, elements: 0 });
      mockBuildTaskScheduler.completeTask.mockResolvedValue(undefined);

      await worker.runOnce();

      expect(mockTaskGenerator.generate).toHaveBeenCalledWith(100, 100);
    });

    // UT-BTW-09: Calls workerPool.shutdown after execution
    it('should call workerPool.shutdown after successful execution', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockResolvedValue(2);
      mockWorkerPool.executeAll.mockResolvedValue({ completed: 2, failed: 0, elements: 10 });
      mockWorkerPool.shutdown.mockResolvedValue(undefined);
      mockBuildTaskScheduler.completeTask.mockResolvedValue(undefined);

      await worker.runOnce();

      expect(mockWorkerPool.shutdown).toHaveBeenCalled();
    });

    // UT-BTW-10: Calls workerPool.shutdown even on error
    it('should call workerPool.shutdown even when execution fails', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 1, sourceId: 100 });
      mockBuildTaskScheduler.claimNextActionTask.mockResolvedValue(sampleTask);
      mockTaskGenerator.generate.mockResolvedValue(2);
      mockWorkerPool.executeAll.mockRejectedValue(new Error('Execution failed'));
      mockWorkerPool.shutdown.mockResolvedValue(undefined);

      await worker.runOnce();

      expect(mockWorkerPool.shutdown).toHaveBeenCalled();
    });
  });
});
