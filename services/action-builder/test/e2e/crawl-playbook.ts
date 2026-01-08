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

// Configuration
const MAX_DEPTH = 3;
const MAX_PAGES = 50; // Maximum pages to crawl
const PAGE_LOAD_TIMEOUT = 30000;
const DEFAULT_OUTPUT_DIR = "./output";

// Page info structure
interface PageInfo {
  url: string;
  title: string;
  depth: number;
  features: string[];
  links: string[];
}

// Result structure
interface CrawlResult {
  startUrl: string;
  domain: string;
  totalPages: number;
  pages: PageInfo[];
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
 * Get simplified text content from page (for LLM analysis)
 */
async function getPageContent(page: Page): Promise<string> {
  const content = await page.evaluate(() => {
    // Remove script and style tags
    const clone = document.body.cloneNode(true) as HTMLElement;
    clone.querySelectorAll("script, style, noscript, svg, img").forEach(el => el.remove());

    // Get main text content
    const text = clone.innerText || clone.textContent || "";

    // Simplify: remove extra whitespace
    return text
      .split(/\n+/)
      .map(line => line.trim())
      .filter(line => line.length > 0)
      .slice(0, 100) // Limit lines
      .join("\n")
      .slice(0, 5000); // Limit total length
  });

  return content;
}

/**
 * Summarize page features using LLM
 */
async function summarizePageFeatures(
  aiClient: AIClient,
  url: string,
  title: string,
  content: string
): Promise<string[]> {
  const systemPrompt = `You are a web page functionality analyst. Analyze the given web page content and summarize its main features.

Output requirements:
- List 1-5 main features
- Describe each feature in one concise sentence
- Output only the feature list, nothing else
- Format example:
1. Users can search for products and view search results
2. Display product categories and recommended product listings
3. Provide user login/registration entry points`;

  const userPrompt = `Please analyze the main features of the following web page:

URL: ${url}
Title: ${title}

Page content summary:
${content}`;

  try {
    const response = await aiClient.chat(
      [
        { role: "system", content: systemPrompt },
        { role: "user", content: userPrompt },
      ],
      [] // 不需要工具
    );

    const assistantMessage = response.choices[0]?.message?.content || "";

    // 解析功能列表
    const features = assistantMessage
      .split("\n")
      .map(line => line.trim())
      .filter(line => /^\d+[.、)]\s*/.test(line))
      .map(line => line.replace(/^\d+[.、)]\s*/, "").trim())
      .filter(line => line.length > 0);

    return features.length > 0 ? features : ["Unable to identify page features"];
  } catch (error) {
    console.error(`  ❌ LLM analysis failed: ${error}`);
    return ["LLM analysis failed"];
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
      timeout: PAGE_LOAD_TIMEOUT,
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

    // Get page content and analyze with LLM
    const content = await getPageContent(page);
    console.log(`   Analyzing page features...`);
    const features = await summarizePageFeatures(aiClient, url, title, content);

    console.log(`   Features:`);
    features.forEach((f, i) => console.log(`     ${i + 1}. ${f}`));

    return {
      url,
      title,
      depth,
      features,
      links: links.slice(0, 20), // 每页最多保留 20 个链接
    };
  } catch (error) {
    console.error(`   ❌ Crawl failed: ${error}`);
    return null;
  } finally {
    await page.close();
  }
}

/**
 * BFS crawl website
 */
async function crawlSite(startUrl: string): Promise<CrawlResult> {
  const domain = extractDomain(startUrl);
  const visited = new Set<string>();
  const pages: PageInfo[] = [];
  const queue: { url: string; depth: number }[] = [{ url: startUrl, depth: 0 }];

  const { provider, model } = getDetectedProvider();
  console.log("=".repeat(60));
  console.log("Crawl Playbook - Website Crawler + LLM Feature Summary");
  console.log("=".repeat(60));
  console.log(`Start URL: ${startUrl}`);
  console.log(`Domain: ${domain}`);
  console.log(`Max Depth: ${MAX_DEPTH}`);
  console.log(`Max Pages: ${MAX_PAGES}`);
  console.log(`LLM Provider: ${provider}`);
  console.log(`LLM Model: ${model}`);
  console.log("=".repeat(60));

  // 初始化浏览器和 AI Client
  const browser = await chromium.launch({ headless: true });
  const aiClient = new AIClient();

  try {
    while (queue.length > 0 && pages.length < MAX_PAGES) {
      const { url, depth } = queue.shift()!;

      // 规范化 URL
      const normalizedUrl = normalizeUrl(url, startUrl);
      if (!normalizedUrl || visited.has(normalizedUrl)) {
        continue;
      }

      visited.add(normalizedUrl);

      // 爬取页面
      const pageInfo = await crawlPage(
        browser,
        aiClient,
        normalizedUrl,
        domain,
        depth,
        visited
      );

      if (pageInfo) {
        pages.push(pageInfo);

        // 如果未达到最大深度，将新链接加入队列
        if (depth < MAX_DEPTH) {
          for (const link of pageInfo.links) {
            if (!visited.has(link)) {
              queue.push({ url: link, depth: depth + 1 });
            }
          }
        }
      }
    }

    return {
      startUrl,
      domain,
      totalPages: pages.length,
      pages,
    };
  } finally {
    await browser.close();
  }
}

/**
 * Format output results
 */
function formatResults(result: CrawlResult): void {
  console.log("\n" + "=".repeat(60));
  console.log("Crawl Results Summary");
  console.log("=".repeat(60));
  console.log(`Domain: ${result.domain}`);
  console.log(`Total Pages: ${result.totalPages}`);
  console.log("\n📋 Page Feature List:\n");

  for (const page of result.pages) {
    console.log(`${"─".repeat(50)}`);
    console.log(`🔗 ${page.url}`);
    console.log(`   Title: ${page.title}`);
    console.log(`   Depth: ${page.depth}`);
    console.log(`   Features:`);
    page.features.forEach((f, i) => {
      console.log(`     ${i + 1}. ${f}`);
    });
  }

  console.log("\n" + "=".repeat(60));
}

/**
 * Save results to file
 */
function saveResults(result: CrawlResult, outputDir: string): string {
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
      crawl_time: new Date().toISOString(),
      max_depth: MAX_DEPTH,
    },
    pages: result.pages.map(page => ({
      url: page.url,
      title: page.title,
      depth: page.depth,
      features: page.features,
      links: page.links, // Internal links found on this page (for crawling next level)
    })),
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
function parseArgs(): { url: string; outputDir: string } {
  const args = process.argv.slice(2);

  if (args.length === 0 || args[0] === "--help" || args[0] === "-h") {
    console.log(`
Crawl Playbook - Website Crawler + LLM Feature Summary

Usage:
  npx tsx test/e2e/crawl_playbook.ts <URL> [options]

Options:
  --output <dir>   Output directory (default: ./output)
  --help, -h       Show help information

Examples:
  npx tsx test/e2e/crawl_playbook.ts https://arxiv.org
  npx tsx test/e2e/crawl_playbook.ts https://arxiv.org --output ./my-output
`);
    process.exit(0);
  }

  let url = args[0];
  let outputDir = DEFAULT_OUTPUT_DIR;

  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--output" && args[i + 1]) {
      outputDir = args[++i];
    }
  }

  return { url, outputDir };
}

/**
 * Main function
 */
async function main(): Promise<void> {
  const { url: startUrl, outputDir } = parseArgs();

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
    const savedPath = saveResults(result, outputDir);
    console.log(`\n✅ Crawl completed, results saved to: ${savedPath}`);

    process.exit(0);
  } catch (error) {
    console.error("❌ Crawl failed:", error);
    process.exit(1);
  }
}

// 运行主函数
main().catch((error) => {
  console.error("Unhandled error:", error);
  process.exit(1);
});

