---
name: analyze
description: Analyze any topic, domain, paper, or technology and generate a comprehensive HTML report
agent: researcher
---

# /deep-research:analyze

Deep-dive into any subject and generate a beautiful, structured HTML report.

## Usage

```
/deep-research:analyze <topic>
/deep-research:analyze <topic> --lang zh
/deep-research:analyze <topic> --output ./reports/my-report.json
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `topic` | Yes | - | The subject to research (any text) |
| `--lang` | No | `both` | Language: `en`, `zh`, or `both` (bilingual) |
| `--output` | No | `./output/<topic-slug>.json` | Output path for JSON report |

## Examples

```
# Research a technology
/deep-research:analyze "WebAssembly 2026 ecosystem"

# Analyze an arXiv paper
/deep-research:analyze "arxiv:2601.08521"

# Research in Chinese
/deep-research:analyze "Rust async runtime comparison" --lang zh

# Custom output path
/deep-research:analyze "LLM inference optimization" --output ./reports/llm-inference.json
```

## Workflow

1. **Parse topic**: Identify if it's a paper (arxiv ID), technology, trend, or general topic
2. **Plan search strategy**: Generate 5-8 diverse search queries
3. **Browse & discover**: Use `actionbook browser` to search the web
4. **Collect URLs**: Extract relevant source URLs from search results
5. **Deep read**: Visit top sources, extract text content
6. **For papers**: Use ar5iv.org HTML selectors for structured extraction
7. **Synthesize**: Organize findings into structured report sections
8. **Generate JSON**: Write json-ui format report
9. **Render HTML**: Run `npx @actionbookdev/json-ui render <file.json>`
10. **Open report**: Launch in default browser

## Topic Detection

| Pattern | Type | Strategy |
|---------|------|----------|
| `arxiv:XXXX.XXXXX` | Paper | Fetch from ar5iv, analyze paper |
| `doi:10.XXX/...` | Paper | Resolve DOI, fetch paper |
| URL | Specific page | Fetch and analyze the page |
| General text | Topic research | Multi-source web research |

## Output

A self-contained HTML file rendered from json-ui, including:

- Actionbook branded header
- Structured sections with icons
- Key findings and metrics
- Data tables and charts
- Source references with links
- Bilingual content (if `--lang both`)
- Light/dark theme support
- Print-optimized layout

## Error Handling

| Error | Response |
|-------|----------|
| Empty topic | "Please provide a topic to research" |
| Browser unavailable | "Cannot start browser. Run `actionbook browser status` to check." |
| No sources found | "No relevant sources found. Try rephrasing the topic." |
| Render failed | "json-ui render failed. JSON saved at: {path}" |
