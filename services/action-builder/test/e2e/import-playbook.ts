#!/usr/bin/env npx tsx
/**
 * Import Playbook - 导入爬虫数据到数据库
 *
 * 功能:
 * - 读取 crawl_playbook 的 JSON/YAML 输出
 * - 创建/更新 source 记录
 * - 创建 source_version 记录（如果已存在则新建版本）
 * - 创建 documents 记录
 * - 创建 chunks 记录（1:1 对应 document，带 embedding）
 * - 创建 build_task 记录
 *
 * 使用方法:
 *   npx tsx scripts/import-playbook.ts <json_file>
 *   npx tsx scripts/import-playbook.ts output/sites/arxiv.org/crawl_playbooks/crawl_playbook_xxx.json
 *
 * 环境变量:
 *   DATABASE_URL - 数据库连接字符串
 *   OPENAI_API_KEY 或 OPENROUTER_API_KEY - 用于生成 embedding
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

// 加载环境变量 (同时加载 action-builder 和 knowledge-builder 的 .env)
import * as dotenv from "dotenv";
// 先加载 action-builder 的 .env (DATABASE_URL 等)
dotenv.config({ path: path.resolve(process.cwd(), ".env") });
// 再加载 knowledge-builder 的 .env (OPENAI_API_KEY 等)，不覆盖已有的
dotenv.config({ path: path.resolve(process.cwd(), "../knowledge-builder/.env") });

// ============================================================================
// Types
// ============================================================================

interface PageInfo {
  url: string;
  title: string;
  depth: number;
  features: string[];
  links: string[];
}

interface CrawlPlaybookData {
  startUrl: string;
  domain: string;
  totalPages: number;
  pages: PageInfo[];
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
    // 使用 OpenAI API (与 knowledge-builder 保持一致)
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
  // 简单估算：英文约 4 字符/token，中文约 2 字符/token
  return Math.ceil(text.length / 3);
}

function formatPageAsMarkdown(page: PageInfo): string {
  const lines: string[] = [];
  lines.push(`# ${page.title || "Untitled"}`);
  lines.push("");
  lines.push(`**URL:** ${page.url}`);
  lines.push(`**Depth:** ${page.depth}`);
  lines.push("");
  lines.push("## 页面功能");
  lines.push("");
  page.features.forEach((f, i) => {
    lines.push(`${i + 1}. ${f}`);
  });
  return lines.join("\n");
}

function formatContentText(page: PageInfo): string {
  return page.features.join("\n");
}

// ============================================================================
// Import Logic
// ============================================================================

async function importPlaybook(filePath: string): Promise<ImportResult> {
  // 读取文件
  console.log(`\n📖 读取文件: ${filePath}`);
  const content = fs.readFileSync(filePath, "utf-8");
  const data: CrawlPlaybookData = JSON.parse(content);

  console.log(`   域名: ${data.domain}`);
  console.log(`   起始 URL: ${data.startUrl}`);
  console.log(`   总页面数: ${data.totalPages}`);

  const db = getDb();
  const embeddingService = new EmbeddingService();

  // 1. 创建或获取 source
  console.log(`\n📦 创建/获取 Source...`);
  let source = await db
    .select()
    .from(sources)
    .where(eq(sources.domain, data.domain))
    .limit(1)
    .then((rows) => rows[0]);

  if (source) {
    console.log(`   已存在 source #${source.id}，将新建版本`);
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
    console.log(`   创建 source #${source.id}`);
  }

  // 2. 创建 source_version
  console.log(`\n📋 创建 Source Version...`);
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
  console.log(`   创建 version #${version.id} (v${nextVersionNumber})`);

  // 3. 创建 build_task
  console.log(`\n📝 创建 Build Task...`);
  const taskResult = await db
    .insert(buildTasks)
    .values({
      sourceId: source.id,
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
  console.log(`   创建 build_task #${buildTask.id}`);

  // 4. 准备所有页面内容用于批量生成 embedding
  console.log(`\n🧠 Generating Embeddings...`);
  const pageContents = data.pages.map((page) => formatContentText(page));
  const embeddings = await embeddingService.getEmbeddings(pageContents);
  console.log(`   Generated ${embeddings.length} embeddings (${embeddings[0]?.length || 0} dim)`);

  // 5. 创建 documents 和 chunks
  console.log(`\n📄 创建 Documents 和 Chunks...`);
  let documentsCreated = 0;
  let chunksCreated = 0;

  for (let i = 0; i < data.pages.length; i++) {
    const page = data.pages[i];
    const embedding = embeddings[i];
    const contentText = formatContentText(page);
    const contentMd = formatPageAsMarkdown(page);
    const urlHash = md5(page.url);
    const contentHash = md5(contentText);

    // 创建 document
    const docResult = await db
      .insert(documents)
      .values({
        sourceId: source.id,
        sourceVersionId: version.id,
        url: page.url,
        urlHash,
        title: page.title || "Untitled",
        description: page.features[0] || "",
        contentText,
        contentHtml: "", // playbook 不保存 HTML
        contentMd,
        depth: page.depth,
        breadcrumb: [],
        wordCount: contentText.length,
        language: "zh",
        contentHash,
        status: "active" as DocumentStatus,
        version: 1,
      })
      .returning();
    const doc = docResult[0];
    documentsCreated++;

    // 创建 chunk (1:1)
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
    `);
    chunksCreated++;

    // 进度显示
    if ((i + 1) % 10 === 0 || i === data.pages.length - 1) {
      console.log(`   进度: ${i + 1}/${data.pages.length}`);
    }
  }

  // 6. 更新版本和任务状态 (knowledge_build 阶段完成，等待 action_build)
  console.log(`\n✅ 完成 Knowledge Build 阶段...`);

  // 版本状态保持 'building'，等待 action-builder 完成后再发布为 'active'
  // source_versions.status = 'building' (已经是默认值，不需要更新)

  // 更新 source 的 lastCrawledAt
  await db
    .update(sources)
    .set({
      lastCrawledAt: new Date(),
      updatedAt: new Date(),
    })
    .where(eq(sources.id, source.id));

  // 更新 build_task: knowledge_build 阶段完成
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
Import Playbook - 导入爬虫数据到数据库

用法:
  npx tsx scripts/import-playbook.ts <json_file>

示例:
  npx tsx scripts/import-playbook.ts output/sites/arxiv.org/crawl_playbooks/crawl_playbook_xxx.json

环境变量:
  DATABASE_URL       - 数据库连接字符串
  OPENAI_API_KEY     - OpenAI API Key (用于 embedding)
  OPENROUTER_API_KEY - OpenRouter API Key (优先使用)
`);
    process.exit(0);
  }

  const filePath = args[0];

  // 验证文件存在
  if (!fs.existsSync(filePath)) {
    console.error(`❌ 文件不存在: ${filePath}`);
    process.exit(1);
  }

  // 验证文件类型
  if (!filePath.endsWith(".json")) {
    console.error(`❌ 只支持 JSON 文件`);
    process.exit(1);
  }

  console.log("=".repeat(60));
  console.log("Import Playbook - 导入爬虫数据到数据库");
  console.log("=".repeat(60));

  try {
    const result = await importPlaybook(filePath);

    console.log("\n" + "=".repeat(60));
    console.log("导入完成！");
    console.log("=".repeat(60));
    console.log(`📦 Source ID: ${result.sourceId}`);
    console.log(`📋 Version ID: ${result.versionId}`);
    console.log(`📝 Build Task ID: ${result.buildTaskId}`);
    console.log(`📄 Documents: ${result.documentsCreated}`);
    console.log(`🧩 Chunks: ${result.chunksCreated}`);

    process.exit(0);
  } catch (error) {
    console.error("\n❌ 导入失败:", error);
    process.exit(1);
  }
}

main();

