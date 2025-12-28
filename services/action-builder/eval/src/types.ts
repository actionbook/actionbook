/**
 * Action-Builder Eval Types
 *
 * Type definitions for the evaluation system.
 */

import type { SiteCapability } from "../../src/types/index.js";

// Re-export for convenience
export type { SiteCapability };

// ============ Pre-Action Types ============

export type PreActionType =
  | "click" // Click on element
  | "hover" // Hover on element
  | "type" // Type text into element
  | "press" // Press key combination
  | "scroll" // Scroll page or to element
  | "goto" // Navigate to URL
  | "go_back" // Browser back
  | "go_forward" // Browser forward
  | "wait"; // Wait for time (ms)

export interface PreActionStep {
  /** Target element ID (required for click/hover/type/scroll to element) */
  element_id?: string;
  /** Action type */
  action: PreActionType;
  /** Action parameter: text for type, key combo for press, up/down for scroll, URL for goto, ms for wait */
  value?: string;
}

// ============ Dataset Types ============

export interface GoldenElement {
  /** Semantic ID for matching */
  id: string;
  /** Human-readable description */
  description: string;
  /** Primary reference selector */
  ref_selector: string;
  /** Fallback selectors (for site changes) */
  ref_selector_fallbacks?: string[];
  /** Pre-actions to execute before validating this element */
  pre_actions?: PreActionStep[];
  /** Whether the selector is a template */
  is_template?: boolean;
  /** Template parameters to substitute */
  template_params?: Record<string, string>;
  /** Allowed methods for this element */
  allowed_methods?: string[];
}

export interface EvalCase {
  /** Unique case identifier */
  id: string;
  /** Starting URL */
  url: string;
  /** Task description (prompt for AB) */
  scenario: string;
  /** Path to pre-recorded capability file (for offline eval) */
  capability_file?: string;
  /** Elements that must be captured */
  must_have_elements: GoldenElement[];
  /** Preconditions (e.g., "logged_in") */
  preconditions?: string[];
  /** Tags for filtering */
  tags?: string[];
}

export interface Dataset {
  version: string;
  cases: EvalCase[];
}

// ============ Test Matrix Types ============

export interface TestEnv {
  id: string;
  viewport: { width: number; height: number };
  locale: string;
}

export interface EvalInput {
  /** Case ID */
  caseId: string;
  /** Starting URL */
  url: string;
  /** Task scenario */
  scenario: string;
  /** LLM model name (for online eval) */
  modelName?: string;
  /** Test environment */
  env?: TestEnv;
  /** Expected elements */
  expected: {
    must_have_elements: GoldenElement[];
  };
  /** Path to capability file (for offline eval) */
  capabilityFile?: string;
}

// ============ Eval Output Types ============

export interface EvalOutput {
  /** Site capability from AB or loaded from file */
  siteCapability: SiteCapability | null;
  /** Operational cost data */
  cost: {
    /** Total LLM tokens */
    tokens: number;
    /** Number of LLM turns */
    turns: number;
    /** Duration in milliseconds */
    duration: number;
  };
  /** Pre-computed recall result (from DOM verification in task phase) */
  recallResult?: RecallScoreResult;
  /** Pre-computed robustness score (from async validation) */
  robustnessScore?: number;
  /** Error message if failed */
  error?: string;
}

// ============ Score Result Types ============

export interface ElementMatchResult {
  /** Golden element ID */
  goldenId: string;
  /** Whether a match was found */
  matched: boolean;
  /** Matched recorded element ID (if found) */
  matchedRecordedId?: string;
  /** Match method: semantic, dom, or none */
  matchMethod?: "semantic" | "dom" | "none";
  /** Error or reason for no match */
  error?: string;
}

export interface RecallScoreResult {
  /** Recall score (0-1) */
  score: number;
  /** Number of matched elements */
  matched: number;
  /** Total expected elements */
  total: number;
  /** Per-element match details */
  details: ElementMatchResult[];
}

export interface ScoreResult {
  /** Element recall score (0-1) */
  recall: number;
  /** Robustness score (0-1) - for future use */
  robustness?: number;
  /** Redundancy rate (0-1) - for future use */
  redundancy?: number;
  /** Cost metrics */
  cost: {
    tokens: number;
    turns: number;
    duration: number;
  };
}

export interface EvalResult {
  /** Case ID */
  caseId: string;
  /** Model name */
  modelName?: string;
  /** Environment ID */
  envId?: string;
  /** Trial index */
  trialIndex?: number;
  /** Overall success */
  success: boolean;
  /** Scores */
  scores: ScoreResult;
  /** Detailed results */
  details: {
    recall: RecallScoreResult;
  };
}

// ============ Braintrust Types ============

export interface BraintrustTestcase {
  input: EvalInput;
  expected: {
    must_have_elements: GoldenElement[];
  };
  name: string;
  tags?: string[];
  metadata?: Record<string, unknown>;
}
