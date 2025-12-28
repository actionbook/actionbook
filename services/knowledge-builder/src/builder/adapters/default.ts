import type { CheerioAPI } from 'cheerio'
import type { BreadcrumbItem } from '@actionbookdev/db'
import type { SiteAdapter, ExtractedContent } from './types.js'

/**
 * Options for configuring DefaultAdapter
 */
export interface DefaultAdapterOptions {
  removeSelectors?: string[]
  waitForSelector?: string
  contentSelector?: string
  titleSelector?: string
  descriptionSelector?: string
  breadcrumbSelector?: string
}

/**
 * Default site adapter for generic websites
 */
export class DefaultAdapter implements SiteAdapter {
  name = 'default'
  type: 'code' | 'yaml' | 'smart' = 'code'

  removeSelectors = [
    'nav',
    'footer',
    'header',
    '.sidebar',
    '.toc',
    'script',
    'style',
  ]

  waitForSelector?: string

  // Configurable selectors
  protected contentSelector =
    'article, main, .content, section, [role="main"], body'
  protected titleSelector = 'h1'
  protected descriptionSelector = 'meta[name="description"]'
  protected breadcrumbSelector = 'nav[aria-label="breadcrumb"]'

  constructor(options: DefaultAdapterOptions = {}) {
    if (options.removeSelectors) this.removeSelectors = options.removeSelectors
    if (options.waitForSelector) this.waitForSelector = options.waitForSelector
    if (options.contentSelector) this.contentSelector = options.contentSelector
    if (options.titleSelector) this.titleSelector = options.titleSelector
    if (options.descriptionSelector)
      this.descriptionSelector = options.descriptionSelector
    if (options.breadcrumbSelector)
      this.breadcrumbSelector = options.breadcrumbSelector
  }

  extractContent($: CheerioAPI, url: string): ExtractedContent {
    // Remove unwanted elements
    for (const selector of this.removeSelectors) {
      $(selector).remove()
    }

    // Extract content
    const contentElements = $(this.contentSelector)
    let contentHtml = ''
    contentElements.each((_, el) => {
      contentHtml += $(el).html() || ''
    })
    const contentText = contentElements.text().trim()

    // Extract title
    const title =
      $(this.titleSelector).first().text().trim() ||
      $('title').text().trim() ||
      'Untitled'

    // Extract description
    const description = $(this.descriptionSelector).attr('content') || undefined

    // Extract links
    const links = this.extractLinks($, url)

    // Extract breadcrumb
    const breadcrumb = this.extractBreadcrumb($, url, title)

    return {
      title,
      description,
      contentHtml,
      contentText,
      links,
      breadcrumb,
    }
  }

  protected extractLinks($: CheerioAPI, currentUrl: string): string[] {
    const links: string[] = []
    const baseHost = new URL(currentUrl).host

    $('a[href]').each((_, el) => {
      const href = $(el).attr('href')
      if (!href) return

      try {
        const absoluteUrl = new URL(href, currentUrl).href
        const urlObj = new URL(absoluteUrl)

        // Only include same-host links
        if (urlObj.host !== baseHost) return

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

  protected extractBreadcrumb(
    $: CheerioAPI,
    url: string,
    title: string
  ): BreadcrumbItem[] {
    const breadcrumb: BreadcrumbItem[] = []

    const $nav = $(this.breadcrumbSelector)

    if ($nav.length) {
      $nav.find('a').each((_, el) => {
        const $el = $(el)
        breadcrumb.push({
          title: $el.text().trim(),
          url: new URL($el.attr('href') || '', url).href,
        })
      })
    }

    // Always add current page
    breadcrumb.push({ title, url })

    return breadcrumb
  }
}
