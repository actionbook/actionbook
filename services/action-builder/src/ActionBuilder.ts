import { createBrowserAuto } from '@actionbookdev/browser'
import type { BrowserAdapter } from '@actionbookdev/browser'
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
  private loggerInitialized: boolean = false
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
      buildTimeoutMs: config.buildTimeoutMs,
      browserRetryConfig: config.browserRetryConfig,
    }

    // Create instance-specific file logger for parallel execution support
    this.fileLogger = new FileLogger()

    // Initialize browser using factory function (auto-detects environment)
    // Uses AgentCoreBrowser in AWS AgentCore Runtime, otherwise StagehandBrowser
    this.browser = createBrowserAuto({
      headless: this.config.headless,
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
    if (!this.loggerInitialized) {
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

      this.loggerInitialized = true

      this.log('info', `[ActionBuilder] Log file: ${logFile}`)
      this.log(
        'info',
        `[ActionBuilder] Using LLM: ${this.llmClient.getProvider()}/${this.llmClient.getModel()}`
      )
      this.log(
        'info',
        `[ActionBuilder] Output directory: ${this.config.outputDir}`
      )
    }

    if (this.initialized) {
      return
    }

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

    // Always write to file
    this.fileLogger.write(message)

    // In quiet mode, suppress ActionBuilder info logs (only show errors/warnings)
    const quietMode = process.env.ACTION_BUILDER_QUIET === 'true'
    const shouldOutput =
      level === 'error' ||
      level === 'warn' ||
      !quietMode ||
      (level === 'debug' && process.env.DEBUG)

    if (!shouldOutput) {
      return
    }

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
   * Use LLM to extract the first example URL from scenarioDescription
   * Extracts from "Pages:" section. Falls back to original URL if extraction fails.
   */
  private async extractStartUrl(
    url: string,
    scenarioDescription?: string
  ): Promise<string> {
    // If no scenario description, use original URL
    if (!scenarioDescription) {
      return url;
    }

    try {
      const systemMessage = 'You are a URL extractor. Extract the FIRST URL from the "Pages:" or "pages:" section. Return ONLY the URL, no explanations.';
      const userMessage = `Given the following scenario description, extract the FIRST URL from the "Pages:" section.
If there are no pages listed, return the original URL.

Original URL: ${url}

Scenario Description:
${scenarioDescription}

Return ONLY the URL, nothing else.`;

      const response = await this.llmClient.chat(
        [
          { role: 'system', content: systemMessage },
          { role: 'user', content: userMessage }
        ],
        [] // No tools needed
      );

      const extractedUrl = response.choices[0]?.message?.content?.trim() || '';

      // Basic validation - check if it's a valid HTTP(S) URL
      if (extractedUrl && (extractedUrl.startsWith('http://') || extractedUrl.startsWith('https://'))) {
        if (extractedUrl !== url) {
          this.log('info', `[ActionBuilder] LLM extracted start URL: ${extractedUrl} (from pattern: ${url})`);
        }
        return extractedUrl;
      }

      // Fallback to original URL if LLM extraction failed
      this.log('warn', `[ActionBuilder] LLM failed to extract valid URL, using original: ${url}`);
      return url;
    } catch (error) {
      const errMsg = error instanceof Error ? error.message : String(error);
      this.log('error', `[ActionBuilder] Error extracting URL with LLM: ${errMsg}`);
      return url;
    }
  }

  /**
   * Check if an error is retryable (browser/connection errors)
   */
  private isRetryableError(error: unknown): boolean {
    const errorMsg = error instanceof Error ? error.message : String(error);
    const defaultPatterns = [
      'ECONNREFUSED',
      'Target closed',
      'Browser closed',
      'Connection closed',
      'Protocol error',
      'Session closed',
      'ECONNRESET',
      'socket hang up',
    ];

    const patterns = this.config.browserRetryConfig?.retryableErrors ?? defaultPatterns;
    return patterns.some(pattern => errorMsg.includes(pattern));
  }

  /**
   * Check if error is a timeout
   */
  private hasTimeout(error: unknown): boolean {
    const errorMsg = error instanceof Error ? error.message : String(error);
    return errorMsg.includes('timeout');
  }

  /**
   * Execute function with timeout protection
   */
  private async executeWithTimeout<T>(
    fn: () => Promise<T>,
    timeoutMs: number
  ): Promise<T> {
    let timeoutId: NodeJS.Timeout;

    const timeoutPromise = new Promise<never>((_, reject) => {
      timeoutId = setTimeout(() => {
        reject(new Error(`Build timeout after ${timeoutMs / 1000 / 60} minutes`));
      }, timeoutMs);
    });

    try {
      const result = await Promise.race([fn(), timeoutPromise]);
      clearTimeout(timeoutId!);
      return result;
    } catch (error) {
      clearTimeout(timeoutId!);
      throw error;
    }
  }

  /**
   * Handle timeout by attempting to save partial results
   */
  private async handleTimeout(timeoutMs: number): Promise<BuildResult> {
    this.log('warn', '[ActionBuilder] Build timeout - attempting partial save...');

    const partialResult = await this.savePartialResult();

    if (partialResult && partialResult.elements > 0) {
      this.log('info', `[ActionBuilder] Saved ${partialResult.elements} elements despite timeout`);

      return {
        success: true,
        message: `Build timeout after saving ${partialResult.elements} elements`,
        turns: partialResult.turns,
        totalDuration: timeoutMs,
        tokens: partialResult.tokens,
        siteCapability: partialResult.siteCapability,
        partialResult: true,
      };
    }

    throw new Error('Build timeout with no elements discovered');
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
    const maxRetries = this.config.browserRetryConfig?.maxAttempts ?? 3;
    const baseDelay = this.config.browserRetryConfig?.baseDelayMs ?? 2000;
    const buildTimeout = this.config.buildTimeoutMs ?? 10 * 60 * 1000;

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        // Wrap with timeout
        const result = await this.executeWithTimeout(
          () => this.buildInternal(url, scenario, options),
          buildTimeout
        );

        if (attempt > 1) {
          this.log('info', `[ActionBuilder] Build succeeded on attempt ${attempt}/${maxRetries}`);
        }

        return result;

      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        const isRetryable = this.isRetryableError(error);

        // Handle timeout specially - try to save partial results
        if (this.hasTimeout(error)) {
          return await this.handleTimeout(buildTimeout);
        }

        // Non-retryable or last attempt
        if (!isRetryable || attempt === maxRetries) {
          this.log('error', `[ActionBuilder] Build failed: ${errorMessage}`);
          throw error;
        }

        // Retryable error - wait and retry
        const delay = baseDelay * attempt;
        this.log('warn', `[ActionBuilder] Build failed on attempt ${attempt}/${maxRetries}: ${errorMessage}`);
        this.log('info', `[ActionBuilder] Retrying in ${delay}ms...`);

        await new Promise(resolve => setTimeout(resolve, delay));

        // Close browser only (keep DB/Logger for retry)
        await this.closeBrowserOnly();
      }
    }

    // Should never reach here
    throw new Error('Unexpected: retry loop completed without result');
  }

  /**
   * Internal build logic (called by build() with retry wrapper)
   */
  private async buildInternal(
    url: string,
    scenario: string,
    options: BuildOptions
  ): Promise<BuildResult> {
    if (!this.initialized) {
      await this.initialize()
    }

    this.log('info', `[ActionBuilder] Building capabilities for: ${url}`)
    this.log('info', `[ActionBuilder] Scenario: ${scenario}`)

    // Use LLM to extract the first example URL from scenarioDescription if available
    const actualUrl = await this.extractStartUrl(url, options.scenarioDescription);

    // Support custom prompts for task-driven recording
    const systemPrompt =
      options.customSystemPrompt || CAPABILITY_RECORDER_SYSTEM_PROMPT
    const userMessage =
      options.customUserPrompt ||
      generateUserPrompt(scenario, actualUrl, {
        scenarioDescription: options.scenarioDescription,
        focusAreas: options.focusAreas,
      })

    if (options.customSystemPrompt || options.customUserPrompt) {
      this.log(
        'info',
        `[ActionBuilder] Using custom prompts (task-driven mode)`
      )
    }

    // Pass playbook options to recorder (targetUrlPattern, autoScrollToBottom)
    if (options.targetUrlPattern !== undefined || options.autoScrollToBottom !== undefined) {
      this.recorder.updatePlaybookConfig({
        targetUrlPattern: options.targetUrlPattern,
        autoScrollToBottom: options.autoScrollToBottom,
      })
    }

    const result = await this.recorder.record(
      scenario,
      systemPrompt,
      userMessage,
      options.siteName,
      options.siteDescription,
      actualUrl, // Pass resolved startUrl (pattern -> example URL)
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
   * Save partial result when timeout occurs
   * Delegates to recorder to save discovered elements
   *
   * @returns Object with element count, siteCapability, and statistics, or null if nothing to save
   */
  async savePartialResult(): Promise<{
    elements: number;
    siteCapability: any;
    turns: number;
    steps: number;
    tokens: {
      input: number;
      output: number;
      total: number;
      planning: { input: number; output: number };
      browser: { input: number; output: number };
    };
  } | null> {
    if (!this.recorder) {
      this.log('warn', '[ActionBuilder] No recorder available for partial save')
      return null
    }

    try {
      this.log('info', '[ActionBuilder] Attempting to save partial result...')
      const result = await this.recorder.savePartialResult()

      if (result && result.elements > 0) {
        this.log(
          'info',
          `[ActionBuilder] Successfully saved ${result.elements} elements as partial result (turns: ${result.turns}, tokens: ${result.tokens.total})`
        )
      } else {
        this.log('warn', '[ActionBuilder] No elements to save in partial result')
      }

      return result
    } catch (error) {
      const errMsg = error instanceof Error ? error.message : String(error)
      this.log('error', `[ActionBuilder] Failed to save partial result: ${errMsg}`)
      return null
    }
  }

  /**
   * Close only browser resources (for retry scenarios)
   * Does not close DB/Logger to allow retry attempts to continue
   */
  private async closeBrowserOnly(): Promise<void> {
    await this.browser.close()
    this.initialized = false
    this.log('info', '[ActionBuilder] Browser closed for retry.')
  }

  /**
   * Close browser and cleanup all resources (DB, Logger)
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
    this.loggerInitialized = false
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
