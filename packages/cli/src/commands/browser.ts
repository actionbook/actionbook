import { Command } from 'commander'
import chalk from 'chalk'
import { spawnCommand } from '../utils/process.js'

export const browserCommand = new Command('browser')
  .description('Execute agent-browser commands (browser automation)')
  .allowUnknownOption(true) // Critical: allow any args through
  .allowExcessArguments(true)
  .helpOption('-h, --help', 'Display help for actionbook browser')
  .addHelpText(
    'after',
    `
Examples:
  $ actionbook browser open example.com
  $ actionbook browser snapshot -i
  $ actionbook browser click @e1
  $ actionbook browser fill @e3 "test@example.com"

For detailed agent-browser commands:
  $ agent-browser --help

Learn more: ${chalk.cyan('https://github.com/vercel-labs/agent-browser')}
  `
  )
  .action(async (_options, command) => {
    // Get all arguments passed after 'browser'
    const args = command.args

    // If no args and user didn't ask for help, show agent-browser help
    if (args.length === 0) {
      console.log(chalk.yellow('No arguments provided. Showing agent-browser help:\n'))
      const exitCode = await spawnCommand('agent-browser', ['--help'])
      process.exit(exitCode)
      return
    }

    // Execute agent-browser with all args
    const exitCode = await spawnCommand('agent-browser', args)
    process.exit(exitCode)
  })
