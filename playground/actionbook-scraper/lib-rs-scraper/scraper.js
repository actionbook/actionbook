/**
 * lib.rs Scraper
 *
 * Comprehensive scraper for Lib.rs - the Rust crate index
 * Supports: homepage categories, category browsing, crate details, search
 *
 * Selectors verified by Actionbook (https://actionbook.app)
 * Action IDs:
 *   - https://lib.rs/
 *   - https://lib.rs/crates/{crate_name}
 *   - https://lib.rs/{category}
 */

const { chromium } = require('playwright');
const fs = require('fs');

// Actionbook verified selectors
const SELECTORS = {
  // Homepage selectors
  homepage: {
    categoriesSection: 'section#home-categories',
    categoryItem: 'ul.cat > li',
    categoryTitle: 'div > a > h3, div > a > h4',
    categoryLink: 'div > a',
    categoryDesc: 'span.desc',
    crateList: 'ul.crates',
    crateLink: 'ul.crates > li > a:not(.more)',
    moreLink: 'ul.crates > li > a.more',
    searchInput: 'input#search_q',
  },

  // Category page selectors
  category: {
    crateRow: 'ul.crates > li',
    crateName: 'a',
    crateDesc: 'a[title]',
    sortDropdown: 'select, .sort-select',
    pagination: '.pagination a, a[rel="next"]',
  },

  // Crate detail page selectors
  detail: {
    header: 'header#package',
    name: 'header#package h2 span[property="name"]',
    description: 'p.desc',
    labels: 'h2 span.labels span',
    breadcrumbCategory: 'div.breadcrumbs span.categories a span[property="name"]',

    // Version info
    versionsSection: 'section#versions',
    latestVersion: 'section#versions table tbody tr:first-child th',
    versionTable: 'section#versions table tbody tr',
    releaseCount: 'section#versions h3 a',
    stableCount: 'section#versions h3 span',

    // Download stats
    downloadsSection: 'section#downloads',
    categoryRank: 'section#downloads p.top-n',
    downloadCount: 'section#downloads p b',
    dependentCrates: 'section#downloads p a[href*="/rev"]',

    // Metadata
    license: 'section#license b[property="license"]',
    packageSize: 'section#sloc > p:nth-child(2)',
    sloc: 'section#sloc > p:nth-child(3)',

    // Authors and links
    authors: 'p.byline a.owner span[property="name"]',
    keywords: 'span.keywords a.keyword',
    apiDocsLink: 'nav ul li a[property="softwareHelp"]',
    githubLink: 'nav ul li a[property="url"]',
    installLink: 'nav ul li a[href^="/install/"]',
    auditLink: 'nav ul li a[href*="/audit"]',

    // README
    readme: 'section#readme.readme',

    // Dependencies
    depsSection: 'section#deps',
    runtimeDeps: 'section#deps nav > ul:not(.dev):not(.features) li',
    devDeps: 'section#deps ul.dev li',
    features: 'section#deps ul.features li',
  },
};

const CONFIG = {
  baseUrl: 'https://lib.rs',
  delay: 500,
  maxCratesPerCategory: 100,
};

/**
 * Parse numeric value from text (handles commas)
 */
function parseNumber(text) {
  if (!text) return 0;
  const match = text.match(/[\d,]+/);
  if (!match) return 0;
  return parseInt(match[0].replace(/,/g, ''), 10) || 0;
}

/**
 * Clean text content
 */
function cleanText(text) {
  if (!text) return '';
  return text.replace(/\s+/g, ' ').trim();
}

/**
 * Scrape homepage - get all categories and featured crates
 */
async function scrapeHomepage(page) {
  console.log('Scraping lib.rs homepage...');
  await page.goto(CONFIG.baseUrl, { waitUntil: 'networkidle' });

  // Get total crate count from header
  const headerText = await page.$eval('header#home p', el => el.textContent || '');
  const totalCrates = parseNumber(headerText);

  // Get all categories
  const categories = await page.$$eval(SELECTORS.homepage.categoryItem, (items) => {
    return items.map(item => {
      const linkEl = item.querySelector('div > a');
      const titleEl = item.querySelector('h3, h4');
      const descEl = item.querySelector('span.desc');
      const crateEls = item.querySelectorAll('ul.crates > li > a:not(.more)');
      const moreEl = item.querySelector('ul.crates > li > a.more');

      const isSubcategory = titleEl?.tagName === 'H4';

      // Extract featured crates
      const featuredCrates = Array.from(crateEls).map(el => ({
        name: el.textContent?.replace(/<wbr>/g, '').trim() || '',
        description: el.getAttribute('title') || '',
        url: el.getAttribute('href') || '',
      }));

      // Extract more count
      let moreCount = 0;
      if (moreEl) {
        const moreText = moreEl.textContent || '';
        const match = moreText.match(/(\d[\d,]*)/);
        if (match) moreCount = parseInt(match[1].replace(/,/g, ''), 10);
      }

      return {
        name: titleEl?.textContent?.trim() || '',
        description: descEl?.textContent?.trim() || '',
        url: linkEl?.getAttribute('href') || '',
        isSubcategory,
        featuredCrates,
        totalCrates: moreCount + featuredCrates.length,
      };
    });
  });

  console.log(`  Found ${categories.length} categories`);

  return {
    totalCrates,
    categories,
  };
}

/**
 * Scrape a category page - get all crates in a category
 */
async function scrapeCategory(page, categoryPath, options = {}) {
  const { maxCrates = CONFIG.maxCratesPerCategory, sort = 'popular' } = options;

  console.log(`Scraping category: ${categoryPath}...`);

  const url = `${CONFIG.baseUrl}${categoryPath}?sort=${sort}`;
  await page.goto(url, { waitUntil: 'networkidle' });

  // Get category info
  const categoryInfo = await page.evaluate(() => {
    const titleEl = document.querySelector('h1, header h2');
    const descEl = document.querySelector('header p.desc, .category-desc');
    return {
      name: titleEl?.textContent?.trim() || '',
      description: descEl?.textContent?.trim() || '',
    };
  });

  // Get all crates
  const crates = await page.$$eval('ul.crates > li', (items) => {
    return items.map(item => {
      const linkEl = item.querySelector('a');
      if (!linkEl || linkEl.classList.contains('more')) return null;

      return {
        name: linkEl.textContent?.replace(/<wbr>/g, '').trim() || '',
        description: linkEl.getAttribute('title') || '',
        url: linkEl.getAttribute('href') || '',
      };
    }).filter(Boolean);
  });

  console.log(`  Found ${crates.length} crates in ${categoryPath}`);

  return {
    ...categoryInfo,
    path: categoryPath,
    sort,
    crates: crates.slice(0, maxCrates),
  };
}

/**
 * Scrape crate detail page
 */
async function scrapeCrateDetail(page, crateName) {
  console.log(`Scraping crate detail: ${crateName}...`);

  const url = `${CONFIG.baseUrl}/crates/${crateName}`;
  await page.goto(url, { waitUntil: 'networkidle' });

  // Wait for content to load
  try {
    await page.waitForSelector(SELECTORS.detail.header, { timeout: 10000 });
  } catch {
    throw new Error(`Crate not found: ${crateName}`);
  }

  const detail = await page.evaluate((selectors) => {
    const getText = (selector) => document.querySelector(selector)?.textContent?.trim() || '';
    const getAttr = (selector, attr) => document.querySelector(selector)?.getAttribute(attr) || '';
    const getAllText = (selector) => Array.from(document.querySelectorAll(selector)).map(el => el.textContent?.trim()).filter(Boolean);

    // Basic info
    const name = getText(selectors.name);
    const description = getText(selectors.description);

    // Labels (no-std, async, etc.)
    const labels = getAllText(selectors.labels);

    // Categories from breadcrumb
    const categories = getAllText(selectors.breadcrumbCategory);

    // Version info
    const latestVersionEl = document.querySelector(selectors.latestVersion);
    const latestVersion = latestVersionEl?.getAttribute('content') || latestVersionEl?.textContent?.trim().replace(/^new\s*/, '') || '';

    // Get all versions from table
    const versionRows = document.querySelectorAll(selectors.versionTable);
    const versions = Array.from(versionRows).slice(0, 10).map(row => {
      const versionEl = row.querySelector('th');
      const dateEl = row.querySelector('td.date');
      return {
        version: versionEl?.getAttribute('content') || versionEl?.textContent?.trim().replace(/^new\s*/, '') || '',
        date: dateEl?.textContent?.trim() || '',
      };
    });

    // Release counts
    const releaseCountText = getText(selectors.releaseCount);
    const releaseCount = parseInt(releaseCountText.match(/\d+/)?.[0] || '0', 10);
    const stableCountText = getText(selectors.stableCount);
    const stableCount = parseInt(stableCountText.match(/\d+/)?.[0] || '0', 10);

    // Download stats
    const categoryRankEl = document.querySelector(selectors.categoryRank);
    let categoryRank = null;
    let categoryName = null;
    if (categoryRankEl) {
      const rankMatch = categoryRankEl.textContent?.match(/#(\d+)/);
      if (rankMatch) categoryRank = parseInt(rankMatch[1], 10);
      const categoryLinkEl = categoryRankEl.querySelector('a');
      categoryName = categoryLinkEl?.textContent?.trim() || null;
    }

    const downloadEls = document.querySelectorAll(selectors.downloadCount);
    let downloadsPerMonth = 0;
    let dependentCratesCount = 0;
    let directDependents = 0;

    downloadEls.forEach(el => {
      const text = el.textContent || '';
      if (text.includes(',')) {
        downloadsPerMonth = parseInt(text.replace(/,/g, ''), 10);
      }
    });

    const dependentEl = document.querySelector(selectors.dependentCrates);
    if (dependentEl) {
      const depText = dependentEl.textContent || '';
      dependentCratesCount = parseInt(depText.replace(/,/g, ''), 10) || 0;
      const directMatch = dependentEl.parentElement?.textContent?.match(/\((\d+)\s*directly\)/);
      if (directMatch) directDependents = parseInt(directMatch[1], 10);
    }

    // Metadata
    const license = getText(selectors.license);
    const packageSize = getText(selectors.packageSize);
    const sloc = getText(selectors.sloc);

    // Authors
    const authors = getAllText(selectors.authors);

    // Keywords
    const keywordEls = document.querySelectorAll(selectors.keywords);
    const keywords = Array.from(keywordEls).map(el => el.textContent?.replace('#', '').trim()).filter(Boolean);

    // Links
    const apiDocsUrl = getAttr(selectors.apiDocsLink, 'href');
    const githubUrl = getAttr(selectors.githubLink, 'href');
    const installUrl = getAttr(selectors.installLink, 'href');

    // Dependencies
    const runtimeDepEls = document.querySelectorAll(selectors.runtimeDeps);
    const runtimeDeps = Array.from(runtimeDepEls).map(el => {
      const linkEl = el.querySelector('a[href^="/crates/"]');
      const isOptional = el.classList.contains('optional') || !!el.querySelector('.feature');
      return {
        name: linkEl?.querySelector('b')?.textContent?.trim() || linkEl?.textContent?.trim() || '',
        version: linkEl?.getAttribute('title') || '',
        optional: isOptional,
      };
    }).filter(d => d.name);

    const devDepEls = document.querySelectorAll(selectors.devDeps);
    const devDeps = Array.from(devDepEls).map(el => {
      const linkEl = el.querySelector('a[href^="/crates/"]');
      return {
        name: linkEl?.querySelector('b')?.textContent?.trim() || linkEl?.textContent?.trim() || '',
        version: linkEl?.getAttribute('title') || '',
      };
    }).filter(d => d.name);

    const featureEls = document.querySelectorAll(selectors.features);
    const features = Array.from(featureEls).map(el => el.textContent?.trim()).filter(Boolean);

    // README (limited)
    const readmeEl = document.querySelector(selectors.readme);
    const readmeHtml = readmeEl?.innerHTML?.substring(0, 10000) || '';
    const readmeText = readmeEl?.textContent?.substring(0, 5000)?.trim() || '';

    return {
      name,
      description,
      labels,
      categories,
      latestVersion,
      versions,
      releaseCount,
      stableCount,
      categoryRank,
      categoryName,
      downloadsPerMonth,
      dependentCratesCount,
      directDependents,
      license,
      packageSize,
      sloc,
      authors,
      keywords,
      links: {
        apiDocs: apiDocsUrl,
        github: githubUrl,
        install: installUrl ? `${window.location.origin}${installUrl}` : '',
      },
      dependencies: {
        runtime: runtimeDeps,
        dev: devDeps,
      },
      features,
      readme: {
        text: readmeText,
        htmlPreview: readmeHtml.substring(0, 2000),
      },
    };
  }, SELECTORS.detail);

  return detail;
}

/**
 * Search crates
 */
async function searchCrates(page, query, maxResults = 50) {
  console.log(`Searching lib.rs: "${query}"...`);

  const url = `${CONFIG.baseUrl}/search?q=${encodeURIComponent(query)}`;
  await page.goto(url, { waitUntil: 'networkidle' });

  // Wait for results
  await page.waitForTimeout(1000);

  const results = await page.$$eval('ul.crates > li', (items) => {
    return items.map(item => {
      const linkEl = item.querySelector('a');
      if (!linkEl || linkEl.classList.contains('more')) return null;

      return {
        name: linkEl.textContent?.replace(/<wbr>/g, '').trim() || '',
        description: linkEl.getAttribute('title') || '',
        url: linkEl.getAttribute('href') || '',
      };
    }).filter(Boolean);
  });

  console.log(`  Found ${results.length} results for "${query}"`);
  return results.slice(0, maxResults);
}

/**
 * Scrape multiple crate details
 */
async function scrapeMultipleCrates(page, crateNames) {
  const crates = [];

  for (const name of crateNames) {
    try {
      const detail = await scrapeCrateDetail(page, name);
      crates.push(detail);
      await page.waitForTimeout(CONFIG.delay);
    } catch (error) {
      console.error(`  Failed to scrape ${name}: ${error.message}`);
      crates.push({ name, error: error.message });
    }
  }

  return crates;
}

/**
 * Main scraper function
 */
async function scrapeLibRs(options = {}) {
  const {
    mode = 'homepage', // 'homepage', 'category', 'detail', 'search', 'all'
    category = null,
    crateName = null,
    crateNames = [],
    searchQuery = null,
    sort = 'popular',
    maxCrates = 50,
    outputFile = 'lib-rs-data.json',
  } = options;

  console.log('Starting lib.rs scraper...');
  console.log(`Mode: ${mode}`);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
    viewport: { width: 1920, height: 1080 },
  });
  const page = await context.newPage();

  const result = {
    metadata: {
      source: 'lib.rs',
      scrapedAt: new Date().toISOString(),
      mode,
      actionbookActionIds: [
        'https://lib.rs/',
        'https://lib.rs/crates/{crate_name}',
        'https://lib.rs/{category}',
      ],
    },
  };

  try {
    switch (mode) {
      case 'homepage':
        result.homepage = await scrapeHomepage(page);
        break;

      case 'category':
        if (!category) throw new Error('category is required for category mode');
        result.category = await scrapeCategory(page, category, { maxCrates, sort });
        break;

      case 'detail':
        if (crateName) {
          result.crate = await scrapeCrateDetail(page, crateName);
        } else if (crateNames.length > 0) {
          result.crates = await scrapeMultipleCrates(page, crateNames);
        } else {
          throw new Error('crateName or crateNames is required for detail mode');
        }
        break;

      case 'search':
        if (!searchQuery) throw new Error('searchQuery is required for search mode');
        result.searchQuery = searchQuery;
        result.results = await searchCrates(page, searchQuery, maxCrates);
        break;

      case 'all':
        // Scrape homepage
        result.homepage = await scrapeHomepage(page);
        await page.waitForTimeout(CONFIG.delay);

        // Scrape a few popular categories
        const popularCategories = ['/rust-patterns', '/network-programming', '/algorithms'];
        result.categories = [];

        for (const cat of popularCategories) {
          const catData = await scrapeCategory(page, cat, { maxCrates: 20 });
          result.categories.push(catData);
          await page.waitForTimeout(CONFIG.delay);
        }

        // Scrape details of top featured crates
        const topCrates = result.homepage.categories
          .flatMap(c => c.featuredCrates)
          .slice(0, 10)
          .map(c => c.name);

        result.crateDetails = await scrapeMultipleCrates(page, topCrates);
        break;

      default:
        throw new Error(`Unknown mode: ${mode}`);
    }

    // Save results
    fs.writeFileSync(outputFile, JSON.stringify(result, null, 2), 'utf-8');
    console.log(`\nScraping complete!`);
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
      case '--category':
        options.category = args[++i];
        break;
      case '--crate':
        options.crateName = args[++i];
        break;
      case '--crates':
        options.crateNames = args[++i].split(',');
        break;
      case '--search':
        options.searchQuery = args[++i];
        break;
      case '--sort':
        options.sort = args[++i];
        break;
      case '--max':
        options.maxCrates = parseInt(args[++i], 10);
        break;
      case '--output':
        options.outputFile = args[++i];
        break;
      case '--help':
        console.log(`
lib.rs Scraper - Rust Crate Index Scraper

Usage: node scraper.js [options]

Options:
  --mode <mode>       Scrape mode: homepage, category, detail, search, all (default: homepage)
  --category <path>   Category path for category mode (e.g., /rust-patterns)
  --crate <name>      Crate name for detail mode
  --crates <names>    Comma-separated crate names for batch detail mode
  --search <query>    Search query for search mode
  --sort <sort>       Sort order: popular, new (default: popular)
  --max <n>           Max crates to scrape (default: 50)
  --output <file>     Output file (default: lib-rs-data.json)
  --help              Show this help

Examples:
  node scraper.js --mode homepage
  node scraper.js --mode category --category /rust-patterns --max 100
  node scraper.js --mode detail --crate tokio
  node scraper.js --mode detail --crates tokio,serde,anyhow
  node scraper.js --mode search --search "async runtime"
  node scraper.js --mode all --output full-data.json
        `);
        process.exit(0);
    }
  }

  scrapeLibRs(options)
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

module.exports = {
  scrapeLibRs,
  scrapeHomepage,
  scrapeCategory,
  scrapeCrateDetail,
  searchCrates,
};
