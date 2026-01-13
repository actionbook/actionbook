import chalk from 'chalk'
import { isActionbookError } from '@actionbookdev/sdk'

/**
 * Get API key from options or environment
 */
export function getApiKey(options: { apiKey?: string }): string | undefined {
  return options.apiKey ?? process.env.ACTIONBOOK_API_KEY
}

/**
 * Output result as JSON
 */
export function outputResult(data: unknown): void {
  console.log(JSON.stringify(data, null, 2))
}

/**
 * Handle and display errors
 */
export function handleError(error: unknown): void {
  if (isActionbookError(error)) {
    console.error(chalk.red(`\nError: ${error.code}`))
    console.error(chalk.white(error.message))
    if (error.suggestion) {
      console.error(chalk.yellow(`\nSuggestion: ${error.suggestion}`))
    }
  } else if (error instanceof Error) {
    console.error(chalk.red(`\nError: ${error.message}`))
  } else {
    console.error(chalk.red('\nUnknown error occurred'))
  }
  process.exit(1)
}
