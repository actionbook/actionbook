#!/usr/bin/env npx tsx
/**
 * DOM Verifier Test Script
 *
 * Tests the DOM verification flow with Playwright against a live website.
 *
 * Usage:
 *   pnpm test:dom
 */

import path from "path";
import { loadTestcases } from "./suites/loader.js";
import { loadCapability } from "./tasks/offline_eval.js";
import { DOMVerifier } from "./utils/dom_verifier.js";
import { calculateRecall, setRecallScorerOptions } from "./scorers/recall.js";
import type { GoldenElement, SiteCapability } from "./types.js";

async function testDOMVerifier(): Promise<void> {
  console.log("=".repeat(60));
  console.log("DOM Verifier Test");
  console.log("=".repeat(60));

  // Load test case
  const testcases = loadTestcases("smoke");
  const testcase = testcases[0];

  if (!testcase) {
    console.error("No testcases found");
    process.exit(1);
  }

  console.log(`\nTest Case: ${testcase.input.caseId}`);
  console.log(`URL: ${testcase.input.url}`);
  console.log(`Golden Elements: ${testcase.input.expected.must_have_elements.length}`);

  // Load capability
  if (!testcase.input.capabilityFile) {
    console.error("No capability file specified in testcase");
    process.exit(1);
  }

  const capabilityPath = path.resolve(
    import.meta.dirname,
    "../datasets",
    testcase.input.capabilityFile
  );
  const capability = loadCapability(capabilityPath);

  if (!capability) {
    console.error("Failed to load capability");
    process.exit(1);
  }

  console.log("\n--- Test 1: Direct DOM Verifier ---\n");
  await testDirectVerifier(
    testcase.input.url,
    testcase.input.expected.must_have_elements,
    capability
  );

  console.log("\n--- Test 2: Recall Scorer with DOM Verification ---\n");
  await testRecallScorerWithDOM(
    testcase.input.url,
    testcase.input.expected.must_have_elements,
    capability
  );

  console.log("\n" + "=".repeat(60));
  console.log("Tests Complete");
  console.log("=".repeat(60));
}

async function testDirectVerifier(
  url: string,
  goldenElements: GoldenElement[],
  capability: SiteCapability
): Promise<void> {
  const verifier = new DOMVerifier({
    headless: true,
    verbose: true,
    timeout: 10000,
  });

  try {
    await verifier.init();
    await verifier.navigate(url);

    console.log("Testing individual elements:\n");

    for (const golden of goldenElements) {
      console.log(`Element: ${golden.id}`);
      console.log(`  Description: ${golden.description}`);
      console.log(`  Ref Selector: ${golden.ref_selector}`);

      if (golden.pre_actions && golden.pre_actions.length > 0) {
        console.log(`  Pre-actions: ${golden.pre_actions.length} steps`);
        // Reset page for elements with pre-actions
        await verifier.navigate(url);
      }

      const result = await verifier.verifyGoldenElement(golden, capability);

      console.log(`  Result: ${result.matched ? "✓ MATCHED" : "✗ NOT MATCHED"}`);
      if (result.error) {
        console.log(`  Error: ${result.error}`);
      }
      console.log(`  Recorded found: ${result.recordedFound}, Ref found: ${result.refFound}`);
      console.log("");
    }
  } finally {
    await verifier.close();
  }
}

async function testRecallScorerWithDOM(
  url: string,
  goldenElements: GoldenElement[],
  capability: SiteCapability
): Promise<void> {
  // Set options for DOM verification
  setRecallScorerOptions({
    enableDOMVerification: true,
    headless: true,
    verbose: true,
  });

  const result = await calculateRecall(goldenElements, capability, url);

  console.log(`\nRecall Score: ${result.matched}/${result.total} (${(result.score * 100).toFixed(1)}%)`);
  console.log("\nDetails:");

  for (const detail of result.details) {
    const status = detail.matched ? "✓" : "✗";
    const method = detail.matchMethod ? `[${detail.matchMethod}]` : "";
    const matchInfo = detail.matched
      ? `-> ${detail.matchedRecordedId} ${method}`
      : detail.error;
    console.log(`  ${status} ${detail.goldenId}: ${matchInfo}`);
  }
}

// Run
testDOMVerifier().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});
