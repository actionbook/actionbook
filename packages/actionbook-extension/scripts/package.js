#!/usr/bin/env node

const fs = require('fs')
const path = require('path')
const { execSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..')
const DIST = path.join(ROOT, 'dist')

const manifest = JSON.parse(
  fs.readFileSync(path.join(ROOT, 'manifest.json'), 'utf8')
)
const version = manifest.version

const zipName = `actionbook-extension-v${version}.zip`
const zipPath = path.join(DIST, zipName)

// Files to include in the zip
const includeFiles = [
  'manifest.json',
  'background.js',
  'popup.html',
  'popup.js',
  'offscreen.html',
  'offscreen.js',
  'icons/icon-16.png',
  'icons/icon-48.png',
  'icons/icon-128.png',
]

// Verify all files exist
const missing = includeFiles.filter(
  (f) => !fs.existsSync(path.join(ROOT, f))
)
if (missing.length > 0) {
  console.error('Missing files:', missing.join(', '))
  process.exit(1)
}

// Create dist directory
fs.mkdirSync(DIST, { recursive: true })

// Remove existing zip if present
if (fs.existsSync(zipPath)) {
  fs.unlinkSync(zipPath)
}

// Build file list for zip command
const fileArgs = includeFiles.map((f) => `"${f}"`).join(' ')

execSync(`zip -j9 "${zipPath}" ${fileArgs.replace(/icons\//g, '')}`, {
  cwd: ROOT,
  stdio: 'pipe',
})

// zip -j flattens paths, so we need a different approach for icons/
// Use zip with stored paths instead
if (fs.existsSync(zipPath)) {
  fs.unlinkSync(zipPath)
}

execSync(`zip -r9 "${zipPath}" ${includeFiles.join(' ')}`, {
  cwd: ROOT,
  stdio: 'inherit',
})

const stats = fs.statSync(zipPath)
const sizeKB = (stats.size / 1024).toFixed(1)

console.log()
console.log(`Packaged: ${zipName} (${sizeKB} KB)`)
console.log(`Output:   ${zipPath}`)
