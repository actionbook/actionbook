/**
 * BuildTaskScheduler Unit Tests
 *
 * Tests for the build task scheduler that manages build_tasks table operations
 * for the action_build stage.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BuildTaskScheduler } from '../../src/task-worker/build-task-scheduler';
import { createSampleBuildTaskInfo } from '../helpers/mock-factory';

/**
 * Create a properly chained mock database for BuildTaskScheduler
 */
function createChainedMockDb() {
  // For select queries
  const selectMock = vi.fn();
  const fromMock = vi.fn();
  const whereMock = vi.fn();
  const orderByMock = vi.fn();
  const limitMock = vi.fn();

  // For update queries
  const updateMock = vi.fn();
  const setMock = vi.fn();
  const updateWhereMock = vi.fn();

  // For raw SQL execute (claimNextActionTask)
  const executeMock = vi.fn();

  // Chain setup for select
  selectMock.mockReturnValue({ from: fromMock });
  fromMock.mockReturnValue({ where: whereMock });
  whereMock.mockReturnValue({ orderBy: orderByMock, limit: limitMock });
  orderByMock.mockReturnValue({ limit: limitMock });
  limitMock.mockResolvedValue([]);

  // Chain setup for update
  updateMock.mockReturnValue({ set: setMock });
  setMock.mockReturnValue({ where: updateWhereMock });
  updateWhereMock.mockResolvedValue([]);

  // Default execute result
  executeMock.mockResolvedValue({ rows: [] });

  return {
    select: selectMock,
    from: fromMock,
    where: whereMock,
    orderBy: orderByMock,
    limit: limitMock,
    update: updateMock,
    set: setMock,
    updateWhere: updateWhereMock,
    execute: executeMock,
  };
}

describe('BuildTaskScheduler', () => {
  let scheduler: BuildTaskScheduler;
  let mockDb: ReturnType<typeof createChainedMockDb>;

  beforeEach(() => {
    mockDb = createChainedMockDb();
    scheduler = new BuildTaskScheduler(mockDb as any);
  });

  describe('claimNextActionTask', () => {
    // UT-BTS-00: Returns null when no tasks available (rows empty)
    it('should return null when no tasks available', async () => {
      // Both stale and pending queries return empty
      mockDb.execute.mockResolvedValue({ rows: [] });

      const result = await scheduler.claimNextActionTask();

      expect(result).toBeNull();
      expect(mockDb.execute).toHaveBeenCalled();
    });

    // UT-BTS-00B: Claims new pending task and maps fields correctly (snake_case -> camelCase)
    it('should claim new pending task and map fields correctly', async () => {
      const now = new Date();
      // First call: no stale tasks, second call: return pending task
      mockDb.execute
        .mockResolvedValueOnce({ rows: [] }) // claimStaleRunningTask - no stale tasks
        .mockResolvedValueOnce({
          rows: [
            {
              id: 123,
              source_id: 456,
              source_url: 'https://example.com',
              source_name: 'Example',
              source_category: 'help',
              stage: 'action_build',
              stage_status: 'running',
              config: { attemptCount: 0 },
              created_at: now,
              updated_at: now,
              knowledge_started_at: null,
              knowledge_completed_at: null,
              action_started_at: now,
              action_completed_at: null,
            },
          ],
        }); // claimNewPendingTask

      const result = await scheduler.claimNextActionTask();

      expect(result).not.toBeNull();
      expect(result?.id).toBe(123);
      expect(result?.sourceId).toBe(456);
      expect(result?.sourceUrl).toBe('https://example.com');
      expect(result?.sourceName).toBe('Example');
      expect(result?.sourceCategory).toBe('help');
      expect(result?.stage).toBe('action_build');
      expect(result?.stageStatus).toBe('running');
      expect(mockDb.execute).toHaveBeenCalledTimes(2);
    });

    // UT-BTS-00C: Recovers stale running task first (priority)
    it('should recover stale running task first (priority over new pending)', async () => {
      const staleTime = new Date(Date.now() - 60 * 60 * 1000); // 1 hour ago
      const staleTask = {
        id: 999,
        source_id: 111,
        source_url: 'https://stale.com',
        source_name: 'Stale Site',
        source_category: 'help',
        stage: 'action_build',
        stage_status: 'running',
        config: { attemptCount: 0 },
        created_at: staleTime,
        updated_at: staleTime,
        knowledge_started_at: null,
        knowledge_completed_at: null,
        action_started_at: staleTime,
        action_completed_at: null,
      };

      mockDb.execute
        .mockResolvedValueOnce({ rows: [staleTask] }) // claimStaleRunningTask - found stale task
        .mockResolvedValueOnce({ rows: [] }); // update stale task

      const result = await scheduler.claimNextActionTask();

      expect(result).not.toBeNull();
      expect(result?.id).toBe(999);
      expect(result?.config.attemptCount).toBe(1); // Incremented
      expect(result?.config.lastError).toContain('Recovered from stale');
    });

    // UT-BTS-00D: Marks stale task as error when max attempts exceeded
    it('should mark stale task as error when max attempts exceeded', async () => {
      const staleTime = new Date(Date.now() - 60 * 60 * 1000);
      const staleTaskAtMax = {
        id: 888,
        source_id: 222,
        source_url: 'https://max-attempts.com',
        source_name: 'Max Attempts Site',
        source_category: 'help',
        stage: 'action_build',
        stage_status: 'running',
        config: { attemptCount: 3 }, // Already at max (default maxAttempts=3)
        created_at: staleTime,
        updated_at: staleTime,
        knowledge_started_at: null,
        knowledge_completed_at: null,
        action_started_at: staleTime,
        action_completed_at: null,
      };

      mockDb.execute
        .mockResolvedValueOnce({ rows: [staleTaskAtMax] }) // claimStaleRunningTask - found stale task at max
        .mockResolvedValueOnce({ rows: [] }) // UPDATE to mark as error
        .mockResolvedValueOnce({ rows: [] }) // recursive: no more stale tasks
        .mockResolvedValueOnce({ rows: [] }); // no pending tasks

      const result = await scheduler.claimNextActionTask();

      // Should return null since task was marked as error and no other tasks available
      expect(result).toBeNull();
      // execute should be called 4 times
      expect(mockDb.execute).toHaveBeenCalledTimes(4);
    });
  });

  describe('getNextActionTask', () => {
    // UT-BTS-01: Returns task with correct stage and status
    it('should return task with stage=knowledge_build and stageStatus=completed', async () => {
      const sampleTask = createSampleBuildTaskInfo();
      mockDb.limit.mockResolvedValue([sampleTask]);

      const result = await scheduler.getNextActionTask();

      expect(result).not.toBeNull();
      expect(result?.id).toBe(sampleTask.id);
      expect(result?.sourceId).toBe(sampleTask.sourceId);
      expect(result?.sourceUrl).toBe(sampleTask.sourceUrl);
    });

    // UT-BTS-02: Returns null when no tasks available
    it('should return null when no tasks available', async () => {
      mockDb.limit.mockResolvedValue([]);

      const result = await scheduler.getNextActionTask();

      expect(result).toBeNull();
    });

    // UT-BTS-03: Query chain is called correctly
    it('should call query chain in correct order', async () => {
      mockDb.limit.mockResolvedValue([]);

      await scheduler.getNextActionTask();

      // Verify select chain was called
      expect(mockDb.select).toHaveBeenCalled();
      expect(mockDb.from).toHaveBeenCalled();
      expect(mockDb.where).toHaveBeenCalled();
      expect(mockDb.orderBy).toHaveBeenCalled();
      expect(mockDb.limit).toHaveBeenCalledWith(1);
    });
  });

  describe('startActionStage', () => {
    // UT-BTS-04: Updates stage and stageStatus correctly
    it('should update stage to action_build and stageStatus to running', async () => {
      await scheduler.startActionStage(1);

      expect(mockDb.update).toHaveBeenCalled();
      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          stage: 'action_build',
          stageStatus: 'running',
        })
      );
    });

    // UT-BTS-05: Sets actionStartedAt timestamp
    it('should set actionStartedAt to current time', async () => {
      const beforeCall = new Date();
      await scheduler.startActionStage(1);
      const afterCall = new Date();

      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          actionStartedAt: expect.any(Date),
          updatedAt: expect.any(Date),
        })
      );

      // Verify timestamp is reasonable
      const callArg = mockDb.set.mock.calls[0][0];
      expect(callArg.actionStartedAt.getTime()).toBeGreaterThanOrEqual(beforeCall.getTime());
      expect(callArg.actionStartedAt.getTime()).toBeLessThanOrEqual(afterCall.getTime());
    });
  });

  describe('completeTask', () => {
    beforeEach(() => {
      // Mock getTaskById to return a task
      const sampleTask = createSampleBuildTaskInfo();
      mockDb.limit.mockResolvedValue([sampleTask]);
    });

    // UT-BTS-06: Updates stage and stageStatus to completed
    it('should update stage to completed and stageStatus to completed', async () => {
      await scheduler.completeTask(1);

      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          stage: 'completed',
          stageStatus: 'completed',
        })
      );
    });

    // UT-BTS-07: Sets actionCompletedAt timestamp
    it('should set actionCompletedAt to current time', async () => {
      await scheduler.completeTask(1);

      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          actionCompletedAt: expect.any(Date),
          updatedAt: expect.any(Date),
        })
      );
    });

    // UT-BTS-08: Stores stats in config when provided
    it('should store stats in config when provided', async () => {
      const stats = {
        recordingTasksCreated: 10,
        recordingTasksCompleted: 8,
        recordingTasksFailed: 2,
        elementsCreated: 50,
        duration_ms: 120000,
      };

      await scheduler.completeTask(1, stats);

      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          config: expect.objectContaining({
            stats,
          }),
        })
      );
    });
  });

  describe('failTask', () => {
    // UT-BTS-09: Increments attemptCount on first failure
    it('should increment attemptCount on first failure', async () => {
      // Mock getTaskById to return task with attemptCount = 0
      const sampleTask = createSampleBuildTaskInfo({ config: { attemptCount: 0 } });
      mockDb.limit.mockResolvedValue([sampleTask]);

      scheduler = new BuildTaskScheduler(mockDb as any, { maxAttempts: 3 });

      await scheduler.failTask(1, 'Test error');

      // Should update with attemptCount = 1
      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          stage: 'knowledge_build',
          stageStatus: 'completed',
          config: expect.objectContaining({
            attemptCount: 1,
            lastError: 'Test error',
          }),
        })
      );

      // Should NOT set stage to error (still has retries left)
      const setArg = mockDb.set.mock.calls[0][0];
      expect(setArg.stage).not.toBe('error');
    });

    // UT-BTS-10: Marks as error when max attempts reached
    it('should set stage to error when max attempts reached', async () => {
      // Mock getTaskById to return task with attemptCount = 2 (at max - 1)
      const sampleTask = createSampleBuildTaskInfo({ config: { attemptCount: 2 } });
      mockDb.limit.mockResolvedValue([sampleTask]);

      scheduler = new BuildTaskScheduler(mockDb as any, { maxAttempts: 3 });

      await scheduler.failTask(1, 'Test error');

      // Should set stage and stageStatus to error
      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          stage: 'error',
          stageStatus: 'error',
          config: expect.objectContaining({
            attemptCount: 3,
            lastError: 'Test error',
          }),
        })
      );
    });

    // UT-BTS-11: Uses default maxAttempts of 3
    it('should use default maxAttempts of 3', async () => {
      const sampleTask = createSampleBuildTaskInfo({ config: { attemptCount: 2 } });
      mockDb.limit.mockResolvedValue([sampleTask]);

      // No config passed - should use default maxAttempts = 3
      scheduler = new BuildTaskScheduler(mockDb as any);

      await scheduler.failTask(1, 'Test error');

      // attemptCount goes from 2 to 3, which equals maxAttempts, so should mark as error
      expect(mockDb.set).toHaveBeenCalledWith(
        expect.objectContaining({
          stage: 'error',
          stageStatus: 'error',
        })
      );
    });
  });

  describe('getTaskById', () => {
    // UT-BTS-12: Returns task by ID
    it('should return task by ID', async () => {
      const sampleTask = createSampleBuildTaskInfo({ id: 42 });
      mockDb.limit.mockResolvedValue([sampleTask]);

      const result = await scheduler.getTaskById(42);

      expect(result).not.toBeNull();
      expect(result?.id).toBe(42);
    });

    // UT-BTS-13: Returns null for non-existent task
    it('should return null for non-existent task', async () => {
      mockDb.limit.mockResolvedValue([]);

      const result = await scheduler.getTaskById(999);

      expect(result).toBeNull();
    });
  });

  describe('publishVersion', () => {
    // Helper to create a mock with transaction support
    function createMockDbWithTransaction() {
      const baseMock = createChainedMockDb();

      // Track query call order for verification
      const queryResults: Map<string, unknown[]> = new Map();
      let queryCallIndex = 0;

      // Override limit to return different results based on call order
      baseMock.limit.mockImplementation(() => {
        const results = queryResults.get(`query_${queryCallIndex}`) || [];
        queryCallIndex++;
        return Promise.resolve(results);
      });

      // Transaction mock
      const txMock = {
        update: vi.fn().mockReturnThis(),
        set: vi.fn().mockReturnThis(),
        where: vi.fn().mockResolvedValue([]),
      };
      txMock.update.mockReturnValue({ set: txMock.set });
      txMock.set.mockReturnValue({ where: txMock.where });

      const transactionMock = vi.fn().mockImplementation(async (fn) => {
        return fn(txMock);
      });

      return {
        ...baseMock,
        transaction: transactionMock,
        txMock,
        setQueryResults: (index: number, results: unknown[]) => {
          queryResults.set(`query_${index}`, results);
        },
        resetQueryIndex: () => {
          queryCallIndex = 0;
        },
      };
    }

    // UT-BTS-14: Returns error when no building version found
    it('should return error when no building version found', async () => {
      const mockDbTx = createMockDbWithTransaction();
      // First query: find building version - returns empty
      mockDbTx.setQueryResults(0, []);

      const schedulerWithTx = new BuildTaskScheduler(mockDbTx as any);
      const result = await schedulerWithTx.publishVersion(123);

      expect(result.success).toBe(false);
      expect(result.error).toContain("No 'building' version found");
      expect(result.versionId).toBeUndefined();
    });

    // UT-BTS-15: Returns error when source not found
    it('should return error when source not found', async () => {
      const mockDbTx = createMockDbWithTransaction();
      // First query: find building version - returns version
      mockDbTx.setQueryResults(0, [{ id: 10, sourceId: 123, versionNumber: 1, status: 'building' }]);
      // Second query: get source - returns empty
      mockDbTx.setQueryResults(1, []);

      const schedulerWithTx = new BuildTaskScheduler(mockDbTx as any);
      const result = await schedulerWithTx.publishVersion(123);

      expect(result.success).toBe(false);
      expect(result.error).toContain('Source 123 not found');
    });

    // UT-BTS-16: Successfully publishes version (first publish, no previous active)
    it('should successfully publish version when no previous active version', async () => {
      const mockDbTx = createMockDbWithTransaction();
      // First query: find building version
      mockDbTx.setQueryResults(0, [{ id: 10, sourceId: 123, versionNumber: 1, status: 'building' }]);
      // Second query: get source - no current version
      mockDbTx.setQueryResults(1, [{ currentVersionId: null }]);

      const schedulerWithTx = new BuildTaskScheduler(mockDbTx as any);
      const result = await schedulerWithTx.publishVersion(123);

      expect(result.success).toBe(true);
      expect(result.versionId).toBe(10);
      expect(result.archivedVersionId).toBeNull();
      // Transaction should be called
      expect(mockDbTx.transaction).toHaveBeenCalled();
      // tx.update should be called twice: set new version to active + update source
      expect(mockDbTx.txMock.update).toHaveBeenCalledTimes(2);
    });

    // UT-BTS-17: Successfully publishes version and archives old active
    it('should archive previous active version when publishing new version', async () => {
      const mockDbTx = createMockDbWithTransaction();
      // First query: find building version
      mockDbTx.setQueryResults(0, [{ id: 20, sourceId: 123, versionNumber: 2, status: 'building' }]);
      // Second query: get source - has current version
      mockDbTx.setQueryResults(1, [{ currentVersionId: 10 }]);

      const schedulerWithTx = new BuildTaskScheduler(mockDbTx as any);
      const result = await schedulerWithTx.publishVersion(123);

      expect(result.success).toBe(true);
      expect(result.versionId).toBe(20);
      expect(result.archivedVersionId).toBe(10);
      // Transaction should be called
      expect(mockDbTx.transaction).toHaveBeenCalled();
      // tx.update should be called 3 times: archive old + set new to active + update source
      expect(mockDbTx.txMock.update).toHaveBeenCalledTimes(3);
    });

    // UT-BTS-18: Transaction performs atomic operations in correct order
    it('should perform atomic operations: archive old, activate new, update source', async () => {
      const mockDbTx = createMockDbWithTransaction();
      mockDbTx.setQueryResults(0, [{ id: 20, sourceId: 123, versionNumber: 2, status: 'building' }]);
      mockDbTx.setQueryResults(1, [{ currentVersionId: 10 }]);

      const schedulerWithTx = new BuildTaskScheduler(mockDbTx as any);
      await schedulerWithTx.publishVersion(123);

      // Verify set was called with correct status values
      const setCalls = mockDbTx.txMock.set.mock.calls;

      // First call should archive old version (status: 'archived')
      expect(setCalls[0][0]).toEqual(expect.objectContaining({ status: 'archived' }));

      // Second call should activate new version (status: 'active')
      expect(setCalls[1][0]).toEqual(
        expect.objectContaining({
          status: 'active',
          publishedAt: expect.any(Date),
        })
      );

      // Third call should update source's currentVersionId
      expect(setCalls[2][0]).toEqual(
        expect.objectContaining({
          currentVersionId: 20,
          updatedAt: expect.any(Date),
        })
      );
    });
  });
});
