# Deep Research

> Analyze any topic, domain, or paper and generate a beautiful HTML report — powered by Actionbook CLI and json-ui.

```
/deep-research:analyze "WebAssembly 2026 ecosystem"
```

## Features

- **Actionbook CLI powered** — Uses `actionbook browser` for all web browsing and content extraction
- **Beautiful HTML reports** — Rendered via `@actionbookdev/json-ui` with light/dark theme, responsive layout
- **Bilingual** — Reports support English, Chinese, or both
- **Academic papers** — Understands arXiv IDs, fetches from ar5iv.org with structured selectors
- **Pure local** — Runs entirely on your machine via Claude Code

## Quick Start

### Prerequisites

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI installed
- [Actionbook CLI](https://www.npmjs.com/package/@actionbookdev/cli) installed (`npm i -g @actionbookdev/cli`)
- A Chromium-based browser (Chrome, Brave, Edge, Arc)

### Step 1: Install Claude Code

```bash
npm install -g @anthropic-ai/claude-code
```

### Step 2: Install Actionbook CLI

```bash
npm install -g @actionbookdev/cli
```

Verify installation:

```bash
actionbook browser status
```

### Step 3: Add the Skill

**Option A: Use from this repository**

```bash
cd playground/deep-research
claude
```

Claude Code auto-detects the `.claude-plugin/plugin.json` in the working directory.

**Option B: Copy to your project**

Copy the `playground/deep-research/` directory into your project, then start Claude Code from that directory.

### Step 4: Run

```bash
# In Claude Code:
/deep-research:analyze "WebAssembly 2026 ecosystem"
```

The agent will:
1. Search the web using `actionbook browser`
2. Read top sources and extract content
3. Generate a structured JSON report
4. Render to HTML via json-ui
5. Open the report in your browser

## Command Reference

```
/deep-research:analyze <topic> [options]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `topic` | Yes | — | Any topic, technology, or `arxiv:XXXX.XXXXX` |
| `--lang` | No | `both` | `en`, `zh`, or `both` |
| `--output` | No | `./output/<slug>.json` | Custom output path |

### Examples

```bash
# Research a technology
/deep-research:analyze "Rust async runtime comparison 2026"

# Analyze an arXiv paper
/deep-research:analyze "arxiv:2601.08521"

# Report in Chinese
/deep-research:analyze "大语言模型推理优化" --lang zh

# Custom output
/deep-research:analyze "RISC-V ecosystem" --output ./reports/riscv.json
```

## How It Works

```
┌──────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────┐
│  Claude   │────▶│  Actionbook  │────▶│  Web Pages   │────▶│ Extract  │
│  Code     │     │  Browser CLI │     │  (multiple)  │     │ Content  │
└──────────┘     └──────────────┘     └──────────────┘     └─────┬────┘
                                                                  │
┌──────────┐     ┌──────────────┐     ┌──────────────┐           │
│  Open in │◀────│   json-ui    │◀────│  Write JSON  │◀──────────┘
│  Browser │     │   render     │     │  Report      │  Synthesize
└──────────┘     └──────────────┘     └──────────────┘
```

1. **Search**: Agent uses `actionbook browser open` to search Google/Bing
2. **Collect**: Extracts URLs and snippets from search results
3. **Read**: Visits top sources, extracts text via `actionbook browser text`
4. **Synthesize**: Organizes findings into structured sections
5. **Generate**: Writes a json-ui JSON report
6. **Render**: `npx @actionbookdev/json-ui render report.json` produces HTML
7. **View**: Opens the HTML report in your default browser

## Report Structure

Reports are generated in `@actionbookdev/json-ui` format with these sections:

| Section | Icon | Description |
|---------|------|-------------|
| Brand Header | — | Actionbook branding |
| Overview | paper | Topic summary |
| Key Findings | star | Numbered core findings |
| Detailed Analysis | bulb | In-depth examination |
| Key Metrics | chart | Numbers and stats |
| Sources | link | Reference links |
| Brand Footer | — | Timestamp and disclaimer |

For academic papers, additional components are used:
- `PaperHeader` with arXiv metadata
- `AuthorList` with affiliations
- `Formula` for LaTeX equations
- `ResultsTable` with performance comparisons

## Rendering the Sample Report

To preview the sample report without running a full research:

```bash
npx @actionbookdev/json-ui render examples/sample-report.json
```

This generates an HTML file and opens it in your browser.

## Customization

### Modify Report Template

Edit `agents/researcher.md` to change:
- Default report sections
- json-ui component usage
- Research depth (number of sources)
- Language defaults

### Add Custom Components

The full list of json-ui components is available in `skills/deep-research/SKILL.md`. Add any component to the JSON template in `agents/researcher.md`.

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `actionbook: command not found` | Run `npm i -g @actionbookdev/cli` |
| Browser won't open | Check `actionbook browser status`. Ensure a Chromium browser is installed. |
| `json-ui: command not found` | Use `npx @actionbookdev/json-ui@latest render` instead |
| Empty report | Verify internet connection. Try a simpler topic. |
| Permission denied | Check `.claude/settings.local.json` has the right permissions |
| Report not bilingual | Add `--lang both` or ensure the agent template uses i18n objects |

## Project Structure

```
playground/deep-research/
├── .claude-plugin/
│   └── plugin.json              # Plugin manifest
├── .claude/
│   └── settings.local.json     # Permissions for actionbook/json-ui
├── .mcp.json                   # Actionbook MCP server config
├── skills/
│   └── deep-research/
│       └── SKILL.md            # Main skill definition
├── commands/
│   └── analyze.md              # /deep-research:analyze command
├── agents/
│   └── researcher.md           # Research agent (sonnet, Bash+Read+Write)
├── examples/
│   └── sample-report.json      # Sample json-ui report
├── .gitignore
└── README.md
```

## License

Apache-2.0
