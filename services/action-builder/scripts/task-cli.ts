#!/usr/bin/env npx tsx
/**
 * Task CLI - Simple Task Management CLI Tool
 *
 * Usage:
 *   pnpm task:create <source_id> [limit]  - Create tasks (from chunks without elements)
 *   pnpm task:status [source_id]          - View task status
 *   pnpm task:run <source_id> [limit]     - Execute tasks
 */

import 'dotenv/config'
import {
  getDb,
  chunks,
  documents,
  sources,
  elements,
  recordingTasks,
  eq,
  sql,
  isNull,
  desc,
} from '@actionbookdev/db'
import { detectChunkType } from '../src/task-worker/utils/chunk-detector.js'
import { TaskExecutor } from '../src/task-worker/task-executor.js'

const db = getDb()

// ============================================================
// Commands
// ============================================================

/**
 * Create tasks - select chunks without elements
 */
async function createTasks(
  sourceId: number,
  limit: number = 10
): Promise<void> {
  console.log(`\n📋 Creating tasks for source_id=${sourceId}, limit=${limit}\n`)

  // 1. Query chunks without elements (via LEFT JOIN elements check)
  const chunksWithoutElements = await db
    .select({
      chunkId: chunks.id,
      sourceId: sources.id,
      chunkContent: chunks.content,
      documentUrl: documents.url,
      documentTitle: documents.title,
      sourceDomain: sources.domain,
    })
    .from(chunks)
    .innerJoin(documents, eq(chunks.documentId, documents.id))
    .innerJoin(sources, eq(documents.sourceId, sources.id))
    .leftJoin(recordingTasks, eq(chunks.id, recordingTasks.chunkId))
    .where(eq(sources.id, sourceId))
    .where(isNull(recordingTasks.id)) // No corresponding task
    .limit(limit)

  console.log(`📦 Found ${chunksWithoutElements.length} chunks without tasks\n`)

  if (chunksWithoutElements.length === 0) {
    console.log('✅ All chunks already have tasks')
    return
  }

  // 2. Create task for each chunk
  let created = 0
  for (const chunk of chunksWithoutElements) {
    const chunkType = detectChunkType(chunk.chunkContent)
    const config = JSON.stringify({ chunk_type: chunkType })

    try {
      const scenario = `task_${Date.now()}_chunk_${chunk.chunkId}`
      const result = await db.execute<{ id: number }>(sql`
        INSERT INTO recording_tasks (source_id, chunk_id, start_url, status, config, attempt_count, scenario)
        VALUES (${chunk.sourceId}, ${chunk.chunkId}, ${chunk.documentUrl}, 'pending', ${config}::jsonb, 0, ${scenario})
        RETURNING id
      `)
      const taskId = result.rows[0].id
      console.log(
        `  ✅ Task ${taskId}: chunk=${chunk.chunkId}, type=${chunkType}`
      )
      created++
    } catch (error) {
      console.error(`  ❌ Failed for chunk ${chunk.chunkId}:`, error)
    }
  }

  console.log(`\n🎉 Created ${created} tasks`)
}

/**
 * View task status
 */
async function showStatus(sourceId?: number): Promise<void> {
  console.log('\n📊 Task Status\n')

  // Get all sources or specified source
  let sourceList
  if (sourceId) {
    sourceList = await db.select().from(sources).where(eq(sources.id, sourceId))
  } else {
    sourceList = await db.select().from(sources)
  }

  for (const source of sourceList) {
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, source.id))

    if (tasks.length === 0) continue

    const pending = tasks.filter((t) => t.status === 'pending').length
    const running = tasks.filter((t) => t.status === 'running').length
    const completed = tasks.filter((t) => t.status === 'completed').length
    const failed = tasks.filter((t) => t.status === 'failed').length

    console.log(`📁 Source ${source.id}: ${source.domain}`)
    console.log(`   Total: ${tasks.length}`)
    console.log(`   ⏳ Pending:   ${pending}`)
    console.log(`   🔄 Running:   ${running}`)
    console.log(`   ✅ Completed: ${completed}`)
    console.log(`   ❌ Failed:    ${failed}`)
    console.log('')

    // Show task details
    if (tasks.length <= 20) {
      console.log('   Tasks:')
      for (const task of tasks) {
        const icon =
          task.status === 'completed'
            ? '✅'
            : task.status === 'running'
            ? '🔄'
            : task.status === 'failed'
            ? '❌'
            : '⏳'
        const chunkType = (task.config as any)?.chunk_type || 'unknown'
        console.log(
          `   ${icon} Task ${task.id}: chunk=${task.chunkId}, type=${chunkType}, status=${task.status}`
        )
        if (task.errorMessage) {
          console.log(`      Error: ${task.errorMessage.substring(0, 100)}...`)
        }
      }
      console.log('')
    }
  }
}

/**
 * Execute tasks
 */
async function runTasks(sourceId: number, limit: number = 1): Promise<void> {
  console.log(`\n🚀 Running tasks for source_id=${sourceId}, limit=${limit}\n`)

  // Get pending tasks
  const pendingTasks = await db
    .select()
    .from(recordingTasks)
    .where(eq(recordingTasks.sourceId, sourceId))
    .where(eq(recordingTasks.status, 'pending'))
    .orderBy(recordingTasks.id)
    .limit(limit)

  if (pendingTasks.length === 0) {
    console.log('✅ No pending tasks')
    return
  }

  console.log(`📋 Found ${pendingTasks.length} pending tasks\n`)

  // Create TaskExecutor
  // LLM configuration is auto-detected from environment variables by AIClient
  // Priority: OPENROUTER > OPENAI > ANTHROPIC > BEDROCK
  const executor = new TaskExecutor(db, {
    databaseUrl: process.env.DATABASE_URL || '',
    headless: process.env.HEADLESS !== 'false',
    maxTurns: parseInt(process.env.MAX_TURNS || '30'),
    outputDir: './output',
  })

  // Execute tasks
  for (const task of pendingTasks) {
    console.log(`\n--------------------------------------------------`)
    console.log(`🎬 Executing Task ${task.id}`)
    console.log(`   Chunk: ${task.chunkId}`)
    console.log(`   Type: ${(task.config as any)?.chunk_type}`)
    console.log(`--------------------------------------------------\n`)

    try {
      const result = await executor.execute({
        id: task.id,
        sourceId: task.sourceId,
        chunkId: task.chunkId,
        startUrl: task.startUrl,
        status: task.status as any,
        progress: task.progress,
        config: task.config as any,
        attemptCount: task.attemptCount,
        errorMessage: task.errorMessage,
        completedAt: task.completedAt,
        lastHeartbeat: task.lastHeartbeat,
        createdAt: task.createdAt,
        updatedAt: task.updatedAt,
      })

      if (result.success) {
        console.log(`\n✅ Task ${task.id} completed!`)
        console.log(`   Elements created: ${result.actions_created}`)
        console.log(`   Duration: ${(result.duration_ms / 1000).toFixed(1)}s`)
        console.log(`   Turns: ${result.turns}`)
      } else {
        console.log(`\n❌ Task ${task.id} failed: ${result.error}`)
      }
    } catch (error) {
      console.error(`\n❌ Task ${task.id} error:`, error)
    }
  }
}

/**
 * Clear tasks
 */
async function clearTasks(sourceId: number): Promise<void> {
  console.log(`\n🧹 Clearing tasks for source_id=${sourceId}\n`)

  const result = await db
    .delete(recordingTasks)
    .where(eq(recordingTasks.sourceId, sourceId))

  console.log(`✅ Cleared tasks for source ${sourceId}`)
}

// ============================================================
// Main
// ============================================================

async function main() {
  const args = process.argv.slice(2)
  const command = args[0]

  try {
    switch (command) {
      case 'create':
        const createSourceId = parseInt(args[1])
        const createLimit = parseInt(args[2]) || 10
        if (isNaN(createSourceId)) {
          console.error('Usage: pnpm task:create <source_id> [limit]')
          process.exit(1)
        }
        await createTasks(createSourceId, createLimit)
        break

      case 'status':
        const statusSourceId = args[1] ? parseInt(args[1]) : undefined
        await showStatus(statusSourceId)
        break

      case 'run':
        const runSourceId = parseInt(args[1])
        const runLimit = parseInt(args[2]) || 1
        if (isNaN(runSourceId)) {
          console.error('Usage: pnpm task:run <source_id> [limit]')
          process.exit(1)
        }
        await runTasks(runSourceId, runLimit)
        break

      case 'clear':
        const clearSourceId = parseInt(args[1])
        if (isNaN(clearSourceId)) {
          console.error('Usage: pnpm task:clear <source_id>')
          process.exit(1)
        }
        await clearTasks(clearSourceId)
        break

      default:
        console.log(`
📋 Task CLI - Task Management Tool

Commands:
  pnpm task:create <source_id> [limit]  Create tasks (from chunks without tasks)
  pnpm task:status [source_id]          View task status
  pnpm task:run <source_id> [limit]     Execute tasks
  pnpm task:clear <source_id>           Clear tasks

Examples:
  pnpm task:create 1 10    # Create up to 10 tasks for source 1
  pnpm task:status         # View task status for all sources
  pnpm task:status 1       # View task status for source 1
  pnpm task:run 1 2        # Execute 2 pending tasks for source 1
  pnpm task:clear 1        # Clear all tasks for source 1
`)
        break
    }
  } catch (error) {
    console.error('Error:', error)
    process.exit(1)
  }

  process.exit(0)
}

main()
