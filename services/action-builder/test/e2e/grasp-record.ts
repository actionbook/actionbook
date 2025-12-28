#!/usr/bin/env npx tsx
/**
 * Grasp Record Test - Simple test using ActionBuilder
 *
 * Tests the action-builder module with getgrasp.ai
 * Similar to airbnb-record but simpler site
 *
 * Environment:
 *   Set ONE of: OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY
 *   AIClient auto-detects provider from available API keys.
 */

import { ActionBuilder } from "../../src/ActionBuilder.js";
import {
  loadEnv,
  requireLLMApiKey,
  getDetectedProvider,
} from "../helpers/env-loader.js";

// Load environment and validate
loadEnv();
requireLLMApiKey();

async function runGraspRecordTest(): Promise<void> {
  const { provider, model } = getDetectedProvider();

  console.log("=".repeat(60));
  console.log("Action Builder - Grasp Record Test");
  console.log("=".repeat(60));
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);
  console.log(`Output Dir: ./output`);
  console.log("=".repeat(60));

  // AIClient auto-detects provider from environment variables
  const builder = new ActionBuilder({
    outputDir: "./output",
    headless: false,
    maxTurns: 20,  // Simpler site, fewer turns needed
    databaseUrl: process.env.DATABASE_URL,
  });

  try {
    await builder.initialize();

    const result = await builder.build(
      "https://getgrasp.ai/",
      "grasp_homepage",
      {
        siteName: "Grasp",
        siteDescription: "AI automation platform homepage",
        focusAreas: ["navigation", "call-to-action buttons", "links"],
      }
    );

    // Output recorded elements details
    if (result.success && result.siteCapability) {
      const cap = result.siteCapability;
      console.log("\n" + "=".repeat(60));
      console.log("Recorded Elements");
      console.log("=".repeat(60));

      for (const [pageType, page] of Object.entries(cap.pages)) {
        console.log(`\n📄 Page: ${pageType}`);
        for (const [elementId, element] of Object.entries(page.elements)) {
          console.log(`   - ${elementId}: ${element.description?.substring(0, 50) || "No description"}`);
        }
      }

      if (Object.keys(cap.global_elements).length > 0) {
        console.log(`\n📄 Global Elements:`);
        for (const [elementId, element] of Object.entries(cap.global_elements)) {
          console.log(`   - ${elementId}: ${element.description?.substring(0, 50) || "No description"}`);
        }
      }

      // Validate recorded selectors
      console.log("\n" + "=".repeat(60));
      console.log("Validating Selectors");
      console.log("=".repeat(60));

      const validateResult = await builder.validate(cap.domain, { verbose: true });

      console.log("\n" + "=".repeat(60));
      console.log("Validation Results");
      console.log("=".repeat(60));
      console.log(`📊 Total Elements: ${validateResult.totalElements}`);
      console.log(`✅ Valid: ${validateResult.validElements}`);
      console.log(`❌ Invalid: ${validateResult.invalidElements}`);
      console.log(`📈 Rate: ${(validateResult.validationRate * 100).toFixed(1)}%`);
    }

    if (!result.success) {
      console.error("\n❌ Recording failed:", result.message);
    }

    await builder.close();
    process.exit(result.success ? 0 : 1);
  } catch (error) {
    console.error("Fatal error:", error);
    await builder.close();
    process.exit(1);
  }
}

// Run the test
runGraspRecordTest().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});
