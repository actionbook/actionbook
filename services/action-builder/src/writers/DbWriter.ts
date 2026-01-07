import {
  type Database,
  sources,
  pages,
  elements,
  chunks,
  recordingTasks,
  recordingSteps,
  eq,
  and,
  sql,
  type RecordingConfig,
  type ElementType as DbElementType,
  type AllowMethod as DbAllowMethod,
  type ArgumentDef as DbArgumentDef,
  type SelectorItem as DbSelectorItem,
} from '@actionbookdev/db'
import type {
  SiteCapability,
  PageCapability,
  ElementCapability,
  AllowMethod,
  ArgumentDef,
  SelectorItem,
} from '../types/capability.js'

const GLOBAL_PAGE_TYPE = '__global__'

/**
 * Simplified element format for chunks.elements field
 */
interface ChunkElementEntry {
  css_selector?: string
  xpath_selector?: string
  description: string
  element_type: string
  allow_methods: string[]
  depends_on?: string
  visibility_condition?: string
  // Page module classification
  module?: string
  // Input-specific attributes
  input_type?: string
  input_name?: string
  input_value?: string
  // Link-specific attributes
  href?: string
}

/**
 * DbWriter - Database Storage Implementation
 * Save SiteCapability to PostgreSQL database
 */
export class DbWriter {
  constructor(private db: Database) {}

  private elementsColumnInfo:
    | {
        selector: 'missing' | 'nullable' | 'required'
        selectors: boolean
      }
    | undefined

  private async getElementsColumnInfo(): Promise<{
    selector: 'missing' | 'nullable' | 'required'
    selectors: boolean
  }> {
    if (this.elementsColumnInfo) return this.elementsColumnInfo

    const res = await this.db.execute<{
      column_name: string
      is_nullable: 'YES' | 'NO'
    }>(sql`
      select column_name, is_nullable
      from information_schema.columns
      where table_schema='public'
        and table_name='elements'
        and column_name in ('selector', 'selectors')
    `)

    const rowByName = new Map(
      res.rows.map((r) => [r.column_name, r.is_nullable])
    )
    const hasSelectors = rowByName.has('selectors')
    const selectorNullability = rowByName.get('selector')
    const selector =
      selectorNullability === undefined
        ? 'missing'
        : selectorNullability === 'NO'
        ? 'required'
        : 'nullable'

    this.elementsColumnInfo = { selector, selectors: hasSelectors }
    return this.elementsColumnInfo
  }

  /**
   * Save SiteCapability to database
   * @returns source.id
   */
  async save(capability: SiteCapability): Promise<number> {
    // Defensive check: ensure capability and pages exist
    if (!capability) {
      throw new Error('[DbWriter] Cannot save: capability is null or undefined')
    }
    if (!capability.pages) {
      throw new Error(
        '[DbWriter] Cannot save: capability.pages is null or undefined'
      )
    }

    // 1. Upsert source
    // Prefer baseUrl (unique in DB) to avoid NULL-domain sources causing duplicate inserts.
    const baseUrl = `https://${capability.domain}`
    const now = new Date()

    const insertedOrUpdated = await this.db
      .insert(sources)
      .values({
        name: capability.name,
        baseUrl,
        description: capability.description,
        domain: capability.domain,
        healthScore: capability.health_score,
        lastRecordedAt: now,
        updatedAt: now,
      })
      .onConflictDoUpdate({
        target: sources.baseUrl,
        set: {
          // Note: Do NOT update 'name' to avoid unique constraint violation
          // when another source already has the same name
          description: capability.description,
          // Fill domain when previously NULL; keep consistent with YAML output
          domain: capability.domain,
          healthScore: capability.health_score,
          lastRecordedAt: now,
          updatedAt: now,
        },
      })
      .returning({ id: sources.id })

    const sourceId = insertedOrUpdated[0].id

    // 2. Upsert pages and elements
    for (const page of Object.values(capability.pages)) {
      const pageId = await this.upsertPage(sourceId, page as PageCapability)

      // 3. Upsert elements for this page
      for (const element of Object.values((page as PageCapability).elements)) {
        await this.upsertElement(pageId, element)
      }
    }

    // 4. Persist global_elements under a synthetic page to keep DB schema simple.
    const globalElements = capability.global_elements || {}
    if (Object.keys(globalElements).length > 0) {
      const globalPageId = await this.upsertPage(sourceId, {
        page_type: GLOBAL_PAGE_TYPE,
        name: 'Global Elements',
        description: 'Synthetic page holding global elements',
        url_patterns: [],
        wait_for: undefined,
        elements: {},
      })

      for (const element of Object.values(globalElements)) {
        await this.upsertElement(globalPageId, element, { isGlobal: true })
      }
    }

    return sourceId
  }

  /**
   * Upsert a page record
   */
  private async upsertPage(
    sourceId: number,
    page: PageCapability
  ): Promise<number> {
    const existingPage = await this.db
      .select()
      .from(pages)
      .where(
        and(eq(pages.sourceId, sourceId), eq(pages.pageType, page.page_type))
      )
      .limit(1)

    if (existingPage.length > 0) {
      // Update
      await this.db
        .update(pages)
        .set({
          name: page.name,
          description: page.description,
          urlPatterns: page.url_patterns,
          waitFor: page.wait_for,
          updatedAt: new Date(),
        })
        .where(eq(pages.id, existingPage[0].id))
      return existingPage[0].id
    } else {
      // Insert
      const result = await this.db
        .insert(pages)
        .values({
          sourceId,
          pageType: page.page_type,
          name: page.name,
          description: page.description,
          urlPatterns: page.url_patterns,
          waitFor: page.wait_for,
        })
        .returning({ id: pages.id })
      return result[0].id
    }
  }

  /**
   * Upsert an element record
   */
  private async upsertElement(
    pageId: number,
    element: ElementCapability,
    options?: { isGlobal?: boolean }
  ): Promise<number> {
    // Defensive check: skip elements with missing required fields
    if (!element.id || !element.element_type) {
      console.warn(
        `[DbWriter] Skipping element with missing required fields: id=${element.id}, element_type=${element.element_type}`
      )
      return -1
    }

    // Skip elements with empty selectors (phantom elements)
    if (!element.selectors || element.selectors.length === 0) {
      console.warn(
        `[DbWriter] Skipping element with empty selectors: id=${element.id}`
      )
      return -1
    }

    const columnInfo = await this.getElementsColumnInfo()
    const existingElement = await this.db
      .select()
      .from(elements)
      .where(
        and(eq(elements.pageId, pageId), eq(elements.semanticId, element.id))
      )
      .limit(1)

    const isGlobal = options?.isGlobal ?? false
    const selectors: DbSelectorItem[] = element.selectors as DbSelectorItem[]
    const legacySelector: DbSelectorItem =
      selectors[0] ||
      ({ type: 'css', value: '', priority: 0, confidence: 0 } as DbSelectorItem)
    const allowMethods = (element.allow_methods || []) as DbAllowMethod[]
    const args = element.arguments as DbArgumentDef[] | undefined
    const discoveredAt = element.discovered_at
      ? new Date(element.discovered_at)
      : new Date()

    // Some DBs still have a legacy NOT NULL `selector` column (singular). Use a raw upsert to
    // populate both fields when required, otherwise Drizzle inserts will fail.
    if (columnInfo.selector === 'required') {
      const now = new Date()
      const legacySelectorJson = JSON.stringify(legacySelector)
      const selectorsJson = JSON.stringify(selectors)
      const allowMethodsJson = JSON.stringify(allowMethods)
      const argsJson = args ? JSON.stringify(args) : null

      const upsert = columnInfo.selectors
        ? sql`
            insert into "elements" (
              "page_id",
              "semantic_id",
              "element_type",
              "description",
              "selector",
              "selectors",
              "allow_methods",
              "arguments",
              "leads_to",
              "wait_after",
              "confidence",
              "is_global",
              "status",
              "discovered_at",
              "updated_at"
            ) values (
              ${pageId},
              ${element.id},
              ${element.element_type},
              ${element.description},
              ${legacySelectorJson}::jsonb,
              ${selectorsJson}::jsonb,
              ${allowMethodsJson}::jsonb,
              ${argsJson}::jsonb,
              ${element.leads_to ?? null},
              ${element.wait_after ?? null},
              ${element.confidence ?? null},
              ${isGlobal},
              ${'discovered'},
              ${discoveredAt},
              ${now}
            )
            on conflict ("page_id", "semantic_id") do update set
              "element_type" = excluded."element_type",
              "description" = excluded."description",
              "selector" = excluded."selector",
              "selectors" = excluded."selectors",
              "allow_methods" = excluded."allow_methods",
              "arguments" = excluded."arguments",
              "leads_to" = excluded."leads_to",
              "wait_after" = excluded."wait_after",
              "confidence" = excluded."confidence",
              "is_global" = excluded."is_global",
              "status" = excluded."status",
              "updated_at" = excluded."updated_at"
            returning "id"
          `
        : sql`
            insert into "elements" (
              "page_id",
              "semantic_id",
              "element_type",
              "description",
              "selector",
              "allow_methods",
              "arguments",
              "leads_to",
              "wait_after",
              "confidence",
              "is_global",
              "status",
              "discovered_at",
              "updated_at"
            ) values (
              ${pageId},
              ${element.id},
              ${element.element_type},
              ${element.description},
              ${legacySelectorJson}::jsonb,
              ${allowMethodsJson}::jsonb,
              ${argsJson}::jsonb,
              ${element.leads_to ?? null},
              ${element.wait_after ?? null},
              ${element.confidence ?? null},
              ${isGlobal},
              ${'discovered'},
              ${discoveredAt},
              ${now}
            )
            on conflict ("page_id", "semantic_id") do update set
              "element_type" = excluded."element_type",
              "description" = excluded."description",
              "selector" = excluded."selector",
              "allow_methods" = excluded."allow_methods",
              "arguments" = excluded."arguments",
              "leads_to" = excluded."leads_to",
              "wait_after" = excluded."wait_after",
              "confidence" = excluded."confidence",
              "is_global" = excluded."is_global",
              "status" = excluded."status",
              "updated_at" = excluded."updated_at"
            returning "id"
          `

      const result = await this.db.execute<{ id: number }>(upsert)
      return result.rows[0].id
    }

    if (existingElement.length > 0) {
      // Update
      await this.db
        .update(elements)
        .set({
          elementType: element.element_type as DbElementType,
          description: element.description,
          selectors,
          allowMethods,
          arguments: args,
          leadsTo: element.leads_to,
          waitAfter: element.wait_after,
          confidence: element.confidence,
          isGlobal,
          status: 'discovered',
          updatedAt: new Date(),
        })
        .where(eq(elements.id, existingElement[0].id))
      return existingElement[0].id
    } else {
      // Insert
      const result = await this.db
        .insert(elements)
        .values({
          pageId,
          semanticId: element.id,
          elementType: element.element_type as DbElementType,
          description: element.description,
          selectors,
          allowMethods,
          arguments: args,
          leadsTo: element.leads_to,
          waitAfter: element.wait_after,
          confidence: element.confidence,
          isGlobal,
          status: 'discovered',
          discoveredAt,
        })
        .returning({ id: elements.id })
      return result[0].id
    }
  }

  /**
   * Load SiteCapability from database
   */
  async load(domain: string): Promise<SiteCapability | null> {
    // 1. Find source by domain
    const sourceResult = await this.db
      .select()
      .from(sources)
      .where(eq(sources.domain, domain))
      .limit(1)

    if (sourceResult.length === 0) {
      return null
    }

    const source = sourceResult[0]

    // 2. Find all pages for this source
    const pagesResult = await this.db
      .select()
      .from(pages)
      .where(eq(pages.sourceId, source.id))

    // 3. Build pages with elements
    const pagesMap: Record<string, PageCapability> = {}
    const globalElements: Record<string, ElementCapability> = {}
    const columnInfo = await this.getElementsColumnInfo()

    for (const page of pagesResult) {
      // Find elements for this page
      const elementsResult = await this.db.execute<{
        semantic_id: string
        element_type: string
        description: string | null
        selectors: SelectorItem[] | null
        allow_methods: AllowMethod[] | null
        arguments: ArgumentDef[] | null
        leads_to: string | null
        wait_after: number | null
        confidence: number | null
        discovered_at: Date
      }>(
        columnInfo.selector !== 'missing'
          ? sql`
              select
                semantic_id,
                element_type,
                description,
                coalesce(selectors, selector) as selectors,
                allow_methods,
                arguments,
                leads_to,
                wait_after,
                confidence,
                discovered_at
              from elements
              where page_id = ${page.id}
            `
          : sql`
              select
                semantic_id,
                element_type,
                description,
                selectors,
                allow_methods,
                arguments,
                leads_to,
                wait_after,
                confidence,
                discovered_at
              from elements
              where page_id = ${page.id}
            `
      )

      // Build elements map
      const elementsMap: Record<string, ElementCapability> = {}
      for (const elem of elementsResult.rows) {
        const discoveredAt =
          elem.discovered_at instanceof Date
            ? elem.discovered_at
            : new Date(elem.discovered_at as unknown as string)
        const selectors = (elem.selectors || []) as SelectorItem[]
        elementsMap[elem.semantic_id] = {
          id: elem.semantic_id,
          selectors,
          description: elem.description || '',
          element_type: elem.element_type as ElementCapability['element_type'],
          allow_methods: (elem.allow_methods || []) as AllowMethod[],
          arguments: (elem.arguments || undefined) as ArgumentDef[] | undefined,
          leads_to: elem.leads_to || undefined,
          wait_after: elem.wait_after || undefined,
          confidence: elem.confidence || undefined,
          discovered_at: discoveredAt.toISOString(),
        }
      }

      if (page.pageType === GLOBAL_PAGE_TYPE) {
        Object.assign(globalElements, elementsMap)
        continue
      }

      pagesMap[page.pageType] = {
        page_type: page.pageType,
        name: page.name,
        description: page.description || '',
        url_patterns: (page.urlPatterns || []) as string[],
        wait_for: page.waitFor || undefined,
        elements: elementsMap,
      }
    }

    // 4. Build SiteCapability
    return {
      domain: source.domain || domain,
      name: source.name,
      description: source.description || '',
      version: '1.0.0', // Default version
      recorded_at:
        source.lastRecordedAt?.toISOString() || new Date().toISOString(),
      scenario: '', // Not stored in DB
      health_score: source.healthScore || undefined,
      global_elements: globalElements,
      pages: pagesMap,
    }
  }

  /**
   * List all site domains
   */
  async listSites(): Promise<string[]> {
    const result = await this.db
      .select({ domain: sources.domain })
      .from(sources)
      .where(eq(sources.domain, sources.domain)) // Just to filter non-null

    return result
      .filter((r) => r.domain !== null)
      .map((r) => r.domain as string)
  }

  /**
   * Check if site exists
   */
  async exists(domain: string): Promise<boolean> {
    const result = await this.db
      .select({ id: sources.id })
      .from(sources)
      .where(eq(sources.domain, domain))
      .limit(1)

    return result.length > 0
  }

  // =========================================================================
  // Recording Task Related Methods
  // =========================================================================

  /**
   * Create recording task
   */
  async createTask(
    sourceId: number,
    scenario: string,
    startUrl: string,
    config?: RecordingConfig
  ): Promise<number> {
    const result = await this.db
      .insert(recordingTasks)
      .values({
        sourceId,
        scenario,
        startUrl,
        status: 'pending',
        config,
        startedAt: new Date(),
      })
      .returning({ id: recordingTasks.id })

    return result[0].id
  }

  /**
   * Update task progress
   */
  async updateTaskProgress(
    taskId: number,
    progress: number,
    elementsDiscovered: number,
    pagesDiscovered: number
  ): Promise<void> {
    await this.db
      .update(recordingTasks)
      .set({
        status: 'running',
        progress,
        elementsDiscovered,
        pagesDiscovered,
      })
      .where(eq(recordingTasks.id, taskId))
  }

  /**
   * Complete task
   */
  async completeTask(
    taskId: number,
    status: 'completed' | 'failed',
    durationMs: number,
    tokensUsed: number,
    errorMessage?: string
  ): Promise<void> {
    await this.db
      .update(recordingTasks)
      .set({
        status,
        progress: status === 'completed' ? 100 : undefined,
        durationMs,
        tokensUsed,
        errorMessage,
        completedAt: new Date(),
      })
      .where(eq(recordingTasks.id, taskId))
  }

  /**
   * Add recording step
   */
  async addStep(
    taskId: number,
    step: {
      stepOrder: number
      toolName: string
      toolInput?: unknown
      toolOutput?: unknown
      pageType?: string
      elementId?: string
      durationMs?: number
      status: 'success' | 'failed'
      errorMessage?: string
      screenshotUrl?: string
    }
  ): Promise<number> {
    const result = await this.db
      .insert(recordingSteps)
      .values({
        taskId,
        stepOrder: step.stepOrder,
        toolName: step.toolName as any,
        toolInput: step.toolInput,
        toolOutput: step.toolOutput,
        pageType: step.pageType,
        elementId: step.elementId,
        durationMs: step.durationMs,
        status: step.status,
        errorMessage: step.errorMessage,
        screenshotUrl: step.screenshotUrl,
      })
      .returning({ id: recordingSteps.id })

    return result[0].id
  }

  // =========================================================================
  // Chunk Elements Update Methods
  // =========================================================================

  /**
   * Update chunk's elements field with extracted elements from SiteCapability
   *
   * Extracts all elements from SiteCapability (pages + global_elements),
   * formats them into a simplified JSON structure, and updates chunks.elements.
   *
   * @param chunkId - The chunk ID to update
   * @param capability - The SiteCapability containing discovered elements
   * @returns Number of elements written
   */
  async updateChunkElements(
    chunkId: number,
    capability: SiteCapability
  ): Promise<number> {
    // Extract all elements from pages and global_elements
    const elementsMap: Record<string, ChunkElementEntry> = {}

    // Process page elements
    for (const page of Object.values(capability.pages)) {
      for (const [elementId, element] of Object.entries(
        (page as PageCapability).elements
      )) {
        elementsMap[elementId] = this.formatElementForChunk(element)
      }
    }

    // Process global elements
    for (const [elementId, element] of Object.entries(
      capability.global_elements || {}
    )) {
      elementsMap[elementId] = this.formatElementForChunk(element)
    }

    const elementCount = Object.keys(elementsMap).length

    if (elementCount === 0) {
      return 0
    }

    // Update chunks.elements field
    const elementsJson = JSON.stringify(elementsMap)
    await this.db
      .update(chunks)
      .set({ elements: elementsJson })
      .where(eq(chunks.id, chunkId))

    return elementCount
  }

  /**
   * Format ElementCapability to simplified chunk element entry
   */
  private formatElementForChunk(element: ElementCapability): ChunkElementEntry {
    const entry: ChunkElementEntry = {
      description: element.description,
      element_type: element.element_type,
      allow_methods: element.allow_methods,
    }

    // Extract css and xpath selectors from selectors array
    for (const selector of element.selectors || []) {
      if (selector.type === 'css' && !entry.css_selector) {
        entry.css_selector = selector.value
      } else if (selector.type === 'xpath' && !entry.xpath_selector) {
        entry.xpath_selector = selector.value
      }
    }

    // Add optional fields if present
    if (element.depends_on) {
      entry.depends_on = element.depends_on
    }
    if (element.visibility_condition) {
      entry.visibility_condition = element.visibility_condition
    }
    // Page module classification
    if (element.module) {
      entry.module = element.module
    }
    // Input-specific attributes
    if (element.input_type) {
      entry.input_type = element.input_type
    }
    if (element.input_name) {
      entry.input_name = element.input_name
    }
    if (element.input_value && element.input_value.trim()) {
      entry.input_value = element.input_value
    }
    // Link-specific attributes
    if (element.href) {
      entry.href = element.href
    }

    return entry
  }
}
