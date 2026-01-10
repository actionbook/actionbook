#!/usr/bin/env npx tsx
/**
 * Crawl Playbook Batch Runner
 *
 * Sequentially calls test/e2e/crawl-playbook.ts to crawl websites and aggregates JSON results.
 *
 * Usage:
 *   npx tsx scripts/crawl-playbook-batch.ts
 *   npx tsx scripts/crawl-playbook-batch.ts --max-pages 6 --max-depth 1 --concurrency 3
 *   npx tsx scripts/crawl-playbook-batch.ts --output ./output --summary-output ./output/batch_summary.json
 *
 * Notes:
 * - Each site's output is still written by crawl-playbook.ts: output/sites/{domain}/crawl_playbooks/*.json
 * - This script parses "JSON saved:" lines from stdout to locate files, then reads and aggregates them
 */

import fs from 'node:fs'
import path from 'node:path'
import { spawn } from 'node:child_process'

type SiteRunStatus = 'completed' | 'failed' | 'timeout'

type SiteRunResult = {
  url: string
  status: SiteRunStatus
  startedAt: string
  finishedAt: string
  durationMs: number
  exitCode: number | null
  signal: NodeJS.Signals | null
  yamlPath?: string
  jsonPath?: string
  error?: string
  // Parsed crawl-playbook JSON (when available)
  data?: unknown
}

type BatchSummary = {
  metadata: {
    createdAt: string
    total: number
    completed: number
    failed: number
    timeout: number
    batchArgs: string[]
  }
  results: SiteRunResult[]
}

const DEFAULT_SITES: string[] = [
  'https://lib.rs/',
  'https://crates.io/',
  'https://www.wolframalpha.com',
  'https://www.allrecipes.com',
  'https://www.booking.com',
  'https://www.espn.com',
  'https://www.apple.com',
  'https://arxiv.org',
  'https://dictionary.cambridge.org',
  'https://www.google.com',
  'https://huggingface.co',
  'https://www.bbc.com/news',
  'https://www.coursera.org',
  'https://www.google.com/flights',
  'https://www.amazon.com',
  'https://github.com',
  'https://www.google.com/maps',
]

function nowIso(): string {
  return new Date().toISOString()
}

function timestampForFile(): string {
  return new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
}

function parseArgValue(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name)
  if (idx === -1) return undefined
  return args[idx + 1]
}

function hasFlag(args: string[], name: string): boolean {
  return args.includes(name)
}

function ensureDir(dirPath: string) {
  fs.mkdirSync(dirPath, { recursive: true })
}

function getActionBuilderRoot(): string {
  // scripts/ -> action-builder root is one level up
  return path.resolve(path.dirname(new URL(import.meta.url).pathname), '..')
}

function resolveToxPath(actionBuilderRoot: string): string {
  // Prefer local tsx binary to avoid npx overhead
  const tsxPath = path.join(actionBuilderRoot, 'node_modules', '.bin', 'tsx')
  if (fs.existsSync(tsxPath)) return tsxPath
  // Fallback to npx/tsx on PATH
  return 'tsx'
}

function extractSavedPaths(output: string): { yamlPath?: string; jsonPath?: string } {
  // crawl-playbook.ts prints:
  // 📁 YAML saved: <path>
  // 📁 JSON saved: <path>
  const yamlMatch = output.match(/YAML saved:\s*(.+)\s*$/m)
  const jsonMatch = output.match(/JSON saved:\s*(.+)\s*$/m)
  return {
    yamlPath: yamlMatch?.[1]?.trim(),
    jsonPath: jsonMatch?.[1]?.trim(),
  }
}

async function runOneSite(options: {
  actionBuilderRoot: string
  tsxPath: string
  url: string
  forwardArgs: string[]
  timeoutMs: number
}): Promise<SiteRunResult> {
  const startedAt = nowIso()
  const t0 = Date.now()

  const childArgs = ['test/e2e/crawl-playbook.ts', options.url, ...options.forwardArgs]

  console.log('\n' + '='.repeat(80))
  console.log(`🌐 Site: ${options.url}`)
  console.log(`▶️  Run: ${options.tsxPath} ${childArgs.join(' ')}`)
  console.log('='.repeat(80))

  const child = spawn(options.tsxPath, childArgs, {
    cwd: options.actionBuilderRoot,
    env: process.env,
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  let combined = ''
  let timedOut = false

  const append = (chunk: Buffer) => {
    const s = chunk.toString('utf8')
    combined += s
    return s
  }

  child.stdout.on('data', (d: Buffer) => {
    process.stdout.write(append(d))
  })

  child.stderr.on('data', (d: Buffer) => {
    process.stderr.write(append(d))
  })

  const timeout = setTimeout(() => {
    timedOut = true
    try {
      child.kill('SIGTERM')
    } catch {
      // ignore
    }
  }, options.timeoutMs)

  const { exitCode, signal } = await new Promise<{
    exitCode: number | null
    signal: NodeJS.Signals | null
  }>((resolve) => {
    child.on('close', (code, sig) => resolve({ exitCode: code, signal: sig }))
  })

  clearTimeout(timeout)

  const finishedAt = nowIso()
  const durationMs = Date.now() - t0

  const { yamlPath, jsonPath } = extractSavedPaths(combined)

  const result: SiteRunResult = {
    url: options.url,
    status: timedOut ? 'timeout' : exitCode === 0 ? 'completed' : 'failed',
    startedAt,
    finishedAt,
    durationMs,
    exitCode,
    signal,
    yamlPath,
    jsonPath,
  }

  if (timedOut) {
    result.error = `timeout after ${options.timeoutMs}ms`
    return result
  }

  if (exitCode !== 0) {
    result.error = `crawl-playbook exited with code ${exitCode}${signal ? ` (signal=${signal})` : ''}`
    return result
  }

  if (!jsonPath) {
    result.status = 'failed'
    result.error = 'cannot find "JSON saved:" line in output'
    return result
  }

  try {
    const absJsonPath = path.isAbsolute(jsonPath)
      ? jsonPath
      : path.resolve(options.actionBuilderRoot, jsonPath)
    const raw = fs.readFileSync(absJsonPath, 'utf-8')
    result.jsonPath = absJsonPath
    result.data = JSON.parse(raw) as unknown
  } catch (e: any) {
    result.status = 'failed'
    result.error = `failed to read/parse json: ${String(e?.message ?? e)}`
  }

  if (yamlPath && !path.isAbsolute(yamlPath)) {
    result.yamlPath = path.resolve(options.actionBuilderRoot, yamlPath)
  }

  return result
}

async function main() {
  const argv = process.argv.slice(2)

  // Script-level options
  const summaryOutputArg = parseArgValue(argv, '--summary-output')
  const timeoutMinutesArg = parseArgValue(argv, '--site-timeout-minutes')
  const continueOnError = !hasFlag(argv, '--stop-on-error')

  // Forward args to crawl-playbook.ts (strip batch-only flags)
  const forwardArgs: string[] = []
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    const next = argv[i + 1]

    if (a === '--summary-output' || a === '--site-timeout-minutes') {
      i++ // skip value
      continue
    }
    if (a === '--stop-on-error') {
      continue
    }

    // pass-through for crawl-playbook.ts options
    if (
      a === '--output' ||
      a === '--max-depth' ||
      a === '--max-pages' ||
      a === '--timeout' ||
      a === '--html-limit' ||
      a === '--concurrency'
    ) {
      if (!next) {
        console.error(`❌ Missing value for ${a}`)
        process.exit(1)
      }
      forwardArgs.push(a, next)
      i++
      continue
    }

    // Unknown flags are ignored to keep script resilient
  }

  const actionBuilderRoot = getActionBuilderRoot()
  const tsxPath = resolveToxPath(actionBuilderRoot)

  const timeoutMinutes = timeoutMinutesArg ? parseInt(timeoutMinutesArg, 10) : 60
  const timeoutMs = Math.max(1, timeoutMinutes) * 60_000

  const outputDirArg = parseArgValue(forwardArgs, '--output') ?? './output'
  const defaultSummaryPath = path.resolve(
    actionBuilderRoot,
    outputDirArg,
    'batch',
    `crawl_playbook_batch_summary_${timestampForFile()}.json`
  )
  const summaryOutputPath = summaryOutputArg
    ? path.isAbsolute(summaryOutputArg)
      ? summaryOutputArg
      : path.resolve(actionBuilderRoot, summaryOutputArg)
    : defaultSummaryPath

  ensureDir(path.dirname(summaryOutputPath))

  console.log('='.repeat(80))
  console.log('Crawl Playbook - Batch Runner')
  console.log('='.repeat(80))
  console.log(`Action Builder Root: ${actionBuilderRoot}`)
  console.log(`Sites: ${DEFAULT_SITES.length}`)
  console.log(`Per-site Timeout: ${timeoutMinutes} minutes`)
  console.log(`Forward Args: ${forwardArgs.join(' ') || '(none)'}`)
  console.log(`Summary Output: ${summaryOutputPath}`)
  console.log(`Stop on error: ${continueOnError ? 'false' : 'true'}`)
  console.log('='.repeat(80))

  const results: SiteRunResult[] = []

  for (const url of DEFAULT_SITES) {
    const r = await runOneSite({
      actionBuilderRoot,
      tsxPath,
      url,
      forwardArgs,
      timeoutMs,
    })
    results.push(r)

    const statusIcon = r.status === 'completed' ? '✅' : r.status === 'timeout' ? '⏱️' : '❌'
    console.log(
      `\n${statusIcon} Done: ${url} (${(r.durationMs / 1000).toFixed(1)}s) status=${r.status}`
    )
    if (r.error) console.log(`   Error: ${r.error}`)
    if (!continueOnError && r.status !== 'completed') {
      console.log('\n🛑 Stop-on-error enabled, exiting early.')
      break
    }
  }

  const summary: BatchSummary = {
    metadata: {
      createdAt: nowIso(),
      total: results.length,
      completed: results.filter((r) => r.status === 'completed').length,
      failed: results.filter((r) => r.status === 'failed').length,
      timeout: results.filter((r) => r.status === 'timeout').length,
      batchArgs: argv,
    },
    results,
  }

  fs.writeFileSync(summaryOutputPath, JSON.stringify(summary, null, 2), 'utf-8')

  console.log('\n' + '='.repeat(80))
  console.log('✅ Batch Summary Saved')
  console.log(`📁 ${summaryOutputPath}`)
  console.log(
    `Stats: completed=${summary.metadata.completed}, failed=${summary.metadata.failed}, timeout=${summary.metadata.timeout}, total=${summary.metadata.total}`
  )
  console.log('='.repeat(80))
}

main().catch((error) => {
  console.error('Fatal error:', error)
  process.exit(1)
})

