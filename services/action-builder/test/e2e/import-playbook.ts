#!/usr/bin/env npx tsx
/**
 * Import Playbook - Import crawled playbook data to database
 *
 * Features:
 * - Read JSON output from crawl_playbook
 * - Create/update source records
 * - Create source_version records (new version if exists)
 * - Create documents records
 * - Create chunks records (1:1 with document, including embedding)
 * - Create build_task records
 *
 * Supports both formats:
 * - New format: Merged patterns with url_pattern and playbook fields
 * - Legacy format: Individual pages with url and features fields
 *
 * Usage:
 *   npx tsx test/e2e/import-playbook.ts <json_file>
 *   npx tsx test/e2e/import-playbook.ts output/sites/arxiv.org/crawl_playbooks/crawl_playbook_xxx.json
 *
 * Environment Variables:
 *   DATABASE_URL - Database connection string
 *   OPENAI_API_KEY - For embedding generation
 */

import fs from "fs";
import path from "path";
import crypto from "crypto";
import {
  getDb,
  sources,
  sourceVersions,
  documents,
  chunks,
  buildTasks,
  eq,
  desc,
  sql,
} from "@actionbookdev/db";
import type {
  SourceVersionStatus,
  DocumentStatus,
} from "@actionbookdev/db";

// Load environment variables (from both action-builder and knowledge-builder .env)
import * as dotenv from "dotenv";
// Load action-builder .env first (DATABASE_URL, etc.)
dotenv.config({ path: path.resolve(process.cwd(), ".env") });
// Load knowledge-builder .env (OPENAI_API_KEY, etc.), don't override existing
dotenv.config({ path: path.resolve(process.cwd(), "../knowledge-builder/.env") });

// ============================================================================
// Types
// ============================================================================

// Pattern parameter definition
interface PatternParam {
  name: string;
  description: string;
}

// New format: Merged page info (after pattern grouping)
interface MergedPageInfo {
  url_pattern: string;
  pattern_params?: PatternParam[];
  matched_urls?: string[];    // Up to 3 example URLs
  matched_count?: number;     // Total matched URLs count
  title: string;
  depth: number;
  playbook: string;           // Full 7-section Playbook Markdown
  links: string[];
}

// Legacy format: Old page info
interface LegacyPageInfo {
  url: string;
  title: string;
  depth: number;
  features: string[];
  links: string[];
}

// New format: Merged crawl result
interface MergedCrawlPlaybookData {
  startUrl: string;
  domain: string;
  totalPages: number;
  uniquePatterns: number;
  pages: MergedPageInfo[];
}

// Legacy format: Old crawl result
interface LegacyCrawlPlaybookData {
  startUrl: string;
  domain: string;
  totalPages: number;
  pages: LegacyPageInfo[];
}

// Union type for both formats
type CrawlPlaybookData = MergedCrawlPlaybookData | LegacyCrawlPlaybookData;

// Type guard to check if data is new format
function isNewFormat(data: CrawlPlaybookData): data is MergedCrawlPlaybookData {
  return 'uniquePatterns' in data && data.pages.length > 0 && 'url_pattern' in data.pages[0];
}

interface ImportResult {
  sourceId: number;
  versionId: number;
  buildTaskId: number;
  documentsCreated: number;
  chunksCreated: number;
}

// ============================================================================
// Embedding Service (OpenAI - compatible with knowledge-builder)
// ============================================================================

import OpenAI from "openai";
import { HttpsProxyAgent } from "https-proxy-agent";

const EMBEDDING_MODEL = "text-embedding-3-small";
const EMBEDDING_DIMENSION = 1536;
const BATCH_SIZE = 100;

class EmbeddingService {
  private client: OpenAI;
  readonly model = EMBEDDING_MODEL;
  readonly dimension = EMBEDDING_DIMENSION;

  constructor() {
    // Use OpenAI API (consistent with knowledge-builder)
    const apiKey = process.env.OPENAI_API_KEY;
    if (!apiKey) {
      throw new Error(
        "OPENAI_API_KEY not found. Please set it in .env file."
      );
    }

    const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;
    const baseUrl = process.env.OPENAI_BASE_URL;

    if (proxyUrl) {
      console.log(`[Embedding] Using proxy: ${proxyUrl}`);
    }
    if (baseUrl) {
      console.log(`[Embedding] Using custom baseURL: ${baseUrl}`);
    }

    this.client = new OpenAI({
      apiKey,
      baseURL: baseUrl,
      timeout: 60000,
      maxRetries: 3,
      httpAgent: proxyUrl ? new HttpsProxyAgent(proxyUrl) : undefined,
    });

    console.log(`[Embedding] Using OpenAI ${this.model} (${this.dimension} dim)`);
  }

  async getEmbedding(text: string): Promise<number[]> {
    const trimmed = text.trim();
    if (!trimmed) {
      throw new Error("Cannot embed empty text");
    }

    const response = await this.client.embeddings.create({
      model: this.model,
      input: trimmed,
    });

    return response.data[0].embedding;
  }

  async getEmbeddings(texts: string[]): Promise<number[][]> {
    const validTexts = texts.map((t) => t.trim()).filter((t) => t.length > 0);
    if (validTexts.length === 0) return [];

    console.log(`[Embedding] Processing ${validTexts.length} texts`);

    const results: number[][] = [];

    // Process in batches
    for (let i = 0; i < validTexts.length; i += BATCH_SIZE) {
      const batch = validTexts.slice(i, i + BATCH_SIZE);

      const response = await this.client.embeddings.create({
        model: this.model,
        input: batch,
      });

      for (const data of response.data) {
        results.push(data.embedding);
      }

      console.log(`   Embedding progress: ${Math.min(i + BATCH_SIZE, validTexts.length)}/${validTexts.length}`);

      // Simple rate limiting
      if (i + BATCH_SIZE < validTexts.length) {
        await new Promise((r) => setTimeout(r, 100));
      }
    }

    return results;
  }
}

// ============================================================================
// Helper Functions
// ============================================================================

function md5(text: string): string {
  return crypto.createHash("md5").update(text).digest("hex");
}

function estimateTokens(text: string): number {
  // Simple estimation: ~4 chars/token for English, ~2 chars/token for Chinese
  return Math.ceil(text.length / 3);
}

// Get URL from page (handles both formats)
function getPageUrl(page: MergedPageInfo | LegacyPageInfo): string {
  if ('url_pattern' in page) {
    return page.url_pattern;
  }
  return page.url;
}

// Get content text from page (handles both formats)
function getContentText(page: MergedPageInfo | LegacyPageInfo): string {
  if ('playbook' in page) {
    // New format: playbook is already a full Markdown string
    return page.playbook;
  }
  // Legacy format: join features array
  return page.features.join("\n");
}

// Get description from page (handles both formats)
function getDescription(page: MergedPageInfo | LegacyPageInfo): string {
  if ('playbook' in page) {
    // Extract first line or first 200 chars from playbook
    const firstLine = page.playbook.split('\n').find(line => line.trim() && !line.startsWith('#'));
    return firstLine?.slice(0, 200) || page.title || "";
  }
  return page.features[0] || "";
}

// Format page as Markdown (handles both formats)
function formatPageAsMarkdown(page: MergedPageInfo | LegacyPageInfo): string {
  if ('playbook' in page) {
    // New format: playbook is already formatted Markdown, add metadata header
    const lines: string[] = [];
    lines.push(`# ${page.title || "Untitled"}`);
    lines.push("");
    lines.push(`**URL Pattern:** ${page.url_pattern}`);
    if (page.matched_urls && page.matched_urls.length > 0) {
      lines.push(`**Example URLs:** ${page.matched_urls.slice(0, 3).join(", ")}`);
    }
    if (page.matched_count && page.matched_count > 1) {
      lines.push(`**Total Matched:** ${page.matched_count} pages`);
    }
    lines.push(`**Depth:** ${page.depth}`);
    lines.push("");
    lines.push("---");
    lines.push("");
    lines.push(page.playbook);
    return lines.join("\n");
  }

  // Legacy format
  const lines: string[] = [];
  lines.push(`# ${page.title || "Untitled"}`);
  lines.push("");
  lines.push(`**URL:** ${page.url}`);
  lines.push(`**Depth:** ${page.depth}`);
  lines.push("");
  lines.push("## Page Features");
  lines.push("");
  page.features.forEach((f, i) => {
    lines.push(`${i + 1}. ${f}`);
  });
  return lines.join("\n");
}

// ============================================================================
// Import Logic
// ============================================================================

async function importPlaybook(filePath: string): Promise<ImportResult> {
  // Read file
  console.log(`\n📖 Reading file: ${filePath}`);
  const content = fs.readFileSync(filePath, "utf-8");
  const data: CrawlPlaybookData = JSON.parse(content);

  const isNew = isNewFormat(data);
  console.log(`   Format: ${isNew ? 'New (merged patterns)' : 'Legacy (individual pages)'}`);
  console.log(`   Domain: ${data.domain}`);
  console.log(`   Start URL: ${data.startUrl}`);
  console.log(`   Total pages: ${data.totalPages}`);
  if (isNew) {
    console.log(`   Unique patterns: ${(data as MergedCrawlPlaybookData).uniquePatterns}`);
  }

  const db = getDb();
  const embeddingService = new EmbeddingService();

  // 1. Create or get source
  console.log(`\n📦 Creating/Getting Source...`);
  let source = await db
    .select()
    .from(sources)
    .where(eq(sources.domain, data.domain))
    .limit(1)
    .then((rows) => rows[0]);

  if (source) {
    console.log(`   Found existing source #${source.id}, creating new version`);
  } else {
    const result = await db
      .insert(sources)
      .values({
        name: data.domain,
        baseUrl: data.startUrl,
        domain: data.domain,
        description: `Playbook crawl data for ${data.domain}`,
        crawlConfig: {
          maxDepth: 3,
          maxPages: 50,
        },
      })
      .returning();
    source = result[0];
    console.log(`   Created source #${source.id}`);
  }

  // 2. Create source_version
  console.log(`\n📋 Creating Source Version...`);
  const latestVersion = await db
    .select({ versionNumber: sourceVersions.versionNumber })
    .from(sourceVersions)
    .where(eq(sourceVersions.sourceId, source.id))
    .orderBy(desc(sourceVersions.versionNumber))
    .limit(1);

  const nextVersionNumber = (latestVersion[0]?.versionNumber ?? 0) + 1;

  const versionResult = await db
    .insert(sourceVersions)
    .values({
      sourceId: source.id,
      versionNumber: nextVersionNumber,
      status: "building" as SourceVersionStatus,
      commitMessage: `Import from playbook: ${path.basename(filePath)}`,
      createdBy: "import-playbook",
    })
    .returning();
  const version = versionResult[0];
  console.log(`   Created version #${version.id} (v${nextVersionNumber})`);

  // 3. Create build_task (with sourceVersionId linkage)
  console.log(`\n📝 Creating Build Task...`);
  const taskResult = await db
    .insert(buildTasks)
    .values({
      sourceId: source.id,
      sourceVersionId: version.id, // Link to the source_version
      sourceUrl: data.startUrl,
      sourceName: data.domain,
      sourceCategory: "playbook",
      stage: "knowledge_build",
      stageStatus: "running",
      config: {
        importedFrom: filePath,
        totalPages: data.totalPages,
      },
      knowledgeStartedAt: new Date(),
    })
    .returning();
  const buildTask = taskResult[0];
  console.log(`   Created build_task #${buildTask.id} (linked to version #${version.id})`);

  // 4. Prepare page contents for batch embedding generation
  console.log(`\n🧠 Generating Embeddings...`);
  const pageContents = data.pages.map((page) => getContentText(page));
  const embeddings = await embeddingService.getEmbeddings(pageContents);
  console.log(`   Generated ${embeddings.length} embeddings (${embeddings[0]?.length || 0} dim)`);

  // 5. Create documents and chunks
  console.log(`\n📄 Creating Documents and Chunks...`);
  let documentsCreated = 0;
  let chunksCreated = 0;

  for (let i = 0; i < data.pages.length; i++) {
    const page = data.pages[i];
    const embedding = embeddings[i];
    const contentText = getContentText(page);
    const contentMd = formatPageAsMarkdown(page);
    const pageUrl = getPageUrl(page);
    const urlHash = md5(pageUrl);
    const contentHash = md5(contentText);

    // Upsert document (update if URL exists in this version)
    const docResult = await db
      .insert(documents)
      .values({
        sourceId: source.id,
        sourceVersionId: version.id,
        url: pageUrl,
        urlHash,
        title: page.title || "Untitled",
        description: getDescription(page),
        contentText,
        contentHtml: "", // playbook doesn't save HTML
        contentMd,
        depth: page.depth,
        breadcrumb: [],
        wordCount: contentText.length,
        language: "en", // Changed to English as playbooks are now in English format
        contentHash,
        status: "active" as DocumentStatus,
        version: 1,
      })
      .onConflictDoUpdate({
        target: [documents.sourceVersionId, documents.urlHash],
        set: {
          title: page.title || "Untitled",
          description: getDescription(page),
          contentText,
          contentMd,
          depth: page.depth,
          wordCount: contentText.length,
          contentHash,
          updatedAt: new Date(),
        },
      })
      .returning();
    const doc = docResult[0];
    documentsCreated++;

    // Upsert chunk (1:1 mapping with document)
    const embeddingStr = `[${embedding.join(",")}]`;
    await db.execute(sql`
      INSERT INTO chunks (
        document_id, source_version_id, content, content_hash, chunk_index,
        start_char, end_char, heading, heading_hierarchy,
        token_count, embedding, embedding_model
      ) VALUES (
        ${doc.id},
        ${version.id},
        ${contentText},
        ${contentHash},
        ${0},
        ${0},
        ${contentText.length},
        ${page.title || "Untitled"},
        ${JSON.stringify([{ level: 1, text: page.title || "Untitled" }])}::jsonb,
        ${estimateTokens(contentText)},
        ${embeddingStr}::vector,
        ${"text-embedding-3-small"}
      )
      ON CONFLICT (document_id, chunk_index)
      DO UPDATE SET
        source_version_id = EXCLUDED.source_version_id,
        content = EXCLUDED.content,
        content_hash = EXCLUDED.content_hash,
        start_char = EXCLUDED.start_char,
        end_char = EXCLUDED.end_char,
        heading = EXCLUDED.heading,
        heading_hierarchy = EXCLUDED.heading_hierarchy,
        token_count = EXCLUDED.token_count,
        embedding = EXCLUDED.embedding,
        embedding_model = EXCLUDED.embedding_model
    `);
    chunksCreated++;

    // Progress display
    if ((i + 1) % 10 === 0 || i === data.pages.length - 1) {
      console.log(`   Progress: ${i + 1}/${data.pages.length}`);
    }
  }

  // 6. Update version and task status (knowledge_build stage completed, waiting for action_build)
  console.log(`\n✅ Knowledge Build stage completed...`);

  // Version status remains 'building', waiting for action-builder to complete before publishing as 'active'
  // source_versions.status = 'building' (already default, no update needed)

  // Update source lastCrawledAt
  await db
    .update(sources)
    .set({
      lastCrawledAt: new Date(),
      updatedAt: new Date(),
    })
    .where(eq(sources.id, source.id));

  // Update build_task: knowledge_build stage completed
  await db
    .update(buildTasks)
    .set({
      stage: "knowledge_build",
      stageStatus: "completed",
      knowledgeCompletedAt: new Date(),
      updatedAt: new Date(),
    })
    .where(eq(buildTasks.id, buildTask.id));

  return {
    sourceId: source.id,
    versionId: version.id,
    buildTaskId: buildTask.id,
    documentsCreated,
    chunksCreated,
  };
}

// ============================================================================
// CLI
// ============================================================================

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.length === 0 || args[0] === "--help" || args[0] === "-h") {
    console.log(`
Import Playbook - Import crawled playbook data to database

Usage:
  npx tsx test/e2e/import-playbook.ts <json_file>

Examples:
  npx tsx test/e2e/import-playbook.ts output/sites/arxiv.org/crawl_playbooks/crawl_playbook_xxx.json

Supported Formats:
  - New format: Merged patterns with url_pattern and playbook fields
  - Legacy format: Individual pages with url and features fields

Environment Variables:
  DATABASE_URL       - Database connection string
  OPENAI_API_KEY     - OpenAI API Key (for embedding generation)
`);
    process.exit(0);
  }

  const filePath = args[0];

  // Validate file exists
  if (!fs.existsSync(filePath)) {
    console.error(`❌ File not found: ${filePath}`);
    process.exit(1);
  }

  // Validate file type
  if (!filePath.endsWith(".json")) {
    console.error(`❌ Only JSON files are supported`);
    process.exit(1);
  }

  console.log("=".repeat(60));
  console.log("Import Playbook - Import crawled data to database");
  console.log("=".repeat(60));

  try {
    const result = await importPlaybook(filePath);

    console.log("\n" + "=".repeat(60));
    console.log("Import completed!");
    console.log("=".repeat(60));
    console.log(`📦 Source ID: ${result.sourceId}`);
    console.log(`📋 Version ID: ${result.versionId}`);
    console.log(`📝 Build Task ID: ${result.buildTaskId}`);
    console.log(`📄 Documents: ${result.documentsCreated}`);
    console.log(`🧩 Chunks: ${result.chunksCreated}`);

    process.exit(0);
  } catch (error) {
    console.error("\n❌ Import failed:", error);
    process.exit(1);
  }
}

main();

