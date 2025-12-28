/**
 * Eval Logger
 *
 * Provides per-task logging for eval runs.
 * Each task gets its own log file for experiment analysis.
 *
 * Directory structure:
 *   logs/{experiment}/{caseId}/
 *     ├── eval.log           # Eval task log
 *     └── action-builder.log # ActionBuilder log (if online mode)
 */

import fs from "fs";
import path from "path";

const LOGS_DIR = path.resolve(import.meta.dirname, "../../logs");

export interface EvalLogEntry {
  timestamp: string;
  level: "info" | "warn" | "error" | "debug";
  message: string;
  data?: unknown;
}

export class EvalLogger {
  private logFile: string;
  private logDir: string;
  private entries: EvalLogEntry[] = [];
  private caseId: string;
  private experimentName: string;

  constructor(experimentName: string, caseId: string) {
    this.experimentName = experimentName;
    this.caseId = caseId;

    // Create log directory: logs/{experiment}/{caseId}/
    const sanitizedCaseId = caseId.replace(/[^a-zA-Z0-9_-]/g, "_");
    this.logDir = path.join(LOGS_DIR, experimentName, sanitizedCaseId);

    if (!fs.existsSync(this.logDir)) {
      fs.mkdirSync(this.logDir, { recursive: true });
    }

    // Eval log file path
    this.logFile = path.join(this.logDir, "eval.log");
  }

  /**
   * Get the log directory for this case
   * (Used to pass to ActionBuilder for its log file)
   */
  getLogDir(): string {
    return this.logDir;
  }

  /**
   * Get the ActionBuilder log file path
   */
  getActionBuilderLogFile(): string {
    return path.join(this.logDir, "action-builder.log");
  }

  private formatEntry(entry: EvalLogEntry): string {
    const dataStr = entry.data ? ` | ${JSON.stringify(entry.data)}` : "";
    return `[${entry.timestamp}] [${entry.level.toUpperCase()}] ${entry.message}${dataStr}`;
  }

  private log(level: EvalLogEntry["level"], message: string, data?: unknown): void {
    const entry: EvalLogEntry = {
      timestamp: new Date().toISOString(),
      level,
      message,
      data,
    };
    this.entries.push(entry);

    // Also output to console
    const formatted = this.formatEntry(entry);
    if (level === "error") {
      console.error(formatted);
    } else {
      console.log(formatted);
    }
  }

  info(message: string, data?: unknown): void {
    this.log("info", message, data);
  }

  warn(message: string, data?: unknown): void {
    this.log("warn", message, data);
  }

  error(message: string, data?: unknown): void {
    this.log("error", message, data);
  }

  debug(message: string, data?: unknown): void {
    this.log("debug", message, data);
  }

  /**
   * Log a section header
   */
  section(title: string): void {
    this.info(`${"=".repeat(60)}`);
    this.info(title);
    this.info(`${"=".repeat(60)}`);
  }

  /**
   * Log robustness results in detail
   */
  logRobustnessResults(results: {
    score: number;
    validCount: number;
    totalCount: number;
    elementResults: Array<{
      elementId: string;
      score: number;
      envResults: Array<{
        envId: string;
        valid: boolean;
        found: boolean;
        unique: boolean;
        visible: boolean;
        count: number;
        error?: string;
      }>;
    }>;
  }): void {
    this.section("Robustness Results");
    this.info(`Score: ${results.validCount}/${results.totalCount} (${(results.score * 100).toFixed(1)}%)`);

    for (const elem of results.elementResults) {
      const status = elem.score === 1 ? "✓" : elem.score > 0 ? "△" : "✗";
      const details = elem.envResults.map(e => {
        if (e.valid) return `${e.envId}:✓`;
        if (!e.found) return `${e.envId}:not_found`;
        if (!e.unique) return `${e.envId}:not_unique(${e.count})`;
        if (!e.visible) return `${e.envId}:not_visible`;
        return `${e.envId}:${e.error || "failed"}`;
      }).join(", ");
      this.info(`  ${status} ${elem.elementId}: ${details}`);
    }
  }

  /**
   * Log recall results in detail
   */
  logRecallResults(results: {
    score: number;
    matched: number;
    total: number;
    details: Array<{
      goldenId: string;
      matched: boolean;
      matchedRecordedId?: string;
      matchMethod?: string;
      error?: string;
    }>;
  }): void {
    this.section("Recall Results");
    this.info(`Score: ${results.matched}/${results.total} (${(results.score * 100).toFixed(1)}%)`);

    for (const detail of results.details) {
      const status = detail.matched ? "✓" : "✗";
      const matchInfo = detail.matched
        ? `-> ${detail.matchedRecordedId} [${detail.matchMethod}]`
        : detail.error || "not matched";
      this.info(`  ${status} ${detail.goldenId}: ${matchInfo}`);
    }
  }

  /**
   * Log redundancy results in detail
   */
  logRedundancyResults(results: {
    score: number;
    matchedCount: number;
    totalRecorded: number;
    redundantElements: string[];
  }): void {
    this.section("Redundancy Results");
    this.info(`Score: ${(results.score * 100).toFixed(1)}% redundant`);
    this.info(`Matched: ${results.matchedCount}/${results.totalRecorded} elements`);

    if (results.redundantElements.length > 0) {
      this.info(`Redundant elements: ${results.redundantElements.join(", ")}`);
    }
  }

  /**
   * Log build results
   */
  logBuildResults(results: {
    success: boolean;
    tokens: number;
    turns: number;
    duration: number;
    pages?: number;
    elements?: number;
    error?: string;
  }): void {
    this.section("Build Results");
    this.info(`Success: ${results.success}`);
    this.info(`Tokens: ${results.tokens}`);
    this.info(`Turns: ${results.turns}`);
    this.info(`Duration: ${(results.duration / 1000).toFixed(1)}s`);
    if (results.pages !== undefined) {
      this.info(`Pages: ${results.pages}`);
    }
    if (results.elements !== undefined) {
      this.info(`Elements: ${results.elements}`);
    }
    if (results.error) {
      this.error(`Error: ${results.error}`);
    }
  }

  /**
   * Save log to file
   */
  save(): void {
    const content = this.entries.map(e => this.formatEntry(e)).join("\n");
    fs.writeFileSync(this.logFile, content, "utf-8");
    console.log(`[EvalLogger] Saved log to: ${this.logFile}`);
  }

  /**
   * Get log file path
   */
  getLogFile(): string {
    return this.logFile;
  }

  /**
   * Get all entries as JSON (for Braintrust metadata)
   */
  toJSON(): EvalLogEntry[] {
    return this.entries;
  }
}

/**
 * Global logger registry for current experiment
 */
let currentExperiment: string | undefined;
const loggers = new Map<string, EvalLogger>();

export function setCurrentExperiment(name: string): void {
  currentExperiment = name;
}

export function getLogger(caseId: string): EvalLogger {
  if (!currentExperiment) {
    currentExperiment = `eval-${new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19)}`;
  }

  const key = `${currentExperiment}_${caseId}`;
  if (!loggers.has(key)) {
    loggers.set(key, new EvalLogger(currentExperiment, caseId));
  }
  return loggers.get(key)!;
}

export function saveAllLogs(): void {
  for (const logger of loggers.values()) {
    logger.save();
  }
  loggers.clear();
}
