/**
 * Recall Scorer
 *
 * Calculates element recall score by comparing golden elements with recorded elements.
 *
 * Recall = |E_must ∩ E_recorded| / |E_must|
 *
 * Matching methods:
 * 1. Semantic matching: Compare element IDs for similarity
 * 2. DOM validation: Use Playwright to verify selectors point to same element
 */

import type {
  GoldenElement,
  SiteCapability,
  EvalOutput,
  RecallScoreResult,
  ElementMatchResult,
  EvalInput,
} from "../types.js";
import { getRecordedElements } from "../tasks/offline_eval.js";
import { DOMVerifier, type DOMMatchResult } from "../utils/dom_verifier.js";

export interface RecallScorerOptions {
  /** Enable DOM verification (slower but more accurate) */
  enableDOMVerification?: boolean;
  /** Run browser in headless mode */
  headless?: boolean;
  /** Enable verbose logging */
  verbose?: boolean;
}

// Global options (can be set before running eval)
let globalOptions: RecallScorerOptions = {
  enableDOMVerification: false,
  headless: true,
  verbose: false,
};

/**
 * Set global recall scorer options
 */
export function setRecallScorerOptions(options: RecallScorerOptions): void {
  globalOptions = { ...globalOptions, ...options };
}

/**
 * Calculate string similarity using Levenshtein distance
 */
function stringSimilarity(a: string, b: string): number {
  const longer = a.length > b.length ? a : b;
  const shorter = a.length > b.length ? b : a;

  if (longer.length === 0) {
    return 1.0;
  }

  const editDistance = levenshteinDistance(longer, shorter);
  return (longer.length - editDistance) / longer.length;
}

function levenshteinDistance(a: string, b: string): number {
  const matrix: number[][] = [];

  for (let i = 0; i <= b.length; i++) {
    matrix[i] = [i];
  }

  for (let j = 0; j <= a.length; j++) {
    matrix[0][j] = j;
  }

  for (let i = 1; i <= b.length; i++) {
    for (let j = 1; j <= a.length; j++) {
      if (b.charAt(i - 1) === a.charAt(j - 1)) {
        matrix[i][j] = matrix[i - 1][j - 1];
      } else {
        matrix[i][j] = Math.min(
          matrix[i - 1][j - 1] + 1,
          matrix[i][j - 1] + 1,
          matrix[i - 1][j] + 1
        );
      }
    }
  }

  return matrix[b.length][a.length];
}

/**
 * Normalize element ID for comparison
 */
function normalizeId(id: string): string {
  return id
    .toLowerCase()
    .replace(/_/g, " ")
    .replace(/btn/g, "button")
    .replace(/input/g, "field")
    .replace(/picker/g, "selector");
}

/**
 * Find best matching recorded element for a golden element (semantic only)
 */
function findSemanticMatch(
  goldenElement: GoldenElement,
  recordedElements: Map<string, { pageType: string; selectors: unknown[] }>
): { matchedId: string | null; score: number } {
  const goldenId = normalizeId(goldenElement.id);
  let bestMatch: string | null = null;
  let bestScore = 0;

  // First, try exact match
  if (recordedElements.has(goldenElement.id)) {
    return { matchedId: goldenElement.id, score: 1.0 };
  }

  // Then, try semantic similarity
  for (const recordedId of recordedElements.keys()) {
    const normalizedRecordedId = normalizeId(recordedId);
    const similarity = stringSimilarity(goldenId, normalizedRecordedId);

    if (similarity > bestScore && similarity >= 0.7) {
      bestScore = similarity;
      bestMatch = recordedId;
    }
  }

  return { matchedId: bestMatch, score: bestScore };
}

/**
 * Calculate recall score with semantic matching only
 */
function calculateSemanticRecall(
  goldenElements: GoldenElement[],
  capability: SiteCapability
): RecallScoreResult {
  const details: ElementMatchResult[] = [];
  const recordedElements = getRecordedElements(capability);
  let matchedCount = 0;

  for (const goldenElement of goldenElements) {
    const match = findSemanticMatch(goldenElement, recordedElements);

    if (match.matchedId) {
      matchedCount++;
      details.push({
        goldenId: goldenElement.id,
        matched: true,
        matchedRecordedId: match.matchedId,
        matchMethod: "semantic",
      });
    } else {
      details.push({
        goldenId: goldenElement.id,
        matched: false,
        matchMethod: "none",
        error: "No matching element found in recorded capability",
      });
    }
  }

  const score = goldenElements.length > 0 ? matchedCount / goldenElements.length : 0;

  return {
    score,
    matched: matchedCount,
    total: goldenElements.length,
    details,
  };
}

/**
 * Calculate recall score with DOM verification
 */
async function calculateDOMRecall(
  goldenElements: GoldenElement[],
  capability: SiteCapability,
  url: string,
  options: RecallScorerOptions
): Promise<RecallScoreResult> {
  const details: ElementMatchResult[] = [];
  const recordedElements = getRecordedElements(capability);
  let matchedCount = 0;

  const verifier = new DOMVerifier({
    headless: options.headless,
    verbose: options.verbose,
  });

  try {
    await verifier.init();
    await verifier.navigate(url);

    for (const goldenElement of goldenElements) {
      // First try semantic match to find candidate
      const semanticMatch = findSemanticMatch(goldenElement, recordedElements);

      if (!semanticMatch.matchedId) {
        details.push({
          goldenId: goldenElement.id,
          matched: false,
          matchMethod: "none",
          error: "No semantic match found",
        });
        continue;
      }

      // Skip DOM verification if no ref_selector
      if (!goldenElement.ref_selector) {
        matchedCount++;
        details.push({
          goldenId: goldenElement.id,
          matched: true,
          matchedRecordedId: semanticMatch.matchedId,
          matchMethod: "semantic",
        });
        continue;
      }

      // Reset page for elements with pre-actions
      if (goldenElement.pre_actions && goldenElement.pre_actions.length > 0) {
        await verifier.navigate(url);
      }

      // DOM verification
      const domResult = await verifier.verifyGoldenElement(goldenElement, capability);

      if (domResult.matched) {
        matchedCount++;
        details.push({
          goldenId: goldenElement.id,
          matched: true,
          matchedRecordedId: semanticMatch.matchedId,
          matchMethod: "dom",
        });
      } else if (domResult.recordedFound && domResult.refFound) {
        // Both found but not same element - semantic match but DOM mismatch
        details.push({
          goldenId: goldenElement.id,
          matched: false,
          matchedRecordedId: semanticMatch.matchedId,
          matchMethod: "none",
          error: "DOM verification failed: selectors point to different elements",
        });
      } else {
        details.push({
          goldenId: goldenElement.id,
          matched: false,
          matchMethod: "none",
          error: domResult.error || "DOM verification failed",
        });
      }
    }
  } finally {
    await verifier.close();
  }

  const score = goldenElements.length > 0 ? matchedCount / goldenElements.length : 0;

  return {
    score,
    matched: matchedCount,
    total: goldenElements.length,
    details,
  };
}

/**
 * Calculate recall score for a single eval case
 */
export async function calculateRecall(
  goldenElements: GoldenElement[],
  capability: SiteCapability | null,
  url?: string,
  options?: RecallScorerOptions
): Promise<RecallScoreResult> {
  const opts = { ...globalOptions, ...options };

  if (!capability) {
    return {
      score: 0,
      matched: 0,
      total: goldenElements.length,
      details: goldenElements.map((g) => ({
        goldenId: g.id,
        matched: false,
        matchMethod: "none" as const,
        error: "No capability data available",
      })),
    };
  }

  // Use DOM verification if enabled and URL is provided
  if (opts.enableDOMVerification && url) {
    return calculateDOMRecall(goldenElements, capability, url, opts);
  }

  // Fall back to semantic matching
  return calculateSemanticRecall(goldenElements, capability);
}

/**
 * Braintrust scorer function for recall
 *
 * Reads pre-computed DOM verification result from EvalOutput (computed in task phase).
 * Falls back to semantic matching if DOM result is not available.
 */
export function recallScorer(args: {
  input: EvalInput;
  output: EvalOutput;
}): { name: string; score: number; metadata: Record<string, unknown> } {
  const goldenElements = args.input.expected.must_have_elements;

  // Handle null siteCapability (build failed)
  if (!args.output.siteCapability) {
    const result: RecallScoreResult = {
      score: 0,
      matched: 0,
      total: goldenElements.length,
      details: goldenElements.map((g) => ({
        goldenId: g.id,
        matched: false,
        matchMethod: "none" as const,
        error: args.output.error || "No capability data available",
      })),
    };
    console.log(`[RecallScorer] Score: 0/${result.total} (0.0%) - Build failed`);
    return {
      name: "recall",
      score: 0,
      metadata: result as unknown as Record<string, unknown>,
    };
  }

  // Use pre-computed DOM verification result (preferred)
  if (args.output.recallResult) {
    const result = args.output.recallResult;
    console.log(`[RecallScorer] Score: ${result.matched}/${result.total} (${(result.score * 100).toFixed(1)}%) [DOM]`);
    for (const detail of result.details) {
      const status = detail.matched ? "✓" : "✗";
      const matchInfo = detail.matched
        ? `[${detail.matchMethod}]`
        : detail.error;
      console.log(`  ${status} ${detail.goldenId}: ${matchInfo}`);
    }
    return {
      name: "recall",
      score: result.score,
      metadata: result as unknown as Record<string, unknown>,
    };
  }

  // Fallback to semantic matching (for backwards compatibility)
  const result = calculateSemanticRecall(goldenElements, args.output.siteCapability);

  // Log details
  console.log(`[RecallScorer] Score: ${result.matched}/${result.total} (${(result.score * 100).toFixed(1)}%) [semantic]`);
  for (const detail of result.details) {
    const status = detail.matched ? "✓" : "✗";
    const matchInfo = detail.matched
      ? `-> ${detail.matchedRecordedId}`
      : detail.error;
    console.log(`  ${status} ${detail.goldenId}: ${matchInfo}`);
  }

  return {
    name: "recall",
    score: result.score,
    metadata: result as unknown as Record<string, unknown>,
  };
}

/**
 * Async recall scorer with DOM verification support
 * Use this for standalone runs, not with Braintrust
 */
export async function recallScorerAsync(args: {
  input: EvalInput;
  output: EvalOutput;
  options?: RecallScorerOptions;
}): Promise<{ name: string; score: number; metadata: RecallScoreResult }> {
  const result = await calculateRecall(
    args.input.expected.must_have_elements,
    args.output.siteCapability,
    args.input.url,
    args.options
  );

  // Log details
  console.log(`[RecallScorer] Score: ${result.matched}/${result.total} (${(result.score * 100).toFixed(1)}%)`);
  for (const detail of result.details) {
    const status = detail.matched ? "✓" : "✗";
    const method = detail.matchMethod ? `[${detail.matchMethod}]` : "";
    const matchInfo = detail.matched
      ? `-> ${detail.matchedRecordedId} ${method}`
      : detail.error;
    console.log(`  ${status} ${detail.goldenId}: ${matchInfo}`);
  }

  return {
    name: "recall",
    score: result.score,
    metadata: result,
  };
}
