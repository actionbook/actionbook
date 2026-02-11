# Deep Research

> Analyze any topic, domain, or paper and generate a beautiful HTML report — powered by Actionbook and Claude Code.

All you need is **Claude Code** + **Actionbook CLI** + **json-ui**. Everything runs locally on your machine.

## Demo Gallery

### Demo 1: Elon Musk's X.com Posting Activity

> `/deep-research:analyze "Research how many tweets Elon Musk posted today on X.com"`

The agent logged into X.com via Actionbook browser, queried `from:elonmusk since:2026-02-09`, scrolled through all results, and produced a comprehensive activity report:

**What the agent did:**
1. Queried Actionbook API for X.com selectors
2. Opened X.com Advanced Search via `actionbook browser`
3. Scrolled to exhaustion to capture all tweets
4. Counted 38+ original tweets/replies + 2 retweets
5. Identified 3 distinct posting bursts and 6 topic categories
6. Generated bilingual (EN/ZH) json-ui report

**Report highlights:**

| Metric | Value |
|--------|-------|
| Total Posts (as of 12:30 UTC) | 38+ tweets/replies |
| Peak Rate | ~1 tweet per 3 min |
| Most Viewed | "Grok" (Tim Pool quote) — 21M views |
| Top Topics | Epstein/Bannon (7), Politics (8), Grok/xAI (6) |
| Posting Pattern | 3 bursts with 3-4h dormant gaps |

Output: [`output/musk-tweets-2026-02-09.json`](output/musk-tweets-2026-02-09.json)

---

### Demo 2: WebAssembly 2026 Ecosystem Deep Dive

> `/deep-research:analyze "WebAssembly 2026 ecosystem"`

The agent combined **arXiv Advanced Search** (academic papers) with **Google** (industry sources) to produce a comprehensive ecosystem report:

**What the agent did:**
1. Queried Actionbook API for arXiv Advanced Search selectors (40+ form fields)
2. Searched arXiv: title="WebAssembly", CS category, date range 2025-01 to 2026-02 → **20 papers found**
3. Searched Google for industry sources (WasmCon 2025, Fermyon, The New Stack)
4. Deep-read 8+ sources via `actionbook browser`
5. Synthesized academic + industry perspectives into a single report

**Report highlights:**

| Metric | Value |
|--------|-------|
| Spec Version | WebAssembly 3.0 (published Feb 6, 2026) |
| WASI Version | 0.3.0 (WASIp3, imminent) |
| arXiv Papers Found | 20 (CS category, 2025-2026) |
| Research Clusters | Serverless/Edge (8), Security (5), IoT (2), Tooling (3), Other (2) |
| Key Runtimes | Wasmtime, Wasmer, WasmEdge, WAMR |

**Key finding:** Wasm is already everywhere — most users don't realize it. The academic consensus: Wasm excels at cold starts and workload density vs containers.

Output: [`output/webassembly-2026-ecosystem-v2.json`](output/webassembly-2026-ecosystem-v2.json)

---

## Why Actionbook?

Traditional AI tools (WebFetch, WebSearch) can only do simple keyword searches and read raw HTML. Actionbook is different — it **indexes website UI structures** and gives AI agents verified selectors to operate complex web forms.

**Example: arXiv Advanced Search**

Actionbook has indexed the entire arXiv Advanced Search form (40+ selectors). This means the AI agent can:

| What the agent can do | How |
|-----------------------|-----|
| Search by Title, Author, or Abstract separately | Select field via `#terms-0-field` dropdown |
| Filter to Computer Science papers only | Click `#classification-computer_science` checkbox |
| Restrict to papers from 2025-2026 | Set date range via `#date-from_date` / `#date-to_date` |
| Add multiple search terms with boolean logic | Click "Add another term +" button |

**Example: X.com (Twitter) Search**

The agent can log into X.com, use advanced search operators (`from:user since:date`), scroll through results, and extract engagement metrics — all via Actionbook browser automation.

None of this is possible with WebFetch or WebSearch — they can only send a single keyword query.

## Quick Start (from Zero)

### Prerequisites

- **Node.js 18+** (check: `node --version`)
- A Chromium-based browser (Chrome, Brave, Edge, Arc)
- An Anthropic API key

### Step 1: Install Claude Code

```bash
npm install -g @anthropic-ai/claude-code
```

Verify:

```bash
claude --version
```

### Step 2: Install Actionbook CLI

```bash
npm install -g @actionbookdev/cli
```

Verify:

```bash
actionbook --version
actionbook browser status
```

### Step 3: Install json-ui (Report Renderer)

```bash
npm install -g @actionbookdev/json-ui
```

This provides the `json-ui` CLI that converts JSON reports into self-contained HTML files.

Verify:

```bash
json-ui --version
```

### Step 4: Add the Deep Research Skill

**Option A: One-command install via `npx skills` (recommended)**

```bash
npx skills add actionbook/actionbook --skill deep-research -g
```

This installs the skill globally — it works in **any directory** with Claude Code.

**Option B: Install from local clone**

If you've already cloned this repo:

```bash
npx skills add ./playground/deep-research -g
```

**Option C: Manual copy**

```bash
mkdir -p ~/.claude/skills/deep-research
cp playground/deep-research/skills/deep-research/SKILL.md ~/.claude/skills/deep-research/SKILL.md
```

### Step 5: Run Your First Research

Start Claude Code:

```bash
claude
```

Then type:

```
/deep-research:analyze "WebAssembly 2026 ecosystem"
```

Or in natural language (supports English and Chinese):

```
Research the WebAssembly 2026 ecosystem and generate a report
```

That's it! The agent will search the web, read sources, generate a report, and open it in your browser.

## Command Reference

```
/deep-research:analyze <topic> [options]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `topic` | Yes | — | Any topic, technology, URL, or `arxiv:XXXX.XXXXX` |
| `--lang` | No | `both` | `en`, `zh`, or `both` |
| `--output` | No | `./output/<slug>.json` | Custom output path |

### Topic Detection

| Pattern | Type | Strategy |
|---------|------|----------|
| `arxiv:XXXX.XXXXX` | Paper | arXiv Advanced Search + ar5iv deep read |
| Academic keywords | Academic topic | arXiv Advanced Search + Google |
| URL | Specific page | Actionbook browser fetch and analyze |
| General text | Topic research | Google search + arXiv if relevant |

### More Examples

```bash
# Research a specific website's data (like X.com, Reddit, HackerNews)
/deep-research:analyze "Research how many tweets Elon Musk posted today on X.com"

# Deep dive a technology ecosystem
/deep-research:analyze "WebAssembly 2026 ecosystem"

# Analyze an arXiv paper
/deep-research:analyze "arxiv:2501.12599"

# Search by research topic (uses arXiv Advanced Search)
/deep-research:analyze "large language model agent papers 2025"

# Report in Chinese only
/deep-research:analyze "LLM inference optimization" --lang zh

# Custom output path
/deep-research:analyze "RISC-V ecosystem" --output ./reports/riscv.json
```

## How It Works

```
┌──────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────┐
│  Claude   │────▶│  Actionbook  │────▶│  Web Pages   │────▶│ Extract  │
│  Code     │     │  Browser CLI │     │  (multiple)  │     │ Content  │
└──────────┘     └──────────────┘     └──────────────┘     └─────┬────┘
      │                                                           │
      │          ┌──────────────┐     ┌──────────────┐           │
      ├─────────▶│  Actionbook  │     │ arXiv Adv.   │           │
      │          │  search/get  │────▶│ Search Form  │──────────▶│
      │          │  (selectors) │     │ (40+ fields) │           │
      │          └──────────────┘     └──────────────┘           │
      │                                                           │
┌──────────┐     ┌──────────────┐     ┌──────────────┐           │
│  Open in │◀────│   json-ui    │◀────│  Write JSON  │◀──────────┘
│  Browser │     │   render     │     │  Report      │  Synthesize
└──────────┘     └──────────────┘     └──────────────┘
```

### Workflow Steps

| Step | Action | Description |
|------|--------|-------------|
| 1 | **Plan** | Decide search strategy based on topic type |
| 2 | **Query Actionbook API** | Get verified selectors for known sites (arXiv, X.com, etc.) BEFORE browsing |
| 3 | **arXiv Advanced Search** | Multi-field academic search using Actionbook selectors (if topic is academic) |
| 4 | **Google / Bing** | Supplement with blogs, news, code, non-academic sources |
| 5 | **Deep Read** | Visit top sources, extract content via verified selectors |
| 6 | **Synthesize** | Organize findings into structured sections |
| 7 | **Generate** | Write a json-ui JSON report (bilingual EN/ZH) |
| 8 | **Render** | Produce self-contained HTML via `@actionbookdev/json-ui` |
| 9 | **View** | Open the report in your browser |
| 10 | **Close** | Close the Actionbook browser session |

## Features

### Bilingual Reports (EN/ZH)

All reports are bilingual by default. The Chinese content follows quality guidelines to ensure natural, native Chinese writing — not machine translation.

Key principles:
- Chinese text is written independently, not translated from English
- Active voice preferred over passive voice
- Technical terms with established Chinese names use Chinese; others keep English
- Short sentences preferred (Chinese readers prefer concise phrasing)

### Actionbook Browser Automation

The skill uses `actionbook browser` CLI commands exclusively (not WebFetch/WebSearch):

```bash
actionbook browser open <url>        # Navigate to page
actionbook browser text [selector]   # Extract text content
actionbook browser click <selector>  # Click element
actionbook browser snapshot          # Get accessibility tree
actionbook browser close             # Close browser
```

### json-ui Report Components

Reports use 20+ `@actionbookdev/json-ui` components:

| Component | Use For |
|-----------|---------|
| `BrandHeader` / `BrandFooter` | Actionbook branding |
| `Section` | Major report sections with icons |
| `Prose` | Rich text content (Markdown) |
| `ContributionList` | Numbered key findings |
| `MetricsGrid` | Key stats dashboard |
| `Table` | Data tables |
| `LinkGroup` | Source links |
| `Callout` | Important notes / warnings |
| `CodeBlock` | Code snippets |
| `Formula` | LaTeX equations (for papers) |

See `skills/deep-research/SKILL.md` for the full component catalog.

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `actionbook: command not found` | `npm i -g @actionbookdev/cli` |
| `json-ui: command not found` | `npm i -g @actionbookdev/json-ui` |
| `claude: command not found` | `npm i -g @anthropic-ai/claude-code` |
| Browser won't open | `actionbook browser status` — ensure Chromium browser is installed |
| Empty report | Check internet connection, try a simpler topic |
| HTML render fails | The JSON report is saved at `./output/<slug>.json` — install json-ui and render manually: `json-ui render <file>.json -o <file>.html` |
| `[object Object]` in HTML | Update json-ui: `npm i -g @actionbookdev/json-ui@latest` (fixed in v0.1.0+) |
| Skill not found | Ensure SKILL.md is at `~/.claude/skills/deep-research/SKILL.md` |
| Chinese text reads like machine translation | The skill includes quality guidelines — re-run to regenerate |

## Dependencies

This skill requires two npm packages installed globally:

| Package | npm Name | Purpose | Install |
|---------|----------|---------|---------|
| Actionbook CLI | `@actionbookdev/cli` | Browser automation, web search, selector queries | `npm i -g @actionbookdev/cli` |
| json-ui | `@actionbookdev/json-ui` | JSON report → self-contained HTML rendering | `npm i -g @actionbookdev/json-ui` |

**One-command install:**

```bash
npm install -g @actionbookdev/cli @actionbookdev/json-ui
```

These dependencies are also declared in `.claude-plugin/plugin.json` under the `dependencies` and `setup` fields, enabling future auto-install support in Claude Code skill managers.

## Project Structure

```
playground/deep-research/
├── .claude-plugin/
│   ├── plugin.json              # Plugin manifest
│   └── marketplace.json         # Marketplace metadata
├── .mcp.json                    # Actionbook MCP server config
├── skills/
│   └── deep-research/
│       └── SKILL.md             # Main skill definition (core logic)
├── commands/
│   └── analyze.md               # /deep-research:analyze command
├── agents/
│   └── researcher.md            # Research agent (sonnet, Bash+Read+Write)
├── examples/
│   └── sample-report.json       # Sample json-ui report
├── output/                      # Generated reports (gitignored)
│   ├── musk-tweets-2026-02-09.json
│   ├── webassembly-2026-ecosystem-v2.json
│   └── ...
├── .gitignore
└── README.md
```

## License

Apache-2.0
