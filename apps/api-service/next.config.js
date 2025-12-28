const path = require('path');

/** @type {import('next').NextConfig} */
const nextConfig = {
  // API-only service, no static pages needed
  output: 'standalone',
  // Set workspace root to monorepo root to fix lockfile detection warning
  outputFileTracingRoot: path.join(__dirname, '../../'),
};

module.exports = nextConfig;
