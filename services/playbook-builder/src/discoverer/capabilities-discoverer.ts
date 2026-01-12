/**
 * CapabilitiesDiscoverer - Generate comprehensive Playbook for a page
 *
 * Generates a 7-section Playbook document to guide AI Agents:
 * - Section 0: Page URL (parameters, dynamic params)
 * - Section 1: Page Overview (core business objective)
 * - Section 2: Page Function Summary (function list)
 * - Section 3: Page Structure Summary (layout modules + CSS selectors)
 * - Section 4: DOM Structure Instance (pattern recognition, HTML snippets)
 * - Section 5: Parsing & Processing Summary (data retrieval scenarios)
 * - Section 6: Operation Summary (interactive operations)
 */

import type OpenAI from 'openai';
import { AIClient } from '../brain/index.js';
import { log } from '../utils/index.js';
import type { PageCapabilities } from '../types/index.js';

/**
 * Playbook generation system prompt (7-section format)
 * Adapted from crawl-playbook.ts
 */
const PLAYBOOK_SYSTEM_PROMPT = `You are a senior web automation and crawler architect. Your goal is to deeply analyze the target webpage and generate a standardized "Playbook" document to guide AI Agents or automation scripts in understanding, parsing, and interacting with the page.

**Core Task**:
Analyze the structure and logic of specific page types (e.g., detail pages, list pages, documentation pages).

---

## Output Document Structure (Strict Format)

Generate a Markdown document strictly following these 7 sections:

### 0. Page URL

\${url}
- Query parameters: one per line, brief description
    - \${name}: \${description}
- Params: if URL contains dynamic parameters
    - \${name}: \${description}

### 1. Page Overview
*   **Definition**: Clearly define the core business objective of this page in one sentence.
*   *Example*: "Retrieve detailed changelog, release date, and categorized updates for a specific software version."

### 2. Page Function Summary
*   **Format**: Function list. Each function on one line with name and brief description (1-2 sentences).
*   *Example*:
    *   **Version Switching**: Allows users to quickly jump to other historical versions via sidebar or dropdown menu.
    *   **Content Search**: Provides keyword search capability for current document or site-wide content.

### 3. Page Structure Summary
*   **Definition**: Macro-level breakdown of page layout modules (e.g., Header, Sidebar, Main Content).
*   **Requirement**: Provide **brief DOM description** for each module (key CSS selectors or semantic tags).
*   *Example*:
    *   **Sidebar (\`aside.nav\`)**: Contains the complete version history navigation list.
    *   **Main Content (\`main > article\`)**: Holds the core document content and changelog.

### 4. DOM Structure Instance
*   **Core Task (Pattern Recognition)**: If the page has different states or layout variants (e.g., with/without images, published/unpublished), list them **by pattern** here.
*   **Content**: Provide simplified HTML code snippets, preserving key data nodes and hierarchy.

### 5. Parsing & Processing Summary
*   **Data Retrieval Scenarios**: Define how data is presented and how to extract it.
    *   **Direct Retrieval**: Data is in initial HTML (provide CSS/XPath selectors).
    *   **Post-Interaction Retrieval**: Requires clicking to expand, switching tabs, or scroll loading.
    *   **Implicit Retrieval**: Data is in \`<script>\` tags, JSON attributes, or Shadow DOM.
*   **Logic Recommendations**: Provide compatible parsing logic for the patterns discovered in Section 4.

Note: Keep content summarized, no need for exhaustive details.

### 6. Operation Summary
*   **Definition**: Interactive operations available for Agent execution on the page.
*   **Format**:
    *   **Operation Type**: (input / click / hover)
    *   **Target Element**: (provide selector)
    *   **Expected Result**: What changes after the operation (URL change / partial DOM refresh / modal popup).

Note: Keep content summarized, no need for exhaustive details.`;

/**
 * CapabilitiesDiscoverer - Generates comprehensive Playbook for a page
 */
export class CapabilitiesDiscoverer {
  private ai: AIClient;
  private customPrompt?: string;

  constructor(ai: AIClient, customPrompt?: string) {
    this.ai = ai;
    this.customPrompt = customPrompt;
  }

  /**
   * Generate Playbook for a page
   * Returns PageCapabilities with playbook markdown
   */
  async discover(screenshot: Buffer, htmlContent: string, pageName: string, pageUrl?: string): Promise<PageCapabilities> {
    log('info', `[CapabilitiesDiscoverer] Generating Playbook for: ${pageName}`);

    // Get simplified HTML for context (limit size for LLM)
    const simplifiedHtml = this.getSimplifiedHTML(htmlContent);

    // Build user prompt
    let userText = `Please analyze the following webpage and generate a Playbook:

URL: ${pageUrl || 'N/A'}
Title: ${pageName}

Page HTML structure (simplified):
${simplifiedHtml}`;

    if (this.customPrompt) {
      userText += `\n\n## Site-specific Instructions\n${this.customPrompt}`;
    }

    const messages: OpenAI.Chat.Completions.ChatCompletionMessageParam[] = [
      { role: 'system', content: PLAYBOOK_SYSTEM_PROMPT },
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
      // No tool calling - get markdown directly from LLM
      const response = await this.ai.chat(messages, []);

      const playbook = response.choices[0]?.message?.content || '';
      const trimmedPlaybook = playbook.trim();

      if (!trimmedPlaybook) {
        log('warn', '[CapabilitiesDiscoverer] Empty playbook response, using fallback');
        return this.createFallbackCapabilities(pageName);
      }

      // Extract description from playbook (Section 1: Page Overview)
      const description = this.extractDescription(trimmedPlaybook, pageName);

      // Extract capabilities from playbook (Section 2: Page Function Summary)
      const capabilities = this.extractCapabilities(trimmedPlaybook);

      log('info', `[CapabilitiesDiscoverer] Generated Playbook (${trimmedPlaybook.length} chars), ${capabilities.length} capabilities extracted`);

      return {
        description,
        capabilities,
        playbook: trimmedPlaybook,
      };

    } catch (error) {
      log('error', '[CapabilitiesDiscoverer] Error generating Playbook:', error);
      throw error;
    }
  }

  /**
   * Get simplified HTML content (remove scripts, styles, etc.)
   */
  private getSimplifiedHTML(html: string): string {
    // Remove scripts, styles, SVG, images
    let simplified = html
      .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
      .replace(/<noscript[^>]*>[\s\S]*?<\/noscript>/gi, '')
      .replace(/<svg[^>]*>[\s\S]*?<\/svg>/gi, '')
      .replace(/<img[^>]*>/gi, '')
      .replace(/<iframe[^>]*>[\s\S]*?<\/iframe>/gi, '')
      .replace(/<video[^>]*>[\s\S]*?<\/video>/gi, '')
      .replace(/<audio[^>]*>[\s\S]*?<\/audio>/gi, '');

    // Remove inline styles
    simplified = simplified.replace(/\s+style="[^"]*"/gi, '');

    // Remove data-* attributes except data-testid
    simplified = simplified.replace(/\s+data-(?!testid)[a-z-]+="[^"]*"/gi, '');

    // Limit size to ~20KB
    return simplified.slice(0, 20000);
  }

  /**
   * Extract description from Playbook (Section 1: Page Overview)
   */
  private extractDescription(playbook: string, pageName: string): string {
    // Try to find Section 1 content
    const overviewMatch = playbook.match(/###\s*1\.\s*Page Overview[\s\S]*?(?=###\s*2\.|$)/i);
    if (overviewMatch) {
      // Extract the definition or first meaningful line
      const content = overviewMatch[0]
        .replace(/###\s*1\.\s*Page Overview/i, '')
        .replace(/\*\*Definition\*\*:?/gi, '')
        .trim();
      const firstLine = content.split('\n').find(line => line.trim().length > 10);
      if (firstLine) {
        return firstLine.replace(/^\*\s*/, '').trim();
      }
    }
    return `This is the ${pageName} page.`;
  }

  /**
   * Extract capabilities from Playbook (Section 2: Page Function Summary)
   */
  private extractCapabilities(playbook: string): string[] {
    const capabilities: string[] = [];

    // Try to find Section 2 content
    const functionMatch = playbook.match(/###\s*2\.\s*Page Function Summary[\s\S]*?(?=###\s*3\.|$)/i);
    if (functionMatch) {
      const content = functionMatch[0];
      // Extract function names (bold text before colon)
      const functionMatches = content.matchAll(/\*\*([^*]+)\*\*:?\s*([^\n]*)/g);
      for (const match of functionMatches) {
        const funcName = match[1].trim();
        if (funcName && funcName !== 'Format' && funcName !== 'Example') {
          capabilities.push(funcName);
        }
      }
    }

    return capabilities;
  }

  /**
   * Create fallback capabilities when LLM fails
   */
  private createFallbackCapabilities(pageName: string): PageCapabilities {
    return {
      description: `This is the ${pageName} page. Playbook could not be automatically generated.`,
      capabilities: [],
      playbook: `### 0. Page URL\n\nN/A\n\n### 1. Page Overview\n\nThis is the ${pageName} page.\n\n### 2-6. [Content not available]\n\nPlaybook generation failed.`,
    };
  }
}
