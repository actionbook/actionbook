import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest'
import { DbWriter } from './DbWriter'
import {
  createDb,
  sources,
  pages,
  elements,
  eq,
  type Database,
} from '@actionbookdev/db'
import type { SiteCapability } from '../types/capability'

/**
 * DbWriter Tests
 * MVP critical path tests: save / load / upsert
 */
describe('DbWriter', () => {
  let db: Database
  let dbWriter: DbWriter
  const testDomain = 'test.example.com'

  // Test SiteCapability
  const mockSiteCapability: SiteCapability = {
    domain: testDomain,
    name: 'Test Site',
    description: 'A test site for unit testing',
    version: '1.0.0',
    recorded_at: new Date().toISOString(),
    scenario: 'test-scenario',
    health_score: 90,
    global_elements: {}, // Skip for MVP phase
    pages: {
      home: {
        page_type: 'home',
        name: 'Home Page',
        description: 'The home page',
        url_patterns: [
          'https://test.example.com/',
          'https://test.example.com/home',
        ],
        elements: {
          search_input: {
            id: 'search_input',
            selectors: [
              {
                type: 'data-testid',
                value: '[data-testid="search-input"]',
                priority: 1,
                confidence: 0.95,
              },
              {
                type: 'xpath',
                value: '//input[@data-testid="search-input"]',
                priority: 2,
                confidence: 0.6,
              },
            ],
            description: 'Search input field',
            element_type: 'input',
            allow_methods: ['click', 'type', 'clear'],
            confidence: 0.95,
            discovered_at: new Date().toISOString(),
          },
          submit_button: {
            id: 'submit_button',
            selectors: [
              {
                type: 'css',
                value: 'button[type="submit"]',
                priority: 1,
                confidence: 0.9,
              },
            ],
            description: 'Submit button',
            element_type: 'button',
            allow_methods: ['click'],
            confidence: 0.9,
            discovered_at: new Date().toISOString(),
          },
        },
      },
      search_results: {
        page_type: 'search_results',
        name: 'Search Results',
        description: 'Search results page',
        url_patterns: ['https://test.example.com/search*'],
        elements: {
          result_item: {
            id: 'result_item',
            selectors: [
              {
                type: 'css',
                value: '.result-item',
                priority: 1,
                confidence: 0.75,
              },
            ],
            description: 'Search result item',
            element_type: 'link',
            allow_methods: ['click'],
            discovered_at: new Date().toISOString(),
          },
        },
      },
    },
  }

  beforeAll(async () => {
    // Use database connection from environment variables
    db = createDb()
    dbWriter = new DbWriter(db)
  })

  afterAll(async () => {
    // Clean up test data
    await cleanupTestData()
  })

  beforeEach(async () => {
    // Clean up before each test
    await cleanupTestData()
  })

  async function cleanupTestData() {
    // Find and delete test source
    const testSources = await db
      .select()
      .from(sources)
      .where(eq(sources.domain, testDomain))

    for (const source of testSources) {
      // Delete elements (via pages cascade)
      // Delete pages
      await db.delete(pages).where(eq(pages.sourceId, source.id))
      // Delete source
      await db.delete(sources).where(eq(sources.id, source.id))
    }
  }

  /**
   * Test 1: save() can correctly write to database
   */
  it('should save SiteCapability to database', async () => {
    // Act
    const sourceId = await dbWriter.save(mockSiteCapability)

    // Assert - verify source record
    expect(sourceId).toBeGreaterThan(0)

    const savedSource = await db
      .select()
      .from(sources)
      .where(eq(sources.id, sourceId))
    expect(savedSource).toHaveLength(1)
    expect(savedSource[0].domain).toBe(testDomain)
    expect(savedSource[0].name).toBe('Test Site')
    expect(savedSource[0].healthScore).toBe(90)

    // Assert - verify pages records
    const savedPages = await db
      .select()
      .from(pages)
      .where(eq(pages.sourceId, sourceId))
    expect(savedPages).toHaveLength(2)

    const homePage = savedPages.find((p) => p.pageType === 'home')
    expect(homePage).toBeDefined()
    expect(homePage?.name).toBe('Home Page')

    // Assert - verify elements records
    if (homePage) {
      const savedElements = await db
        .select()
        .from(elements)
        .where(eq(elements.pageId, homePage.id))
      expect(savedElements).toHaveLength(2)

      const searchInput = savedElements.find(
        (e) => e.semanticId === 'search_input'
      )
      expect(searchInput).toBeDefined()
      expect(searchInput?.elementType).toBe('input')
      expect(searchInput?.confidence).toBeCloseTo(0.95)
    }
  })

  it('should persist global_elements under a synthetic page and load them back', async () => {
    const capabilityWithGlobal: SiteCapability = {
      ...mockSiteCapability,
      pages: {},
      global_elements: {
        global_nav: {
          id: 'global_nav',
          selectors: [
            { type: 'css', value: 'nav', priority: 1, confidence: 0.7 },
          ],
          description: 'Global navigation',
          element_type: 'other',
          allow_methods: ['click'],
          discovered_at: new Date().toISOString(),
        },
      },
    }

    const sourceId = await dbWriter.save(capabilityWithGlobal)

    const savedPages = await db
      .select()
      .from(pages)
      .where(eq(pages.sourceId, sourceId))
    const globalPage = savedPages.find((p) => p.pageType === '__global__')
    expect(globalPage).toBeDefined()

    const savedGlobalElements = await db
      .select()
      .from(elements)
      .where(eq(elements.pageId, globalPage!.id))
    expect(savedGlobalElements).toHaveLength(1)
    expect(savedGlobalElements[0].semanticId).toBe('global_nav')
    expect(savedGlobalElements[0].isGlobal).toBe(true)

    const loaded = await dbWriter.load(testDomain)
    expect(loaded?.global_elements.global_nav).toBeDefined()
    expect(Object.keys(loaded?.pages || {})).toHaveLength(0)
  })

  it('should upsert existing source by baseUrl (domain may be null)', async () => {
    // Pre-create a source with the same baseUrl but NULL domain to mimic firstround DB state.
    const existing = await db
      .insert(sources)
      .values({
        name: `preexisting-${Date.now()}`,
        baseUrl: `https://${testDomain}`,
        description: 'preexisting',
        domain: null,
        crawlConfig: {},
      })
      .returning({ id: sources.id })

    const existingId = existing[0].id

    const savedId = await dbWriter.save(mockSiteCapability)
    expect(savedId).toBe(existingId)

    const sourceRow = await db
      .select()
      .from(sources)
      .where(eq(sources.id, existingId))
      .limit(1)
    expect(sourceRow[0].domain).toBe(testDomain)

    const savedPages = await db
      .select()
      .from(pages)
      .where(eq(pages.sourceId, existingId))
    expect(savedPages.length).toBeGreaterThan(0)

    const savedElements = await db
      .select()
      .from(elements)
      .innerJoin(pages, eq(elements.pageId, pages.id))
      .where(eq(pages.sourceId, existingId))
    expect(savedElements.length).toBeGreaterThan(0)
  })

  /**
   * Test 2: load() can correctly read and assemble SiteCapability
   */
  it('should load SiteCapability from database', async () => {
    // Arrange - save data first
    await dbWriter.save(mockSiteCapability)

    // Act
    const loaded = await dbWriter.load(testDomain)

    // Assert
    expect(loaded).not.toBeNull()
    expect(loaded?.domain).toBe(testDomain)
    expect(loaded?.name).toBe('Test Site')
    expect(loaded?.health_score).toBe(90)

    // Verify pages
    expect(Object.keys(loaded?.pages || {})).toHaveLength(2)
    expect(loaded?.pages.home).toBeDefined()
    expect(loaded?.pages.home.name).toBe('Home Page')

    // Verify elements
    expect(Object.keys(loaded?.pages.home.elements || {})).toHaveLength(2)
    expect(loaded?.pages.home.elements.search_input).toBeDefined()
    expect(loaded?.pages.home.elements.search_input.element_type).toBe('input')
  })

  /**
   * Test 3: save() supports upsert (update existing records)
   */
  it('should upsert existing records', async () => {
    // Arrange - first save
    const firstSourceId = await dbWriter.save(mockSiteCapability)

    // Modify data
    const updatedCapability: SiteCapability = {
      ...mockSiteCapability,
      name: 'Updated Test Site',
      health_score: 85,
      pages: {
        ...mockSiteCapability.pages,
        home: {
          ...mockSiteCapability.pages.home,
          elements: {
            ...mockSiteCapability.pages.home.elements,
            search_input: {
              ...mockSiteCapability.pages.home.elements.search_input,
              description: 'Updated search input',
              confidence: 0.99,
            },
          },
        },
      },
    }

    // Act - second save (should update instead of insert new record)
    const secondSourceId = await dbWriter.save(updatedCapability)

    // Assert - sourceId should be the same
    expect(secondSourceId).toBe(firstSourceId)

    // Verify only one source record
    const allSources = await db
      .select()
      .from(sources)
      .where(eq(sources.domain, testDomain))
    expect(allSources).toHaveLength(1)
    // Note: name is intentionally NOT updated during upsert to avoid unique constraint violations
    expect(allSources[0].name).toBe('Test Site')
    expect(allSources[0].healthScore).toBe(85)

    // Verify element was updated
    const loaded = await dbWriter.load(testDomain)
    expect(loaded?.pages.home.elements.search_input.description).toBe(
      'Updated search input'
    )
    expect(loaded?.pages.home.elements.search_input.confidence).toBeCloseTo(
      0.99
    )
  })

  /**
   * Test 4: load() returns null for non-existent domain
   */
  it('should return null for non-existent domain', async () => {
    const result = await dbWriter.load('non-existent.com')
    expect(result).toBeNull()
  })

  /**
   * Test 5: listSites() returns all site domains
   */
  it('should list all site domains', async () => {
    // Arrange
    await dbWriter.save(mockSiteCapability)

    // Act
    const sites = await dbWriter.listSites()

    // Assert
    expect(sites).toContain(testDomain)
  })

  /**
   * Test 6: exists() checks if site exists
   */
  it('should check if site exists', async () => {
    // Arrange
    await dbWriter.save(mockSiteCapability)

    // Act & Assert
    expect(await dbWriter.exists(testDomain)).toBe(true)
    expect(await dbWriter.exists('non-existent.com')).toBe(false)
  })
})
