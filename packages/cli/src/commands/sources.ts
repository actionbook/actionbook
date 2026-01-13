import { Command } from 'commander'
import { Actionbook } from '@actionbookdev/sdk'
import chalk from 'chalk'
import { getApiKey, handleError, outputResult } from '../output.js'
import type { SourceItem } from '@actionbookdev/sdk'

export const sourcesCommand = new Command('sources')
  .description('List or search available sources (websites)')
  .option('-l, --limit <number>', 'Maximum results', '50')
  .option('-j, --json', 'Output raw JSON')
  .action(async (options) => {
    try {
      const apiKey = getApiKey(options)
      const client = new Actionbook({ apiKey })

      const result = await client.listSources({
        limit: parseInt(options.limit, 10),
      })

      if (options.json) {
        outputResult(result)
      } else {
        formatSourceList(result.results, result.count)
      }
    } catch (error) {
      handleError(error)
    }
  })

// Add search subcommand
sourcesCommand
  .command('search <query>')
  .alias('s')
  .description('Search for sources by keyword')
  .option('-l, --limit <number>', 'Maximum results', '10')
  .option('-j, --json', 'Output raw JSON')
  .action(async (query: string, options) => {
    try {
      // Get parent command options
      const parentOpts = sourcesCommand.opts()
      const apiKey = getApiKey({ ...parentOpts, ...options })
      const client = new Actionbook({ apiKey })

      const result = await client.searchSources({
        query,
        limit: parseInt(options.limit, 10),
      })

      if (options.json) {
        outputResult(result)
      } else {
        if (result.results.length === 0) {
          console.log(chalk.yellow(`\nNo sources found for "${query}"`))
        } else {
          console.log(chalk.bold.cyan(`\nSources matching "${query}"\n`))
          formatSourceList(result.results, result.count)
        }
      }
    } catch (error) {
      handleError(error)
    }
  })

function formatSourceList(sources: SourceItem[], count: number): void {
  if (sources.length === 0) {
    console.log(chalk.yellow('No sources found'))
    return
  }

  console.log(chalk.dim(`${count} source(s)\n`))

  // Calculate column widths
  const maxIdLen = Math.max(...sources.map((s) => String(s.id).length), 2)
  const maxNameLen = Math.min(Math.max(...sources.map((s) => s.name.length), 4), 30)

  // Header
  console.log(
    chalk.bold(
      `${'ID'.padEnd(maxIdLen)}  ${'Name'.padEnd(maxNameLen)}  ${'URL'}`
    )
  )
  console.log(chalk.dim('─'.repeat(80)))

  // Rows
  for (const source of sources) {
    const id = String(source.id).padEnd(maxIdLen)
    const name = truncate(source.name, maxNameLen).padEnd(maxNameLen)
    const url = source.baseUrl

    console.log(`${chalk.yellow(id)}  ${chalk.white(name)}  ${chalk.cyan(url)}`)

    if (source.description) {
      console.log(chalk.dim(`${''.padEnd(maxIdLen)}  ${truncate(source.description, 70)}`))
    }

    if (source.tags?.length) {
      console.log(
        chalk.dim(`${''.padEnd(maxIdLen)}  Tags: `) +
          source.tags.map((t) => chalk.magenta(t)).join(', ')
      )
    }
  }

  console.log()
  console.log(
    chalk.cyan('Tip: ') +
      chalk.white('Use --source-ids with search to filter by source')
  )
  console.log(
    chalk.dim('Example: actionbook search "login" --source-ids 1,2')
  )
}

function truncate(str: string, maxLen: number): string {
  if (str.length <= maxLen) return str
  return str.substring(0, maxLen - 1) + '…'
}
