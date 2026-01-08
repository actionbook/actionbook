/**
 * Logger utility for browser package
 *
 * Provides a simple logging interface that can be customized
 * by consumers of the package.
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export type LogFunction = (level: LogLevel, message: string) => void;

/**
 * Default logger that outputs to console
 */
const defaultLogger: LogFunction = (level: LogLevel, message: string) => {
  const timestamp = new Date().toISOString();
  const prefix = `[${timestamp}] [${level.toUpperCase()}]`;

  switch (level) {
    case 'error':
      console.error(`${prefix} ${message}`);
      break;
    case 'warn':
      console.warn(`${prefix} ${message}`);
      break;
    case 'debug':
      if (process.env.DEBUG) {
        console.debug(`${prefix} ${message}`);
      }
      break;
    default:
      console.log(`${prefix} ${message}`);
  }
};

let currentLogger: LogFunction = defaultLogger;

/**
 * Set a custom logger function
 */
export function setLogger(logger: LogFunction): void {
  currentLogger = logger;
}

/**
 * Reset to default logger
 */
export function resetLogger(): void {
  currentLogger = defaultLogger;
}

/**
 * Log a message
 */
export function log(level: LogLevel, message: string): void {
  currentLogger(level, message);
}
