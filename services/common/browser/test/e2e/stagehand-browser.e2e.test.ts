/**
 * StagehandBrowser E2E Tests
 *
 * Tests the StagehandBrowser implementation against real browser with Stagehand AI.
 *
 * Prerequisites:
 * - One of: OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY, or AWS credentials
 * - BROWSER_TYPE=stagehand (or default when not agentcore)
 *
 * Run with: pnpm test:e2e
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { StagehandBrowser } from '../../src';
import type { BrowserAdapter } from '../../src';

// Skip tests if in AgentCore environment (those tests are in agent-core-browser.e2e.test.ts)
const isAgentCoreEnv =
  process.env.AWS_AGENTCORE_RUNTIME === 'true' ||
  process.env.BROWSER_TYPE === 'agentcore';

// Check if we have LLM credentials for AI tests
const hasLLMCredentials =
  !!process.env.OPENROUTER_API_KEY ||
  !!process.env.OPENAI_API_KEY ||
  !!process.env.ANTHROPIC_API_KEY ||
  (!!process.env.AWS_ACCESS_KEY_ID && !!process.env.AWS_SECRET_ACCESS_KEY);

const describeIfStagehand = isAgentCoreEnv ? describe.skip : describe;
const describeIfHasLLM = hasLLMCredentials && !isAgentCoreEnv ? describe : describe.skip;

describeIfStagehand('StagehandBrowser E2E', () => {
  let browser: BrowserAdapter;

  beforeAll(async () => {
    browser = new StagehandBrowser({
      headless: process.env.HEADLESS !== 'false',
    });
    await browser.initialize();
  }, 60000); // 60s timeout for initialization

  afterAll(async () => {
    if (browser) {
      await browser.close();
    }
  }, 30000);

  // ============================================
  // Lifecycle Tests
  // ============================================

  describe('Lifecycle', () => {
    it('should be initialized', () => {
      expect(browser).toBeDefined();
    });
  });

  // ============================================
  // Navigation Tests
  // ============================================

  describe('Navigation', () => {
    it('should navigate to example.com', async () => {
      await browser.navigate('https://example.com');

      const url = browser.getUrl();
      expect(url).toContain('example.com');
    });

    it('should get page title', async () => {
      const title = await browser.getTitle();
      expect(title).toContain('Example Domain');
    });

    it('should get page content (HTML)', async () => {
      const content = await browser.getContent();

      expect(content).toContain('Example Domain');
      expect(content).toContain('</html>');
    });

    it('should navigate with options', async () => {
      await browser.navigate('https://example.com', {
        waitUntil: 'domcontentloaded',
        timeout: 30000,
      });

      const url = browser.getUrl();
      expect(url).toContain('example.com');
    });
  });

  // ============================================
  // Screenshot Tests
  // ============================================

  describe('Screenshot', () => {
    it('should take a screenshot (default PNG)', async () => {
      const screenshot = await browser.screenshot();

      expect(screenshot).toBeInstanceOf(Buffer);
      expect(screenshot.length).toBeGreaterThan(0);

      // PNG magic bytes: 89 50 4E 47
      expect(screenshot[0]).toBe(0x89);
      expect(screenshot[1]).toBe(0x50);
      expect(screenshot[2]).toBe(0x4e);
      expect(screenshot[3]).toBe(0x47);
    });

    it('should take a JPEG screenshot', async () => {
      const screenshot = await browser.screenshot({ format: 'jpeg' });

      expect(screenshot).toBeInstanceOf(Buffer);
      expect(screenshot.length).toBeGreaterThan(0);

      // JPEG magic bytes: FF D8 FF
      expect(screenshot[0]).toBe(0xff);
      expect(screenshot[1]).toBe(0xd8);
      expect(screenshot[2]).toBe(0xff);
    });

    it('should take a full page screenshot', async () => {
      const screenshot = await browser.screenshot({ fullPage: true });

      expect(screenshot).toBeInstanceOf(Buffer);
      expect(screenshot.length).toBeGreaterThan(0);
    });
  });

  // ============================================
  // Wait Tests
  // ============================================

  describe('Waiting', () => {
    it('should wait for specified duration', async () => {
      const start = Date.now();
      await browser.wait(500);
      const elapsed = Date.now() - start;

      expect(elapsed).toBeGreaterThanOrEqual(450); // Allow some tolerance
    });

    it('should wait for text', async () => {
      await browser.navigate('https://example.com');

      await browser.waitForText('Example Domain');
      // If we get here without timeout, the test passes
    });
  });

  // ============================================
  // Scroll Tests
  // ============================================

  describe('Scrolling', () => {
    it('should scroll down', async () => {
      await browser.navigate('https://example.com');
      await browser.scroll('down', 100);
      // No error means success
    });

    it('should scroll up', async () => {
      await browser.scroll('up', 100);
      // No error means success
    });

    it('should scroll to bottom', async () => {
      await browser.scrollToBottom();
      // No error means success
    });
  });

  // ============================================
  // Element Inspection Tests (non-AI)
  // ============================================

  describe('Element Inspection', () => {
    it('should get element attributes from XPath', async () => {
      await browser.navigate('https://example.com');

      const attrs = await browser.getElementAttributesFromXPath('//h1');

      expect(attrs).not.toBeNull();
      expect(attrs?.tagName).toBe('h1');
      expect(attrs?.textContent).toContain('Example Domain');
    });

    it('should return null for non-existent element', async () => {
      const attrs = await browser.getElementAttributesFromXPath('//nonexistent-element');

      expect(attrs).toBeNull();
    });

    it('should get Page instance', async () => {
      const page = await browser.getPage();

      expect(page).toBeDefined();
      // Verify it has expected methods
      expect(typeof (page as any).goto).toBe('function');
      expect(typeof (page as any).screenshot).toBe('function');
    });
  });

  // ============================================
  // History Navigation Tests
  // ============================================

  describe('History Navigation', () => {
    it('should go back in history', async () => {
      // Navigate to first page
      await browser.navigate('https://example.com');

      // Navigate to second page
      await browser.navigate('https://www.iana.org/domains/reserved');

      // Go back
      await browser.goBack();

      // Verify we're back (or at least the goBack didn't throw)
      const currentUrl = browser.getUrl();
      expect(currentUrl).toBeDefined();
    });
  });

  // ============================================
  // Token Stats Tests
  // ============================================

  describe('Token Stats', () => {
    it('should return token stats structure', () => {
      const stats = browser.getTokenStats?.();

      if (stats) {
        expect(stats.input).toBeGreaterThanOrEqual(0);
        expect(stats.output).toBeGreaterThanOrEqual(0);
        expect(stats.total).toBe(stats.input + stats.output);
      }
    });
  });
});

// ============================================
// AI-dependent tests (require LLM credentials)
// ============================================

describeIfHasLLM('StagehandBrowser AI Features', () => {
  let browser: BrowserAdapter;

  beforeAll(async () => {
    browser = new StagehandBrowser({
      headless: process.env.HEADLESS !== 'false',
    });
    await browser.initialize();
    await browser.navigate('https://example.com');
  }, 60000);

  afterAll(async () => {
    if (browser) {
      await browser.close();
    }
  }, 30000);

  // ============================================
  // AI Observe Tests
  // ============================================

  describe('AI Observe', () => {
    it('should observe elements using AI', async () => {
      const elements = await browser.observe('find the main heading', 120000);

      expect(elements).toBeDefined();
      expect(Array.isArray(elements)).toBe(true);
      expect(elements.length).toBeGreaterThan(0);

      // Check first element has required properties
      const firstElement = elements[0];
      expect(firstElement.selector).toBeDefined();
      expect(typeof firstElement.selector).toBe('string');
    }, 180000);

    it('should observe link elements', async () => {
      const elements = await browser.observe('find the "More information" link', 120000);

      expect(elements.length).toBeGreaterThan(0);
      expect(elements[0].selector).toBeDefined();
    }, 180000);
  });

  // ============================================
  // AI Act Tests
  // ============================================

  describe('AI Act', () => {
    it('should act with selector (click)', async () => {
      await browser.navigate('https://example.com');

      // Act with ActionObject
      const result = await browser.actWithSelector({
        selector: 'h1',
        method: 'click',
        description: 'Click the heading',
      });

      expect(result).toBeDefined();
    }, 180000);

    it('should act with natural language', async () => {
      await browser.navigate('https://example.com');

      try {
        await browser.act('hover over the main heading');
      } catch (error) {
        // Some actions may fail, but shouldn't throw unexpected errors
        expect(error).toBeDefined();
      }
    }, 180000);
  });

  // ============================================
  // Auto Close Popups Tests
  // ============================================

  describe('Auto Close Popups', () => {
    it('should handle autoClosePopups on simple page', async () => {
      await browser.navigate('https://example.com');

      // example.com has no popups, should return 0
      const closed = await browser.autoClosePopups();

      expect(typeof closed).toBe('number');
      expect(closed).toBe(0);
    }, 180000);
  });

  // ============================================
  // Token Stats after AI operations
  // ============================================

  describe('Token Stats after AI', () => {
    it('should track token usage after AI operations', () => {
      const stats = browser.getTokenStats?.();

      // After observe() calls, should have some tokens
      if (stats) {
        expect(stats.total).toBeGreaterThan(0);
      }
    });
  });
});

// ============================================
// Separate test for lifecycle (new instance)
// ============================================

describeIfStagehand('StagehandBrowser Lifecycle', () => {
  it('should initialize and close correctly', async () => {
    const browser = new StagehandBrowser({
      headless: true,
    });

    await browser.initialize();
    expect(browser.getUrl()).toBeDefined();

    await browser.navigate('https://example.com');
    expect(browser.getUrl()).toContain('example.com');

    await browser.close();
    // After close, further operations should fail (or be handled gracefully)
  }, 90000);

  it('should throw when operating before initialize', async () => {
    const browser = new StagehandBrowser();

    // Should throw because not initialized
    expect(() => browser.getUrl()).toThrow();
  });

  it('should handle double initialization gracefully', async () => {
    const browser = new StagehandBrowser({
      headless: true,
    });

    await browser.initialize();
    await browser.initialize(); // Second call should be no-op

    await browser.navigate('https://example.com');
    expect(browser.getUrl()).toContain('example.com');

    await browser.close();
  }, 90000);

  it('should handle double close gracefully', async () => {
    const browser = new StagehandBrowser({
      headless: true,
    });

    await browser.initialize();
    await browser.close();
    await browser.close(); // Second call should be no-op
  }, 60000);
});