# crates.io Scraper

Comprehensive scraper for the Rust package registry (crates.io).

## Data Source

- **URL**: https://crates.io
- **Selectors verified by**: [Actionbook](https://actionbook.app)
- **Action IDs**:
  - `https://crates.io/`
  - `https://crates.io/crates`
  - `https://crates.io/crates/{crate_name}`

## Features

| Mode | Description |
|------|-------------|
| `homepage` | Scrape homepage stats, new crates, most downloaded |
| `list` | Scrape paginated crate list with sorting options |
| `detail` | Scrape single crate's full details |
| `search` | Search crates by keyword |
| `all` | Scrape homepage + top crates + new crates |

## Installation

```bash
npm install
```

## Usage

### Basic Commands

```bash
# Scrape crate list (default: recent downloads, 3 pages)
npm run scrape

# Scrape homepage stats
npm run scrape:homepage

# Scrape top downloaded crates
npm run scrape:top

# Scrape newest crates
npm run scrape:new

# Scrape all data
npm run scrape:all
```

### Advanced Usage

```bash
# Scrape specific crate details
node scraper.js --mode detail --crate tokio

# Search crates
node scraper.js --mode search --search "async runtime"

# Custom list scraping
node scraper.js --mode list --sort downloads --pages 10 --output top-100.json
```

### CLI Options

| Option | Description | Default |
|--------|-------------|---------|
| `--mode` | Scrape mode: homepage, list, detail, search, all | list |
| `--sort` | Sort: downloads, recent-downloads, new, alpha | recent-downloads |
| `--pages` | Max pages to scrape | 3 |
| `--crate` | Crate name (for detail mode) | - |
| `--search` | Search query (for search mode) | - |
| `--output` | Output JSON file | crates-io-data.json |

## Extracted Data

### Homepage Mode

```json
{
  "stats": {
    "downloads": 12345678,
    "crates": 150000
  },
  "newCrates": [...],
  "mostDownloaded": [...]
}
```

### List Mode

For each crate:

| Field | Description |
|-------|-------------|
| `name` | Crate name |
| `version` | Latest version |
| `description` | Crate description |
| `allTimeDownloads` | Total download count |
| `recentDownloads` | Recent download count |
| `updatedAt` | Last update timestamp |
| `links` | Homepage, docs, repository URLs |

### Detail Mode

For a single crate:

| Field | Description |
|-------|-------------|
| `name` | Crate name |
| `version` | Current version |
| `description` | Full description |
| `keywords` | Keyword tags |
| `msrv` | Minimum Supported Rust Version |
| `edition` | Rust edition (2018, 2021, etc.) |
| `license` | License type |
| `lineCount` | Source lines of code |
| `packageSize` | Package size |
| `repository` | GitHub/GitLab URL |
| `documentation` | docs.rs URL |
| `owners` | Maintainer list |
| `categories` | Category tags |
| `stats` | Download statistics |
| `readme` | README content (truncated) |

## Key Selectors (Actionbook Verified)

### List Page

| Element | Selector |
|---------|----------|
| Crate row | `ol.list_ef1bd7ef3 > li` |
| Crate name | `a.name_ee3a027e7` |
| Version | `span.version` |
| Description | `div.description_ee3a027e7` |
| Downloads | `div.downloads_ee3a027e7` |

### Detail Page

| Element | Selector |
|---------|----------|
| Name | `h1.heading_e5aa661bf > span` |
| Version | `h1.heading_e5aa661bf > small` |
| Description | `div.description_e5aa661bf` |
| Keywords | `ul.keywords_e5aa661bf a` |
| License | `div.license_e2a51d261` |
| MSRV | `div.msrv_e2a51d261 span` |

## Sort Options

| Value | Description |
|-------|-------------|
| `downloads` | All-time download count |
| `recent-downloads` | Recent download count (default) |
| `new` | Newest crates first |
| `alpha` | Alphabetical order |
| `recent-updates` | Recently updated first |

## Example Output

```json
{
  "metadata": {
    "source": "crates.io",
    "scrapedAt": "2026-01-22T...",
    "mode": "list"
  },
  "crates": [
    {
      "name": "tokio",
      "version": "1.49.0",
      "description": "An event-driven, non-blocking I/O platform",
      "allTimeDownloadsCount": 500000000,
      "recentDownloadsCount": 50000000,
      "updatedAt": "2026-01-15T...",
      "links": {
        "documentation": "https://docs.rs/tokio",
        "repository": "https://github.com/tokio-rs/tokio"
      }
    }
  ]
}
```

## Notes

- crates.io uses Ember.js with dynamically loaded content
- README content is loaded asynchronously
- Download statistics may include commas and K/M suffixes
- Rate limiting: ~500ms delay between requests recommended
