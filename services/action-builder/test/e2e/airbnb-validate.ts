#!/usr/bin/env npx tsx
/**
 * Validate Airbnb selectors test
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
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);

  // AIClient auto-detects provider from environment variables
  const builder = new ActionBuilder({
    outputDir: "./output",
    headless: false,
  });

  try {
    await builder.initialize();

    console.log("🔍 Validating www.airbnb.com selectors...\n");
    const result = await builder.validate("www.airbnb.com", { verbose: true });

    console.log("\n" + "=".repeat(60));
    console.log("📊 Validation Results");
    console.log("=".repeat(60));
    console.log(`  Total Elements: ${result.totalElements}`);
    console.log(`  Valid Elements: ${result.validElements}`);
    console.log(`  Invalid Elements: ${result.invalidElements}`);
    console.log(`  Validation Rate: ${(result.validationRate * 100).toFixed(1)}%`);

    if (result.details.length > 0) {
      console.log("\n📋 Details:");
      for (const d of result.details) {
        const status = d.valid ? "✅" : "❌";
        console.log(`  ${status} ${d.elementId} (${d.pageType})`);
        if (d.selectorsDetail) {
          for (const s of d.selectorsDetail) {
            const ss = s.valid ? "✓" : "✗";
            const template = s.isTemplate ? " [template]" : "";
            console.log(`     ${ss} ${s.type}: ${s.value.substring(0, 50)}${template}`);
            if (s.error) {
              console.log(`        Error: ${s.error}`);
            }
          }
        }
      }
    }

    console.log("\n" + "=".repeat(60));
    await builder.close();
    process.exit(result.success ? 0 : 1);
  } catch (error) {
    console.error("Fatal error:", error);
    await builder.close();
    process.exit(1);
  }
}

validate().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});

