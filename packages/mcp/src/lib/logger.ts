export type LogLevel = 'debug' | 'info' | 'warn' | 'error'

const LEVEL_WEIGHT: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
}

type LogSink = (level: LogLevel, message: string, ...args: unknown[]) => void

export class Logger {
  private level: LogLevel
  private sink: LogSink

  constructor(level: LogLevel = 'info', sink?: LogSink) {
    this.level = level
    this.sink =
      sink ??
      ((lvl, message, ...args) => {
        const timestamp = new Date().toISOString()
        console.error(`[${timestamp}] [${lvl.toUpperCase()}]`, message, ...args)
      })
  }

  setLevel(level: LogLevel): void {
    this.level = level
  }

  debug(message: string, ...args: unknown[]): void {
    this.log('debug', message, ...args)
  }

  info(message: string, ...args: unknown[]): void {
    this.log('info', message, ...args)
  }

  warn(message: string, ...args: unknown[]): void {
    this.log('warn', message, ...args)
  }

  error(message: string, ...args: unknown[]): void {
    this.log('error', message, ...args)
  }

  private log(level: LogLevel, message: string, ...args: unknown[]): void {
    if (LEVEL_WEIGHT[level] < LEVEL_WEIGHT[this.level]) {
      return
    }
    this.sink(level, message, ...args)
  }
}

export const logger = new Logger(
  (process.env.ACTIONBOOK_LOG_LEVEL as LogLevel) || 'info'
)
