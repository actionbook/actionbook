# Deep Research

> Analyze any topic, domain, or paper and generate a beautiful HTML report — powered by Actionbook and Claude Code.

All you need is **Claude Code** + **Actionbook CLI**. Everything runs locally on your machine.

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

### Step 3: Add the Deep Research Skill

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

### Step 4: Run Your First Research

Start Claude Code:

```bash
claude
```

Then type:

```
/deep-research:analyze "WebAssembly 2026 ecosystem"
```

Or in natural language:

```
帮我深度研究 WebAssembly 2026 生态并生成报告
```

That's it! The agent will search the web, read sources, generate a report, and open it in your browser.

## Complete Demo: Research an arXiv Paper

Here's a full end-to-end example using only Claude Code:

```
# 1. Start Claude Code (from any directory if you did Option A)
claude

# 2. Ask it to analyze a paper
> /deep-research:analyze "arxiv:2501.12599"

# What happens behind the scenes:
# - Agent opens arXiv Advanced Search via Actionbook browser
# - Uses indexed selectors to search by paper ID
# - Reads the paper from ar5iv.org with verified selectors
# - Fetches supplementary info from HuggingFace, GitHub
# - Generates a json-ui JSON report
# - Renders to HTML and opens in your browser

# 3. The HTML report opens automatically
# You'll see: title, authors, abstract, key contributions,
# method overview, results table, source links — all beautifully formatted
```

**What you'll see:**

```
┌─────────────────────────────────────────────────┐
│  🔬 Deep Research Report  ·  Powered by Actionbook  │
├─────────────────────────────────────────────────┤
│                                                 │
│  📄 Paper: Da Vinci: Elevating Coding Agents    │
│     Authors: ...                                │
│     arXiv: 2501.12599 · Jan 2025               │
│                                                 │
│  ⭐ Key Contributions                           │
│  1. Agent-Environment Interface Design          │
│  2. RepoGraph for Repository Comprehension      │
│  3. State-of-the-art on SWE-bench Verified     │
│                                                 │
│  📊 Results                                     │
│  ┌────────────┬──────────────┐                  │
│  │ Benchmark  │ Score        │                  │
│  ├────────────┼──────────────┤                  │
│  │ SWE-bench  │ 58.6% (+6)  │                  │
│  └────────────┴──────────────┘                  │
│                                                 │
│  🔗 Sources: arxiv, ar5iv, GitHub, HuggingFace  │
└─────────────────────────────────────────────────┘
```

## Command Reference

```
/deep-research:analyze <topic> [options]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `topic` | Yes | — | Any topic, technology, or `arxiv:XXXX.XXXXX` |
| `--lang` | No | `both` | `en`, `zh`, or `both` |
| `--output` | No | `./output/<slug>.json` | Custom output path |

### More Examples

```bash
# Research a technology
/deep-research:analyze "Rust async runtime comparison 2026"

# Analyze an arXiv paper
/deep-research:analyze "arxiv:2601.08521"

# Search by research topic (uses arXiv Advanced Search)
/deep-research:analyze "large language model agent papers 2025"

# Report in Chinese
/deep-research:analyze "大语言模型推理优化" --lang zh

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

1. **Plan**: Decide search strategy — arXiv Advanced Search for academic topics, Google for general topics
2. **Search**: Use `actionbook browser` to search the web, with Actionbook-indexed selectors for known sites
3. **Read**: Visit top sources, extract text via verified selectors
4. **Synthesize**: Organize findings into structured sections
5. **Generate**: Write a json-ui JSON report
6. **Render**: Produce self-contained HTML
7. **View**: Open the report in your browser

## Report Components

Reports use `@actionbookdev/json-ui` components:

| Section | Icon | Description |
|---------|------|-------------|
| Brand Header | — | Actionbook branding |
| Overview | paper | Topic summary |
| Key Findings | star | Numbered core findings |
| Detailed Analysis | bulb | In-depth examination |
| Key Metrics | chart | Numbers and stats |
| Sources | link | Reference links |
| Brand Footer | — | Timestamp and disclaimer |

For academic papers, additional components:
- `PaperHeader` with arXiv metadata
- `AuthorList` with affiliations
- `Formula` for LaTeX equations
- `ResultsTable` with benchmark comparisons

## Customization

### Modify Report Template

Edit `agents/researcher.md` to change default report sections, component usage, research depth, or language defaults.

### Available json-ui Components

See `skills/deep-research/SKILL.md` for the full component catalog (20+ components).

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `actionbook: command not found` | `npm i -g @actionbookdev/cli` |
| `claude: command not found` | `npm i -g @anthropic-ai/claude-code` |
| Browser won't open | `actionbook browser status` — ensure Chromium browser is installed |
| Empty report | Check internet connection, try a simpler topic |
| HTML render fails | The JSON report is saved at `./output/<slug>.json` — you can render it later |
| Skill not found | Ensure SKILL.md is at `~/.claude/skills/deep-research/SKILL.md` |

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
├── .gitignore
└── README.md
```

## License

Apache-2.0
