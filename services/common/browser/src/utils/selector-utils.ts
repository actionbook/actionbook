/**
 * Selector utilities for browser automation
 */

/**
 * State-related data attributes that should be filtered out.
 * These attributes change based on user interaction and are not stable selectors.
 */
const STATE_DATA_ATTRS = new Set([
  'data-state',
  'data-checked',
  'data-disabled',
  'data-active',
  'data-selected',
  'data-expanded',
  'data-open',
  'data-closed',
  'data-focus',
  'data-focus-visible',
  'data-hover',
  'data-pressed',
  'data-visible',
  'data-hidden',
  'data-loading',
  'data-readonly',
  'data-invalid',
  'data-valid',
  'data-highlighted',
  'data-orientation',
]);

/**
 * State-related values that indicate the attribute is a state attribute.
 */
const STATE_VALUES = new Set([
  'open',
  'closed',
  'on',
  'off',
  'true',
  'false',
  'active',
  'inactive',
  'enabled',
  'disabled',
  'visible',
  'hidden',
  'expanded',
  'collapsed',
  'checked',
  'unchecked',
  'selected',
  'unselected',
  'pressed',
  'unpressed',
  'valid',
  'invalid',
  'loading',
  'loaded',
  'horizontal',
  'vertical',
]);

/**
 * Filter out state-related data attributes that are not stable for selectors.
 */
export function filterStateDataAttributes(
  dataAttributes: Record<string, string> | undefined
): Record<string, string> | undefined {
  if (!dataAttributes) return undefined;

  const filtered: Record<string, string> = {};
  for (const [name, value] of Object.entries(dataAttributes)) {
    // Skip if attribute name is a known state attribute
    if (STATE_DATA_ATTRS.has(name)) {
      continue;
    }
    // Skip if value is a known state value
    if (STATE_VALUES.has(value.toLowerCase())) {
      continue;
    }
    filtered[name] = value;
  }

  return Object.keys(filtered).length > 0 ? filtered : undefined;
}

/**
 * Create a CSS ID selector, handling special characters
 * @param id - Element ID
 * @returns CSS selector string
 */
export function createIdSelector(id: string): string {
  // If ID contains special characters (dots, colons, etc.), use attribute selector
  if (/[.:#\[\]()>+~\s]/.test(id)) {
    return `[id="${id}"]`;
  }
  return `#${id}`;
}

/**
 * Generate an optimized XPath selector based on element attributes.
 * Priority (highest to lowest stability):
 * 1. @id / @data-testid - Most stable, unique identifiers
 * 2. @name / @aria-label - Stable, semantic attributes
 * 3. @class + tag - Moderately stable
 * 4. text() content - Can change with i18n
 * 5. Fallback to original absolute path - Least stable
 */
export function generateOptimizedXPath(
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

  // Priority 1: @id (most stable, unique)
  if (attrs.id) {
    return {
      xpath: `//${tag}[@id="${attrs.id}"]`,
      source: 'id',
    };
  }

  // Priority 1: @data-testid (most stable, designed for testing)
  if (attrs.dataTestId) {
    return {
      xpath: `//${tag}[@data-testid="${attrs.dataTestId}"]`,
      source: 'data-testid',
    };
  }

  // Priority 1: Other stable data-* attributes
  if (attrs.dataAttributes) {
    const stableDataAttrs = [
      'data-id',
      'data-component',
      'data-element',
      'data-action',
      'data-section',
      'data-name',
    ];
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
      source: 'name',
    };
  }

  // Priority 2: @aria-label (stable, semantic)
  if (attrs.ariaLabel) {
    return {
      xpath: `//${tag}[@aria-label="${attrs.ariaLabel}"]`,
      source: 'aria-label',
    };
  }

  // Priority 2: @placeholder (for inputs)
  if (attrs.placeholder) {
    return {
      xpath: `//${tag}[@placeholder="${attrs.placeholder}"]`,
      source: 'placeholder',
    };
  }

  // Priority 3: @class (moderately stable)
  if (attrs.className) {
    const classes = attrs.className.split(' ').filter((c: string) => {
      if (!c) return false;
      // Keep BEM-style classes
      if (
        /^[a-z][a-z0-9]*(-[a-z0-9]+)*(__[a-z0-9]+(-[a-z0-9]+)*)?(--[a-z0-9]+(-[a-z0-9]+)*)?$/i.test(
          c
        )
      ) {
        return true;
      }
      // Filter out hash-like classes
      if (/^[a-z]{1,3}-[a-zA-Z0-9]{4,}$/.test(c)) return false;
      if (/^[a-zA-Z]{2,}[A-Z][a-z]+$/.test(c)) return false;
      if (/[A-Z].*[A-Z]/.test(c) && c.length < 12) return false;
      return true;
    });

    if (classes.length > 0) {
      const bemClass = classes.find(
        (c: string) => c.includes('__') || c.includes('--')
      );
      const selectedClass = bemClass || classes[0];
      return {
        xpath: `//${tag}[contains(@class, "${selectedClass}")]`,
        source: `class(${selectedClass})`,
      };
    }
  }

  // Priority 4: text() content
  if (
    attrs.textContent &&
    attrs.textContent.length > 2 &&
    attrs.textContent.length <= 30
  ) {
    const escapedText = attrs.textContent.replace(/"/g, '\\"');
    return {
      xpath: `//${tag}[normalize-space()="${escapedText}"]`,
      source: 'text',
    };
  }

  // Priority 5: Fallback to original absolute path
  return {
    xpath: originalXPath,
    source: 'absolute-path',
  };
}

/**
 * Filter CSS classes, keeping only stable ones
 */
export function filterCssClasses(className: string): string[] {
  return className.split(' ').filter((c: string) => {
    if (!c) return false;
    // Keep BEM-style classes
    if (
      /^[a-z][a-z0-9]*(-[a-z0-9]+)*(__[a-z0-9]+(-[a-z0-9]+)*)?(--[a-z0-9]+(-[a-z0-9]+)*)?$/i.test(
        c
      )
    ) {
      return true;
    }
    // Filter out hash-like classes
    if (/^[a-z]{1,3}-[a-zA-Z0-9]{4,}$/.test(c)) return false;
    if (/^[a-zA-Z]{2,}[A-Z][a-z]+$/.test(c)) return false;
    if (/[A-Z].*[A-Z]/.test(c) && c.length < 12) return false;
    return true;
  });
}
