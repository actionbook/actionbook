import { ApiClient, type FetchFunction } from './api-client.js'
import {
  searchActionsDescription,
  searchActionsParams,
  searchActionsSchema,
  getActionByIdDescription,
  getActionByIdParams,
  getActionByIdSchema,
  listSourcesDescription,
  listSourcesParams,
  listSourcesSchema,
  searchSourcesDescription,
  searchSourcesParams,
  searchSourcesSchema,
  type SearchActionsInput,
  type GetActionByIdInput,
  type ListSourcesInput,
  type SearchSourcesInput,
  type ToolParams,
} from './tool-defs.js'
import type {
  ChunkSearchResult,
  ChunkActionDetail,
  SourceListResult,
  SourceSearchResult,
} from './types.js'

/**
 * Method with attached tool definition (description and params)
 */
export interface ToolMethod<TInput, TOutput> {
  (input: TInput): Promise<TOutput>
  /** Tool description for LLM */
  description: string
  /** Tool parameters in JSON Schema and Zod formats */
  params: ToolParams<any>
}

/**
 * Overloaded method signature for searchActions
 */
export interface SearchActionsMethod {
  /** Search with query string only */
  (query: string): Promise<ChunkSearchResult>
  /** Search with full options */
  (options: SearchActionsInput): Promise<ChunkSearchResult>
  /** Tool description for LLM */
  description: string
  /** Tool parameters in JSON Schema and Zod formats */
  params: ToolParams<typeof searchActionsSchema>
}

/**
 * Overloaded method signature for getActionById
 */
export interface GetActionByIdMethod {
  /** Get action by ID */
  (id: number): Promise<ChunkActionDetail>
  /** Get action with options object */
  (options: GetActionByIdInput): Promise<ChunkActionDetail>
  /** Tool description for LLM */
  description: string
  /** Tool parameters in JSON Schema and Zod formats */
  params: ToolParams<typeof getActionByIdSchema>
}

/**
 * Method signature for listSources
 */
export interface ListSourcesMethod {
  /** List sources with default limit */
  (): Promise<SourceListResult>
  /** List sources with options */
  (options: ListSourcesInput): Promise<SourceListResult>
  /** Tool description for LLM */
  description: string
  /** Tool parameters in JSON Schema and Zod formats */
  params: ToolParams<typeof listSourcesSchema>
}

/**
 * Overloaded method signature for searchSources
 */
export interface SearchSourcesMethod {
  /** Search with query string only */
  (query: string): Promise<SourceSearchResult>
  /** Search with full options */
  (options: SearchSourcesInput): Promise<SourceSearchResult>
  /** Tool description for LLM */
  description: string
  /** Tool parameters in JSON Schema and Zod formats */
  params: ToolParams<typeof searchSourcesSchema>
}

export interface ActionbookOptions {
  /** API key for authentication */
  apiKey?: string
  /** Base URL for the API (default: https://api.actionbook.dev) */
  baseUrl?: string
  /** Request timeout in milliseconds (default: 30000) */
  timeoutMs?: number
  /**
   * Custom fetch function. Defaults to global fetch.
   * Use this to provide a custom implementation with proxy support, etc.
   *
   * @example
   * ```typescript
   * import { fetch as undiciFetch, ProxyAgent } from 'undici';
   *
   * const proxyAgent = new ProxyAgent('http://proxy:8080');
   * const client = new Actionbook({
   *   apiKey: 'xxx',
   *   fetch: (url, init) => undiciFetch(url, { ...init, dispatcher: proxyAgent }),
   * });
   * ```
   */
  fetch?: FetchFunction
}

/**
 * Actionbook SDK Client
 *
 * @example
 * ```typescript
 * import { Actionbook } from '@actionbookdev/sdk'
 *
 * const client = new Actionbook({ apiKey: 'YOUR_API_KEY' })
 *
 * // Search for actions
 * const results = await client.searchActions('airbnb search')
 *
 * // Get action details
 * const action = await client.getActionById(123)
 *
 * // Access tool definitions for LLM integration
 * console.log(client.searchActions.description)
 * console.log(client.searchActions.params.json)
 * console.log(client.searchActions.params.zod)
 * ```
 */
export class Actionbook {
  private readonly apiClient: ApiClient

  /** Search for action manuals by keyword */
  public readonly searchActions: SearchActionsMethod

  /** Get complete action details by action ID */
  public readonly getActionById: GetActionByIdMethod

  /** List all available sources */
  public readonly listSources: ListSourcesMethod

  /** Search for sources by keyword */
  public readonly searchSources: SearchSourcesMethod

  constructor(options: ActionbookOptions = {}) {
    // Use environment variable as fallback for API key
    const apiKey = options.apiKey ?? process.env.ACTIONBOOK_API_KEY

    this.apiClient = new ApiClient({
      apiKey,
      baseUrl: options.baseUrl,
      timeoutMs: options.timeoutMs,
      fetch: options.fetch,
    })

    // Create searchActions method with attached tool definition
    const searchActionsFn = async (
      queryOrOptions: string | SearchActionsInput
    ): Promise<ChunkSearchResult> => {
      const options =
        typeof queryOrOptions === 'string'
          ? { query: queryOrOptions }
          : queryOrOptions

      return this.apiClient.searchActions({
        query: options.query,
        type: options.type ?? 'hybrid',
        limit: options.limit ?? 5,
        sourceIds: options.sourceIds,
        minScore: options.minScore,
      })
    }

    // Attach tool definition to the method
    ;(searchActionsFn as SearchActionsMethod).description =
      searchActionsDescription
    ;(searchActionsFn as SearchActionsMethod).params = searchActionsParams
    this.searchActions = searchActionsFn as SearchActionsMethod

    // Create getActionById method with attached tool definition
    const getActionByIdFn = async (
      idOrOptions: number | GetActionByIdInput
    ): Promise<ChunkActionDetail> => {
      const id = typeof idOrOptions === 'number' ? idOrOptions : idOrOptions.id
      return this.apiClient.getActionById(id)
    }

    // Attach tool definition to the method
    ;(getActionByIdFn as GetActionByIdMethod).description =
      getActionByIdDescription
    ;(getActionByIdFn as GetActionByIdMethod).params = getActionByIdParams
    this.getActionById = getActionByIdFn as GetActionByIdMethod

    // Create listSources method with attached tool definition
    const listSourcesFn = async (
      options?: ListSourcesInput
    ): Promise<SourceListResult> => {
      return this.apiClient.listSources(options?.limit ?? 50)
    }

    // Attach tool definition to the method
    ;(listSourcesFn as ListSourcesMethod).description = listSourcesDescription
    ;(listSourcesFn as ListSourcesMethod).params = listSourcesParams
    this.listSources = listSourcesFn as ListSourcesMethod

    // Create searchSources method with attached tool definition
    const searchSourcesFn = async (
      queryOrOptions: string | SearchSourcesInput
    ): Promise<SourceSearchResult> => {
      const opts =
        typeof queryOrOptions === 'string'
          ? { query: queryOrOptions }
          : queryOrOptions

      return this.apiClient.searchSources(opts.query, opts.limit ?? 10)
    }

    // Attach tool definition to the method
    ;(searchSourcesFn as SearchSourcesMethod).description =
      searchSourcesDescription
    ;(searchSourcesFn as SearchSourcesMethod).params = searchSourcesParams
    this.searchSources = searchSourcesFn as SearchSourcesMethod
  }
}
