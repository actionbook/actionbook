import * as fs from "fs";
import * as path from "path";

/**
 * File logger for persistent logging
 * Each instance manages its own log file, allowing parallel execution
 */
export class FileLogger {
  private logFile: string | null = null;
  private writeStream: fs.WriteStream | null = null;

  /**
   * Initialize with auto-generated filename
   */
  initialize(baseDir: string = ".", prefix: string = "action-builder"): string {
    const logsDir = path.join(baseDir, "logs");

    if (!fs.existsSync(logsDir)) {
      fs.mkdirSync(logsDir, { recursive: true });
    }

    const now = new Date();
    const timestamp =
      now.getFullYear().toString() +
      String(now.getMonth() + 1).padStart(2, "0") +
      String(now.getDate()).padStart(2, "0") +
      String(now.getHours()).padStart(2, "0") +
      String(now.getMinutes()).padStart(2, "0") +
      String(now.getSeconds()).padStart(2, "0");

    this.logFile = path.join(logsDir, `${prefix}_${timestamp}.log`);
    this.writeStream = fs.createWriteStream(this.logFile, { flags: "a" });

    return this.logFile;
  }

  /**
   * Initialize with explicit log file path
   */
  initializeWithPath(logFilePath: string): string {
    const logsDir = path.dirname(logFilePath);

    if (!fs.existsSync(logsDir)) {
      fs.mkdirSync(logsDir, { recursive: true });
    }

    this.logFile = logFilePath;
    this.writeStream = fs.createWriteStream(this.logFile, { flags: "a" });

    return this.logFile;
  }

  write(message: string): void {
    if (this.writeStream) {
      this.writeStream.write(message + "\n");
    }
  }

  close(): void {
    if (this.writeStream) {
      this.writeStream.end();
      this.writeStream = null;
    }
  }

  getLogFile(): string | null {
    return this.logFile;
  }
}

/**
 * Global file logger instance for backward compatibility
 * Note: For parallel execution, create separate FileLogger instances
 */
export const fileLogger = new FileLogger();

export type LogLevel = "info" | "warn" | "error" | "debug";

/**
 * Log a message with level and timestamp
 */
export function log(level: LogLevel, ...args: unknown[]): void {
  const timestamp = new Date().toISOString();
  const prefix = `[${timestamp}] [${level.toUpperCase()}]`;

  const message =
    prefix +
    " " +
    args
      .map((arg) => (typeof arg === "object" ? JSON.stringify(arg) : String(arg)))
      .join(" ");

  fileLogger.write(message);

  switch (level) {
    case "error":
      console.error(prefix, ...args);
      break;
    case "warn":
      console.warn(prefix, ...args);
      break;
    case "debug":
      if (process.env.DEBUG) {
        console.log(prefix, ...args);
      }
      break;
    default:
      console.log(prefix, ...args);
  }
}

/**
 * Log raw output without formatting (for tables, etc.)
 */
export function logRaw(message: string): void {
  fileLogger.write(message);
  console.log(message);
}
