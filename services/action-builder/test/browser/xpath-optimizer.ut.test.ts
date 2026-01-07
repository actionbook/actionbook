/**
 * XPath Optimizer Unit Tests
 *
 * Tests for generateOptimizedXPath function that creates stable XPath selectors
 * based on element attributes with priority ordering.
 */

import { describe, it, expect } from 'vitest'

// Re-implement the function for testing (since it's not exported)
// This mirrors the implementation in StagehandBrowser.ts
function generateOptimizedXPath(
  attrs: {
    tagName: string;
    id?: string;
    dataTestId?: string;
    name?: string;
    ariaLabel?: string;
    className?: string;
    textContent?: string;
    placeholder?: string;
    dataAttributes?: Record<string, string>;
  },
  originalXPath: string
): { xpath: string; source: string } {
  const tag = attrs.tagName;

  // Priority 1: @id (most stable)
  if (attrs.id) {
    return {
      xpath: `//${tag}[@id="${attrs.id}"]`,
      source: "id",
    };
  }

  // Priority 1: @data-testid (most stable, designed for testing)
  if (attrs.dataTestId) {
    return {
      xpath: `//${tag}[@data-testid="${attrs.dataTestId}"]`,
      source: "data-testid",
    };
  }

  // Priority 1: Other stable data-* attributes
  if (attrs.dataAttributes) {
    const stableDataAttrs = ['data-id', 'data-component', 'data-element', 'data-action', 'data-section', 'data-name'];
    for (const attr of stableDataAttrs) {
      if (attrs.dataAttributes[attr]) {
        return {
          xpath: `//${tag}[@${attr}="${attrs.dataAttributes[attr]}"]`,
          source: attr,
        };
      }
    }
  }

  // Priority 2: @name (stable for form elements)
  if (attrs.name) {
    return {
      xpath: `//${tag}[@name="${attrs.name}"]`,
      source: "name",
    };
  }

  // Priority 2: @aria-label (stable, semantic)
  if (attrs.ariaLabel) {
    return {
      xpath: `//${tag}[@aria-label="${attrs.ariaLabel}"]`,
      source: "aria-label",
    };
  }

  // Priority 2: @placeholder (for inputs)
  if (attrs.placeholder) {
    return {
      xpath: `//${tag}[@placeholder="${attrs.placeholder}"]`,
      source: "placeholder",
    };
  }

  // Priority 3: @class (moderately stable, filter out hash-like classes)
  if (attrs.className) {
    const classes = attrs.className.split(" ").filter((c: string) => {
      if (!c) return false;
      // Keep BEM-style classes
      if (/^[a-z][a-z0-9]*(-[a-z0-9]+)*(__[a-z0-9]+(-[a-z0-9]+)*)?(--[a-z0-9]+(-[a-z0-9]+)*)?$/i.test(c)) {
        return true;
      }
      // Filter out hash-like classes
      if (/^[a-z]{1,3}-[a-zA-Z0-9]{4,}$/.test(c)) return false;
      if (/^[a-zA-Z]{2,}[A-Z][a-z]+$/.test(c)) return false;
      if (/[A-Z].*[A-Z]/.test(c) && c.length < 12) return false;
      return true;
    });

    if (classes.length > 0) {
      const bemClass = classes.find((c: string) => c.includes('__') || c.includes('--'));
      const selectedClass = bemClass || classes[0];
      return {
        xpath: `//${tag}[contains(@class, "${selectedClass}")]`,
        source: `class(${selectedClass})`,
      };
    }
  }

  // Priority 4: text() content
  if (attrs.textContent && attrs.textContent.length > 2 && attrs.textContent.length <= 30) {
    const escapedText = attrs.textContent.replace(/"/g, '\\"');
    return {
      xpath: `//${tag}[normalize-space()="${escapedText}"]`,
      source: "text",
    };
  }

  // Priority 5: Fallback to original absolute path
  return {
    xpath: originalXPath,
    source: "absolute-path",
  };
}

describe('generateOptimizedXPath', () => {
  const originalXPath = '/html[1]/body[1]/div[3]/button[1]';

  describe('Priority 1: @id and @data-testid (highest stability)', () => {
    it('UT-XPATH-01: generates XPath with @id when available', () => {
      const result = generateOptimizedXPath(
        { tagName: 'button', id: 'submit-btn' },
        originalXPath
      );
      expect(result.xpath).toBe('//button[@id="submit-btn"]');
      expect(result.source).toBe('id');
    });

    it('UT-XPATH-02: generates XPath with @data-testid when available', () => {
      const result = generateOptimizedXPath(
        { tagName: 'button', dataTestId: 'checkout-btn' },
        originalXPath
      );
      expect(result.xpath).toBe('//button[@data-testid="checkout-btn"]');
      expect(result.source).toBe('data-testid');
    });

    it('UT-XPATH-03: prefers @id over @data-testid', () => {
      const result = generateOptimizedXPath(
        { tagName: 'button', id: 'btn-1', dataTestId: 'checkout-btn' },
        originalXPath
      );
      expect(result.xpath).toBe('//button[@id="btn-1"]');
      expect(result.source).toBe('id');
    });

    it('UT-XPATH-04: generates XPath with data-id when available', () => {
      const result = generateOptimizedXPath(
        { tagName: 'div', dataAttributes: { 'data-id': 'card-123' } },
        originalXPath
      );
      expect(result.xpath).toBe('//div[@data-id="card-123"]');
      expect(result.source).toBe('data-id');
    });

    it('UT-XPATH-05: generates XPath with data-component when available', () => {
      const result = generateOptimizedXPath(
        { tagName: 'div', dataAttributes: { 'data-component': 'header' } },
        originalXPath
      );
      expect(result.xpath).toBe('//div[@data-component="header"]');
      expect(result.source).toBe('data-component');
    });
  });

  describe('Priority 2: @name, @aria-label, @placeholder', () => {
    it('UT-XPATH-06: generates XPath with @name for form elements', () => {
      const result = generateOptimizedXPath(
        { tagName: 'input', name: 'email' },
        originalXPath
      );
      expect(result.xpath).toBe('//input[@name="email"]');
      expect(result.source).toBe('name');
    });

    it('UT-XPATH-07: generates XPath with @aria-label', () => {
      const result = generateOptimizedXPath(
        { tagName: 'button', ariaLabel: 'Close dialog' },
        originalXPath
      );
      expect(result.xpath).toBe('//button[@aria-label="Close dialog"]');
      expect(result.source).toBe('aria-label');
    });

    it('UT-XPATH-08: generates XPath with @placeholder for inputs', () => {
      const result = generateOptimizedXPath(
        { tagName: 'input', placeholder: 'Enter your email' },
        originalXPath
      );
      expect(result.xpath).toBe('//input[@placeholder="Enter your email"]');
      expect(result.source).toBe('placeholder');
    });

    it('UT-XPATH-09: prefers @name over @aria-label', () => {
      const result = generateOptimizedXPath(
        { tagName: 'input', name: 'email', ariaLabel: 'Email input' },
        originalXPath
      );
      expect(result.xpath).toBe('//input[@name="email"]');
      expect(result.source).toBe('name');
    });
  });

  describe('Priority 3: @class', () => {
    it('UT-XPATH-10: generates XPath with class using contains()', () => {
      const result = generateOptimizedXPath(
        { tagName: 'button', className: 'btn primary' },
        originalXPath
      );
      expect(result.xpath).toBe('//button[contains(@class, "btn")]');
      expect(result.source).toBe('class(btn)');
    });

    it('UT-XPATH-11: prefers BEM-style classes', () => {
      const result = generateOptimizedXPath(
        { tagName: 'div', className: 'card card__header' },
        originalXPath
      );
      expect(result.xpath).toBe('//div[contains(@class, "card__header")]');
      expect(result.source).toBe('class(card__header)');
    });

    it('UT-XPATH-12: uses first valid class when multiple available', () => {
      // Multiple classes available, should use first valid one
      const result = generateOptimizedXPath(
        { tagName: 'div', className: 'container wrapper' },
        originalXPath
      );
      expect(result.xpath).toBe('//div[contains(@class, "container")]');
      expect(result.source).toBe('class(container)');
    });
  });

  describe('Priority 4: text content', () => {
    it('UT-XPATH-13: generates XPath with text content', () => {
      const result = generateOptimizedXPath(
        { tagName: 'button', textContent: 'Submit' },
        originalXPath
      );
      expect(result.xpath).toBe('//button[normalize-space()="Submit"]');
      expect(result.source).toBe('text');
    });

    it('UT-XPATH-14: ignores very short text content', () => {
      const result = generateOptimizedXPath(
        { tagName: 'span', textContent: 'OK' },
        originalXPath
      );
      // Too short, falls back to absolute path
      expect(result.xpath).toBe(originalXPath);
      expect(result.source).toBe('absolute-path');
    });

    it('UT-XPATH-15: ignores very long text content', () => {
      const result = generateOptimizedXPath(
        { tagName: 'p', textContent: 'This is a very long text content that exceeds the limit' },
        originalXPath
      );
      // Too long, falls back to absolute path
      expect(result.xpath).toBe(originalXPath);
      expect(result.source).toBe('absolute-path');
    });
  });

  describe('Priority 5: Fallback to absolute path', () => {
    it('UT-XPATH-16: falls back to absolute path when no attributes available', () => {
      const result = generateOptimizedXPath(
        { tagName: 'div' },
        originalXPath
      );
      expect(result.xpath).toBe(originalXPath);
      expect(result.source).toBe('absolute-path');
    });

    it('UT-XPATH-17: falls back when className is empty', () => {
      // Empty className after trim should fall back
      const result = generateOptimizedXPath(
        { tagName: 'div', className: '   ' },
        originalXPath
      );
      expect(result.xpath).toBe(originalXPath);
      expect(result.source).toBe('absolute-path');
    });
  });

  describe('Real-world examples', () => {
    it('UT-XPATH-18: arxiv.org category link with dot in ID', () => {
      const result = generateOptimizedXPath(
        { tagName: 'a', id: 'cs.AI' },
        '/html[1]/body[1]/div[2]/main[1]/ul[1]/li[5]/a[1]'
      );
      expect(result.xpath).toBe('//a[@id="cs.AI"]');
      expect(result.source).toBe('id');
    });

    it('UT-XPATH-19: form input with name attribute', () => {
      const result = generateOptimizedXPath(
        { tagName: 'input', name: 'search', placeholder: 'Search...' },
        '/html[1]/body[1]/header[1]/form[1]/input[1]'
      );
      expect(result.xpath).toBe('//input[@name="search"]');
      expect(result.source).toBe('name');
    });

    it('UT-XPATH-20: button with data-testid', () => {
      const result = generateOptimizedXPath(
        { tagName: 'button', className: 'btn btn--primary sc-xyz123', dataTestId: 'submit-form' },
        '/html[1]/body[1]/form[1]/div[3]/button[1]'
      );
      expect(result.xpath).toBe('//button[@data-testid="submit-form"]');
      expect(result.source).toBe('data-testid');
    });
  });
});
