/**
 * AgentCoreBrowser Unit Tests
 *
 * Tests internal logic, parameter validation, and response parsing
 * without requiring real AWS environment.
 *
 * Run with: pnpm test
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mock external dependencies before importing the class
vi.mock('bedrock-agentcore/browser/playwright', () => ({
  PlaywrightBrowser: vi.fn().mockImplementation(() => ({
    navigate: vi.fn(),
    back: vi.fn(),
    evaluate: vi.fn(),
    getHtml: vi.fn(),
    screenshot: vi.fn(),
    click: vi.fn(),
    fill: vi.fn(),
    type: vi.fn(),
    getText: vi.fn(),
    waitForSelector: vi.fn(),
    pressKey: vi.fn(),
    stopSession: vi.fn(),
  })),
}));

vi.mock('@aws-sdk/client-bedrock-runtime', () => ({
  BedrockRuntimeClient: vi.fn().mockImplementation(() => ({
    send: vi.fn(),
  })),
  InvokeModelCommand: vi.fn(),
}));

// Import after mocking
import { AgentCoreBrowser } from '../../src/implementations/agent-core-browser.js';
import { PlaywrightBrowser } from 'bedrock-agentcore/browser/playwright';
import { BedrockRuntimeClient } from '@aws-sdk/client-bedrock-runtime';

describe('AgentCoreBrowser Unit Tests', () => {
  let browser: AgentCoreBrowser;
  let mockClient: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    browser = new AgentCoreBrowser();
  });

  afterEach(async () => {
    // Clean up
  });

  // ============================================
  // Configuration Tests
  // ============================================

  describe('Configuration', () => {
    it('should use default config values', () => {
      const browser = new AgentCoreBrowser();
      // Access private config through any type assertion for testing
      const config = (browser as any).config;

      expect(config.sessionTimeoutMinutes).toBe(15);
      expect(config.region).toBe('us-east-1');
      expect(config.timeout).toBe(60000);
      expect(config.bedrockModelId).toBe('us.anthropic.claude-3-5-sonnet-20241022-v2:0');
    });

    it('should accept custom config values', () => {
      const browser = new AgentCoreBrowser({
        sessionTimeoutMinutes: 30,
        region: 'eu-west-1',
        timeout: 120000,
        bedrockModelId: 'custom-model-id',
      });
      const config = (browser as any).config;

      expect(config.sessionTimeoutMinutes).toBe(30);
      expect(config.region).toBe('eu-west-1');
      expect(config.timeout).toBe(120000);
      expect(config.bedrockModelId).toBe('custom-model-id');
    });

    it('should use environment variables as fallback', () => {
      const originalRegion = process.env.AWS_REGION;
      const originalModelId = process.env.AGENTCORE_BROWSER_MODEL_ID;

      process.env.AWS_REGION = 'ap-northeast-1';
      process.env.AGENTCORE_BROWSER_MODEL_ID = 'env-model-id';

      const browser = new AgentCoreBrowser();
      const config = (browser as any).config;

      expect(config.region).toBe('ap-northeast-1');
      expect(config.bedrockModelId).toBe('env-model-id');

      // Restore (must use delete for undefined, not assignment)
      if (originalRegion === undefined) {
        delete process.env.AWS_REGION;
      } else {
        process.env.AWS_REGION = originalRegion;
      }
      if (originalModelId === undefined) {
        delete process.env.AGENTCORE_BROWSER_MODEL_ID;
      } else {
        process.env.AGENTCORE_BROWSER_MODEL_ID = originalModelId;
      }
    });
  });

  // ============================================
  // Initialization State Tests
  // ============================================

  describe('Initialization State', () => {
    it('should throw when navigate() called before initialize()', async () => {
      await expect(browser.navigate('https://example.com')).rejects.toThrow(
        'Browser not initialized'
      );
    });

    it('should throw when getTitle() called before initialize()', async () => {
      await expect(browser.getTitle()).rejects.toThrow('Browser not initialized');
    });

    it('should throw when screenshot() called before initialize()', async () => {
      await expect(browser.screenshot()).rejects.toThrow('Browser not initialized');
    });

    it('should return about:blank for getUrl() before navigation', () => {
      expect(browser.getUrl()).toBe('about:blank');
    });

    it('should initialize successfully', async () => {
      await browser.initialize();

      expect(PlaywrightBrowser).toHaveBeenCalledWith({
        region: 'us-east-1',
      });
      expect(BedrockRuntimeClient).toHaveBeenCalledWith({
        region: 'us-east-1',
      });
    });

    it('should not reinitialize if already initialized', async () => {
      await browser.initialize();
      await browser.initialize(); // Second call

      expect(PlaywrightBrowser).toHaveBeenCalledTimes(1);
    });
  });

  // ============================================
  // Screenshot Format Conversion Tests
  // ============================================

  describe('Screenshot Format Conversion', () => {
    beforeEach(async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
      mockClient.screenshot.mockResolvedValue(Buffer.from('fake-image').toString('base64'));
    });

    it('should convert webp to png', async () => {
      await browser.screenshot({ format: 'webp' });

      expect(mockClient.screenshot).toHaveBeenCalledWith({
        fullPage: false,
        type: 'png', // webp converted to png
      });
    });

    it('should pass jpeg format as-is', async () => {
      await browser.screenshot({ format: 'jpeg' });

      expect(mockClient.screenshot).toHaveBeenCalledWith({
        fullPage: false,
        type: 'jpeg',
      });
    });

    it('should default to png when no format specified', async () => {
      await browser.screenshot();

      expect(mockClient.screenshot).toHaveBeenCalledWith({
        fullPage: false,
        type: 'png',
      });
    });

    it('should pass fullPage option', async () => {
      await browser.screenshot({ fullPage: true });

      expect(mockClient.screenshot).toHaveBeenCalledWith({
        fullPage: true,
        type: 'png',
      });
    });

    it('should convert base64 string to Buffer', async () => {
      const base64Data = Buffer.from('test-image').toString('base64');
      mockClient.screenshot.mockResolvedValue(base64Data);

      const result = await browser.screenshot();

      expect(result).toBeInstanceOf(Buffer);
      expect(result.toString()).toBe('test-image');
    });
  });

  // ============================================
  // waitForSelector State Mapping Tests
  // ============================================

  describe('waitForSelector State Mapping', () => {
    beforeEach(async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
      mockClient.waitForSelector.mockResolvedValue(undefined);
    });

    it('should map hidden option to hidden state', async () => {
      await browser.waitForSelector('div', { hidden: true });

      expect(mockClient.waitForSelector).toHaveBeenCalledWith({
        selector: 'div',
        timeout: 30000,
        state: 'hidden',
      });
    });

    it('should map visible option to visible state', async () => {
      await browser.waitForSelector('div', { visible: true });

      expect(mockClient.waitForSelector).toHaveBeenCalledWith({
        selector: 'div',
        timeout: 30000,
        state: 'visible',
      });
    });

    it('should default to attached state', async () => {
      await browser.waitForSelector('div');

      expect(mockClient.waitForSelector).toHaveBeenCalledWith({
        selector: 'div',
        timeout: 30000,
        state: 'attached',
      });
    });

    it('should use custom timeout', async () => {
      await browser.waitForSelector('div', { timeout: 5000 });

      expect(mockClient.waitForSelector).toHaveBeenCalledWith({
        selector: 'div',
        timeout: 5000,
        state: 'attached',
      });
    });
  });

  // ============================================
  // Scroll Direction Tests
  // ============================================

  describe('Scroll Direction', () => {
    beforeEach(async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
      mockClient.evaluate.mockResolvedValue(undefined);
    });

    it('should scroll down with positive delta', async () => {
      await browser.scroll('down', 100);

      expect(mockClient.evaluate).toHaveBeenCalledWith({
        script: 'window.scrollBy(0, 100)',
      });
    });

    it('should scroll up with negative delta', async () => {
      await browser.scroll('up', 100);

      expect(mockClient.evaluate).toHaveBeenCalledWith({
        script: 'window.scrollBy(0, -100)',
      });
    });

    it('should use default amount of 300', async () => {
      await browser.scroll('down');

      expect(mockClient.evaluate).toHaveBeenCalledWith({
        script: 'window.scrollBy(0, 300)',
      });
    });
  });

  // ============================================
  // act() Routing Tests
  // ============================================

  describe('act() Routing', () => {
    beforeEach(async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
    });

    it('should route ActionObject to actWithSelector', async () => {
      mockClient.click.mockResolvedValue(undefined);

      const result = await browser.act({
        selector: '#btn',
        method: 'click',
        description: 'test button',
      });

      expect(mockClient.click).toHaveBeenCalledWith({ selector: '#btn' });
      expect(result).toEqual({ success: true, action: 'click' });
    });

    it('should throw when no elements found for string instruction', async () => {
      // Mock observe to return empty array
      const originalObserve = browser.observe.bind(browser);
      browser.observe = vi.fn().mockResolvedValue([]);

      await expect(browser.act('click the button')).rejects.toThrow(
        'No elements found matching'
      );

      browser.observe = originalObserve;
    });
  });

  // ============================================
  // actWithSelector Method Dispatch Tests
  // ============================================

  describe('actWithSelector Method Dispatch', () => {
    beforeEach(async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
    });

    it('should handle click action', async () => {
      mockClient.click.mockResolvedValue(undefined);

      const result = await browser.actWithSelector({
        selector: '#btn',
        method: 'click',
        description: 'Click button',
      });

      expect(mockClient.click).toHaveBeenCalledWith({ selector: '#btn' });
      expect(result).toEqual({ success: true, action: 'click' });
    });

    it('should handle fill action with argument', async () => {
      mockClient.fill.mockResolvedValue(undefined);

      const result = await browser.actWithSelector({
        selector: '#input',
        method: 'fill',
        description: 'Fill input',
        arguments: ['test value'],
      });

      expect(mockClient.fill).toHaveBeenCalledWith({
        selector: '#input',
        value: 'test value',
      });
      expect(result).toEqual({ success: true, action: 'fill', text: 'test value' });
    });

    it('should handle type action with argument', async () => {
      mockClient.type.mockResolvedValue(undefined);

      const result = await browser.actWithSelector({
        selector: '#input',
        method: 'type',
        description: 'Type text',
        arguments: ['typed text'],
      });

      expect(mockClient.type).toHaveBeenCalledWith({
        selector: '#input',
        text: 'typed text',
      });
      expect(result).toEqual({ success: true, action: 'type', text: 'typed text' });
    });

    it('should throw when fill/type action missing argument', async () => {
      await expect(
        browser.actWithSelector({
          selector: '#input',
          method: 'fill',
          description: 'Fill input',
        })
      ).rejects.toThrow('fill action requires text argument');

      await expect(
        browser.actWithSelector({
          selector: '#input',
          method: 'type',
          description: 'Type text',
          arguments: [], // Empty array
        })
      ).rejects.toThrow('type action requires text argument');
    });

    it('should handle press action', async () => {
      mockClient.pressKey.mockResolvedValue(undefined);

      const result = await browser.actWithSelector({
        selector: '#input',
        method: 'press',
        description: 'Press key',
        arguments: ['Enter'],
      });

      expect(mockClient.pressKey).toHaveBeenCalledWith('Enter');
      expect(result).toEqual({ success: true, action: 'press', key: 'Enter' });
    });

    it('should throw when press action missing argument', async () => {
      await expect(
        browser.actWithSelector({
          selector: '#input',
          method: 'press',
          description: 'Press key',
        })
      ).rejects.toThrow('press action requires key argument');
    });

    it('should handle wait action with default ms', async () => {
      const result = await browser.actWithSelector({
        selector: '',
        method: 'wait',
        description: 'Wait',
      });

      expect(result).toEqual({ success: true, action: 'wait', ms: 1000 });
    });

    it('should handle wait action with custom ms', async () => {
      const result = await browser.actWithSelector({
        selector: '',
        method: 'wait',
        description: 'Wait custom',
        arguments: ['500'],
      });

      expect(result).toEqual({ success: true, action: 'wait', ms: 500 });
    });

    it('should throw for unsupported action method', async () => {
      await expect(
        browser.actWithSelector({
          selector: '#btn',
          method: 'unsupported' as any,
          description: 'Unsupported',
        })
      ).rejects.toThrow('Unsupported action method: unsupported');
    });
  });

  // ============================================
  // XPath Normalization Tests
  // ============================================

  describe('XPath Normalization', () => {
    beforeEach(async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
    });

    it('should remove xpath= prefix', async () => {
      mockClient.evaluate.mockResolvedValue({
        tagName: 'div',
        textContent: 'test',
      });

      await browser.getElementAttributesFromXPath('xpath=//div[@id="test"]');

      // Check that the script uses normalized xpath (without prefix)
      expect(mockClient.evaluate).toHaveBeenCalled();
      const call = mockClient.evaluate.mock.calls[0][0];
      expect(call.script).toContain('//div[@id="test"]');
      expect(call.script).not.toContain('xpath=');
    });

    it('should handle xpath without prefix', async () => {
      mockClient.evaluate.mockResolvedValue({
        tagName: 'div',
        textContent: 'test',
      });

      await browser.getElementAttributesFromXPath('//div[@id="test"]');

      expect(mockClient.evaluate).toHaveBeenCalled();
      const call = mockClient.evaluate.mock.calls[0][0];
      expect(call.script).toContain('//div[@id="test"]');
    });

    it('should return null when element not found', async () => {
      mockClient.evaluate.mockResolvedValue(null);

      const result = await browser.getElementAttributesFromXPath('//nonexistent');

      expect(result).toBeNull();
    });
  });

  // ============================================
  // parseObserveResponse Tests
  // ============================================

  describe('parseObserveResponse', () => {
    // Access private method for testing
    const callParseObserveResponse = (browser: AgentCoreBrowser, content: string) => {
      return (browser as any).parseObserveResponse(content);
    };

    it('should parse valid JSON array', () => {
      const content = `[
        {"selector": "xpath=//button", "description": "A button", "method": "click"}
      ]`;

      const result = callParseObserveResponse(browser, content);

      expect(result).toEqual([
        {
          selector: 'xpath=//button',
          description: 'A button',
          method: 'click',
          arguments: undefined,
        },
      ]);
    });

    it('should extract JSON from markdown code blocks', () => {
      const content = `Here are the elements:
\`\`\`json
[{"selector": "xpath=//a", "description": "A link", "method": "click"}]
\`\`\``;

      const result = callParseObserveResponse(browser, content);

      expect(result).toHaveLength(1);
      expect(result[0].selector).toBe('xpath=//a');
    });

    it('should extract JSON from code blocks without language tag', () => {
      const content = `\`\`\`
[{"selector": "xpath=//div", "description": "A div"}]
\`\`\``;

      const result = callParseObserveResponse(browser, content);

      expect(result).toHaveLength(1);
      expect(result[0].selector).toBe('xpath=//div');
    });

    it('should return empty array when no JSON found', () => {
      const content = 'No elements found on this page.';

      const result = callParseObserveResponse(browser, content);

      expect(result).toEqual([]);
    });

    it('should filter out invalid items', () => {
      const content = `[
        {"selector": "xpath=//valid", "description": "Valid item"},
        {"selector": null, "description": "Missing selector"},
        {"selector": "xpath=//also-valid", "description": "Also valid"},
        {"description": "No selector at all"},
        null
      ]`;

      const result = callParseObserveResponse(browser, content);

      expect(result).toHaveLength(2);
      expect(result[0].selector).toBe('xpath=//valid');
      expect(result[1].selector).toBe('xpath=//also-valid');
    });

    it('should default method to click', () => {
      const content = `[{"selector": "xpath=//btn", "description": "Button"}]`;

      const result = callParseObserveResponse(browser, content);

      expect(result[0].method).toBe('click');
    });

    it('should include arguments when present', () => {
      const content = `[{
        "selector": "xpath=//input",
        "description": "Input field",
        "method": "type",
        "arguments": ["hello"]
      }]`;

      const result = callParseObserveResponse(browser, content);

      expect(result[0].arguments).toEqual(['hello']);
    });

    it('should handle malformed JSON gracefully', () => {
      const content = `[{"selector": broken json`;

      const result = callParseObserveResponse(browser, content);

      expect(result).toEqual([]);
    });

    it('should handle non-array JSON', () => {
      const content = `{"selector": "xpath=//div", "description": "Single object"}`;

      const result = callParseObserveResponse(browser, content);

      expect(result).toEqual([]);
    });
  });

  // ============================================
  // buildObservePrompt Tests
  // ============================================

  describe('buildObservePrompt', () => {
    const callBuildObservePrompt = (browser: AgentCoreBrowser, instruction: string) => {
      return (browser as any).buildObservePrompt(instruction);
    };

    it('should include instruction in prompt', () => {
      const prompt = callBuildObservePrompt(browser, 'find the login button');

      expect(prompt).toContain('find the login button');
      // Instruction appears twice in the prompt
      expect(prompt.match(/find the login button/g)?.length).toBe(2);
    });

    it('should include XPath format requirement', () => {
      const prompt = callBuildObservePrompt(browser, 'test');

      expect(prompt).toContain('xpath=');
      expect(prompt).toContain('XPath');
    });

    it('should include JSON format example', () => {
      const prompt = callBuildObservePrompt(browser, 'test');

      expect(prompt).toContain('selector');
      expect(prompt).toContain('description');
      expect(prompt).toContain('method');
      expect(prompt).toContain('JSON array');
    });
  });

  // ============================================
  // Navigation URL Tracking Tests
  // ============================================

  describe('Navigation URL Tracking', () => {
    beforeEach(async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
      mockClient.navigate.mockResolvedValue(undefined);
    });

    it('should update currentUrl after navigate', async () => {
      await browser.navigate('https://example.com');

      expect(browser.getUrl()).toBe('https://example.com');
    });

    it('should update currentUrl on subsequent navigations', async () => {
      await browser.navigate('https://first.com');
      expect(browser.getUrl()).toBe('https://first.com');

      await browser.navigate('https://second.com');
      expect(browser.getUrl()).toBe('https://second.com');
    });
  });

  // ============================================
  // Token Stats Tests
  // ============================================

  describe('Token Stats', () => {
    it('should return undefined (Bedrock billing is separate)', () => {
      expect(browser.getTokenStats()).toBeUndefined();
    });
  });

  // ============================================
  // Close/Cleanup Tests
  // ============================================

  describe('Close/Cleanup', () => {
    it('should reset state after close', async () => {
      await browser.initialize();
      mockClient = (browser as any).client;
      mockClient.stopSession.mockResolvedValue(undefined);
      mockClient.navigate.mockResolvedValue(undefined);

      await browser.navigate('https://example.com');
      expect(browser.getUrl()).toBe('https://example.com');

      await browser.close();

      expect(browser.getUrl()).toBe('about:blank');
      expect((browser as any).initialized).toBe(false);
      expect((browser as any).client).toBeNull();
    });

    it('should handle close when not initialized', async () => {
      // Should not throw
      await browser.close();
    });
  });
});
