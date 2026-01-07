#!/usr/bin/env npx tsx
/**
 * Playbook Mode Record Script
 *
 * Single-page element recording with module classification.
 * Supports:
 * - Target URL pattern filtering
 * - Auto-scroll for lazy-loaded content
 * - Page module classification (header, footer, main, etc.)
 * - go_back for navigation control
 *
 * Usage:
 *   npx tsx scripts/playbook-record.ts <url> [options]
 *
 * Options:
 *   --scenario <text>     Page description/scenario (required)
 *   --pattern <regex>     Target URL pattern (optional, e.g., "^/search")
 *   --no-scroll           Disable auto-scroll to bottom
 *   --headless            Run in headless mode
 *   --output <dir>        Output directory (default: ./output)
 *
 * Examples:
 *   npx tsx scripts/playbook-record.ts "https://www.airbnb.com/" --scenario "Airbnb homepage with search form"
 *   npx tsx scripts/playbook-record.ts "https://example.com/search" --scenario "Search results page" --pattern "^/search"
 */

import { ActionBuilder } from "../src/ActionBuilder.js";
import type { StepEvent } from "../src/types/index.js";

// Simple argument parsing
function parseArgs(): {
  url: string;
  scenario: string;
  pattern?: string;
  autoScroll: boolean;
  headless: boolean;
  outputDir: string;
} {
  const args = process.argv.slice(2);

  if (args.length === 0 || args[0] === "--help" || args[0] === "-h") {
    console.log(`
Playbook Mode Record Script

Usage:
  npx tsx scripts/playbook-record.ts <url> [options]

Options:
  --scenario <text>     Page description/scenario (required)
  --pattern <regex>     Target URL pattern (optional)
  --no-scroll           Disable auto-scroll to bottom
  --headless            Run in headless mode
  --output <dir>        Output directory (default: ./output)

Examples:
  npx tsx scripts/playbook-record.ts "https://www.airbnb.com/" --scenario "Airbnb homepage"
  npx tsx scripts/playbook-record.ts "https://example.com/search" --scenario "Search page" --pattern "^/search"
`);
    process.exit(0);
  }

  const url = args[0];
  let scenario = "";
  let pattern: string | undefined;
  let autoScroll = true;
  let headless = false;
  let outputDir = "./output";

  for (let i = 1; i < args.length; i++) {
    switch (args[i]) {
      case "--scenario":
        scenario = args[++i] || "";
        break;
      case "--pattern":
        pattern = args[++i];
        break;
      case "--no-scroll":
        autoScroll = false;
        break;
      case "--headless":
        headless = true;
        break;
      case "--output":
        outputDir = args[++i] || "./output";
        break;
    }
  }

  if (!scenario) {
    console.error("Error: --scenario is required");
    process.exit(1);
  }

  return { url, scenario, pattern, autoScroll, headless, outputDir };
}

async function runPlaybookRecord(): Promise<void> {
  const config = parseArgs();

  console.log("=".repeat(60));
  console.log("Playbook Mode Recording");
  console.log("=".repeat(60));
  console.log(`URL: ${config.url}`);
  console.log(`Scenario: ${config.scenario}`);
  if (config.pattern) {
    console.log(`Target Pattern: ${config.pattern}`);
  }
  console.log(`Auto Scroll: ${config.autoScroll}`);
  console.log(`Headless: ${config.headless}`);
  console.log(`Output: ${config.outputDir}`);
  console.log("=".repeat(60));

  // Track progress
  let stepCount = 0;
  const moduleStats: Record<string, number> = {};

  const builder = new ActionBuilder({
    outputDir: config.outputDir,
    headless: config.headless,
    maxTurns: 40, // Increased for complex pages with many elements
    databaseUrl: process.env.DATABASE_URL,
    onStepFinish: (event: StepEvent) => {
      stepCount++;
      const status = event.success ? "\u2705" : "\u274c";
      console.log(`\n${status} Step ${stepCount}: ${event.toolName} (${event.durationMs}ms)`);

      // Track module stats from register_element calls
      if (event.toolName === "register_element" && event.success) {
        const args = event.toolArgs as { module?: string; element_id?: string };
        const module = args.module || "unknown";
        moduleStats[module] = (moduleStats[module] || 0) + 1;
        console.log(`   Element: ${args.element_id} [${module}]`);
      } else if (event.toolName === "scroll_to_bottom") {
        console.log(`   Scrolled to bottom for lazy loading`);
      } else if (event.toolName === "go_back") {
        console.log(`   Navigated back`);
      } else if (event.toolName === "observe_page") {
        const args = event.toolArgs as { focus?: string; module?: string };
        console.log(`   Focus: ${args.focus || "all"}`);
        if (args.module) {
          console.log(`   Module: ${args.module}`);
        }
      }

      if (event.error) {
        console.log(`   Error: ${event.error}`);
      }
    },
  });

  // Generate domain name from URL
  const urlObj = new URL(config.url);
  const domainName = urlObj.hostname.replace(/^www\./, "").replace(/\./g, "_");
  const scenarioId = `${domainName}_playbook_${Date.now()}`;

  try {
    await builder.initialize();

    // Use default CAPABILITY_RECORDER_SYSTEM_PROMPT + generateUserPrompt
    // Pass config.scenario as scenarioDescription
    const result = await builder.build(config.url, scenarioId, {
      siteName: urlObj.hostname,
      siteDescription: config.scenario,
      scenarioDescription: config.scenario,
      // Playbook mode options
      targetUrlPattern: config.pattern,
      autoScrollToBottom: config.autoScroll,
    });

    console.log("\n" + "=".repeat(60));
    console.log("Recording Results");
    console.log("=".repeat(60));

    if (result.success) {
      console.log("\u2705 Playbook recording completed!");
    } else {
      console.log("\u26a0\ufe0f Recording finished with issues");
    }

    console.log(`\ud83d\udcc1 Saved to: ${result.savedPath}`);
    console.log(`\ud83d\udd04 Turns used: ${result.turns}`);
    console.log(`\ud83d\udcb0 Tokens: input=${result.tokens.input}, output=${result.tokens.output}, total=${result.tokens.total}`);
    console.log(`\u23f1\ufe0f Duration: ${result.totalDuration}ms`);
    console.log(`\ud83d\udcca Steps: ${stepCount}`);

    // Module statistics
    if (Object.keys(moduleStats).length > 0) {
      console.log(`\n\ud83c\udfe0 Elements by Module:`);
      for (const [module, count] of Object.entries(moduleStats).sort((a, b) => b[1] - a[1])) {
        console.log(`   ${module}: ${count}`);
      }
    }

    // Capability summary
    if (result.siteCapability) {
      const cap = result.siteCapability;
      console.log(`\n\ud83d\udcca Capability Summary:`);
      console.log(`   Domain: ${cap.domain}`);
      console.log(`   Pages: ${Object.keys(cap.pages).length}`);

      let totalElements = Object.keys(cap.global_elements).length;
      for (const page of Object.values(cap.pages)) {
        totalElements += Object.keys(page.elements).length;
      }
      console.log(`   Total Elements: ${totalElements}`);

      // Show elements with module info
      for (const [pageType, page] of Object.entries(cap.pages)) {
        console.log(`\n   \ud83d\udcc4 Page: ${pageType}`);
        for (const [elementId, element] of Object.entries(page.elements)) {
          const module = element.module || "unknown";
          console.log(`      [${module}] ${elementId}: ${element.element_type}`);
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

    await builder.close();
    process.exit(result.success ? 0 : 1);
  } catch (error) {
    console.error("Fatal error:", error);
    await builder.close();
    process.exit(1);
  }
}

// Run
runPlaybookRecord().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});
