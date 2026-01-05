/**
 * SelectorOptimizer - Use LLM to batch analyze and optimize CSS selectors
 *
 * This optimizer takes all discovered elements and uses LLM to:
 * 1. Filter out unstable selectors (e.g., dynamic counters, timestamps)
 * 2. Choose the best CSS selector for each element
 * 3. Assign appropriate confidence scores
 */

import { generateText } from 'ai'
import { createOpenAI } from '@ai-sdk/openai'
import { createAnthropic } from '@ai-sdk/anthropic'
import { createOpenRouter } from '@openrouter/ai-sdk-provider'
import { createAmazonBedrock } from '@ai-sdk/amazon-bedrock'
import type { LanguageModel } from 'ai'
import { ProxyAgent, fetch as undiciFetch } from 'undici'
import { log } from '../utils/logger.js'
import type { SelectorItem } from '@actionbookdev/db'
import type { ElementCapability } from '../types/capability.js'

/**
 * Input format for selector optimization
 */
interface SelectorInput {
  elementId: string
  description: string
  selectors: SelectorItem[]
}

/**
 * Output format from LLM analysis
 */
interface SelectorAnalysisResult {
  elementId: string
  bestSelectorIndex: number // Index of the best selector in the selectors array
  unstableSelectorIndices: number[] // Indices of all unstable selectors
  reason: string
  adjustedConfidence?: number
}

/**
 * Optimization result for a single element
 */
export interface OptimizedElement {
  elementId: string
  originalSelectors: SelectorItem[]
  optimizedSelectors: SelectorItem[]
  reason: string
}

/**
 * Batch optimization result
 */
export interface OptimizationResult {
  success: boolean
  optimizedCount: number
  totalElements: number
  elements: OptimizedElement[]
  tokensUsed: {
    input: number
    output: number
    total: number
  }
  error?: string
}

type LLMProvider = 'openrouter' | 'openai' | 'anthropic' | 'bedrock'

/**
 * Create a fetch function with proxy support
 */
function createProxyFetch(): typeof fetch | undefined {
  const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY
  if (!proxyUrl) return undefined

  const proxyAgent = new ProxyAgent(proxyUrl)
  return async (
    input: RequestInfo | URL,
    init?: RequestInit
  ): Promise<Response> => {
    const url =
      typeof input === 'string'
        ? input
        : input instanceof URL
        ? input.href
        : input.url
    const response = await undiciFetch(url, {
      ...init,
      dispatcher: proxyAgent,
    } as Parameters<typeof undiciFetch>[1])
    return response as unknown as Response
  }
}

export class SelectorOptimizer {
  private model: LanguageModel

  constructor() {
    const { provider, model, llmModel } = this.resolveConfig()
    this.model = llmModel
    log('info', `[SelectorOptimizer] Initialized with ${provider}/${model}`)
  }

  private resolveConfig(): {
    provider: LLMProvider
    model: string
    llmModel: LanguageModel
  } {
    const proxyFetch = createProxyFetch()
    const openrouterKey = process.env.OPENROUTER_API_KEY
    const openaiKey = process.env.OPENAI_API_KEY
    const anthropicKey = process.env.ANTHROPIC_API_KEY
    const hasBedrock =
      process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY

    // Use a smaller/faster model for selector optimization
    const optimizerModel = process.env.SELECTOR_OPTIMIZER_MODEL

    if (openrouterKey) {
      const model = optimizerModel || 'gpt-4o-mini'
      const openrouter = createOpenRouter({
        apiKey: openrouterKey,
        fetch: proxyFetch,
      })
      return { provider: 'openrouter', model, llmModel: openrouter(model) }
    }
    if (openaiKey) {
      const model = optimizerModel || 'gpt-4o-mini'
      const openai = createOpenAI({ apiKey: openaiKey, fetch: proxyFetch })
      return { provider: 'openai', model, llmModel: openai(model) }
    }
    if (anthropicKey) {
      const model = optimizerModel || 'claude-3-5-haiku-latest'
      const anthropic = createAnthropic({
        apiKey: anthropicKey,
        fetch: proxyFetch,
      })
      return { provider: 'anthropic', model, llmModel: anthropic(model) }
    }
    if (hasBedrock) {
      const model = optimizerModel || 'anthropic.claude-3-5-haiku-20241022-v1:0'
      const bedrock = createAmazonBedrock({
        region: process.env.AWS_REGION || 'us-east-1',
        accessKeyId: process.env.AWS_ACCESS_KEY_ID,
        secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY,
        sessionToken: process.env.AWS_SESSION_TOKEN,
        fetch: proxyFetch,
      })
      return { provider: 'bedrock', model, llmModel: bedrock(model) }
    }

    throw new Error('No LLM API key found for SelectorOptimizer')
  }

  /**
   * Batch optimize selectors for all elements
   */
  async optimizeSelectors(
    elements: Map<string, ElementCapability>
  ): Promise<OptimizationResult> {
    const elementsArray = Array.from(elements.values())

    if (elementsArray.length === 0) {
      return {
        success: true,
        optimizedCount: 0,
        totalElements: 0,
        elements: [],
        tokensUsed: { input: 0, output: 0, total: 0 },
      }
    }

    log(
      'info',
      `[SelectorOptimizer] Optimizing selectors for ${elementsArray.length} elements`
    )

    // Prepare input for LLM
    const selectorInputs: SelectorInput[] = elementsArray.map((el) => ({
      elementId: el.id,
      description: el.description,
      selectors: el.selectors,
    }))

    try {
      // Call LLM to analyze selectors
      const analysisResults = await this.callLLM(selectorInputs)

      // Apply optimization results
      const optimizedElements: OptimizedElement[] = []
      let optimizedCount = 0

      for (const result of analysisResults) {
        const element = elements.get(result.elementId)
        if (!element) continue

        const originalSelectors = [...element.selectors]

        // Mark all unstable selectors with low confidence
        const unstableIndices = result.unstableSelectorIndices || []
        for (const idx of unstableIndices) {
          if (idx >= 0 && idx < element.selectors.length) {
            element.selectors[idx].confidence = 0.1
            log(
              'debug',
              `[SelectorOptimizer] Marked unstable: ${element.id} - ${element.selectors[idx].value}`
            )
          }
        }

        // Reorder selectors based on LLM recommendation
        const bestIdx = result.bestSelectorIndex
        if (bestIdx >= 0 && bestIdx < element.selectors.length) {
          const bestSelector = element.selectors[bestIdx]

          if (bestIdx === 0) {
            // Best selector is already first, just update confidence
            element.selectors[0] = {
              ...bestSelector,
              priority: 1,
              confidence: result.adjustedConfidence || bestSelector.confidence,
            }
          } else {
            // Move the best selector to the front and update priorities
            element.selectors = [
              {
                ...bestSelector,
                priority: 1,
                confidence:
                  result.adjustedConfidence || bestSelector.confidence,
              },
              ...element.selectors
                .filter((_, i) => i !== bestIdx)
                .map((s, i) => ({
                  ...s,
                  priority: i + 2,
                })),
            ]
          }
          optimizedCount++
        }

        optimizedElements.push({
          elementId: element.id,
          originalSelectors,
          optimizedSelectors: element.selectors,
          reason: result.reason,
        })
      }

      log(
        'info',
        `[SelectorOptimizer] Optimized ${optimizedCount}/${elementsArray.length} elements`
      )

      return {
        success: true,
        optimizedCount,
        totalElements: elementsArray.length,
        elements: optimizedElements,
        tokensUsed: { input: 0, output: 0, total: 0 }, // TODO: track actual tokens
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      log(
        'error',
        `[SelectorOptimizer] Failed to optimize selectors: ${errorMsg}`
      )
      return {
        success: false,
        optimizedCount: 0,
        totalElements: elementsArray.length,
        elements: [],
        tokensUsed: { input: 0, output: 0, total: 0 },
        error: errorMsg,
      }
    }
  }

  private async callLLM(
    inputs: SelectorInput[]
  ): Promise<SelectorAnalysisResult[]> {
    const systemPrompt = `You are a web automation expert analyzing CSS selectors for stability and reliability.

Your task is to analyze each element's selectors and:
1. Identify the BEST selector for reliable element targeting
2. Flag selectors that contain DYNAMIC content (unstable)

UNSTABLE patterns to detect:
- Counters/notifications: "0 条新通知", "3 new messages", "(5)", "+99"
- Timestamps: "2 minutes ago", "刚刚", "Yesterday"
- User-specific: Contains usernames, IDs like "user-12345"
- Session-specific: Random tokens, session IDs
- Framework-generated IDs: "#ember123", "#react-xxx", ".ng-c123", ":r0:", ":r1:", "#radix-123"
- Hash-based CSS classes (change on each build):
  - CSS Modules: ".styles_button__x7y8z", ".Component_name__hash"
  - styled-components: ".sc-1a2b3c", ".sc-aBcDeF"
  - Emotion: ".css-1a2b3c", ".css-xxxxxx"
  - Generic hash patterns: classes ending with "_[a-z0-9]{5,}", "__[a-z0-9]{5,}", or "-[a-z0-9]{6,}"

STABLE patterns (prefer these):
- data-testid: Always stable, designed for testing
- data-* attributes: data-id, data-component, data-element, data-action, data-section (semantic, stable)
- Static aria-label: "Submit", "Close", "Search" (no dynamic content)
- Semantic IDs: "main-nav", "search-button"
- BEM class names: "header__nav-item", "btn--primary"

SELECTOR PRIORITY (when all are stable):
1. data-testid (most stable)
2. Other semantic data-* attributes (data-id, data-component, etc.)
3. id selector (#id)
4. Static aria-label
5. Semantic CSS class
6. XPath (least preferred)

Return JSON array with your analysis.`

    const userPrompt = `Analyze these element selectors and return the best choice for each:

${JSON.stringify(inputs, null, 2)}

Return a JSON array with this format:
[
  {
    "elementId": "element_id",
    "bestSelectorIndex": 1,
    "unstableSelectorIndices": [0, 2],
    "reason": "Brief explanation",
    "adjustedConfidence": 0.9
  }
]

Rules:
- bestSelectorIndex: Index of the BEST stable selector (0-based). If all are unstable, pick the least bad one.
- unstableSelectorIndices: Array of indices for ALL selectors that contain dynamic/unstable content. Can be empty [] if all are stable.
- adjustedConfidence: 0.1-0.95 based on best selector's reliability
- reason: Brief explanation of why you chose this selector and what makes others unstable`

    const startTime = Date.now()

    try {
      const response = await generateText({
        model: this.model,
        messages: [
          { role: 'system', content: systemPrompt },
          { role: 'user', content: userPrompt },
        ],
        temperature: 0.1,
      })

      const latencyMs = Date.now() - startTime
      log('info', `[SelectorOptimizer] LLM call completed in ${latencyMs}ms`)

      // Parse response
      const text = response.text.trim()

      // Extract JSON from response (handle markdown code blocks)
      let jsonStr = text
      const jsonMatch = text.match(/```(?:json)?\s*([\s\S]*?)\s*```/)
      if (jsonMatch) {
        jsonStr = jsonMatch[1]
      }

      const results: SelectorAnalysisResult[] = JSON.parse(jsonStr)
      return results
    } catch (error) {
      log('error', `[SelectorOptimizer] LLM call failed: ${error}`)
      throw error
    }
  }
}
