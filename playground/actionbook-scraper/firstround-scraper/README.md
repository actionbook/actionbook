# First Round Capital Portfolio Scraper

Scraper for extracting First Round Capital's portfolio companies data.

## Data Source

- **URL**: https://www.firstround.com/companies?category=all
- **Selectors verified by**: [Actionbook](https://actionbook.app)
- **Action ID**: `https://www.firstround.com/companies`

## Extracted Data

For each company, the scraper extracts:

| Field | Description |
|-------|-------------|
| `name` | Company name |
| `description` | Company tagline/mission |
| `founders` | Founder names |
| `initialPartnership` | Investment stage (Pre-Seed, Seed, Series A) |
| `categories` | Industry categories |
| `location` | Company location(s) |
| `partner` | First Round partner(s) |
| `websiteUrl` | Company website URL |
| `exitStatus` | Exit status (ACQUIRED, IPO, or null) |

## Usage

```bash
# Install dependencies
npm install

# Run the scraper
npm run scrape
```

## Output

Results are saved to `firstround-companies.json`:

```json
{
  "metadata": {
    "source": "First Round Capital",
    "url": "https://www.firstround.com/companies?category=all",
    "scrapedAt": "2026-01-22T...",
    "totalCompanies": 194,
    "actionbookActionId": "https://www.firstround.com/companies"
  },
  "companies": [
    {
      "name": "Notion",
      "description": "anyone could build the software they need",
      "founders": "Ivan Zhao, Simon Last",
      "initialPartnership": "Seed",
      "categories": "Enterprise",
      "location": "SF Bay Area",
      "partner": "Josh Kopelman",
      "websiteUrl": "https://notion.so",
      "exitStatus": null
    }
  ]
}
```

## Key Selectors (Actionbook Verified)

| Element | Selector |
|---------|----------|
| Company card | `div.company-list-card-small` |
| Card expand button | `button.company-list-card-small__button` |
| Company name | `div.company-list-card-small__button-name` |
| Company description | `div.company-list-card-small__button-statement p` |
| Website link | `a.company-list-card-small__link` |
| Info list | `dl.company-list-company-info__list` |
| Info item | `div.company-list-company-info__item` |

## Notes

- The page uses React/Next.js with dynamic content loading
- Must use `?category=all` URL parameter to load all companies
- Cards need to be clicked to expand and reveal detailed info
- Page may have lazy loading, scraper handles scrolling automatically
