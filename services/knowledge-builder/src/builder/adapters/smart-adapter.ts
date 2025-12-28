import type { CheerioAPI } from 'cheerio'
import { Readability } from '@mozilla/readability'
import { parseHTML } from 'linkedom'
import type { BreadcrumbItem } from '@actionbookdev/db'
import type { SiteAdapter, ExtractedContent, StructuredData } from './types.js'

/**
 * Elements to remove before Readability processing
 */
const NOISE_SELECTORS = [
  // Roles
  '[role="navigation"]',
  '[role="banner"]',
  '[role="contentinfo"]',
  '[role="complementary"]',

  // Attributes
  '[aria-hidden="true"]',
  '[hidden]',

  // Common class patterns (not removed by Readability)
  '.sidebar',
  '.widget',
  '.comment',
  '.comments',
  '.share',
  '.social',
  '.related',
  '.related-posts',
  '.recommend',
  '.recommended',
  '.popular',
  '.trending',
  '.advertisement',
  '.ads',
  '.ad',
  '.banner',
  '.promo',
  '.newsletter',
  '.subscribe',
  '.author-bio',
  '.pagination',
]

/**
 * Breadcrumb selectors
 */
const BREADCRUMB_SELECTORS = [
  'nav[aria-label*="breadcrumb" i]',
  '[aria-label*="breadcrumb" i]',
  '.breadcrumb',
  '.breadcrumbs',
  '[itemtype*="BreadcrumbList"]',
]

/**
 * SmartAdapter - Zero-configuration content extraction using Mozilla Readability
 *
 * Strategy:
 * 1. Extract structured data (JSON-LD, OpenGraph) - most reliable metadata
 * 2. Use Mozilla Readability for main content extraction
 * 3. Fallback to heuristics if Readability fails
 */
export class SmartAdapter implements SiteAdapter {
  name = 'smart'
  type: 'code' | 'yaml' | 'smart' = 'smart'
  removeSelectors = NOISE_SELECTORS
  waitForSelector?: string

  extractContent($: CheerioAPI, url: string): ExtractedContent {
    // Get raw HTML for Readability
    const rawHtml = $.html()

    // Step 1: Extract structured data BEFORE any modification
    const structuredData = this.extractStructuredData($)

    // Step 2: Extract breadcrumb BEFORE Readability modifies DOM
    const tempTitle =
      structuredData.title || $('h1').first().text().trim() || 'Untitled'
    const breadcrumb = this.extractBreadcrumb($, url, tempTitle)

    // Step 3: Extract links BEFORE Readability
    const links = this.extractLinks($, url)

    // Step 4: Use Readability for main content
    const readabilityResult = this.extractWithReadability(rawHtml, url)

    let contentHtml: string
    let contentText: string
    let confidence: number
    let title: string

    if (readabilityResult) {
      // Readability succeeded
      contentHtml = readabilityResult.content
      contentText = readabilityResult.textContent
      title = readabilityResult.title || structuredData.title || tempTitle
      confidence = 0.9
    } else {
      // Fallback to simple extraction
      const fallback = this.fallbackExtraction($)
      contentHtml = fallback.contentHtml
      contentText = fallback.contentText
      title = structuredData.title || tempTitle
      confidence = 0.5
    }

    // Step 5: Extract description
    const description = structuredData.description || this.extractDescription($)

    // Step 6: Calculate quality score
    const qualityScore = this.calculateQualityScore(
      contentText,
      title,
      confidence
    )

    // Update breadcrumb with final title
    if (breadcrumb.length > 0) {
      breadcrumb[breadcrumb.length - 1].title = title
    }

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
   * Extract content using Mozilla Readability
   */
  private extractWithReadability(
    html: string,
    _url: string
  ): { title: string; content: string; textContent: string } | null {
    try {
      // Parse HTML with linkedom (lightweight DOM implementation)
      const { document } = parseHTML(html)

      // Set document URL for relative link resolution
      // Note: linkedom doesn't support setting baseURI directly
      // Readability will handle relative URLs based on the document

      // Create Readability instance
      const reader = new Readability(document, {
        // Keep classes for potential styling
        keepClasses: false,
        // Debug mode off
        debug: false,
        // Character threshold for content
        charThreshold: 100,
      })

      // Parse the article
      const article = reader.parse()

      if (!article || !article.content || article.textContent.length < 100) {
        return null
      }

      return {
        title: article.title || '',
        content: article.content,
        textContent: article.textContent,
      }
    } catch (error) {
      console.warn('[SmartAdapter] Readability failed:', error)
      return null
    }
  }

  /**
   * Fallback extraction when Readability fails
   */
  private fallbackExtraction($: CheerioAPI): {
    contentHtml: string
    contentText: string
  } {
    // Remove noise elements
    for (const selector of this.removeSelectors) {
      $(selector).remove()
    }

    // Also remove standard noise tags
    $('nav, footer, header, aside, script, style, noscript, iframe').remove()

    // Try semantic containers first
    const contentSelectors = [
      'article',
      '[role="main"]',
      'main',
      '.post-content',
      '.article-content',
      '.content',
      '#content',
    ]

    for (const selector of contentSelectors) {
      const $el = $(selector).first()
      if ($el.length) {
        const text = $el.text().trim()
        if (text.length > 200) {
          return {
            contentHtml: $el.html() || '',
            contentText: text,
          }
        }
      }
    }

    // Fallback to body
    const bodyHtml = $('body').html() || ''
    const bodyText = $('body').text().trim()

    return { contentHtml: bodyHtml, contentText: bodyText }
  }

  /**
   * Extract structured data from page (JSON-LD, OpenGraph, Schema.org)
   */
  private extractStructuredData($: CheerioAPI): StructuredData {
    const data: StructuredData = {}

    // Try JSON-LD first (most reliable)
    $('script[type="application/ld+json"]').each((_, el) => {
      try {
        const jsonText = $(el).html()
        if (!jsonText) return

        const json = JSON.parse(jsonText)
        const items = Array.isArray(json) ? json : [json]

        for (const item of items) {
          // Handle @graph structure
          const entities = item['@graph'] || [item]

          for (const entity of entities) {
            if (
              entity['@type'] === 'Article' ||
              entity['@type'] === 'WebPage' ||
              entity['@type'] === 'BlogPosting' ||
              entity['@type'] === 'NewsArticle' ||
              entity['@type'] === 'TechArticle' ||
              entity['@type'] === 'HowTo'
            ) {
              data.title = data.title || entity.headline || entity.name
              data.description = data.description || entity.description
              data.author =
                data.author ||
                (typeof entity.author === 'string'
                  ? entity.author
                  : entity.author?.name)
              data.publishDate = data.publishDate || entity.datePublished
              data.modifiedDate = data.modifiedDate || entity.dateModified
              data.type = data.type || entity['@type']
            }
          }
        }
      } catch {
        // Invalid JSON, skip
      }
    })

    // OpenGraph fallback
    if (!data.title) {
      data.title = $('meta[property="og:title"]').attr('content')
    }
    if (!data.description) {
      data.description = $('meta[property="og:description"]').attr('content')
    }
    if (!data.type) {
      data.type = $('meta[property="og:type"]').attr('content')
    }
    if (!data.image) {
      data.image = $('meta[property="og:image"]').attr('content')
    }
    if (!data.url) {
      data.url = $('meta[property="og:url"]').attr('content')
    }

    // Twitter Cards fallback
    if (!data.title) {
      data.title = $('meta[name="twitter:title"]').attr('content')
    }
    if (!data.description) {
      data.description = $('meta[name="twitter:description"]').attr('content')
    }

    return data
  }

  /**
   * Extract description with fallbacks
   */
  private extractDescription($: CheerioAPI): string | undefined {
    return (
      $('meta[name="description"]').attr('content') ||
      $('meta[property="og:description"]').attr('content') ||
      undefined
    )
  }

  /**
   * Extract links from page
   */
  private extractLinks($: CheerioAPI, currentUrl: string): string[] {
    const links: string[] = []
    const baseHost = new URL(currentUrl).host

    $('a[href]').each((_, el) => {
      const href = $(el).attr('href')
      if (!href) return

      try {
        const absoluteUrl = new URL(href, currentUrl).href
        const urlObj = new URL(absoluteUrl)

        // Only same-host links
        if (urlObj.host !== baseHost) return

        // Skip common non-content paths
        const path = urlObj.pathname.toLowerCase()
        if (
          path.includes('/login') ||
          path.includes('/signup') ||
          path.includes('/register') ||
          path.includes('/auth') ||
          path.includes('/cart') ||
          path.includes('/checkout')
        ) {
          return
        }

        // Skip resource files
        if (path.match(/\.(pdf|jpg|jpeg|png|gif|svg|css|js|zip|tar|gz)$/i)) {
          return
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

    for (const selector of BREADCRUMB_SELECTORS) {
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

        if (breadcrumb.length > 0) break
      }
    }

    // Always add current page
    breadcrumb.push({ title, url })

    return breadcrumb
  }

  /**
   * Calculate overall quality score
   */
  private calculateQualityScore(
    contentText: string,
    title: string,
    confidence: number
  ): number {
    let score = confidence

    // Penalize short content
    if (contentText.length < 100) {
      score -= 0.3
    } else if (contentText.length < 500) {
      score -= 0.1
    }

    // Penalize missing title
    if (!title || title === 'Untitled') {
      score -= 0.2
    }

    // Boost for longer content
    if (contentText.length > 2000) {
      score += 0.1
    }

    return Math.max(0, Math.min(1, score))
  }
}
