/**
 * PageDiscoverer - Discover pages on a website using LLM
 */

import type OpenAI from 'openai';
import { AIClient } from '../brain/index.js';
import { log } from '../utils/index.js';
import type { DiscoveredPage } from '../types/index.js';

const PAGE_DISCOVERY_SYSTEM_PROMPT = `You are an expert web analyst. Your task is to analyze a webpage and identify all the different page types/sections that can be navigated to from this page.

For each page you discover, provide:
1. semanticId: A short, snake_case identifier (e.g., 'home', 'search', 'listing_detail', 'user_profile')
2. name: Human-readable name
3. description: Brief description of the page's purpose
4. url: The URL or URL pattern to reach this page
5. navigation: How to navigate to this page from current page (optional)

Focus on:
- Main navigation links
- Sidebar/menu items
- Footer links to main pages
- Any buttons that lead to different page types

Ignore:
- External links (different domain)
- Social media links
- Legal pages (privacy policy, terms of service)
- Help/support pages unless they are main features`;

const discoverPagesTool: OpenAI.Chat.Completions.ChatCompletionTool = {
  type: 'function',
  function: {
    name: 'register_discovered_pages',
    description: 'Register the pages discovered on the website',
    parameters: {
      type: 'object',
      properties: {
        pages: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              semanticId: {
                type: 'string',
                description: 'Short snake_case identifier for the page type',
              },
              name: {
                type: 'string',
                description: 'Human-readable page name',
              },
              description: {
                type: 'string',
                description: 'Brief description of the page purpose',
              },
              url: {
                type: 'string',
                description: 'URL or URL pattern to reach this page',
              },
              navigation: {
                type: 'string',
                description: 'How to navigate to this page from current page',
              },
            },
            required: ['semanticId', 'name', 'description', 'url'],
          },
        },
      },
      required: ['pages'],
    },
  },
};

/**
 * PageDiscoverer - Discovers pages on a website
 */
export class PageDiscoverer {
  private ai: AIClient;
  private customPrompt?: string;

  constructor(ai: AIClient, customPrompt?: string) {
    this.ai = ai;
    this.customPrompt = customPrompt;
  }

  /**
   * Discover pages from the current page
   */
  async discover(screenshot: Buffer, htmlContent: string, currentUrl: string): Promise<DiscoveredPage[]> {
    log('info', '[PageDiscoverer] Starting page discovery');

    // Simplify HTML for LLM (extract navigation-related elements)
    const simplifiedHtml = this.extractNavigationHtml(htmlContent);

    // Build user prompt with optional custom instructions
    let userText = `Current URL: ${currentUrl}\n\nNavigable HTML elements:\n${simplifiedHtml}\n\nAnalyze this page and identify all navigable page types.`;
    if (this.customPrompt) {
      userText += `\n\n## Site-specific Instructions\n${this.customPrompt}`;
    }

    const messages: OpenAI.Chat.Completions.ChatCompletionMessageParam[] = [
      { role: 'system', content: PAGE_DISCOVERY_SYSTEM_PROMPT },
      {
        role: 'user',
        content: [
          {
            type: 'image_url',
            image_url: {
              url: `data:image/png;base64,${screenshot.toString('base64')}`,
            },
          },
          {
            type: 'text',
            text: userText,
          },
        ],
      },
    ];

    try {
      const response = await this.ai.chat(messages, [discoverPagesTool]);

      const toolCalls = response.choices[0]?.message?.tool_calls;
      if (!toolCalls || toolCalls.length === 0) {
        log('warn', '[PageDiscoverer] No tool calls in response');
        return [];
      }

      const toolCall = toolCalls[0];
      const args = JSON.parse(toolCall.function.arguments);
      const pages: DiscoveredPage[] = args.pages || [];

      // Normalize URLs
      const baseUrl = new URL(currentUrl);
      const normalizedPages = pages.map((page: DiscoveredPage) => ({
        ...page,
        url: this.normalizeUrl(page.url, baseUrl),
      }));

      log('info', `[PageDiscoverer] Discovered ${normalizedPages.length} pages`);
      return normalizedPages;

    } catch (error) {
      log('error', '[PageDiscoverer] Error discovering pages:', error);
      throw error;
    }
  }

  /**
   * Extract navigation-relevant HTML
   */
  private extractNavigationHtml(html: string): string {
    // Simple extraction - get nav, header, footer, and main menu areas
    const patterns = [
      /<nav[^>]*>[\s\S]*?<\/nav>/gi,
      /<header[^>]*>[\s\S]*?<\/header>/gi,
      /<footer[^>]*>[\s\S]*?<\/footer>/gi,
      /<a[^>]*href[^>]*>[\s\S]*?<\/a>/gi,
    ];

    const matches: string[] = [];
    for (const pattern of patterns) {
      const found = html.match(pattern);
      if (found) {
        matches.push(...found);
      }
    }

    // Limit size
    const result = matches.join('\n').slice(0, 10000);
    return result || html.slice(0, 10000);
  }

  /**
   * Normalize URL to absolute
   */
  private normalizeUrl(url: string, baseUrl: URL): string {
    try {
      // Handle relative URLs
      if (url.startsWith('/')) {
        return `${baseUrl.origin}${url}`;
      }
      if (!url.startsWith('http')) {
        return new URL(url, baseUrl.origin).href;
      }
      return url;
    } catch {
      return url;
    }
  }
}
