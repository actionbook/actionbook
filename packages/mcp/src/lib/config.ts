import { z } from 'zod'

export const ServerConfigSchema = z.object({
  apiUrl: z.string().url().default('https://api.actionbook.dev'),
  apiKey: z.string().optional(),
  transport: z.enum(['stdio', 'http']).default('stdio'),
  http: z
    .object({
      port: z.number().int().positive().default(3001),
      host: z.string().default('0.0.0.0'),
      corsOrigins: z.array(z.string()).default(['*']),
    })
    .optional(),
  logLevel: z.enum(['debug', 'info', 'warn', 'error']).default('info'),
  timeout: z.number().int().positive().default(30000),
  retry: z
    .object({
      maxRetries: z.number().int().nonnegative().default(3),
      retryDelay: z.number().int().nonnegative().default(1000),
    })
    .default({}),
})

export type ServerConfig = z.infer<typeof ServerConfigSchema>

interface ParsedArgs {
  apiUrl?: string
  apiKey?: string
  transport?: string
  logLevel?: string
  timeout?: number
  retryMax?: number
  retryDelay?: number
  httpPort?: number
  httpHost?: string
  httpCors?: string[]
}

export function loadConfig(
  args: string[],
  env: NodeJS.ProcessEnv = process.env
): ServerConfig {
  const parsedArgs = parseArgs(args)

  const httpConfig =
    parsedArgs.httpPort ||
    parsedArgs.httpHost ||
    parsedArgs.httpCors ||
    env.ACTIONBOOK_HTTP_PORT ||
    env.ACTIONBOOK_HTTP_HOST ||
    env.ACTIONBOOK_HTTP_CORS
      ? {
          port: parsedArgs.httpPort ?? numberFrom(env.ACTIONBOOK_HTTP_PORT),
          host: parsedArgs.httpHost ?? env.ACTIONBOOK_HTTP_HOST,
          corsOrigins:
            parsedArgs.httpCors ??
            (env.ACTIONBOOK_HTTP_CORS
              ? env.ACTIONBOOK_HTTP_CORS.split(',').map((s) => s.trim())
              : undefined),
        }
      : undefined

  const configInput = {
    apiUrl: parsedArgs.apiUrl ?? env.ACTIONBOOK_API_URL,
    apiKey: parsedArgs.apiKey ?? env.ACTIONBOOK_API_KEY,
    transport: parsedArgs.transport ?? env.ACTIONBOOK_TRANSPORT,
    logLevel: parsedArgs.logLevel ?? env.ACTIONBOOK_LOG_LEVEL,
    timeout: parsedArgs.timeout ?? numberFrom(env.ACTIONBOOK_TIMEOUT),
    retry: {
      maxRetries: parsedArgs.retryMax ?? numberFrom(env.ACTIONBOOK_RETRY_MAX),
      retryDelay:
        parsedArgs.retryDelay ?? numberFrom(env.ACTIONBOOK_RETRY_DELAY),
    },
    http: httpConfig,
  }

  return ServerConfigSchema.parse(configInput)
}

function parseArgs(argv: string[]): ParsedArgs {
  const parsed: ParsedArgs = {}

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    const next = argv[i + 1]

    switch (arg) {
      case '--api-url':
        parsed.apiUrl = next
        i += 1
        break
      case '--api-key':
        parsed.apiKey = next
        i += 1
        break
      case '--transport':
        parsed.transport = next
        i += 1
        break
      case '--log-level':
        parsed.logLevel = next
        i += 1
        break
      case '--timeout':
        parsed.timeout = numberFrom(next)
        i += 1
        break
      case '--retry-max':
        parsed.retryMax = numberFrom(next)
        i += 1
        break
      case '--retry-delay':
        parsed.retryDelay = numberFrom(next)
        i += 1
        break
      case '--http-port':
        parsed.httpPort = numberFrom(next)
        i += 1
        break
      case '--http-host':
        parsed.httpHost = next
        i += 1
        break
      case '--http-cors':
        parsed.httpCors = next?.split(',').map((s) => s.trim())
        i += 1
        break
      default:
        break
    }
  }

  return parsed
}

function numberFrom(value?: string): number | undefined {
  if (value === undefined) return undefined
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : undefined
}
