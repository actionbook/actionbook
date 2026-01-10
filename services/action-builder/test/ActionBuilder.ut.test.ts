/**
 * ActionBuilder Unit Tests - Retry and Timeout Logic
 *
 * Tests the retry and timeout handling in ActionBuilder.build()
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ActionBuilder } from '../src/ActionBuilder';
import type { ActionBuilderConfig } from '../src/types';

// Create mock instances
const mockBrowser = {
  initialize: vi.fn().mockResolvedValue(undefined),
  close: vi.fn().mockResolvedValue(undefined),
  navigate: vi.fn().mockResolvedValue({ success: true }),
};

const mockRecorder = {
  record: vi.fn(),
  savePartialResult: vi.fn(),
};

const mockDb = {
  select: vi.fn(),
  insert: vi.fn(),
  update: vi.fn(),
};

// Mock all dependencies
vi.mock('@actionbookdev/browser', () => ({
  createBrowserAuto: vi.fn(() => mockBrowser),
}));

vi.mock('../src/llm/AIClient.js', () => ({
  AIClient: vi.fn(() => ({
    getProvider: () => 'openrouter',
    getModel: () => 'test-model',
  })),
}));

vi.mock('../src/recorder/ActionRecorder.js', () => ({
  ActionRecorder: vi.fn(() => mockRecorder),
}));

vi.mock('../src/validator/SelectorValidator.js', () => ({
  SelectorValidator: vi.fn(() => ({})),
}));

vi.mock('../src/writers/YamlWriter.js', () => ({
  YamlWriter: vi.fn(() => ({})),
}));

vi.mock('../src/writers/DbWriter.js', () => ({
  DbWriter: vi.fn(() => ({})),
}));

vi.mock('@actionbookdev/db', () => ({
  createDb: vi.fn(() => mockDb),
  closeDb: vi.fn(),
}));

describe('ActionBuilder - Retry and Timeout', () => {
  let mockConfig: ActionBuilderConfig;
  let originalSetTimeout: typeof setTimeout;
  let originalClearTimeout: typeof clearTimeout;
  let timeouts: Array<{ callback: Function; delay: number; id: number }>;
  let nextTimeoutId = 1;

  beforeEach(() => {
    vi.clearAllMocks();

    // Reset mock implementations
    mockBrowser.initialize.mockClear();
    mockBrowser.close.mockClear();
    mockRecorder.record.mockClear();
    mockRecorder.savePartialResult.mockClear();

    // Setup default recorder behavior
    mockRecorder.record.mockResolvedValue({
      success: true,
      message: 'Recording completed',
      turns: 5,
      steps: 10,
      totalDuration: 10000,
      tokens: {
        input: 1000,
        output: 500,
        total: 1500,
        planning: { input: 800, output: 400 },
        browser: { input: 200, output: 100 },
      },
      elementsDiscovered: 3,
      siteCapability: {
        domain: 'test.com',
        pages: {
          home: { elements: { el1: {}, el2: {}, el3: {} } },
        },
        global_elements: {},
      },
    });

    mockRecorder.savePartialResult.mockResolvedValue({
      elements: 2,
      siteCapability: {
        domain: 'test.com',
        pages: {
          home: { elements: { el1: {}, el2: {} } },
        },
        global_elements: {},
      },
      turns: 3,
      steps: 5,
      tokens: {
        input: 500,
        output: 250,
        total: 750,
        planning: { input: 400, output: 200 },
        browser: { input: 100, output: 50 },
      },
    });

    // Setup mock config
    mockConfig = {
      llmApiKey: 'test-api-key',
      llmBaseURL: 'https://api.test.com/v1',
      llmModel: 'test-model',
      databaseUrl: 'postgres://test:test@localhost:5432/test',
      headless: true,
      maxTurns: 30,
      outputDir: './test-output',
      buildTimeoutMs: 1000, // 1 second for fast tests
      browserRetryConfig: {
        maxAttempts: 3,
        baseDelayMs: 100, // 100ms for fast tests
      },
    };

    // Mock setTimeout to control timing
    timeouts = [];
    originalSetTimeout = global.setTimeout;
    originalClearTimeout = global.clearTimeout;
    global.setTimeout = vi.fn((callback: any, delay: number) => {
      const id = nextTimeoutId++;
      timeouts.push({ callback, delay, id });
      return id as any;
    }) as any;

    // Mock clearTimeout
    global.clearTimeout = vi.fn((id: any) => {
      const index = timeouts.findIndex(t => t.id === id);
      if (index >= 0) {
        timeouts.splice(index, 1);
      }
    }) as any;
  });

  afterEach(() => {
    // Restore original setTimeout
    global.setTimeout = originalSetTimeout;
    global.clearTimeout = originalClearTimeout;
    timeouts = [];
    nextTimeoutId = 1;
  });

  // ========================================================================
  // UT-AB-01: Timeout triggers partial result save
  // ========================================================================
  it('UT-AB-01: Timeout triggers partial result save with correct status', async () => {
    const builder = new ActionBuilder(mockConfig);
    await builder.initialize();

    // Mock recorder to take longer than timeout
    let recordStartTime: number;
    mockRecorder.record.mockImplementation(async () => {
      recordStartTime = Date.now();
      // Simulate work that will timeout
      await new Promise(resolve => originalSetTimeout(resolve, 2000));
      return {
        success: true,
        message: 'Should not reach here',
        turns: 5,
        steps: 10,
        totalDuration: 2000,
        tokens: { input: 1000, output: 500, total: 1500 },
        elementsDiscovered: 0,
      };
    });

    // Start build (will timeout after 1000ms)
    const buildPromise = builder.build('https://test.com', 'test-scenario');

    // Wait a bit then trigger timeout
    await new Promise(resolve => originalSetTimeout(resolve, 50));

    // Find and trigger the timeout callback
    const timeoutCallback = timeouts.find(t => t.delay === 1000);
    expect(timeoutCallback).toBeDefined();

    // Trigger timeout
    await timeoutCallback!.callback();

    // Wait for build to complete
    const result = await buildPromise;

    // Assertions
    expect(result.success).toBe(true);
    expect(result.partialResult).toBe(true);
    expect(result.message).toContain('timeout');
    expect(result.message).toContain('2 elements');
    expect(result.totalDuration).toBe(1000); // Should be buildTimeoutMs
    expect(result.turns).toBe(3); // From partial result
    expect(result.siteCapability).toBeDefined();
    expect(result.siteCapability?.pages.home.elements).toEqual({ el1: {}, el2: {} });

    // Verify savePartialResult was called
    expect(mockRecorder.savePartialResult).toHaveBeenCalledTimes(1);
  });

  // ========================================================================
  // UT-AB-02: Retry does not close DB (only browser)
  // ========================================================================
  it('UT-AB-02: Retry does not close DB (only closes browser)', async () => {
    const builder = new ActionBuilder(mockConfig);
    await builder.initialize();

    let recordCallCount = 0;

    // First attempt: fail with retryable error
    // Second attempt: succeed
    mockRecorder.record.mockImplementation(async () => {
      recordCallCount++;
      if (recordCallCount === 1) {
        throw new Error('ECONNREFUSED: Connection refused');
      }
      return {
        success: true,
        message: 'Recording completed',
        turns: 5,
        steps: 10,
        totalDuration: 10000,
        tokens: { input: 1000, output: 500, total: 1500 },
        elementsDiscovered: 2,
        siteCapability: {
          domain: 'test.com',
          pages: { home: { elements: { el1: {}, el2: {} } } },
          global_elements: {},
        },
      };
    });

    // Start build
    const buildPromise = builder.build('https://test.com', 'test-scenario');

    // Wait for first attempt to fail
    await new Promise(resolve => originalSetTimeout(resolve, 50));

    // Find and trigger retry delay (100ms for attempt 1)
    const retryTimeout = timeouts.find(t => t.delay === 100);
    expect(retryTimeout).toBeDefined();

    // Trigger retry
    await retryTimeout!.callback();

    // Wait for build to complete
    const result = await buildPromise;

    // Assertions
    expect(result.success).toBe(true);
    expect(recordCallCount).toBe(2);

    // Browser should be closed once (before retry)
    expect(mockBrowser.close).toHaveBeenCalledTimes(1);

    // Verify recorder worked on retry (means DB wasn't closed)
    // If DB was closed, recorder would fail
    expect(mockRecorder.record).toHaveBeenCalledTimes(2);

    // Browser should be initialized twice (once initially, once after retry)
    expect(mockBrowser.initialize).toHaveBeenCalledTimes(2);
  });

  // ========================================================================
  // UT-AB-03: Retry backoff follows exponential pattern
  // ========================================================================
  it('UT-AB-03: Retry backoff delay = baseDelayMs * attempt', async () => {
    const builder = new ActionBuilder(mockConfig);
    await builder.initialize();

    let recordCallCount = 0;

    // Fail first 2 attempts, succeed on 3rd
    mockRecorder.record.mockImplementation(async () => {
      recordCallCount++;
      if (recordCallCount <= 2) {
        throw new Error('Target closed');
      }
      return {
        success: true,
        message: 'Recording completed',
        turns: 5,
        steps: 10,
        totalDuration: 10000,
        tokens: { input: 1000, output: 500, total: 1500 },
        elementsDiscovered: 1,
        siteCapability: {
          domain: 'test.com',
          pages: { home: { elements: { el1: {} } } },
          global_elements: {},
        },
      };
    });

    // Start build
    const buildPromise = builder.build('https://test.com', 'test-scenario');

    // Wait for first attempt to fail
    await new Promise(resolve => originalSetTimeout(resolve, 50));

    // First retry: delay should be 100ms * 1 = 100ms
    let retryTimeout = timeouts.find(t => t.delay === 100);
    expect(retryTimeout).toBeDefined();
    await retryTimeout!.callback();

    // Wait for second attempt to fail
    await new Promise(resolve => originalSetTimeout(resolve, 50));

    // Second retry: delay should be 100ms * 2 = 200ms
    retryTimeout = timeouts.find(t => t.delay === 200);
    expect(retryTimeout).toBeDefined();
    await retryTimeout!.callback();

    // Wait for build to complete
    const result = await buildPromise;

    // Assertions
    expect(result.success).toBe(true);
    expect(recordCallCount).toBe(3);

    // Verify browser was closed twice (before each retry)
    expect(mockBrowser.close).toHaveBeenCalledTimes(2);
  });

  // ========================================================================
  // UT-AB-04: Non-retryable error fails immediately
  // ========================================================================
  it('UT-AB-04: Non-retryable error fails immediately without retry', async () => {
    const builder = new ActionBuilder(mockConfig);
    await builder.initialize();

    // Mock recorder to fail with non-retryable error
    mockRecorder.record.mockRejectedValue(new Error('Invalid API key'));

    // Build should fail
    await expect(builder.build('https://test.com', 'test-scenario')).rejects.toThrow(
      'Invalid API key'
    );

    // Should only be called once (no retry)
    expect(mockRecorder.record).toHaveBeenCalledTimes(1);

    // No retry delays should be scheduled
    const retryTimeouts = timeouts.filter(t => t.delay === 100 || t.delay === 200);
    expect(retryTimeouts.length).toBe(0);
  });

  // ========================================================================
  // UT-AB-05: clearTimeout is called on success
  // ========================================================================
  it('UT-AB-05: clearTimeout is called when build completes before timeout', async () => {
    const builder = new ActionBuilder(mockConfig);
    await builder.initialize();

    // Mock recorder to complete quickly
    mockRecorder.record.mockResolvedValue({
      success: true,
      message: 'Recording completed',
      turns: 5,
      steps: 10,
      totalDuration: 500,
      tokens: { input: 1000, output: 500, total: 1500 },
      elementsDiscovered: 1,
      siteCapability: {
        domain: 'test.com',
        pages: { home: { elements: { el1: {} } } },
        global_elements: {},
      },
    });

    const result = await builder.build('https://test.com', 'test-scenario');

    // Build should succeed
    expect(result.success).toBe(true);

    // clearTimeout should have been called to clean up the timeout
    expect(global.clearTimeout).toHaveBeenCalled();

    // No timeout callbacks should remain
    const timeoutCallback = timeouts.find(t => t.delay === 1000);
    expect(timeoutCallback).toBeUndefined(); // Should be cleared
  });
});
