import fs from 'fs'
import path from 'path'
import YAML from 'yaml'
import { getDb, sources, pages, elements, eq, and } from '@actionbookdev/db'
import dotenv from 'dotenv'
import { fileURLToPath } from 'url'

// Load environment variables from project root
const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
dotenv.config({ path: path.resolve(__dirname, '../../../.env') })

// Also try loading from services/action-builder/.env if it exists
dotenv.config({ path: path.resolve(__dirname, '../.env') })

async function importYaml(siteDir: string) {
  const db = getDb()
  console.log(`Importing from ${siteDir}`)

  // 1. Read site.yaml
  const siteYamlPath = path.join(siteDir, 'site.yaml')
  if (!fs.existsSync(siteYamlPath)) {
    console.error(`Site config not found: ${siteYamlPath}`)
    return
  }
  const siteConfig = YAML.parse(fs.readFileSync(siteYamlPath, 'utf8'))
  console.log(`Site: ${siteConfig.name} (${siteConfig.domain})`)

  // 2. Update Sources Table
  let sourceId: number
  const existingSources = await db
    .select()
    .from(sources)
    .where(eq(sources.domain, siteConfig.domain))

  if (existingSources.length > 0) {
    sourceId = existingSources[0].id
    console.log(`Using existing source ID: ${sourceId}`)

    // Update source info
    await db
      .update(sources)
      .set({
        name: siteConfig.name,
        description: siteConfig.description,
        // Add other fields if necessary
        updatedAt: new Date(),
      })
      .where(eq(sources.id, sourceId))
  } else {
    const [newSource] = await db
      .insert(sources)
      .values({
        domain: siteConfig.domain,
        name: siteConfig.name,
        description: siteConfig.description,
        baseUrl: siteConfig.base_url || `https://${siteConfig.domain}`,
        // Add other required fields with defaults if necessary
      })
      .returning()
    sourceId = newSource.id
    console.log(`Created new source ID: ${sourceId}`)
  }

  // 3. Process Pages
  const pagesDir = path.join(siteDir, 'pages')
  if (fs.existsSync(pagesDir)) {
    const pageFiles = fs
      .readdirSync(pagesDir)
      .filter((f) => f.endsWith('.yaml') || f.endsWith('.yml'))

    for (const pageFile of pageFiles) {
      const pageYamlPath = path.join(pagesDir, pageFile)
      const pageConfig = YAML.parse(fs.readFileSync(pageYamlPath, 'utf8'))
      console.log(`Processing page: ${pageConfig.page_type}`)

      // 3.1 Upsert Page
      let pageId: number
      const existingPages = await db
        .select()
        .from(pages)
        .where(
          and(
            eq(pages.sourceId, sourceId),
            eq(pages.pageType, pageConfig.page_type)
          )
        )

      if (existingPages.length > 0) {
        pageId = existingPages[0].id
        // Update page info
        await db
          .update(pages)
          .set({
            name: pageConfig.name,
            description: pageConfig.description,
            urlPatterns: pageConfig.url_patterns,
            waitFor: pageConfig.wait_for,
            version: siteConfig.version, // Use site version or page version if available
            updatedAt: new Date(),
          })
          .where(eq(pages.id, pageId))
        // console.log(`  Updated page ID: ${pageId}`);
      } else {
        const [newPage] = await db
          .insert(pages)
          .values({
            sourceId,
            pageType: pageConfig.page_type,
            name: pageConfig.name,
            description: pageConfig.description,
            urlPatterns: pageConfig.url_patterns,
            waitFor: pageConfig.wait_for,
            version: siteConfig.version,
          })
          .returning()
        pageId = newPage.id
        console.log(`  Created page ID: ${pageId}`)
      }

      // 3.2 Process Elements
      if (pageConfig.elements) {
        let elementCount = 0
        for (const [semanticId, elData] of Object.entries(
          pageConfig.elements
        )) {
          const element = elData as any
          // Ensure selectors and allow_methods are arrays
          const selectors = Array.isArray(element.selectors)
            ? element.selectors
            : []
          const allowMethods = Array.isArray(element.allow_methods)
            ? element.allow_methods
            : []
          const args = Array.isArray(element.arguments) ? element.arguments : []

          // Upsert Element
          const existingElements = await db
            .select()
            .from(elements)
            .where(
              and(
                eq(elements.pageId, pageId),
                eq(elements.semanticId, semanticId)
              )
            )

          const elementValues = {
            elementType: element.element_type,
            description: element.description,
            selectors: selectors,
            allowMethods: allowMethods,
            arguments: args,
            confidence: element.confidence || 0,
            status: 'discovered' as const, // Or use existing status logic
            updatedAt: new Date(),
          }

          if (existingElements.length > 0) {
            await db
              .update(elements)
              .set(elementValues)
              .where(eq(elements.id, existingElements[0].id))
          } else {
            await db.insert(elements).values({
              pageId,
              semanticId: semanticId,
              ...elementValues,
              // discoveredAt will be set to defaultNow()
            })
          }
          elementCount++
        }
        console.log(`  Processed ${elementCount} elements`)
      }
    }
  }

  console.log('Import completed successfully!')
  process.exit(0)
}

// Get command line args
const sitePath = process.argv[2]
if (!sitePath) {
  console.error('Please provide site directory path')
  console.error('Usage: npx tsx scripts/import-yaml.ts <path-to-site-dir>')
  process.exit(1)
}

importYaml(path.resolve(sitePath)).catch((err) => {
  console.error('Import failed:', err)
  process.exit(1)
})
