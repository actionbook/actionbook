/**
 * crates.io Scraper (agent-browser version)
 *
 * Comprehensive scraper for the Rust package registry
 * Supports: crate list, crate details, search, homepage stats
 *
 * Selectors verified by Actionbook (https://actionbook.app)
 * Action IDs:
 *   - https://crates.io/
 *   - https://crates.io/crates
 *   - https://crates.io/crates/{crate_name}
 */

const { execSync } = require('child_process');
const fs = require('fs');

// Session name to isolate this scraper
const SESSION_NAME = 'crates-io-scraper';

// Actionbook verified selectors
const SELECTORS = {
  // Crate list page selectors
  list: {
    crateRow: 'ol.list_ef1bd7ef3 > li',
    crateName: 'a.name_ee3a027e7',
    crateVersion: 'span.version',
    crateDescription: 'div.description_ee3a027e7',
    allTimeDownloads: 'div.downloads_ee3a027e7',
    recentDownloads: 'div.recent-downloads_ee3a027e7',
    updatedAt: 'div.updated-at_ee3a027e7 time',
    quickLinks: 'ul.quick-links_ee3a027e7 a',
  },

  // Crate detail page selectors
  detail: {
    name: 'h1.heading_e5aa661bf > span',
    version: 'h1.heading_e5aa661bf > small',
    description: 'div.description_e5aa661bf',
    keywords: 'ul.keywords_e5aa661bf a',
    publishedDate: 'time.date_e2a51d261',
    msrv: 'div.msrv_e2a51d261 span',
    edition: 'div.edition_e2a51d261 span',
    license: 'div.license_e2a51d261',
    lineCount: 'div.linecount_e2a51d261 span',
    packageSize: 'div.bytes_e2a51d261',
    purl: 'span.purl-text_e2a51d261',
    repositoryLink: 'div.links_e2a51d261 a[href*="github"]',
    docsLink: 'div.links_e2a51d261 a[href*="docs.rs"]',
    owners: 'ul.list_ee547f86c a.link_ee547f86c span.name_ee547f86c',
    categories: 'ul.categories_e2a51d261 a',
    totalDownloads: 'div.stat_ea66d4641 .num__align_ea66d4641',
    readme: 'div.rendered-markdown',
  },
};

const CONFIG = {
  baseUrl: 'https://crates.io',
  defaultPerPage: 50,
  maxPages: 3,
  delay: 500,
};

/**
 * Wait for a specified time
 */
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Execute agent-browser command and return output
 */
function agentBrowser(command, args = []) {
  const sessionArg = `--session ${SESSION_NAME}`;
  const quotedArgs = args.map(arg => {
    if (arg.startsWith('"') || arg.startsWith("'")) return arg;
    if (arg.includes(' ') || arg.includes('?') || arg.includes('&') || arg.includes('=')) {
      return `"${arg}"`;
    }
    return arg;
  });
  const fullCommand = `agent-browser ${command} ${quotedArgs.join(' ')} ${sessionArg}`.trim();

  console.log(`> ${fullCommand}`);

  try {
    const output = execSync(fullCommand, {
      encoding: 'utf-8',
      maxBuffer: 50 * 1024 * 1024,
      timeout: 120000,
    });
    return output.trim();
  } catch (error) {
    if (error.stdout) return error.stdout.trim();
    throw error;
  }
}

/**
 * Execute JavaScript in browser via agent-browser eval
 */
function evalInline(jsCode) {
  const escaped = jsCode
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

  return agentBrowser('eval', [`"${escaped}"`]);
}

/**
 * Close any existing session first
 */
async function closeExistingSession() {
  try {
    console.log('Closing any existing session...');
    execSync(`agent-browser close --session ${SESSION_NAME}`, {
      encoding: 'utf-8',
      timeout: 10000,
    });
    await sleep(1000);
  } catch {
    // Session might not exist
  }
}

/**
 * Scrape homepage statistics
 */
async function scrapeHomepage() {
  console.log('\nScraping crates.io homepage...');

  agentBrowser('open', [CONFIG.baseUrl]);
  agentBrowser('wait', ['2000']);

  const extractScript = `
    JSON.stringify((function() {
      const stats = {};
      const statEls = document.querySelectorAll('.stats_ea965244a .stats-value_ee8666cd3');
      statEls.forEach(el => {
        const value = el.querySelector('.value_ee8666cd3')?.textContent?.trim();
        const label = el.querySelector('.label_ee8666cd3')?.textContent?.trim();
        if (label && value) stats[label.toLowerCase().replace(/\\s+/g, '_')] = value;
      });

      const newCrates = [];
      document.querySelectorAll('section:has(h2 a[href="/crates?sort=new"]) li a.box_e93d40046').forEach(el => {
        newCrates.push({
          name: el.querySelector('.title_e93d40046')?.textContent?.trim(),
          version: el.querySelector('.subtitle_e93d40046')?.textContent?.trim()?.replace('v', ''),
        });
      });

      const mostDownloaded = [];
      document.querySelectorAll('section:has(h2 a[href="/crates?sort=downloads"]) li a.box_e93d40046').forEach(el => {
        mostDownloaded.push({
          name: el.querySelector('.title_e93d40046')?.textContent?.trim(),
        });
      });

      return { stats, newCrates, mostDownloaded };
    })())
  `.replace(/\n/g, ' ').replace(/\s+/g, ' ');

  const result = evalInline(extractScript);

  try {
    let parsed = JSON.parse(result);
    if (typeof parsed === 'string') parsed = JSON.parse(parsed);
    return parsed;
  } catch {
    console.error('Failed to parse homepage data');
    return { stats: {}, newCrates: [], mostDownloaded: [] };
  }
}

/**
 * Scrape crate list page
 */
async function scrapeCrateList(options = {}) {
  const { sort = 'recent-downloads', maxPages = CONFIG.maxPages, perPage = CONFIG.defaultPerPage } = options;

  console.log(`\nScraping crate list (sort: ${sort})...`);

  const allCrates = [];

  for (let page = 1; page <= maxPages; page++) {
    const url = `${CONFIG.baseUrl}/crates?sort=${sort}&page=${page}&per_page=${perPage}`;
    console.log(`  Page ${page}: ${url}`);

    agentBrowser('open', [url]);
    agentBrowser('wait', ['2000']);
    agentBrowser('wait', [SELECTORS.list.crateRow]);

    const extractScript = `
      JSON.stringify((function() {
        const crates = [];
        document.querySelectorAll('${SELECTORS.list.crateRow}').forEach(row => {
          const nameEl = row.querySelector('${SELECTORS.list.crateName}');
          const versionEl = row.querySelector('${SELECTORS.list.crateVersion}');
          const descEl = row.querySelector('${SELECTORS.list.crateDescription}');
          const downloadsEl = row.querySelector('${SELECTORS.list.allTimeDownloads}');
          const recentEl = row.querySelector('${SELECTORS.list.recentDownloads}');
          const updatedEl = row.querySelector('${SELECTORS.list.updatedAt}');

          const links = {};
          row.querySelectorAll('${SELECTORS.list.quickLinks}').forEach(link => {
            const text = link.textContent?.trim().toLowerCase();
            if (text) links[text] = link.getAttribute('href');
          });

          crates.push({
            name: nameEl?.textContent?.trim() || '',
            version: versionEl?.textContent?.trim()?.replace('v', '') || '',
            description: descEl?.textContent?.trim() || '',
            allTimeDownloads: downloadsEl?.textContent?.replace(/[^0-9,]/g, '').trim() || '',
            recentDownloads: recentEl?.textContent?.replace(/[^0-9,]/g, '').trim() || '',
            updatedAt: updatedEl?.getAttribute('datetime') || '',
            links,
          });
        });
        return crates;
      })())
    `.replace(/\n/g, ' ').replace(/\s+/g, ' ');

    const result = evalInline(extractScript);

    try {
      let parsed = JSON.parse(result);
      if (typeof parsed === 'string') parsed = JSON.parse(parsed);
      if (Array.isArray(parsed)) {
        allCrates.push(...parsed);
        if (parsed.length < perPage) break; // No more pages
      }
    } catch (e) {
      console.error(`Failed to parse page ${page}:`, e.message);
    }

    await sleep(CONFIG.delay);
  }

  console.log(`  Total crates scraped: ${allCrates.length}`);
  return allCrates;
}

/**
 * Scrape single crate detail page
 */
async function scrapeCrateDetail(crateName) {
  console.log(`\nScraping crate detail: ${crateName}...`);

  const url = `${CONFIG.baseUrl}/crates/${crateName}`;
  agentBrowser('open', [url]);
  agentBrowser('wait', ['3000']); // Wait for README to load

  const extractScript = `
    JSON.stringify((function() {
      const getText = (sel) => document.querySelector(sel)?.textContent?.trim() || '';
      const getAttr = (sel, attr) => document.querySelector(sel)?.getAttribute(attr) || '';
      const getAllText = (sel) => Array.from(document.querySelectorAll(sel)).map(el => el.textContent?.trim());

      const stats = {};
      document.querySelectorAll('div.stat_ea66d4641').forEach(el => {
        const value = el.querySelector('.num__align_ea66d4641')?.textContent?.trim();
        const label = el.querySelector('.text--small')?.textContent?.trim();
        if (label && value) stats[label.toLowerCase().replace(/\\s+/g, '_')] = value;
      });

      return {
        name: getText('${SELECTORS.detail.name}'),
        version: getText('${SELECTORS.detail.version}').replace('v', ''),
        description: getText('${SELECTORS.detail.description}'),
        keywords: getAllText('${SELECTORS.detail.keywords}'),
        publishedAt: getAttr('${SELECTORS.detail.publishedDate}', 'datetime'),
        publishedAtRelative: getText('${SELECTORS.detail.publishedDate}'),
        msrv: getText('${SELECTORS.detail.msrv}'),
        edition: getText('${SELECTORS.detail.edition}'),
        license: getText('${SELECTORS.detail.license}'),
        lineCount: getText('${SELECTORS.detail.lineCount}'),
        packageSize: getText('${SELECTORS.detail.packageSize}'),
        purl: getText('${SELECTORS.detail.purl}'),
        repository: getAttr('${SELECTORS.detail.repositoryLink}', 'href'),
        documentation: getAttr('${SELECTORS.detail.docsLink}', 'href'),
        owners: getAllText('${SELECTORS.detail.owners}'),
        categories: getAllText('${SELECTORS.detail.categories}'),
        stats,
        readme: getText('${SELECTORS.detail.readme}').substring(0, 3000),
      };
    })())
  `.replace(/\n/g, ' ').replace(/\s+/g, ' ');

  const result = evalInline(extractScript);

  try {
    let parsed = JSON.parse(result);
    if (typeof parsed === 'string') parsed = JSON.parse(parsed);
    return parsed;
  } catch (e) {
    console.error('Failed to parse crate detail:', e.message);
    return null;
  }
}

/**
 * Search crates by keyword
 */
async function searchCrates(query, maxResults = 50) {
  console.log(`\nSearching crates: "${query}"...`);

  const url = `${CONFIG.baseUrl}/search?q=${encodeURIComponent(query)}`;
  agentBrowser('open', [url]);
  agentBrowser('wait', ['2000']);
  agentBrowser('wait', [SELECTORS.list.crateRow]);

  const extractScript = `
    JSON.stringify((function() {
      const crates = [];
      document.querySelectorAll('${SELECTORS.list.crateRow}').forEach((row, i) => {
        if (i >= ${maxResults}) return;
        const nameEl = row.querySelector('${SELECTORS.list.crateName}');
        const versionEl = row.querySelector('${SELECTORS.list.crateVersion}');
        const descEl = row.querySelector('${SELECTORS.list.crateDescription}');
        crates.push({
          name: nameEl?.textContent?.trim() || '',
          version: versionEl?.textContent?.trim()?.replace('v', '') || '',
          description: descEl?.textContent?.trim() || '',
        });
      });
      return crates;
    })())
  `.replace(/\n/g, ' ').replace(/\s+/g, ' ');

  const result = evalInline(extractScript);

  try {
    let parsed = JSON.parse(result);
    if (typeof parsed === 'string') parsed = JSON.parse(parsed);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    console.error('Failed to parse search results');
    return [];
  }
}

/**
 * Main scraper function using agent-browser
 */
async function scrapeCratesIO(options = {}) {
  const {
    mode = 'list',
    sort = 'recent-downloads',
    maxPages = 3,
    crateName = null,
    searchQuery = null,
    outputFile = 'crates-io-data-agent-browser.json',
  } = options;

  console.log('Starting crates.io scraper (agent-browser)...');
  console.log(`Mode: ${mode}`);
  console.log(`Session: ${SESSION_NAME}\n`);

  await closeExistingSession();

  const result = {
    metadata: {
      source: 'crates.io',
      scrapedAt: new Date().toISOString(),
      mode,
      scraper: 'agent-browser',
      actionbookActionIds: [
        'https://crates.io/',
        'https://crates.io/crates',
        'https://crates.io/crates/{crate_name}',
      ],
    },
  };

  try {
    switch (mode) {
      case 'homepage':
        result.homepage = await scrapeHomepage();
        break;

      case 'list':
        result.crates = await scrapeCrateList({ sort, maxPages });
        break;

      case 'detail':
        if (!crateName) throw new Error('crateName is required for detail mode');
        result.crate = await scrapeCrateDetail(crateName);
        break;

      case 'search':
        if (!searchQuery) throw new Error('searchQuery is required for search mode');
        result.searchQuery = searchQuery;
        result.results = await searchCrates(searchQuery);
        break;

      case 'all':
        result.homepage = await scrapeHomepage();
        await sleep(CONFIG.delay);
        result.topCrates = await scrapeCrateList({ sort: 'downloads', maxPages: 1 });
        await sleep(CONFIG.delay);
        result.recentCrates = await scrapeCrateList({ sort: 'new', maxPages: 1 });
        break;

      default:
        throw new Error(`Unknown mode: ${mode}`);
    }

    // Save results
    fs.writeFileSync(outputFile, JSON.stringify(result, null, 2), 'utf-8');
    console.log(`\n✓ Scraping complete!`);
    console.log(`Results saved to: ${outputFile}`);

    return result;

  } catch (error) {
    console.error('\nScraping failed:', error.message);
    throw error;
  } finally {
    console.log('\nClosing browser...');
    try {
      agentBrowser('close');
      console.log('Browser closed');
    } catch (e) {
      console.error('Failed to close browser:', e.message);
    }
  }
}

// CLI interface
if (require.main === module) {
  const args = process.argv.slice(2);
  const options = {};

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case '--mode':
        options.mode = args[++i];
        break;
      case '--sort':
        options.sort = args[++i];
        break;
      case '--pages':
        options.maxPages = parseInt(args[++i], 10);
        break;
      case '--crate':
        options.crateName = args[++i];
        break;
      case '--search':
        options.searchQuery = args[++i];
        break;
      case '--output':
        options.outputFile = args[++i];
        break;
      case '--help':
        console.log(`
crates.io Scraper (agent-browser version)

Usage: node scraper-agent-browser.js [options]

Options:
  --mode <mode>     Scrape mode: homepage, list, detail, search, all (default: list)
  --sort <sort>     Sort order: downloads, recent-downloads, new, alpha (default: recent-downloads)
  --pages <n>       Max pages to scrape (default: 3)
  --crate <name>    Crate name for detail mode
  --search <query>  Search query for search mode
  --output <file>   Output file (default: crates-io-data-agent-browser.json)
  --help            Show this help

Examples:
  node scraper-agent-browser.js --mode homepage
  node scraper-agent-browser.js --mode list --sort downloads --pages 5
  node scraper-agent-browser.js --mode detail --crate tokio
  node scraper-agent-browser.js --mode search --search "async runtime"
        `);
        process.exit(0);
    }
  }

  scrapeCratesIO(options)
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

module.exports = {
  scrapeCratesIO,
  scrapeHomepage,
  scrapeCrateList,
  scrapeCrateDetail,
  searchCrates,
};
