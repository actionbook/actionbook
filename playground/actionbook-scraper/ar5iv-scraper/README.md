# ar5iv Scraper

Scraper for ar5iv.labs.arxiv.org - arXiv papers as HTML5 web pages.

## Data Source

- **URL**: https://ar5iv.labs.arxiv.org
- **Selectors verified by**: [Actionbook](https://actionbook.app)
- **Action IDs**:
  - `https://ar5iv.labs.arxiv.org`
  - `https://ar5iv.labs.arxiv.org/html/{paper_id}`

## Features

| Mode | Description |
|------|-------------|
| `paper` | Scrape single paper by arXiv ID |
| `batch` | Scrape multiple papers |
| `random` | Scrape a random paper (feeling lucky) |

## Usage

```bash
# Scrape a specific paper
node scraper-agent-browser.js --paper 1910.06709

# Scrape multiple papers
node scraper-agent-browser.js --papers 1910.06709,1706.03762,2301.00001

# Scrape a random paper
node scraper-agent-browser.js --random
npm run scrape:random

# Custom output file
node scraper-agent-browser.js --paper 1706.03762 --output attention-paper.json
```

## Paper ID Formats

The scraper accepts various input formats:

| Format | Example |
|--------|---------|
| arXiv ID | `1910.06709`, `2301.00001v2` |
| ar5iv URL | `https://ar5iv.labs.arxiv.org/html/1910.06709` |
| arXiv URL | `https://arxiv.org/abs/1910.06709` |

## Extracted Data

For each paper:

| Field | Description |
|-------|-------------|
| `paperId` | arXiv paper ID |
| `title` | Paper title |
| `authors` | List of author names |
| `affiliations` | Author affiliations |
| `date` | Publication/submission date |
| `abstract` | Paper abstract |
| `sectionCount` | Number of sections |
| `sections` | Section titles and previews |
| `bibliographyCount` | Number of references |
| `bibliography` | Reference list (limited) |
| `arxivUrl` | Original arXiv URL |
| `ar5ivUrl` | ar5iv HTML URL |

## Key Selectors (Actionbook Verified)

| Element | Selector |
|---------|----------|
| Title | `h1.ltx_title_document` |
| Authors | `span.ltx_personname` |
| Affiliation | `span.ltx_contact.ltx_role_affiliation` |
| Date | `div.ltx_dates` |
| Abstract | `div.ltx_abstract p.ltx_p` |
| Sections | `section.ltx_section` |
| Section title | `h2.ltx_title_section` |
| Paragraph | `div.ltx_para p.ltx_p` |
| Bibliography | `ul.ltx_biblist li.ltx_bibitem` |

## Example Output

```json
{
  "metadata": {
    "source": "ar5iv.labs.arxiv.org",
    "scrapedAt": "2026-01-22T...",
    "mode": "paper",
    "scraper": "agent-browser"
  },
  "paper": {
    "paperId": "1706.03762",
    "title": "Attention Is All You Need",
    "authors": ["Ashish Vaswani", "Noam Shazeer", "..."],
    "abstract": "The dominant sequence transduction models...",
    "sectionCount": 7,
    "sections": [
      {
        "title": "1 Introduction",
        "paragraphCount": 5,
        "preview": ["..."]
      }
    ],
    "bibliographyCount": 42,
    "arxivUrl": "https://arxiv.org/abs/1706.03762",
    "ar5ivUrl": "https://ar5iv.labs.arxiv.org/html/1706.03762"
  }
}
```

## Famous Papers to Try

```bash
# Attention Is All You Need (Transformer)
node scraper-agent-browser.js --paper 1706.03762

# BERT
node scraper-agent-browser.js --paper 1810.04805

# GPT-2
node scraper-agent-browser.js --paper 1901.00001

# ResNet
node scraper-agent-browser.js --paper 1512.03385
```

## Notes

- ar5iv converts LaTeX papers to HTML5 using LaTeXML
- Not all papers are perfectly converted (some may have rendering issues)
- The scraper limits output size to avoid huge JSON files
- Use `--random` to discover interesting papers
