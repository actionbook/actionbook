/**
 * PageAnalyzer - Analyze a page for capabilities and features
 */

import type OpenAI from 'openai';
import { AIClient } from '../brain/index.js';
import { log } from '../utils/index.js';
import type { DiscoveredPage, AnalyzedPage } from '../types/index.js';

const PAGE_ANALYSIS_SYSTEM_PROMPT = `You are an expert web analyst. Your task is to analyze a webpage and identify its capabilities and features.

For the given page, identify:
1. capabilities: List of things a user can DO on this page (actions, not just information)
2. prerequisites: What conditions must be met to access this page (e.g., 'logged_in', 'has_items_in_cart')
3. urlPattern: A regex-like pattern that matches URLs for this page type
4. waitFor: A CSS selector that indicates the page is fully loaded

Focus on USER ACTIONS:
- Form submissions
- Button clicks
- Search functionality
- Filtering/sorting
- Navigation to other pages
- Data entry
- File uploads
- etc.

Be specific about what the user can accomplish on this page.`;

const analyzePageTool: OpenAI.Chat.Completions.ChatCompletionTool = {
  type: 'function',
  function: {
    name: 'register_page_analysis',
    description: 'Register the analysis results for a page',
    parameters: {
      type: 'object',
      properties: {
        capabilities: {
          type: 'array',
          items: { type: 'string' },
          description: 'List of actions/capabilities available on this page',
        },
        prerequisites: {
          type: 'array',
          items: { type: 'string' },
          description: 'Conditions required to access this page',
        },
        urlPattern: {
          type: 'string',
          description: 'URL pattern that matches this page type',
        },
        waitFor: {
          type: 'string',
          description: 'CSS selector to wait for page load',
        },
        updatedDescription: {
          type: 'string',
          description: 'Updated description based on analysis',
        },
      },
      required: ['capabilities'],
    },
  },
};

/**
 * PageAnalyzer - Analyzes pages for capabilities
 */
export class PageAnalyzer {
  private ai: AIClient;
  private customPrompt?: string;

  constructor(ai: AIClient, customPrompt?: string) {
    this.ai = ai;
    this.customPrompt = customPrompt;
  }

  /**
   * Analyze a page for capabilities
   */
  async analyze(screenshot: Buffer, htmlContent: string, page: DiscoveredPage): Promise<AnalyzedPage> {
    log('info', `[PageAnalyzer] Analyzing page: ${page.name}`);

    // Extract interactive elements from HTML
    const interactiveHtml = this.extractInteractiveHtml(htmlContent);

    // Build user prompt with optional custom instructions
    let userText = `Page: ${page.name}
URL: ${page.url}
Current description: ${page.description}

Interactive elements on this page:
${interactiveHtml}

Analyze this page and identify all capabilities and features.`;
    if (this.customPrompt) {
      userText += `\n\n## Site-specific Instructions\n${this.customPrompt}`;
    }

    const messages: OpenAI.Chat.Completions.ChatCompletionMessageParam[] = [
      { role: 'system', content: PAGE_ANALYSIS_SYSTEM_PROMPT },
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
      const response = await this.ai.chat(messages, [analyzePageTool]);

      const toolCalls = response.choices[0]?.message?.tool_calls;
      if (!toolCalls || toolCalls.length === 0) {
        log('warn', '[PageAnalyzer] No tool calls in response, using defaults');
        return {
          ...page,
          capabilities: ['View content'],
        };
      }

      const toolCall = toolCalls[0];
      const args = JSON.parse(toolCall.function.arguments);

      const analyzedPage: AnalyzedPage = {
        ...page,
        description: args.updatedDescription || page.description,
        capabilities: args.capabilities || ['View content'],
        prerequisites: args.prerequisites,
        urlPattern: args.urlPattern,
        waitFor: args.waitFor,
      };

      log('info', `[PageAnalyzer] Found ${analyzedPage.capabilities.length} capabilities`);
      return analyzedPage;

    } catch (error) {
      log('error', '[PageAnalyzer] Error analyzing page:', error);
      throw error;
    }
  }

  /**
   * Extract interactive elements from HTML
   */
  private extractInteractiveHtml(html: string): string {
    const patterns = [
      /<form[^>]*>[\s\S]*?<\/form>/gi,
      /<input[^>]*>/gi,
      /<button[^>]*>[\s\S]*?<\/button>/gi,
      /<select[^>]*>[\s\S]*?<\/select>/gi,
      /<textarea[^>]*>[\s\S]*?<\/textarea>/gi,
      /<a[^>]*href[^>]*>[\s\S]*?<\/a>/gi,
      /\[data-testid[^\]]*\][^>]*/gi,
    ];

    const matches: string[] = [];
    for (const pattern of patterns) {
      const found = html.match(pattern);
      if (found) {
        matches.push(...found.slice(0, 50)); // Limit per pattern
      }
    }

    // Limit total size
    return matches.join('\n').slice(0, 15000);
  }
}
