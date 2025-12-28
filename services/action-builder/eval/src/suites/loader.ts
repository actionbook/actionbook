/**
 * Dataset Loader
 *
 * Loads evaluation datasets from JSON files and converts them to Braintrust testcases.
 */

import fs from "fs";
import path from "path";
import type {
  Dataset,
  EvalCase,
  EvalInput,
  BraintrustTestcase,
} from "../types.js";

const DATASETS_DIR = path.resolve(import.meta.dirname, "../../datasets");

/**
 * Load a dataset from JSON file
 */
export function loadDataset(datasetName: string): Dataset {
  const filePath = path.join(DATASETS_DIR, `${datasetName}.json`);

  if (!fs.existsSync(filePath)) {
    throw new Error(`Dataset not found: ${filePath}`);
  }

  const content = fs.readFileSync(filePath, "utf-8");
  const dataset = JSON.parse(content) as Dataset;

  console.log(
    `[Loader] Loaded dataset: ${datasetName} (${dataset.cases.length} cases)`
  );

  return dataset;
}

/**
 * List available datasets
 */
export function listDatasets(): string[] {
  if (!fs.existsSync(DATASETS_DIR)) {
    return [];
  }

  return fs
    .readdirSync(DATASETS_DIR)
    .filter((f) => f.endsWith(".json"))
    .map((f) => f.replace(".json", ""));
}

/**
 * Convert EvalCase to EvalInput
 */
export function caseToInput(evalCase: EvalCase): EvalInput {
  return {
    caseId: evalCase.id,
    url: evalCase.url,
    scenario: evalCase.scenario,
    expected: {
      must_have_elements: evalCase.must_have_elements,
    },
    capabilityFile: evalCase.capability_file,
  };
}

/**
 * Convert Dataset to Braintrust Testcases
 */
export function datasetToTestcases(dataset: Dataset): BraintrustTestcase[] {
  return dataset.cases.map((evalCase) => ({
    input: caseToInput(evalCase),
    expected: {
      must_have_elements: evalCase.must_have_elements,
    },
    name: evalCase.id,
    tags: evalCase.tags,
    metadata: {
      url: evalCase.url,
      scenario: evalCase.scenario,
    },
  }));
}

/**
 * Load dataset and convert to Braintrust testcases
 */
export function loadTestcases(datasetName: string): BraintrustTestcase[] {
  const dataset = loadDataset(datasetName);
  return datasetToTestcases(dataset);
}

/**
 * Filter testcases by tags
 */
export function filterByTags(
  testcases: BraintrustTestcase[],
  tags: string[]
): BraintrustTestcase[] {
  if (tags.length === 0) {
    return testcases;
  }

  return testcases.filter((tc) =>
    tc.tags?.some((t) => tags.includes(t))
  );
}
