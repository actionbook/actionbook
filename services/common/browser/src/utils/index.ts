/**
 * Browser Utilities
 */

export { log, setLogger, resetLogger, type LogLevel, type LogFunction } from './logger.js';

export {
  filterStateDataAttributes,
  createIdSelector,
  generateOptimizedXPath,
  filterCssClasses,
} from './selector-utils.js';
