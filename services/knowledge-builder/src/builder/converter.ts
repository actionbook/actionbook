/**
 * Converter - HTML to Markdown conversion
 *
 * Responsibilities:
 * - Convert HTML content to clean Markdown
 * - Apply custom conversion rules for code blocks, media, etc.
 * - Normalize output Markdown format
 *
 * Does NOT handle:
 * - Page fetching (handled by PageLoader)
 * - Content extraction (handled by Adapters)
 * - Chunking (handled by Processor)
 */

import TurndownService from 'turndown';

/**
 * Converter configuration options
 */
export interface ConverterConfig {
  /** Heading style: 'atx' (#) or 'setext' (underline) */
  headingStyle?: 'atx' | 'setext';
  /** Code block style: 'fenced' (```) or 'indented' */
  codeBlockStyle?: 'fenced' | 'indented';
  /** Bullet list marker: '-', '+', or '*' */
  bulletListMarker?: '-' | '+' | '*';
  /** Emphasis delimiter: '*' or '_' */
  emDelimiter?: '*' | '_';
  /** Whether to remove images */
  removeImages?: boolean;
  /** Whether to remove media (video, audio, iframe) */
  removeMedia?: boolean;
}

const DEFAULT_CONFIG: Required<ConverterConfig> = {
  headingStyle: 'atx',
  codeBlockStyle: 'fenced',
  bulletListMarker: '-',
  emDelimiter: '*',
  removeImages: true,
  removeMedia: true,
};

/**
 * Converter - converts HTML to Markdown
 */
export class Converter {
  private turndown: TurndownService;
  private config: Required<ConverterConfig>;

  constructor(config: ConverterConfig = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.turndown = this.createTurndownService();
  }

  /**
   * Convert HTML content to Markdown
   *
   * @param html - HTML content to convert
   * @returns Normalized Markdown string
   */
  convert(html: string): string {
    const markdown = this.turndown.turndown(html);
    return this.normalize(markdown);
  }

  /**
   * Normalize Markdown content
   * - Collapse multiple blank lines
   * - Remove trailing whitespace
   * - Trim start/end
   */
  private normalize(content: string): string {
    return content
      .replace(/\n{3,}/g, '\n\n') // Collapse multiple blank lines
      .replace(/[ \t]+$/gm, '') // Remove trailing whitespace
      .trim();
  }

  /**
   * Create configured TurndownService instance
   */
  private createTurndownService(): TurndownService {
    const turndown = new TurndownService({
      headingStyle: this.config.headingStyle,
      codeBlockStyle: this.config.codeBlockStyle,
      bulletListMarker: this.config.bulletListMarker,
      emDelimiter: this.config.emDelimiter,
    });

    // Code blocks - preserve language hints
    turndown.addRule('codeBlock', {
      filter: (node) => node.nodeName === 'PRE' && node.querySelector('code') !== null,
      replacement: (_content, node) => {
        const codeNode = (node as Element).querySelector('code');
        const language = codeNode?.className?.match(/language-(\w+)/)?.[1] || '';
        const code = codeNode?.textContent || '';
        return `\n\`\`\`${language}\n${code}\n\`\`\`\n`;
      },
    });

    // Paragraphs - ensure proper spacing
    turndown.addRule('paragraph', {
      filter: 'p',
      replacement: (content) => `\n\n${content}\n\n`,
    });

    // Divs - convert to block content
    turndown.addRule('div', {
      filter: 'div',
      replacement: (content) => (content ? `\n${content}\n` : ''),
    });

    // Line breaks
    turndown.addRule('br', {
      filter: 'br',
      replacement: () => '\n',
    });

    // Remove images if configured
    if (this.config.removeImages) {
      turndown.addRule('removeImages', {
        filter: (node) => ['IMG', 'PICTURE', 'FIGURE', 'SVG'].includes(node.nodeName),
        replacement: () => '',
      });
    }

    // Remove media if configured
    if (this.config.removeMedia) {
      turndown.addRule('removeMedia', {
        filter: (node) => ['VIDEO', 'AUDIO', 'IFRAME', 'CANVAS'].includes(node.nodeName),
        replacement: () => '',
      });
    }

    return turndown;
  }
}

/**
 * Create a Converter instance with default configuration
 */
export function createConverter(config?: ConverterConfig): Converter {
  return new Converter(config);
}
