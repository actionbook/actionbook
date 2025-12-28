/**
 * Offline Eval Task
 *
 * Loads pre-recorded SiteCapability from JSON file for offline evaluation.
 * Does not run ActionBuilder - just loads existing data for scoring.
 *
 * Pre-computes async metrics (recall via DOM verification, robustness) in task phase
 * so that Braintrust scorers can read them synchronously.
 */

import fs from "fs";
import path from "path";
import type { EvalInput, EvalOutput, SiteCapability, RecallScoreResult, ElementMatchResult } from "../types.js";
import { calculateRobustness, type RobustnessScoreResult } from "../scorers/robustness.js";
import { verifyElements, type DOMMatchResult } from "../utils/dom_verifier.js";
import { getLogger } from "../utils/eval_logger.js";

const DATASETS_DIR = path.resolve(import.meta.dirname, "../../datasets");

// Global options for offline eval
let globalOfflineOptions = {
  enableRobustness: true,  // Default: enabled
  robustnessEnvIds: ["desktop_en"] as string[],
};

/**
 * Set global offline eval options
 */
export function setOfflineEvalOptions(options: {
  enableRobustness?: boolean;
  robustnessEnvIds?: string[];
}): void {
  globalOfflineOptions = { ...globalOfflineOptions, ...options };
}

/**
 * Load SiteCapability from a JSON file
 */
export function loadCapability(filePath: string): SiteCapability | null {
  // Resolve relative paths from datasets directory
  const resolvedPath = filePath.startsWith("./") || filePath.startsWith("../")
    ? path.resolve(DATASETS_DIR, filePath)
    : filePath;

  if (!fs.existsSync(resolvedPath)) {
    console.error(`[OfflineEval] Capability file not found: ${resolvedPath}`);
    return null;
  }

  try {
    const content = fs.readFileSync(resolvedPath, "utf-8");
    const capability = JSON.parse(content) as SiteCapability;
    console.log(`[OfflineEval] Loaded capability: ${capability.domain}`);
    return capability;
  } catch (error) {
    console.error(
      `[OfflineEval] Failed to parse capability file: ${resolvedPath}`,
      error
    );
    return null;
  }
}

/**
 * Offline eval task function
 *
 * This task loads a pre-recorded capability file instead of running ActionBuilder.
 * Used for fast iteration on scorers without waiting for AB execution.
 */
export async function offlineEvalTask(input: EvalInput): Promise<EvalOutput> {
  const startTime = Date.now();
  const logger = getLogger(input.caseId);

  logger.section(`Offline Eval: ${input.caseId}`);
  logger.info(`URL: ${input.url}`);
  logger.info(`Scenario: ${input.scenario}`);

  // Load capability from file
  if (!input.capabilityFile) {
    logger.error("No capability_file specified in test case");
    logger.save();
    return {
      siteCapability: null,
      cost: { tokens: 0, turns: 0, duration: Date.now() - startTime },
      error: "No capability_file specified in test case",
    };
  }

  logger.info(`Loading capability file: ${input.capabilityFile}`);
  const capability = loadCapability(input.capabilityFile);

  if (!capability) {
    logger.error(`Failed to load capability file: ${input.capabilityFile}`);
    logger.save();
    return {
      siteCapability: null,
      cost: { tokens: 0, turns: 0, duration: Date.now() - startTime },
      error: `Failed to load capability file: ${input.capabilityFile}`,
    };
  }

  logger.info(`Loaded capability: ${capability.domain}`);
  logger.info(`Pages: ${Object.keys(capability.pages).length}`);
  logger.info(`Global elements: ${Object.keys(capability.global_elements).length}`);

  const goldenElements = input.expected.must_have_elements;

  // Pre-compute Recall with DOM verification
  logger.info(`Running DOM verification for ${goldenElements.length} golden elements...`);
  let recallResult: RecallScoreResult;
  try {
    const domResults = await verifyElements(input.url, goldenElements, capability, { headless: true });
    recallResult = computeRecallFromDOMResults(goldenElements, domResults);
    logger.info(`Recall: ${recallResult.matched}/${recallResult.total} (${(recallResult.score * 100).toFixed(1)}%)`);
    for (const detail of recallResult.details) {
      const status = detail.matched ? "✓" : "✗";
      const info = detail.matched ? `[${detail.matchMethod}]` : detail.error;
      logger.info(`  ${status} ${detail.goldenId}: ${info}`);
    }
  } catch (error) {
    logger.error(`DOM verification failed: ${error}`);
    // Fall back to empty result on error
    recallResult = {
      score: 0,
      matched: 0,
      total: goldenElements.length,
      details: goldenElements.map((g) => ({
        goldenId: g.id,
        matched: false,
        matchMethod: "none" as const,
        error: `DOM verification failed: ${error}`,
      })),
    };
  }

  // Calculate robustness score if enabled (only for matched elements)
  let robustnessScore: number | undefined;
  let robustnessResult: RobustnessScoreResult | undefined;
  if (globalOfflineOptions.enableRobustness) {
    logger.info(`Running robustness validation for matched elements...`);
    robustnessResult = await calculateRobustness(
      capability,
      input.url,
      { envIds: globalOfflineOptions.robustnessEnvIds, headless: true },
      goldenElements,  // Pass golden elements for pre_actions and filtering
      recallResult     // Pass recall result to filter to matched elements only
    );
    robustnessScore = robustnessResult.score;
    logger.logRobustnessResults(robustnessResult);
  }

  logger.section("Eval Complete");
  logger.info(`Total duration: ${((Date.now() - startTime) / 1000).toFixed(1)}s`);
  logger.save();

  // For offline eval, cost is 0 since we're not running AB
  return {
    siteCapability: capability,
    cost: {
      tokens: 0,
      turns: 0,
      duration: Date.now() - startTime,
    },
    recallResult,
    robustnessScore,
  };
}

/**
 * Convert DOM verification results to RecallScoreResult
 */
function computeRecallFromDOMResults(
  goldenElements: { id: string }[],
  domResults: Map<string, DOMMatchResult>
): RecallScoreResult {
  const details: ElementMatchResult[] = [];
  let matchedCount = 0;

  for (const golden of goldenElements) {
    const domResult = domResults.get(golden.id);

    if (domResult?.matched) {
      matchedCount++;
      details.push({
        goldenId: golden.id,
        matched: true,
        matchMethod: "dom",
      });
    } else {
      details.push({
        goldenId: golden.id,
        matched: false,
        matchMethod: "none",
        error: domResult?.error || "Element not found in DOM",
      });
    }
  }

  return {
    score: goldenElements.length > 0 ? matchedCount / goldenElements.length : 0,
    matched: matchedCount,
    total: goldenElements.length,
    details,
  };
}

/**
 * Get all recorded elements from a SiteCapability
 */
export function getRecordedElements(
  capability: SiteCapability
): Map<string, { pageType: string; selectors: unknown[] }> {
  const elements = new Map<
    string,
    { pageType: string; selectors: unknown[] }
  >();

  // Global elements
  for (const [id, element] of Object.entries(capability.global_elements)) {
    elements.set(id, {
      pageType: "global",
      selectors: element.selectors,
    });
  }

  // Page elements
  for (const [pageType, page] of Object.entries(capability.pages)) {
    for (const [id, element] of Object.entries(page.elements)) {
      elements.set(id, {
        pageType,
        selectors: element.selectors,
      });
    }
  }

  return elements;
}
