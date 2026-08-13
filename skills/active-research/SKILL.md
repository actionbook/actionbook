---
name: active-research
description: Deep research and analysis tool. Generates comprehensive HTML reports on any topic, domain, paper, or technology using explicit Actionbook browser sessions.
exclude_tools: group:web
triggers: research, deep-dive, investigate, analyze topic, deep research, generate report
force_tool_turns: 10
---

# Active Research

Research a topic from multiple sources and generate a structured JSON/HTML report. Use only current Actionbook CLI commands for web access.

## Browser Ownership and Acquisition

Classify ownership and install cleanup before the first browser mutation. For a task-owned persistent research profile, use this canonical shell setup:

<!-- executable-active-research-cleanup-smoke:start -->
```bash
#!/usr/bin/env bash
set -Eeuo pipefail

ACTIONBOOK="${ACTIONBOOK:-actionbook}"
SESSION="${SESSION:-active-research}"
PROFILE="${PROFILE:-active-research}"
TAB="${TAB:-t1}"
START_URL="${START_URL:-about:blank}"
HEADLESS="${HEADLESS:-true}"
RELEASE_STATE=owned

cleanup() {
  "$ACTIONBOOK" browser stop --session "$SESSION" >/dev/null 2>&1 || true
}
handle_exit() {
  local status=$?
  trap - EXIT INT TERM
  case "$RELEASE_STATE" in
    released) ;;
    release_failed:*)
      status=${RELEASE_STATE#release_failed:}
      printf 'early browser stop failed with status %d; retrying best-effort terminal cleanup\n' \
        "$status" >&2
      cleanup
      ;;
    acquire_failed:*)
      status=${RELEASE_STATE#acquire_failed:}
      printf 'replacement browser start failed with status %d; retrying best-effort cleanup of the attempted replacement\n' \
        "$status" >&2
      cleanup
      ;;
    *) cleanup ;;
  esac
  exit "$status"
}
handle_signal() {
  PENDING_SIGNAL=$1
  if [[ "$RELEASE_STATE" != releasing && "$RELEASE_STATE" != acquiring ]]; then
    finish_pending_signal
  fi
}
finish_pending_signal() {
  local signal=$PENDING_SIGNAL
  PENDING_SIGNAL=
  trap - EXIT INT TERM
  case "$RELEASE_STATE" in
    released) ;;
    release_failed:*)
      printf 'early browser stop failed with status %d; retrying best-effort cleanup before SIG%s\n' \
        "${RELEASE_STATE#release_failed:}" "$signal" >&2
      cleanup
      ;;
    acquire_failed:*)
      printf 'replacement browser start failed with status %d; retrying best-effort cleanup of the attempted replacement before SIG%s\n' \
        "${RELEASE_STATE#acquire_failed:}" "$signal" >&2
      cleanup
      ;;
    *) cleanup ;;
  esac
  kill "-$signal" "$$"
}
install_cleanup_traps() {
  PENDING_SIGNAL=
  trap 'handle_exit' EXIT
  trap 'handle_signal INT' INT
  trap 'handle_signal TERM' TERM
}
install_cleanup_traps

"$ACTIONBOOK" browser start \
  --session "$SESSION" \
  --profile "$PROFILE" \
  --headless "$HEADLESS" \
  --open-url "$START_URL"
```
<!-- executable-active-research-cleanup-smoke:end -->

The traps clean exactly once on normal exit or command failure and preserve that status. On SIGINT or SIGTERM they clean exactly once, then re-raise the same signal. Cleanup is best-effort; a disposable profile may instead use best-effort `browser close --session "$SESSION"` only when deletion is intended.

For a shared session, never stop or close the session. Register task tab IDs before opening them, then trap only task-owned tab cleanup:

```bash
set -Eeuo pipefail
ACTIONBOOK="${ACTIONBOOK:-actionbook}"
SESSION=lzero-default
OWNED_TABS=(research-source-1 research-source-2)
cleanup_shared() {
  for tab in "${OWNED_TABS[@]}"; do
    "$ACTIONBOOK" browser close-tab --session "$SESSION" --tab "$tab" >/dev/null 2>&1 || true
  done
}
trap 'status=$?; trap - EXIT INT TERM; cleanup_shared; exit "$status"' EXIT
trap 'trap - EXIT INT TERM; cleanup_shared; kill -INT "$$"' INT
trap 'trap - EXIT INT TERM; cleanup_shared; kill -TERM "$$"' TERM

"$ACTIONBOOK" browser open "https://example.com/source-1" \
  --session "$SESSION" --tab "${OWNED_TABS[0]}"
"$ACTIONBOOK" browser open "https://example.com/source-2" \
  --session "$SESSION" --tab "${OWNED_TABS[1]}"
```

All interactive examples below continue the dedicated canonical session and explicitly address `$SESSION` / `$TAB`. A login or manual pause keeps its trap installed and may remain live only under a named owner and resume condition, for example: “task `active-research` owns session `active-research`, tab `t1`; resume when the user confirms MFA completed.”

## Required Tool Boundary

- Use `actionbook search` and `actionbook manual` to discover indexed actions.
- Use `actionbook browser` for every web page interaction.
- Do not use curl, wget, ad-hoc HTTP libraries, WebFetch, or WebSearch.
- Run `actionbook <command> --help` when syntax is uncertain.

## Research Workflow

### 1. Plan Diverse Queries

Create 5-8 query angles: overview, recent developments, technical details, comparisons, evidence/benchmarks, use cases, and criticism. Aim for at least 3-5 independent sources.

### 2. Query Actionbook Manuals First

```bash
"$ACTIONBOOK" search "<keywords>" -d "<domain>"
"$ACTIONBOOK" manual "<site>" "<group>" "<action>"
```

Use returned selectors when available. Otherwise navigate and snapshot the live page:

```bash
"$ACTIONBOOK" browser goto "<url>" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser snapshot --interactive --session "$SESSION" --tab "$TAB"
```

### 3. Search Academic and General Sources

URL-based arXiv search is the preferred academic path:

```bash
"$ACTIONBOOK" browser goto \
  "https://arxiv.org/search/?query=large+language+model+agent&searchtype=all" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser text "#main-container" --session "$SESSION" --tab "$TAB"
```

If advanced form interaction is required, use current explicit commands rather than an unsupported generic batch command:

```bash
"$ACTIONBOOK" browser goto "https://arxiv.org/search/advanced" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser select "#terms-0-field" title --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser fill "#terms-0-term" "large language model agent" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser click "#classification-computer_science" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser click "button[type=submit]" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait navigation --session "$SESSION" --tab "$TAB"
```

Supplement with multiple general-search angles:

```bash
"$ACTIONBOOK" browser goto "https://www.google.com/search?q=<encoded_query>" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser text "#search" --session "$SESSION" --tab "$TAB"

"$ACTIONBOOK" browser goto "https://www.bing.com/search?q=<encoded_query>" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser text "#b_results" --session "$SESSION" --tab "$TAB"
```

Never reconstruct truncated snippet URLs. Snapshot the results and use the real href/ref:

```bash
"$ACTIONBOOK" browser snapshot --interactive --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser click @e5 --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait navigation --session "$SESSION" --tab "$TAB"
```

### 4. Deep-Read Sources

Reuse the owned tab for sequential reading:

```bash
"$ACTIONBOOK" browser goto "<source-url>" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --timeout 15000 \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait condition \
  "document.body.innerText.length > 100" --timeout 5000 \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser text --session "$SESSION" --tab "$TAB"
```

For indexed content, read a focused selector. For unindexed content, snapshot first:

```bash
"$ACTIONBOOK" browser text "article" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser snapshot --interactive --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser describe "main" --nearby --session "$SESSION" --tab "$TAB"
```

If content is incomplete, inspect current error logs and page state:

```bash
"$ACTIONBOOK" browser logs errors --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait condition \
  "document.querySelector('.content') !== null" --timeout 5000 \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser describe ".content" --session "$SESSION" --tab "$TAB"
```

For a dead page, detect once and move on instead of retrying:

```bash
"$ACTIONBOOK" browser wait condition \
  "!document.title.includes('404') && !document.title.includes('Not Found')" \
  --timeout 3000 --session "$SESSION" --tab "$TAB"
```

For anti-bot failures, stop the owned Chrome (retaining profile state), disarm cleanup only after that stop succeeds, then reinstall cleanup before reacquiring the same dedicated identity with start-level stealth enabled. The release state lets a delayed signal recognize a successful explicit stop without stopping the session twice; a failed or uncertain stop remains fail-visible and transfers cleanup ownership to the terminal handler:

<!-- executable-active-research-restart-smoke:start -->
```bash
RELEASE_STATE=releasing
if "$ACTIONBOOK" browser stop --session "$SESSION"; then
  RELEASE_STATE=released
else
  RELEASE_STATE="release_failed:$?"
fi
[[ -z "$PENDING_SIGNAL" ]] || finish_pending_signal
case "$RELEASE_STATE" in
  released) trap - EXIT INT TERM ;;
  release_failed:*) exit "${RELEASE_STATE#release_failed:}" ;;
  *) exit 1 ;;
esac

install_cleanup_traps
RELEASE_STATE=acquiring
if "$ACTIONBOOK" browser start --stealth true --headless "$HEADLESS" \
  --session "$SESSION" --profile "$PROFILE" --open-url "<protected-url>"; then
  RELEASE_STATE=owned
else
  RELEASE_STATE="acquire_failed:$?"
fi
[[ -z "$PENDING_SIGNAL" ]] || finish_pending_signal
case "$RELEASE_STATE" in
  owned) ;;
  acquire_failed:*) exit "${RELEASE_STATE#acquire_failed:}" ;;
  *) exit 1 ;;
esac
"$ACTIONBOOK" browser wait network-idle --session "$SESSION" --tab "$TAB"
```
<!-- executable-active-research-restart-smoke:end -->

### 5. Academic Paper Source Order

1. `arxiv.org/abs/<id>` for metadata and abstract.
2. Hugging Face paper/model pages for community context.
3. The linked repository for implementation evidence.
4. ar5iv HTML for full text when it renders completely.

```bash
"$ACTIONBOOK" browser goto "https://ar5iv.org/html/<arxiv_id>" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --timeout 15000 \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait condition \
  "document.body.innerText.length > 5000" --timeout 10000 \
  --session "$SESSION" --tab "$TAB"
```

If the condition times out, move to the next source rather than treating partial HTML as complete.

### 6. Synthesize

Organize evidence into:

1. Executive summary
2. Key findings
3. Detailed analysis
4. Supporting evidence and limitations
5. Implications
6. Source links

Cross-check consequential claims across independent sources and distinguish source facts from inference.

### 7. Generate and Render the Report

Write `./output/<topic-slug>.json` (or the user-specified path) using the json-ui schema below. Use absolute paths for rendering. Try each renderer until one succeeds:

```bash
node "$(git rev-parse --show-toplevel)/packages/json-ui/dist/cli.js" \
  render /absolute/path/report.json -o /absolute/path/report.html
json-ui render /absolute/path/report.json -o /absolute/path/report.html
npx @actionbookdev/json-ui render /absolute/path/report.json \
  -o /absolute/path/report.html
```

If every renderer fails, report the saved JSON path and the local `npm link` setup command. Opening the rendered local HTML with `open`/`xdg-open` is not a browser-session acquisition and does not replace the installed Actionbook cleanup.

## Failure Handling

| Failure | Response |
|---|---|
| Navigation timeout | Inspect URL, increase `--timeout` once, then move on |
| Missing selector | Refresh `snapshot --interactive`; do not guess stale refs |
| Dynamic content missing | `wait network-idle`, then `wait condition` |
| Page errors | Inspect `logs errors` |
| CAPTCHA/access denied | Restart the owned profile with `browser start --stealth true` |
| 404/dead URL | Skip immediately and seek an independent source |
| Render failure | Preserve JSON and try all three renderer paths |

The installed trap is the terminal cleanup. Do not append a happy-path-only stop at the end, and never stop/close a shared session.

## json-ui Report Template

**IMPORTANT: Always include BrandHeader and BrandFooter.**

```json
{
  "type": "Report",
  "props": { "theme": "auto" },
  "children": [
    {
      "type": "BrandHeader",
      "props": {
        "badge": "Deep Research Report",
        "poweredBy": "Actionbook"
      }
    },
    {
      "type": "Section",
      "props": { "title": "Overview", "icon": "paper" },
      "children": [
        {
          "type": "Prose",
          "props": {
            "content": "Overview of the topic..."
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Key Findings", "icon": "star" },
      "children": [
        {
          "type": "ContributionList",
          "props": {
            "items": [
              {
                "badge": "Finding",
                "title": "...",
                "description": "..."
              }
            ]
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Detailed Analysis", "icon": "bulb" },
      "children": [
        {
          "type": "Prose",
          "props": { "content": "..." }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Key Metrics", "icon": "chart" },
      "children": [
        {
          "type": "MetricsGrid",
          "props": { "metrics": [], "cols": 3 }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Sources", "icon": "link" },
      "children": [
        {
          "type": "LinkGroup",
          "props": { "links": [] }
        }
      ]
    },
    {
      "type": "BrandFooter",
      "props": {
        "timestamp": "YYYY-MM-DDTHH:MM:SSZ",
        "attribution": "Powered by Actionbook",
        "disclaimer": "This report was generated by AI using web sources. Verify critical information independently."
      }
    }
  ]
}
```

### Paper Report Template (for arXiv papers)

When analyzing academic papers, use a richer template with:
- `PaperHeader` (title, arxivId, date, categories)
- `AuthorList` (authors with affiliations)
- `Abstract` (with keyword highlights)
- `ContributionList` (key contributions)
- `MethodOverview` (step-by-step method)
- `ResultsTable` (experimental results)
- `Formula` (key equations, LaTeX)
- `Figure` (paper figures from ar5iv)

### Available json-ui Components

| Component | Use For | Key Props |
|-----------|---------|-----------|
| `BrandHeader` | Report header | `badge`, `poweredBy` |
| `PaperHeader` | Paper metadata | `title`, `arxivId`, `date`, `categories` |
| `AuthorList` | Authors | `authors: [{name, affiliation}]`, `maxVisible` |
| `Section` | Major section | `title`, `icon` (paper/star/bulb/chart/code/link/info/warning) |
| `Prose` | Rich text | `content` (supports **bold**, *italic*, `code`, lists) |
| `Abstract` | Abstract text | `text`, `highlights: ["keyword"]` |
| `ContributionList` | Numbered findings | `items: [{badge, title, description}]` |
| `MethodOverview` | Step-by-step | `steps: [{step, title, description}]` |
| `MetricsGrid` | Key stats | `metrics: [{label, value, trend, suffix}]`, `cols` |
| `ResultsTable` | Data table | `columns`, `rows`, `highlights: [{row, col}]` |
| `Table` | Generic table | `columns: [{key, label}]`, `rows`, `striped`, `compact` |
| `Callout` | Info/tip/warning | `type` (info/tip/warning/important/note), `title`, `content` |
| `Highlight` | Blockquote | `type` (quote/important/warning/code), `text`, `source` |
| `KeyPoint` | Key finding card | `icon`, `title`, `description`, `variant` |
| `CodeBlock` | Code snippet | `code`, `language`, `title`, `showLineNumbers` |
| `Formula` | LaTeX equation | `latex`, `block`, `label` |
| `Figure` | Image(s) | `images: [{src, alt, width}]`, `label`, `caption` |
| `Image` | Single image | `src`, `alt`, `caption`, `width` |
| `DefinitionList` | Term/definition | `items: [{term, definition}]` |
| `LinkGroup` | Source links | `links: [{href, label, icon}]` |
| `Grid` | Grid layout | `cols`, children |
| `Card` | Card container | `padding` (sm/md/lg), `shadow` |
| `TagList` | Tags | `tags: [{label, color, href}]` |
| `BrandFooter` | Footer | `timestamp`, `attribution`, `disclaimer` |

### json-ui Known Pitfalls

| Pitfall | Symptom | Fix |
|---------|---------|-----|
| `MetricsGrid.suffix` as object | `text.replace is not a function` | `suffix` must be a **plain string** |
| `MetricsGrid.value` as number | Render error | `value` must be a **string** (e.g., `"58.5"` not `58.5`) |
| Missing `BrandHeader`/`BrandFooter` | Report looks broken | Always include both |
| `Table` row values as object | `[object Object]` in cells | Row cell values must be **plain strings** |
| Very long Prose content | Truncated render | Split into multiple Prose blocks or use subsections |

### Text Fields

All text fields should use **plain English strings**.

```json
{ "title": "Key Findings" }
```

**Note:** `MetricsGrid` props `value` and `suffix`, and `Table` row cell values must always be plain strings.

## Academic Paper Support

### arXiv Papers

**ar5iv.org HTML** (preferred for reading, but often incomplete for papers < 3 months old):

| Element | Selector | Reliability | Fallback |
|---------|----------|-------------|----------|
| Title | `h1.ltx_title_document` | High | `div.ltx_abstract` |
| Authors | `div.ltx_authors` | High | — |
| Abstract | `div.ltx_abstract` | High | — |
| Full article | `article` | Medium | Use when section selectors fail |
| Sections | `section.ltx_section` | **Low on new papers** | `article` |
| Figures | `figure.ltx_figure` | Medium | — |
| Tables | `table.ltx_tabular` | Medium | — |

**Recommended approach:** Continue the canonical owned session and verify ar5iv content explicitly:

```bash
"$ACTIONBOOK" browser goto "https://ar5iv.org/html/<arxiv_id>" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait network-idle --timeout 15000 \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait condition \
  "document.body.innerText.length > 5000" --timeout 10000 \
  --session "$SESSION" --tab "$TAB"
# If the condition times out, fall back to other sources.
```

### Recommended Source Priority

| Priority | Source | What you get | Reliability |
|----------|--------|-------------|-------------|
| 1 | `arxiv.org/abs/<id>` | Abstract, metadata, submission history | Very high |
| 2 | `huggingface.co/papers/<id>` | Abstract, community, related models | Very high |
| 3 | GitHub repo | README, code, model zoo | High |
| 4 | HuggingFace model card | Training recipe, benchmarks | High |
| 5 | `ar5iv.org/html/<id>` | Full paper HTML | Medium |
| 6 | Google Scholar / Semantic Scholar | Citations, related work | Medium |

### Other Academic Sources

- Google Scholar (`scholar.google.com`) — Actionbook indexed
- Semantic Scholar (`semanticscholar.org`)
- Papers With Code (`paperswithcode.com`)
- Conference proceedings sites

## Quality Guidelines

1. **Breadth**: Research from at least 3-5 diverse sources
2. **Depth**: Read full articles, not just snippets
3. **Accuracy**: Cross-reference facts across sources
4. **Structure**: Use appropriate json-ui components for each content type
5. **Attribution**: Always include source links in the report
6. **Freshness**: Prefer recent sources when relevance is equal
