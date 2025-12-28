import { Server } from '@modelcontextprotocol/sdk/server/index.js'
import {
  CallToolRequestSchema,
  ErrorCode,
  InitializeRequestSchema,
  ListToolsRequestSchema,
  McpError,
  type CallToolResult,
  type Tool,
} from '@modelcontextprotocol/sdk/types.js'
import { CURRENT_PROTOCOL_VERSION } from './lib/protocol.js'
import { ApiClient } from './lib/api-client.js'
import { logger } from './lib/logger.js'
import { ServerConfig } from './lib/config.js'
import { ActionbookError, ErrorCodes, isActionbookError } from './lib/errors.js'
import { toolInputToJsonSchema } from './lib/schema.js'
import { createSearchActionsTool } from './tools/search-actions.js'
import { createGetActionByIdTool } from './tools/get-action-by-id.js'
import { createListSourcesTool } from './tools/list-sources.js'
import { createSearchSourcesTool } from './tools/search-sources.js'
import { z } from 'zod'
import type { ToolDefinition } from './tools/index.js'

interface ActionbookMcpServerDeps {
  apiClient?: ApiClient
}

export class ActionbookMcpServer {
  private readonly server: Server
  private readonly tools = new Map<string, ToolDefinition<any>>()
  private readonly apiClient: ApiClient

  constructor(
    private readonly config: ServerConfig,
    deps: ActionbookMcpServerDeps = {}
  ) {
    logger.setLevel(config.logLevel)
    this.apiClient =
      deps.apiClient ??
      new ApiClient(config.apiUrl, {
        apiKey: config.apiKey,
        timeoutMs: config.timeout,
        retry: config.retry,
      })

    this.server = new Server(
      { name: 'actionbook-mcp', version: '0.1.0' },
      {
        capabilities: {
          tools: { listChanged: true },
        },
      }
    )

    this.registerTools()
    this.registerHandlers()
  }

  async start(transport: any): Promise<void> {
    // API health check (non-blocking)
    this.apiClient
      .healthCheck()
      .then((ok) =>
        logger.info(`[Actionbook MCP] API health: ${ok ? 'ok' : 'degraded'}`)
      )
      .catch((err) =>
        logger.warn('[Actionbook MCP] API health check failed', err)
      )

    await this.server.connect(transport)
    logger.info('[Actionbook MCP] Server started')
  }

  async close(): Promise<void> {
    await this.server.close()
    logger.info('[Actionbook MCP] Server closed')
  }

  listTools(): Tool[] {
    return Array.from(this.tools.values()).map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: toolInputToJsonSchema(
        tool.inputSchema
      ) as Tool['inputSchema'],
    }))
  }

  async callTool(name: string, args: unknown): Promise<string> {
    const tool = this.tools.get(name)
    if (!tool) {
      throw new ActionbookError(
        ErrorCodes.INVALID_QUERY,
        `Unknown tool: ${name}`,
        'Available: search_actions, get_action_by_id, list_sources, search_sources'
      )
    }

    const parsed = tool.inputSchema.safeParse(args)
    if (!parsed.success) {
      const errors = parsed.error.errors
        .map((e: z.ZodIssue) => `${e.path.join('.')}: ${e.message}`)
        .join('; ')
      throw new ActionbookError(ErrorCodes.INVALID_QUERY, errors)
    }

    return tool.handler(parsed.data as any)
  }

  private registerTools(): void {
    this.registerTool(createSearchActionsTool(this.apiClient))
    this.registerTool(createGetActionByIdTool(this.apiClient))
    this.registerTool(createListSourcesTool(this.apiClient))
    this.registerTool(createSearchSourcesTool(this.apiClient))
  }

  private registerTool(tool: ToolDefinition<any>): void {
    this.tools.set(tool.name, tool)
    logger.debug(`[Actionbook MCP] Registered tool: ${tool.name}`)
  }

  private registerHandlers(): void {
    this.server.setRequestHandler(ListToolsRequestSchema, async () => ({
      tools: this.listTools(),
    }))

    this.server.setRequestHandler(
      CallToolRequestSchema,
      async (request): Promise<CallToolResult> => {
        const { name, arguments: args } = request.params
        try {
          const result = await this.callTool(name, args)
          return {
            content: [{ type: 'text', text: result }],
          }
        } catch (error) {
          return this.handleToolError(name, error)
        }
      }
    )

    this.server.setRequestHandler(InitializeRequestSchema, async () => ({
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      capabilities: { tools: { listChanged: true } },
      serverInfo: { name: 'actionbook-mcp', version: '0.1.0' },
    }))
  }

  private handleToolError(toolName: string, error: unknown): CallToolResult {
    logger.error(`[Actionbook MCP] Tool ${toolName} error`, error)
    const message = this.formatError(error)
    return {
      isError: true,
      content: [{ type: 'text', text: message }],
    }
  }

  private formatError(error: unknown): string {
    if (isActionbookError(error)) {
      const lines = [`## Error: ${error.code}`, '', error.message]
      if (error.suggestion) {
        lines.push('', '**Suggestion:**', error.suggestion)
      }
      return lines.join('\n')
    }

    if (error instanceof McpError) {
      return `## MCP Error\n\n${error.message}`
    }

    if (error instanceof Error) {
      return `## Internal Error\n\n${error.message}`
    }

    return '## Internal Error\n\nUnknown error'
  }
}
