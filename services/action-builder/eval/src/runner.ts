#!/usr/bin/env npx tsx
/**
 * Action-Builder Eval Runner
 *
 * Main entry point for running evaluations with Braintrust.
 *
 * Usage:
 *   pnpm eval --dataset=smoke
 *   pnpm eval --dataset=smoke --tags=search
 *   pnpm eval --dataset=smoke --env=all
 */

import dotenv from 'dotenv'
import path from 'path'

// Load .env file from eval directory
dotenv.config({ path: path.resolve(import.meta.dirname, '../.env') })

import { Eval } from 'braintrust'
import { loadTestcases, filterByTags, listDatasets } from './suites/loader.js'
import { offlineEvalTask, setOfflineEvalOptions } from './tasks/offline_eval.js'
import { onlineEvalTask, setOnlineEvalOptions } from './tasks/online_eval.js'
import { recallScorer } from './scorers/recall.js'
import { redundancyScorer } from './scorers/redundancy.js'
import { robustnessScorer } from './scorers/robustness.js'
import { listEnvironments } from './utils/env_matrix.js'
import { setCurrentExperiment } from './utils/eval_logger.js'
import type { EvalInput, EvalOutput } from './types.js'

// Parse CLI arguments
function parseArgs(): {
  dataset: string
  tags: string[]
  envs: string[]
  online: boolean
  skipRobustness: boolean
  maxTurns: number
  help: boolean
  list: boolean
  listEnvs: boolean
} {
  const args = process.argv.slice(2)
  const result = {
    dataset: 'smoke',
    tags: [] as string[],
    envs: [] as string[],
    online: false,
    skipRobustness: false,
    maxTurns: 20,
    help: false,
    list: false,
    listEnvs: false,
  }

  for (const arg of args) {
    if (arg === '--help' || arg === '-h') {
      result.help = true
    } else if (arg === '--list' || arg === 'list') {
      result.list = true
    } else if (arg === '--list-envs') {
      result.listEnvs = true
    } else if (arg === '--online') {
      result.online = true
    } else if (arg === '--skip-robustness' || arg === '--no-robustness') {
      result.skipRobustness = true
    } else if (arg.startsWith('--dataset=')) {
      result.dataset = arg.replace('--dataset=', '')
    } else if (arg.startsWith('--tags=')) {
      result.tags = arg.replace('--tags=', '').split(',')
    } else if (arg.startsWith('--env=')) {
      const envArg = arg.replace('--env=', '')
      result.envs = envArg === 'all' ? ['all'] : envArg.split(',')
    } else if (arg.startsWith('--max-turns=')) {
      result.maxTurns = parseInt(arg.replace('--max-turns=', ''), 10)
    }
  }

  return result
}

function printHelp(): void {
  console.log(`
Action-Builder Eval Runner

Usage:
  pnpm eval [options]

Options:
  --dataset=<name>     Dataset to evaluate (default: smoke)
  --tags=<tags>        Filter by tags (comma-separated)
  --online             Run ActionBuilder.build() instead of loading fixtures
  --max-turns=<n>      Max LLM turns for online mode (default: 20)
  --env=<envs>         Test environments for robustness (comma-separated, or "all")
  --skip-robustness    Skip robustness validation (faster, no browser)
  --list               List available datasets
  --list-envs          List available test environments
  --help, -h           Show this help message

Examples:
  pnpm eval --dataset=smoke                    # Offline eval with fixtures
  pnpm eval --dataset=smoke --online           # Online eval (runs ActionBuilder)
  pnpm eval --dataset=smoke --online --max-turns=30
  pnpm eval --dataset=smoke --skip-robustness  # Quick eval without robustness
  pnpm eval --dataset=smoke --env=all          # Test all environments
  pnpm eval list
`)
}

async function main(): Promise<void> {
  const args = parseArgs()

  if (args.help) {
    printHelp()
    process.exit(0)
  }

  if (args.list) {
    const datasets = listDatasets()
    console.log('Available datasets:')
    for (const ds of datasets) {
      console.log(`  - ${ds}`)
    }
    process.exit(0)
  }

  if (args.listEnvs) {
    const envs = listEnvironments()
    console.log('Available test environments:')
    for (const env of envs) {
      console.log(`  - ${env}`)
    }
    process.exit(0)
  }

  console.log('='.repeat(60))
  console.log('Action-Builder Eval Runner')
  console.log('='.repeat(60))
  console.log(`Dataset: ${args.dataset}`)
  console.log(
    `Mode: ${args.online ? 'online (ActionBuilder)' : 'offline (fixtures)'}`
  )
  if (args.tags.length > 0) {
    console.log(`Tags filter: ${args.tags.join(', ')}`)
  }
  if (args.online) {
    console.log(`Max turns: ${args.maxTurns}`)
  }

  // Configure robustness (default: enabled)
  const envIds = args.envs.length > 0 ? args.envs : ['desktop_en']
  if (args.skipRobustness) {
    console.log(`Robustness: skipped`)
  } else {
    console.log(`Robustness: enabled (envs: ${envIds.join(', ')})`)
  }

  // Configure eval options based on mode
  if (args.online) {
    setOnlineEvalOptions({
      builderConfig: { maxTurns: args.maxTurns, headless: true },
      enableRobustness: !args.skipRobustness,
      robustnessEnvIds: envIds,
    })
  } else {
    setOfflineEvalOptions({
      enableRobustness: !args.skipRobustness,
      robustnessEnvIds: envIds,
    })
  }
  console.log('='.repeat(60))

  // Load testcases
  let testcases = loadTestcases(args.dataset)

  // Filter by tags if specified
  if (args.tags.length > 0) {
    testcases = filterByTags(testcases, args.tags)
    console.log(`[Runner] Filtered to ${testcases.length} testcases by tags`)
  }

  if (testcases.length === 0) {
    console.error('[Runner] No testcases to run')
    process.exit(1)
  }

  // Generate experiment name
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
  const experimentName = `ab-eval-${args.dataset}-${timestamp}`

  // Set experiment name for per-task logging
  setCurrentExperiment(experimentName)

  console.log(`[Runner] Experiment: ${experimentName}`)
  console.log(`[Runner] Logs directory: logs/${experimentName}/`)
  console.log(`[Runner] Running ${testcases.length} testcases...`)
  console.log('')

  // Select task based on mode
  const evalTask = args.online ? onlineEvalTask : offlineEvalTask
  const mode = args.online ? 'online' : 'offline'

  // Run evaluation with Braintrust
  // Note: maxConcurrency=1 ensures sequential execution for accurate per-task logs
  // The global fileLogger singleton causes log mixing in parallel execution
  const evalResult = await Eval('actionbook-action-builder', {
    experimentName,
    data: () => testcases,
    task: async (input: EvalInput): Promise<EvalOutput> => {
      return evalTask(input)
    },
    scores: [recallScorer, redundancyScorer, robustnessScorer],
    maxConcurrency: 1, // Run tasks sequentially to avoid log mixing
    metadata: {
      dataset: args.dataset,
      tags: args.tags,
      envs: args.envs,
      mode,
      maxTurns: args.online ? args.maxTurns : undefined,
    },
  })

  // Print summary
  console.log('')
  console.log('='.repeat(60))
  console.log('Evaluation Complete')
  console.log('='.repeat(60))
  console.log(`Experiment: ${experimentName}`)
  console.log(
    `Results: ${
      evalResult.summary?.experimentUrl || 'See Braintrust dashboard'
    }`
  )
}

// Run
main().catch((error) => {
  console.error('Fatal error:', error)
  process.exit(1)
})
