/**
 * CapabilitiesDiscoverer - Discover what a page can do
 *
 * Focuses on:
 * - Page purpose and capabilities (WHAT can be done)
 * - User scenarios/workflows (HOW users accomplish goals)
 * - Functional areas (WHERE things happen)
 *
 * Does NOT focus on:
 * - Specific elements or selectors (action-builder's job)
 * - Technical implementation details
 */

import type OpenAI from 'openai';
import { AIClient } from '../brain/index.js';
import { log } from '../utils/index.js';
import type { PageCapabilities, UserScenario } from '../types/index.js';

const CAPABILITIES_SYSTEM_PROMPT = `You are an expert in analyzing web applications from a user's perspective.

Your task is to describe WHAT users can do on a webpage, not HOW (element details will be handled separately).

## Output Requirements

### 1. description
A 2-3 sentence overview explaining:
- The page's primary purpose
- Who would use this page and why
- Any important context (authenticated page, part of a flow, etc.)

### 2. capabilities
Action phrases describing what users can accomplish. Use "verb + object" format:
- Good: "Search for flights", "Filter by price range", "Add to wishlist", "Compare products"
- Bad: "Search button", "Filter dropdown", "Wishlist feature"

Be specific to this page's functionality. Include both primary and secondary capabilities.

### 3. functionalAreas
Name the key functional regions on the page:
- Examples: "Search form", "Results list", "Filters sidebar", "Navigation header", "User account menu"
- Focus on functional groupings, not layout

### 4. scenarios
Common user workflows on this page. Each scenario represents a complete user goal:
- name: Clear, descriptive name (e.g., "Book a flight", "Find cheapest option")
- goal: What the user wants to achieve
- steps: Natural language description of the flow (NOT element IDs)
- outcome: What happens when successful

Example scenario:
{
  "name": "Search for accommodation",
  "goal": "Find available places to stay in a specific location and date range",
  "steps": [
    "Enter destination city or region",
    "Select check-in and check-out dates",
    "Specify number of guests",
    "Submit search"
  ],
  "outcome": "Search results page shows available listings matching criteria"
}

### 5. prerequisites
Conditions needed to use this page:
- "User must be logged in"
- "Requires items in shopping cart"
- "Only accessible after completing previous step"

## Guidelines

1. Write from the USER's perspective, not the developer's
2. Focus on user GOALS and OUTCOMES, not UI mechanics
3. Be specific to THIS page - don't describe generic website features
4. Scenarios should represent realistic user journeys
5. Keep language natural and accessible`;

const discoverCapabilitiesTool: OpenAI.Chat.Completions.ChatCompletionTool = {
  type: 'function',
  function: {
    name: 'register_page_capabilities',
    description: 'Register the capabilities and scenarios discovered on the page',
    parameters: {
      type: 'object',
      properties: {
        description: {
          type: 'string',
          description: 'Page purpose and main functionality (2-3 sentences)',
        },
        capabilities: {
          type: 'array',
          items: { type: 'string' },
          description: 'Action phrases describing what users can do (verb + object format)',
        },
        functionalAreas: {
          type: 'array',
          items: { type: 'string' },
          description: 'Key functional regions on the page',
        },
        scenarios: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              name: {
                type: 'string',
                description: 'Scenario name',
              },
              goal: {
                type: 'string',
                description: 'What the user wants to achieve',
              },
              steps: {
                type: 'array',
                items: { type: 'string' },
                description: 'Natural language steps (not element IDs)',
              },
              outcome: {
                type: 'string',
                description: 'Expected result when successful',
              },
            },
            required: ['name', 'goal', 'steps', 'outcome'],
          },
          description: 'Common user workflows/scenarios',
        },
        prerequisites: {
          type: 'array',
          items: { type: 'string' },
          description: 'Conditions needed to use this page',
        },
      },
      required: ['description', 'capabilities'],
    },
  },
};

/**
 * CapabilitiesDiscoverer - Discovers page capabilities and scenarios
 */
export class CapabilitiesDiscoverer {
  private ai: AIClient;

  constructor(ai: AIClient) {
    this.ai = ai;
  }

  /**
   * Discover capabilities for a page
   */
  async discover(screenshot: Buffer, htmlContent: string, pageName: string): Promise<PageCapabilities> {
    log('info', `[CapabilitiesDiscoverer] Analyzing page: ${pageName}`);

    // Extract key text for context
    const pageContext = this.extractPageContext(htmlContent);

    const messages: OpenAI.Chat.Completions.ChatCompletionMessageParam[] = [
      { role: 'system', content: CAPABILITIES_SYSTEM_PROMPT },
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
            text: `Analyze this page "${pageName}" and describe its capabilities from a user's perspective.

Page context (key text extracted from HTML):
${pageContext}

Focus on:
1. What can users DO on this page? (capabilities)
2. What are the main functional areas?
3. What are common user scenarios/workflows?
4. Any prerequisites to use this page?

Describe capabilities and scenarios in natural language - element details will be handled separately.`,
          },
        ],
      },
    ];

    try {
      const response = await this.ai.chat(messages, [discoverCapabilitiesTool]);

      const toolCalls = response.choices[0]?.message?.tool_calls;
      if (!toolCalls || toolCalls.length === 0) {
        log('warn', '[CapabilitiesDiscoverer] No tool calls in response, using fallback');
        return this.createFallbackCapabilities(pageName);
      }

      const toolCall = toolCalls[0];
      const args = JSON.parse(toolCall.function.arguments);

      const capabilities: PageCapabilities = {
        description: args.description || `This is the ${pageName} page.`,
        capabilities: args.capabilities || [],
        functionalAreas: args.functionalAreas,
        scenarios: this.normalizeScenarios(args.scenarios),
        prerequisites: args.prerequisites,
      };

      log('info', `[CapabilitiesDiscoverer] Discovered ${capabilities.capabilities.length} capabilities, ${capabilities.scenarios?.length || 0} scenarios`);
      return capabilities;

    } catch (error) {
      log('error', '[CapabilitiesDiscoverer] Error discovering capabilities:', error);
      throw error;
    }
  }

  /**
   * Normalize scenarios from LLM response
   */
  private normalizeScenarios(scenarios: unknown[] | undefined): UserScenario[] | undefined {
    if (!scenarios || !Array.isArray(scenarios)) return undefined;

    return scenarios
      .filter((s): s is Record<string, unknown> => typeof s === 'object' && s !== null)
      .map((s) => ({
        name: String(s.name || 'Unnamed scenario'),
        goal: String(s.goal || ''),
        steps: Array.isArray(s.steps) ? s.steps.map(String) : [],
        outcome: String(s.outcome || ''),
      }));
  }

  /**
   * Create fallback capabilities when LLM fails
   */
  private createFallbackCapabilities(pageName: string): PageCapabilities {
    return {
      description: `This is the ${pageName} page. Capabilities could not be automatically detected.`,
      capabilities: [],
    };
  }

  /**
   * Extract page context for LLM reference
   */
  private extractPageContext(html: string): string {
    const patterns = [
      /<button[^>]*>([^<]*)<\/button>/gi,
      /<a[^>]*>([^<]{1,50})<\/a>/gi,
      /<label[^>]*>([^<]*)<\/label>/gi,
      /<h[1-6][^>]*>([^<]*)<\/h[1-6]>/gi,
      /placeholder="([^"]*)"/gi,
      /aria-label="([^"]*)"/gi,
      /title="([^"]*)"/gi,
    ];

    const matches: string[] = [];
    for (const pattern of patterns) {
      let match;
      while ((match = pattern.exec(html)) !== null) {
        const text = match[1]?.replace(/<[^>]*>/g, '').trim();
        if (text && text.length > 0 && text.length < 100) {
          matches.push(text);
        }
      }
    }

    const unique = [...new Set(matches)].slice(0, 60);
    return unique.join('\n');
  }
}
