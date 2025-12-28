/**
 * Retry options for async operations
 */
export interface RetryOptions {
  maxAttempts: number;
  baseDelayMs: number;
  maxDelayMs: number;
  backoffMultiplier: number;
}

const DEFAULT_RETRY_OPTIONS: RetryOptions = {
  maxAttempts: 3,
  baseDelayMs: 1000,
  maxDelayMs: 10000,
  backoffMultiplier: 2,
};

/**
 * Sleep for a given number of milliseconds
 */
export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Add a human-like delay between actions
 */
export async function humanDelay(
  minMs: number = 1000,
  maxMs: number = 3000
): Promise<void> {
  const delay = minMs + Math.random() * (maxMs - minMs);
  await sleep(delay);
}

/**
 * Execute an async operation with retry logic
 */
export async function withRetry<T>(
  operation: () => Promise<T>,
  options: Partial<RetryOptions> = {},
  operationName: string = "operation"
): Promise<T> {
  const opts = { ...DEFAULT_RETRY_OPTIONS, ...options };
  let lastError: Error | null = null;

  for (let attempt = 1; attempt <= opts.maxAttempts; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error instanceof Error ? error : new Error(String(error));

      if (attempt === opts.maxAttempts) {
        console.error(
          `[${operationName}] Failed after ${attempt} attempts: ${lastError.message}`
        );
        throw lastError;
      }

      const delay = Math.min(
        opts.baseDelayMs * Math.pow(opts.backoffMultiplier, attempt - 1),
        opts.maxDelayMs
      );

      console.log(
        `[${operationName}] Attempt ${attempt} failed, retrying in ${delay}ms...`
      );
      await sleep(delay);
    }
  }

  throw lastError;
}
