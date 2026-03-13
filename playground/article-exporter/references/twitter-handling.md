# Twitter/X Long Posts Special Handling

## Problem

Twitter/X long posts have **SEVERE formatting issues** with `fetch` command:

| Issue | Impact | Example |
|-------|--------|---------|
| Lost Markdown structure | No headings, code blocks, lists | `# Title` → flat text |
| Twitter UI noise | First 7 lines are author/stats | `huangserva`, `@user`, `51`, `445` |
| Code blocks unformatted | JSON/code loses ``` markers | Unreadable inline text |
| Trailing junk | Last 10+ lines timestamp/stats | `5:27 PM · Mar 5, 2026`, `622K views` |

## Root Cause

`actionbook browser fetch --format markdown` uses generic text extraction:
- ✅ Works: Medium, Dev.to, OpenAI blog (native Markdown)
- ❌ Breaks: Twitter/X (custom UI, no semantic HTML)

## Solution: AI Reformatting (Mandatory)

**DO NOT use `fetch` alone for Twitter/X. ALWAYS add AI reformatting.**

### Step 1: Fetch Raw Content

```bash
actionbook browser fetch "$URL" --format markdown --wait-hint heavy 2>/dev/null | \
  sed '/^[[:space:]]*$/d;/^\x1b\[/d;/^INFO/d' > /tmp/article_raw.md
```

### Step 2: AI Reformatting (CRITICAL)

**Prompt Template:**

```
Reformat the following Twitter/X long post into proper Markdown:

1. **Clean up Twitter UI**:
   - Remove author name, handle, stats (first 7 lines)
   - Remove timestamp and view count (last 10 lines)

2. **Add Markdown structure**:
   - Main title: `# {title}`
   - Sections: `## Section Name`
   - Lists: Use `-` or `1.`

3. **Format code blocks**:
   ```json / ```typescript / ```bash

4. **Add tables** where appropriate

5. **Preserve content integrity** (don't add/remove info)

---
[Paste /tmp/article_raw.md content]
```

### Step 3: Save Reformatted Content

```bash
# After AI generates reformatted content
cp /tmp/article_reformatted.md "$ARTICLE_DIR/README.md"
```

## Alternative: Snapshot + AI Parsing

For better structure detection:

```bash
actionbook browser open "$URL"
actionbook browser snapshot --format compact > /tmp/snapshot.txt

# Use AI to parse structure from accessibility tree
```

## When to Use Each Method

| Scenario | Method | Reason |
|----------|--------|--------|
| Twitter/X long posts | `fetch` + AI reformat | **ALWAYS** |
| Twitter/X threads | Snapshot + AI parsing | Better tweet separation |
| Medium, blogs | `fetch` alone | Native Markdown works |

---

**Last Updated**: 2026-03-13
