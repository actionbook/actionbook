#!/usr/bin/env node
/**
 * First Round Capital - Full Pipeline Test
 *
 * Test ActionBuilder full pipeline based on real data in chunks table:
 * 1. Read 2 records from chunks table
 * 2. Generate recording_tasks
 * 3. Execute ActionBuilder recording
 * 4. Verify YAML and DB output
 * 5. Run validate to verify selectors
 *
 * Usage:
 *   pnpm -C services/action-builder firstround:pipeline
 *
 * Environment:
 *   Set ONE of: OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY
 *   AIClient auto-detects provider from available API keys.
 *
 * Optional env overrides:
 *   - ACTION_BUILDER_E2E_DATABASE_URL (defaults to DATABASE_URL)
 *   - ACTION_BUILDER_E2E_SOURCE_ID (defaults to 1)
 *   - ACTION_BUILDER_E2E_CHUNK_LIMIT (defaults to 2)
 *   - ACTION_BUILDER_E2E_RUN_ONLY_CHUNK_INDEX (defaults to 2; set 0 to run all)
 *   - ACTION_BUILDER_E2E_HEADLESS (defaults to false; set true for CI)
 */

import { config } from 'dotenv'
import { resolve } from 'path'
import * as fs from 'fs'
import net from 'node:net'

// Load environment variables
// This script is compiled to `dist/`, so `__dirname` cannot be used reliably.
// Use `process.cwd()` (expected to be `services/action-builder`) instead.
const projectRoot = process.cwd()
config({ path: resolve(projectRoot, '../db/.env') })
config({ path: resolve(projectRoot, '.env'), override: true })

import { TaskGenerator } from '../../src/task-worker/task-generator.js'
import { TaskExecutor } from '../../src/task-worker/task-executor.js'
import { ActionBuilder } from '../../src/ActionBuilder.js'
import type {
  TaskExecutorConfig,
  RecordingTask,
} from '../../src/task-worker/types/index.js'
import {
  createDb,
  closeDb,
  sources,
  documents,
  chunks,
  pages,
  elements,
  recordingTasks,
  eq,
  and,
  inArray,
  desc,
  sql,
} from '@actionbookdev/db'

// ============================================================================
// Configuration
// ============================================================================

// Generate timestamped output directory
function generateOutputDir(): string {
  const now = new Date()
  const timestamp = now
    .toISOString()
    .replace(/[-:]/g, '')
    .replace('T', '_')
    .replace(/\..+/, '')
    .slice(0, 15) // YYYYMMDD_hhmmss
  return `./output/test_${timestamp}`
}

const OUTPUT_DIR = generateOutputDir()

function parseBooleanEnv(
  value: string | undefined,
  defaultValue: boolean
): boolean {
  if (value === undefined) return defaultValue
  return (
    value.toLowerCase() === 'true' ||
    value === '1' ||
    value.toLowerCase() === 'yes'
  )
}

async function assertDbConnectivity(databaseUrl: string): Promise<void> {
  const url = new URL(databaseUrl)
  const port = url.port ? Number(url.port) : 5432
  const host = url.hostname || 'localhost'

  await new Promise<void>((resolvePromise, rejectPromise) => {
    const socket = net.connect({ host, port })
    const timeout = setTimeout(() => {
      socket.destroy()
      rejectPromise(
        Object.assign(new Error('connect timeout'), { code: 'ETIMEDOUT' })
      )
    }, 1000)

    socket.once('connect', () => {
      clearTimeout(timeout)
      socket.end()
      resolvePromise()
    })
    socket.once('error', (err: any) => {
      clearTimeout(timeout)
      socket.destroy()
      rejectPromise(err)
    })
  })
}

// AIClient auto-detects provider from environment variables
const CONFIG: TaskExecutorConfig = {
  databaseUrl:
    process.env.ACTION_BUILDER_E2E_DATABASE_URL ||
    process.env.DATABASE_URL ||
    '',
  headless: parseBooleanEnv(process.env.ACTION_BUILDER_E2E_HEADLESS, false), // Set to true for CI
  maxTurns: 30,
  outputDir: OUTPUT_DIR,
}

// Get detected provider for logging
function getDetectedLLMInfo(): { provider: string; model: string } {
  if (process.env.OPENROUTER_API_KEY) {
    return {
      provider: 'OpenRouter',
      model: process.env.OPENROUTER_MODEL || 'anthropic/claude-sonnet-4',
    }
  }
  if (process.env.OPENAI_API_KEY) {
    return {
      provider: 'OpenAI',
      model: process.env.OPENAI_MODEL || 'gpt-4o',
    }
  }
  if (process.env.ANTHROPIC_API_KEY) {
    return {
      provider: 'Anthropic',
      model: process.env.ANTHROPIC_MODEL || 'claude-sonnet-4-5',
    }
  }
  return { provider: 'none', model: 'none' }
}

function hasLLMApiKey(): boolean {
  return !!(
    process.env.OPENROUTER_API_KEY ||
    process.env.OPENAI_API_KEY ||
    process.env.ANTHROPIC_API_KEY
  )
}

const SOURCE_ID = Number(process.env.ACTION_BUILDER_E2E_SOURCE_ID || 1) // First Round Capital source
const CHUNK_LIMIT = Number(process.env.ACTION_BUILDER_E2E_CHUNK_LIMIT || 2) // Only process 2 chunks
const RUN_ONLY_CHUNK_INDEX = Number(
  process.env.ACTION_BUILDER_E2E_RUN_ONLY_CHUNK_INDEX || 0
) // Only run the second chunk (task_driven), set to 0 to run all

// ============================================================================
// Helper Functions
// ============================================================================

function printSection(title: string): void {
  console.log('\n' + '='.repeat(70))
  console.log(title)
  console.log('='.repeat(70))
}

function printSubsection(title: string): void {
  console.log('\n' + '-'.repeat(50))
  console.log(title)
  console.log('-'.repeat(50))
}

// ============================================================================
// Main Test Flow
// ============================================================================

async function runFullPipelineTest(): Promise<void> {
  const { provider, model } = getDetectedLLMInfo()

  printSection('🚀 First Round Capital - Full Pipeline Test')
  console.log(`\n📅 Start Time: ${new Date().toISOString()}`)
  console.log(`📊 LLM Provider: ${provider}`)
  console.log(`📊 LLM Model: ${model}`)
  console.log(`🗄️  Database: ${CONFIG.databaseUrl ? '[set]' : '[missing]'}`)
  console.log(`🎯 Source ID: ${SOURCE_ID}`)
  console.log(`📦 Chunk Limit: ${CHUNK_LIMIT}`)

  if (!CONFIG.databaseUrl) {
    console.error(
      '❌ Missing database URL. Set ACTION_BUILDER_E2E_DATABASE_URL or DATABASE_URL.'
    )
    process.exit(1)
  }
  if (!hasLLMApiKey()) {
    console.error(
      '❌ Missing LLM API key. Set OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY.'
    )
    process.exit(1)
  }

  try {
    await assertDbConnectivity(CONFIG.databaseUrl)
  } catch (error: any) {
    console.error('❌ Database not reachable:', error?.message || String(error))
    process.exit(1)
  }

  const db = createDb(CONFIG.databaseUrl)

  // =========================================================================
  // Step 1: Verify existing data
  // =========================================================================
  printSection('📋 Step 1: Verify Existing Data')

  // Check source
  const sourceData = await db
    .select()
    .from(sources)
    .where(eq(sources.id, SOURCE_ID))
    .limit(1)

  if (sourceData.length === 0) {
    console.error(
      '❌ Source not found! Please ensure data exists in the database.'
    )
    process.exit(1)
  }

  console.log(`\n✅ Source: ${sourceData[0].name}`)
  console.log(`   Domain: ${sourceData[0].domain}`)
  console.log(`   Base URL: ${sourceData[0].baseUrl}`)
  const resolvedDomain =
    sourceData[0].domain || new URL(sourceData[0].baseUrl).hostname

  // Check chunks
  const chunksData = await db
    .select({
      chunkId: chunks.id,
      chunkIndex: chunks.chunkIndex,
      content: chunks.content,
      documentUrl: documents.url,
      documentTitle: documents.title,
    })
    .from(chunks)
    .innerJoin(documents, eq(chunks.documentId, documents.id))
    .where(eq(documents.sourceId, SOURCE_ID))
    .orderBy(desc(chunks.id))
    .limit(CHUNK_LIMIT)

  console.log(`\n📦 Found ${chunksData.length} chunks:`)
  for (const chunk of chunksData) {
    console.log(`   - Chunk ${chunk.chunkId}: ${chunk.documentTitle}`)
    console.log(`     Content preview: ${chunk.content.substring(0, 80)}...`)
  }

  // =========================================================================
  // Step 2: Generate recording tasks
  // =========================================================================
  printSection('🔄 Step 2: Generate Recording Tasks')

  const generator = new TaskGenerator(db)

  // Clear existing tasks ONLY for the chunks we will process (safe cleanup in shared DBs)
  console.log('\n🧹 Clearing existing tasks for selected chunks...')
  const selectedChunkIds = chunksData.map((c) => c.chunkId)
  if (selectedChunkIds.length > 0) {
    await db
      .delete(recordingTasks)
      .where(
        and(
          eq(recordingTasks.sourceId, SOURCE_ID),
          inArray(recordingTasks.chunkId, selectedChunkIds)
        )
      )
  }

  // Debug: check what chunks the generator sees
  const debugChunks = await db
    .select({
      chunkId: chunks.id,
      sourceId: sources.id,
      sourceDomain: sources.domain,
    })
    .from(chunks)
    .innerJoin(documents, eq(chunks.documentId, documents.id))
    .innerJoin(sources, eq(documents.sourceId, sources.id))
    .where(eq(sources.id, SOURCE_ID))
    .limit(10)
  console.log('\n📍 Debug - Chunks visible to generator:')
  for (const c of debugChunks) {
    console.log(
      `   chunk_id=${c.chunkId}, source_id=${c.sourceId}, domain=${c.sourceDomain}`
    )
  }

  // Generate tasks
  const generatedCount = await generator.generate(SOURCE_ID, CHUNK_LIMIT)
  console.log(`✅ Generated ${generatedCount} recording tasks`)

  // Query generated tasks (use raw SQL to ensure chunk_id is returned correctly)
  const tasksResult = await db.execute<{
    id: number
    source_id: number
    chunk_id: number | null
    start_url: string
    status: string
    config: any
  }>(sql`
    SELECT id, source_id, chunk_id, start_url, status, config
    FROM recording_tasks
    WHERE source_id = ${SOURCE_ID}
    ORDER BY id
  `)
  const tasks = tasksResult.rows

  console.log(`\n📊 Task Details:`)
  for (const task of tasks) {
    const chunkType = task.config?.chunk_type || 'unknown'
    console.log(
      `   - Task ${task.id}: chunk_id=${task.chunk_id}, type=${chunkType}, status=${task.status}`
    )
  }

  // Only keep tasks for the selected chunks from Step 1
  const filteredTasks =
    selectedChunkIds.length > 0
      ? tasks.filter(
          (t) => t.chunk_id !== null && selectedChunkIds.includes(t.chunk_id)
        )
      : tasks

  // =========================================================================
  // Step 3: Execute recording tasks
  // =========================================================================
  printSection('⚙️  Step 3: Execute Recording Tasks')

  const executor = new TaskExecutor(db, CONFIG)

  const executionResults: Array<{
    taskId: number
    success: boolean
    duration: number
    actions: number
    error?: string
  }> = []

  // Filter tasks if RUN_ONLY_CHUNK_INDEX is specified
  const tasksToRun =
    RUN_ONLY_CHUNK_INDEX > 0
      ? filteredTasks.filter((_, idx) => idx + 1 === RUN_ONLY_CHUNK_INDEX)
      : filteredTasks

  if (RUN_ONLY_CHUNK_INDEX > 0) {
    console.log(
      `\n⚠️  Only running chunk index ${RUN_ONLY_CHUNK_INDEX} (set RUN_ONLY_CHUNK_INDEX=0 to run all)`
    )
  }

  for (let i = 0; i < tasksToRun.length; i++) {
    const task = tasksToRun[i]
    const chunkType = task.config?.chunk_type || 'unknown'

    printSubsection(`Task ${i + 1}/${tasksToRun.length}: ${chunkType}`)
    console.log(`   Task ID: ${task.id}`)
    console.log(`   Chunk ID: ${task.chunk_id}`)
    console.log(`   Start URL: ${task.start_url}`)

    // Convert raw SQL result to RecordingTask format
    const taskForExecutor: RecordingTask = {
      id: task.id,
      sourceId: task.source_id,
      chunkId: task.chunk_id,
      startUrl: task.start_url,
      status: task.status as 'pending' | 'running' | 'completed' | 'failed',
      config: task.config,
      progress: 0,
      attemptCount: 0,
      errorMessage: null,
      completedAt: null,
      lastHeartbeat: null,
      createdAt: new Date(),
      updatedAt: new Date(),
    }

    try {
      console.log('\n   🎬 Starting recording...')
      const startTime = Date.now()
      const result = await executor.execute(taskForExecutor)
      void (Date.now() - startTime)

      executionResults.push({
        taskId: task.id,
        success: result.success,
        duration: result.duration_ms,
        actions: result.actions_created,
        error: result.error,
      })

      if (result.success) {
        console.log(`   ✅ Success!`)
        console.log(`      Duration: ${result.duration_ms}ms`)
        console.log(`      Actions created: ${result.actions_created}`)
        console.log(`      Turns: ${result.turns || 'N/A'}`)
        console.log(`      Tokens: ${result.tokens_used || 'N/A'}`)
        if (result.saved_path) {
          console.log(`      YAML saved: ${result.saved_path}`)
        }
      } else {
        console.log(`   ❌ Failed: ${result.error}`)
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      console.log(`   ❌ Exception: ${errorMsg}`)
      executionResults.push({
        taskId: task.id,
        success: false,
        duration: 0,
        actions: 0,
        error: errorMsg,
      })
    }
  }

  // =========================================================================
  // Step 4: Verify YAML output
  // =========================================================================
  printSection('📄 Step 4: Verify YAML Output')

  console.log(`\n📂 Output Directory: ${OUTPUT_DIR}`)
  const yamlDir = resolve(projectRoot, OUTPUT_DIR)
  const yamlPath = resolve(yamlDir, 'sites', resolvedDomain, 'site.yaml')

  if (fs.existsSync(yamlPath)) {
    console.log(`✅ YAML file exists: ${yamlPath}`)
    const yamlContent = fs.readFileSync(yamlPath, 'utf-8')
    const lines = yamlContent.split('\n')
    console.log(`   Lines: ${lines.length}`)
    console.log(`   Size: ${(yamlContent.length / 1024).toFixed(2)} KB`)
    console.log('\n   Preview (first 20 lines):')
    console.log('   ' + '-'.repeat(40))
    lines.slice(0, 20).forEach((line) => console.log(`   ${line}`))
    console.log('   ' + '-'.repeat(40))
  } else {
    console.log(`⚠️  YAML file not found: ${yamlPath}`)
  }

  // =========================================================================
  // Step 5: Verify Database output
  // =========================================================================
  printSection('🗄️  Step 5: Verify Database Output')

  // Check pages table
  const pagesData = await db
    .select()
    .from(pages)
    .where(eq(pages.sourceId, SOURCE_ID))

  console.log(`\n📄 Pages (${pagesData.length} records):`)
  for (const page of pagesData) {
    console.log(`   - ${page.pageType}: ${page.name}`)
  }

  // Check elements table
  const elementsData = await db
    .select({
      id: elements.id,
      semanticId: elements.semanticId,
      elementType: elements.elementType,
      description: elements.description,
      pageType: pages.pageType,
    })
    .from(elements)
    .innerJoin(pages, eq(elements.pageId, pages.id))
    .where(eq(pages.sourceId, SOURCE_ID))

  console.log(`\n🔲 Elements (${elementsData.length} records):`)
  for (const elem of elementsData) {
    console.log(
      `   - [${elem.pageType}] ${elem.semanticId} (${elem.elementType})`
    )
    if (elem.description) {
      console.log(`     ${elem.description.substring(0, 60)}...`)
    }
  }

  // Check updated task status
  const updatedTasks = await db
    .select()
    .from(recordingTasks)
    .where(eq(recordingTasks.sourceId, SOURCE_ID))

  console.log(`\n📊 Task Status:`)
  const statusCounts: Record<string, number> = {}
  for (const task of updatedTasks) {
    statusCounts[task.status] = (statusCounts[task.status] || 0) + 1
  }
  for (const [status, count] of Object.entries(statusCounts)) {
    console.log(`   - ${status}: ${count}`)
  }

  // =========================================================================
  // Step 6: Run validation
  // =========================================================================
  printSection('🔍 Step 6: Run Validation')

  if (pagesData.length === 0 || elementsData.length === 0) {
    console.log('⚠️  Skipping validation - no elements to validate')
  } else {
    console.log('\n🔄 Starting selector validation...')

    // AIClient auto-detects provider from environment variables
    const builder = new ActionBuilder({
      databaseUrl: CONFIG.databaseUrl,
      headless: false,
      outputDir: CONFIG.outputDir,
    })

    try {
      await builder.initialize()

      const validationResult = await builder.validate(resolvedDomain, {
        verbose: true,
      })

      console.log(`\n📊 Validation Results:`)
      console.log(`   Total Elements: ${validationResult.totalElements}`)
      console.log(`   Valid: ${validationResult.validElements}`)
      console.log(`   Invalid: ${validationResult.invalidElements}`)
      console.log(
        `   Validation Rate: ${(validationResult.validationRate * 100).toFixed(
          1
        )}%`
      )

      if (validationResult.details && validationResult.details.length > 0) {
        console.log(`\n   Details:`)
        for (const detail of validationResult.details.slice(0, 10)) {
          const icon = detail.valid ? '✅' : '❌'
          const message = detail.valid
            ? 'OK'
            : detail.selector?.css?.error ??
              detail.selector?.xpath?.error ??
              detail.selectorsDetail?.find((s) => !s.valid)?.error ??
              'Invalid'
          console.log(`   ${icon} ${detail.elementId}: ${message}`)
        }
        if (validationResult.details.length > 10) {
          console.log(`   ... and ${validationResult.details.length - 10} more`)
        }
      }

      await builder.close()
    } catch (error) {
      console.error('❌ Validation error:', error)
    }
  }

  // =========================================================================
  // Summary
  // =========================================================================
  printSection('📈 Summary')

  console.log(`\n✅ Pipeline Test Complete!`)
  console.log(`\n📊 Execution Results:`)
  for (const result of executionResults) {
    const icon = result.success ? '✅' : '❌'
    console.log(
      `   ${icon} Task ${result.taskId}: ${
        result.success ? 'Success' : 'Failed'
      }`
    )
    console.log(
      `      Duration: ${result.duration}ms, Actions: ${result.actions}`
    )
    if (result.error) {
      console.log(`      Error: ${result.error}`)
    }
  }

  console.log(`\n📦 Data Summary:`)
  console.log(`   - Source: ${sourceData[0].name} (ID: ${SOURCE_ID})`)
  console.log(`   - Chunks processed: ${chunksData.length}`)
  console.log(`   - Tasks created: ${filteredTasks.length}`)
  console.log(`   - Tasks executed: ${tasksToRun.length}`)
  console.log(`   - Pages recorded: ${pagesData.length}`)
  console.log(`   - Elements recorded: ${elementsData.length}`)

  console.log(`\n🔗 Verification SQL:`)
  console.log(`SELECT * FROM sources WHERE id = ${SOURCE_ID};`)
  console.log(`SELECT * FROM pages WHERE source_id = ${SOURCE_ID};`)
  console.log(
    `SELECT e.*, p.page_type FROM elements e JOIN pages p ON e.page_id = p.id WHERE p.source_id = ${SOURCE_ID};`
  )
  console.log(
    `SELECT id, status, progress, config->>'chunk_type' as chunk_type FROM recording_tasks WHERE source_id = ${SOURCE_ID};`
  )

  console.log(`\n📅 End Time: ${new Date().toISOString()}`)

  // Close database connection
  await closeDb(db)
}

// ============================================================================
// Run Test
// ============================================================================

runFullPipelineTest()
  .then(() => {
    console.log('\n✅ Test script finished successfully')
    process.exit(0)
  })
  .catch((error) => {
    console.error('\n❌ Test script failed:', error)
    process.exit(1)
  })
