import { Command } from 'commander'
import { Actionbook, formatSearchResults } from '@actionbookdev/sdk'
import chalk from 'chalk'
import { getApiKey, handleError, outputResult } from '../output.js'
import type { SearchType } from '@actionbookdev/sdk'

export const searchCommand = new Command('search')
  .alias('s')
  .description('Search for action manuals by keyword')
  .argument('<query>', 'Search keyword (e.g., "airbnb search", "google login")')
  .option('-t, --type <type>', 'Search type: vector, fulltext, or hybrid', 'hybrid')
  .option('-l, --limit <number>', 'Maximum results (1-100)', '5')
  .option('-s, --source-ids <ids>', 'Filter by source IDs (comma-separated)')
  .option('--min-score <score>', 'Minimum similarity score (0-1)')
  .option('-j, --json', 'Output raw JSON')
  .action(async (query: string, options) => {
    try {
      const apiKey = getApiKey(options)
      const client = new Actionbook({ apiKey })

      const result = await client.searchActions({
        query,
        type: options.type as SearchType,
        limit: parseInt(options.limit, 10),
        sourceIds: options.sourceIds,
        minScore: options.minScore ? parseFloat(options.minScore) : undefined,
      })

      if (options.json) {
        outputResult(result)
      } else {
        // Formatted output
        if (result.results.length === 0) {
          console.log(chalk.yellow(`No actions found for "${query}"`))
          console.log(chalk.dim('Try broader search terms or different search type'))
        } else {
          console.log(chalk.bold.cyan(`\nSearch Results for "${query}"\n`))
          console.log(chalk.dim(`Found ${result.count} result(s)\n`))

          result.results.forEach((action, index) => {
            const num = index + 1
            console.log(chalk.bold.white(`${num}. ${action.action_id}`))
            console.log(chalk.dim(`   Score: ${(action.score ?? 0).toFixed(3)}`))
            console.log(chalk.gray(`   ${truncate(action.content, 120)}`))
            console.log()
          })

          if (result.hasMore) {
            console.log(chalk.dim('More results available. Increase --limit to see more.\n'))
          }

          console.log(chalk.cyan('Next step: ') + chalk.white(`actionbook get "<action_id>"`))
        }
      }
    } catch (error) {
      handleError(error)
    }
  })

function truncate(str: string, maxLen: number): string {
  const cleaned = str.replace(/\n/g, ' ').trim()
  if (cleaned.length <= maxLen) return cleaned
  return cleaned.substring(0, maxLen) + '...'
}
