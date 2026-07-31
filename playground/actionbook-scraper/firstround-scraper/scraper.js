/**
 * First Round Capital Portfolio Companies Scraper
 *
 * Target URL: https://www.firstround.com/companies?category=all
 * Data extracted: Company name, description, founders, investment stage,
 *                 categories, location, partners, website URL, exit status
 *
 * Selectors verified by Actionbook (https://actionbook.app)
 * Action ID: https://www.firstround.com/companies
 */

const { chromium } = require('playwright');
const fs = require('fs');

// Actionbook verified selectors
const SELECTORS = {
  // Small company cards (All Companies view)
  smallCard: 'div.company-list-card-small',
  smallCardButton: 'button.company-list-card-small__button',
  smallCardName: 'div.company-list-card-small__button-name',
  smallCardStatement: 'div.company-list-card-small__button-statement p',
  smallCardExpanded: 'div.company-list-card-small--open',
  smallCardWebsiteLink: 'a.company-list-card-small__link',

  // Company info (inside expanded card)
  companyInfoList: 'dl.company-list-company-info__list',
  companyInfoItem: 'div.company-list-company-info__item',
  companyInfoLabel: 'dt.company-list-company-info__label',
  companyInfoValue: 'dd.company-list-company-info__value',

  // Alternative selectors for info items (some cards use different classes)
  companyInfoLabelAlt: 'dt',
  companyInfoValueAlt: 'dd',
};

const CONFIG = {
  url: 'https://www.firstround.com/companies?category=all',
  outputFile: 'firstround-companies.json',
  scrollDelay: 500,        // ms between scroll actions
  expandDelay: 300,        // ms after expanding a card
  maxScrollAttempts: 50,   // prevent infinite scrolling
  batchSize: 10,           // process cards in batches to avoid memory issues
};

/**
 * Scroll to bottom of page to load all lazy-loaded content
 */
async function scrollToLoadAll(page) {
  console.log('Scrolling to load all companies...');

  let previousHeight = 0;
  let scrollAttempts = 0;

  while (scrollAttempts < CONFIG.maxScrollAttempts) {
    const currentHeight = await page.evaluate(() => document.body.scrollHeight);

    if (currentHeight === previousHeight) {
      // No new content loaded, we're done
      break;
    }

    previousHeight = currentHeight;
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await page.waitForTimeout(CONFIG.scrollDelay);
    scrollAttempts++;
  }

  // Scroll back to top
  await page.evaluate(() => window.scrollTo(0, 0));
  console.log(`Completed scrolling after ${scrollAttempts} attempts`);
}

/**
 * Extract company data from an expanded small card
 */
async function extractCompanyData(card) {
  const data = {
    name: '',
    description: '',
    founders: '',
    initialPartnership: '',
    categories: '',
    location: '',
    partner: '',
    websiteUrl: '',
    exitStatus: null, // 'ACQUIRED', 'IPO', or null
  };

  // Extract name
  const nameEl = await card.$(SELECTORS.smallCardName);
  if (nameEl) {
    data.name = (await nameEl.textContent())?.trim() || '';
  }

  // Extract description (tagline)
  const statementEl = await card.$(SELECTORS.smallCardStatement);
  if (statementEl) {
    let text = (await statementEl.textContent())?.trim() || '';
    // Remove "Imagine if" prefix if present
    text = text.replace(/^Imagine if\s*/i, '').trim();
    data.description = text;
  }

  // Check for exit status in button text
  const buttonEl = await card.$(SELECTORS.smallCardButton);
  if (buttonEl) {
    const buttonText = (await buttonEl.textContent()) || '';
    if (buttonText.includes('ACQUIRED') || buttonText.includes('Acquired')) {
      data.exitStatus = 'ACQUIRED';
    } else if (buttonText.includes('IPO')) {
      data.exitStatus = 'IPO';
    }
  }

  // Extract website URL
  const linkEl = await card.$(SELECTORS.smallCardWebsiteLink);
  if (linkEl) {
    data.websiteUrl = await linkEl.getAttribute('href') || '';
  }

  // Extract company info from dl list
  const infoItems = await card.$$(SELECTORS.companyInfoItem);

  for (const item of infoItems) {
    const labelEl = await item.$(SELECTORS.companyInfoLabel) || await item.$(SELECTORS.companyInfoLabelAlt);
    const valueEl = await item.$(SELECTORS.companyInfoValue) || await item.$(SELECTORS.companyInfoValueAlt);

    if (labelEl && valueEl) {
      const label = (await labelEl.textContent())?.trim().toLowerCase() || '';
      const value = (await valueEl.textContent())?.trim() || '';

      switch (label) {
        case 'founders':
        case 'founder':
          data.founders = value;
          break;
        case 'initial partnership':
          data.initialPartnership = value;
          break;
        case 'categories':
        case 'category':
          data.categories = value;
          break;
        case 'location':
        case 'locations':
          data.location = value;
          break;
        case 'partner':
        case 'partners':
          data.partner = value;
          break;
      }
    }
  }

  return data;
}

/**
 * Process cards in batches to manage memory
 */
async function processCardsInBatches(page, cards) {
  const companies = [];
  const totalCards = cards.length;

  console.log(`Processing ${totalCards} company cards...`);

  for (let i = 0; i < totalCards; i++) {
    const card = cards[i];

    try {
      // Check if card is already expanded
      const isExpanded = await card.evaluate(el => el.classList.contains('company-list-card-small--open'));

      if (!isExpanded) {
        // Click to expand the card
        const button = await card.$(SELECTORS.smallCardButton);
        if (button) {
          await button.click();
          await page.waitForTimeout(CONFIG.expandDelay);
        }
      }

      // Extract data from expanded card
      const companyData = await extractCompanyData(card);

      if (companyData.name) {
        companies.push(companyData);
      }

      // Collapse the card to free up DOM
      if (!isExpanded) {
        const button = await card.$(SELECTORS.smallCardButton);
        if (button) {
          await button.click();
          await page.waitForTimeout(50); // Brief delay
        }
      }

      // Progress logging
      if ((i + 1) % 20 === 0 || i + 1 === totalCards) {
        console.log(`Processed ${i + 1}/${totalCards} companies`);
      }

    } catch (error) {
      console.error(`Error processing card ${i + 1}:`, error.message);
    }
  }

  return companies;
}

/**
 * Main scraper function
 */
async function scrapeFirstRoundCompanies() {
  console.log('Starting First Round Capital portfolio scraper...');
  console.log(`Target URL: ${CONFIG.url}`);

  const browser = await chromium.launch({
    headless: true,
  });

  const context = await browser.newContext({
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
    viewport: { width: 1920, height: 1080 },
  });

  const page = await context.newPage();

  try {
    // Navigate to the companies page with category=all
    console.log('Navigating to page...');
    await page.goto(CONFIG.url, {
      waitUntil: 'networkidle',
      timeout: 60000,
    });

    // Wait for company cards to load
    await page.waitForSelector(SELECTORS.smallCard, { timeout: 30000 });

    // Scroll to load all lazy-loaded content
    await scrollToLoadAll(page);

    // Get all company cards
    const cards = await page.$$(SELECTORS.smallCard);
    console.log(`Found ${cards.length} company cards`);

    // Process all cards
    const companies = await processCardsInBatches(page, cards);

    // Save results
    const output = {
      metadata: {
        source: 'First Round Capital',
        url: CONFIG.url,
        scrapedAt: new Date().toISOString(),
        totalCompanies: companies.length,
        actionbookActionId: 'https://www.firstround.com/companies',
      },
      companies: companies,
    };

    fs.writeFileSync(
      CONFIG.outputFile,
      JSON.stringify(output, null, 2),
      'utf-8'
    );

    console.log(`\nScraping complete!`);
    console.log(`Total companies extracted: ${companies.length}`);
    console.log(`Results saved to: ${CONFIG.outputFile}`);

    // Summary statistics
    const acquired = companies.filter(c => c.exitStatus === 'ACQUIRED').length;
    const ipo = companies.filter(c => c.exitStatus === 'IPO').length;
    console.log(`\nSummary:`);
    console.log(`  - Acquired: ${acquired}`);
    console.log(`  - IPO: ${ipo}`);
    console.log(`  - Active: ${companies.length - acquired - ipo}`);

    return companies;

  } catch (error) {
    console.error('Scraping failed:', error);
    throw error;
  } finally {
    await browser.close();
  }
}

// Run if called directly
if (require.main === module) {
  scrapeFirstRoundCompanies()
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

module.exports = { scrapeFirstRoundCompanies };
