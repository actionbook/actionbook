/**
 * Robustness Scorer
 *
 * Measures how well recorded selectors work across different environments.
 * Tests selector validity in various viewport sizes and locales.
 *
 * Key changes from original design:
 * - Only tests elements that matched in Recall phase (matched golden elements)
 * - Executes pre_actions using golden ref_selectors before validation
 * - Uses relaxed matching (A ⊆ B)
 *
 * Robustness = Σ Valid(e, env) / (|E_matched| × |Envs|)
 *
 * Valid(e, env) = 1 if:
 * - Selector finds at least one element
 * - Element is visible
 * - Reference element is among the found elements (relaxed matching)
 */

import { chromium, type Browser, type Page, type Locator } from "playwright";
import type { SiteCapability, TestEnv, EvalInput, EvalOutput, GoldenElement, RecallScoreResult, PreActionStep } from "../types.js";
import { getRecordedElements } from "../tasks/offline_eval.js";
import { TEST_ENVIRONMENTS, getEnvironments, DEFAULT_ENV } from "../utils/env_matrix.js";

export interface ElementEnvResult {
  /** Environment ID */
  envId: string;
  /** Whether selector is valid in this environment */
  valid: boolean;
  /** Whether element was found */
  found: boolean;
  /** Whether element is unique */
  unique: boolean;
  /** Whether element is visible */
  visible: boolean;
  /** Number of elements found */
  count: number;
  /** Error message if any */
  error?: string;
}

export interface ElementRobustnessResult {
  /** Element ID */
  elementId: string;
  /** Results for each environment */
  envResults: ElementEnvResult[];
  /** Element robustness score (0-1) */
  score: number;
}

export interface RobustnessScoreResult {
  /** Overall robustness score (0-1) */
  score: number;
  /** Number of valid element-env combinations */
  validCount: number;
  /** Total element-env combinations tested */
  totalCount: number;
  /** Per-element results */
  elementResults: ElementRobustnessResult[];
  /** Environments tested */
  environments: string[];
}

export interface RobustnessScorerOptions {
  /** Environment IDs to test (default: ["desktop_en"]) */
  envIds?: string[];
  /** Run browser in headless mode */
  headless?: boolean;
  /** Enable verbose logging */
  verbose?: boolean;
  /** Timeout for element operations (ms) */
  timeout?: number;
}

// Global options
let globalOptions: RobustnessScorerOptions = {
  envIds: ["desktop_en"],
  headless: true,
  verbose: false,
  timeout: 5000,
};

/**
 * Set global robustness scorer options
 */
export function setRobustnessScorerOptions(options: RobustnessScorerOptions): void {
  globalOptions = { ...globalOptions, ...options };
}

/**
 * Validate a single selector in a specific environment
 */
async function validateSelectorInEnv(
  page: Page,
  selectors: Array<{ type: string; value: string }>,
  timeout: number
): Promise<{ found: boolean; unique: boolean; visible: boolean; count: number }> {
  for (const sel of selectors) {
    try {
      let locator;
      switch (sel.type) {
        case "xpath":
          locator = page.locator(`xpath=${sel.value}`);
          break;
        case "text":
          locator = page.locator(`text=${sel.value}`);
          break;
        default:
          locator = page.locator(sel.value);
      }

      const count = await locator.count();
      if (count > 0) {
        const isVisible = await locator.first().isVisible({ timeout });
        return {
          found: true,
          unique: count === 1,
          visible: isVisible,
          count,
        };
      }
    } catch {
      // Try next selector
      continue;
    }
  }

  return { found: false, unique: false, visible: false, count: 0 };
}

/**
 * Calculate robustness score with multi-environment validation
 *
 * Only tests elements that matched in Recall phase (when goldenElements and recallResult are provided).
 * Executes pre_actions using golden ref_selectors before validation.
 *
 * @param capability - Recorded capability
 * @param url - URL to test
 * @param options - Scorer options
 * @param goldenElements - Golden elements (for filtering and pre_actions)
 * @param recallResult - Recall result (for filtering to matched elements only)
 */
export async function calculateRobustness(
  capability: SiteCapability | null,
  url: string,
  options?: RobustnessScorerOptions,
  goldenElements?: GoldenElement[],
  recallResult?: RecallScoreResult
): Promise<RobustnessScoreResult> {
  const opts = { ...globalOptions, ...options };
  const envs = getEnvironments(opts.envIds);

  if (!capability) {
    return {
      score: 0,
      validCount: 0,
      totalCount: 0,
      elementResults: [],
      environments: envs.map((e) => e.id),
    };
  }

  const recordedElements = getRecordedElements(capability);

  // Filter to matched elements only (if recallResult is provided)
  const matchedElementIds = recallResult
    ? new Set(recallResult.details.filter((d) => d.matched).map((d) => d.goldenId))
    : null;

  // Create a map of golden elements for quick lookup
  const goldenMap = goldenElements
    ? new Map(goldenElements.map((g) => [g.id, g]))
    : null;

  // Determine which elements to test
  const elementsToTest: Array<{
    elementId: string;
    selectors: Array<{ type: string; value: string }>;
    preActions?: PreActionStep[];
    refSelector?: string;
  }> = [];

  if (matchedElementIds && goldenMap) {
    // Only test matched elements, using golden info
    for (const elementId of matchedElementIds) {
      const recorded = recordedElements.get(elementId);
      const golden = goldenMap.get(elementId);
      if (recorded) {
        elementsToTest.push({
          elementId,
          selectors: recorded.selectors as Array<{ type: string; value: string }>,
          preActions: golden?.pre_actions,
          refSelector: golden?.ref_selector,
        });
      }
    }
  } else {
    // Fallback: test all recorded elements (legacy behavior)
    for (const [elementId, elementData] of recordedElements) {
      elementsToTest.push({
        elementId,
        selectors: elementData.selectors as Array<{ type: string; value: string }>,
      });
    }
  }

  const totalElements = elementsToTest.length;

  if (totalElements === 0) {
    return {
      score: 0,
      validCount: 0,
      totalCount: 0,
      elementResults: [],
      environments: envs.map((e) => e.id),
    };
  }

  const elementResults: ElementRobustnessResult[] = [];
  let totalValidCount = 0;
  const totalCount = totalElements * envs.length;

  // Launch browser
  const browser = await chromium.launch({ headless: opts.headless });

  try {
    for (const element of elementsToTest) {
      const envResults: ElementEnvResult[] = [];
      let elementValidCount = 0;

      for (const env of envs) {
        if (opts.verbose) {
          console.log(`[RobustnessScorer] Testing ${element.elementId} in ${env.id}`);
        }

        const context = await browser.newContext({
          viewport: env.viewport,
          locale: env.locale,
        });
        const page = await context.newPage();

        try {
          await page.goto(url, { waitUntil: "domcontentloaded" });
          await page.waitForTimeout(1000);

          // Execute pre_actions if provided (using golden ref_selectors)
          if (element.preActions && element.preActions.length > 0 && goldenElements) {
            await executePreActionsInPage(page, element.preActions, goldenElements, opts.timeout || 5000);
          }

          // Validate with relaxed matching if refSelector is provided
          const result = element.refSelector
            ? await validateWithRelaxedMatching(
                page,
                element.selectors,
                element.refSelector,
                opts.timeout || 5000
              )
            : await validateSelectorInEnv(page, element.selectors, opts.timeout || 5000);

          // Design alignment: if recorded selector matches reference element but is not unique (count > 1),
          // it should NOT contribute to robustness (only to recall). Hence require `unique` here.
          const valid = result.found && result.visible && result.unique;
          if (valid) {
            elementValidCount++;
            totalValidCount++;
          }

          envResults.push({
            envId: env.id,
            valid,
            found: result.found,
            unique: result.unique,
            visible: result.visible,
            count: result.count,
          });
        } catch (error) {
          envResults.push({
            envId: env.id,
            valid: false,
            found: false,
            unique: false,
            visible: false,
            count: 0,
            error: error instanceof Error ? error.message : String(error),
          });
        } finally {
          await context.close();
        }
      }

      elementResults.push({
        elementId: element.elementId,
        envResults,
        score: envResults.length > 0 ? elementValidCount / envResults.length : 0,
      });
    }
  } finally {
    await browser.close();
  }

  const score = totalCount > 0 ? totalValidCount / totalCount : 0;

  return {
    score,
    validCount: totalValidCount,
    totalCount,
    elementResults,
    environments: envs.map((e) => e.id),
  };
}

/**
 * Execute pre-actions in a page using golden ref_selectors
 */
async function executePreActionsInPage(
  page: Page,
  preActions: PreActionStep[],
  goldenElements: GoldenElement[],
  timeout: number
): Promise<void> {
  const goldenMap = new Map(goldenElements.map((g) => [g.id, g]));

  for (const step of preActions) {
    const { action, element_id, value } = step;

    switch (action) {
      case "wait": {
        const ms = value ? parseInt(value, 10) : 500;
        await page.waitForTimeout(ms);
        break;
      }

      case "goto": {
        if (value) {
          await page.goto(value, { waitUntil: "domcontentloaded" });
        }
        break;
      }

      case "go_back": {
        await page.goBack();
        break;
      }

      case "go_forward": {
        await page.goForward();
        break;
      }

      case "press": {
        if (value) {
          await page.keyboard.press(value);
        }
        break;
      }

      case "click":
      case "hover":
      case "type":
      case "scroll": {
        if (element_id) {
          const golden = goldenMap.get(element_id);
          if (golden?.ref_selector) {
            const locator = page.locator(golden.ref_selector);
            const count = await locator.count();
            if (count > 0) {
              if (action === "click") {
                await locator.first().click({ timeout });
              } else if (action === "hover") {
                await locator.first().hover({ timeout });
              } else if (action === "type" && value) {
                await locator.first().fill(value, { timeout });
              } else if (action === "scroll") {
                await locator.first().scrollIntoViewIfNeeded({ timeout });
              }
            }
          }
        } else if (action === "scroll") {
          if (value === "up") {
            await page.mouse.wheel(0, -300);
          } else if (value === "down") {
            await page.mouse.wheel(0, 300);
          }
        }
        break;
      }
    }

    await page.waitForTimeout(300); // ACTION_DELAY
  }
}

/**
 * Validate with relaxed matching (A ⊆ B)
 * Checks if the reference element is among the elements found by recorded selectors
 */
async function validateWithRelaxedMatching(
  page: Page,
  selectors: Array<{ type: string; value: string }>,
  refSelector: string,
  timeout: number
): Promise<{ found: boolean; unique: boolean; visible: boolean; count: number }> {
  // First, find the reference element
  const refLocator = page.locator(refSelector);
  const refCount = await refLocator.count();
  if (refCount === 0) {
    return { found: false, unique: false, visible: false, count: 0 };
  }
  const refHandle = await refLocator.first().elementHandle();

  // Try each recorded selector
  for (const sel of selectors) {
    try {
      let locator: Locator;
      switch (sel.type) {
        case "xpath":
          locator = page.locator(`xpath=${sel.value}`);
          break;
        case "text":
          locator = page.locator(`text=${sel.value}`);
          break;
        default:
          locator = page.locator(sel.value);
      }

      const count = await locator.count();
      if (count > 0 && refHandle) {
        // Relaxed matching: check if ref element is ANY of the found elements
        for (let i = 0; i < count; i++) {
          const handle = await locator.nth(i).elementHandle();
          if (handle) {
            const isSame = await page.evaluate(
              ([a, b]) => a === b,
              [refHandle, handle]
            );
            if (isSame) {
              const isVisible = await locator.nth(i).isVisible({ timeout });
              return {
                found: true,
                unique: count === 1,
                visible: isVisible,
                count,
              };
            }
          }
        }
      }
    } catch {
      continue;
    }
  }

  return { found: false, unique: false, visible: false, count: 0 };
}

/**
 * Synchronous robustness scorer for Braintrust
 * Reads pre-computed robustness score from EvalOutput (computed in task phase)
 *
 * Returns null score when robustness was skipped, so it doesn't affect averages.
 */
export function robustnessScorer(args: {
  input: EvalInput;
  output: EvalOutput;
}): { name: string; score: number | null; metadata: Record<string, unknown> } {
  const score = args.output.robustnessScore;

  if (score === undefined) {
    // Robustness not computed - return null so it doesn't affect averages
    console.log(`[RobustnessScorer] Skipped (remove --skip-robustness to enable)`);
    return {
      name: "robustness",
      score: null, // Don't affect averages when skipped
      metadata: { computed: false },
    };
  }

  console.log(`[RobustnessScorer] Score: ${(score * 100).toFixed(1)}%`);
  return {
    name: "robustness",
    score,
    metadata: { computed: true },
  };
}

/**
 * Async robustness scorer with full cross-environment validation
 */
export async function robustnessScorerAsync(args: {
  input: EvalInput;
  output: EvalOutput;
  options?: RobustnessScorerOptions;
}): Promise<{ name: string; score: number; metadata: RobustnessScoreResult }> {
  const result = await calculateRobustness(
    args.output.siteCapability,
    args.input.url,
    args.options
  );

  // Log summary
  console.log(
    `[RobustnessScorer] Score: ${result.validCount}/${result.totalCount} (${(result.score * 100).toFixed(1)}%)`
  );
  console.log(`  Environments: ${result.environments.join(", ")}`);

  for (const elem of result.elementResults) {
    const validEnvs = elem.envResults.filter((e) => e.valid).map((e) => e.envId);
    const status = elem.score === 1 ? "✓" : elem.score > 0 ? "△" : "✗";
    console.log(
      `  ${status} ${elem.elementId}: ${(elem.score * 100).toFixed(0)}% (${validEnvs.join(", ") || "none"})`
    );
  }

  return {
    name: "robustness",
    score: result.score,
    metadata: result,
  };
}
