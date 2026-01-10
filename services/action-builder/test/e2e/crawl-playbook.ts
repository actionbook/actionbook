#!/usr/bin/env npx tsx
/**
 * Crawl Playbook - Simple Website Crawler + LLM Page Feature Summary
 *
 * Features:
 * - Input a URL and expand pages via links (max depth: 3)
 * - Only crawl internal links, ignore external links
 * - For each page, use LLM to summarize page features
 * - Save results as YAML/JSON files to output/sites/{domain}/ directory
 *
 * Usage:
 *   npx tsx test/e2e/crawl_playbook.ts https://example.com
 *   npx tsx test/e2e/crawl_playbook.ts https://example.com --output ./my-output
 *
 * Environment Variables:
 *   Set ONE of: OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY
 */

import fs from "fs";
import path from "path";
import { chromium, type Browser, type Page } from "playwright";
import YAML from "yaml";
import { AIClient } from "../../src/llm/AIClient.js";
import {
  loadEnv,
  requireLLMApiKey,
  getDetectedProvider,
} from "../helpers/env-loader.js";

// Load environment and validate
loadEnv();
requireLLMApiKey();

// Default configuration
const DEFAULT_MAX_DEPTH = 3;
const DEFAULT_MAX_PAGES = 30;
const DEFAULT_PAGE_LOAD_TIMEOUT = 30000;
const DEFAULT_OUTPUT_DIR = "./output";
const DEFAULT_HTML_LIMIT = 20000;
const DEFAULT_CONCURRENCY = 5;

// Runtime configuration (set by parseArgs)
interface Config {
  maxDepth: number;
  maxPages: number;
  pageLoadTimeout: number;
  outputDir: string;
  htmlLimit: number;
  concurrency: number;
}

let config: Config = {
  maxDepth: DEFAULT_MAX_DEPTH,
  maxPages: DEFAULT_MAX_PAGES,
  pageLoadTimeout: DEFAULT_PAGE_LOAD_TIMEOUT,
  outputDir: DEFAULT_OUTPUT_DIR,
  htmlLimit: DEFAULT_HTML_LIMIT,
  concurrency: DEFAULT_CONCURRENCY,
};

// Page info structure (single page crawl result)
interface PageInfo {
  url: string;
  title: string;
  depth: number;
  playbook: string;  // Full 7-section Playbook Markdown
  links: string[];
  html?: string;     // Simplified HTML for reference
}

// Pattern parameter definition
interface PatternParam {
  name: string;
  description: string;
}

// URL pattern group from LLM analysis
interface PatternGroup {
  pattern: string;
  params: PatternParam[];
  urls: string[];
}

// Merged page info (after pattern grouping)
interface MergedPageInfo {
  url_pattern: string;
  pattern_params?: PatternParam[];
  matched_urls?: string[];    // Up to 3 example URLs
  matched_count?: number;     // Total matched URLs count
  title: string;
  depth: number;
  playbook: string;
  links: string[];
}

// Merged result structure (after pattern grouping)
interface MergedCrawlResult {
  startUrl: string;
  domain: string;
  totalPages: number;
  uniquePatterns: number;
  pages: MergedPageInfo[];
}

/**
 * Extract domain from URL
 */
function extractDomain(url: string): string {
  try {
    const urlObj = new URL(url);
    return urlObj.hostname;
  } catch {
    return "";
  }
}

/**
 * Normalize URL (remove hash, unify trailing slash)
 */
function normalizeUrl(url: string, baseUrl: string): string | null {
  try {
    const fullUrl = new URL(url, baseUrl);
    // Remove hash
    fullUrl.hash = "";
    // Unify protocol
    if (fullUrl.protocol !== "http:" && fullUrl.protocol !== "https:") {
      return null;
    }
    return fullUrl.href;
  } catch {
    return null;
  }
}

/**
 * Check if URL is same domain
 */
function isSameDomain(url: string, domain: string): boolean {
  try {
    const urlDomain = extractDomain(url);
    return urlDomain === domain || urlDomain.endsWith(`.${domain}`);
  } catch {
    return false;
  }
}

/**
 * Check if URL should be crawled (exclude resource files, etc.)
 */
function shouldCrawl(url: string): boolean {
  const skipExtensions = [
    ".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".ico",
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".mp3", ".mp4", ".avi", ".mov", ".wmv",
    ".zip", ".tar", ".gz", ".rar",
    ".css", ".js", ".json", ".xml", ".woff", ".woff2", ".ttf", ".eot",
  ];

  const urlLower = url.toLowerCase();
  return !skipExtensions.some(ext => urlLower.endsWith(ext));
}

/**
 * Extract all links from page
 */
async function extractLinks(page: Page): Promise<string[]> {
  const links = await page.evaluate(() => {
    const anchors = document.querySelectorAll("a[href]");
    return Array.from(anchors).map(a => (a as HTMLAnchorElement).href);
  });
  return links;
}

/**
 * Get simplified HTML content from page (for LLM Playbook generation)
 * Removes scripts, styles, SVG, images, and inline styles
 */
async function getSimplifiedHTML(page: Page): Promise<string> {
  const html = await page.evaluate(() => {
    const clone = document.body.cloneNode(true) as HTMLElement;

    // Remove useless elements
    clone.querySelectorAll("script, style, noscript, svg, img, iframe, video, audio").forEach(el => el.remove());

    // Remove all inline styles
    clone.querySelectorAll("[style]").forEach(el => el.removeAttribute("style"));

    // Remove data-* attributes to reduce noise
    clone.querySelectorAll("*").forEach(el => {
      Array.from(el.attributes)
        .filter(attr => attr.name.startsWith("data-") && attr.name !== "data-testid")
        .forEach(attr => el.removeAttribute(attr.name));
    });

    return clone.outerHTML;
  });

  // Limit size
  return html.slice(0, config.htmlLimit);
}

/**
 * Playbook generation system prompt (from playbook_prompt.md)
 */
const PLAYBOOK_SYSTEM_PROMPT = `You are a senior web automation and crawler architect. Your goal is to deeply analyze the target webpage and generate a standardized "Playbook" document to guide AI Agents or automation scripts in understanding, parsing, and interacting with the page.

**Core Task**:
Analyze the structure and logic of specific page types (e.g., detail pages, list pages, documentation pages).

---

## Output Document Structure (Strict Format)

Generate a Markdown document strictly following these 7 sections:

### 0. Page URL

\${url}
- Query parameters: one per line, brief description
    - \${name}: \${description}
- Params: if URL contains dynamic parameters
    - \${name}: \${description}

### 1. Page Overview
*   **Definition**: Clearly define the core business objective of this page in one sentence.
*   *Example*: "Retrieve detailed changelog, release date, and categorized updates for a specific software version."

### 2. Page Function Summary
*   **Format**: Function list. Each function on one line with name and brief description (1-2 sentences).
*   *Example*:
    *   **Version Switching**: Allows users to quickly jump to other historical versions via sidebar or dropdown menu.
    *   **Content Search**: Provides keyword search capability for current document or site-wide content.

### 3. Page Structure Summary
*   **Definition**: Macro-level breakdown of page layout modules (e.g., Header, Sidebar, Main Content).
*   **Requirement**: Provide **brief DOM description** for each module (key CSS selectors or semantic tags).
*   *Example*:
    *   **Sidebar (\`aside.nav\`)**: Contains the complete version history navigation list.
    *   **Main Content (\`main > article\`)**: Holds the core document content and changelog.

### 4. DOM Structure Instance
*   **Core Task (Pattern Recognition)**: If the page has different states or layout variants (e.g., with/without images, published/unpublished), list them **by pattern** here.
*   **Content**: Provide simplified HTML code snippets, preserving key data nodes and hierarchy.

### 5. Parsing & Processing Summary
*   **Data Retrieval Scenarios**: Define how data is presented and how to extract it.
    *   **Direct Retrieval**: Data is in initial HTML (provide CSS/XPath selectors).
    *   **Post-Interaction Retrieval**: Requires clicking to expand, switching tabs, or scroll loading.
    *   **Implicit Retrieval**: Data is in \`<script>\` tags, JSON attributes, or Shadow DOM.
*   **Logic Recommendations**: Provide compatible parsing logic for the patterns discovered in Section 4.

Note: Keep content summarized, no need for exhaustive details.

### 6. Operation Summary
*   **Definition**: Interactive operations available for Agent execution on the page.
*   **Format**:
    *   **Operation Type**: (input / click / hover)
    *   **Target Element**: (provide selector)
    *   **Expected Result**: What changes after the operation (URL change / partial DOM refresh / modal popup).

Note: Keep content summarized, no need for exhaustive details.`;

/**
 * Generate Playbook for a single page using LLM
 */
async function generatePlaybook(
  aiClient: AIClient,
  url: string,
  title: string,
  html: string
): Promise<string> {
  const userPrompt = `Please analyze the following webpage and generate a Playbook:

URL: ${url}
Title: ${title}

Page HTML structure (simplified):
${html}`;

  try {
    const response = await aiClient.chat(
      [
        { role: "system", content: PLAYBOOK_SYSTEM_PROMPT },
        { role: "user", content: userPrompt },
      ],
      [] // No tools needed
    );

    const playbook = response.choices[0]?.message?.content || "";
    return playbook.trim() || "Playbook generation failed: empty response";
  } catch (error) {
    console.error(`  ❌ Playbook generation failed: ${error}`);
    // Retry once
    try {
      console.log(`  🔄 Retrying Playbook generation...`);
      const response = await aiClient.chat(
        [
          { role: "system", content: PLAYBOOK_SYSTEM_PROMPT },
          { role: "user", content: userPrompt },
        ],
        []
      );
      const playbook = response.choices[0]?.message?.content || "";
      return playbook.trim() || "Playbook generation failed after retry";
    } catch (retryError) {
      console.error(`  ❌ Retry also failed: ${retryError}`);
      return "Playbook generation failed";
    }
  }
}

/**
 * URL Pattern grouping system prompt
 */
const URL_PATTERN_SYSTEM_PROMPT = `You are a URL analysis expert. Analyze the given URL list, identify URLs with the same structure and group them.

Output requirements:
1. Return in JSON format
2. Each group contains: pattern (with variable placeholders), params (variable definitions), urls (list of matching URLs)
3. Variable names should be semantic (e.g., {version}, {slug}, {id})
4. Unique URLs (cannot be categorized) should have pattern as the URL itself
5. Different path prefixes should remain independent (/blog/ and /news/ should not merge)

Output format example:
{
  "patterns": [
    {
      "pattern": "https://example.com/",
      "params": [],
      "urls": ["https://example.com/"]
    },
    {
      "pattern": "https://example.com/docs/{version}/",
      "params": [
        {
          "name": "version",
          "description": "Version number, format like 1.94.0, 1.93.0"
        }
      ],
      "urls": [
        "https://example.com/docs/1.94.0/",
        "https://example.com/docs/1.93.0/"
      ]
    }
  ]
}`;

/**
 * Group URLs by pattern using LLM
 */
async function groupUrlsByPattern(
  aiClient: AIClient,
  urls: string[]
): Promise<PatternGroup[]> {
  // Sort URLs for better LLM analysis
  const sortedUrls = [...urls].sort();

  const userPrompt = `Please analyze the following URL list and group them:

${sortedUrls.map((url, i) => `${i + 1}. ${url}`).join("\n")}

Please return the grouping result in JSON format.`;

  try {
    const response = await aiClient.chat(
      [
        { role: "system", content: URL_PATTERN_SYSTEM_PROMPT },
        { role: "user", content: userPrompt },
      ],
      []
    );

    const content = response.choices[0]?.message?.content || "";

    // Extract JSON from response (handle markdown code blocks)
    const jsonMatch = content.match(/```(?:json)?\s*([\s\S]*?)```/) || [null, content];
    const jsonStr = jsonMatch[1]?.trim() || content.trim();

    const result = JSON.parse(jsonStr) as { patterns: PatternGroup[] };
    return result.patterns;
  } catch (error) {
    console.error(`  ❌ URL pattern grouping failed: ${error}`);
    // Retry once
    try {
      console.log(`  🔄 Retrying URL pattern grouping...`);
      const response = await aiClient.chat(
        [
          { role: "system", content: URL_PATTERN_SYSTEM_PROMPT },
          { role: "user", content: userPrompt },
        ],
        []
      );
      const content = response.choices[0]?.message?.content || "";
      const jsonMatch = content.match(/```(?:json)?\s*([\s\S]*?)```/) || [null, content];
      const jsonStr = jsonMatch[1]?.trim() || content.trim();
      const result = JSON.parse(jsonStr) as { patterns: PatternGroup[] };
      return result.patterns;
    } catch (retryError) {
      console.error(`  ❌ Retry also failed, falling back to individual URLs: ${retryError}`);
      // Fallback: each URL as its own pattern
      return urls.map(url => ({
        pattern: url,
        params: [],
        urls: [url],
      }));
    }
  }
}

/**
 * Playbook merge system prompt (follows playbook_prompt.md 7-section format)
 */
const PLAYBOOK_MERGE_SYSTEM_PROMPT = `You are a senior web automation architect. You need to merge multiple Playbooks for similar pages into one comprehensive Playbook.

## Output Format Requirements

Generate the merged Markdown document strictly following these 7 sections:

### 0. Page URL

\${url_pattern}
- Query parameters: one per line, brief description
    - \${name}: \${description}
- Params: if URL contains dynamic parameters
    - \${name}: \${description}
- Pages: one per line, just url
    - \${url1}
    - \${url2}
    - \${url3}

* hint: just keep the format, no need to hightlight format

### 1. Page Overview
Clearly define the core business objective of this page type in one sentence.

### 2. Page Function Summary
Function list, merging all functions discovered across pages.

### 3. Page Structure Summary
Macro-level breakdown of page layout modules, providing key CSS selectors.

### 4. DOM Structure Instance
**Core Task**: If different pages have different states or layout variants, list them **by pattern** (e.g., Pattern A, Pattern B).

### 5. Parsing & Processing Summary
Provide parsing logic recommendations compatible with all patterns.

### 6. Operation Summary
Interactive operations available for Agent execution on the page.

## Merge Requirements
1. Preserve all variant patterns (e.g., Stable vs Nightly)
2. URL section uses pattern format, list 3 examples
3. DOM structure section must cover all discovered patterns
4. Parsing logic must provide recommendations compatible with all patterns
5. Strictly maintain 7-section format`;

/**
 * Merge multiple Playbooks into one using LLM
 */
async function mergePlaybooks(
  aiClient: AIClient,
  pattern: string,
  params: PatternParam[],
  playbooks: { url: string; playbook: string }[]
): Promise<string> {
  // Select up to 3 playbooks randomly
  const selectedPlaybooks = playbooks.length <= 3
    ? playbooks
    : playbooks.sort(() => Math.random() - 0.5).slice(0, 3);

  const paramsStr = params.length > 0
    ? `Pattern parameters:\n${params.map(p => `  - ${p.name}: ${p.description}`).join("\n")}`
    : "";

  const playbooksStr = selectedPlaybooks
    .map((p, i) => `=== Playbook ${i + 1} (from ${p.url}) ===\n${p.playbook}`)
    .join("\n\n");

  const userPrompt = `Please merge the following ${selectedPlaybooks.length} Playbooks (from different pages of the same pattern):

URL Pattern: ${pattern}
${paramsStr}

${playbooksStr}`;

  try {
    const response = await aiClient.chat(
      [
        { role: "system", content: PLAYBOOK_MERGE_SYSTEM_PROMPT },
        { role: "user", content: userPrompt },
      ],
      []
    );

    const mergedPlaybook = response.choices[0]?.message?.content || "";
    return mergedPlaybook.trim() || "Playbook merge failed: empty response";
  } catch (error) {
    console.error(`  ❌ Playbook merge failed: ${error}`);
    // Retry once
    try {
      console.log(`  🔄 Retrying Playbook merge...`);
      const response = await aiClient.chat(
        [
          { role: "system", content: PLAYBOOK_MERGE_SYSTEM_PROMPT },
          { role: "user", content: userPrompt },
        ],
        []
      );
      const mergedPlaybook = response.choices[0]?.message?.content || "";
      return mergedPlaybook.trim() || "Playbook merge failed after retry";
    } catch (retryError) {
      console.error(`  ❌ Retry also failed: ${retryError}`);
      // Fallback: return the first playbook
      return selectedPlaybooks[0]?.playbook || "Playbook merge failed";
    }
  }
}

/**
 * Crawl a single page
 */
async function crawlPage(
  browser: Browser,
  aiClient: AIClient,
  url: string,
  domain: string,
  depth: number,
  visited: Set<string>
): Promise<PageInfo | null> {
  const page = await browser.newPage();

  try {
    console.log(`\n📄 [Depth ${depth}] Crawling: ${url}`);

    await page.goto(url, {
      waitUntil: "domcontentloaded",
      timeout: config.pageLoadTimeout,
    });

    // Wait for page to stabilize
    await page.waitForTimeout(1000);

    // Get title
    const title = await page.title();
    console.log(`   Title: ${title}`);

    // Extract links
    const rawLinks = await extractLinks(page);
    const links = rawLinks
      .map(link => normalizeUrl(link, url))
      .filter((link): link is string =>
        link !== null &&
        isSameDomain(link, domain) &&
        shouldCrawl(link) &&
        !visited.has(link)
      );

    console.log(`   Found ${links.length} new links`);

    // Get simplified HTML and generate Playbook with LLM
    const html = await getSimplifiedHTML(page);
    console.log(`   Generating Playbook (HTML size: ${html.length} chars)...`);
    const playbook = await generatePlaybook(aiClient, url, title, html);

    // Show first line of playbook as preview
    const previewLine = playbook.split("\n").find(l => l.trim().length > 0) || "...";
    console.log(`   Playbook preview: ${previewLine.slice(0, 60)}...`);

    return {
      url,
      title,
      depth,
      playbook,
      links: links.slice(0, 20), // Keep max 20 links per page
      html, // Save HTML for potential merge reference
    };
  } catch (error) {
    console.error(`   ❌ Crawl failed: ${error}`);
    return null;
  } finally {
    await page.close();
  }
}

/**
 * BFS crawl website and merge by URL patterns
 */
async function crawlSite(startUrl: string): Promise<MergedCrawlResult> {
  const domain = extractDomain(startUrl);
  const visited = new Set<string>();
  const pages: PageInfo[] = [];
  const queue: { url: string; depth: number }[] = [{ url: startUrl, depth: 0 }];

  const { provider, model } = getDetectedProvider();
  console.log("=".repeat(60));
  console.log("Crawl Playbook - Website Crawler + LLM Playbook Generation");
  console.log("=".repeat(60));
  console.log(`Start URL: ${startUrl}`);
  console.log(`Domain: ${domain}`);
  console.log(`Max Depth: ${config.maxDepth}`);
  console.log(`Max Pages: ${config.maxPages}`);
  console.log(`Concurrency: ${config.concurrency}`);
  console.log(`HTML Limit: ${config.htmlLimit} chars`);
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);
  console.log("=".repeat(60));

  // Initialize browser and AI Client
  const browser = await chromium.launch({ headless: true });
  const aiClient = new AIClient();

  try {
    // Phase 1: BFS crawl all pages (concurrent batch processing)
    console.log(`\n📥 Phase 1: Crawling pages (concurrency: ${config.concurrency})...\n`);

    // Process queue in concurrent batches
    while (queue.length > 0 && pages.length < config.maxPages) {
      // Get next batch of URLs (up to concurrency limit)
      const batch: { url: string; depth: number }[] = [];
      while (batch.length < config.concurrency && queue.length > 0) {
        const task = queue.shift();
        if (task) {
          // Normalize and check if already visited
          const normalizedUrl = normalizeUrl(task.url, startUrl);
          if (normalizedUrl && !visited.has(normalizedUrl)) {
            visited.add(normalizedUrl);
            batch.push({ url: normalizedUrl, depth: task.depth });
          }
        }
      }

      if (batch.length === 0) {
        break; // No valid URLs to process
      }

      // Process batch concurrently
      const batchResults = await Promise.all(
        batch.map(({ url, depth }) =>
          crawlPage(browser, aiClient, url, domain, depth, visited)
        )
      );

      // Collect results and add new links to queue
      for (let i = 0; i < batchResults.length; i++) {
        const pageInfo = batchResults[i];
        const { depth } = batch[i];

        if (pageInfo) {
          pages.push(pageInfo);

          // Add new links to queue if not at max depth
          if (depth < config.maxDepth) {
            for (const link of pageInfo.links) {
              if (!visited.has(link)) {
                queue.push({ url: link, depth: depth + 1 });
              }
            }
          }
        }

        // Stop if we've reached max pages
        if (pages.length >= config.maxPages) {
          break;
        }
      }
    }

    console.log(`\n✅ Crawled ${pages.length} pages total\n`);

    // Phase 2: Group URLs by pattern
    console.log("=".repeat(60));
    console.log("📊 Phase 2: Grouping URLs by pattern...\n");

    const urls = pages.map(p => p.url);
    const patternGroups = await groupUrlsByPattern(aiClient, urls);

    console.log(`   Found ${patternGroups.length} URL patterns:`);
    patternGroups.forEach((g, i) => {
      console.log(`   ${i + 1}. ${g.pattern} (${g.urls.length} URLs)`);
    });

    // Phase 3: Merge Playbooks by pattern
    console.log("\n" + "=".repeat(60));
    console.log("🔄 Phase 3: Merging Playbooks by pattern...\n");

    const mergedPages: MergedPageInfo[] = [];

    for (const group of patternGroups) {
      const matchedPages = pages.filter(p => group.urls.includes(p.url));

      if (matchedPages.length === 0) continue;

      // Get representative depth (minimum)
      const minDepth = Math.min(...matchedPages.map(p => p.depth));

      // Collect all unique links
      const allLinks = Array.from(new Set(matchedPages.flatMap(p => p.links)));

      let finalPlaybook: string;
      let title: string;

      if (matchedPages.length === 1) {
        // Single page, no merge needed
        finalPlaybook = matchedPages[0].playbook;
        title = matchedPages[0].title;
        console.log(`   📄 ${group.pattern} - single page, no merge needed`);
      } else {
        // Multiple pages, merge Playbooks
        console.log(`   🔀 ${group.pattern} - merging ${matchedPages.length} pages...`);

        const playbooksToMerge = matchedPages.map(p => ({
          url: p.url,
          playbook: p.playbook,
        }));

        finalPlaybook = await mergePlaybooks(
          aiClient,
          group.pattern,
          group.params,
          playbooksToMerge
        );

        // Use pattern-based title or first page's title
        title = matchedPages[0].title.replace(/[\d.]+/, "{version}");
      }

      const mergedPage: MergedPageInfo = {
        url_pattern: group.pattern,
        title,
        depth: minDepth,
        playbook: finalPlaybook,
        links: allLinks.slice(0, 20),
      };

      // Add pattern-specific fields for multi-page patterns
      if (matchedPages.length > 1) {
        mergedPage.pattern_params = group.params;
        mergedPage.matched_urls = group.urls.slice(0, 3); // Keep up to 3 examples
        mergedPage.matched_count = group.urls.length;
      }

      mergedPages.push(mergedPage);
    }

    console.log(`\n✅ Merged into ${mergedPages.length} unique patterns\n`);

    return {
      startUrl,
      domain,
      totalPages: pages.length,
      uniquePatterns: mergedPages.length,
      pages: mergedPages,
    };
  } finally {
    await browser.close();
  }
}

/**
 * Format output results
 */
function formatResults(result: MergedCrawlResult): void {
  console.log("\n" + "=".repeat(60));
  console.log("Crawl Results Summary");
  console.log("=".repeat(60));
  console.log(`Domain: ${result.domain}`);
  console.log(`Total Pages Crawled: ${result.totalPages}`);
  console.log(`Unique Patterns: ${result.uniquePatterns}`);
  console.log("\n📋 Playbook List:\n");

  for (const page of result.pages) {
    console.log(`${"─".repeat(50)}`);
    console.log(`🔗 ${page.url_pattern}`);
    console.log(`   Title: ${page.title}`);
    console.log(`   Depth: ${page.depth}`);
    if (page.matched_count && page.matched_count > 1) {
      console.log(`   Matched URLs: ${page.matched_count}`);
      if (page.matched_urls) {
        page.matched_urls.forEach((url: string) => console.log(`     - ${url}`));
      }
    }
    // Show playbook preview (first 3 lines)
    const playbookLines = page.playbook.split("\n").filter((l: string) => l.trim()).slice(0, 3);
    console.log(`   Playbook preview:`);
    playbookLines.forEach((line: string) => console.log(`     ${line.slice(0, 70)}${line.length > 70 ? "..." : ""}`));
  }

  console.log("\n" + "=".repeat(60));
}

/**
 * Save results to file
 */
function saveResults(result: MergedCrawlResult, outputDir: string): string {
  // Create output directory: output/sites/{domain}/crawl_playbooks/
  const siteDir = path.join(outputDir, "sites", result.domain, "crawl_playbooks");
  fs.mkdirSync(siteDir, { recursive: true });

  // Generate timestamp
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);

  // Save YAML file
  const yamlPath = path.join(siteDir, `crawl_playbook_${timestamp}.yaml`);
  const yamlContent = {
    metadata: {
      start_url: result.startUrl,
      domain: result.domain,
      total_pages: result.totalPages,
      unique_patterns: result.uniquePatterns,
      crawl_time: new Date().toISOString(),
      max_depth: config.maxDepth,
    },
    pages: result.pages.map(page => {
      const pageData: Record<string, unknown> = {
        url_pattern: page.url_pattern,
        title: page.title,
        depth: page.depth,
        playbook: page.playbook,
        links: page.links,
      };

      // Add pattern-specific fields if present
      if (page.pattern_params && page.pattern_params.length > 0) {
        pageData.pattern_params = page.pattern_params;
      }
      if (page.matched_urls && page.matched_urls.length > 0) {
        pageData.matched_urls = page.matched_urls;
      }
      if (page.matched_count && page.matched_count > 1) {
        pageData.matched_count = page.matched_count;
      }

      return pageData;
    }),
  };
  fs.writeFileSync(yamlPath, YAML.stringify(yamlContent, { lineWidth: 0 }), "utf-8");
  console.log(`\n📁 YAML saved: ${yamlPath}`);

  // Save JSON file (for programmatic processing)
  const jsonPath = path.join(siteDir, `crawl_playbook_${timestamp}.json`);
  fs.writeFileSync(jsonPath, JSON.stringify(result, null, 2), "utf-8");
  console.log(`📁 JSON saved: ${jsonPath}`);

  return yamlPath;
}

/**
 * Parse command line arguments
 */
function parseArgs(): { url: string } {
  const args = process.argv.slice(2);

  if (args.length === 0 || args[0] === "--help" || args[0] === "-h") {
    console.log(`
Crawl Playbook - Website Crawler + LLM Playbook Generation

Features:
  - Crawl website pages (BFS)
  - Generate 7-section Playbook for each page using LLM
  - Group similar URLs by pattern (e.g., /docs/{version}/)
  - Merge Playbooks for same pattern into one

Usage:
  npx tsx test/e2e/crawl-playbook.ts <URL> [options]

Options:
  --output <dir>       Output directory (default: ${DEFAULT_OUTPUT_DIR})
  --max-depth <n>      Maximum crawl depth (default: ${DEFAULT_MAX_DEPTH})
  --max-pages <n>      Maximum pages to crawl (default: ${DEFAULT_MAX_PAGES})
  --timeout <ms>       Page load timeout in ms (default: ${DEFAULT_PAGE_LOAD_TIMEOUT})
  --html-limit <n>     HTML content limit in chars (default: ${DEFAULT_HTML_LIMIT})
  --concurrency <n>    Number of concurrent workers (default: ${DEFAULT_CONCURRENCY})
  --help, -h           Show help information

Examples:
  npx tsx test/e2e/crawl-playbook.ts https://releases.rs
  npx tsx test/e2e/crawl-playbook.ts https://example.com --max-pages 50 --max-depth 2
  npx tsx test/e2e/crawl-playbook.ts https://example.com --concurrency 5 --max-pages 100
  npx tsx test/e2e/crawl-playbook.ts https://example.com --output ./my-output --timeout 60000
`);
    process.exit(0);
  }

  const url = args[0];

  for (let i = 1; i < args.length; i++) {
    const arg = args[i];
    const nextArg = args[i + 1];

    if (arg === "--output" && nextArg) {
      config.outputDir = nextArg;
      i++;
    } else if (arg === "--max-depth" && nextArg) {
      config.maxDepth = parseInt(nextArg, 10);
      i++;
    } else if (arg === "--max-pages" && nextArg) {
      config.maxPages = parseInt(nextArg, 10);
      i++;
    } else if (arg === "--timeout" && nextArg) {
      config.pageLoadTimeout = parseInt(nextArg, 10);
      i++;
    } else if (arg === "--html-limit" && nextArg) {
      config.htmlLimit = parseInt(nextArg, 10);
      i++;
    } else if (arg === "--concurrency" && nextArg) {
      config.concurrency = parseInt(nextArg, 10);
      i++;
    }
  }

  return { url };
}

/**
 * Main function
 */
async function main(): Promise<void> {
  const { url: startUrl } = parseArgs();

  // Validate URL
  try {
    new URL(startUrl);
  } catch {
    console.error(`❌ Invalid URL: ${startUrl}`);
    process.exit(1);
  }

  try {
    const result = await crawlSite(startUrl);
    formatResults(result);

    // Save results to file
    const savedPath = saveResults(result, config.outputDir);
    console.log(`\n✅ Crawl completed, results saved to: ${savedPath}`);

    process.exit(0);
  } catch (error) {
    console.error("❌ Crawl failed:", error);
    process.exit(1);
  }
}

// Run main function
main().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});

