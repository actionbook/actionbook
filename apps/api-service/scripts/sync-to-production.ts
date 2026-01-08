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
import {
  createDb,
  sources,
  sourceVersions,
  documents,
  chunks,
  eq,
  desc,
  inArray,
} from '@actionbookdev/db'
import * as readline from 'readline'
import { request as httpsRequest } from 'https'
import { request as httpRequest } from 'http'
import { HttpsProxyAgent } from 'https-proxy-agent'

// Load environment variables
dotenv.config()

// Configuration
// Note: api-service dev server default is http://localhost:3100 (see apps/api-service/README.md)
const API_SERVICE_URL = process.env.API_SERVICE_URL || 'http://localhost:3100'
// Support both env var names for convenience / backwards compatibility
const API_KEY = process.env.API_KEY || process.env.API_SERVICE_KEY || ''
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
  cancelExisting: boolean
}

interface PublishOptions extends BaseOptions {
  versionId: number
  force: boolean
}

interface SyncAllOptions extends BaseOptions {
  dryRun: boolean
  force: boolean
  autoPublish: boolean
  cancelExisting: boolean
  skipExistingRemote: boolean
  onlySources?: string[]
  excludeSources?: string[]
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

function getProxyUrl(): string | undefined {
  return (
    process.env.HTTPS_PROXY ||
    process.env.HTTP_PROXY ||
    process.env.ALL_PROXY ||
    undefined
  )
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function splitCsv(value: string): string[] {
  return value
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
}

function chunkArray<T>(arr: T[], size: number): T[][] {
  const out: T[][] = []
  for (let i = 0; i < arr.length; i += size) out.push(arr.slice(i, i + size))
  return out
}

function isRetryableNetworkError(err: unknown): boolean {
  if (!err || typeof err !== 'object') return false
  const code = (err as any).code as string | undefined
  // Common transient network errors with proxies / TLS handshakes
  return (
    code === 'ECONNRESET' ||
    code === 'ETIMEDOUT' ||
    code === 'EPIPE' ||
    code === 'ECONNREFUSED'
  )
}

async function requestJsonWithRetry<T>(
  url: string,
  init: {
    method: 'GET' | 'POST' | 'DELETE'
    headers: Record<string, string>
    body?: string
  },
  retries = 5
): Promise<T> {
  let attempt = 0
  // Exponential backoff with jitter
  while (true) {
    try {
      return await requestJson<T>(url, init)
    } catch (err) {
      attempt++
      if (attempt > retries || !isRetryableNetworkError(err)) {
        throw err
      }
      const backoff = Math.min(10_000, 500 * 2 ** (attempt - 1))
      const jitter = Math.floor(Math.random() * 250)
      console.warn(
        `   ⚠️  Network error (${(err as any)?.code || 'unknown'}), retrying in ${
          backoff + jitter
        }ms... (attempt ${attempt}/${retries})`
      )
      await sleep(backoff + jitter)
    }
  }
}

/**
 * Minimal JSON request helper that supports HTTP(S)_PROXY via https-proxy-agent.
 * Node's built-in fetch (undici) does not automatically honor proxy env vars.
 */
async function requestJson<T>(
  url: string,
  init: {
    method: 'GET' | 'POST' | 'DELETE'
    headers: Record<string, string>
    body?: string
  }
): Promise<T> {
  const u = new URL(url)
  const isHttps = u.protocol === 'https:'
  const proxyUrl = getProxyUrl()
  const agent = proxyUrl ? new HttpsProxyAgent(proxyUrl) : undefined

  const requestFn = isHttps ? httpsRequest : httpRequest

  return await new Promise<T>((resolve, reject) => {
    const req = requestFn(
      {
        protocol: u.protocol,
        hostname: u.hostname,
        port: u.port || (isHttps ? 443 : 80),
        path: `${u.pathname}${u.search}`,
        method: init.method,
        headers: init.headers,
        agent,
      },
      (res) => {
        const chunks: Buffer[] = []
        res.on('data', (d) => chunks.push(Buffer.isBuffer(d) ? d : Buffer.from(d)))
        res.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8')
          try {
            resolve(JSON.parse(text) as T)
          } catch (e) {
            reject(
              new Error(
                `Failed to parse JSON response (status ${res.statusCode}): ${text.slice(
                  0,
                  500
                )}`
              )
            )
          }
        })
      }
    )

    req.on('error', reject)
    if (init.body) req.write(init.body)
    req.end()
  })
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

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    // Some deployments use x-api-key (per apps/api-service/README.md),
    // some older clients used Authorization: Bearer. Send both.
    'x-api-key': apiKey,
    Authorization: `Bearer ${apiKey}`,
  }
  const bodyText = body ? JSON.stringify(body) : undefined
  if (bodyText) headers['Content-Length'] = String(Buffer.byteLength(bodyText))

  return await requestJsonWithRetry<ApiResponse<T>>(
    url,
    {
      method,
      headers,
      body: bodyText,
    },
    5
  )
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

/**
 * Write progress in a way that's visible in non-interactive logs.
 * - TTY: overwrite same line with \\r
 * - Non-TTY (CI / captured logs): print a normal line
 */
function writeProgress(message: string): void {
  if (process.stdout.isTTY) {
    process.stdout.write(`\r${message}`)
  } else {
    console.log(message)
  }
}

// ============================================================================
// Sync Command
// ============================================================================

/**
 * Sync data to production (upload only, does not publish)
 */
async function syncCommand(options: SyncOptions): Promise<number | null> {
  const { sourceName, dryRun, force, autoPublish, cancelExisting } = options

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
      const existingVersionId = Number(initResult.error.data?.existingVersionId)
      console.error(`❌ Another sync is already in progress`)
      console.error(`   Version ID: ${existingVersionId}`)
      console.error(`   Created at: ${initResult.error.data?.createdAt}`)

      if (cancelExisting && Number.isFinite(existingVersionId)) {
        console.log(
          `\n🧹 cancelExisting enabled → cancelling existing version ${existingVersionId} and retrying...`
        )
        await cancelCommand(existingVersionId, options, true)
        return await syncCommand({ ...options, cancelExisting: false })
      }

      console.log('\n💡 To cancel the existing sync:')
      console.log(
        `   npx tsx scripts/sync-to-production.ts cancel ${existingVersionId}`
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
    writeProgress(
      `   📤 Uploaded: ${formatNumber(uploadedDocs)}/${formatNumber(
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
    writeProgress(
      `   📤 Progress: ${formatNumber(processedDocs)}/${formatNumber(
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
// Sync-All Command
// ============================================================================

async function syncAllCommand(options: SyncAllOptions): Promise<void> {
  const {
    dryRun,
    force,
    autoPublish,
    cancelExisting,
    skipExistingRemote,
    onlySources,
    excludeSources,
  } = options

  console.log(`\n🔄 Sync ALL sources to Production`)
  console.log('='.repeat(50))
  console.log(`   Remote: ${options.apiUrl}`)
  if (dryRun) console.log('📋 DRY RUN MODE - No changes will be made\n')

  const db = createDb()

  let all = await db.select().from(sources).orderBy(desc(sources.createdAt))

  if (onlySources && onlySources.length > 0) {
    const onlySet = new Set(onlySources)
    all = all.filter((s) => onlySet.has(s.name))
  }

  if (excludeSources && excludeSources.length > 0) {
    const excludeSet = new Set(excludeSources)
    all = all.filter((s) => !excludeSet.has(s.name))
  }

  console.log(`   Sources selected: ${formatNumber(all.length)}`)

  if (!force && !dryRun) {
    const shouldContinue = await confirm('\n⚠️  Proceed with sync-all?')
    if (!shouldContinue) {
      console.log('❌ Sync-all cancelled')
      process.exit(0)
    }
  }

  const results: Array<{
    sourceName: string
    localSourceId: number
    localVersionId: number | null
    localVersionNumber: number | null
    documents: number
    chunks: number
    remoteVersionId: number | null
    remoteVersionNumber: number | null
    status: 'ok' | 'skipped' | 'error'
    error?: string
  }> = []

  for (const source of all) {
    console.log('\n' + '-'.repeat(50))
    console.log(`📌 Source: ${source.name} (local id: ${source.id})`)

    // Skip if remote already has this source (avoid overwriting / changing active)
    if (skipExistingRemote) {
      const remoteInfo = await apiRequest<{
        sourceId: number
        sourceName: string
        currentVersionId: number | null
        versions: Array<{
          id: number
          versionNumber: number
          status: string
        }>
      }>('GET', `/sources/${source.name}/versions`, undefined, options)

      if (remoteInfo.success) {
        const remoteVersions = remoteInfo.data?.versions ?? []
        if (remoteVersions.length > 0 || remoteInfo.data?.currentVersionId) {
          console.log(
            `   ⏭️  Remote already has versions (currentVersionId=${remoteInfo.data?.currentVersionId ?? 'null'}, versions=${remoteVersions.length}) → skipping`
          )
          results.push({
            sourceName: source.name,
            localSourceId: source.id,
            localVersionId: null,
            localVersionNumber: null,
            documents: 0,
            chunks: 0,
            remoteVersionId: remoteInfo.data?.currentVersionId ?? null,
            remoteVersionNumber: null,
            status: 'skipped',
          })
          continue
        }
      }
    }

    // Pick latest local source_version (by versionNumber)
    const latestLocalVersion = await db
      .select()
      .from(sourceVersions)
      .where(eq(sourceVersions.sourceId, source.id))
      .orderBy(desc(sourceVersions.versionNumber))
      .limit(1)
      .then((rows) => rows[0] || null)

    const localVersionId = latestLocalVersion?.id ?? null
    const localVersionNumber = latestLocalVersion?.versionNumber ?? null

    if (localVersionId) {
      console.log(
        `   Local latest version: v${localVersionNumber} (id: ${localVersionId}, status: ${latestLocalVersion?.status})`
      )
    } else {
      console.log(`   Local latest version: (none) → fallback to source_id only`)
    }

    // Load documents (prefer version-scoped docs if present)
    let localDocs: (typeof documents.$inferSelect)[] = []
    if (localVersionId) {
      const hasAnyVersionDocs = await db
        .select({ id: documents.id })
        .from(documents)
        .where(eq(documents.sourceVersionId, localVersionId))
        .limit(1)
        .then((rows) => rows.length > 0)

      if (hasAnyVersionDocs) {
        localDocs = await db
          .select()
          .from(documents)
          .where(eq(documents.sourceVersionId, localVersionId))
      } else {
        console.warn(
          `   ⚠️  No documents found for local source_version_id=${localVersionId}; falling back to source_id=${source.id}`
        )
        localDocs = await db
          .select()
          .from(documents)
          .where(eq(documents.sourceId, source.id))
      }
    } else {
      localDocs = await db
        .select()
        .from(documents)
        .where(eq(documents.sourceId, source.id))
    }

    const docIds = localDocs.map((d) => d.id)

    // Load chunks in batches to avoid huge IN() lists
    let localChunks: (typeof chunks.$inferSelect)[] = []
    for (const ids of chunkArray(docIds, 500)) {
      if (ids.length === 0) continue
      const rows = await db
        .select()
        .from(chunks)
        .where(inArray(chunks.documentId, ids))
      localChunks.push(...rows)
    }

    console.log(`   ✅ Documents: ${formatNumber(localDocs.length)}`)
    console.log(`   ✅ Chunks: ${formatNumber(localChunks.length)}`)

    if (dryRun) {
      results.push({
        sourceName: source.name,
        localSourceId: source.id,
        localVersionId,
        localVersionNumber,
        documents: localDocs.length,
        chunks: localChunks.length,
        remoteVersionId: null,
        remoteVersionNumber: null,
        status: 'skipped',
      })
      continue
    }

    // Reuse single-source sync logic by calling init + upload directly (same as syncCommand)
    try {
      // Init remote version
      const initResult = await apiRequest<{
        sourceId: number
        versionId: number
        versionNumber: number
        status: string
      }>(
        'POST',
        `/sources/${source.name}/versions`,
        {
          commitMessage: `Sync-all from local at ${new Date().toISOString()}${
            localVersionNumber ? ` (local v${localVersionNumber})` : ''
          }`,
          createdBy: process.env.USER || 'cli',
        },
        options
      )

      if (!initResult.success) {
        if (initResult.error?.code === 'SYNC_IN_PROGRESS') {
          const existingVersionId = Number(
            initResult.error.data?.existingVersionId
          )
          if (cancelExisting && Number.isFinite(existingVersionId)) {
            console.log(
              `   🧹 cancelExisting enabled → cancelling existing remote version ${existingVersionId} and retrying init...`
            )
            await cancelCommand(existingVersionId, options, true)
            // Retry once
            const retry = await apiRequest<{
              sourceId: number
              versionId: number
              versionNumber: number
              status: string
            }>(
              'POST',
              `/sources/${source.name}/versions`,
              {
                commitMessage: `Sync-all retry from local at ${new Date().toISOString()}${
                  localVersionNumber ? ` (local v${localVersionNumber})` : ''
                }`,
                createdBy: process.env.USER || 'cli',
              },
              options
            )
            if (!retry.success) throw new Error(retry.error?.message || 'init failed')
            initResult.success = true
            initResult.data = retry.data
          } else {
            throw new Error(
              `SYNC_IN_PROGRESS (existingVersionId=${existingVersionId})`
            )
          }
        } else {
          throw new Error(initResult.error?.message || 'init failed')
        }
      }

      const { versionId, versionNumber } = initResult.data!
      console.log(`   ✅ Remote version created: v${versionNumber} (id: ${versionId})`)

      // Upload documents (same payload mapping as syncCommand)
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
          { documents: docsPayload },
          options
        )

        if (!uploadResult.success) {
          throw new Error(uploadResult.error?.message || 'upload documents failed')
        }

        for (const m of uploadResult.data!.mapping) {
          localIdToProdId.set(m.localId, m.prodId)
        }

        uploadedDocs += batch.length
        writeProgress(
          `   📤 Uploaded docs: ${formatNumber(uploadedDocs)}/${formatNumber(
            localDocs.length
          )}`
        )
      }

      console.log(`\n   ✅ Documents uploaded: ${formatNumber(uploadedDocs)}`)

      // Upload chunks grouped by document (same as syncCommand)
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
        if (!prodDocId) continue

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
            { chunks: chunksPayload },
            options
          )

          if (!uploadResult.success) {
            throw new Error(uploadResult.error?.message || 'upload chunks failed')
          }

          uploadedChunks += uploadResult.data!.insertedCount
        }

        processedDocs++
        writeProgress(
          `   📤 Chunks progress: ${formatNumber(processedDocs)}/${formatNumber(
            totalDocs
          )} docs, ${formatNumber(uploadedChunks)} chunks`
        )
      }

      console.log(`\n   ✅ Chunks uploaded: ${formatNumber(uploadedChunks)}`)

      // Optionally publish
      if (autoPublish) {
        await publishCommand({
          ...options,
          versionId,
          force: true,
        })
      }

      results.push({
        sourceName: source.name,
        localSourceId: source.id,
        localVersionId,
        localVersionNumber,
        documents: uploadedDocs,
        chunks: uploadedChunks,
        remoteVersionId: versionId,
        remoteVersionNumber: versionNumber,
        status: 'ok',
      })
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.error(`   ❌ Failed: ${msg}`)
      results.push({
        sourceName: source.name,
        localSourceId: source.id,
        localVersionId,
        localVersionNumber,
        documents: localDocs.length,
        chunks: localChunks.length,
        remoteVersionId: null,
        remoteVersionNumber: null,
        status: 'error',
        error: msg,
      })
      // Continue with next source
      continue
    }
  }

  console.log('\n' + '='.repeat(50))
  const ok = results.filter((r) => r.status === 'ok')
  const skipped = results.filter((r) => r.status === 'skipped')
  const failed = results.filter((r) => r.status === 'error')

  console.log(`✅ Done.`)
  console.log(`   OK: ${formatNumber(ok.length)}`)
  console.log(`   Skipped (dry-run): ${formatNumber(skipped.length)}`)
  console.log(`   Failed: ${formatNumber(failed.length)}`)

  if (failed.length > 0) {
    console.log('\n❌ Failed sources:')
    for (const f of failed) {
      console.log(`   - ${f.sourceName}: ${f.error}`)
    }
    process.exit(1)
  }
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
  sync-all              Upload ALL local sources to production (creates building versions)
  publish <version-id>  Publish a building version to make it active
  cancel <version-id>   Cancel/delete a building version

Sync Options:
  --dry-run     Preview changes without executing
  --force       Skip confirmation prompts
  --publish     Auto-publish after sync (combines sync + publish)
  --cancel-existing  If a remote building version exists, cancel it and retry (dangerous)
  --api-url     API service URL (default: ${API_SERVICE_URL})
  --api-key     API key (default: from API_KEY env var)

Sync-All Options:
  --dry-run     Preview changes without executing
  --force       Skip confirmation prompts
  --publish     Auto-publish each source after sync
  --cancel-existing  If a remote building version exists, cancel it and retry (dangerous)
  --sync-existing  Sync even if remote already has versions (default: skip existing remotes)
  --only        Comma-separated source names to sync (e.g. --only arxiv,airbnb)
  --exclude     Comma-separated source names to skip
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

  # Sync all sources (upload only)
  npx tsx scripts/sync-to-production.ts sync-all --force

  # Sync only a few sources
  npx tsx scripts/sync-to-production.ts sync-all --only arxiv,airbnb --force --publish

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
  let cancelExisting = false

  for (let i = 1; i < args.length; i++) {
    if (args[i] === '--api-url' && args[i + 1]) {
      apiUrl = args[++i]
    } else if (args[i] === '--api-key' && args[i + 1]) {
      apiKey = args[++i]
    } else if (args[i] === '--force') {
      force = true
    } else if (args[i] === '--cancel-existing') {
      cancelExisting = true
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
        cancelExisting,
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

    case 'sync-all': {
      let dryRun = false
      let autoPublish = false
      let onlySources: string[] | undefined
      let excludeSources: string[] | undefined
      let skipExistingRemote = true

      for (let i = 1; i < args.length; i++) {
        if (args[i] === '--dry-run') {
          dryRun = true
        } else if (args[i] === '--publish') {
          autoPublish = true
        } else if (args[i] === '--sync-existing') {
          skipExistingRemote = false
        } else if (args[i] === '--only' && args[i + 1]) {
          onlySources = splitCsv(args[++i])
        } else if (args[i] === '--exclude' && args[i + 1]) {
          excludeSources = splitCsv(args[++i])
        }
      }

      const syncAllOptions: SyncAllOptions = {
        ...baseOptions,
        dryRun,
        force,
        autoPublish,
        cancelExisting,
        skipExistingRemote,
        onlySources,
        excludeSources,
      }

      await syncAllCommand(syncAllOptions)
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
