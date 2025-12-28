import crypto from 'crypto'
import type { HeadingItem } from '@actionbookdev/db'

export interface ChunkerOptions {
  chunkSize: number // Target tokens per chunk
  chunkOverlap: number // Overlap tokens between chunks
  minChunkSize: number // Minimum chunk size
  splitHeadingLevel: number // Only split at this heading level (1=H1, 2=H2, etc.)
}

export interface ChunkData {
  content: string
  chunkIndex: number
  startChar: number
  endChar: number
  heading?: string
  headingHierarchy: HeadingItem[]
  tokenCount: number
}

const DEFAULT_OPTIONS: ChunkerOptions = {
  chunkSize: 2000,
  chunkOverlap: 50,
  minChunkSize: 100,
  splitHeadingLevel: 2,
}

interface Section {
  content: string
  heading: string
  headingHierarchy: HeadingItem[]
  startChar: number
}

export class DocumentChunker {
  private options: ChunkerOptions

  constructor(options: Partial<ChunkerOptions> = {}) {
    this.options = { ...DEFAULT_OPTIONS, ...options }
  }

  // Headings to skip (noise sections that don't contain useful content)
  private static SKIP_HEADINGS = [
    'Related articles',
    'Related topics',
    'Site Footer',
    'Support',
    'Hosting',
    'Airbnb',
  ]

  /**
   * Split a markdown document into chunks
   */
  chunk(content: string): ChunkData[] {
    // If chunkSize is very large (e.g., --no-chunk mode), return entire content as single chunk
    const contentTokens = this.estimateTokens(content)
    if (contentTokens <= this.options.chunkSize) {
      const h1Match = content.match(/^#\s+(.+)$/m)
      return [
        {
          content: content.trim(),
          chunkIndex: 0,
          startChar: 0,
          endChar: content.length,
          heading: h1Match ? h1Match[1].trim() : '',
          headingHierarchy: h1Match
            ? [{ level: 1, text: h1Match[1].trim() }]
            : [],
          tokenCount: contentTokens,
        },
      ]
    }

    const sections = this.splitByHeadings(content)
    const chunks: ChunkData[] = []
    let chunkIndex = 0

    for (const section of sections) {
      // Skip noise sections like "Related articles"
      if (DocumentChunker.SKIP_HEADINGS.includes(section.heading)) {
        continue
      }

      const sectionChunks = this.chunkSection(section, chunkIndex)
      chunks.push(...sectionChunks)
      chunkIndex += sectionChunks.length
    }

    return chunks
  }

  /**
   * Split content by markdown headings at the specified level
   */
  private splitByHeadings(content: string): Section[] {
    const splitLevel = this.options.splitHeadingLevel
    const headingPattern = new RegExp(`^(#{1,${splitLevel}})\\s+(.+)$`, 'gm')
    const headingStack: Array<{ level: number; text: string }> = []

    // Find all split positions
    headingPattern.lastIndex = 0
    const splitPositions: number[] = []
    let match

    while ((match = headingPattern.exec(content)) !== null) {
      if (match[1].length === splitLevel) {
        splitPositions.push(match.index)
      }
    }
    splitPositions.push(content.length)

    // Rebuild sections with content
    const finalSections: Section[] = []
    headingPattern.lastIndex = 0
    headingStack.length = 0

    for (let i = 0; i < splitPositions.length - 1; i++) {
      const start = splitPositions[i]
      const end = splitPositions[i + 1]
      const sectionContent = content.substring(start, end)

      // Extract heading from this section
      const firstLineMatch = sectionContent.match(/^(#{1,6})\s+(.+)$/m)
      const heading = firstLineMatch ? firstLineMatch[2].trim() : ''
      const level = firstLineMatch ? firstLineMatch[1].length : splitLevel

      // Update heading stack
      while (
        headingStack.length > 0 &&
        headingStack[headingStack.length - 1].level >= level
      ) {
        headingStack.pop()
      }
      if (heading) {
        headingStack.push({ level, text: heading })
      }

      finalSections.push({
        content: sectionContent,
        heading,
        headingHierarchy: headingStack.map((h) => ({
          level: h.level,
          text: h.text,
        })),
        startChar: start,
      })
    }

    // Handle content before first split-level heading
    if (splitPositions[0] > 0) {
      const introContent = content.substring(0, splitPositions[0])
      if (introContent.trim()) {
        const h1Match = introContent.match(/^#\s+(.+)$/m)
        finalSections.unshift({
          content: introContent,
          heading: h1Match ? h1Match[1].trim() : '',
          headingHierarchy: h1Match
            ? [{ level: 1, text: h1Match[1].trim() }]
            : [],
          startChar: 0,
        })
      }
    }

    // If no sections found, treat entire content as one section
    if (finalSections.length === 0) {
      const h1Match = content.match(/^#\s+(.+)$/m)
      finalSections.push({
        content,
        heading: h1Match ? h1Match[1].trim() : '',
        headingHierarchy: h1Match
          ? [{ level: 1, text: h1Match[1].trim() }]
          : [],
        startChar: 0,
      })
    }

    return finalSections
  }

  /**
   * Chunk a single section
   */
  private chunkSection(section: Section, startIndex: number): ChunkData[] {
    const tokens = this.estimateTokens(section.content)

    // If section is small enough, return as single chunk
    if (tokens <= this.options.chunkSize) {
      return [
        {
          content: section.content.trim(),
          chunkIndex: startIndex,
          startChar: section.startChar,
          endChar: section.startChar + section.content.length,
          heading: section.heading,
          headingHierarchy: section.headingHierarchy,
          tokenCount: tokens,
        },
      ]
    }

    // Split by paragraphs
    const paragraphs = this.splitParagraphs(section.content)
    const chunks: ChunkData[] = []

    let currentChunk = ''
    let currentTokens = 0
    let chunkStartChar = section.startChar
    let charOffset = 0

    for (const paragraph of paragraphs) {
      const paraTokens = this.estimateTokens(paragraph)

      // If adding this paragraph exceeds chunk size
      if (currentTokens + paraTokens > this.options.chunkSize && currentChunk) {
        // Save current chunk
        chunks.push({
          content: currentChunk.trim(),
          chunkIndex: startIndex + chunks.length,
          startChar: chunkStartChar,
          endChar: section.startChar + charOffset,
          heading: section.heading,
          headingHierarchy: section.headingHierarchy,
          tokenCount: currentTokens,
        })

        // Start new chunk with overlap
        const overlap = this.getOverlapText(currentChunk)
        currentChunk = overlap + paragraph
        currentTokens = this.estimateTokens(currentChunk)
        chunkStartChar = section.startChar + charOffset - overlap.length
      } else {
        currentChunk += paragraph
        currentTokens += paraTokens
      }

      charOffset += paragraph.length
    }

    // Add final chunk - merge into previous if too small to preserve content
    if (currentChunk.trim()) {
      const finalTokens = this.estimateTokens(currentChunk)

      if (finalTokens >= this.options.minChunkSize || chunks.length === 0) {
        // Normal case: add as new chunk
        chunks.push({
          content: currentChunk.trim(),
          chunkIndex: startIndex + chunks.length,
          startChar: chunkStartChar,
          endChar: section.startChar + section.content.length,
          heading: section.heading,
          headingHierarchy: section.headingHierarchy,
          tokenCount: finalTokens,
        })
      } else {
        // Small final chunk: merge into previous chunk to avoid losing content
        const lastChunk = chunks[chunks.length - 1]
        lastChunk.content = lastChunk.content + '\n\n' + currentChunk.trim()
        lastChunk.endChar = section.startChar + section.content.length
        lastChunk.tokenCount = this.estimateTokens(lastChunk.content)
      }
    }

    return chunks
  }

  /**
   * Split content into paragraphs while preserving code blocks
   */
  private splitParagraphs(content: string): string[] {
    // Protect code blocks
    const codeBlocks: string[] = []
    const protectedContent = content.replace(/```[\s\S]*?```/g, (match) => {
      codeBlocks.push(match)
      return `__CODE_BLOCK_${codeBlocks.length - 1}__`
    })

    // Split by double newlines
    const paragraphs = protectedContent.split(/\n\s*\n/).map((p) => p + '\n\n')

    // Restore code blocks
    return paragraphs.map((para) => {
      return para.replace(/__CODE_BLOCK_(\d+)__/g, (_, index) => {
        return codeBlocks[parseInt(index)]
      })
    })
  }

  /**
   * Get overlap text from the end of a chunk
   */
  private getOverlapText(text: string): string {
    const words = text.trim().split(/\s+/)
    const overlapWords = words.slice(-this.options.chunkOverlap)
    return overlapWords.join(' ') + ' '
  }

  /**
   * Estimate token count (rough approximation)
   */
  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4)
  }
}

/**
 * Generate a hash for chunk content
 */
export function hashChunk(content: string): string {
  return crypto
    .createHash('sha256')
    .update(content)
    .digest('hex')
    .substring(0, 16)
}
