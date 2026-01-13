#!/usr/bin/env node

import { Command } from 'commander'
import { searchCommand } from './commands/search.js'
import { getCommand } from './commands/get.js'
import { sourcesCommand } from './commands/sources.js'

const program = new Command()

program
  .name('actionbook')
  .description('CLI for Actionbook - Get website action manuals for AI agents')
  .version('0.1.0')
  .option('--api-key <key>', 'API key (or set ACTIONBOOK_API_KEY env var)')

program.addCommand(searchCommand)
program.addCommand(getCommand)
program.addCommand(sourcesCommand)

program.parse()
