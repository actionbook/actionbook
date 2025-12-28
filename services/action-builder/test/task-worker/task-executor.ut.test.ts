/**
 * TaskExecutor Unit Tests - Full Version with ActionBuilder Integration
 *
 * Tests the complete TaskExecutor that uses ActionBuilder for recording.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { TaskExecutor } from '../../src/task-worker/task-executor';
import type { RecordingTask, TaskExecutorConfig } from '../../src/task-worker/types';

// Mock ActionBuilder module
vi.mock('../../src/ActionBuilder', () => ({
  ActionBuilder: vi.fn().mockImplementation(() => ({
    initialize: vi.fn().mockResolvedValue(undefined),
    build: vi.fn().mockResolvedValue({
      success: true,
      turns: 5,
      totalDuration: 10000,
      tokens: { input: 1000, output: 500, total: 1500 },
      savedPath: './output/test.yaml',
      siteCapability: {
        domain: 'test.com',
        pages: {
          home: {
            elements: {
              search_input: {},
              submit_button: {},
            },
          },
        },
        global_elements: {},
      },
    }),
    close: vi.fn().mockResolvedValue(undefined),
  })),
}));

describe('TaskExecutor', () => {
  let executor: TaskExecutor;
  let mockDb: any;
  let selectChain: any;
  let updateChain: any;
  let mockConfig: TaskExecutorConfig;

  beforeEach(() => {
    // Setup mock database chains
    selectChain = {
      from: vi.fn().mockReturnThis(),
      innerJoin: vi.fn().mockReturnThis(),
      where: vi.fn().mockReturnThis(),
      limit: vi.fn().mockResolvedValue([]),
    };
    selectChain.from.mockReturnValue(selectChain);
    selectChain.innerJoin.mockReturnValue(selectChain);
    selectChain.where.mockReturnValue(selectChain);

    updateChain = {
      set: vi.fn().mockReturnThis(),
      where: vi.fn().mockResolvedValue([]),
    };
    updateChain.set.mockReturnValue(updateChain);

    mockDb = {
      select: vi.fn(() => selectChain),
      update: vi.fn(() => updateChain),
    } as any;

    mockConfig = {
      llmApiKey: 'test-api-key',
      llmBaseURL: 'https://api.test.com/v1',
      llmModel: 'test-model',
      databaseUrl: 'postgres://test:test@localhost:5432/test',
      headless: true,
      maxTurns: 30,
      outputDir: './output',
    };

    executor = new TaskExecutor(mockDb, mockConfig);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  // Helper to create mock chunk data response
  function mockChunkData(overrides: Partial<{
    id: number;
    source_id: number;
    document_url: string;
    document_title: string;
    source_domain: string;
    source_name: string;
    source_base_url: string;
    source_description: string;
    chunk_content: string;
    chunk_index: number;
  }> = {}) {
    return {
      id: overrides.id ?? 1001,
      source_id: overrides.source_id ?? 100,
      document_url: overrides.document_url ?? 'https://test.com/',
      document_title: overrides.document_title ?? 'Test Page',
      source_domain: overrides.source_domain ?? 'test.com',
      source_name: overrides.source_name ?? 'Test Site',
      source_base_url: overrides.source_base_url ?? 'https://test.com',
      source_description: overrides.source_description ?? 'Test site description',
      chunk_content: overrides.chunk_content ?? 'Test chunk content',
      chunk_index: overrides.chunk_index ?? 0,
      createdAt: new Date(),
    };
  }

  // Helper to create mock task
  // Note: Use 'in' operator to allow explicit null values (e.g., { chunkId: null })
  function createMockTask(overrides: Partial<RecordingTask> = {}): RecordingTask {
    return {
      id: overrides.id ?? 1,
      sourceId: overrides.sourceId ?? 100,
      chunkId: 'chunkId' in overrides ? overrides.chunkId! : 1001,
      startUrl: overrides.startUrl ?? 'https://test.com/',
      status: overrides.status ?? 'pending',
      progress: overrides.progress ?? 0,
      config: overrides.config ?? { chunk_type: 'task_driven' },
      attemptCount: overrides.attemptCount ?? 0,
      createdAt: overrides.createdAt ?? new Date(),
      updatedAt: overrides.updatedAt ?? new Date(),
      errorMessage: overrides.errorMessage,
      completedAt: overrides.completedAt,
      lastHeartbeat: overrides.lastHeartbeat,
    };
  }

  // ========================================================================
  // UT-TE-01: Execute task_driven task with ActionBuilder
  // ========================================================================
  it('UT-TE-01: Execute task_driven task with ActionBuilder', async () => {
    // Arrange
    const mockTask = createMockTask({
      config: { chunk_type: 'task_driven' },
    });

    selectChain.limit.mockResolvedValueOnce([mockChunkData({
      chunk_content: 'Task: Search for hotels\nSteps:\n1. Click search\n2. Type location',
    })]);

    // Act
    const result = await executor.execute(mockTask);

    // Assert: Success with recorded elements
    expect(result.success).toBe(true);
    expect(result.actions_created).toBe(2); // 2 elements from mock
    expect(result.turns).toBe(5);
    expect(result.tokens_used).toBe(1500);
    expect(result.saved_path).toBe('./output/test.yaml');
  });

  // ========================================================================
  // UT-TE-02: Execute exploratory task with ActionBuilder
  // ========================================================================
  it('UT-TE-02: Execute exploratory task with ActionBuilder', async () => {
    // Arrange
    const mockTask = createMockTask({
      config: { chunk_type: 'exploratory' },
    });

    selectChain.limit.mockResolvedValueOnce([mockChunkData({
      chunk_content: '# Search Results Page\n- Navigation bar\n- Filter sidebar',
    })]);

    // Act
    const result = await executor.execute(mockTask);

    // Assert
    expect(result.success).toBe(true);
    expect(result.actions_created).toBeGreaterThan(0);
  });

  // ========================================================================
  // UT-TE-03: ActionBuilder.build() receives correct custom prompts
  // ========================================================================
  it('UT-TE-03: ActionBuilder.build() receives correct custom prompts', async () => {
    // Arrange
    const { ActionBuilder } = await import('../../src/ActionBuilder');
    const mockTask = createMockTask({
      config: { chunk_type: 'task_driven' },
    });

    const testContent = 'Task: Test scenario\n1. Step one\n2. Step two';
    selectChain.limit.mockResolvedValueOnce([mockChunkData({
      chunk_content: testContent,
      document_url: 'https://example.com/test',
      document_title: 'Example Test',
      source_domain: 'example.com',
    })]);

    // Act
    await executor.execute(mockTask);

    // Assert: ActionBuilder.build() was called with correct options
    const mockInstance = (ActionBuilder as any).mock.results[0].value;
    expect(mockInstance.build).toHaveBeenCalledWith(
      'https://test.com', // startUrl from source.base_url
      expect.stringContaining('task_'), // scenario name
      expect.objectContaining({
        siteName: 'Test Site',
        customSystemPrompt: expect.stringContaining('interact'),
        customUserPrompt: expect.stringContaining(testContent),
      })
    );
  });

  // ========================================================================
  // UT-TE-04: Task status updated to completed on success
  // ========================================================================
  it('UT-TE-04: Task status updated to completed on success', async () => {
    // Arrange
    const mockTask = createMockTask({ attemptCount: 2 });

    selectChain.limit.mockResolvedValueOnce([mockChunkData()]);

    // Act
    const result = await executor.execute(mockTask);

    // Assert
    expect(result.success).toBe(true);
    expect(mockDb.update).toHaveBeenCalled();
    expect(updateChain.set).toHaveBeenCalledWith(
      expect.objectContaining({
        status: 'completed',
        progress: 100,
        attemptCount: 3, // Incremented
      })
    );
  });

  // ========================================================================
  // UT-TE-05: Task status updated to failed on error
  // ========================================================================
  it('UT-TE-05: Task status updated to failed on error', async () => {
    // Arrange
    const { ActionBuilder } = await import('../../src/ActionBuilder');
    const mockTask = createMockTask();

    selectChain.limit.mockResolvedValueOnce([mockChunkData()]);

    // Mock ActionBuilder.build() to fail
    const mockInstance = (ActionBuilder as any).mock.results[0]?.value;
    if (mockInstance) {
      mockInstance.build.mockResolvedValueOnce({
        success: false,
        message: 'Recording failed: browser timeout',
        turns: 3,
        totalDuration: 5000,
        tokens: { input: 300, output: 200, total: 500 },
      });
    }

    // Re-create executor to get fresh ActionBuilder instance
    const { ActionBuilder: AB } = await import('../../src/ActionBuilder');
    (AB as any).mockImplementationOnce(() => ({
      initialize: vi.fn().mockResolvedValue(undefined),
      build: vi.fn().mockResolvedValue({
        success: false,
        message: 'Recording failed: browser timeout',
        turns: 3,
        totalDuration: 5000,
        tokens: { input: 300, output: 200, total: 500 },
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }));

    const executor2 = new TaskExecutor(mockDb, mockConfig);

    // Act
    const result = await executor2.execute(mockTask);

    // Assert
    expect(result.success).toBe(false);
    expect(result.error).toContain('Recording failed');
    expect(updateChain.set).toHaveBeenCalledWith(
      expect.objectContaining({
        status: 'failed',
        attemptCount: 1,
      })
    );
  });

  // ========================================================================
  // UT-TE-06: Chunk not found throws error
  // ========================================================================
  it('UT-TE-06: Chunk not found throws error', async () => {
    // Arrange
    const mockTask = createMockTask();

    // Return empty result
    selectChain.limit.mockResolvedValueOnce([]);

    // Act
    const result = await executor.execute(mockTask);

    // Assert
    expect(result.success).toBe(false);
    expect(result.error).toContain('Chunk not found');
  });

  // ========================================================================
  // UT-TE-07: Null chunkId throws error
  // ========================================================================
  it('UT-TE-07: Null chunkId throws error', async () => {
    // Arrange
    const mockTask = createMockTask({ chunkId: null });

    // Act
    const result = await executor.execute(mockTask);

    // Assert
    expect(result.success).toBe(false);
    expect(result.error).toContain('Chunk ID is required');
  });

  // ========================================================================
  // UT-TE-08: Token limit applied (chunk > 24KB truncated)
  // ========================================================================
  it('UT-TE-08: Token limit applied (chunk > 24KB truncated)', async () => {
    // Arrange
    const { ActionBuilder } = await import('../../src/ActionBuilder');
    const mockTask = createMockTask();

    // Create large content (30KB)
    const largeContent = 'A'.repeat(30000);
    selectChain.limit.mockResolvedValueOnce([mockChunkData({
      chunk_content: largeContent,
    })]);

    // Act
    await executor.execute(mockTask);

    // Assert: ActionBuilder.build() was called with truncated content
    const mockInstance = (ActionBuilder as any).mock.results[0].value;
    const buildCall = mockInstance.build.mock.calls[0];
    const userPrompt = buildCall[2]?.customUserPrompt || '';

    // Content should be truncated (less than original 30KB)
    expect(userPrompt.length).toBeLessThan(30000);
    expect(userPrompt).toContain('[... content truncated ...]');
  });

  // ========================================================================
  // UT-TE-09: attempt_count incremented on each execution
  // ========================================================================
  it('UT-TE-09: attempt_count incremented on each execution', async () => {
    // Arrange
    const mockTask = createMockTask({ attemptCount: 4 });

    selectChain.limit.mockResolvedValueOnce([mockChunkData()]);

    // Act
    await executor.execute(mockTask);

    // Assert
    expect(updateChain.set).toHaveBeenCalledWith(
      expect.objectContaining({
        attemptCount: 5, // Incremented from 4 to 5
      })
    );
  });

  // ========================================================================
  // UT-TE-10: ActionBuilder closed after execution (success)
  // ========================================================================
  it('UT-TE-10: ActionBuilder closed after execution (success)', async () => {
    // Arrange
    const { ActionBuilder } = await import('../../src/ActionBuilder');
    const mockTask = createMockTask();

    selectChain.limit.mockResolvedValueOnce([mockChunkData()]);

    // Act
    await executor.execute(mockTask);

    // Assert: close() was called
    const mockInstance = (ActionBuilder as any).mock.results[0].value;
    expect(mockInstance.close).toHaveBeenCalledTimes(1);
  });

  // ========================================================================
  // UT-TE-11: ActionBuilder closed after execution (failure)
  // ========================================================================
  it('UT-TE-11: ActionBuilder closed after execution (failure)', async () => {
    // Arrange
    const { ActionBuilder } = await import('../../src/ActionBuilder');

    // Mock ActionBuilder to throw during build
    (ActionBuilder as any).mockImplementationOnce(() => ({
      initialize: vi.fn().mockResolvedValue(undefined),
      build: vi.fn().mockRejectedValue(new Error('Build crashed')),
      close: vi.fn().mockResolvedValue(undefined),
    }));

    const executor2 = new TaskExecutor(mockDb, mockConfig);
    const mockTask = createMockTask();

    selectChain.limit.mockResolvedValueOnce([mockChunkData()]);

    // Act
    const result = await executor2.execute(mockTask);

    // Assert: close() was still called despite failure
    expect(result.success).toBe(false);
    const mockInstance = (ActionBuilder as any).mock.results[0].value;
    expect(mockInstance.close).toHaveBeenCalledTimes(1);
  });

  // ========================================================================
  // UT-TE-12: Duration tracked in result
  // ========================================================================
  it('UT-TE-12: Duration tracked in result', async () => {
    // Arrange
    const mockTask = createMockTask();

    selectChain.limit.mockResolvedValueOnce([mockChunkData()]);

    // Act
    const result = await executor.execute(mockTask);

    // Assert: Duration is tracked as a number (may be 0 in mocked tests)
    expect(result.duration_ms).toBeGreaterThanOrEqual(0);
    expect(typeof result.duration_ms).toBe('number');
  });

  // ========================================================================
  // UT-TE-13: Uses source.base_url for ActionBuilder startUrl
  // ========================================================================
  it('UT-TE-13: Uses source.base_url for ActionBuilder startUrl', async () => {
    // Arrange
    const { ActionBuilder } = await import('../../src/ActionBuilder');
    const mockTask = createMockTask();

    selectChain.limit.mockResolvedValueOnce([mockChunkData({
      source_base_url: 'https://custom-start.example.com',
    })]);

    // Act
    await executor.execute(mockTask);

    // Assert: build() called with source.base_url
    const mockInstance = (ActionBuilder as any).mock.results[0].value;
    expect(mockInstance.build).toHaveBeenCalledWith(
      'https://custom-start.example.com',
      expect.any(String),
      expect.any(Object)
    );
  });

  // ========================================================================
  // UT-TE-14: Count elements from siteCapability correctly
  // ========================================================================
  it('UT-TE-14: Count elements from siteCapability correctly', async () => {
    // Arrange
    const { ActionBuilder } = await import('../../src/ActionBuilder');

    // Mock with multiple pages and global elements
    (ActionBuilder as any).mockImplementationOnce(() => ({
      initialize: vi.fn().mockResolvedValue(undefined),
      build: vi.fn().mockResolvedValue({
        success: true,
        turns: 10,
        totalDuration: 20000,
        tokens: { input: 2000, output: 1000, total: 3000 },
        savedPath: './output/multi.yaml',
        siteCapability: {
          domain: 'multi.com',
          pages: {
            home: {
              elements: {
                el1: {}, el2: {}, el3: {},
              },
            },
            search: {
              elements: {
                el4: {}, el5: {},
              },
            },
          },
          global_elements: {
            nav: {}, footer: {},
          },
        },
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }));

    const executor2 = new TaskExecutor(mockDb, mockConfig);
    const mockTask = createMockTask();

    selectChain.limit.mockResolvedValueOnce([mockChunkData()]);

    // Act
    const result = await executor2.execute(mockTask);

    // Assert: Total = 3 (home) + 2 (search) + 2 (global) = 7
    expect(result.actions_created).toBe(7);
  });
});
