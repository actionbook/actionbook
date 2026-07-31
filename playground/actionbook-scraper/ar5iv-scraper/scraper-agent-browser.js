/**
 * ar5iv.labs.arxiv.org Scraper (agent-browser version)
 *
 * Scrapes academic papers from ar5iv (arXiv papers as HTML5)
 * Supports: single paper, batch papers, random paper
 *
 * Selectors verified by Actionbook (https://actionbook.app)
 * Action IDs:
 *   - https://ar5iv.labs.arxiv.org
 *   - https://ar5iv.labs.arxiv.org/html/{paper_id}
 */

const { execSync } = require('child_process');
const fs = require('fs');

// Session name
const SESSION_NAME = 'ar5iv-scraper';

// Actionbook verified selectors
const SELECTORS = {
  // Paper page selectors
  paper: {
    title: 'h1.ltx_title_document',
    authors: 'div.ltx_authors',
    authorName: 'span.ltx_personname',
    authorAffiliation: 'span.ltx_contact.ltx_role_affiliation',
    date: 'div.ltx_dates',
    abstract: 'div.ltx_abstract',
    abstractText: 'div.ltx_abstract p.ltx_p',
    sections: 'section.ltx_section',
    sectionTitle: 'h2.ltx_title_section',
    paragraph: 'div.ltx_para p.ltx_p',
    citations: 'cite.ltx_cite a',
    bibliography: 'ul.ltx_biblist li.ltx_bibitem',
    bibLabel: 'span.ltx_tag_bibitem',
    bibText: 'span.ltx_bibblock',
    // Navigation
    nextPaper: 'a[href^="/html/"]:has-text("►")',
    prevPaper: 'a[href^="/html/"]:has-text("◄")',
    originalArxiv: 'a[href*="arxiv.org/abs"]',
  },
};

const CONFIG = {
  baseUrl: 'https://ar5iv.labs.arxiv.org',
  delay: 1000,
};

/**
 * Wait for a specified time
 */
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Execute agent-browser command
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
 * Execute JavaScript in browser
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
 * Close any existing session
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
 * Extract paper ID from various URL formats
 */
function extractPaperId(input) {
  // Already a paper ID (e.g., 1910.06709)
  if (/^\d{4}\.\d{4,5}(v\d+)?$/.test(input)) {
    return input;
  }
  // Full ar5iv URL
  const ar5ivMatch = input.match(/ar5iv\.(?:labs\.arxiv\.)?org\/(?:html|abs)\/(\d{4}\.\d{4,5}(?:v\d+)?)/);
  if (ar5ivMatch) return ar5ivMatch[1];
  // Full arxiv URL
  const arxivMatch = input.match(/arxiv\.org\/(?:abs|pdf)\/(\d{4}\.\d{4,5}(?:v\d+)?)/);
  if (arxivMatch) return arxivMatch[1];
  return input;
}

/**
 * Scrape a single paper
 */
async function scrapePaper(paperId) {
  paperId = extractPaperId(paperId);
  console.log(`\nScraping paper: ${paperId}...`);

  const url = `${CONFIG.baseUrl}/html/${paperId}`;
  agentBrowser('open', [url]);
  agentBrowser('wait', ['3000']); // Wait for content to render

  const extractScript = `
    JSON.stringify((function() {
      const getText = (sel) => document.querySelector(sel)?.textContent?.trim() || '';
      const getAttr = (sel, attr) => document.querySelector(sel)?.getAttribute(attr) || '';
      const getAllText = (sel) => Array.from(document.querySelectorAll(sel)).map(el => el.textContent?.trim()).filter(Boolean);

      // Extract authors
      const authors = [];
      document.querySelectorAll('${SELECTORS.paper.authorName}').forEach(el => {
        const name = el.textContent?.trim();
        if (name && !authors.includes(name)) authors.push(name);
      });

      // Extract affiliations
      const affiliations = getAllText('${SELECTORS.paper.authorAffiliation}');

      // Extract sections with content
      const sections = [];
      document.querySelectorAll('${SELECTORS.paper.sections}').forEach(section => {
        const titleEl = section.querySelector('${SELECTORS.paper.sectionTitle}');
        const title = titleEl?.textContent?.trim() || '';

        // Get section paragraphs (limited to avoid huge output)
        const paragraphs = [];
        section.querySelectorAll('${SELECTORS.paper.paragraph}').forEach((p, i) => {
          if (i < 5) { // Limit paragraphs per section
            const text = p.textContent?.trim();
            if (text && text.length > 20) paragraphs.push(text.substring(0, 1000));
          }
        });

        if (title) {
          sections.push({ title, paragraphCount: section.querySelectorAll('${SELECTORS.paper.paragraph}').length, preview: paragraphs.slice(0, 2) });
        }
      });

      // Extract bibliography
      const bibliography = [];
      document.querySelectorAll('${SELECTORS.paper.bibliography}').forEach(item => {
        const label = item.querySelector('${SELECTORS.paper.bibLabel}')?.textContent?.trim() || '';
        const text = item.querySelector('${SELECTORS.paper.bibText}')?.textContent?.trim() || item.textContent?.trim() || '';
        if (text) bibliography.push({ label, text: text.substring(0, 500) });
      });

      // Get abstract
      const abstractParagraphs = getAllText('${SELECTORS.paper.abstractText}');
      const abstract = abstractParagraphs.join(' ').substring(0, 2000);

      // Get original arxiv link
      const arxivLink = getAttr('${SELECTORS.paper.originalArxiv}', 'href');

      return {
        paperId: '${paperId}',
        title: getText('${SELECTORS.paper.title}'),
        authors,
        affiliations,
        date: getText('${SELECTORS.paper.date}'),
        abstract,
        sectionCount: sections.length,
        sections,
        bibliographyCount: bibliography.length,
        bibliography: bibliography.slice(0, 20), // Limit bibliography
        arxivUrl: arxivLink,
        ar5ivUrl: '${url}',
      };
    })())
  `.replace(/\n/g, ' ').replace(/\s+/g, ' ');

  const result = evalInline(extractScript);

  try {
    let parsed = JSON.parse(result);
    if (typeof parsed === 'string') parsed = JSON.parse(parsed);
    return parsed;
  } catch (e) {
    console.error('Failed to parse paper data:', e.message);
    return null;
  }
}

/**
 * Scrape a random paper (feeling lucky)
 */
async function scrapeRandomPaper() {
  console.log('\nFetching random paper...');

  const url = `${CONFIG.baseUrl}/feeling_lucky`;
  agentBrowser('open', [url]);
  agentBrowser('wait', ['3000']);

  // Get current URL to find paper ID
  const currentUrl = agentBrowser('get', ['url']);
  const match = currentUrl.match(/\/html\/(\d{4}\.\d{4,5}(?:v\d+)?)/);

  if (match) {
    return await scrapePaper(match[1]);
  }

  console.error('Could not determine paper ID from random redirect');
  return null;
}

/**
 * Scrape multiple papers
 */
async function scrapePapers(paperIds) {
  const results = [];

  for (const paperId of paperIds) {
    try {
      const paper = await scrapePaper(paperId);
      if (paper) results.push(paper);
      await sleep(CONFIG.delay);
    } catch (e) {
      console.error(`Failed to scrape ${paperId}:`, e.message);
    }
  }

  return results;
}

/**
 * Main scraper function
 */
async function scrapeAr5iv(options = {}) {
  const {
    mode = 'paper', // 'paper', 'batch', 'random'
    paperId = null,
    paperIds = [],
    outputFile = 'ar5iv-data.json',
  } = options;

  console.log('Starting ar5iv scraper (agent-browser)...');
  console.log(`Mode: ${mode}`);
  console.log(`Session: ${SESSION_NAME}\n`);

  await closeExistingSession();

  const result = {
    metadata: {
      source: 'ar5iv.labs.arxiv.org',
      scrapedAt: new Date().toISOString(),
      mode,
      scraper: 'agent-browser',
      actionbookActionIds: [
        'https://ar5iv.labs.arxiv.org',
        'https://ar5iv.labs.arxiv.org/html/{paper_id}',
      ],
    },
  };

  try {
    switch (mode) {
      case 'paper':
        if (!paperId) throw new Error('paperId is required for paper mode');
        result.paper = await scrapePaper(paperId);
        break;

      case 'batch':
        if (!paperIds.length) throw new Error('paperIds array is required for batch mode');
        result.papers = await scrapePapers(paperIds);
        break;

      case 'random':
        result.paper = await scrapeRandomPaper();
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
      case '--paper':
        options.paperId = args[++i];
        options.mode = options.mode || 'paper';
        break;
      case '--papers':
        options.paperIds = args[++i].split(',');
        options.mode = 'batch';
        break;
      case '--random':
        options.mode = 'random';
        break;
      case '--output':
        options.outputFile = args[++i];
        break;
      case '--help':
        console.log(`
ar5iv.labs.arxiv.org Scraper (agent-browser version)

Usage: node scraper-agent-browser.js [options]

Options:
  --paper <id>      Scrape single paper by arXiv ID (e.g., 1910.06709)
  --papers <ids>    Scrape multiple papers (comma-separated IDs)
  --random          Scrape a random paper (feeling lucky)
  --output <file>   Output file (default: ar5iv-data.json)
  --help            Show this help

Paper ID formats supported:
  - arXiv ID: 1910.06709, 2301.00001v2
  - ar5iv URL: https://ar5iv.labs.arxiv.org/html/1910.06709
  - arXiv URL: https://arxiv.org/abs/1910.06709

Examples:
  node scraper-agent-browser.js --paper 1910.06709
  node scraper-agent-browser.js --papers 1910.06709,2301.00001,2312.12345
  node scraper-agent-browser.js --random
  node scraper-agent-browser.js --paper 1706.03762 --output attention-paper.json
        `);
        process.exit(0);
    }
  }

  // Default to random if no paper specified
  if (!options.mode && !options.paperId && !options.paperIds?.length) {
    options.mode = 'random';
  }

  scrapeAr5iv(options)
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

module.exports = {
  scrapeAr5iv,
  scrapePaper,
  scrapePapers,
  scrapeRandomPaper,
};
