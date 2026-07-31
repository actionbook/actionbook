/**
 * lib.rs Full Site Scraper
 *
 * Scrapes Rust crate information from lib.rs:
 * - Homepage categories
 * - Category page crate listings
 * - Individual crate details
 *
 * Selectors sourced from Actionbook (https://actionbook.app)
 */

import { chromium } from 'playwright';
import fs from 'fs/promises';

// ============================================
// Actionbook Verified Selectors
// ============================================

const SELECTORS = {
  // Homepage (https://lib.rs/)
  homepage: {
    totalCrateCount: 'header#home p',
    categoryCard: 'section#home-categories ul.cat > li',
    categoryTitle: 'div > a > h3, div > a > h4',
    categoryUrl: 'div > a',
    categoryDescription: 'div > a > span.desc',
    crateList: 'ul.crates > li > a:not(.more)',
    moreLink: 'ul.crates > li > a.more',
  },

  // Category page (https://lib.rs/{category})
  categoryPage: {
    categoryHeader: 'header#category h2',
    categoryDescription: 'header#category p.desc',
    crateCount: 'nav > ul > li.active',
    sortLinks: '.sort-by a[href*="sort="]',
    subcategoryList: '#category-subcategories ul.crates-list > li',
    crateList: '#category-crates ul.crates-list > li',
    crateLink: 'a[href^="/crates/"]',
    crateName: 'h4',
    crateDescription: 'p.desc',
    crateVersion: '.version',
    crateDownloads: '.downloads',
    crateLabels: '.labels span',
    crateKeywords: '.meta .k',
    pagination: 'a[rel="next"]',
  },

  // Crate detail page (https://lib.rs/crates/{name})
  crateDetail: {
    name: 'header#package h2 span[property="name"]',
    description: 'p.desc',
    labels: 'h2 span.labels span',
    latestVersion: 'section#versions table tbody tr:first-child th',
    publishDate: 'section#versions table tbody tr:first-child td.date',
    allVersions: 'section#versions table tbody tr',
    // Downloads section: first p is ranking, second p has download count
    downloadCount: 'section#downloads p:not(.top-n) > b:first-child',
    categoryRanking: 'section#downloads p.top-n b',
    dependentCrates: 'section#downloads p a[href*="/rev"] b',
    license: 'section#license b[property="license"]',
    // Size info is in section#sloc > p with span elements
    packageSize: 'section#sloc > p > span:first-of-type',
    sloc: 'section#sloc > p > span:last-of-type',
    authors: 'p.byline a.owner span[property="name"]',
    categories: 'div.breadcrumbs span.categories a span[property="name"]',
    keywords: 'span.keywords a.keyword',
    // Nav links - use href patterns instead of property attributes
    apiDocsLink: 'header#package nav ul li a[href^="https://docs.rs"]',
    githubLink: 'header#package nav ul li a[href^="https://github.com"]',
    readme: 'section#readme.readme',
    dependencies: 'section#deps nav > ul:not(.dev) li[property="requirements"]',
    devDependencies: 'section#deps ul.dev li',
    features: 'section#deps ul.features li a.feature',
  },
};

// ============================================
// Helper Functions
// ============================================

/**
 * Safe text extraction with timeout
 */
async function getText(locator, timeout = 5000) {
  try {
    const count = await locator.count();
    if (count === 0) return null;
    return (await locator.first().textContent({ timeout }))?.trim() || null;
  } catch {
    return null;
  }
}

/**
 * Safe attribute extraction
 */
async function getAttr(locator, attr, timeout = 5000) {
  try {
    const count = await locator.count();
    if (count === 0) return null;
    return await locator.first().getAttribute(attr, { timeout });
  } catch {
    return null;
  }
}

/**
 * Get all text from multiple elements
 */
async function getAllText(locator, timeout = 5000) {
  try {
    const elements = await locator.all();
    const results = [];
    for (const el of elements) {
      const text = await el.textContent({ timeout }).catch(() => null);
      if (text) results.push(text.trim());
    }
    return results;
  } catch {
    return [];
  }
}

/**
 * Parse number from text (handles commas)
 */
function parseNumber(text) {
  if (!text) return null;
  const match = text.match(/(\d{1,3}(?:,\d{3})*)/);
  return match ? parseInt(match[1].replace(/,/g, '')) : null;
}

/**
 * Delay between requests
 */
function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// ============================================
// Scraper Functions
// ============================================

/**
 * Scrape all categories from lib.rs homepage
 */
async function scrapeHomepage(page) {
  console.log('📦 Scraping lib.rs homepage...');
  await page.goto('https://lib.rs/', { waitUntil: 'domcontentloaded' });

  const sel = SELECTORS.homepage;

  // Total crate count
  const totalText = await getText(page.locator(sel.totalCrateCount));
  const totalCrates = parseNumber(totalText);
  console.log(`   Total crates indexed: ${totalCrates?.toLocaleString() || 'N/A'}`);

  // Categories
  const categories = [];
  const categoryCards = await page.locator(sel.categoryCard).all();

  for (const card of categoryCards) {
    const title = await getText(card.locator(sel.categoryTitle));
    const url = await getAttr(card.locator(sel.categoryUrl), 'href');
    const description = await getText(card.locator(sel.categoryDescription));

    // Featured crates
    const crateLinks = await card.locator(sel.crateList).all();
    const featuredCrates = [];

    for (const link of crateLinks.slice(0, 10)) {
      const name = (await link.textContent())?.replace(/<wbr>/g, '').trim();
      const crateUrl = await link.getAttribute('href');
      const crateDesc = await link.getAttribute('title');
      if (name) {
        featuredCrates.push({ name, url: crateUrl, description: crateDesc });
      }
    }

    // "More" count
    const moreText = await getText(card.locator(sel.moreLink));
    const totalInCategory = parseNumber(moreText) || featuredCrates.length;

    if (title && url) {
      categories.push({
        title: title.trim(),
        url: `https://lib.rs${url}`,
        description: description?.trim() || null,
        totalCrates: totalInCategory,
        featuredCrates,
      });
    }
  }

  console.log(`   Found ${categories.length} categories`);
  return { totalCrates, categories, scrapedAt: new Date().toISOString() };
}

/**
 * Scrape crates from a category page
 */
async function scrapeCategoryPage(page, categoryUrl, options = {}) {
  const { maxPages = 10, delayMs = 1000 } = options;
  console.log(`📂 Scraping category: ${categoryUrl}`);

  const sel = SELECTORS.categoryPage;
  const crates = [];
  let currentPage = 1;

  while (currentPage <= maxPages) {
    const url = currentPage === 1 ? categoryUrl : `${categoryUrl}?page=${currentPage}`;
    await page.goto(url, { waitUntil: 'domcontentloaded' });

    // Get category info on first page
    if (currentPage === 1) {
      const categoryName = await getText(page.locator(sel.categoryHeader));
      const categoryDesc = await getText(page.locator(sel.categoryDescription));
      const countText = await getText(page.locator(sel.crateCount));
      console.log(`   Category: ${categoryName}`);
      console.log(`   ${countText || ''}`);
    }

    // Get crates on this page
    const crateItems = await page.locator(sel.crateList).all();
    if (crateItems.length === 0) break;

    for (const item of crateItems) {
      const link = item.locator('a').first();
      const name = await getText(item.locator(sel.crateName));
      const crateUrl = await getAttr(link, 'href');
      const description = await getText(item.locator(sel.crateDescription));
      const versionEl = item.locator(sel.crateVersion);
      const version = await getText(versionEl);
      const isStable = (await versionEl.getAttribute('class'))?.includes('stable');
      const downloadsEl = item.locator(sel.crateDownloads);
      const downloadsText = await getText(downloadsEl);
      const downloadsExact = await getAttr(downloadsEl, 'title');
      const labels = await getAllText(item.locator(sel.crateLabels));
      const keywords = (await getAllText(item.locator(sel.crateKeywords)))
        .map(k => k.replace('#', ''));

      if (name && crateUrl) {
        crates.push({
          name: name.replace(/<wbr>/g, '').trim(),
          url: `https://lib.rs${crateUrl}`,
          description: description?.trim() || null,
          version: version?.replace('v', '').trim() || null,
          stable: isStable,
          downloads: parseNumber(downloadsExact) || downloadsText,
          labels: labels.filter(Boolean),
          keywords: keywords.filter(Boolean),
        });
      }
    }

    console.log(`   Page ${currentPage}: ${crateItems.length} crates (total: ${crates.length})`);
    currentPage++;

    // Check for next page
    const hasNext = await page.locator(sel.pagination).count() > 0;
    if (!hasNext) break;

    await delay(delayMs);
  }

  return crates;
}

/**
 * Scrape detailed information for a specific crate
 */
async function scrapeCrateDetail(page, crateName) {
  const url = `https://lib.rs/crates/${crateName}`;
  console.log(`🔍 Scraping crate: ${crateName}`);

  await page.goto(url, { waitUntil: 'domcontentloaded' });

  const sel = SELECTORS.crateDetail;

  // Basic info
  const name = await getText(page.locator(sel.name));
  const description = await getText(page.locator(sel.description));
  const labels = await getAllText(page.locator(sel.labels));

  // Version info
  const latestVersionText = await getText(page.locator(sel.latestVersion));
  const latestVersion = latestVersionText?.replace('new', '').trim();
  const publishDate = await getText(page.locator(sel.publishDate));

  // Get all versions
  const versionRows = await page.locator(sel.allVersions).all();
  const versions = [];
  for (const row of versionRows.slice(0, 10)) {
    const ver = await getText(row.locator('th'));
    const date = await getText(row.locator('td.date'));
    if (ver) {
      versions.push({
        version: ver.replace('new', '').trim(),
        date: date?.trim() || null,
      });
    }
  }

  // Metrics
  const downloadText = await getText(page.locator(sel.downloadCount));
  const downloads = parseNumber(downloadText);
  const rankText = await getText(page.locator(sel.categoryRanking));
  const categoryRanking = rankText ? parseInt(rankText) : null;
  const dependentText = await getText(page.locator(sel.dependentCrates));
  const dependentCrates = parseNumber(dependentText);

  // License and size
  const license = await getText(page.locator(sel.license));
  const packageSize = await getText(page.locator(sel.packageSize));
  const sloc = await getText(page.locator(sel.sloc));

  // Authors, categories, keywords
  const authors = await getAllText(page.locator(sel.authors));
  const categories = await getAllText(page.locator(sel.categories));
  const keywords = (await getAllText(page.locator(sel.keywords)))
    .map(k => k.replace('#', ''));

  // Links
  const apiDocsUrl = await getAttr(page.locator(sel.apiDocsLink), 'href');
  const githubUrl = await getAttr(page.locator(sel.githubLink), 'href');

  // Dependencies - crate link has title attr with version, text is crate name
  const depElements = await page.locator(sel.dependencies).all();
  const dependencies = [];
  for (const dep of depElements) {
    // Find the crate link (has title attribute with version, not a feature link)
    const crateLink = dep.locator('a[title][href^="/crates/"]:not(.feature)').first();
    const depName = await getText(crateLink);
    const version = await getAttr(crateLink, 'title');
    const isOptional = await dep.locator('.optional, a.feature').count() > 0;
    if (depName) {
      dependencies.push({ name: depName.trim(), version, optional: isOptional });
    }
  }

  // Dev dependencies
  const devDepElements = await page.locator(sel.devDependencies).all();
  const devDependencies = [];
  for (const dep of devDepElements) {
    const crateLink = dep.locator('a[title][href^="/crates/"]:not(.feature)').first();
    const depName = await getText(crateLink);
    const version = await getAttr(crateLink, 'title');
    if (depName) {
      devDependencies.push({ name: depName.trim(), version });
    }
  }

  // Features
  const features = await getAllText(page.locator(sel.features));

  // README (optional, can be large)
  let readmeHtml = null;
  try {
    const readmeEl = page.locator(sel.readme).first();
    if (await readmeEl.count() > 0) {
      readmeHtml = await readmeEl.innerHTML({ timeout: 5000 });
    }
  } catch {
    // README not available
  }

  return {
    name,
    description,
    labels: labels.filter(Boolean),
    latestVersion,
    publishDate,
    versions,
    downloads,
    categoryRanking,
    dependentCrates,
    license,
    packageSize,
    sloc,
    authors: authors.filter(Boolean),
    categories: categories.filter(Boolean),
    keywords: keywords.filter(Boolean),
    links: {
      librs: url,
      apiDocs: apiDocsUrl,
      github: githubUrl,
    },
    dependencies,
    devDependencies,
    features: features.filter(Boolean),
    readmeHtml,
    scrapedAt: new Date().toISOString(),
  };
}

// ============================================
// CLI Interface
// ============================================

async function main() {
  const command = process.argv[2] || 'help';
  const arg = process.argv[3];
  const arg2 = process.argv[4];

  if (command === 'help') {
    console.log(`
lib.rs Scraper - Powered by Actionbook Selectors

Usage:
  node scraper.js <command> [arguments]

Commands:
  homepage              Scrape all categories from homepage
  category <url>        Scrape crates from a category page
  detail <crate-name>   Scrape detailed info for a crate
  batch <file>          Batch scrape crates from JSON file
  help                  Show this help message

Examples:
  node scraper.js homepage
  node scraper.js category https://lib.rs/rust-patterns
  node scraper.js detail tokio
  node scraper.js batch crates-to-scrape.json

Output:
  Results are saved to JSON files in the current directory.
`);
    return;
  }

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
  });
  const page = await context.newPage();

  try {
    let result;
    let outputFile;

    switch (command) {
      case 'homepage':
        result = await scrapeHomepage(page);
        outputFile = 'homepage.json';
        console.log(`\n✅ Found ${result.categories.length} categories`);
        break;

      case 'category':
        if (!arg) {
          console.error('Error: Category URL required');
          console.log('Usage: node scraper.js category <url>');
          console.log('Example: node scraper.js category https://lib.rs/rust-patterns');
          process.exit(1);
        }
        const maxPages = arg2 ? parseInt(arg2) : 10;
        result = await scrapeCategoryPage(page, arg, { maxPages });
        const categorySlug = arg.split('/').pop();
        outputFile = `category-${categorySlug}.json`;
        console.log(`\n✅ Found ${result.length} crates`);
        break;

      case 'detail':
        if (!arg) {
          console.error('Error: Crate name required');
          console.log('Usage: node scraper.js detail <crate-name>');
          console.log('Example: node scraper.js detail tokio');
          process.exit(1);
        }
        result = await scrapeCrateDetail(page, arg);
        outputFile = `crate-${arg}.json`;
        console.log(`\n✅ Scraped: ${result.name} v${result.latestVersion}`);
        break;

      case 'batch':
        if (!arg) {
          console.error('Error: Input file required');
          console.log('Usage: node scraper.js batch <file.json>');
          process.exit(1);
        }
        const input = JSON.parse(await fs.readFile(arg, 'utf-8'));
        const crateNames = Array.isArray(input) ? input : input.crates;
        const results = [];
        for (let i = 0; i < crateNames.length; i++) {
          const name = typeof crateNames[i] === 'string' ? crateNames[i] : crateNames[i].name;
          console.log(`[${i + 1}/${crateNames.length}]`);
          try {
            const detail = await scrapeCrateDetail(page, name);
            results.push(detail);
          } catch (err) {
            console.error(`   ❌ Failed: ${err.message}`);
            results.push({ name, error: err.message });
          }
          if (i < crateNames.length - 1) await delay(1500);
        }
        result = { crates: results, total: results.length, scrapedAt: new Date().toISOString() };
        outputFile = 'batch-results.json';
        console.log(`\n✅ Scraped ${results.filter(r => !r.error).length}/${crateNames.length} crates`);
        break;

      default:
        console.error(`Unknown command: ${command}`);
        console.log('Run "node scraper.js help" for usage');
        process.exit(1);
    }

    // Save results
    await fs.writeFile(outputFile, JSON.stringify(result, null, 2));
    console.log(`📁 Results saved to: ${outputFile}`);

  } finally {
    await browser.close();
  }
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
