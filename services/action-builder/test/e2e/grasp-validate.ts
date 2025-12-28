#!/usr/bin/env npx tsx
/**
 * Grasp Validate Test - Validate recorded capabilities
 *
 * Validates the selectors recorded for getgrasp.ai
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

async function validate() {
  const { provider, model } = getDetectedProvider();

  // AIClient auto-detects provider from environment variables
  const builder = new ActionBuilder({
    outputDir: "./output",
    headless: false,
  });

  console.log("=".repeat(60));
  console.log("Grasp Validate Test");
  console.log("=".repeat(60));
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);

  try {
    await builder.initialize();

    const result = await builder.validate("getgrasp.ai", {
      verbose: true,
    });

    console.log("\n" + "=".repeat(60));
    console.log("Validation Results");
    console.log("=".repeat(60));
    console.log(`📊 Total Elements: ${result.totalElements}`);
    console.log(`✅ Valid Elements: ${result.validElements}`);
    console.log(`❌ Invalid Elements: ${result.invalidElements}`);
    console.log(`📈 Validation Rate: ${(result.validationRate * 100).toFixed(1)}%`);

    if (result.details.length > 0) {
      console.log("\n📋 Element Details:");
      for (const detail of result.details) {
        const status = detail.valid ? "✅" : "❌";
        console.log(`\n${status} ${detail.elementId} (${detail.pageType})`);

        if (detail.selectorsDetail) {
          for (const sel of detail.selectorsDetail) {
            const selStatus = sel.valid ? "✓" : "✗";
            const template = sel.isTemplate ? " [template]" : "";
            console.log(`   ${selStatus} ${sel.type}: ${sel.value.substring(0, 60)}${template}`);
            if (sel.error) {
              console.log(`     Error: ${sel.error}`);
            }
          }
        }
      }
    }

    await builder.close();
    process.exit(result.validationRate >= 0.8 ? 0 : 1);
  } catch (error) {
    console.error("❌ Validation error:", error);
    await builder.close();
    process.exit(1);
  }
}

validate().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});
