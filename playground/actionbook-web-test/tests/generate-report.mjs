#!/usr/bin/env node
/**
 * Generate an actionbook-web-test report JSON from YAML workflow + execution results.
 *
 * Usage:
 *   node generate-report.mjs <execution-result.json> [-o output.json]
 *
 * The execution result JSON references a YAML workflow file and contains
 * only runtime data (status, duration, errors, screenshots).
 * Test metadata (name, tags, steps) comes from the YAML.
 */
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import yaml from 'js-yaml'

// ── Helpers ──

function formatDuration(ms) {
  if (ms >= 60000) {
    const m = Math.floor(ms / 60000)
    const s = Math.round((ms % 60000) / 1000)
    return `${m}m ${s}s`
  }
  return `${(ms / 1000).toFixed(1)}s`
}

function loadScreenshot(filepath, baseDir) {
  const abs = resolve(baseDir, filepath)
  try {
    const buffer = readFileSync(abs)
    return `data:image/png;base64,${buffer.toString('base64')}`
  } catch {
    return null
  }
}

function statusBadge(status) {
  const map = { passed: 'PASS', failed: 'FAIL', skipped: 'SKIP', error: 'ERROR' }
  return map[status] ?? status.toUpperCase()
}

function isAssertionStep(step) {
  return step.assert != null
}

// ── Merge YAML + Execution Result ──

function mergeWorkflowAndResult(workflow, result) {
  const steps = workflow.steps.map((yamlStep, i) => {
    const execStep = result.steps?.[i] ?? {}
    return {
      name: yamlStep.name,
      status: execStep.status ?? 'skipped',
      duration: execStep.duration ?? 0,
      command: execStep.command ?? null,
      error: execStep.error ?? null,
      assertion: isAssertionStep(yamlStep),
    }
  })

  const totalDuration = steps.reduce((sum, s) => sum + s.duration, 0)
  const anyFailed = steps.some(s => s.status === 'failed')
  const anyPassed = steps.some(s => s.status === 'passed')

  return {
    name: workflow.name,
    status: anyFailed ? 'failed' : anyPassed ? 'passed' : 'skipped',
    duration: totalDuration,
    tags: workflow.tags ?? [],
    summary: workflow.description ?? '',
    url: workflow.url,
    steps,
    screenshot: result.screenshot ?? null,
  }
}

// ── Report Builder ──

function buildReport(tests, environment, baseDir) {
  const timestamp = environment.timestamp ?? new Date().toISOString()
  const summary = computeSummary(tests)

  return {
    type: 'Report',
    props: { title: 'Actionbook Test Report' },
    children: [
      buildBrandHeader(),
      buildSummarySection(summary),
      buildEnvironmentSection(environment, timestamp),
      buildResultsTable(tests, summary),
      ...buildTestDetailSections(tests, baseDir),
      buildFooter(timestamp),
    ],
  }
}

function computeSummary(tests) {
  const total = tests.length
  const passed = tests.filter(t => t.status === 'passed').length
  const failed = tests.filter(t => t.status === 'failed').length
  const skipped = tests.filter(t => t.status === 'skipped').length
  const totalDuration = tests.reduce((sum, t) => sum + (t.duration ?? 0), 0)
  return { total, passed, failed, skipped, totalDuration }
}

function buildBrandHeader() {
  return {
    type: 'BrandHeader',
    props: { badge: 'Actionbook Test', poweredBy: 'actionbook-web-test', showBadge: true },
  }
}

function buildSummarySection({ total, passed, failed, skipped, totalDuration }) {
  return {
    type: 'Section',
    props: { title: 'Summary', icon: 'chart' },
    children: [
      {
        type: 'MetricsGrid',
        props: {
          cols: 5,
          metrics: [
            { label: 'Total', value: String(total), icon: 'list' },
            { label: 'Passed', value: String(passed), ...(passed > 0 ? { trend: 'up' } : {}), icon: 'check' },
            { label: 'Failed', value: String(failed), ...(failed > 0 ? { trend: 'down' } : {}), icon: 'warning' },
            { label: 'Skipped', value: String(skipped), icon: 'skip' },
            { label: 'Duration', value: formatDuration(totalDuration), icon: 'clock' },
          ],
        },
      },
    ],
  }
}

function buildEnvironmentSection(env, timestamp) {
  return {
    type: 'Section',
    props: { title: 'Environment', icon: 'settings', collapsible: true },
    children: [
      {
        type: 'DefinitionList',
        props: {
          items: [
            { term: 'Timestamp', definition: timestamp },
            { term: 'Browser', definition: env.browser ?? 'Chromium (headless)' },
            { term: 'Viewport', definition: env.viewport ?? '1280x720' },
            { term: 'Profile', definition: env.profile ?? 'default' },
            { term: 'Target', definition: env.target },
          ],
        },
      },
    ],
  }
}

function buildResultsTable(tests, summary) {
  const rows = tests.map(t => {
    const totalSteps = t.steps.length
    const passedSteps = t.steps.filter(s => s.status === 'passed').length
    const assertionSteps = t.steps.filter(s => s.assertion)
    const passedAssertions = assertionSteps.filter(s => s.status === 'passed').length

    return {
      status: statusBadge(t.status),
      name: t.name,
      steps: `${passedSteps}/${totalSteps}`,
      assertions: assertionSteps.length > 0 ? `${passedAssertions}/${assertionSteps.length}` : '-',
      duration: formatDuration(t.duration),
      tags: t.tags.join(', '),
    }
  })

  return {
    type: 'Section',
    props: { title: 'Test Results', icon: 'code' },
    children: [
      {
        type: 'Table',
        props: {
          columns: [
            { key: 'status', label: 'Status' },
            { key: 'name', label: 'Test' },
            { key: 'steps', label: 'Steps' },
            { key: 'assertions', label: 'Assertions' },
            { key: 'duration', label: 'Duration' },
            { key: 'tags', label: 'Tags' },
          ],
          rows,
          striped: true,
          caption: `${summary.total} tests executed`,
        },
      },
    ],
  }
}

function buildTestDetailSections(tests, baseDir) {
  return tests.map(t => {
    const icon = t.status === 'passed' ? 'check' : t.status === 'failed' ? 'warning' : 'skip'
    const calloutType = t.status === 'passed' ? 'tip' : t.status === 'failed' ? 'important' : 'note'

    const totalSteps = t.steps.length
    const passedSteps = t.steps.filter(s => s.status === 'passed').length

    const children = [
      {
        type: 'Callout',
        props: {
          type: calloutType,
          title: `${statusBadge(t.status)} — ${passedSteps}/${totalSteps} steps, ${formatDuration(t.duration)}`,
          content: t.summary,
        },
      },
    ]

    if (t.steps.length > 0) {
      children.push({
        type: 'ContributionList',
        props: {
          numbered: true,
          items: t.steps.map(s => ({
            title: s.name,
            badge: statusBadge(s.status),
            description: buildStepDescription(s),
          })),
        },
      })
    }

    if (t.screenshot) {
      const src = loadScreenshot(t.screenshot, baseDir)
      if (src) {
        children.push({
          type: 'Image',
          props: { src, alt: `${t.name} — final screenshot`, caption: `Test completion screenshot — ${t.name}` },
        })
      }
    }

    return {
      type: 'Section',
      props: { title: t.name, icon, collapsible: true },
      children,
    }
  })
}

function buildStepDescription(step) {
  if (step.status === 'skipped') return 'Skipped'
  if (step.error) return `${step.command ? `\`${step.command}\` — ` : ''}${step.error}`
  if (step.command) return `\`${step.command}\` (${formatDuration(step.duration)})`
  return step.assertion ? `Assertion (${formatDuration(step.duration)})` : `(${formatDuration(step.duration)})`
}

function buildFooter(timestamp) {
  return {
    type: 'BrandFooter',
    props: { timestamp, attribution: 'Generated by actionbook-web-test' },
  }
}

// ── CLI ──

function writeExampleData(outputFile) {
  const example = {
    environment: {
      timestamp: new Date().toISOString(),
      browser: 'Chromium 125.0 (extension mode)',
      viewport: '1280x720',
      profile: 'default',
      target: 'https://example.com',
    },
    tests: [
      {
        name: 'example-smoke',
        status: 'passed',
        duration: 3200,
        tags: ['smoke', 'example'],
        summary: 'Verify example.com loads with expected content.',
        steps: [
          { name: 'Open page', status: 'passed', command: 'browser open https://example.com', duration: 800 },
          { name: 'Verify heading', status: 'passed', command: null, duration: 200, assertion: true },
          { name: 'Verify links', status: 'passed', command: null, duration: 150, assertion: true },
        ],
      },
    ],
  }
  const outPath = resolve(process.cwd(), outputFile)
  writeFileSync(outPath, JSON.stringify(example, null, 2))
  console.log(`Example data written to ${outPath}`)
}

function main() {
  const args = process.argv.slice(2)

  if (args.includes('--example')) {
    const outputIdx = args.indexOf('-o')
    const outputFile = outputIdx !== -1 ? args[outputIdx + 1] : 'example-result.json'
    writeExampleData(outputFile)
    return
  }

  const inputFile = args.find(a => !a.startsWith('-'))
  if (!inputFile) {
    console.error('Usage: node generate-report.mjs <execution-result.json> [-o output.json]')
    console.error('       node generate-report.mjs --example [-o output.json]')
    console.error('')
    console.error('Execution result JSON format:')
    console.error('  {')
    console.error('    "workflow": "reddit-ui-smoke.yaml",')
    console.error('    "environment": { "browser": "...", "target": "..." },')
    console.error('    "steps": [{ "status": "passed", "duration": 800, "command": "..." }],')
    console.error('    "screenshot": "screenshots/final.png"')
    console.error('  }')
    process.exit(1)
  }

  const outputIdx = args.indexOf('-o')
  const outputFile = outputIdx !== -1 ? args[outputIdx + 1] : 'report.json'

  // Resolve CLI paths against cwd, internal paths against input file's directory
  const inputPath = resolve(process.cwd(), inputFile)
  const inputDir = dirname(inputPath)
  const raw = JSON.parse(readFileSync(inputPath, 'utf-8'))
  const execResults = Array.isArray(raw) ? raw : [raw]

  // Collect environment from first result (shared across all tests)
  const environment = execResults[0].environment ?? {}

  // Merge each execution result with its YAML workflow
  const tests = execResults.map(result => {
    const workflowPath = resolve(inputDir, result.workflow)
    const workflow = yaml.load(readFileSync(workflowPath, 'utf-8'))
    // Use workflow url as target if environment doesn't specify
    if (!environment.target) {
      environment.target = workflow.url
    }
    return mergeWorkflowAndResult(workflow, result)
  })

  const report = buildReport(tests, environment, inputDir)

  const outPath = resolve(process.cwd(), outputFile)
  writeFileSync(outPath, JSON.stringify(report, null, 2))
  const size = (readFileSync(outPath).length / 1024).toFixed(0)
  console.log(`Report written to ${outPath} (${size} KB)`)
}

main()
