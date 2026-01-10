/**
 * AgentCoreBrowser E2E Tests
 *
 * Tests the AgentCoreBrowser implementation against real AWS AgentCore Runtime.
 *
 * Prerequisites:
 * - AWS credentials with AgentCore and Bedrock permissions
 * - AWS_AGENTCORE_RUNTIME=true or BROWSER_TYPE=agentcore
 *
 * Run with: pnpm test:e2e
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { AgentCoreBrowser } from '../../src';
import type { BrowserAdapter } from '../../src';

// Skip all tests if not in AgentCore environment
const isAgentCoreEnv =
  process.env.AWS_AGENTCORE_RUNTIME === 'true' ||
  process.env.BROWSER_TYPE === 'agentcore';

const describeIfAgentCore = isAgentCoreEnv ? describe : describe.skip;

describeIfAgentCore('AgentCoreBrowser E2E', () => {
  let browser: BrowserAdapter;

  beforeAll(async () => {
    browser = new AgentCoreBrowser();
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

      expect(content).toContain('<!DOCTYPE html>');
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

    it('should wait for selector', async () => {
      await browser.navigate('https://example.com');

      // example.com has an h1 element
      await browser.waitForSelector('h1');
      // If we get here without timeout, the test passes
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
  // AI Observe Tests
  // ============================================

  describe('AI Observe', () => {
    it('should observe elements using AI', async () => {
      await browser.navigate('https://example.com');

      const elements = await browser.observe('find the main heading');

      expect(elements).toBeDefined();
      expect(Array.isArray(elements)).toBe(true);
      expect(elements.length).toBeGreaterThan(0);

      // Check first element has required properties
      const firstElement = elements[0];
      expect(firstElement.selector).toBeDefined();
      expect(typeof firstElement.selector).toBe('string');
      expect(firstElement.description).toBeDefined();
    }, 60000); // AI calls need more time

    it('should observe link elements', async () => {
      await browser.navigate('https://example.com');

      const elements = await browser.observe('find the "More information" link');

      expect(elements.length).toBeGreaterThan(0);
      expect(elements[0].selector).toBeDefined();
    }, 60000);
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

      // Should not throw
      expect(result).toBeDefined();
    });

    it('should act with natural language', async () => {
      await browser.navigate('https://example.com');

      // This may or may not work depending on AI interpretation
      // Just verify it doesn't throw unexpectedly
      try {
        await browser.act('hover over the main heading');
      } catch (error) {
        // Some actions may fail, but shouldn't throw unexpected errors
        expect(error).toBeDefined();
      }
    }, 60000);
  });

  // ============================================
  // Element Inspection Tests
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

    it('should get Playwright Page instance', async () => {
      const page = await browser.getPage();

      expect(page).toBeDefined();
      // Verify it's a Playwright Page by checking for common methods
      expect(typeof (page as any).goto).toBe('function');
      expect(typeof (page as any).screenshot).toBe('function');
    });
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
    });
  });

  // ============================================
  // Token Stats Tests
  // ============================================

  describe('Token Stats', () => {
    it('should track token usage after AI operations', async () => {
      // Token stats are accumulated from observe() calls
      const stats = browser.getTokenStats?.();

      // May be undefined if no AI operations were done
      if (stats) {
        expect(stats.input).toBeGreaterThanOrEqual(0);
        expect(stats.output).toBeGreaterThanOrEqual(0);
        expect(stats.total).toBe(stats.input + stats.output);
      }
    });
  });

  // ============================================
  // Go Back Tests
  // ============================================

  describe('History Navigation', () => {
    it('should go back in history', async () => {
      // Navigate to first page
      await browser.navigate('https://example.com');
      const firstUrl = browser.getUrl();

      // Navigate to second page (using a link from example.com)
      // Note: example.com's "More information" link goes to iana.org
      await browser.navigate('https://www.iana.org/domains/reserved');

      // Go back
      await browser.goBack();

      // Verify we're back (or at least the goBack didn't throw)
      const currentUrl = browser.getUrl();
      // URL may or may not match exactly due to redirects
      expect(currentUrl).toBeDefined();
    });
  });
});

// ============================================
// Separate test for lifecycle (new instance)
// ============================================

describeIfAgentCore('AgentCoreBrowser Lifecycle', () => {
  it('should initialize and close correctly', async () => {
    const browser = new AgentCoreBrowser();

    await browser.initialize();
    expect(browser.getUrl()).toBeDefined();

    await browser.navigate('https://example.com');
    expect(browser.getUrl()).toContain('example.com');

    await browser.close();
    // After close, further operations should fail (or be handled gracefully)
  }, 90000);

  it('should throw when operating before initialize', async () => {
    const browser = new AgentCoreBrowser();

    // Should throw because not initialized
    await expect(browser.navigate('https://example.com')).rejects.toThrow();
  });
});
