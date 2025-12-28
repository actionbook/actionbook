import type { CheerioAPI } from 'cheerio'
import type { BreadcrumbItem } from '@actionbookdev/db'
import type { SiteAdapter, ExtractedContent, YamlSiteConfig } from './types.js'

/**
 * Default selectors used when not specified in YAML config
 */
const DEFAULT_SELECTORS = {
  content: 'article, main, .content, section, [role="main"], body',
  title: 'h1',
  description: 'meta[name="description"]',
  breadcrumb: 'nav[aria-label="breadcrumb"], .breadcrumb, .breadcrumbs',
}

/**
 * Default elements to remove
 */
const DEFAULT_REMOVE_SELECTORS = [
  'nav',
  'footer',
  'header',
  '.sidebar',
  '.toc',
  'script',
  'style',
  'noscript',
  'iframe',
  '[aria-hidden="true"]',
  '[hidden]',
]

/**
 * YAML-based site adapter
 * Allows configuration-driven content extraction without writing code
 */
export class YamlAdapter implements SiteAdapter {
  name: string
  type: 'code' | 'yaml' | 'smart' = 'yaml'
  removeSelectors: string[]
  waitForSelector?: string

  private config: YamlSiteConfig
  private contentSelector: string
  private titleSelector: string
  private descriptionSelector: string
  private breadcrumbSelector: string

  constructor(config: YamlSiteConfig) {
    this.config = config
    this.name = config.name

    // Set up selectors with defaults
    this.contentSelector =
      config.selectors?.content || DEFAULT_SELECTORS.content
    this.titleSelector = config.selectors?.title || DEFAULT_SELECTORS.title
    this.descriptionSelector =
      config.selectors?.description || DEFAULT_SELECTORS.description
    this.breadcrumbSelector =
      config.selectors?.breadcrumb || DEFAULT_SELECTORS.breadcrumb

    // Set up remove selectors
    this.removeSelectors = config.remove || DEFAULT_REMOVE_SELECTORS

    // Set up wait selector
    this.waitForSelector = config.wait?.selector
  }

  /**
   * Get crawl config from YAML
   */
  getCrawlConfig() {
    if (!this.config.crawl) return undefined

    return {
      maxDepth: this.config.crawl.maxDepth,
      includePatterns: this.config.crawl.include,
      excludePatterns: this.config.crawl.exclude,
      rateLimit: this.config.crawl.rateLimit,
    }
  }

  extractContent($: CheerioAPI, url: string): ExtractedContent {
    // Step 1: Remove unwanted elements
    for (const selector of this.removeSelectors) {
      $(selector).remove()
    }

    // Step 2: Apply cleaning rules - skip sections by heading
    if (this.config.cleaning?.skipSections) {
      this.removeSkipSections($, this.config.cleaning.skipSections)
    }

    // Step 3: Extract content
    const contentElements = $(this.contentSelector)
    let contentHtml = ''
    contentElements.each((_, el) => {
      contentHtml += $(el).html() || ''
    })
    let contentText = contentElements.text().trim()

    // Step 4: Apply text replacements
    if (this.config.cleaning?.replacements) {
      for (const { pattern, replacement } of this.config.cleaning
        .replacements) {
        const regex = new RegExp(pattern, 'g')
        contentText = contentText.replace(regex, replacement)
        contentHtml = contentHtml.replace(regex, replacement)
      }
    }

    // Step 5: Extract title
    let title = $(this.titleSelector).first().text().trim()
    if (!title) {
      title = $('title').text().trim() || 'Untitled'
    }

    // Step 6: Extract description
    const description = $(this.descriptionSelector).attr('content') || undefined

    // Step 7: Extract links
    const links = this.extractLinks($, url)

    // Step 8: Extract breadcrumb
    const breadcrumb = this.extractBreadcrumb($, url, title)

    // Calculate quality score
    const qualityScore = this.calculateQualityScore(
      contentText,
      title,
      links.length
    )

    return {
      title,
      description,
      contentHtml,
      contentText,
      links,
      breadcrumb,
      qualityScore,
    }
  }

  /**
   * Remove sections by heading text
   */
  private removeSkipSections($: CheerioAPI, skipSections: string[]): void {
    $('h1, h2, h3, h4, h5, h6').each((_, el) => {
      const headingText = $(el).text().trim()
      if (
        skipSections.some((skip) =>
          headingText.toLowerCase().includes(skip.toLowerCase())
        )
      ) {
        const $heading = $(el)
        const headingLevel = el.tagName.toLowerCase()

        // Remove this heading and following siblings until next same-or-higher level heading
        let $next = $heading.next()
        while ($next.length) {
          const nextTag = $next.prop('tagName')?.toLowerCase() || ''
          // Stop if we hit another heading of same or higher level
          if (nextTag.match(/^h[1-6]$/) && nextTag <= headingLevel) {
            break
          }
          const $toRemove = $next
          $next = $next.next()
          $toRemove.remove()
        }
        $heading.remove()
      }
    })
  }

  /**
   * Extract links from the page
   */
  private extractLinks($: CheerioAPI, currentUrl: string): string[] {
    const links: string[] = []
    const baseHost = new URL(currentUrl).host
    const includePatterns = this.config.links?.includePatterns || []
    const followExternal = this.config.links?.followExternal ?? false

    $('a[href]').each((_, el) => {
      const href = $(el).attr('href')
      if (!href) return

      try {
        const absoluteUrl = new URL(href, currentUrl).href
        const urlObj = new URL(absoluteUrl)

        // Check external links
        if (urlObj.host !== baseHost && !followExternal) return

        // Check include patterns if specified
        if (includePatterns.length > 0) {
          const path = urlObj.pathname
          const matches = includePatterns.some((pattern) => {
            const regex = new RegExp(pattern.replace(/\*/g, '.*'))
            return regex.test(path)
          })
          if (!matches) return
        }

        // Remove hash
        urlObj.hash = ''

        const normalizedUrl = urlObj.href
        if (!links.includes(normalizedUrl)) {
          links.push(normalizedUrl)
        }
      } catch {
        // Invalid URL, skip
      }
    })

    return links
  }

  /**
   * Extract breadcrumb navigation
   */
  private extractBreadcrumb(
    $: CheerioAPI,
    url: string,
    title: string
  ): BreadcrumbItem[] {
    const breadcrumb: BreadcrumbItem[] = []

    // Try multiple breadcrumb selectors
    const selectors = this.breadcrumbSelector.split(',').map((s) => s.trim())

    for (const selector of selectors) {
      const $nav = $(selector).first()
      if ($nav.length) {
        $nav.find('a').each((_, el) => {
          const $el = $(el)
          const text = $el.text().trim()
          if (text) {
            breadcrumb.push({
              title: text,
              url: new URL($el.attr('href') || '', url).href,
            })
          }
        })
        break // Use first matching selector
      }
    }

    // Always add current page
    breadcrumb.push({ title, url })

    return breadcrumb
  }

  /**
   * Calculate extraction quality score
   */
  private calculateQualityScore(
    contentText: string,
    title: string,
    linkCount: number
  ): number {
    let score = 1.0

    // Penalize if content is too short
    if (contentText.length < 100) {
      score -= 0.4
    } else if (contentText.length < 500) {
      score -= 0.2
    }

    // Penalize if no title
    if (!title || title === 'Untitled') {
      score -= 0.2
    }

    // Penalize if too few words (might be navigation-heavy)
    const wordCount = contentText.split(/\s+/).length
    if (wordCount < 50) {
      score -= 0.2
    }

    // Penalize if link density is too high (navigation page)
    const linkDensity = linkCount / Math.max(wordCount, 1)
    if (linkDensity > 0.5) {
      score -= 0.2
    }

    return Math.max(0, Math.min(1, score))
  }
}
