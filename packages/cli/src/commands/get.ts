import { Command } from 'commander'
import { Actionbook, formatActionDetail } from '@actionbookdev/sdk'
import chalk from 'chalk'
import { getApiKey, handleError, outputResult } from '../output.js'

export const getCommand = new Command('get')
  .alias('g')
  .description('Get complete action details by action ID')
  .argument('<id>', 'Action ID (URL or domain, e.g., "https://example.com/page" or "example.com/page")')
  .option('-j, --json', 'Output raw JSON')
  .action(async (id: string, options) => {
    try {
      const apiKey = getApiKey(options)
      const client = new Actionbook({ apiKey })

      const result = await client.getActionById(id)

      if (options.json) {
        outputResult(result)
      } else {
        // Formatted output
        console.log(chalk.bold.cyan(`\n${result.heading || result.documentTitle}\n`))

        console.log(chalk.bold('Metadata'))
        console.log(chalk.dim('─'.repeat(50)))
        console.log(`${chalk.gray('Action ID:')}    ${result.action_id}`)
        console.log(`${chalk.gray('Document:')}     ${result.documentTitle}`)
        console.log(`${chalk.gray('URL:')}          ${result.documentUrl}`)
        console.log(`${chalk.gray('Chunk Index:')}  ${result.chunkIndex}`)
        console.log(`${chalk.gray('Token Count:')}  ${result.tokenCount}`)
        console.log()

        console.log(chalk.bold('Content'))
        console.log(chalk.dim('─'.repeat(50)))
        console.log(result.content)
        console.log()

        if (result.elements) {
          try {
            const elements = JSON.parse(result.elements)
            console.log(chalk.bold('UI Elements'))
            console.log(chalk.dim('─'.repeat(50)))

            for (const [name, el] of Object.entries(elements as Record<string, any>)) {
              console.log(chalk.yellow(`  ${name}:`))
              if (el.css_selector) {
                console.log(`    ${chalk.gray('CSS:')} ${el.css_selector}`)
              }
              if (el.xpath_selector) {
                console.log(`    ${chalk.gray('XPath:')} ${el.xpath_selector}`)
              }
              if (el.description) {
                console.log(`    ${chalk.gray('Description:')} ${el.description}`)
              }
              if (el.element_type) {
                console.log(`    ${chalk.gray('Type:')} ${el.element_type}`)
              }
              if (el.allow_methods?.length) {
                console.log(`    ${chalk.gray('Methods:')} ${el.allow_methods.join(', ')}`)
              }
            }
            console.log()
          } catch {
            console.log(chalk.dim('(Failed to parse elements data)'))
          }
        }
      }
    } catch (error) {
      handleError(error)
    }
  })
