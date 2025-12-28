import { Command } from 'commander'
import chalk from 'chalk'
import { ActionBuilder } from '../../src/ActionBuilder.js'

export const validateCommand = new Command('validate')
  .description('Validate selector effectiveness for a recorded site')
  .argument('<domain>', 'Site domain (e.g., www.airbnb.com)')
  .option('-i, --input <dir>', 'Capability store directory', './output')
  .option('--headless', 'Run in headless mode', false)
  .option('-v, --verbose', 'Verbose output', false)
  .option(
    '-p, --page <pageType>',
    'Filter to specific page type (e.g., home, search)'
  )
  .option('--live', 'Show detailed live validation for each selector')
  .option(
    '-t, --template <params>',
    'Template parameters as JSON (e.g., \'{"date":"2025-12-10"}\')'
  )
  .action(async (domain: string, options) => {
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

    console.log(chalk.blue('Actionbook Builder - Validate'))
    console.log(chalk.gray('='.repeat(50)))
    console.log(`Domain: ${chalk.cyan(domain)}`)
    console.log(`Input: ${chalk.cyan(options.input)}`)
    console.log(`Headless: ${chalk.cyan(options.headless)}`)
    if (options.page) {
      console.log(`Page filter: ${chalk.cyan(options.page)}`)
    }
    if (options.live) {
      console.log(`Mode: ${chalk.cyan('Live (detailed)')}`)
    }
    if (options.template) {
      console.log(`Template params: ${chalk.cyan(options.template)}`)
    }
    console.log(chalk.gray('='.repeat(50)))
    console.log()

    // Parse template parameters if provided
    let templateParams: Record<string, string> | undefined
    if (options.template) {
      try {
        templateParams = JSON.parse(options.template)
      } catch (e) {
        console.error(
          chalk.red(`❌ Invalid JSON for --template: ${options.template}`)
        )
        process.exit(2)
      }
    }

    // AIClient auto-detects provider from environment variables
    const builder = new ActionBuilder({
      outputDir: options.input,
      headless: options.headless,
    })

    try {
      // Check if site exists
      if (!builder.siteExists(domain)) {
        console.error(chalk.red(`❌ Site not found: ${domain}`))
        console.error(
          chalk.yellow(
            `Run 'actionbook-builder record' first to record the site capabilities.`
          )
        )
        process.exit(7)
      }

      await builder.initialize()

      const result = await builder.validate(domain, {
        pageFilter: options.page ? [options.page] : undefined,
        templateParams,
        verbose: options.verbose || options.live,
      })

      console.log()
      console.log(chalk.gray('='.repeat(50)))
      console.log(chalk.blue('Validation Summary'))
      console.log(chalk.gray('='.repeat(50)))
      console.log(`Domain: ${chalk.cyan(result.domain)}`)
      console.log(`Total elements: ${chalk.cyan(result.totalElements)}`)
      console.log(`Valid elements: ${chalk.green(result.validElements)}`)
      console.log(`Invalid elements: ${chalk.red(result.invalidElements)}`)
      console.log(
        `Validation rate: ${
          result.validationRate >= 0.8
            ? chalk.green((result.validationRate * 100).toFixed(1) + '%')
            : chalk.red((result.validationRate * 100).toFixed(1) + '%')
        }`
      )
      console.log(chalk.gray('='.repeat(50)))

      // Show detailed selector results when --live flag is set
      if (options.live) {
        console.log()
        console.log(chalk.blue('Detailed Selector Results:'))
        for (const detail of result.details) {
          const status = detail.valid ? chalk.green('✓') : chalk.red('✗')
          console.log(
            `\n${status} ${chalk.cyan(detail.pageType)}/${chalk.bold(
              detail.elementId
            )}`
          )

          // Show new selectors array details
          if (detail.selectorsDetail && detail.selectorsDetail.length > 0) {
            for (const sel of detail.selectorsDetail) {
              const selStatus = sel.valid ? chalk.green('✓') : chalk.red('✗')
              const templateMark = sel.isTemplate
                ? chalk.yellow(' [template]')
                : ''
              console.log(
                `    ${selStatus} ${chalk.gray(sel.type)}: ${
                  sel.value
                }${templateMark}`
              )
              if (!sel.valid && sel.error) {
                console.log(`       ${chalk.red('→ ' + sel.error)}`)
              }
            }
          } else {
            // Fallback to legacy format
            if (detail.selector.css) {
              const cssStatus = detail.selector.css.valid
                ? chalk.green('✓')
                : chalk.red('✗')
              console.log(
                `    ${cssStatus} css: ${
                  detail.selector.css.valid
                    ? 'valid'
                    : detail.selector.css.error
                }`
              )
            }
            if (detail.selector.xpath) {
              const xpathStatus = detail.selector.xpath.valid
                ? chalk.green('✓')
                : chalk.red('✗')
              console.log(
                `    ${xpathStatus} xpath: ${
                  detail.selector.xpath.valid
                    ? 'valid'
                    : detail.selector.xpath.error
                }`
              )
            }
          }
        }
      } else {
        // Show invalid elements only (default behavior)
        const invalidDetails = result.details.filter((d) => !d.valid)
        if (invalidDetails.length > 0) {
          console.log()
          console.log(chalk.yellow('Invalid elements:'))
          for (const detail of invalidDetails) {
            console.log(
              `  - ${chalk.cyan(detail.pageType)}/${chalk.cyan(
                detail.elementId
              )}`
            )

            // Show new selectors array details if available
            if (detail.selectorsDetail && detail.selectorsDetail.length > 0) {
              const invalidSelectors = detail.selectorsDetail.filter(
                (s) => !s.valid
              )
              for (const sel of invalidSelectors) {
                console.log(
                  `    ${chalk.gray(sel.type)}: ${chalk.red(
                    sel.error || 'Element not found'
                  )}`
                )
              }
            } else {
              // Fallback to legacy format
              if (detail.selector.css && !detail.selector.css.valid) {
                console.log(
                  `    CSS: ${chalk.red(detail.selector.css.error || 'N/A')}`
                )
              }
              if (detail.selector.xpath && !detail.selector.xpath.valid) {
                console.log(
                  `    XPath: ${chalk.red(
                    detail.selector.xpath.error || 'N/A'
                  )}`
                )
              }
            }
          }
        }
      }

      if (result.success) {
        console.log()
        console.log(chalk.green('✅ Validation passed!'))
        process.exit(0)
      } else {
        console.log()
        console.log(
          chalk.yellow(
            '⚠️ Validation rate below 80%, some selectors may be stale.'
          )
        )
        process.exit(7)
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      console.error(chalk.red(`❌ Error: ${message}`))
      process.exit(1)
    } finally {
      await builder.close()
    }
  })
