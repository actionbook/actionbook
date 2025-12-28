import { ProxyAgent, fetch as undiciFetch } from 'undici'
import { ApiClient as BaseApiClient } from '@actionbookdev/sdk'

// Re-export types from SDK for convenience
export type {
  ChunkSearchResult,
  ChunkActionDetail,
  SearchType,
  SourceListResult,
  SourceSearchResult,
  SearchActionsParams,
} from '@actionbookdev/sdk'

/**
 * Get proxy URL from environment variables
 */
function getProxyUrl(): string | undefined {
  return (
    process.env.HTTPS_PROXY ||
    process.env.HTTP_PROXY ||
    process.env.https_proxy ||
    process.env.http_proxy
  )
}

/**
 * Check if a URL should bypass the proxy based on NO_PROXY
 */
function shouldBypassProxy(url: string): boolean {
  const noProxy = process.env.NO_PROXY || process.env.no_proxy
  if (!noProxy) return false
  if (noProxy === '*') return true

  const urlHost = new URL(url).hostname.toLowerCase()
  const bypassList = noProxy.split(',').map((h) => h.trim().toLowerCase())

  return bypassList.some((bypass) => {
    if (bypass.startsWith('.')) {
      return urlHost.endsWith(bypass) || urlHost === bypass.slice(1)
    }
    return urlHost === bypass || urlHost.endsWith('.' + bypass)
  })
}

/**
 * Create a fetch function with proxy support
 */
function createProxyFetch(): typeof fetch {
  const proxyUrl = getProxyUrl()
  const proxyAgent = proxyUrl ? new ProxyAgent(proxyUrl) : undefined

  const customFetch = async (
    url: string | URL | Request,
    init?: RequestInit
  ): Promise<Response> => {
    const urlString =
      typeof url === 'string'
        ? url
        : url instanceof URL
        ? url.toString()
        : url.url
    const useProxy = proxyAgent && !shouldBypassProxy(urlString)

    const response = await undiciFetch(
      url as any,
      {
        ...init,
        dispatcher: useProxy ? proxyAgent : undefined,
      } as any
    )

    return response as unknown as Response
  }

  return customFetch as typeof fetch
}

export interface ApiClientOptions {
  apiKey?: string
  timeoutMs?: number
  retry?: {
    maxRetries?: number
    retryDelay?: number
  }
}

/**
 * ApiClient with proxy support for MCP server
 *
 * This extends the base SDK ApiClient with:
 * - undici fetch for better Node.js compatibility
 * - Automatic proxy support via HTTPS_PROXY/HTTP_PROXY environment variables
 * - NO_PROXY support for bypassing proxy
 */
export class ApiClient extends BaseApiClient {
  constructor(baseUrl: string, options: ApiClientOptions = {}) {
    super({
      apiKey: options.apiKey ?? '',
      baseUrl,
      timeoutMs: options.timeoutMs,
      retry: options.retry,
      fetch: createProxyFetch(),
    })
  }
}
