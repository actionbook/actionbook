#!/usr/bin/env npx tsx
/**
 * M1 Data Verification Script
 *
 * Used to verify M1 complete pipeline and retain data in database
 *
 * Usage:
 *   npx tsx test/verify-m1-data.ts
 *
 * Verification steps:
 * 1. Create test data (1 source, 10 chunks)
 * 2. Generate 10 recording_tasks
 * 3. Execute all tasks (using mock ActionBuilder)
 * 4. Print database data
 * 5. Retain data (no cleanup)
 *
 * Note: This script uses mock ActionBuilder, no real browser operations
 */

import { config } from 'dotenv'
import { fileURLToPath } from 'url'
import { dirname, resolve } from 'path'

// Get __dirname equivalent in ES modules
const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

// Load environment variables
config({ path: resolve(__dirname, '../.env') })
config({ path: resolve(__dirname, '../../db/.env') })

import { TaskGenerator } from '../src/task-worker/task-generator'
import { TaskExecutor } from '../src/task-worker/task-executor'
import type { TaskExecutorConfig } from '../src/task-worker/types'
import {
  getDb,
  sources,
  documents,
  chunks,
  recordingTasks,
  eq,
} from '@actionbookdev/db'
import type { Database } from '@actionbookdev/db'

// Mock config for TaskExecutor
const mockConfig: TaskExecutorConfig = {
  llmApiKey: process.env.OPENROUTER_API_KEY || 'test-api-key',
  llmBaseURL: process.env.LLM_BASE_URL || 'https://openrouter.ai/api/v1',
  llmModel: process.env.LLM_MODEL || 'openai/gpt-4o',
  databaseUrl:
    process.env.DATABASE_URL ||
    'postgres://grasp:grasp@localhost:5432/actionbook',
  headless: true,
  maxTurns: 30,
  outputDir: './output',
}

async function verifyM1Data() {
  console.log('='.repeat(60))
  console.log('M1 Data Verification - Complete Pipeline Verification')
  console.log('='.repeat(60))
  console.log(
    '\n⚠️  Note: This script uses mock ActionBuilder, no real browser operations'
  )

  const db = getDb()

  const generator = new TaskGenerator(db)
  const executor = new TaskExecutor(db, mockConfig)

  // Step 1: Create test data
  console.log('\n📝 Step 1: Creating test data...')
  const timestamp = Date.now()

  const sourceResult = await db
    .insert(sources)
    .values({
      name: `m1_verification_${timestamp}`,
      baseUrl: `https://m1-test-${timestamp}.example.com`,
      description: 'M1 Verification Test Source',
      domain: `m1-test-${timestamp}.example.com`,
      crawlConfig: {},
    })
    .returning({ id: sources.id })

  const sourceId = sourceResult[0].id
  console.log(`✅ Created source: ID=${sourceId}, domain=m1-test.example.com`)

  // Create 10 chunks (5 task-driven, 5 exploratory)
  console.log('\n📦 Creating 10 chunks...')
  for (let i = 0; i < 10; i++) {
    const isTaskDriven = i < 5
    const content = isTaskDriven
      ? `Task: Test task ${i}\nSteps:\n1. Navigate to page\n2. Interact with element\n3. Verify result`
      : `# Test Page ${i}\n## Elements\n- Search input\n- Submit button\n- Result list`

    const docResult = await db
      .insert(documents)
      .values({
        sourceId,
        url: `https://m1-test.example.com/page${i}`,
        urlHash: `m1_hash_${timestamp}_${i}`,
        title: `M1 Test Page ${i} (${isTaskDriven ? 'Task' : 'Exploratory'})`,
        contentText: content,
      })
      .returning({ id: documents.id })

    const documentId = docResult[0].id

    await db.insert(chunks).values({
      documentId,
      content,
      contentHash: `m1_chunk_${timestamp}_${i}`,
      chunkIndex: i,
      startChar: 0,
      endChar: content.length,
      tokenCount: 100,
    })

    console.log(
      `  ✓ Chunk ${i}: ${isTaskDriven ? 'task_driven' : 'exploratory'}`
    )
  }

  // Step 2: Generate tasks
  console.log('\n🔄 Step 2: Generating tasks...')
  const generatedCount = await generator.generate(sourceId)
  console.log(`✅ Generated ${generatedCount} tasks`)

  // Verify tasks created
  const tasks = await db
    .select()
    .from(recordingTasks)
    .where(eq(recordingTasks.sourceId, sourceId))

  console.log(`\n📊 Task Status:`)
  const taskTypes = tasks.reduce((acc, t) => {
    const type = (t.config as any)?.chunk_type || 'unknown'
    acc[type] = (acc[type] || 0) + 1
    return acc
  }, {} as Record<string, number>)

  for (const [type, count] of Object.entries(taskTypes)) {
    console.log(`  - ${type}: ${count}`)
  }

  // Step 3: Execute all tasks
  console.log('\n⚙️  Step 3: Executing tasks...')
  console.log('   (Using mock ActionBuilder, no real LLM calls)')
  for (let i = 0; i < tasks.length; i++) {
    const task = tasks[i]
    const chunkType = (task.config as any)?.chunk_type
    console.log(
      `  [${i + 1}/${tasks.length}] Executing task ${task.id} (${chunkType})...`
    )

    try {
      const result = await executor.execute(task)

      if (result.success) {
        console.log(
          `    ✅ Success (duration: ${result.duration_ms}ms, actions: ${result.actions_created})`
        )
      } else {
        console.log(`    ❌ Failed: ${result.error}`)
      }
    } catch (error) {
      console.log(
        `    ❌ Error: ${
          error instanceof Error ? error.message : String(error)
        }`
      )
    }
  }

  // Step 4: Verify results
  console.log('\n📈 Step 4: Verification Results')
  console.log('='.repeat(60))

  const completedTasks = await db
    .select()
    .from(recordingTasks)
    .where(eq(recordingTasks.sourceId, sourceId))

  const statusCount = completedTasks.reduce((acc, t) => {
    acc[t.status] = (acc[t.status] || 0) + 1
    return acc
  }, {} as Record<string, number>)

  console.log('\n✅ Task Execution Summary:')
  for (const [status, count] of Object.entries(statusCount)) {
    console.log(`  - ${status}: ${count}`)
  }

  console.log('\n📊 Database Data:')
  console.log(`  - sources: 1 record (ID=${sourceId})`)
  console.log(`  - documents: 10 records`)
  console.log(`  - chunks: 10 records`)
  console.log(`  - recording_tasks: ${completedTasks.length} records`)

  // Print SQL queries for verification
  console.log('\n🔍 SQL Verification Queries:')
  console.log('='.repeat(60))
  console.log('\n-- 1. Check source:')
  console.log(`SELECT * FROM sources WHERE id = ${sourceId};`)

  console.log('\n-- 2. Check documents:')
  console.log(
    `SELECT id, url, title FROM documents WHERE source_id = ${sourceId};`
  )

  console.log('\n-- 3. Check chunks:')
  console.log(`SELECT c.id, c.chunk_index, LEFT(c.content, 50) as content_preview
FROM chunks c
JOIN documents d ON c.document_id = d.id
WHERE d.source_id = ${sourceId}
ORDER BY c.chunk_index;`)

  console.log('\n-- 4. Check recording_tasks:')
  console.log(`SELECT id, status, progress, config->>'chunk_type' as chunk_type, attempt_count, completed_at
FROM recording_tasks
WHERE source_id = ${sourceId}
ORDER BY id;`)

  console.log('\n-- 5. Check complete pipeline:')
  console.log(`SELECT
  s.domain,
  d.title as document_title,
  c.chunk_index,
  rt.status as task_status,
  rt.config->>'chunk_type' as chunk_type,
  rt.progress,
  rt.attempt_count
FROM sources s
JOIN documents d ON d.source_id = s.id
JOIN chunks c ON c.document_id = d.id
LEFT JOIN recording_tasks rt ON rt.chunk_id = c.id
WHERE s.id = ${sourceId}
ORDER BY c.chunk_index;`)

  console.log('\n' + '='.repeat(60))
  console.log('✅ M1 Verification Complete!')
  console.log('='.repeat(60))
  console.log('\n💡 Data retained in database, use above SQL queries to verify')
  console.log(
    `\n🔗 Database: postgresql://grasp:grasp@localhost:5432/actionbook`
  )
  console.log(`📌 Source ID: ${sourceId}`)
  console.log('\nTo clean up data, run:')
  console.log(`DELETE FROM recording_tasks WHERE source_id = ${sourceId};`)
  console.log(
    `DELETE FROM chunks WHERE document_id IN (SELECT id FROM documents WHERE source_id = ${sourceId});`
  )
  console.log(`DELETE FROM documents WHERE source_id = ${sourceId};`)
  console.log(`DELETE FROM sources WHERE id = ${sourceId};`)
}

// Run verification
verifyM1Data()
  .then(() => {
    console.log('\n✅ Script finished successfully')
    process.exit(0)
  })
  .catch((error) => {
    console.error('\n❌ Error:', error)
    process.exit(1)
  })
