/**
 * crates.io Scraper
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

const { chromium } = require('playwright');
const fs = require('fs');

// Actionbook verified selectors
const SELECTORS = {
  // Homepage selectors
  homepage: {
    statsValue: '.value_ee8666cd3',
    statsLabel: '.label_ee8666cd3',
    crateListItem: '.box_e93d40046',
    crateTitle: '.title_e93d40046',
    crateSubtitle: '.subtitle_e93d40046',
    sectionHeader: 'section h2 a',
  },

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
    resultsMeta: 'div.results-meta_ea3e16fcc',
    totalResults: 'span.highlight_e7aadc795',
    sortDropdown: 'button.trigger_e0b879ca2',
    sortOptions: 'ul.list_ec5cfdfb9 a',
    paginationNext: 'a[rel="next"]',
  },

  // Crate detail page selectors
  detail: {
    heading: 'h1.heading_e5aa661bf',
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
    owners: 'ul.list_ee547f86c a.link_ee547f86c',
    ownerName: 'span.name_ee547f86c',
    categories: 'ul.categories_e2a51d261 a',
    totalDownloads: 'div.stat_ea66d4641 .num__align_ea66d4641',
    versionsCount: 'div.stat_ea66d4641:nth-child(2) .num__align_ea66d4641',
    readme: 'div.rendered-markdown',
  },

  // Search
  search: {
    input: 'input[name="q"]',
    submitButton: 'button.submit-button_e39186c09',
  },
};

const CONFIG = {
  baseUrl: 'https://crates.io',
  defaultPerPage: 50,
  maxPages: 10, // Limit pages to scrape
  delay: 500, // Delay between requests (ms)
};

/**
 * Parse numeric value from text (handles commas and K/M suffixes)
 */
function parseNumber(text) {
  if (!text) return 0;
  text = text.replace(/,/g, '').trim();
  if (text.endsWith('K')) {
    return parseFloat(text) * 1000;
  }
  if (text.endsWith('M')) {
    return parseFloat(text) * 1000000;
  }
  return parseInt(text, 10) || 0;
}

/**
 * Extract numeric value from download stats text
 */
function extractDownloadCount(text) {
  if (!text) return 0;
  const match = text.match(/[\d,]+/);
  return match ? parseNumber(match[0]) : 0;
}

/**
 * Scrape homepage statistics and featured crates
 */
async function scrapeHomepage(page) {
  console.log('Scraping crates.io homepage...');
  await page.goto(CONFIG.baseUrl, { waitUntil: 'networkidle' });

  const stats = {};

  // Get statistics
  const statElements = await page.$$('.stats_ea965244a .stats-value_ee8666cd3');
  for (const el of statElements) {
    const value = await el.$eval('.value_ee8666cd3', e => e.textContent?.trim());
    const label = await el.$eval('.label_ee8666cd3', e => e.textContent?.trim());
    if (label && value) {
      stats[label.toLowerCase().replace(/\s+/g, '_')] = parseNumber(value);
    }
  }

  // Get new crates
  const newCrates = await page.$$eval(
    'section:has(h2 a[href="/crates?sort=new"]) li a.box_e93d40046',
    items => items.map(item => ({
      name: item.querySelector('.title_e93d40046')?.textContent?.trim(),
      version: item.querySelector('.subtitle_e93d40046')?.textContent?.trim()?.replace('v', ''),
      url: item.getAttribute('href'),
    }))
  );

  // Get most downloaded
  const mostDownloaded = await page.$$eval(
    'section:has(h2 a[href="/crates?sort=downloads"]) li a.box_e93d40046',
    items => items.map(item => ({
      name: item.querySelector('.title_e93d40046')?.textContent?.trim(),
      url: item.getAttribute('href'),
    }))
  );

  return {
    stats,
    newCrates,
    mostDownloaded,
  };
}

/**
 * Scrape crate list page
 */
async function scrapeCrateList(page, options = {}) {
  const { sort = 'recent-downloads', maxPages = CONFIG.maxPages, perPage = CONFIG.defaultPerPage } = options;

  console.log(`Scraping crate list (sort: ${sort})...`);

  const allCrates = [];
  let currentPage = 1;

  while (currentPage <= maxPages) {
    const url = `${CONFIG.baseUrl}/crates?sort=${sort}&page=${currentPage}&per_page=${perPage}`;
    console.log(`  Page ${currentPage}: ${url}`);

    await page.goto(url, { waitUntil: 'networkidle' });
    await page.waitForSelector(SELECTORS.list.crateRow, { timeout: 10000 });

    // Extract crates from current page
    const crates = await page.$$eval(SELECTORS.list.crateRow, (rows, selectors) => {
      return rows.map(row => {
        const nameEl = row.querySelector(selectors.crateName);
        const versionEl = row.querySelector(selectors.crateVersion);
        const descEl = row.querySelector(selectors.crateDescription);
        const downloadsEl = row.querySelector(selectors.allTimeDownloads);
        const recentEl = row.querySelector(selectors.recentDownloads);
        const updatedEl = row.querySelector(selectors.updatedAt);
        const links = Array.from(row.querySelectorAll(selectors.quickLinks));

        return {
          name: nameEl?.textContent?.trim() || '',
          version: versionEl?.textContent?.trim()?.replace('v', '') || '',
          description: descEl?.textContent?.trim() || '',
          allTimeDownloads: downloadsEl?.textContent?.trim() || '',
          recentDownloads: recentEl?.textContent?.trim() || '',
          updatedAt: updatedEl?.getAttribute('datetime') || '',
          updatedAtRelative: updatedEl?.textContent?.trim() || '',
          links: links.reduce((acc, link) => {
            const text = link.textContent?.trim().toLowerCase();
            if (text) acc[text] = link.getAttribute('href');
            return acc;
          }, {}),
        };
      });
    }, {
      crateName: SELECTORS.list.crateName,
      crateVersion: SELECTORS.list.crateVersion,
      crateDescription: SELECTORS.list.crateDescription,
      allTimeDownloads: SELECTORS.list.allTimeDownloads,
      recentDownloads: SELECTORS.list.recentDownloads,
      updatedAt: SELECTORS.list.updatedAt,
      quickLinks: SELECTORS.list.quickLinks,
    });

    // Parse download counts
    crates.forEach(crate => {
      crate.allTimeDownloadsCount = extractDownloadCount(crate.allTimeDownloads);
      crate.recentDownloadsCount = extractDownloadCount(crate.recentDownloads);
    });

    allCrates.push(...crates);

    // Check for next page
    const hasNextPage = await page.$(SELECTORS.list.paginationNext);
    if (!hasNextPage || crates.length < perPage) {
      break;
    }

    currentPage++;
    await page.waitForTimeout(CONFIG.delay);
  }

  console.log(`  Total crates scraped: ${allCrates.length}`);
  return allCrates;
}

/**
 * Scrape single crate detail page
 */
async function scrapeCrateDetail(page, crateName) {
  console.log(`Scraping crate detail: ${crateName}...`);

  const url = `${CONFIG.baseUrl}/crates/${crateName}`;
  await page.goto(url, { waitUntil: 'networkidle' });

  // Wait for README to load (async)
  try {
    await page.waitForSelector(SELECTORS.detail.readme, { timeout: 10000 });
  } catch {
    // README might not exist
  }

  const detail = await page.evaluate((selectors) => {
    const getText = (selector) => document.querySelector(selector)?.textContent?.trim() || '';
    const getAttr = (selector, attr) => document.querySelector(selector)?.getAttribute(attr) || '';
    const getAllText = (selector) => Array.from(document.querySelectorAll(selector)).map(el => el.textContent?.trim());
    const getAllHref = (selector) => Array.from(document.querySelectorAll(selector)).map(el => ({
      text: el.textContent?.trim(),
      href: el.getAttribute('href'),
    }));

    return {
      name: getText(selectors.name),
      version: getText(selectors.version).replace('v', ''),
      description: getText(selectors.description),
      keywords: getAllText(selectors.keywords),
      publishedAt: getAttr(selectors.publishedDate, 'datetime'),
      publishedAtRelative: getText(selectors.publishedDate),
      msrv: getText(selectors.msrv),
      edition: getText(selectors.edition),
      license: getText(selectors.license),
      lineCount: getText(selectors.lineCount),
      packageSize: getText(selectors.packageSize),
      purl: getText(selectors.purl),
      repository: getAttr(selectors.repositoryLink, 'href'),
      documentation: getAttr(selectors.docsLink, 'href'),
      owners: getAllHref(selectors.owners).map(o => ({ name: o.text, profile: o.href })),
      categories: getAllText(selectors.categories),
      readme: getText(selectors.readme).substring(0, 5000), // Limit README size
    };
  }, SELECTORS.detail);

  // Get download stats
  const stats = await page.$$eval('div.stat_ea66d4641', els => {
    return els.map(el => ({
      value: el.querySelector('.num__align_ea66d4641')?.textContent?.trim(),
      label: el.querySelector('.text--small')?.textContent?.trim(),
    }));
  });

  detail.stats = {};
  stats.forEach(s => {
    if (s.label && s.value) {
      detail.stats[s.label.toLowerCase().replace(/\s+/g, '_')] = s.value;
    }
  });

  return detail;
}

/**
 * Search crates by keyword
 */
async function searchCrates(page, query, maxResults = 50) {
  console.log(`Searching crates: "${query}"...`);

  const url = `${CONFIG.baseUrl}/search?q=${encodeURIComponent(query)}`;
  await page.goto(url, { waitUntil: 'networkidle' });

  await page.waitForSelector(SELECTORS.list.crateRow, { timeout: 10000 });

  // Use same extraction as crate list
  const crates = await page.$$eval(SELECTORS.list.crateRow, (rows, selectors) => {
    return rows.slice(0, 50).map(row => {
      const nameEl = row.querySelector(selectors.crateName);
      const versionEl = row.querySelector(selectors.crateVersion);
      const descEl = row.querySelector(selectors.crateDescription);

      return {
        name: nameEl?.textContent?.trim() || '',
        version: versionEl?.textContent?.trim()?.replace('v', '') || '',
        description: descEl?.textContent?.trim() || '',
      };
    });
  }, {
    crateName: SELECTORS.list.crateName,
    crateVersion: SELECTORS.list.crateVersion,
    crateDescription: SELECTORS.list.crateDescription,
  });

  return crates.slice(0, maxResults);
}

/**
 * Main scraper function
 */
async function scrapeCratesIO(options = {}) {
  const {
    mode = 'list', // 'homepage', 'list', 'detail', 'search', 'all'
    sort = 'recent-downloads',
    maxPages = 3,
    crateName = null,
    searchQuery = null,
    outputFile = 'crates-io-data.json',
  } = options;

  console.log('Starting crates.io scraper...');
  console.log(`Mode: ${mode}`);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
    viewport: { width: 1920, height: 1080 },
  });
  const page = await context.newPage();

  const result = {
    metadata: {
      source: 'crates.io',
      scrapedAt: new Date().toISOString(),
      mode,
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
        result.homepage = await scrapeHomepage(page);
        break;

      case 'list':
        result.crates = await scrapeCrateList(page, { sort, maxPages });
        break;

      case 'detail':
        if (!crateName) throw new Error('crateName is required for detail mode');
        result.crate = await scrapeCrateDetail(page, crateName);
        break;

      case 'search':
        if (!searchQuery) throw new Error('searchQuery is required for search mode');
        result.searchQuery = searchQuery;
        result.results = await searchCrates(page, searchQuery);
        break;

      case 'all':
        result.homepage = await scrapeHomepage(page);
        await page.waitForTimeout(CONFIG.delay);
        result.topCrates = await scrapeCrateList(page, { sort: 'downloads', maxPages: 1 });
        await page.waitForTimeout(CONFIG.delay);
        result.recentCrates = await scrapeCrateList(page, { sort: 'new', maxPages: 1 });
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
    console.error('Scraping failed:', error);
    throw error;
  } finally {
    await browser.close();
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
crates.io Scraper

Usage: node scraper.js [options]

Options:
  --mode <mode>     Scrape mode: homepage, list, detail, search, all (default: list)
  --sort <sort>     Sort order: downloads, recent-downloads, new, alpha (default: recent-downloads)
  --pages <n>       Max pages to scrape (default: 3)
  --crate <name>    Crate name for detail mode
  --search <query>  Search query for search mode
  --output <file>   Output file (default: crates-io-data.json)
  --help            Show this help

Examples:
  node scraper.js --mode homepage
  node scraper.js --mode list --sort downloads --pages 5
  node scraper.js --mode detail --crate tokio
  node scraper.js --mode search --search "async runtime"
  node scraper.js --mode all
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
