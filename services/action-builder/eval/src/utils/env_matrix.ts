/**
 * Environment Matrix Configuration
 *
 * Defines test environments for cross-environment validation.
 * Used by Robustness Scorer to verify selector stability.
 */

import type { TestEnv } from "../types.js";

/**
 * Predefined test environments
 */
export const TEST_ENVIRONMENTS: Record<string, TestEnv> = {
  desktop_en: {
    id: "desktop_en",
    viewport: { width: 1920, height: 1080 },
    locale: "en-US",
  },
  desktop_zh: {
    id: "desktop_zh",
    viewport: { width: 1920, height: 1080 },
    locale: "zh-CN",
  },
  laptop: {
    id: "laptop",
    viewport: { width: 1366, height: 768 },
    locale: "en-US",
  },
  mobile: {
    id: "mobile",
    viewport: { width: 375, height: 667 },
    locale: "en-US",
  },
};

/**
 * Default environment for single-env testing
 */
export const DEFAULT_ENV: TestEnv = TEST_ENVIRONMENTS.desktop_en;

/**
 * Get environments by IDs
 * @param envIds - Environment IDs to include (e.g., ["desktop_en", "mobile"])
 * @returns Array of TestEnv configurations
 */
export function getEnvironments(envIds?: string[]): TestEnv[] {
  if (!envIds || envIds.length === 0) {
    return [DEFAULT_ENV];
  }

  if (envIds.includes("all")) {
    return Object.values(TEST_ENVIRONMENTS);
  }

  return envIds
    .filter((id) => TEST_ENVIRONMENTS[id])
    .map((id) => TEST_ENVIRONMENTS[id]);
}

/**
 * List all available environment IDs
 */
export function listEnvironments(): string[] {
  return Object.keys(TEST_ENVIRONMENTS);
}

/**
 * Generate test matrix (Case × Model × Env × Trial)
 */
export interface TestMatrixConfig {
  /** Environment IDs to test (default: ["desktop_en"]) */
  envIds?: string[];
  /** Model names to test (default: ["default"]) */
  models?: string[];
  /** Number of trials per configuration (default: 1) */
  trials?: number;
}

export interface TestMatrixEntry {
  envId: string;
  env: TestEnv;
  modelName: string;
  trialIndex: number;
}

/**
 * Generate test matrix entries
 */
export function generateTestMatrix(config: TestMatrixConfig = {}): TestMatrixEntry[] {
  const envs = getEnvironments(config.envIds);
  const models = config.models || ["default"];
  const trials = config.trials || 1;

  const entries: TestMatrixEntry[] = [];

  for (const env of envs) {
    for (const modelName of models) {
      for (let trial = 0; trial < trials; trial++) {
        entries.push({
          envId: env.id,
          env,
          modelName,
          trialIndex: trial,
        });
      }
    }
  }

  return entries;
}

/**
 * Parse CLI environment argument
 * @param envArg - CLI argument (e.g., "desktop_en,mobile" or "all")
 */
export function parseEnvArg(envArg?: string): string[] {
  if (!envArg) {
    return [];
  }

  if (envArg === "all") {
    return ["all"];
  }

  return envArg.split(",").map((s) => s.trim());
}
