/**
 * ActionRecorder Unit Tests
 *
 * Tests termination conditions, retry logic, external domain detection,
 * and result finalization.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ActionRecorder } from '../../src/recorder/ActionRecorder';
import type { BrowserAdapter } from '../../src/browser/BrowserAdapter';
import type { RecorderConfig } from '../../src/types/index';

// Mock dependencies
const createMockBrowser = (): BrowserAdapter => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  getPage: vi.fn().mockResolvedValue({ url: () => 'https://test.com' }),
  navigate: vi.fn().mockResolvedValue(undefined),
  observe: vi.fn().mockResolvedValue([]),
  act: vi.fn().mockResolvedValue({ success: true }),
  actWithSelector: vi.fn().mockResolvedValue({ success: true }),
  autoClosePopups: vi.fn().mockResolvedValue(0),
  getElementAttributesFromXPath: vi.fn().mockResolvedValue(null),
  wait: vi.fn().mockResolvedValue(undefined),
  waitForText: vi.fn().mockResolvedValue(undefined),
  scroll: vi.fn().mockResolvedValue(undefined),
  close: vi.fn().mockResolvedValue(undefined),
});

const createMockAIClient = () => ({
  chat: vi.fn(),
  getProvider: vi.fn().mockReturnValue('openai'),
  getModel: vi.fn().mockReturnValue('gpt-4'),
});

const createMockDbWriter = () => ({
  save: vi.fn().mockResolvedValue(1),
  createTask: vi.fn().mockResolvedValue(1),
  addStep: vi.fn().mockResolvedValue(undefined),
  completeTask: vi.fn().mockResolvedValue(undefined),
});

// Helper to create LLM response with tool calls
function createToolCallResponse(toolCalls: Array<{ name: string; args: Record<string, unknown> }>) {
  return {
    choices: [{
      message: {
        content: null,
        tool_calls: toolCalls.map((tc, i) => ({
          id: `call_${i}`,
          type: 'function' as const,
          function: {
            name: tc.name,
            arguments: JSON.stringify(tc.args),
          },
        })),
      },
    }],
    usage: { prompt_tokens: 100, completion_tokens: 50 },
  };
}

// Helper to create LLM response without tool calls (completion)
function createCompletionResponse(content: string) {
  return {
    choices: [{
      message: {
        content,
        tool_calls: undefined,
      },
    }],
    usage: { prompt_tokens: 100, completion_tokens: 50 },
  };
}

// Mock humanDelay to be instant
vi.mock('../../src/utils/index.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../src/utils/index.js')>();
  return {
    ...original,
    humanDelay: vi.fn().mockResolvedValue(undefined),
  };
});

// Mock sleep to be instant
vi.mock('../../src/utils/retry.js', () => ({
  sleep: vi.fn().mockResolvedValue(undefined),
}));

describe('ActionRecorder', () => {
  let mockBrowser: BrowserAdapter;
  let mockAIClient: ReturnType<typeof createMockAIClient>;
  let mockDbWriter: ReturnType<typeof createMockDbWriter>;
  let defaultConfig: RecorderConfig;

  beforeEach(() => {
    mockBrowser = createMockBrowser();
    mockAIClient = createMockAIClient();
    mockDbWriter = createMockDbWriter();
    defaultConfig = {
      maxTurns: 20,
      outputDir: './test-output',
    };
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  // ==========================================================================
  // Termination Condition Tests
  // ==========================================================================
  describe('Termination Conditions', () => {
    // UT-AR-01: Element threshold terminates recording
    it('UT-AR-01: Element threshold terminates recording', async () => {
      const config: RecorderConfig = {
        ...defaultConfig,
        terminationConfig: {
          elementThreshold: 3, // Very low threshold for testing
        },
      };

      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        config,
        mockDbWriter as any
      );

      // Mock observe to return elements
      (mockBrowser.observe as any).mockResolvedValue([
        { description: 'Button 1', selector: 'xpath=//*[@id="btn1"]', method: 'click' },
      ]);

      // Mock LLM to navigate, then register elements
      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com' } },
          ]));
        } else if (callCount === 2) {
          return Promise.resolve(createToolCallResponse([
            { name: 'set_page_context', args: { page_type: 'home', page_name: 'Home' } },
          ]));
        } else if (callCount <= 5) {
          return Promise.resolve(createToolCallResponse([
            {
              name: 'register_element',
              args: {
                element_id: `element_${callCount}`,
                description: `Element ${callCount}`,
                element_type: 'button',
                allow_methods: ['click'],
                xpath_selector: `//*[@id="el${callCount}"]`,
              },
            },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.terminationReason).toBe('element_threshold_reached');
      expect(result.elementsDiscovered).toBeGreaterThanOrEqual(3);
    });

    // UT-AR-02: Max turns terminates recording
    it('UT-AR-02: Max turns terminates recording', async () => {
      const config: RecorderConfig = {
        ...defaultConfig,
        maxTurns: 3,
      };

      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        config,
        mockDbWriter as any
      );

      // Mock LLM to keep returning tool calls (never complete)
      mockAIClient.chat.mockResolvedValue(createToolCallResponse([
        { name: 'navigate', args: { url: 'https://test.com' } },
      ]));

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.terminationReason).toBe('max_turns_reached');
      expect(result.turns).toBe(3);
    });

    // UT-AR-03: Max pages visited terminates recording
    it('UT-AR-03: Max pages visited terminates recording', async () => {
      const config: RecorderConfig = {
        ...defaultConfig,
        maxVisitedPages: 2,
      };

      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        config,
        mockDbWriter as any
      );

      let navCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        navCount++;
        if (navCount <= 3) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: `https://test.com/page${navCount}` } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.terminationReason).toBe('max_pages_visited');
      expect(result.visitedPagesCount).toBe(2);
    });

    // UT-AR-04: Low observe efficiency terminates recording
    it('UT-AR-04: Low observe efficiency terminates recording', async () => {
      const config: RecorderConfig = {
        ...defaultConfig,
        terminationConfig: {
          minObserveEfficiency: 5, // Require at least 5 elements per observe
          minObserveCallsForCheck: 3, // Check after 3 calls
        },
      };

      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        config,
        mockDbWriter as any
      );

      // Mock observe to return only 1 element (below threshold)
      (mockBrowser.observe as any).mockResolvedValue([
        { description: 'Single element', selector: 'xpath=//div', method: 'click' },
      ]);

      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com' } },
          ]));
        } else if (callCount <= 5) {
          return Promise.resolve(createToolCallResponse([
            { name: 'observe_page', args: { focus: 'elements' } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.terminationReason).toBe('low_observe_efficiency');
      expect(result.observeStats?.avgEfficiency).toBeLessThan(5);
    });

    // UT-AR-05: Normal completion returns 'completed'
    it('UT-AR-05: Normal completion returns completed', async () => {
      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        defaultConfig,
        mockDbWriter as any
      );

      mockAIClient.chat
        .mockResolvedValueOnce(createToolCallResponse([
          { name: 'navigate', args: { url: 'https://test.com' } },
        ]))
        .mockResolvedValueOnce(createCompletionResponse('Recording complete'));

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.terminationReason).toBe('completed');
      // success depends on whether elements were discovered
      // In this test, no elements were registered, so success is false
      expect(result.success).toBe(false);
      expect(result.message).toContain('Recording complete');
    });
  });

  // ==========================================================================
  // External Domain Detection Tests
  // ==========================================================================
  describe('External Domain Detection', () => {
    // UT-AR-06: External domain navigation is blocked without loading the page
    it('UT-AR-06: External domain navigation is blocked without loading the page', async () => {
      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        defaultConfig,
        mockDbWriter as any
      );

      // Track navigate calls
      const navigateCalls: string[] = [];
      (mockBrowser.navigate as any).mockImplementation((url: string) => {
        navigateCalls.push(url);
        return Promise.resolve();
      });

      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          // First: navigate to primary domain
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://primary.com/' } },
          ]));
        } else if (callCount === 2) {
          // Second: navigate to another page on primary (to set previousUrl)
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://primary.com/page2' } },
          ]));
        } else if (callCount === 3) {
          // Third: try to navigate to external domain (should be blocked)
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://external.com/page' } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      // Should have navigated to primary and page2 only
      // External domain navigation should be BLOCKED (not loaded at all)
      expect(navigateCalls).toContain('https://primary.com/');
      expect(navigateCalls).toContain('https://primary.com/page2');
      expect(navigateCalls).not.toContain('https://external.com/page');
      // Only 2 navigations should have occurred
      expect(navigateCalls.length).toBe(2);
    });

    // UT-AR-07: Same domain subpages are allowed
    it('UT-AR-07: Same domain subpages are allowed', async () => {
      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        defaultConfig,
        mockDbWriter as any
      );

      const navigateCalls: string[] = [];
      (mockBrowser.navigate as any).mockImplementation((url: string) => {
        navigateCalls.push(url);
        return Promise.resolve();
      });

      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com/' } },
          ]));
        } else if (callCount === 2) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com/page2' } },
          ]));
        } else if (callCount === 3) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com/page3' } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      // All navigations should be to same domain, no going back
      expect(navigateCalls.length).toBe(3);
      expect(result.visitedPagesCount).toBe(3);
    });

    // UT-AR-08: Already visited URL is blocked without loading the page
    it('UT-AR-08: Already visited URL is blocked without loading the page', async () => {
      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        defaultConfig,
        mockDbWriter as any
      );

      const navigateCalls: string[] = [];
      (mockBrowser.navigate as any).mockImplementation((url: string) => {
        navigateCalls.push(url);
        return Promise.resolve();
      });

      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com/' } },
          ]));
        } else if (callCount === 2) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com/page2' } },
          ]));
        } else if (callCount === 3) {
          // Try to revisit first page (should be blocked)
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com/' } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      // Should detect already visited and BLOCK (not reload)
      expect(result.visitedPagesCount).toBe(2); // Only 2 unique pages
      // Only 2 actual navigations should have occurred (first visit + page2)
      // The revisit attempt to https://test.com/ should be blocked
      expect(navigateCalls.length).toBe(2);
      expect(navigateCalls[0]).toBe('https://test.com/');
      expect(navigateCalls[1]).toBe('https://test.com/page2');
    });
  });

  // ==========================================================================
  // Retry Logic Tests
  // ==========================================================================
  describe('Retry Logic', () => {
    // UT-AR-09: observe_page retries on timeout
    it('UT-AR-09: observe_page retries on timeout', async () => {
      const config: RecorderConfig = {
        ...defaultConfig,
        operationTimeouts: { observe: 100 },
        retryConfig: { maxAttempts: 3, baseDelayMs: 10 },
      };

      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        config,
        mockDbWriter as any
      );

      // Mock observe to fail twice, then succeed
      let observeAttempts = 0;
      (mockBrowser.observe as any).mockImplementation(() => {
        observeAttempts++;
        if (observeAttempts < 3) {
          return Promise.reject(new Error('observe_page timeout'));
        }
        return Promise.resolve([{ description: 'Found', selector: '//div', method: 'click' }]);
      });

      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com' } },
          ]));
        } else if (callCount === 2) {
          return Promise.resolve(createToolCallResponse([
            { name: 'observe_page', args: { focus: 'buttons' } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      // Should have retried observe 3 times
      expect(observeAttempts).toBe(3);
    });

    // UT-AR-10: Tool skipped after max retries
    it('UT-AR-10: Tool skipped after max retries', async () => {
      const config: RecorderConfig = {
        ...defaultConfig,
        operationTimeouts: { observe: 100 },
        retryConfig: { maxAttempts: 2, baseDelayMs: 10 },
      };

      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        config,
        mockDbWriter as any
      );

      // Mock observe to always fail
      (mockBrowser.observe as any).mockRejectedValue(new Error('observe_page timeout'));

      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com' } },
          ]));
        } else if (callCount === 2) {
          return Promise.resolve(createToolCallResponse([
            { name: 'observe_page', args: { focus: 'buttons' } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      // Recording should complete (tool was skipped, not failed entirely)
      expect(result.terminationReason).toBe('completed');
    });
  });

  // ==========================================================================
  // Result Finalization Tests
  // ==========================================================================
  describe('Result Finalization', () => {
    // UT-AR-11: Tokens are tracked correctly
    it('UT-AR-11: Tokens are tracked correctly', async () => {
      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        defaultConfig,
        mockDbWriter as any
      );

      mockAIClient.chat
        .mockResolvedValueOnce({
          ...createToolCallResponse([{ name: 'navigate', args: { url: 'https://test.com' } }]),
          usage: { prompt_tokens: 100, completion_tokens: 50 },
        })
        .mockResolvedValueOnce({
          ...createCompletionResponse('Done'),
          usage: { prompt_tokens: 200, completion_tokens: 100 },
        });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.tokens.input).toBe(300); // 100 + 200
      expect(result.tokens.output).toBe(150); // 50 + 100
      expect(result.tokens.total).toBe(450);
    });

    // UT-AR-12: Observe stats are tracked
    it('UT-AR-12: Observe stats are tracked', async () => {
      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        defaultConfig,
        mockDbWriter as any
      );

      // Mock observe to return different numbers of elements
      (mockBrowser.observe as any)
        .mockResolvedValueOnce([
          { description: 'El 1', selector: '//a', method: 'click' },
          { description: 'El 2', selector: '//b', method: 'click' },
        ])
        .mockResolvedValueOnce([
          { description: 'El 3', selector: '//c', method: 'click' },
        ]);

      let callCount = 0;
      mockAIClient.chat.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(createToolCallResponse([
            { name: 'navigate', args: { url: 'https://test.com' } },
          ]));
        } else if (callCount === 2) {
          return Promise.resolve(createToolCallResponse([
            { name: 'observe_page', args: { focus: 'buttons' } },
          ]));
        } else if (callCount === 3) {
          return Promise.resolve(createToolCallResponse([
            { name: 'observe_page', args: { focus: 'links' } },
          ]));
        }
        return Promise.resolve(createCompletionResponse('Done'));
      });

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.observeStats?.totalCalls).toBe(2);
      expect(result.observeStats?.totalElements).toBe(3); // 2 + 1
      expect(result.observeStats?.avgEfficiency).toBe(1.5); // 3/2
    });

    // UT-AR-13: DbWriter.completeTask called with correct status
    it('UT-AR-13: DbWriter.completeTask called with correct status', async () => {
      const recorder = new ActionRecorder(
        mockBrowser,
        mockAIClient as any,
        defaultConfig,
        mockDbWriter as any
      );

      // Make sure save returns a sourceId so task can be created
      mockDbWriter.save.mockResolvedValue(1);
      mockDbWriter.createTask.mockResolvedValue(100);

      mockAIClient.chat
        .mockResolvedValueOnce(createToolCallResponse([
          { name: 'navigate', args: { url: 'https://test.com' } },
        ]))
        .mockResolvedValueOnce(createToolCallResponse([
          { name: 'set_page_context', args: { page_type: 'home', page_name: 'Home' } },
        ]))
        .mockResolvedValueOnce(createToolCallResponse([
          {
            name: 'register_element',
            args: {
              element_id: 'btn1',
              description: 'Button 1',
              element_type: 'button',
              allow_methods: ['click'],
              xpath_selector: '//button[@id="btn1"]',
            },
          },
        ]))
        .mockResolvedValueOnce(createCompletionResponse('Done'));

      const result = await recorder.record(
        'test scenario',
        'system prompt',
        'user message'
      );

      expect(result.success).toBe(true);
      expect(mockDbWriter.completeTask).toHaveBeenCalledWith(
        100, // taskId
        'completed',
        expect.any(Number), // duration
        expect.any(Number), // tokens
        undefined // no error message for success
      );
    });
  });
});
