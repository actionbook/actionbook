#!/usr/bin/env npx tsx
/**
 * First Round Companies Crawler with Action Recording
 *
 * Task: Crawl all company information from First Round website
 * Target data: company name, tagline, founders, initial partnership, categories, partner, locations
 *
 * Also records page element capabilities for subsequent action recording
 *
 * Environment:
 *   Set ONE of: OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY
 *   AIClient auto-detects provider from available API keys.
 */

import { ActionBuilder } from "../../src/ActionBuilder.js";
import type { StepEvent } from "../../src/types/index.js";
import * as fs from "fs";
import * as path from "path";
import {
  loadEnv,
  requireLLMApiKey,
  getDetectedProvider,
} from "../helpers/env-loader.js";

// Load environment and validate
loadEnv();
requireLLMApiKey();

// Company information interface
interface CompanyInfo {
  name: string;
  tagline: string;
  founders: string;
  initialPartnership: string;
  categories: string;
  partner: string;
  locations: string;
}

// Task-driven recording System Prompt
const TASK_DRIVEN_SYSTEM_PROMPT = `You are a web automation agent that records UI element capabilities for AI-driven data extraction.

## Your Goal
Record ALL elements on the page that would help an AI agent understand the page structure and extract data. This includes:
1. **Interactive elements** (buttons, inputs, filters) - for navigation and triggering content
2. **Data display elements** (text fields, data containers) - for data extraction
3. **Element relationships** (parent-child, dependencies) - for understanding page structure

## Available Tools

- **navigate**: Go to a URL
- **interact**: Perform an action on an element AND automatically capture its capability
- **set_page_context**: Set the current page type for organizing recorded elements
- **wait**: Wait for content or time
- **scroll**: Scroll the page
- **register_element**: Register an element (interactive OR data display) without interacting with it

## Key Rules

1. **DO NOT use observe_page** - it has API issues. Instead, directly use interact or register_element
2. **Record BOTH interactive AND data elements** - AI agents need to know where data is located
3. **Specify element relationships** - use depends_on, parent, visibility_condition fields
4. **Use data_key for extractable fields** - e.g., data_key="founders" for the founders field

## register_element Tool - CRITICAL for Data Extraction

Use register_element to record data display elements. Key parameters:
- element_id: Semantic ID (e.g., "company_founders_field", "company_name_text")
- element_type: Use "data_field" for extractable data, "container" for grouping, "list" for repeating items
- allow_methods: Use ["extract"] for data fields
- data_key: The key name for extraction (e.g., "founders", "categories", "partner")
- depends_on: Element ID that must be clicked first (e.g., "company_card_expand_button")
- visibility_condition: When this element is visible (e.g., "after_click:company_card_expand_button")
- parent: Parent container element ID
- is_repeating: true if this pattern repeats (e.g., each company card has same structure)

## Element Types

- **button, input, link, select, checkbox, radio**: Interactive elements
- **text**: Static text element
- **data_field**: Extractable data field (use with data_key)
- **container**: Groups other elements
- **list**: Container with repeating items
- **list_item**: Single item in a list (use is_repeating=true)

## Element Naming Convention

Use descriptive snake_case names:
- company_card (container)
- company_card_expand_button (button)
- company_name_field (data_field, data_key="name")
- company_founders_field (data_field, data_key="founders")
- company_list (list)

## Task Execution Strategy

1. Navigate to the starting page
2. Set page context
3. Scroll to load all content
4. **Register the page structure elements** (containers, lists)
5. **Interact with elements to reveal hidden content** (expand buttons)
6. **Register ALL data fields** with proper data_key and depends_on
7. Document element relationships in descriptions

Remember: The recorded capability should enable an AI agent to:
1. Understand the page structure
2. Know which elements to click to reveal data
3. Know exactly where each data field is located
4. Extract data using the recorded selectors and data_keys`;

// User task Prompt
const TASK_PROMPT = `## Task: Record UI elements for data extraction on First Round Companies page

**Target URL:** https://www.firstround.com/companies?category=all

**Goal:** Record ALL elements needed for an AI agent to extract company data from this page.

**Data to extract per company:**
- Company name
- Tagline (one-sentence description)
- Founders
- Initial Partnership (e.g., Seed, Series A)
- Categories (e.g., Consumer, Enterprise, AI)
- Partner name
- Locations
- URL

**Steps to complete:**

1. Navigate to https://www.firstround.com/companies?category=all
2. Set page context as "companies_list"
3. Wait for the page to load completely

4. **Register page structure elements:**
   - register_element: company_list (element_type="list", description="List of all portfolio companies")
   - register_element: company_card (element_type="list_item", is_repeating=true, parent="company_list")

5. **Interact with a company card to expand it:**
   - interact: Click on the first company card expand button
   - Wait for the expanded content to appear

6. **Register ALL data fields (CRITICAL):**
   After expanding a card, register each data field with:
   - element_type="data_field"
   - allow_methods=["extract"]
   - data_key (the field name for extraction)
   - depends_on="company_card_expand_button"
   - visibility_condition="after_click:company_card_expand_button"
   - is_repeating=true (same pattern in every card)

   Required data fields to register:
   - company_name_field (data_key="name")
   - company_tagline_field (data_key="tagline")
   - company_founders_field (data_key="founders")
   - company_initial_partnership_field (data_key="initial_partnership")
   - company_categories_field (data_key="categories")
   - company_partner_field (data_key="partner")
   - company_locations_field (data_key="locations")

7. **Register category filter buttons:**
   - category_filter_all, category_filter_enterprise, etc.

8. Scroll back to top and summarize recorded elements

**Example register_element call for a data field:**
\`\`\`
register_element(
  element_id="company_founders_field",
  description="Founders names - visible after expanding company card. Contains comma-separated founder names.",
  element_type="data_field",
  allow_methods=["extract"],
  css_selector=".company-list-company-info__value",
  data_key="founders",
  depends_on="company_card_expand_button",
  visibility_condition="after_click:company_card_expand_button",
  parent="company_card",
  is_repeating=true
)
\`\`\`

**Important:**
- Use register_element for data fields, interact for buttons
- ALWAYS specify data_key for data_field elements
- ALWAYS specify depends_on when element requires prior interaction
- Use is_repeating=true for elements that repeat in each company card

Today's date: ${new Date().toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric" })}`;

async function crawlFirstRoundCompanies(): Promise<void> {
  const { provider, model } = getDetectedProvider();

  console.log("=".repeat(60));
  console.log("First Round Companies Crawler with Action Recording");
  console.log("=".repeat(60));
  console.log(`Target: https://www.firstround.com/companies?category=all`);
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);
  console.log("=".repeat(60));

  let stepCount = 0;
  const interactedElements: string[] = [];

  // AIClient auto-detects provider from environment variables
  const builder = new ActionBuilder({
    outputDir: "./output",
    headless: false,
    maxTurns: 30,
    databaseUrl: process.env.DATABASE_URL,
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

      if (event.toolName === "interact") {
        const args = event.toolArgs as { element_id?: string; action?: string; instruction?: string };
        console.log(`   Element: ${args.element_id || "unknown"}`);
        console.log(`   Action: ${args.action || "unknown"}`);
      } else if (event.toolName === "navigate") {
        const args = event.toolArgs as { url?: string };
        console.log(`   URL: ${args.url}`);
      } else if (event.toolName === "set_page_context") {
        const args = event.toolArgs as { page_type?: string };
        console.log(`   Page: ${args.page_type}`);
      }

      if (event.error) {
        console.log(`   ❌ Error: ${event.error}`);
      }
    },
  });

  try {
    await builder.initialize();

    // Use ActionBuilder for task-driven recording
    const result = await builder.build(
      "https://www.firstround.com/companies?category=all",
      "firstround_companies",
      {
        siteName: "First Round Capital",
        siteDescription: "First Round Capital portfolio companies page - company list and details",
        focusAreas: ["company cards", "category filters", "company details", "navigation"],
        customSystemPrompt: TASK_DRIVEN_SYSTEM_PROMPT,
        customUserPrompt: TASK_PROMPT,
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

    console.log(`📁 Capability saved to: ${result.savedPath}`);
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

    // Now extract company data using browser
    console.log("\n" + "=".repeat(60));
    console.log("Extracting Company Data...");
    console.log("=".repeat(60));

    // Get browser page and extract data
    // Note: ActionBuilder has closed the browser, we need to reopen it to extract data
    // Or we can extract data during the recording process

    // Save extracted company data (if any)
    const crawlDir = path.resolve(process.cwd(), "crawl");
    if (!fs.existsSync(crawlDir)) {
      fs.mkdirSync(crawlDir, { recursive: true });
    }

    // Since ActionBuilder is mainly for recording elements, we need to run data extraction separately
    // Here we prompt the user that data extraction needs to be run separately
    console.log("\n📝 Note: Company data extraction requires a separate run.");
    console.log("   The capability recording is complete and saved to output/sites/");
    console.log("   Run the data extraction script separately if needed.");

    await builder.close();
    process.exit(result.success ? 0 : 1);
  } catch (error) {
    console.error("Fatal error:", error);
    await builder.close();
    process.exit(1);
  }
}

// Run the crawler
crawlFirstRoundCompanies().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});
