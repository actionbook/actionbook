#!/usr/bin/env npx tsx
/**
 * Browser Login Script
 *
 * Opens a browser with persistent profile for manual login.
 * The login state will be saved and reused in subsequent runs.
 *
 * Usage:
 *   pnpm login                           # Open browser for manual login
 *   pnpm login --url https://example.com # Open specific URL
 *   pnpm login --clear                   # Clear profile and re-login
 */

import { Stagehand } from "@browserbasehq/stagehand";
import { BrowserProfileManager, DEFAULT_PROFILE_DIR, ANTI_DETECTION_ARGS, IGNORE_DEFAULT_ARGS } from "../src/browser/BrowserProfileManager.js";
import readline from "readline";

// Parse command line arguments
const args = process.argv.slice(2);
const urlArg = args.find((arg) => arg.startsWith("--url="))?.split("=")[1] || args[args.indexOf("--url") + 1];
const shouldClear = args.includes("--clear");
const showHelp = args.includes("--help") || args.includes("-h");

if (showHelp) {
  console.log(`
Browser Login Script

Opens a browser with persistent profile for manual login.
The login state will be saved and reused in subsequent runs.

Usage:
  pnpm login                           Open browser for manual login
  pnpm login --url <url>               Open specific URL directly
  pnpm login --clear                   Clear existing profile and re-login
  pnpm login --help                    Show this help message

Examples:
  pnpm login
  pnpm login --url https://airbnb.com/login
  pnpm login --clear --url https://notion.so
`);
  process.exit(0);
}

async function waitForEnter(): Promise<void> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  return new Promise((resolve) => {
    rl.question("", () => {
      rl.close();
      resolve();
    });
  });
}

async function main() {
  const profileManager = new BrowserProfileManager({ baseDir: DEFAULT_PROFILE_DIR });

  console.log("\n🔐 Browser Login Tool\n");

  // Show current profile info
  const info = profileManager.getInfo();
  if (info.exists) {
    console.log(`📁 Profile: ${info.path}`);
    console.log(`📊 Size: ${info.size}`);
  } else {
    console.log(`📁 Profile: ${info.path} (new)`);
  }

  // Clear profile if requested
  if (shouldClear) {
    console.log("\n🗑️  Clearing existing profile...");
    profileManager.clear();
    console.log("✅ Profile cleared.");
  }

  // Ensure profile directory exists
  profileManager.ensureDir();

  console.log("\n🚀 Launching browser...\n");

  // Check for system proxy
  const proxyUrl = process.env.HTTPS_PROXY || process.env.HTTP_PROXY;
  if (proxyUrl) {
    console.log(`🌐 Using proxy: ${proxyUrl}`);
  }

  // Build browser launch options using common constants
  const localBrowserLaunchOptions: Record<string, unknown> = {
    headless: false,
    userDataDir: profileManager.getProfilePath(),
    preserveUserDataDir: true,
    args: ANTI_DETECTION_ARGS,
    ignoreDefaultArgs: IGNORE_DEFAULT_ARGS,
  };

  // Add proxy if configured
  if (proxyUrl) {
    localBrowserLaunchOptions.proxy = {
      server: proxyUrl,
    };
  }

  // Create Stagehand with profile
  const stagehand = new Stagehand({
    env: "LOCAL",
    localBrowserLaunchOptions,
    verbose: 0, // Suppress Stagehand logs
  });

  await stagehand.init();

  const page = stagehand.context.pages()[0];

  // Navigate to URL if provided
  if (urlArg) {
    console.log(`📍 Navigating to: ${urlArg}`);
    await page.goto(urlArg, { waitUntil: "domcontentloaded", timeout: 60000 });
  }

  console.log("═".repeat(60));
  console.log("");
  console.log("  👉 Please complete your login in the browser window.");
  console.log("  👉 You can login to multiple sites if needed.");
  console.log("  👉 When finished, press ENTER here to save and exit.");
  console.log("");
  console.log("═".repeat(60));

  // Wait for user to press Enter
  await waitForEnter();

  console.log("\n💾 Saving profile...");

  // Close browser (profile is automatically saved)
  await stagehand.close();

  console.log("\n✅ Profile saved successfully!");
  console.log(`📁 Location: ${profileManager.getProfilePath()}`);
  console.log("\n💡 Your login state will be reused in future runs with profile enabled.\n");
}

main().catch((error) => {
  console.error("\n❌ Error:", error.message);
  process.exit(1);
});
