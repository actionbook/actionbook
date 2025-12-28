import { Command } from 'commander'
import chalk from 'chalk'
import { ActionBuilder } from '../../src/ActionBuilder.js'

export const recordCommand = new Command('record')
  .description('Record website UI element capabilities')
  .argument('<url>', 'Target website URL')
  .requiredOption('-s, --scenario <name>', 'Scenario name')
  .option('-n, --name <name>', 'Site name')
  .option('-d, --description <desc>', 'Site description')
  .option('-o, --output <dir>', 'Output directory', './output')
  .option(
    '-f, --focus <areas...>',
    'Focus areas (can be specified multiple times)'
  )
  .option('-m, --max-turns <n>', 'Maximum interaction turns', '50')
  .option('--headless', 'Run in headless mode', false)
  .option('-v, --verbose', 'Verbose output', false)
  .action(async (url: string, options) => {
    // Check for at least one LLM API key (including AWS Bedrock)
    const hasBedrock =
      process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY
    const hasApiKey =
      process.env.OPENROUTER_API_KEY ||
      process.env.OPENAI_API_KEY ||
      process.env.ANTHROPIC_API_KEY ||
      hasBedrock

    if (!hasApiKey) {
      console.error(chalk.red('Error: No LLM API key found.'))
      console.error(
        chalk.yellow(
          'Set one of: OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY, or AWS credentials for Bedrock'
        )
      )
      process.exit(3)
    }

    console.log(chalk.blue('Actionbook Builder - Record'))
    console.log(chalk.gray('='.repeat(50)))
    console.log(`URL: ${chalk.cyan(url)}`)
    console.log(`Scenario: ${chalk.cyan(options.scenario)}`)
    console.log(`Output: ${chalk.cyan(options.output)}`)
    if (options.focus) {
      console.log(`Focus: ${chalk.cyan(options.focus.join(', '))}`)
    }
    console.log(`Headless: ${chalk.cyan(options.headless)}`)
    console.log(chalk.gray('='.repeat(50)))
    console.log()

    const databaseUrl = process.env.DATABASE_URL
    if (databaseUrl) {
      console.log(`Database: ${chalk.cyan('enabled (dual-write)')}`)
    }

    // AIClient auto-detects provider from environment variables
    const builder = new ActionBuilder({
      outputDir: options.output,
      maxTurns: parseInt(options.maxTurns, 10),
      headless: options.headless,
      databaseUrl,
    })

    try {
      await builder.initialize()

      const result = await builder.build(url, options.scenario, {
        siteName: options.name,
        siteDescription: options.description,
        focusAreas: options.focus,
      })

      console.log()

      if (result.success) {
        console.log(chalk.green('✅ Capability recording completed!'))
        console.log(`📁 Saved to: ${chalk.cyan(result.savedPath)}`)

        if (result.siteCapability) {
          const elementCount =
            Object.values(result.siteCapability.pages).reduce(
              (sum, page) => sum + Object.keys(page.elements).length,
              0
            ) + Object.keys(result.siteCapability.global_elements).length

          console.log(`🔍 Elements discovered: ${chalk.cyan(elementCount)}`)
          console.log(
            `📄 Pages: ${chalk.cyan(
              Object.keys(result.siteCapability.pages).length
            )}`
          )
        }

        console.log(`🔄 Turns used: ${chalk.cyan(result.turns)}`)
        console.log(
          `💰 Tokens: in=${chalk.cyan(
            result.tokens.input.toLocaleString()
          )}, out=${chalk.cyan(
            result.tokens.output.toLocaleString()
          )}, total=${chalk.cyan(result.tokens.total.toLocaleString())}`
        )
        console.log(
          `⏱️ Duration: ${chalk.cyan(
            (result.totalDuration / 1000).toFixed(1)
          )}s`
        )

        // Show warning if database save failed
        if (result.dbSaveError) {
          console.log()
          console.log(
            chalk.yellow(`⚠️  Database save failed: ${result.dbSaveError}`)
          )
          console.log(
            chalk.yellow(
              '   YAML file was saved successfully, but database was not updated.'
            )
          )
        }

        process.exit(0)
      } else {
        console.error(chalk.red(`❌ Recording failed: ${result.message}`))
        process.exit(6)
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      console.error(chalk.red(`❌ Error: ${message}`))
      process.exit(1)
    } finally {
      await builder.close()
    }
  })
