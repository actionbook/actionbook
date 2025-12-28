#!/usr/bin/env node

import { Command } from 'commander'
import { recordCommand } from './commands/record.js'
import { validateCommand } from './commands/validate.js'

const program = new Command()

program
  .name('actionbook-builder')
  .description(
    'Actionbook Capability Builder - Record and validate website UI capabilities'
  )
  .version('0.1.0')

program.addCommand(recordCommand)
program.addCommand(validateCommand)

// List command (simple, no browser needed)
program
  .command('list')
  .description('List all recorded sites')
  .option('-i, --input <dir>', 'Capability store directory', './output')
  .option('--format <type>', 'Output format: table, json', 'table')
  .action(async (options) => {
    const fs = await import('fs')
    const path = await import('path')
    const YAML = await import('yaml')

    const sitesDir = path.join(options.input, 'sites')

    if (!fs.existsSync(sitesDir)) {
      console.log('No sites recorded yet.')
      process.exit(0)
    }

    const sites = fs.readdirSync(sitesDir).filter((dir: string) => {
      const siteYamlPath = path.join(sitesDir, dir, 'site.yaml')
      return fs.existsSync(siteYamlPath)
    })

    if (sites.length === 0) {
      console.log('No sites recorded yet.')
      process.exit(0)
    }

    if (options.format === 'json') {
      const siteData = sites.map((domain: string) => {
        const siteYamlPath = path.join(sitesDir, domain, 'site.yaml')
        const siteYaml = YAML.parse(
          fs.readFileSync(siteYamlPath, 'utf-8')
        ) as Record<string, unknown>

        const pagesDir = path.join(sitesDir, domain, 'pages')
        let pageCount = 0
        let elementCount = 0

        if (fs.existsSync(pagesDir)) {
          const pageFiles = fs
            .readdirSync(pagesDir)
            .filter((f: string) => f.endsWith('.yaml'))
          pageCount = pageFiles.length

          for (const pageFile of pageFiles) {
            const pageYaml = YAML.parse(
              fs.readFileSync(path.join(pagesDir, pageFile), 'utf-8')
            ) as { elements?: Record<string, unknown> }
            elementCount += Object.keys(pageYaml.elements || {}).length
          }
        }

        return {
          domain,
          name: siteYaml.name,
          scenario: siteYaml.scenario,
          pages: pageCount,
          elements: elementCount,
          recorded_at: siteYaml.recorded_at,
        }
      })

      console.log(JSON.stringify(siteData, null, 2))
    } else {
      // Table format
      console.log()
      console.log(
        '┌' +
          '─'.repeat(25) +
          '┬' +
          '─'.repeat(12) +
          '┬' +
          '─'.repeat(10) +
          '┬' +
          '─'.repeat(22) +
          '┐'
      )
      console.log(
        '│ ' +
          'Domain'.padEnd(23) +
          ' │ ' +
          'Pages'.padEnd(10) +
          ' │ ' +
          'Elements'.padEnd(8) +
          ' │ ' +
          'Recorded At'.padEnd(20) +
          ' │'
      )
      console.log(
        '├' +
          '─'.repeat(25) +
          '┼' +
          '─'.repeat(12) +
          '┼' +
          '─'.repeat(10) +
          '┼' +
          '─'.repeat(22) +
          '┤'
      )

      for (const domain of sites) {
        const siteYamlPath = path.join(sitesDir, domain as string, 'site.yaml')
        const siteYaml = YAML.parse(
          fs.readFileSync(siteYamlPath, 'utf-8')
        ) as Record<string, unknown>

        const pagesDir = path.join(sitesDir, domain as string, 'pages')
        let pageCount = 0
        let elementCount = 0

        if (fs.existsSync(pagesDir)) {
          const pageFiles = fs
            .readdirSync(pagesDir)
            .filter((f: string) => f.endsWith('.yaml'))
          pageCount = pageFiles.length

          for (const pageFile of pageFiles) {
            const pageYaml = YAML.parse(
              fs.readFileSync(path.join(pagesDir, pageFile), 'utf-8')
            ) as { elements?: Record<string, unknown> }
            elementCount += Object.keys(pageYaml.elements || {}).length
          }
        }

        const recordedAt = siteYaml.recorded_at
          ? new Date(siteYaml.recorded_at as string).toISOString().slice(0, 19)
          : 'N/A'

        console.log(
          '│ ' +
            (domain as string).slice(0, 23).padEnd(23) +
            ' │ ' +
            String(pageCount).padEnd(10) +
            ' │ ' +
            String(elementCount).padEnd(8) +
            ' │ ' +
            recordedAt.padEnd(20) +
            ' │'
        )
      }

      console.log(
        '└' +
          '─'.repeat(25) +
          '┴' +
          '─'.repeat(12) +
          '┴' +
          '─'.repeat(10) +
          '┴' +
          '─'.repeat(22) +
          '┘'
      )
      console.log()
    }
  })

program.parse(process.argv)
