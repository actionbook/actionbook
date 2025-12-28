/**
 * Test script for SelectorOptimizer
 * Run: npx tsx test/selector-optimizer.test.ts
 */

import 'dotenv/config';
import { SelectorOptimizer } from '../src/optimizer/SelectorOptimizer.js';
import type { ElementCapability } from '../src/types/capability.js';

// Test data with various selector patterns
const testElements: Map<string, ElementCapability> = new Map([
  // Case 1: Framework-generated ID (should be marked unstable)
  ['ember_id_element', {
    id: 'ember_id_element',
    description: 'A button with Ember.js generated ID',
    element_type: 'button',
    selectors: [
      { type: 'id', value: '#ember49', priority: 1, confidence: 0.95 },
      { type: 'css', value: '.btn-primary', priority: 2, confidence: 0.75 },
      { type: 'xpath', value: '//button[@class="btn-primary"]', priority: 3, confidence: 0.6 },
    ],
    allow_methods: ['click'],
  }],

  // Case 2: Dynamic counter in aria-label (should be marked unstable)
  ['notification_element', {
    id: 'notification_element',
    description: 'Notification badge with dynamic counter',
    element_type: 'button',
    selectors: [
      { type: 'aria-label', value: '[aria-label="消息，0 条新通知"]', priority: 1, confidence: 0.85 },
      { type: 'css', value: '[data-view-name="navigation-messaging"]', priority: 2, confidence: 0.8 },
      { type: 'xpath', value: '//button[contains(@aria-label, "消息")]', priority: 3, confidence: 0.6 },
    ],
    allow_methods: ['click'],
  }],

  // Case 3: Good stable selectors (should pick data-testid)
  ['stable_element', {
    id: 'stable_element',
    description: 'A well-designed element with stable selectors',
    element_type: 'button',
    selectors: [
      { type: 'css', value: '.some-class', priority: 1, confidence: 0.7 },
      { type: 'data-testid', value: '[data-testid="submit-button"]', priority: 2, confidence: 0.9 },
      { type: 'xpath', value: '//button[@type="submit"]', priority: 3, confidence: 0.6 },
    ],
    allow_methods: ['click'],
  }],

  // Case 4: React generated ID (should be marked unstable)
  ['react_id_element', {
    id: 'react_id_element',
    description: 'Element with React useId generated ID',
    element_type: 'input',
    selectors: [
      { type: 'id', value: '#:r5:', priority: 1, confidence: 0.95 },
      { type: 'css', value: '.search-input', priority: 2, confidence: 0.75 },
      { type: 'aria-label', value: '[aria-label="Search"]', priority: 3, confidence: 0.85 },
    ],
    allow_methods: ['click', 'type'],
  }],

  // Case 5: Timestamp in selector (should be marked unstable)
  ['timestamp_element', {
    id: 'timestamp_element',
    description: 'Post with timestamp',
    element_type: 'link',
    selectors: [
      { type: 'aria-label', value: '[aria-label="Posted 2 minutes ago"]', priority: 1, confidence: 0.85 },
      { type: 'css', value: '.post-link', priority: 2, confidence: 0.75 },
      { type: 'xpath', value: '//a[@class="post-link"]', priority: 3, confidence: 0.6 },
    ],
    allow_methods: ['click'],
  }],

  // Case 6: BEM class name (should be stable)
  ['bem_element', {
    id: 'bem_element',
    description: 'Element with BEM class naming',
    element_type: 'button',
    selectors: [
      { type: 'css', value: '.header__nav-item--active', priority: 1, confidence: 0.8 },
      { type: 'xpath', value: '//button[contains(@class, "header__nav-item")]', priority: 2, confidence: 0.6 },
    ],
    allow_methods: ['click'],
  }],
]);

async function runTest() {
  console.log('='.repeat(80));
  console.log('SelectorOptimizer Test');
  console.log('='.repeat(80));
  console.log();

  try {
    const optimizer = new SelectorOptimizer();

    console.log(`Testing with ${testElements.size} elements...\n`);

    const result = await optimizer.optimizeSelectors(testElements);

    console.log('\n' + '='.repeat(80));
    console.log('Results');
    console.log('='.repeat(80));
    console.log();

    if (!result.success) {
      console.error('Optimization failed:', result.error);
      return;
    }

    console.log(`Optimized: ${result.optimizedCount}/${result.totalElements} elements\n`);

    for (const element of result.elements) {
      console.log(`--- ${element.elementId} ---`);
      console.log(`Reason: ${element.reason}`);
      console.log('Original selectors:');
      for (const sel of element.originalSelectors) {
        console.log(`  [${sel.priority}] ${sel.type}: ${sel.value} (confidence: ${sel.confidence})`);
      }
      console.log('Optimized selectors:');
      for (const sel of element.optimizedSelectors) {
        console.log(`  [${sel.priority}] ${sel.type}: ${sel.value} (confidence: ${sel.confidence})`);
      }
      console.log();
    }

    // Summary check
    console.log('='.repeat(80));
    console.log('Expected Results Check');
    console.log('='.repeat(80));

    const checks = [
      {
        id: 'ember_id_element',
        expected: 'Best selector should be .btn-primary, #ember49 should have low confidence',
        check: (el: typeof result.elements[0]) => {
          const first = el.optimizedSelectors[0];
          const emberSelector = el.optimizedSelectors.find(s => s.value === '#ember49');
          const bestOk = first.value === '.btn-primary';
          const confidenceOk = emberSelector && emberSelector.confidence <= 0.1;
          return bestOk && confidenceOk;
        }
      },
      {
        id: 'notification_element',
        expected: 'Best selector should be data-view-name, counter aria-label should have low confidence',
        check: (el: typeof result.elements[0]) => {
          const first = el.optimizedSelectors[0];
          const counterSelector = el.optimizedSelectors.find(s => s.value.includes('0 条新通知'));
          const bestOk = first.value.includes('data-view-name');
          const confidenceOk = counterSelector && counterSelector.confidence <= 0.1;
          return bestOk && confidenceOk;
        }
      },
      {
        id: 'stable_element',
        expected: 'Should select data-testid as best (all stable)',
        check: (el: typeof result.elements[0]) => {
          const first = el.optimizedSelectors[0];
          return first.value.includes('data-testid');
        }
      },
      {
        id: 'react_id_element',
        expected: 'Best selector should be aria-label="Search", :r5: should have low confidence',
        check: (el: typeof result.elements[0]) => {
          const first = el.optimizedSelectors[0];
          const reactIdSelector = el.optimizedSelectors.find(s => s.value.includes(':r5:'));
          const bestOk = first.value.includes('aria-label="Search"');
          const confidenceOk = reactIdSelector && reactIdSelector.confidence <= 0.1;
          return bestOk && confidenceOk;
        }
      },
      {
        id: 'timestamp_element',
        expected: 'Best selector should be .post-link, timestamp aria-label should have low confidence',
        check: (el: typeof result.elements[0]) => {
          const first = el.optimizedSelectors[0];
          const timestampSelector = el.optimizedSelectors.find(s => s.value.includes('minutes ago'));
          const bestOk = first.value === '.post-link';
          const confidenceOk = timestampSelector && timestampSelector.confidence <= 0.1;
          return bestOk && confidenceOk;
        }
      },
      {
        id: 'bem_element',
        expected: 'Should keep BEM class as stable (no unstable selectors)',
        check: (el: typeof result.elements[0]) => {
          const first = el.optimizedSelectors[0];
          // All selectors should have reasonable confidence (none marked as unstable)
          const allStable = el.optimizedSelectors.every(s => s.confidence > 0.1);
          return first.value.includes('header__nav-item') && allStable;
        }
      },
    ];

    let passed = 0;
    let failed = 0;

    for (const check of checks) {
      const element = result.elements.find(e => e.elementId === check.id);
      if (!element) {
        console.log(`[SKIP] ${check.id}: Element not found in results`);
        continue;
      }

      const result_ok = check.check(element);
      if (result_ok) {
        console.log(`[PASS] ${check.id}: ${check.expected}`);
        passed++;
      } else {
        console.log(`[FAIL] ${check.id}: ${check.expected}`);
        console.log(`       Got: ${element.optimizedSelectors[0].value}`);
        failed++;
      }
    }

    console.log();
    console.log(`Summary: ${passed} passed, ${failed} failed`);

  } catch (error) {
    console.error('Test error:', error);
  }
}

runTest();