#!/usr/bin/env node

import { Command } from 'commander'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { searchCommand } from './commands/search.js'
import { getCommand } from './commands/get.js'
import { sourcesCommand } from './commands/sources.js'
import { browserCommand } from './commands/browser.js'

const __dirname = dirname(fileURLToPath(import.meta.url))
const pkg = JSON.parse(readFileSync(join(__dirname, '..', 'package.json'), 'utf-8'))

const program = new Command()

program
  .name('actionbook')
  .description('CLI for Actionbook - Get website action manuals for AI agents')
  .version(pkg.version)
  .option('--api-key <key>', 'API key (or set ACTIONBOOK_API_KEY env var)')

program.addCommand(searchCommand)
program.addCommand(getCommand)
program.addCommand(sourcesCommand)
program.addCommand(browserCommand)

program.parse()
