#!/usr/bin/env npx tsx
/**
 * Build Task Worker - Continuous polling for action_build stage
 *
 * Usage:
 *   pnpm worker:build-task              # Start with default 30s interval
 *   pnpm worker:build-task --once       # Run once and exit
 *   pnpm worker:build-task --interval 60  # Custom interval (seconds)
 *   pnpm worker:build-task --debug      # Run with headed browser (visible window)
 *   pnpm worker:build-task --headed     # Same as --debug
 *
 * Environment Variables:
 *   ACTION_BUILDER_PROFILE_ENABLED=false # Disable browser profile (enabled by default)
 *   ACTION_BUILDER_PROFILE_DIR=.browser-profile  # Profile directory path
 */

import 'dotenv/config'
import { getDb } from '@actionbookdev/db'
import { BuildTaskWorker } from '../src/task-worker/build-task-worker.js'
import {
  BuildTaskScheduler,
  type TaskStatsResult,
} from '../src/task-worker/build-task-scheduler.js'
import type {
  BuildTaskResult,
  BuildTaskWorkerConfig,
} from '../src/task-worker/types/index.js'

const db = getDb()

interface CliOptions {
  once: boolean
  interval: number // seconds
  debug: boolean // headed browser mode for debugging
}

const DEFAULT_INTERVAL = 30 // seconds

function parseArgs(): CliOptions {
  const args = process.argv.slice(2)
  return {
    once: args.includes('--once'),
    debug: args.includes('--debug') || args.includes('--headed'),
    interval: (() => {
      const idx = args.indexOf('--interval')
      if (idx !== -1 && args[idx + 1]) {
        const parsed = parseInt(args[idx + 1], 10)
        if (Number.isFinite(parsed) && parsed > 0) {
          return parsed
        }
        console.warn(
          `[BuildTaskWorker] Invalid interval "${
            args[idx + 1]
          }", using default ${DEFAULT_INTERVAL}s`
        )
      }
      return DEFAULT_INTERVAL
    })(),
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function logResult(result: BuildTaskResult): void {
  if (result.success) {
    console.log(
      `\n✅ [BuildTaskWorker] Task ${result.taskId} completed successfully!`
    )
    console.log(`   Recording tasks created: ${result.recordingTasksCreated}`)
    console.log(
      `   Recording tasks completed: ${result.recordingTasksCompleted}`
    )
    console.log(`   Recording tasks failed: ${result.recordingTasksFailed}`)
    console.log(`   Elements created: ${result.elementsCreated}`)
    console.log(`   Duration: ${(result.duration_ms / 1000).toFixed(1)}s`)
  } else {
    console.log(
      `\n❌ [BuildTaskWorker] Task ${result.taskId} failed: ${result.error}`
    )
    console.log(`   Duration: ${(result.duration_ms / 1000).toFixed(1)}s`)
  }
}

function logTaskStats(stats: TaskStatsResult): void {
  const { buildTasks, recordingTasks } = stats
  const kb = buildTasks.knowledge_build
  const ab = buildTasks.action_build
  const rt = recordingTasks

  console.log(
    `[TaskStats] knowledge_build(P:${kb.pending}/R:${kb.running}) ` +
      `action_build(P:${ab.pending}/R:${ab.running}) ` +
      `recording_tasks(P:${rt.pending}/R:${rt.running})`
  )
}

function logWorkerStats(worker: BuildTaskWorker): void {
  const { busy, idle, total } = worker.getWorkerStats()
  console.log(`[WorkerStats] workers(B:${busy}/I:${idle}/T:${total})`)
}

async function main() {
  const options = parseArgs()

  // Debug mode: show browser window for debugging
  const isDebugMode = options.debug
  const headless = isDebugMode
    ? false
    : process.env.ACTION_BUILDER_HEADLESS !== 'false'

  console.log('\n===========================================')
  console.log('  Build Task Worker')
  console.log('===========================================')
  console.log(`  Mode: ${options.once ? 'Single run' : 'Continuous polling'}`)
  console.log(`  Poll interval: ${options.interval}s`)
  console.log(
    `  Concurrency: ${
      process.env.ACTION_BUILDER_TASK_CONCURRENCY || '3'
    } workers`
  )
  console.log(`  Browser: ${headless ? 'headless' : 'headed (debug mode)'}`)
  console.log('===========================================\n')

  // Profile configuration: enable browser profile for persistent login state (default: enabled)
  const profileEnabled = process.env.ACTION_BUILDER_PROFILE_ENABLED !== 'false'
  const profileDir =
    process.env.ACTION_BUILDER_PROFILE_DIR || '.browser-profile'

  console.log(
    `  Profile: ${profileEnabled ? `enabled (${profileDir})` : 'disabled'}`
  )

  // LLM configuration is auto-detected from environment variables by AIClient
  // Priority: OPENROUTER > OPENAI > ANTHROPIC > BEDROCK
  const config: BuildTaskWorkerConfig = {
    databaseUrl: process.env.DATABASE_URL || '',
    headless,
    maxTurns: parseInt(process.env.ACTION_BUILDER_MAX_TURNS || '30'),
    outputDir: './output',
    recordingTaskLimit: parseInt(
      process.env.ACTION_BUILDER_RECORDING_TASK_LIMIT || '500'
    ),
    maxAttempts: parseInt(process.env.ACTION_BUILDER_MAX_ATTEMPTS || '3'),
    staleTimeoutMinutes: parseInt(
      process.env.ACTION_BUILDER_STALE_TIMEOUT_MINUTES || '30'
    ),
    concurrency: parseInt(process.env.ACTION_BUILDER_TASK_CONCURRENCY || '3'),
    profileEnabled,
    profileDir,
  }

  const worker = new BuildTaskWorker(db, config)
  const scheduler = new BuildTaskScheduler(db, {
    maxAttempts: config.maxAttempts,
    staleTimeoutMinutes: config.staleTimeoutMinutes,
  })

  // Track if shutdown is in progress
  let isShuttingDown = false

  // Graceful shutdown handler
  const gracefulShutdown = async (signal: string) => {
    if (isShuttingDown) {
      console.log(`\n[BuildTaskWorker] Already shutting down, please wait...`)
      return
    }
    isShuttingDown = true

    console.log(
      `\n[BuildTaskWorker] Received ${signal}, shutting down gracefully...`
    )
    console.log(
      '[BuildTaskWorker] Waiting for current task to complete and browser to close...'
    )

    try {
      // Give the worker pool time to finish current work and close browsers
      await worker.shutdown()
      console.log('[BuildTaskWorker] Worker pool shutdown complete')
    } catch (error) {
      console.error('[BuildTaskWorker] Error during shutdown:', error)
    }

    console.log('[BuildTaskWorker] Goodbye!')
    process.exit(0)
  }

  // Register signal handlers
  process.on('SIGINT', () => gracefulShutdown('SIGINT'))
  process.on('SIGTERM', () => gracefulShutdown('SIGTERM'))

  if (options.once) {
    // Single run mode
    console.log('[BuildTaskWorker] Running once...')
    const stats = await scheduler.getTaskStats()
    logTaskStats(stats)
    logWorkerStats(worker)
    const result = await worker.runOnce()
    if (result) {
      logResult(result)
    } else {
      console.log('[BuildTaskWorker] No tasks available')
    }
    process.exit(0)
  }

  // Continuous polling mode
  console.log('[BuildTaskWorker] Starting continuous polling...')
  console.log('[BuildTaskWorker] Press Ctrl+C to stop\n')

  while (true) {
    try {
      const timestamp = new Date().toISOString()
      console.log(`\n[${timestamp}] Checking for tasks...`)

      // Log task and worker statistics
      const stats = await scheduler.getTaskStats()
      logTaskStats(stats)
      logWorkerStats(worker)

      const result = await worker.runOnce()

      if (result) {
        logResult(result)
      } else {
        console.log('[BuildTaskWorker] No tasks available')
      }
    } catch (error) {
      console.error('[BuildTaskWorker] Error:', error)
    }

    console.log(`[BuildTaskWorker] Sleeping for ${options.interval}s...`)
    await sleep(options.interval * 1000)
  }
}

main().catch((error) => {
  console.error('[BuildTaskWorker] Fatal error:', error)
  process.exit(1)
})
