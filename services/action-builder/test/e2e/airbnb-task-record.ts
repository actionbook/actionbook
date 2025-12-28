#!/usr/bin/env npx tsx
/**
 * Airbnb Task-Driven Record Test
 *
 * Task-driven recording: Record UI elements while performing specific tasks
 *
 * Task: Search for hotels near Tokyo Shibuya Station 10 days from now
 *
 * Environment:
 *   Set ONE of: OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY
 *   AIClient auto-detects provider from available API keys.
 */

import { ActionBuilder } from "../../src/ActionBuilder.js";
import type { StepEvent } from "../../src/types/index.js";
import {
  loadEnv,
  requireLLMApiKey,
  getDetectedProvider,
  getStagehandInfo,
} from "../helpers/env-loader.js";

// Load environment and validate
loadEnv();
requireLLMApiKey();

// Task-driven recording System Prompt
const TASK_DRIVEN_SYSTEM_PROMPT = `You are a web automation agent that performs tasks while recording UI element capabilities.

## Your Goal
Execute the given task on the website. For EVERY element you interact with, capture its capability (selectors, description, methods) using the interact tool.

## Available Tools

- **navigate**: Go to a URL
- **interact**: Perform an action on an element AND automatically capture its capability
- **set_page_context**: Set the current page type for organizing recorded elements
- **wait**: Wait for content or time
- **scroll**: Scroll the page

## Key Rules

1. **DO NOT use observe_page** - it has API issues. Instead, directly use interact with clear instructions
2. **ALWAYS use interact for every action** - this captures the element's capability automatically
3. **Provide semantic element_id** - e.g., "search_location_input", "checkin_date_picker"
4. **Provide clear element_description** - what the element does, for documentation

## interact Tool Usage

When calling interact, provide:
- element_id: A semantic identifier (snake_case, e.g., "search_submit_button")
- action: "click", "type", "clear", or "hover"
- instruction: Natural language instruction for Stagehand to find and act on the element
- value: (optional) Text to type if action is "type"
- element_description: What this element does (for capability documentation)

## Element Naming Convention

Use descriptive snake_case names:
- search_location_input (not "input1")
- checkin_date_picker (not "date_button")
- search_submit_button (not "button")
- guest_count_adults_increment (not "plus_btn")

## Task Execution Strategy

1. Navigate to the starting page
2. Set page context
3. Execute each step using interact
4. When page changes significantly, update page context
5. Continue until task is complete or you've recorded all relevant elements. Once the core interactions are finished and required elements are captured, you may conclude instead of exploring extra actions.

Remember: The goal is to COMPLETE THE TASK while RECORDING every element you touch.`;

// Calculate date 10 days from now
function getDateAfterDays(days: number): { formatted: string; year: number; month: number; day: number } {
  const date = new Date();
  date.setDate(date.getDate() + days);
  return {
    formatted: date.toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric" }),
    year: date.getFullYear(),
    month: date.getMonth() + 1,
    day: date.getDate(),
  };
}

async function runTaskDrivenRecord(): Promise<void> {
  // ActionBuilder uses AIClient with multi-provider support
  // AIClient auto-detects provider from: OPENROUTER_API_KEY > OPENAI_API_KEY > ANTHROPIC_API_KEY

  const { provider, model } = getDetectedProvider();
  const stagehandInfo = getStagehandInfo();

  console.log(`AIClient: ${provider} with ${model}`);
  console.log(`Stagehand: ${stagehandInfo}`);

  // Calculate dates
  const checkinDate = getDateAfterDays(10);
  const checkoutDate = getDateAfterDays(12);

  console.log("=".repeat(60));
  console.log("Airbnb Task-Driven Record Test");
  console.log("=".repeat(60));
  console.log(`Task: Search hotels near Tokyo Shibuya Station`);
  console.log(`Check-in: ${checkinDate.formatted}`);
  console.log(`Check-out: ${checkoutDate.formatted}`);
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);
  console.log("=".repeat(60));

  // Step counter for real-time feedback
  let stepCount = 0;
  const interactedElements: string[] = [];

  // AIClient auto-detects provider from environment variables
  const builder = new ActionBuilder({
    outputDir: "./output",
    headless: false,
    maxTurns: 30,
    databaseUrl: process.env.DATABASE_URL,
    // Real-time step feedback
    onStepFinish: (event: StepEvent) => {
      stepCount++;
      const status = event.success ? "✅" : "❌";
      const duration = `${event.durationMs}ms`;

      // Track interacted elements
      if (event.toolName === "interact" && event.success) {
        const elementId = (event.toolArgs as { element_id?: string }).element_id;
        if (elementId && !interactedElements.includes(elementId)) {
          interactedElements.push(elementId);
        }
      }

      console.log(`\n${status} Step ${stepCount}: ${event.toolName} (${duration})`);

      // Show tool arguments preview
      if (event.toolName === "interact") {
        const args = event.toolArgs as { element_id?: string; action?: string; instruction?: string };
        console.log(`   Element: ${args.element_id || "unknown"}`);
        console.log(`   Action: ${args.action || "unknown"}`);
        if (args.instruction) {
          console.log(`   Instruction: ${args.instruction.substring(0, 60)}...`);
        }
      } else if (event.toolName === "navigate") {
        const args = event.toolArgs as { url?: string };
        console.log(`   URL: ${args.url}`);
      }

      if (event.error) {
        console.log(`   ❌ Error: ${event.error}`);
      }
    },
  });

  // Task prompt
  const taskPrompt = `## Task: Search for hotels near Tokyo Shibuya Station

**Dates:**
- Check-in: ${checkinDate.formatted} (${checkinDate.year}-${String(checkinDate.month).padStart(2, "0")}-${String(checkinDate.day).padStart(2, "0")})
- Check-out: ${checkoutDate.formatted} (${checkoutDate.year}-${String(checkoutDate.month).padStart(2, "0")}-${String(checkoutDate.day).padStart(2, "0")})

**Location:** Tokyo Shibuya Station, Japan

**Steps to complete:**

1. Navigate to https://www.airbnb.com/
2. Set page context as "home"
3. Click on the location/destination search field (interact with element_id: search_location_input)
4. Type "Tokyo Shibuya Station" (interact with same element, action: type)
5. Wait for suggestions to appear
6. Click on the first suggestion (interact with element_id: location_suggestion_item)
7. Click on the check-in date field (interact with element_id: checkin_date_picker)
8. Navigate calendar to the correct month if needed
9. Click on the check-in date: ${checkinDate.day} (interact with element_id: calendar_date_${checkinDate.day})
10. Click on the check-out date: ${checkoutDate.day} (interact with element_id: calendar_date_${checkoutDate.day})
11. Click the search button (interact with element_id: search_submit_button)
12. Wait for results to load
13. Set page context as "search_results"
14. Record any visible filter elements on the results page

**Important:**
- Use interact for EVERY action to record the element capability
- Provide descriptive element_id names
- If an action fails, try alternative approaches
- Record all elements you interact with, they will be saved to the capability store

Today's date: ${new Date().toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric" })}`;

  try {
    await builder.initialize();

    // Use custom prompts for task-driven recording
    const result = await builder.build(
      "https://www.airbnb.com/",
      "airbnb_hotel_search_tokyo",
      {
        siteName: "Airbnb",
        siteDescription: "Accommodation booking platform - Tokyo Shibuya search flow",
        focusAreas: ["search form", "date picker", "location suggestions", "search results"],
        // Task-driven mode: custom prompts
        customSystemPrompt: TASK_DRIVEN_SYSTEM_PROMPT,
        customUserPrompt: taskPrompt,
      }
    );

    console.log("\n" + "=".repeat(60));
    console.log("Recording Results");
    console.log("=".repeat(60));

    if (result.success) {
      console.log("✅ Task-driven recording completed!");
    } else {
      console.log("⚠️ Recording finished (may not have completed all steps)");
    }

    console.log(`📁 Saved to: ${result.savedPath}`);
    console.log(`🔄 Turns used: ${result.turns}`);
    console.log(`💰 Tokens used: ${result.totalTokens}`);
    console.log(`⏱️ Duration: ${result.totalDuration}ms`);
    console.log(`📊 Total steps: ${stepCount}`);
    console.log(`🎯 Elements recorded: ${interactedElements.length}`);

    if (interactedElements.length > 0) {
      console.log(`\n📋 Interacted Elements:`);
      interactedElements.forEach((el, i) => {
        console.log(`   ${i + 1}. ${el}`);
      });
    }

    if (result.siteCapability) {
      const cap = result.siteCapability;
      console.log(`\n📊 Capability Summary:`);
      console.log(`   Domain: ${cap.domain}`);
      console.log(`   Pages: ${Object.keys(cap.pages).length}`);
      console.log(`   Global Elements: ${Object.keys(cap.global_elements).length}`);

      let totalElements = Object.keys(cap.global_elements).length;
      for (const page of Object.values(cap.pages)) {
        totalElements += Object.keys(page.elements).length;
      }
      console.log(`   Total Elements: ${totalElements}`);

      // Show recorded elements per page
      for (const [pageType, page] of Object.entries(cap.pages)) {
        const elementCount = Object.keys(page.elements).length;
        console.log(`\n   📄 Page: ${pageType} (${elementCount} elements)`);
        for (const [elementId, element] of Object.entries(page.elements)) {
          console.log(`      - ${elementId}: ${element.description?.substring(0, 50) || "No description"}`);
        }
      }
    }

    // Run validation
    if (result.siteCapability) {
      console.log("\n" + "=".repeat(60));
      console.log("Running Validation...");
      console.log("=".repeat(60));

      try {
        const validationResult = await builder.validate(result.siteCapability.domain, {
          verbose: true,
        });

        console.log(`\n🔍 Validation Results:`);
        console.log(`   Valid: ${validationResult.validElements}/${validationResult.totalElements}`);
        console.log(`   Rate: ${(validationResult.validationRate * 100).toFixed(1)}%`);
      } catch (validationError) {
        console.error("❌ Validation failed:", validationError);
      }
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
runTaskDrivenRecord().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});

