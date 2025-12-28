/**
 * Knowledge Builder - Public API
 *
 * This is the main entry point for the knowledge-builder package.
 * Re-exports everything from the builder layer for convenience.
 *
 * Usage:
 * ```typescript
 * import { createProcessor } from '@actionbookdev/knowledge-builder';
 *
 * const processor = createProcessor();
 * await processor.process({
 *   sourceName: 'my-docs',
 *   baseUrl: 'https://docs.example.com',
 *   crawlConfig: { maxDepth: 3 },
 * });
 * ```
 */

// Re-export everything from builder layer
export * from './builder/index.js'

// Re-export controller layer
export * from './controller/index.js'
