/**
 * TaskScheduler Unit Tests
 *
 * Tests for stale task recovery functionality
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TaskScheduler } from '../../src/task-worker/task-scheduler';

// Mock database type
type MockDb = {
  select: ReturnType<typeof vi.fn>;
  update: ReturnType<typeof vi.fn>;
  execute: ReturnType<typeof vi.fn>;
};

// Helper to create mock database
function createMockDb(): MockDb {
  const mockDb = {
    select: vi.fn(),
    update: vi.fn(),
    execute: vi.fn(),
  };

  // Default chain returns
  mockDb.select.mockReturnValue({
    from: vi.fn().mockReturnValue({
      where: vi.fn().mockReturnValue({
        orderBy: vi.fn().mockReturnValue({
          limit: vi.fn().mockResolvedValue([]),
        }),
      }),
    }),
  });

  mockDb.update.mockReturnValue({
    set: vi.fn().mockReturnValue({
      where: vi.fn().mockResolvedValue(undefined),
    }),
  });

  // Default execute result (for raw SQL queries like FOR UPDATE SKIP LOCKED)
  mockDb.execute.mockResolvedValue({ rows: [] });

  return mockDb;
}

// Helper to create a mock recording task (camelCase - for Drizzle select)
function createMockTask(overrides: Partial<{
  id: number;
  sourceId: number;
  chunkId: number | null;
  startUrl: string;
  status: string;
  progress: number;
  config: object;
  attemptCount: number;
  errorMessage: string | null;
  completedAt: Date | null;
  lastHeartbeat: Date | null;
  createdAt: Date;
  updatedAt: Date;
}> = {}) {
  return {
    id: 1,
    sourceId: 1,
    chunkId: 100,
    startUrl: 'https://example.com',
    status: 'pending',
    progress: 0,
    config: { chunk_type: 'exploratory' },
    attemptCount: 0,
    errorMessage: null,
    completedAt: null,
    lastHeartbeat: null,
    createdAt: new Date(),
    updatedAt: new Date(),
    ...overrides,
  };
}

// Helper to create a raw mock task (snake_case - for raw SQL execute)
function createRawMockTask(overrides: Partial<{
  id: number;
  source_id: number;
  chunk_id: number | null;
  start_url: string;
  status: string;
  progress: number;
  config: object;
  attempt_count: number;
  error_message: string | null;
  completed_at: Date | null;
  last_heartbeat: Date | null;
  started_at: Date | null;
  created_at: Date;
  updated_at: Date;
}> = {}) {
  const now = new Date();
  return {
    id: 1,
    source_id: 1,
    chunk_id: 100,
    start_url: 'https://example.com',
    status: 'running',
    progress: 0,
    config: { chunk_type: 'exploratory' },
    attempt_count: 0,
    error_message: null,
    completed_at: null,
    last_heartbeat: now,
    started_at: now,
    created_at: now,
    updated_at: now,
    ...overrides,
  };
}

describe('TaskScheduler', () => {
  let mockDb: MockDb;
  let scheduler: TaskScheduler;

  beforeEach(() => {
    mockDb = createMockDb();
    scheduler = new TaskScheduler(mockDb as any);
  });

  describe('getNextTask', () => {
    it('should return null when no pending tasks', async () => {
      const result = await scheduler.getNextTask();
      expect(result).toBeNull();
    });

    it('should return pending task when available', async () => {
      const mockTask = createMockTask({ status: 'pending' });

      mockDb.select.mockReturnValue({
        from: vi.fn().mockReturnValue({
          where: vi.fn().mockReturnValue({
            orderBy: vi.fn().mockReturnValue({
              limit: vi.fn().mockResolvedValue([mockTask]),
            }),
          }),
        }),
      });

      const result = await scheduler.getNextTask();

      expect(result).not.toBeNull();
      expect(result?.id).toBe(1);
      expect(result?.status).toBe('pending');
    });

    it('should filter by sourceId when provided', async () => {
      const mockTask = createMockTask({ sourceId: 5 });
      const whereMock = vi.fn().mockReturnValue({
        orderBy: vi.fn().mockReturnValue({
          limit: vi.fn().mockResolvedValue([mockTask]),
        }),
      });

      mockDb.select.mockReturnValue({
        from: vi.fn().mockReturnValue({
          where: whereMock,
        }),
      });

      await scheduler.getNextTask(5);

      expect(whereMock).toHaveBeenCalled();
    });
  });

  describe('getNextTaskWithRecovery', () => {
    it('should return pending task first (priority)', async () => {
      const pendingTask = createMockTask({ id: 1, status: 'pending' });

      mockDb.select.mockReturnValue({
        from: vi.fn().mockReturnValue({
          where: vi.fn().mockReturnValue({
            orderBy: vi.fn().mockReturnValue({
              limit: vi.fn().mockResolvedValue([pendingTask]),
            }),
          }),
        }),
      });

      const result = await scheduler.getNextTaskWithRecovery();

      expect(result).not.toBeNull();
      expect(result?.id).toBe(1);
      expect(result?.status).toBe('pending');
    });

    it('should return stale running task when no pending tasks', async () => {
      // First call (pending) returns empty, second call (stale) returns task
      const staleTime = new Date(Date.now() - 60 * 60 * 1000); // 1 hour ago
      const staleTask = createMockTask({
        id: 2,
        status: 'running',
        attemptCount: 0,
        updatedAt: staleTime,
        lastHeartbeat: staleTime,
      });

      let callCount = 0;
      mockDb.select.mockReturnValue({
        from: vi.fn().mockReturnValue({
          where: vi.fn().mockReturnValue({
            orderBy: vi.fn().mockReturnValue({
              limit: vi.fn().mockImplementation(() => {
                callCount++;
                if (callCount === 1) {
                  return Promise.resolve([]); // No pending tasks
                }
                return Promise.resolve([staleTask]); // Stale task found
              }),
            }),
          }),
        }),
      });

      const result = await scheduler.getNextTaskWithRecovery({
        staleTimeoutMinutes: 30,
        maxAttempts: 3,
      });

      expect(result).not.toBeNull();
      expect(result?.id).toBe(2);
      expect(result?.attemptCount).toBe(1); // Incremented
    });

    it('should mark stale task as failed when max attempts exceeded', async () => {
      const staleTime = new Date(Date.now() - 60 * 60 * 1000);
      const staleTask = createMockTask({
        id: 3,
        status: 'running',
        attemptCount: 3, // Already at max
        updatedAt: staleTime,
        lastHeartbeat: staleTime,
      });

      let callCount = 0;
      const limitMock = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve([]); // No pending
        }
        if (callCount === 2) {
          return Promise.resolve([staleTask]); // Stale task at max attempts
        }
        return Promise.resolve([]); // No more stale tasks
      });

      mockDb.select.mockReturnValue({
        from: vi.fn().mockReturnValue({
          where: vi.fn().mockReturnValue({
            orderBy: vi.fn().mockReturnValue({
              limit: limitMock,
            }),
          }),
        }),
      });

      const result = await scheduler.getNextTaskWithRecovery({
        staleTimeoutMinutes: 30,
        maxAttempts: 3,
      });

      // Should return null as the task was marked as failed
      expect(result).toBeNull();
      // markFailed should have been called
      expect(mockDb.update).toHaveBeenCalled();
    });

    it('should return null when no pending or stale tasks', async () => {
      mockDb.select.mockReturnValue({
        from: vi.fn().mockReturnValue({
          where: vi.fn().mockReturnValue({
            orderBy: vi.fn().mockReturnValue({
              limit: vi.fn().mockResolvedValue([]),
            }),
          }),
        }),
      });

      const result = await scheduler.getNextTaskWithRecovery();
      expect(result).toBeNull();
    });
  });

  describe('markRunning', () => {
    it('should update task status to running with heartbeat', async () => {
      const setMock = vi.fn().mockReturnValue({
        where: vi.fn().mockResolvedValue(undefined),
      });

      mockDb.update.mockReturnValue({
        set: setMock,
      });

      await scheduler.markRunning(1);

      expect(mockDb.update).toHaveBeenCalled();
      expect(setMock).toHaveBeenCalledWith(
        expect.objectContaining({
          status: 'running',
        })
      );
    });
  });

  describe('updateHeartbeat', () => {
    it('should update lastHeartbeat timestamp', async () => {
      const setMock = vi.fn().mockReturnValue({
        where: vi.fn().mockResolvedValue(undefined),
      });

      mockDb.update.mockReturnValue({
        set: setMock,
      });

      await scheduler.updateHeartbeat(1);

      expect(mockDb.update).toHaveBeenCalled();
      expect(setMock).toHaveBeenCalledWith(
        expect.objectContaining({
          lastHeartbeat: expect.any(Date),
          updatedAt: expect.any(Date),
        })
      );
    });
  });

  describe('markCompleted', () => {
    it('should update task status to completed', async () => {
      const setMock = vi.fn().mockReturnValue({
        where: vi.fn().mockResolvedValue(undefined),
      });

      mockDb.update.mockReturnValue({
        set: setMock,
      });

      await scheduler.markCompleted(1);

      expect(setMock).toHaveBeenCalledWith(
        expect.objectContaining({
          status: 'completed',
          progress: 100,
        })
      );
    });
  });

  describe('markFailed', () => {
    it('should update task status to failed with error message', async () => {
      const setMock = vi.fn().mockReturnValue({
        where: vi.fn().mockResolvedValue(undefined),
      });

      mockDb.update.mockReturnValue({
        set: setMock,
      });

      await scheduler.markFailed(1, 'Test error');

      expect(setMock).toHaveBeenCalledWith(
        expect.objectContaining({
          status: 'failed',
          errorMessage: 'Test error',
        })
      );
    });
  });

  describe('claimNextTask', () => {
    // UT-TS-C01: Returns null when no tasks available
    it('should return null when no pending tasks available', async () => {
      // execute returns empty rows
      mockDb.execute.mockResolvedValue({ rows: [] });

      const result = await scheduler.claimNextTask();

      expect(result).toBeNull();
      expect(mockDb.execute).toHaveBeenCalled();
    });

    // UT-TS-C02: Claims pending task and marks as running atomically
    it('should claim pending task and mark as running atomically', async () => {
      const rawTask = createRawMockTask({
        id: 123,
        source_id: 456,
        status: 'running', // Already set to running by UPDATE
      });

      mockDb.execute.mockResolvedValue({ rows: [rawTask] });

      const result = await scheduler.claimNextTask();

      expect(result).not.toBeNull();
      expect(result?.id).toBe(123);
      expect(result?.sourceId).toBe(456);
      expect(result?.status).toBe('running');
      expect(mockDb.execute).toHaveBeenCalled();
    });

    // UT-TS-C03: Correctly maps snake_case to camelCase
    it('should map raw result fields correctly (snake_case to camelCase)', async () => {
      const now = new Date();
      const rawTask = createRawMockTask({
        id: 1,
        source_id: 2,
        chunk_id: 3,
        start_url: 'https://test.com',
        status: 'running',
        progress: 50,
        config: { chunk_type: 'task_driven' },
        attempt_count: 1,
        error_message: null,
        completed_at: null,
        last_heartbeat: now,
        started_at: now,
        created_at: now,
        updated_at: now,
      });

      mockDb.execute.mockResolvedValue({ rows: [rawTask] });

      const result = await scheduler.claimNextTask();

      expect(result).not.toBeNull();
      expect(result?.id).toBe(1);
      expect(result?.sourceId).toBe(2);
      expect(result?.chunkId).toBe(3);
      expect(result?.startUrl).toBe('https://test.com');
      expect(result?.status).toBe('running');
      expect(result?.progress).toBe(50);
      expect(result?.config).toEqual({ chunk_type: 'task_driven' });
      expect(result?.attemptCount).toBe(1);
      expect(result?.lastHeartbeat).toEqual(now);
    });

    // UT-TS-C04: Supports sourceId filtering
    it('should filter by sourceId when provided', async () => {
      const rawTask = createRawMockTask({
        id: 789,
        source_id: 999,
      });

      mockDb.execute.mockResolvedValue({ rows: [rawTask] });

      const result = await scheduler.claimNextTask({ sourceId: 999 });

      expect(result).not.toBeNull();
      expect(result?.sourceId).toBe(999);
      // Verify execute was called (SQL includes source_id filter)
      expect(mockDb.execute).toHaveBeenCalled();
    });

    // UT-TS-C05: Recovers stale running task when no pending tasks
    it('should recover stale running task when no pending tasks', async () => {
      const staleTask = createRawMockTask({
        id: 100,
        source_id: 200,
        status: 'running',
        attempt_count: 1, // Will be incremented to 2
      });

      // First call: no pending tasks
      // Second call: stale task found
      mockDb.execute
        .mockResolvedValueOnce({ rows: [] })
        .mockResolvedValueOnce({ rows: [staleTask] });

      const result = await scheduler.claimNextTask({
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
      });

      expect(result).not.toBeNull();
      expect(result?.id).toBe(100);
      expect(result?.attemptCount).toBe(1); // Returned value from DB
      expect(mockDb.execute).toHaveBeenCalledTimes(2);
    });

    // UT-TS-C06: Marks stale task as failed when maxAttempts exceeded
    it('should mark stale task as failed when max attempts exceeded', async () => {
      const staleTaskOverMax = createRawMockTask({
        id: 101,
        status: 'running',
        attempt_count: 4, // Exceeds maxAttempts of 3
      });

      // First call: no pending tasks
      // Second call: stale task with attempt_count > maxAttempts
      // Third call: no more stale tasks
      mockDb.execute
        .mockResolvedValueOnce({ rows: [] })
        .mockResolvedValueOnce({ rows: [staleTaskOverMax] })
        .mockResolvedValueOnce({ rows: [] });

      // Mock update for markFailed
      mockDb.update.mockReturnValue({
        set: vi.fn().mockReturnValue({
          where: vi.fn().mockResolvedValue(undefined),
        }),
      });

      const result = await scheduler.claimNextTask({
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
      });

      expect(result).toBeNull();
      // markFailed should have been called
      expect(mockDb.update).toHaveBeenCalled();
    });

    // UT-TS-C07: Returns null when no pending or stale tasks
    it('should return null when no pending or stale tasks', async () => {
      // Both calls return empty
      mockDb.execute
        .mockResolvedValueOnce({ rows: [] })
        .mockResolvedValueOnce({ rows: [] });

      const result = await scheduler.claimNextTask({
        staleTimeoutMinutes: 10,
        maxAttempts: 3,
      });

      expect(result).toBeNull();
      expect(mockDb.execute).toHaveBeenCalledTimes(2);
    });
  });
});
