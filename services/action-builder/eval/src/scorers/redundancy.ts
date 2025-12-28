/**
 * Redundancy Scorer
 *
 * Measures recording precision: what fraction of recorded elements are in the golden set.
 * Higher score = better (less redundant elements, more precise recording).
 *
 * Precision = |E_matched| / |E_recorded|
 *
 * This follows Braintrust convention where higher scores are better.
 * A score of 1.0 means all recorded elements match golden elements (no redundancy).
 * A score of 0.0 means no recorded elements match golden elements (all redundant).
 */

import type {
  GoldenElement,
  SiteCapability,
  EvalInput,
  EvalOutput,
} from "../types.js";
import { getRecordedElements } from "../tasks/offline_eval.js";

export interface RedundancyScoreResult {
  /** Precision score (0-1, higher is better = less redundancy) */
  score: number;
  /** Number of recorded elements that match golden elements */
  matchedCount: number;
  /** Total number of recorded elements */
  totalRecorded: number;
  /** IDs of unmatched (redundant) elements */
  unmatchedElements: string[];
  /** IDs of matched elements */
  matchedElements: string[];
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
 * Check if a recorded element matches any golden element
 */
function matchesGoldenElement(
  recordedId: string,
  goldenElements: GoldenElement[]
): boolean {
  const normalizedRecordedId = normalizeId(recordedId);

  for (const golden of goldenElements) {
    // Exact match
    if (recordedId === golden.id) {
      return true;
    }

    // Semantic similarity match
    const normalizedGoldenId = normalizeId(golden.id);
    const similarity = stringSimilarity(normalizedRecordedId, normalizedGoldenId);

    if (similarity >= 0.7) {
      return true;
    }
  }

  return false;
}

/**
 * Calculate precision score (inverse of redundancy)
 *
 * Precision = |E_matched| / |E_recorded|
 *
 * Higher score = better (less redundancy, more precise recording)
 */
export function calculateRedundancy(
  goldenElements: GoldenElement[],
  capability: SiteCapability | null
): RedundancyScoreResult {
  if (!capability) {
    return {
      score: 0,
      matchedCount: 0,
      totalRecorded: 0,
      unmatchedElements: [],
      matchedElements: [],
    };
  }

  const recordedElements = getRecordedElements(capability);
  const totalRecorded = recordedElements.size;

  if (totalRecorded === 0) {
    // No recorded elements - can't compute precision, return 0
    return {
      score: 0,
      matchedCount: 0,
      totalRecorded: 0,
      unmatchedElements: [],
      matchedElements: [],
    };
  }

  const matchedElements: string[] = [];
  const unmatchedElements: string[] = [];

  for (const recordedId of recordedElements.keys()) {
    if (matchesGoldenElement(recordedId, goldenElements)) {
      matchedElements.push(recordedId);
    } else {
      unmatchedElements.push(recordedId);
    }
  }

  const matchedCount = matchedElements.length;
  // Precision: what fraction of recorded elements are in the golden set
  // Higher = better (less redundancy)
  const score = matchedCount / totalRecorded;

  return {
    score,
    matchedCount,
    totalRecorded,
    unmatchedElements,
    matchedElements,
  };
}

/**
 * Braintrust scorer function for redundancy (precision)
 *
 * Returns precision score: higher = better (less redundancy)
 */
export function redundancyScorer(args: {
  input: EvalInput;
  output: EvalOutput;
}): { name: string; score: number; metadata: Record<string, unknown> } {
  const result = calculateRedundancy(
    args.input.expected.must_have_elements,
    args.output.siteCapability
  );

  // Log details
  console.log(
    `[RedundancyScorer] Precision: ${(result.score * 100).toFixed(1)}% (higher = less redundancy)`
  );
  console.log(
    `  Matched: ${result.matchedCount}/${result.totalRecorded} recorded elements`
  );

  if (result.unmatchedElements.length > 0) {
    console.log(`  Redundant elements: ${result.unmatchedElements.join(", ")}`);
  }

  return {
    name: "redundancy",
    score: result.score,
    metadata: result as unknown as Record<string, unknown>,
  };
}
