#!/usr/bin/env tsx
/**
 * Interactive browser login script
 *
 * Opens a browser with persistent profile for manual login.
 * After logging in, close the browser and the session will be saved.
 *
 * Usage:
 *   pnpm run login [url]
 *
 * Examples:
 *   pnpm run login                          # Opens blank page
 *   pnpm run login https://example.com      # Opens specific URL
 */

import { chromium } from 'playwright'
import {
  BrowserProfileManager,
  ANTI_DETECTION_ARGS,
  IGNORE_DEFAULT_ARGS,
} from '@actionbookdev/browser-profile'

async function main() {
  const url = process.argv[2] || 'about:blank'
  const profileDir = process.env.BROWSER_PROFILE_DIR

  const profileManager = new BrowserProfileManager({
    baseDir: profileDir,
  })

  // Clean up stale locks from crashed sessions
  profileManager.cleanupStaleLocks()

  const info = profileManager.getInfo()
  if (info.exists) {
    console.log(`Using existing profile: ${info.path} (${info.size})`)
  } else {
    console.log(`Creating new profile: ${info.path}`)
  }

  console.log(`\nOpening browser...`)
  console.log(`URL: ${url}`)
  console.log(`\n👉 Please login manually, then close the browser to save the session.\n`)

  const context = await chromium.launchPersistentContext(
    profileManager.getProfilePath(),
    {
      headless: false,
      args: ANTI_DETECTION_ARGS,
      ignoreDefaultArgs: IGNORE_DEFAULT_ARGS,
    }
  )

  const page = await context.newPage()
  await page.goto(url)

  // Wait for browser to be closed by user
  await new Promise<void>((resolve) => {
    context.on('close', () => resolve())
  })

  console.log(`\n✅ Browser closed. Profile saved to: ${info.path}`)
}

main().catch((error) => {
  console.error('Error:', error)
  process.exit(1)
})
