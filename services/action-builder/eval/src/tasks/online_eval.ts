/**
 * Online Eval Task
 *
 * Runs ActionBuilder.build() to record a site capability and collects metrics.
 * Used for evaluating ActionBuilder performance with real LLM calls.
 */

import type { EvalInput, EvalOutput, RecallScoreResult } from "../types.js";
import {
  ActionBuilder,
  type ActionBuilderConfig,
  type BuildOptions,
} from "../../../src/index.js";
import { verifyElements } from "../utils/dom_verifier.js";

// Define LLMProvider type locally (includes 'bedrock' which is in AIClient but not exported)
type LLMProvider = 'openrouter' | 'openai' | 'anthropic' | 'bedrock';
import { calculateRobustness, type RobustnessScoreResult } from "../scorers/robustness.js";
import { getLogger } from "../utils/eval_logger.js";

export interface OnlineEvalOptions {
  /** ActionBuilder configuration */
  builderConfig?: Partial<ActionBuilderConfig>;
  /** Build options for the recording */
  buildOptions?: BuildOptions;
  /** LLM provider to use (default: auto-detect with Bedrock priority) */
  llmProvider?: LLMProvider;
  /** Enable robustness validation after build */
  enableRobustness?: boolean;
  /** Environment IDs for robustness testing */
  robustnessEnvIds?: string[];
}

// Global options for online eval
let globalOnlineOptions: OnlineEvalOptions = {
  builderConfig: {
    headless: true,
    maxTurns: 20,
  },
  enableRobustness: true,
  robustnessEnvIds: ["desktop_en"],
};

/**
 * Set global online eval options
 */
export function setOnlineEvalOptions(options: OnlineEvalOptions): void {
  globalOnlineOptions = {
    ...globalOnlineOptions,
    ...options,
    builderConfig: {
      ...globalOnlineOptions.builderConfig,
      ...options.builderConfig,
    },
  };
}

/**
 * Check if LLM API key is available
 * Priority: Bedrock > OpenRouter > OpenAI > Anthropic
 */
function hasLLMApiKey(): boolean {
  const hasBedrock = !!(process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY);
  const hasOpenRouter = !!process.env.OPENROUTER_API_KEY;
  const hasOpenAI = !!process.env.OPENAI_API_KEY;
  const hasAnthropic = !!process.env.ANTHROPIC_API_KEY;

  return hasBedrock || hasOpenRouter || hasOpenAI || hasAnthropic;
}

/**
 * Detect which LLM provider to use
 * Priority: Bedrock > OpenRouter > OpenAI > Anthropic
 */
function detectLLMProvider(): LLMProvider | undefined {
  if (process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY) {
    return "bedrock";
  }
  if (process.env.OPENROUTER_API_KEY) {
    return "openrouter";
  }
  if (process.env.OPENAI_API_KEY) {
    return "openai";
  }
  if (process.env.ANTHROPIC_API_KEY) {
    return "anthropic";
  }
  return undefined;
}

/**
 * Get LLM model from environment based on provider
 */
function getLLMModel(provider: LLMProvider): string | undefined {
  switch (provider) {
    case "bedrock":
      return process.env.AWS_BEDROCK_MODEL;
    case "openrouter":
      return process.env.OPENROUTER_MODEL;
    case "openai":
      return process.env.OPENAI_MODEL;
    case "anthropic":
      return process.env.ANTHROPIC_MODEL;
    default:
      return undefined;
  }
}

/**
 * Get LLM API key from environment based on provider
 */
function getLLMApiKey(provider: LLMProvider): string | undefined {
  switch (provider) {
    case "bedrock":
      return undefined; // Bedrock uses AWS credentials
    case "openrouter":
      return process.env.OPENROUTER_API_KEY;
    case "openai":
      return process.env.OPENAI_API_KEY;
    case "anthropic":
      return process.env.ANTHROPIC_API_KEY;
    default:
      return undefined;
  }
}

/**
 * Online eval task function
 *
 * Runs ActionBuilder to record a site capability, then evaluates it.
 */
export async function onlineEvalTask(input: EvalInput): Promise<EvalOutput> {
  const startTime = Date.now();
  const logger = getLogger(input.caseId);

  logger.section(`Online Eval: ${input.caseId}`);
  logger.info(`URL: ${input.url}`);
  logger.info(`Scenario: ${input.scenario}`);

  // Check for LLM API key
  if (!hasLLMApiKey()) {
    const error = "No LLM API key found. Set AWS credentials for Bedrock, or OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY in .env";
    logger.error(error);
    logger.save();
    return {
      siteCapability: null,
      cost: { tokens: 0, turns: 0, duration: Date.now() - startTime },
      error,
    };
  }

  // Detect LLM provider (Bedrock priority)
  const provider = globalOnlineOptions.llmProvider || detectLLMProvider();
  if (!provider) {
    const error = "Could not detect LLM provider";
    logger.error(error);
    logger.save();
    return {
      siteCapability: null,
      cost: { tokens: 0, turns: 0, duration: Date.now() - startTime },
      error,
    };
  }

  // Get model and API key from environment
  const llmModel = getLLMModel(provider);
  const llmApiKey = getLLMApiKey(provider);

  logger.info(`LLM provider: ${provider}`);
  if (llmModel) {
    logger.info(`LLM model: ${llmModel}`);
  }

  // Log proxy status
  const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;
  if (proxyUrl) {
    logger.info(`Proxy: ${proxyUrl}`);
  }

  // Create ActionBuilder instance with explicit config from env
  const builder = new ActionBuilder({
    headless: true,
    maxTurns: 20,
    llmProvider: provider,
    llmModel,
    llmApiKey,
    logFile: logger.getActionBuilderLogFile(),
    ...globalOnlineOptions.builderConfig,
  });

  logger.info(`ActionBuilder log: ${logger.getActionBuilderLogFile()}`);

  try {
    // Initialize browser
    await builder.initialize();

    // Run build
    logger.info(`Starting ActionBuilder.build()...`);
    const result = await builder.build(
      input.url,
      input.caseId,
      {
        siteName: extractSiteName(input.url),
        siteDescription: input.scenario,
        ...globalOnlineOptions.buildOptions,
      }
    );

    logger.logBuildResults({
      success: result.success,
      tokens: result.tokens.total,
      turns: result.turns,
      duration: result.totalDuration,
      pages: result.siteCapability ? Object.keys(result.siteCapability.pages).length : 0,
      elements: result.siteCapability
        ? Object.values(result.siteCapability.pages).reduce((sum, p) => sum + Object.keys(p.elements).length, 0)
          + Object.keys(result.siteCapability.global_elements).length
        : 0,
      error: result.success ? undefined : result.message,
    });

    if (!result.success || !result.siteCapability) {
      logger.save();
      return {
        siteCapability: null,
        cost: {
          tokens: result.tokens.total,
          turns: result.turns,
          duration: result.totalDuration,
        },
        error: result.message || "ActionBuilder build failed",
      };
    }

    const goldenElements = input.expected.must_have_elements;

    // Pre-compute Recall with DOM verification
    logger.info(`Running DOM verification for ${goldenElements.length} golden elements...`);
    let recallResult: RecallScoreResult;
    try {
      const domResults = await verifyElements(input.url, goldenElements, result.siteCapability, { headless: true });
      recallResult = computeRecallFromDOMResults(goldenElements, domResults);
      logger.info(`Recall: ${recallResult.matched}/${recallResult.total} (${(recallResult.score * 100).toFixed(1)}%)`);
    } catch (error) {
      logger.error(`DOM verification failed: ${error}`);
      recallResult = {
        score: 0,
        matched: 0,
        total: goldenElements.length,
        details: goldenElements.map((g) => ({
          goldenId: g.id,
          matched: false,
          matchMethod: "none" as const,
          error: `DOM verification failed: ${error}`,
        })),
      };
    }

    // Calculate robustness if enabled (only for matched elements)
    let robustnessScore: number | undefined;
    let robustnessResult: RobustnessScoreResult | undefined;
    if (globalOnlineOptions.enableRobustness) {
      logger.info(`Running robustness validation for matched elements...`);
      robustnessResult = await calculateRobustness(
        result.siteCapability,
        input.url,
        {
          envIds: globalOnlineOptions.robustnessEnvIds,
          headless: true,
        },
        goldenElements,  // Pass golden elements for pre_actions and filtering
        recallResult     // Pass recall result to filter to matched elements only
      );
      robustnessScore = robustnessResult.score;
      logger.logRobustnessResults(robustnessResult);
    }

    logger.section("Eval Complete");
    logger.info(`Total duration: ${((Date.now() - startTime) / 1000).toFixed(1)}s`);
    logger.save();

    return {
      siteCapability: result.siteCapability,
      cost: {
        tokens: result.tokens.total,
        turns: result.turns,
        duration: result.totalDuration,
      },
      recallResult,
      robustnessScore,
    };
  } catch (error) {
    logger.error(`Error: ${error instanceof Error ? error.message : String(error)}`);
    logger.save();
    return {
      siteCapability: null,
      cost: {
        tokens: 0,
        turns: 0,
        duration: Date.now() - startTime,
      },
      error: error instanceof Error ? error.message : String(error),
    };
  } finally {
    // Clean up
    await builder.close();
  }
}

/**
 * Convert DOM verification results to RecallScoreResult
 */
function computeRecallFromDOMResults(
  goldenElements: { id: string }[],
  domResults: Map<string, import("../utils/dom_verifier.js").DOMMatchResult>
): RecallScoreResult {
  const details: import("../types.js").ElementMatchResult[] = [];
  let matchedCount = 0;

  for (const golden of goldenElements) {
    const domResult = domResults.get(golden.id);

    if (domResult?.matched) {
      matchedCount++;
      details.push({
        goldenId: golden.id,
        matched: true,
        matchMethod: "dom",
      });
    } else {
      details.push({
        goldenId: golden.id,
        matched: false,
        matchMethod: "none",
        error: domResult?.error || "Element not found in DOM",
      });
    }
  }

  return {
    score: goldenElements.length > 0 ? matchedCount / goldenElements.length : 0,
    matched: matchedCount,
    total: goldenElements.length,
    details,
  };
}

/**
 * Extract site name from URL
 */
function extractSiteName(url: string): string {
  try {
    const hostname = new URL(url).hostname;
    // Remove www. prefix and get domain name
    const domain = hostname.replace(/^www\./, "");
    // Capitalize first letter
    const name = domain.split(".")[0];
    return name.charAt(0).toUpperCase() + name.slice(1);
  } catch {
    return "Unknown";
  }
}
