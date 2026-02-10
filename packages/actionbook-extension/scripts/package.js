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

// Build zip with relative paths preserved (important for icons/ subdirectory)
execSync(`zip -r9 "${zipPath}" ${includeFiles.join(' ')}`, {
  cwd: ROOT,
  stdio: 'inherit',
})

const stats = fs.statSync(zipPath)
const sizeKB = (stats.size / 1024).toFixed(1)

// --- Chrome Web Store variant (without `key` field) ---
const cwsZipName = `actionbook-extension-v${version}-cws.zip`
const cwsZipPath = path.join(DIST, cwsZipName)

const cwsManifest = { ...manifest }
delete cwsManifest.key

// Stage files in a temp directory so manifest.json has no `key`
const cwsTmp = path.join(DIST, '_cws_tmp')
fs.mkdirSync(cwsTmp, { recursive: true })

for (const f of includeFiles) {
  const dest = path.join(cwsTmp, f)
  fs.mkdirSync(path.dirname(dest), { recursive: true })
  if (f === 'manifest.json') {
    fs.writeFileSync(dest, JSON.stringify(cwsManifest, null, 2) + '\n')
  } else {
    fs.copyFileSync(path.join(ROOT, f), dest)
  }
}

if (fs.existsSync(cwsZipPath)) {
  fs.unlinkSync(cwsZipPath)
}

execSync(`zip -r9 "${cwsZipPath}" ${includeFiles.join(' ')}`, {
  cwd: cwsTmp,
  stdio: 'inherit',
})

fs.rmSync(cwsTmp, { recursive: true })

const cwsStats = fs.statSync(cwsZipPath)
const cwsSizeKB = (cwsStats.size / 1024).toFixed(1)

console.log()
console.log('Sideloading (GitHub Release):')
console.log(`  ${zipName} (${sizeKB} KB)`)
console.log(`Chrome Web Store:`)
console.log(`  ${cwsZipName} (${cwsSizeKB} KB)`)
console.log(`Output: ${DIST}`)
