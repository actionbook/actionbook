import { StagehandBrowser } from './browser/StagehandBrowser.js'
import type { BrowserAdapter } from './browser/BrowserAdapter.js'
import { AIClient } from './llm/AIClient.js'
import { ActionRecorder } from './recorder/ActionRecorder.js'
import { SelectorValidator } from './validator/SelectorValidator.js'
import { YamlWriter } from './writers/YamlWriter.js'
import { DbWriter } from './writers/DbWriter.js'
import {
  CAPABILITY_RECORDER_SYSTEM_PROMPT,
  generateUserPrompt,
} from './llm/prompts/capability-recorder.js'
import { FileLogger, fileLogger as globalFileLogger } from './utils/logger.js'
import { createDb, closeDb, type Database } from '@actionbookdev/db'
import type {
  ActionBuilderConfig,
  BuildOptions,
  BuildResult,
  ValidationResult,
} from './types/index.js'

const DEFAULT_OUTPUT_DIR = './output'
const DEFAULT_MAX_TURNS = 20

/**
 * Validation options for ActionBuilder.validate()
 */
export interface ValidateOptions {
  /** Filter validation to specific page types */
  pageFilter?: string[]
  /** Template parameters for validating template selectors */
  templateParams?: Record<string, string>
  /** Enable verbose output */
  verbose?: boolean
}

/**
 * ActionBuilder - Main coordinator for capability recording and validation
 */
export class ActionBuilder {
  private browser: BrowserAdapter
  private llmClient: AIClient
  private recorder: ActionRecorder
  private validator: SelectorValidator
  private yamlWriter: YamlWriter
  private dbWriter: DbWriter | null = null
  private db: Database | null = null
  private config: ActionBuilderConfig
  private initialized: boolean = false
  private fileLogger: FileLogger

  constructor(config: ActionBuilderConfig) {
    this.config = {
      headless: config.headless ?? false,
      maxTurns: config.maxTurns ?? DEFAULT_MAX_TURNS,
      outputDir: config.outputDir ?? DEFAULT_OUTPUT_DIR,
      llmApiKey: config.llmApiKey,
      llmProvider: config.llmProvider,
      llmModel: config.llmModel,
      databaseUrl: config.databaseUrl,
      logFile: config.logFile,
      profileEnabled: config.profileEnabled,
      profileDir: config.profileDir,
    }

    // Create instance-specific file logger for parallel execution support
    this.fileLogger = new FileLogger()

    // Initialize browser (Stagehand has its own LLM config via env vars)
    this.browser = new StagehandBrowser({
      headless: this.config.headless!,
      llmApiKey: this.config.llmApiKey || '',
      llmBaseURL: config.llmBaseURL,
      llmModel: this.config.llmModel,
      profile: config.profileEnabled
        ? { enabled: true, profileDir: config.profileDir }
        : undefined,
    })

    // Initialize AI client with multi-provider support
    this.llmClient = new AIClient({
      provider: this.config.llmProvider,
      model: this.config.llmModel,
      apiKey: this.config.llmApiKey,
    })

    // Initialize YAML writer
    this.yamlWriter = new YamlWriter(this.config.outputDir)

    // Initialize database connection and DbWriter if databaseUrl is provided
    if (this.config.databaseUrl) {
      this.db = createDb(this.config.databaseUrl)
      this.dbWriter = new DbWriter(this.db)
      this.log(
        'info',
        '[ActionBuilder] Database connection initialized for dual-write'
      )
    }

    // Initialize recorder (with optional dbWriter and onStepFinish callback)
    // Disable observe efficiency check for task-driven mode compatibility
    // Task-driven mode often has low observe efficiency (1-2 elements per call)
    // because it navigates step-by-step rather than discovering many elements at once
    this.recorder = new ActionRecorder(
      this.browser,
      this.llmClient,
      {
        maxTurns: this.config.maxTurns!,
        outputDir: this.config.outputDir!,
        onStepFinish: this.config.onStepFinish,
        terminationConfig: {
          // Disable observe efficiency check (set to 0 to disable)
          minObserveEfficiency: 0,
        },
      },
      this.dbWriter || undefined
    )

    // Initialize validator
    this.validator = new SelectorValidator(this.browser)
  }

  /**
   * Initialize browser and logger
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return
    }

    // Use custom log file path if provided, otherwise auto-generate
    const logFile = this.config.logFile
      ? this.fileLogger.initializeWithPath(this.config.logFile)
      : this.fileLogger.initialize('.', 'action-builder')

    // Also initialize global fileLogger for sub-components (ActionRecorder, etc.)
    // This works correctly with maxConcurrency=1 (sequential execution)
    if (this.config.logFile) {
      globalFileLogger.initializeWithPath(this.config.logFile)
    } else {
      // For non-eval usage, global logger auto-generates its own file
      globalFileLogger.initialize('.', 'action-builder')
    }

    this.log('info', `[ActionBuilder] Log file: ${logFile}`)
    this.log(
      'info',
      `[ActionBuilder] Using LLM: ${this.llmClient.getProvider()}/${this.llmClient.getModel()}`
    )
    this.log(
      'info',
      `[ActionBuilder] Output directory: ${this.config.outputDir}`
    )

    await this.browser.initialize()
    this.initialized = true

    this.log('info', '[ActionBuilder] Initialized successfully.')
  }

  /**
   * Instance-specific logging that writes to this ActionBuilder's log file
   */
  private log(
    level: 'info' | 'warn' | 'error' | 'debug',
    ...args: unknown[]
  ): void {
    const timestamp = new Date().toISOString()
    const prefix = `[${timestamp}] [${level.toUpperCase()}]`

    const message =
      prefix +
      ' ' +
      args
        .map((arg) =>
          typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
        )
        .join(' ')

    this.fileLogger.write(message)

    switch (level) {
      case 'error':
        console.error(prefix, ...args)
        break
      case 'warn':
        console.warn(prefix, ...args)
        break
      case 'debug':
        if (process.env.DEBUG) {
          console.log(prefix, ...args)
        }
        break
      default:
        console.log(prefix, ...args)
    }
  }

  /**
   * Build (record) capabilities for a website
   *
   * Supports two modes:
   * 1. Exploration mode (default): Uses observe_page to discover elements
   * 2. Task-driven mode: Execute a task while recording interacted elements
   *    - Set customSystemPrompt and customUserPrompt for task-driven recording
   */
  async build(
    url: string,
    scenario: string,
    options: BuildOptions = {}
  ): Promise<BuildResult> {
    if (!this.initialized) {
      await this.initialize()
    }

    this.log('info', `[ActionBuilder] Building capabilities for: ${url}`)
    this.log('info', `[ActionBuilder] Scenario: ${scenario}`)

    // Support custom prompts for task-driven recording
    const systemPrompt =
      options.customSystemPrompt || CAPABILITY_RECORDER_SYSTEM_PROMPT
    const userMessage =
      options.customUserPrompt ||
      generateUserPrompt(scenario, url, options.focusAreas)

    if (options.customSystemPrompt || options.customUserPrompt) {
      this.log(
        'info',
        `[ActionBuilder] Using custom prompts (task-driven mode)`
      )
    }

    const result = await this.recorder.record(
      scenario,
      systemPrompt,
      userMessage,
      options.siteName,
      options.siteDescription,
      url, // Pass startUrl for recording task
      options.taskId // Pass existing task ID if provided (TaskWorker mode)
    )

    if (result.success && result.siteCapability) {
      this.log(
        'info',
        `[ActionBuilder] Successfully recorded ${
          Object.keys(result.siteCapability.pages).length
        } pages`
      )
    }

    return result
  }

  /**
   * Validate selectors for a recorded site
   */
  async validate(
    domain: string,
    options: ValidateOptions = {}
  ): Promise<ValidationResult> {
    if (!this.initialized) {
      await this.initialize()
    }

    this.log('info', `[ActionBuilder] Validating capabilities for: ${domain}`)

    // Load the site capability
    const capability = this.yamlWriter.load(domain)

    if (!capability) {
      return {
        success: false,
        domain,
        totalElements: 0,
        validElements: 0,
        invalidElements: 0,
        validationRate: 0,
        details: [],
      }
    }

    // Update validator config with options
    if (
      options.pageFilter ||
      options.templateParams ||
      options.verbose !== undefined
    ) {
      this.validator.updateConfig({
        pageFilter: options.pageFilter,
        templateParams: options.templateParams,
        verbose: options.verbose,
      })
    }

    const result = await this.validator.validate(capability)

    this.log(
      'info',
      `[ActionBuilder] Validation complete: ${result.validElements}/${
        result.totalElements
      } valid (${(result.validationRate * 100).toFixed(1)}%)`
    )

    return result
  }

  /**
   * List all recorded sites
   */
  listSites(): string[] {
    return this.yamlWriter.listSites()
  }

  /**
   * Check if a site has been recorded
   */
  siteExists(domain: string): boolean {
    return this.yamlWriter.exists(domain)
  }

  /**
   * Close browser and cleanup resources
   */
  async close(): Promise<void> {
    await this.browser.close()

    // Close database connection if it exists
    if (this.db) {
      await closeDb(this.db)
      this.db = null
      this.dbWriter = null
      this.log('info', '[ActionBuilder] Database connection closed.')
    }

    this.initialized = false
    this.log('info', '[ActionBuilder] Closed.')
    this.fileLogger.close()
    globalFileLogger.close()
  }
}

/**
 * Create an ActionBuilder instance
 */
export function createActionBuilder(
  config: ActionBuilderConfig
): ActionBuilder {
  return new ActionBuilder(config)
}
