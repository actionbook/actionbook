#!/usr/bin/env npx tsx
/**
 * Sync local data to production using Blue/Green deployment
 *
 * Commands:
 *   sync     Upload data to production (creates building version)
 *   publish  Publish a building version to make it active
 *
 * Usage:
 *   npx tsx scripts/sync-to-production.ts sync <source-name> [options]
 *   npx tsx scripts/sync-to-production.ts publish <version-id> [options]
 *
 * Sync Options:
 *   --dry-run     Preview changes without executing
 *   --force       Skip confirmation prompts
 *   --api-url     API service URL (default: from env)
 *   --api-key     API key (default: from env)
 *
 * Examples:
 *   # Step 1: Upload data (creates building version)
 *   npx tsx scripts/sync-to-production.ts sync firstround.capital
 *
 *   # Step 2: Publish the version (after verification)
 *   npx tsx scripts/sync-to-production.ts publish 123
 *
 *   # Or do both in one command (legacy behavior)
 *   npx tsx scripts/sync-to-production.ts sync firstround.capital --publish
 *
 * Environment variables:
 *   API_SERVICE_URL - Production API URL
 *   API_KEY - API key for authentication
 *   DATABASE_URL - Local database connection string
 */

import * as dotenv from 'dotenv'
import { createDb, sources, documents, chunks, eq } from '@actionbookdev/db'
import * as readline from 'readline'

// Load environment variables
dotenv.config()

// Configuration
const API_SERVICE_URL = process.env.API_SERVICE_URL || 'http://localhost:3000'
const API_KEY = process.env.API_KEY || ''
const BATCH_SIZE_DOCS = 100
const BATCH_SIZE_CHUNKS = 50 // Chunks per request (with embeddings, keep small)

interface BaseOptions {
  apiUrl: string
  apiKey: string
}

interface SyncOptions extends BaseOptions {
  sourceName: string
  dryRun: boolean
  force: boolean
  autoPublish: boolean
}

interface PublishOptions extends BaseOptions {
  versionId: number
  force: boolean
}

interface ApiResponse<T> {
  success: boolean
  data?: T
  error?: {
    code: string
    message: string
    data?: Record<string, unknown>
  }
  message?: string
}

/**
 * Make API request to production
 */
async function apiRequest<T>(
  method: 'GET' | 'POST' | 'DELETE',
  path: string,
  body?: unknown,
  options?: BaseOptions
): Promise<ApiResponse<T>> {
  const url = `${options?.apiUrl || API_SERVICE_URL}/api/sync${path}`
  const apiKey = options?.apiKey || API_KEY

  const response = await fetch(url, {
    method,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    },
    body: body ? JSON.stringify(body) : undefined,
  })

  const data = await response.json()
  return data as ApiResponse<T>
}

/**
 * Prompt user for confirmation
 */
async function confirm(message: string): Promise<boolean> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  })

  return new Promise((resolve) => {
    rl.question(`${message} (y/N) `, (answer) => {
      rl.close()
      resolve(answer.toLowerCase() === 'y')
    })
  })
}

/**
 * Format number with commas
 */
function formatNumber(n: number): string {
  return n.toLocaleString()
}

// ============================================================================
// Sync Command
// ============================================================================

/**
 * Sync data to production (upload only, does not publish)
 */
async function syncCommand(options: SyncOptions): Promise<number | null> {
  const { sourceName, dryRun, force, autoPublish } = options

  console.log(`\n🔄 Sync to Production: ${sourceName}`)
  console.log('='.repeat(50))

  if (dryRun) {
    console.log('📋 DRY RUN MODE - No changes will be made\n')
  }

  // 1. Connect to local database and get source data
  console.log('\n📦 Phase 0: Loading local data...')
  const db = createDb()

  const localSource = await db
    .select()
    .from(sources)
    .where(eq(sources.name, sourceName))
    .limit(1)
    .then((rows) => rows[0])

  if (!localSource) {
    console.error(`❌ Source '${sourceName}' not found in local database`)
    process.exit(1)
  }

  const localDocs = await db
    .select()
    .from(documents)
    .where(eq(documents.sourceId, localSource.id))

  const docIds = localDocs.map((d) => d.id)
  let localChunks: (typeof chunks.$inferSelect)[] = []

  if (docIds.length > 0) {
    // Get chunks for all documents
    for (const docId of docIds) {
      const docChunks = await db
        .select()
        .from(chunks)
        .where(eq(chunks.documentId, docId))
      localChunks.push(...docChunks)
    }
  }

  console.log(`   ✅ Source: ${localSource.name}`)
  console.log(`   ✅ Documents: ${formatNumber(localDocs.length)}`)
  console.log(`   ✅ Chunks: ${formatNumber(localChunks.length)}`)

  // Confirm before proceeding
  if (!force && !dryRun) {
    const shouldContinue = await confirm('\n⚠️  Proceed with sync?')
    if (!shouldContinue) {
      console.log('❌ Sync cancelled')
      process.exit(0)
    }
  }

  if (dryRun) {
    console.log('\n📋 Dry run complete. No changes made.')
    return null
  }

  // 2. Initialize sync (create new version)
  console.log('\n📦 Phase 1: Initializing sync...')

  const initResult = await apiRequest<{
    sourceId: number
    versionId: number
    versionNumber: number
    status: string
  }>(
    'POST',
    `/sources/${sourceName}/versions`,
    {
      commitMessage: `Sync from local at ${new Date().toISOString()}`,
      createdBy: process.env.USER || 'cli',
    },
    options
  )

  if (!initResult.success) {
    if (initResult.error?.code === 'SYNC_IN_PROGRESS') {
      console.error(`❌ Another sync is already in progress`)
      console.error(
        `   Version ID: ${initResult.error.data?.existingVersionId}`
      )
      console.error(`   Created at: ${initResult.error.data?.createdAt}`)
      console.log('\n💡 To cancel the existing sync:')
      console.log(
        `   npx tsx scripts/sync-to-production.ts cancel ${initResult.error.data?.existingVersionId}`
      )
    } else {
      console.error(
        `❌ Failed to initialize sync: ${initResult.error?.message}`
      )
    }
    process.exit(1)
  }

  const { versionId, versionNumber } = initResult.data!
  console.log(`   ✅ Created version v${versionNumber} (id: ${versionId})`)

  // 3. Upload documents
  console.log('\n📦 Phase 2: Uploading documents...')

  const localIdToProdId = new Map<number, number>()
  let uploadedDocs = 0

  for (let i = 0; i < localDocs.length; i += BATCH_SIZE_DOCS) {
    const batch = localDocs.slice(i, i + BATCH_SIZE_DOCS)

    const docsPayload = batch.map((doc) => ({
      localId: doc.id,
      url: doc.url,
      urlHash: doc.urlHash,
      title: doc.title,
      description: doc.description,
      contentText: doc.contentText,
      contentHtml: doc.contentHtml,
      contentMd: doc.contentMd,
      elements: doc.elements,
      breadcrumb: doc.breadcrumb,
      wordCount: doc.wordCount,
      language: doc.language,
      contentHash: doc.contentHash,
      depth: doc.depth,
      parentId: doc.parentId,
    }))

    const uploadResult = await apiRequest<{
      mapping: Array<{ localId: number; prodId: number }>
    }>(
      'POST',
      `/versions/${versionId}/documents`,
      {
        documents: docsPayload,
      },
      options
    )

    if (!uploadResult.success) {
      console.error(
        `❌ Failed to upload documents: ${uploadResult.error?.message}`
      )
      console.log(`\n💡 To cancel this sync:`)
      console.log(
        `   npx tsx scripts/sync-to-production.ts cancel ${versionId}`
      )
      process.exit(1)
    }

    for (const m of uploadResult.data!.mapping) {
      localIdToProdId.set(m.localId, m.prodId)
    }

    uploadedDocs += batch.length
    process.stdout.write(
      `\r   📤 Uploaded: ${formatNumber(uploadedDocs)}/${formatNumber(
        localDocs.length
      )} documents`
    )
  }

  console.log(`\n   ✅ Documents uploaded: ${formatNumber(uploadedDocs)}`)

  // 4. Upload chunks (grouped by document)
  console.log('\n📦 Phase 3: Uploading chunks...')

  // Group chunks by document
  const chunksByDocId = new Map<number, typeof localChunks>()
  for (const chunk of localChunks) {
    const existing = chunksByDocId.get(chunk.documentId) || []
    existing.push(chunk)
    chunksByDocId.set(chunk.documentId, existing)
  }

  let uploadedChunks = 0
  let processedDocs = 0
  const totalDocs = chunksByDocId.size

  for (const [localDocId, docChunks] of chunksByDocId) {
    const prodDocId = localIdToProdId.get(localDocId)
    if (!prodDocId) {
      console.warn(
        `\n   ⚠️  Warning: No prod ID found for local doc ${localDocId}, skipping chunks`
      )
      continue
    }

    // Upload chunks for this document in batches
    for (let i = 0; i < docChunks.length; i += BATCH_SIZE_CHUNKS) {
      const batch = docChunks.slice(i, i + BATCH_SIZE_CHUNKS)

      const chunksPayload = batch.map((chunk) => ({
        documentId: prodDocId,
        content: chunk.content,
        contentHash: chunk.contentHash,
        embedding: chunk.embedding,
        chunkIndex: chunk.chunkIndex,
        startChar: chunk.startChar,
        endChar: chunk.endChar,
        heading: chunk.heading,
        headingHierarchy: chunk.headingHierarchy,
        tokenCount: chunk.tokenCount,
        embeddingModel: chunk.embeddingModel,
        elements: chunk.elements,
      }))

      const uploadResult = await apiRequest<{
        processedDocIds: number[]
        insertedCount: number
      }>(
        'POST',
        `/versions/${versionId}/chunks`,
        {
          chunks: chunksPayload,
        },
        options
      )

      if (!uploadResult.success) {
        console.error(
          `\n❌ Failed to upload chunks for doc ${localDocId}: ${uploadResult.error?.message}`
        )
        console.log(
          `\n💡 To retry sync: npx tsx scripts/sync-to-production.ts sync ${sourceName}`
        )
        console.log(
          `💡 To cancel: npx tsx scripts/sync-to-production.ts cancel ${versionId}`
        )
        process.exit(1)
      }

      uploadedChunks += uploadResult.data!.insertedCount
    }

    processedDocs++
    process.stdout.write(
      `\r   📤 Progress: ${formatNumber(processedDocs)}/${formatNumber(
        totalDocs
      )} documents, ${formatNumber(uploadedChunks)} chunks`
    )
  }

  console.log(`\n   ✅ Chunks uploaded: ${formatNumber(uploadedChunks)}`)

  // Summary
  console.log('\n' + '='.repeat(50))
  console.log('✅ Sync completed successfully!')
  console.log(`   Source: ${sourceName}`)
  console.log(`   Version: v${versionNumber} (id: ${versionId})`)
  console.log(`   Documents: ${formatNumber(uploadedDocs)}`)
  console.log(`   Chunks: ${formatNumber(uploadedChunks)}`)
  console.log(`   Status: building (not yet published)`)

  if (!autoPublish) {
    console.log('\n📋 Next steps:')
    console.log(`   1. Verify data in production (version ${versionId})`)
    console.log(
      `   2. Publish: npx tsx scripts/sync-to-production.ts publish ${versionId}`
    )
    console.log(
      `   3. Or cancel: npx tsx scripts/sync-to-production.ts cancel ${versionId}`
    )
  }

  return versionId
}

// ============================================================================
// Publish Command
// ============================================================================

/**
 * Publish a building version to make it active
 */
async function publishCommand(options: PublishOptions): Promise<void> {
  const { versionId, force } = options

  console.log(`\n🚀 Publishing version ${versionId}`)
  console.log('='.repeat(50))

  // Confirm before proceeding
  if (!force) {
    const shouldContinue = await confirm(
      '\n⚠️  Publish this version? This will make it active.'
    )
    if (!shouldContinue) {
      console.log('❌ Publish cancelled')
      process.exit(0)
    }
  }

  const publishResult = await apiRequest<{
    activeVersionId: number
    archivedVersionId: number | null
    publishedAt: string
  }>('POST', `/versions/${versionId}/publish`, undefined, options)

  if (!publishResult.success) {
    console.error(`❌ Failed to publish: ${publishResult.error?.message}`)
    if (publishResult.error?.code === 'VERSION_LOCKED') {
      console.log(
        '\n💡 This version is not in building state. Only building versions can be published.'
      )
    }
    process.exit(1)
  }

  console.log('\n' + '='.repeat(50))
  console.log('✅ Published successfully!')
  console.log(`   Active version: ${publishResult.data!.activeVersionId}`)
  if (publishResult.data!.archivedVersionId) {
    console.log(
      `   Archived previous version: ${publishResult.data!.archivedVersionId}`
    )
  }
  console.log(`   Published at: ${publishResult.data!.publishedAt}`)
}

// ============================================================================
// Cancel Command
// ============================================================================

/**
 * Cancel/delete a building version
 */
async function cancelCommand(
  versionId: number,
  options: BaseOptions,
  force: boolean
): Promise<void> {
  console.log(`\n🗑️  Cancelling version ${versionId}`)
  console.log('='.repeat(50))

  // Confirm before proceeding
  if (!force) {
    const shouldContinue = await confirm(
      '\n⚠️  Delete this version and all its data?'
    )
    if (!shouldContinue) {
      console.log('❌ Cancel aborted')
      process.exit(0)
    }
  }

  const deleteResult = await apiRequest<void>(
    'DELETE',
    `/versions/${versionId}`,
    undefined,
    options
  )

  if (!deleteResult.success) {
    console.error(`❌ Failed to delete version: ${deleteResult.error?.message}`)
    process.exit(1)
  }

  console.log('\n✅ Version deleted successfully!')
}

// ============================================================================
// CLI Argument Parsing
// ============================================================================

function showHelp(): void {
  console.log(`
Usage: npx tsx scripts/sync-to-production.ts <command> [options]

Commands:
  sync <source-name>    Upload local data to production (creates building version)
  publish <version-id>  Publish a building version to make it active
  cancel <version-id>   Cancel/delete a building version

Sync Options:
  --dry-run     Preview changes without executing
  --force       Skip confirmation prompts
  --publish     Auto-publish after sync (combines sync + publish)
  --api-url     API service URL (default: ${API_SERVICE_URL})
  --api-key     API key (default: from API_KEY env var)

Publish/Cancel Options:
  --force       Skip confirmation prompts
  --api-url     API service URL
  --api-key     API key

Examples:
  # Two-step workflow (recommended)
  npx tsx scripts/sync-to-production.ts sync firstround.capital
  npx tsx scripts/sync-to-production.ts publish 123

  # One-step workflow (auto-publish)
  npx tsx scripts/sync-to-production.ts sync firstround.capital --publish

  # Preview sync without changes
  npx tsx scripts/sync-to-production.ts sync airbnb.com --dry-run

  # Cancel a failed sync
  npx tsx scripts/sync-to-production.ts cancel 123
`)
}

async function main(): Promise<void> {
  const args = process.argv.slice(2)

  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    showHelp()
    process.exit(0)
  }

  const command = args[0]

  // Parse common options
  let apiUrl = API_SERVICE_URL
  let apiKey = API_KEY
  let force = false

  for (let i = 1; i < args.length; i++) {
    if (args[i] === '--api-url' && args[i + 1]) {
      apiUrl = args[++i]
    } else if (args[i] === '--api-key' && args[i + 1]) {
      apiKey = args[++i]
    } else if (args[i] === '--force') {
      force = true
    }
  }

  if (!apiKey) {
    console.error(
      '❌ API_KEY is required. Set it via --api-key or API_KEY env var'
    )
    process.exit(1)
  }

  const baseOptions: BaseOptions = { apiUrl, apiKey }

  switch (command) {
    case 'sync': {
      const sourceName = args[1]
      if (!sourceName || sourceName.startsWith('--')) {
        console.error('❌ Source name is required for sync command')
        console.log(
          'Usage: npx tsx scripts/sync-to-production.ts sync <source-name>'
        )
        process.exit(1)
      }

      let dryRun = false
      let autoPublish = false

      for (let i = 2; i < args.length; i++) {
        if (args[i] === '--dry-run') {
          dryRun = true
        } else if (args[i] === '--publish') {
          autoPublish = true
        }
      }

      const syncOptions: SyncOptions = {
        ...baseOptions,
        sourceName,
        dryRun,
        force,
        autoPublish,
      }

      const versionId = await syncCommand(syncOptions)

      // Auto-publish if requested
      if (autoPublish && versionId) {
        console.log('\n')
        await publishCommand({
          ...baseOptions,
          versionId,
          force: true, // Skip confirmation since user already confirmed sync
        })
      }
      break
    }

    case 'publish': {
      const versionIdStr = args[1]
      if (!versionIdStr || versionIdStr.startsWith('--')) {
        console.error('❌ Version ID is required for publish command')
        console.log(
          'Usage: npx tsx scripts/sync-to-production.ts publish <version-id>'
        )
        process.exit(1)
      }

      const versionId = parseInt(versionIdStr, 10)
      if (isNaN(versionId)) {
        console.error('❌ Version ID must be a number')
        process.exit(1)
      }

      await publishCommand({
        ...baseOptions,
        versionId,
        force,
      })
      break
    }

    case 'cancel': {
      const versionIdStr = args[1]
      if (!versionIdStr || versionIdStr.startsWith('--')) {
        console.error('❌ Version ID is required for cancel command')
        console.log(
          'Usage: npx tsx scripts/sync-to-production.ts cancel <version-id>'
        )
        process.exit(1)
      }

      const versionId = parseInt(versionIdStr, 10)
      if (isNaN(versionId)) {
        console.error('❌ Version ID must be a number')
        process.exit(1)
      }

      await cancelCommand(versionId, baseOptions, force)
      break
    }

    default:
      console.error(`❌ Unknown command: ${command}`)
      showHelp()
      process.exit(1)
  }
}

// Main
main().catch((error) => {
  console.error('❌ Command failed:', error)
  process.exit(1)
})
