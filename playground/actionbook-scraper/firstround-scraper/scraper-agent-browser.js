/**
 * First Round Capital Portfolio Companies Scraper (agent-browser version)
 *
 * Target URL: https://www.firstround.com/companies?category=all
 * Data extracted: Company name, description, founders, investment stage,
 *                 categories, location, partners, website URL, exit status
 *
 * Selectors verified by Actionbook (https://actionbook.app)
 * Action ID: https://www.firstround.com/companies
 *
 * This version uses agent-browser CLI for browser automation
 */

const { execSync } = require('child_process');
const fs = require('fs');

// Session name to isolate this scraper
const SESSION_NAME = 'firstround-scraper';

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
  companyInfoItem: 'div.company-list-company-info__item',
};

const CONFIG = {
  url: 'https://www.firstround.com/companies?category=all',
  outputFile: 'firstround-companies-agent-browser.json',
};

/**
 * Execute agent-browser command and return output
 */
function agentBrowser(command, args = [], options = {}) {
  const sessionArg = `--session ${SESSION_NAME}`;
  const extraArgs = options.json ? '--json' : '';
  // Properly quote arguments that contain special characters
  const quotedArgs = args.map(arg => {
    // If arg already starts with a quote, leave it as is
    if (arg.startsWith('"') || arg.startsWith("'")) {
      return arg;
    }
    // Quote args containing special shell characters
    if (arg.includes(' ') || arg.includes('?') || arg.includes('&') || arg.includes('=')) {
      return `"${arg}"`;
    }
    return arg;
  });
  const fullCommand = `agent-browser ${command} ${quotedArgs.join(' ')} ${sessionArg} ${extraArgs}`.trim();

  console.log(`> ${fullCommand}`);

  try {
    const output = execSync(fullCommand, {
      encoding: 'utf-8',
      maxBuffer: 50 * 1024 * 1024, // 50MB buffer for large outputs
      timeout: 120000, // 2 minute timeout
    });
    return output.trim();
  } catch (error) {
    // Check if it's just a warning but command succeeded
    if (error.stdout) {
      return error.stdout.trim();
    }
    console.error(`Command failed: ${error.message}`);
    throw error;
  }
}

/**
 * Execute JavaScript in browser via agent-browser eval
 */
function evaluate(jsCode) {
  // Write JS to temp file to avoid shell escaping issues
  const tempFile = `/tmp/agent-browser-eval-${Date.now()}.js`;
  fs.writeFileSync(tempFile, jsCode, 'utf-8');

  try {
    const result = agentBrowser('eval', [`"$(cat ${tempFile})"`]);
    return result;
  } finally {
    // Clean up temp file
    try {
      fs.unlinkSync(tempFile);
    } catch {}
  }
}

/**
 * Execute JavaScript using inline approach (for simpler scripts)
 */
function evalInline(jsCode) {
  // Escape for shell - replace newlines and handle quotes
  const escaped = jsCode
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

  return agentBrowser('eval', [`"${escaped}"`]);
}

/**
 * Wait for a specified time
 */
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
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
    // Wait for session to fully close before reopening
    await sleep(1000);
  } catch {
    // Session might not exist, that's fine
  }
}

/**
 * Scroll page to load all content
 */
async function scrollToLoadAll() {
  console.log('Scrolling to load all companies...');

  let scrollAttempts = 0;
  const maxAttempts = 30;

  while (scrollAttempts < maxAttempts) {
    try {
      // Scroll down
      agentBrowser('scroll', ['down', '2000']);
      await sleep(500);

      // Check if we've reached the bottom
      const atBottom = evalInline(
        'window.innerHeight + window.scrollY >= document.body.scrollHeight - 100'
      );

      if (atBottom.includes('true')) {
        console.log('Reached bottom of page');
        break;
      }

      scrollAttempts++;
      if (scrollAttempts % 5 === 0) {
        console.log(`Scroll attempt ${scrollAttempts}...`);
      }
    } catch (error) {
      console.error('Scroll error:', error.message);
      break;
    }
  }

  // Scroll back to top
  agentBrowser('scroll', ['up', '99999']);
  await sleep(500);
  console.log(`Completed scrolling after ${scrollAttempts} attempts`);
}

/**
 * Get total card count
 */
function getCardCount() {
  const result = evalInline(
    `document.querySelectorAll('${SELECTORS.smallCard}').length`
  );
  return parseInt(result, 10) || 0;
}

/**
 * Expand all cards using JavaScript
 */
async function expandAllCards() {
  console.log('Expanding all company cards...');

  const expandScript = `
    (function() {
      const cards = document.querySelectorAll('${SELECTORS.smallCard}');
      let expanded = 0;
      cards.forEach(card => {
        if (!card.classList.contains('company-list-card-small--open')) {
          const button = card.querySelector('${SELECTORS.smallCardButton}');
          if (button) {
            button.click();
            expanded++;
          }
        }
      });
      return expanded;
    })()
  `.replace(/\n/g, ' ').replace(/\s+/g, ' ');

  const result = evalInline(expandScript);
  console.log(`Expanded cards: ${result}`);

  // Wait for animations
  await sleep(2000);
}

/**
 * Extract all company data using JavaScript evaluation
 */
function extractAllCompanies() {
  console.log('Extracting company data...');

  const extractScript = `
    JSON.stringify((function() {
      const companies = [];
      const cards = document.querySelectorAll('${SELECTORS.smallCard}');

      cards.forEach((card) => {
        const data = {
          name: '',
          description: '',
          founders: '',
          initialPartnership: '',
          categories: '',
          location: '',
          partner: '',
          websiteUrl: '',
          exitStatus: null
        };

        const nameEl = card.querySelector('${SELECTORS.smallCardName}');
        if (nameEl) {
          data.name = nameEl.textContent.trim();
        }

        const statementEl = card.querySelector('${SELECTORS.smallCardStatement}');
        if (statementEl) {
          let text = statementEl.textContent.trim();
          text = text.replace(/^Imagine if\\s*/i, '').trim();
          data.description = text;
        }

        const buttonEl = card.querySelector('${SELECTORS.smallCardButton}');
        if (buttonEl) {
          const buttonText = buttonEl.textContent || '';
          if (buttonText.includes('ACQUIRED') || buttonText.includes('Acquired')) {
            data.exitStatus = 'ACQUIRED';
          } else if (buttonText.includes('IPO')) {
            data.exitStatus = 'IPO';
          }
        }

        const linkEl = card.querySelector('${SELECTORS.smallCardWebsiteLink}');
        if (linkEl) {
          data.websiteUrl = linkEl.getAttribute('href') || '';
        }

        const infoItems = card.querySelectorAll('${SELECTORS.companyInfoItem}');
        infoItems.forEach(item => {
          const labelEl = item.querySelector('dt');
          const valueEl = item.querySelector('dd');
          if (labelEl && valueEl) {
            const label = labelEl.textContent.trim().toLowerCase();
            const value = valueEl.textContent.trim();
            if (label === 'founders' || label === 'founder') data.founders = value;
            else if (label === 'initial partnership') data.initialPartnership = value;
            else if (label === 'categories' || label === 'category') data.categories = value;
            else if (label === 'location' || label === 'locations') data.location = value;
            else if (label === 'partner' || label === 'partners') data.partner = value;
          }
        });

        if (data.name) {
          companies.push(data);
        }
      });

      return companies;
    })())
  `.replace(/\n/g, ' ').replace(/\s+/g, ' ');

  const result = evalInline(extractScript);

  try {
    let parsed = JSON.parse(result);
    // If the result is still a string, parse it again
    if (typeof parsed === 'string') {
      parsed = JSON.parse(parsed);
    }
    return Array.isArray(parsed) ? parsed : [];
  } catch (e) {
    console.error('Failed to parse result:', e.message);
    console.error('Result preview:', result.substring(0, 200));
    return [];
  }
}

/**
 * Main scraper function using agent-browser
 */
async function scrapeFirstRoundCompanies() {
  console.log('Starting First Round Capital portfolio scraper (agent-browser)...');
  console.log(`Target URL: ${CONFIG.url}`);
  console.log(`Session: ${SESSION_NAME}\n`);

  // Close any existing session first
  await closeExistingSession();

  try {
    // Open the page
    console.log('1. Opening page...');
    agentBrowser('open', [CONFIG.url]);

    // Wait for page to load
    console.log('2. Waiting for page to load...');
    agentBrowser('wait', ['3000']); // Wait 3 seconds
    agentBrowser('wait', [SELECTORS.smallCard]); // Wait for cards

    // Get initial count
    const initialCount = getCardCount();
    console.log(`   Found ${initialCount} cards initially`);

    // Scroll to load all lazy content
    console.log('\n3. Scrolling to load all content...');
    await scrollToLoadAll();

    const afterScrollCount = getCardCount();
    console.log(`   Found ${afterScrollCount} cards after scrolling`);

    // Expand all cards to reveal detailed info
    console.log('\n4. Expanding all cards...');
    await expandAllCards();

    // Extract all company data
    console.log('\n5. Extracting company data...');
    const companies = extractAllCompanies();

    // Save results
    const output = {
      metadata: {
        source: 'First Round Capital',
        url: CONFIG.url,
        scrapedAt: new Date().toISOString(),
        totalCompanies: companies.length,
        actionbookActionId: 'https://www.firstround.com/companies',
        scraper: 'agent-browser',
      },
      companies: companies,
    };

    fs.writeFileSync(
      CONFIG.outputFile,
      JSON.stringify(output, null, 2),
      'utf-8'
    );

    console.log(`\n✓ Scraping complete!`);
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
    console.error('\nScraping failed:', error.message);
    throw error;
  } finally {
    // Always close browser
    console.log('\n6. Closing browser...');
    try {
      agentBrowser('close');
      console.log('Browser closed');
    } catch (e) {
      console.error('Failed to close browser:', e.message);
    }
  }
}

// Run if called directly
if (require.main === module) {
  scrapeFirstRoundCompanies()
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

module.exports = { scrapeFirstRoundCompanies };
