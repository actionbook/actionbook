#!/usr/bin/env npx tsx
/**
 * Validate First Round Companies selectors test
 *
 * This script validates the recorded selectors against the actual page DOM.
 * It also performs manual DOM inspection to identify correct selectors.
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

    console.log("🔍 Validating www.firstround.com selectors...\n");
    const result = await builder.validate("www.firstround.com", { verbose: true });

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

    // Additional DOM inspection for First Round specific elements
    console.log("\n🔬 Manual DOM Inspection for Company Card Structure...\n");
    await inspectCompanyCardDOM(builder);

    // Re-validate after expanding card
    console.log("\n" + "=".repeat(60));
    console.log("🔄 Re-validating after expanding company card...\n");
    const result2 = await builder.validate("www.firstround.com", { verbose: true });
    console.log(`\n📊 Post-expansion Validation: ${result2.validElements}/${result2.totalElements} valid (${(result2.validationRate * 100).toFixed(1)}%)`);

    await builder.close();
    process.exit(result.success ? 0 : 1);
  } catch (error) {
    console.error("Fatal error:", error);
    await builder.close();
    process.exit(1);
  }
}

/**
 * Manually inspect the DOM structure of company cards
 * to identify correct selectors for data fields
 */
async function inspectCompanyCardDOM(builder: ActionBuilder): Promise<void> {
  const browser = (builder as any).browser;
  if (!browser) {
    console.log("  ⚠️ Browser not available for DOM inspection");
    return;
  }

  // Use Stagehand's act method to interact
  console.log("  📍 Navigating to companies page...");
  await browser.navigate("https://www.firstround.com/companies?category=all");
  await new Promise(resolve => setTimeout(resolve, 3000));

  // Get the page for DOM inspection
  const page = await browser.getPage();

  // Click the first company card to expand it using direct page interaction
  console.log("  🖱️ Expanding first company card...");
  try {
    // Use page.evaluate to click the first expand button
    const clickResult = await page.evaluate(() => {
      const buttons = document.querySelectorAll(".company-list-card-small__button");
      if (buttons.length > 0) {
        const firstButton = buttons[0] as HTMLElement;
        const beforeState = firstButton.getAttribute("aria-expanded");
        firstButton.click();
        return { clicked: true, beforeState, buttonCount: buttons.length };
      }
      return { clicked: false, buttonCount: 0 };
    });
    console.log("  Click result:", JSON.stringify(clickResult));
    await new Promise(resolve => setTimeout(resolve, 3000)); // Wait longer for animation
    console.log("  ✅ Clicked expand button");
  } catch (e) {
    console.log("  ⚠️ Could not expand company card:", e);
  }

  // Check if card is expanded
  const isExpanded = await page.evaluate(() => {
    const expandedCard = document.querySelector(".company-list-card-small__button[aria-expanded='true']");
    const expandedContent = document.querySelector(".company-list-company-info, [class*='card-expanded'], [class*='open']");
    const allButtons = document.querySelectorAll(".company-list-card-small__button");
    const expandedButtons = Array.from(allButtons).filter(b => b.getAttribute("aria-expanded") === "true");
    return {
      totalButtons: allButtons.length,
      expandedButtonCount: expandedButtons.length,
      hasExpandedContent: !!expandedContent,
      firstButtonState: allButtons[0]?.getAttribute("aria-expanded") || null,
      firstLiHTML: document.querySelector(".company-list li")?.innerHTML?.substring(0, 800) || null,
    };
  });
  console.log("  Expansion check:", JSON.stringify(isExpanded, null, 2));

  // Inspect the DOM structure
  console.log("\n  📝 Inspecting DOM structure of expanded company card:\n");

  const domInfo = await page.evaluate(() => {
    const results: Record<string, any> = {};

    // Check for company list
    const companyList = document.querySelector(".company-list, ul.company-list");
    results.companyList = {
      found: !!companyList,
      selector: companyList ? companyList.className : null,
      childCount: companyList ? companyList.children.length : 0,
    };

    // Check for company cards
    const companyCards = document.querySelectorAll(".company-list-company, .company-list > li");
    results.companyCards = {
      found: companyCards.length > 0,
      count: companyCards.length,
      firstCardClasses: companyCards[0]?.className || null,
    };

    // Check for expanded card content
    const expandedContent = document.querySelector("[aria-expanded='true'], .company-list-company--expanded");
    results.expandedContent = {
      found: !!expandedContent,
    };

    // Check for info items structure
    const infoItems = document.querySelectorAll(".company-list-company-info__item");
    results.infoItems = {
      found: infoItems.length > 0,
      count: infoItems.length,
      structure: [] as any[],
    };

    // Analyze each info item
    infoItems.forEach((item, index) => {
      const label = item.querySelector(".company-list-company-info__label, dt");
      const value = item.querySelector(".company-list-company-info__value, dd");
      results.infoItems.structure.push({
        index,
        labelText: label?.textContent?.trim() || null,
        valueText: value?.textContent?.trim()?.substring(0, 50) || null,
        labelSelector: label?.className || null,
        valueSelector: value?.className || null,
      });
    });

    // Check for specific field selectors (the ones we recorded)
    const testSelectors = [
      // Original selectors
      ".company-list-company-info__name",
      ".company-list-company-info__item",
      ".company-list-company-info__label",
      ".company-list-company-info__value",
      // Actual selectors based on DOM inspection
      ".company-list-card-small",
      ".company-list-card-small__button",
      ".company-list-card-small__button-name",
      ".company-list-card-small__button-statement",
      ".company-list-card-expanded",
      ".company-list-card-expanded__info",
      "[aria-expanded='true']",
      // Filter selectors
      "fieldset label",
      "fieldset input[type='radio']",
    ];

    results.selectorTests = {};
    for (const selector of testSelectors) {
      const elements = document.querySelectorAll(selector);
      results.selectorTests[selector] = {
        found: elements.length > 0,
        count: elements.length,
        firstText: elements[0]?.textContent?.trim()?.substring(0, 30) || null,
      };
    }

    // Get the actual HTML structure of first company card info section
    const infoSection = document.querySelector(".company-list-company-info");
    results.infoSectionHTML = infoSection?.innerHTML?.substring(0, 1000) || null;

    // Get the tag name of info items
    const firstInfoItem = document.querySelector(".company-list-company-info__item");
    results.infoItemTagName = firstInfoItem?.tagName || null;
    results.infoItemOuterHTML = firstInfoItem?.outerHTML?.substring(0, 500) || null;

    // Test XPath selectors directly
    const xpathTests: Record<string, any> = {};
    const xpathSelectors = [
      "//div[contains(@class, 'company-list-company-info__item')]",
      "//*[contains(@class, 'company-list-company-info__item')]",
      "//div[contains(@class, 'company-list-company-info__item')][.//dt[contains(text(), 'Founders')]]//dd",
      "//*[contains(@class, 'company-list-company-info__item')][.//dt[contains(text(), 'Founders')]]//dd",
      "//dt[contains(@class, 'company-list-company-info__label')]",
      "//dd[contains(@class, 'company-list-company-info__value')]",
    ];
    for (const xpath of xpathSelectors) {
      try {
        const result = document.evaluate(xpath, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null);
        const node = result.singleNodeValue;
        xpathTests[xpath] = {
          found: !!node,
          text: node?.textContent?.substring(0, 50) || null,
          tagName: (node as Element)?.tagName || null,
        };
      } catch (e) {
        xpathTests[xpath] = { error: String(e) };
      }
    }
    results.xpathTests = xpathTests;

    // Get actual class names
    const infoItem = document.querySelector(".company-list-company-info__item");
    results.actualClassName = infoItem?.className || null;

    return results;
  });

  // Print results
  console.log("  Company List:", JSON.stringify(domInfo.companyList, null, 2));
  console.log("\n  Company Cards:", JSON.stringify(domInfo.companyCards, null, 2));
  console.log("\n  Expanded Content:", JSON.stringify(domInfo.expandedContent, null, 2));
  console.log("\n  Info Items:", JSON.stringify(domInfo.infoItems, null, 2));
  console.log("\n  Selector Tests:");
  for (const [selector, result] of Object.entries(domInfo.selectorTests)) {
    const r = result as any;
    const status = r.found ? "✅" : "❌";
    console.log(`    ${status} ${selector}: ${r.count} elements ${r.firstText ? `("${r.firstText}")` : ""}`);
  }

  // Print XPath tests
  console.log("\n  Info Item Tag Name:", domInfo.infoItemTagName);
  console.log("  Actual Class Name:", domInfo.actualClassName);
  console.log("\n  XPath Tests:");
  for (const [xpath, result] of Object.entries(domInfo.xpathTests || {})) {
    const r = result as any;
    if (r.error) {
      console.log(`    ❌ ${xpath.substring(0, 60)}...: Error - ${r.error}`);
    } else {
      const status = r.found ? "✅" : "❌";
      console.log(`    ${status} ${xpath.substring(0, 60)}...: ${r.found ? `found (${r.tagName}: "${r.text}")` : "not found"}`);
    }
  }

  // Suggest correct selectors based on DOM inspection
  console.log("\n" + "=".repeat(60));
  console.log("💡 Suggested Correct Selectors Based on DOM Inspection:");
  console.log("=".repeat(60));

  if (domInfo.infoItems.structure.length > 0) {
    console.log("\n  The actual DOM structure uses generic .company-list-company-info__item");
    console.log("  with label/value pairs. To extract specific fields, use:");
    console.log("");

    for (const item of domInfo.infoItems.structure) {
      if (item.labelText) {
        const fieldName = item.labelText.toLowerCase().replace(/\s+/g, "_");
        console.log(`  ${item.labelText}:`);
        console.log(`    Selector: .company-list-company-info__item:has(.company-list-company-info__label:contains("${item.labelText}")) .company-list-company-info__value`);
        console.log(`    Or use XPath: //div[contains(@class, 'company-list-company-info__item')][.//dt[contains(text(), '${item.labelText}')]]//dd`);
        console.log("");
      }
    }
  }
}

validate().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});
