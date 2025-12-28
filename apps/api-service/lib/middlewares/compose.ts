import { NextRequest } from 'next/server';

/**
 * Handler type with any additional args (for route context)
 */
export type Handler = (request: NextRequest, ...args: any[]) => Promise<Response>;

/**
 * Middleware type
 */
export type Middleware = (handler: Handler) => Handler;

/**
 * Composes multiple middleware functions into a single middleware
 * Middlewares are applied from left to right
 *
 * @example
 * ```ts
 * const handler = compose(withRateLimit, withLogging)(myHandler);
 * export const GET = handler;
 * ```
 */
export function compose(...middlewares: Middleware[]) {
  return (handler: Handler): Handler => {
    return middlewares.reduceRight(
      (next, middleware) => middleware(next),
      handler
    );
  };
}
