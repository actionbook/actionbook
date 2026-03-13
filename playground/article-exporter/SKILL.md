---
# === Both Claude Code and Codex read these ===
name: article-exporter
description: >
  Export web articles and social posts to Obsidian Markdown
  with optional AI translation and image downloads. Use when
  the user wants to save, archive, or convert online content
  into clean Markdown. Examples: "export this article",
  "save this Medium post", "archive this blog post",
  "导出这篇文章到 Obsidian", "把这个网页转成 markdown".

# === Claude Code specific (Codex ignores, no side effects) ===
when_to_use: >
  Use when the user wants to save an online article, blog post,
  Medium post, Dev.to post, Substack post, OpenAI blog post,
  or X/Twitter thread into Obsidian or a Markdown knowledge base.
  Also use when the user asks to archive, translate, or convert
  web content into clean Markdown with images downloaded locally.
  Examples: "export this article to Obsidian", "save this Medium
  post", "archive this blog post", "convert this thread to
  markdown", "导出这篇文章", "保存这篇文章到 Obsidian",
  "把这个网页转成 markdown", "翻译并导出这篇文章".
allowed-tools:
  - Read
  - Write
  - Bash(actionbook:*)
  - Bash(curl:*)
  - Bash(obsidian-cli:*)
argument-hint: "[output-dir]"
arguments:
  - output_dir
---

# Article Exporter - Export Articles to Obsidian

> **Version:** 0.4.0 | **Last Updated:** 2026-03-13

You are an expert at web content archiving and Obsidian workflow automation.

## Lessons from Failed Exports

These rules were extracted from real export failures. Each one prevents a specific class of error:

1. **Twitter/X needs AI reformatting** — `fetch` returns flat text because Twitter uses custom UI without semantic HTML. The AI reformatting step reconstructs headings, lists, and code blocks. See `references/twitter-handling.md`.
2. **Ask for output path first** — users have different vault locations. Assuming a default creates files in the wrong place and wastes time moving them.
3. **Check actionbook version >= 0.9.1** — the `--wait-hint` parameter was added in 0.9.1. Without it, dynamic content (SPAs, lazy-loaded pages) returns empty or partial results.
4. **Wait after navigation** — use `--wait-hint heavy` for Twitter, Medium, and other dynamic sites. Without it, the page hasn't finished rendering when content is extracted.
5. **Rate limit batch exports** — 3-5s delay between requests prevents being flagged as a bot (ToS compliance).

## Quick Reference

| Task | Command | Success Criteria |
|------|---------|------------------|
| Check deps | `actionbook --version` | Shows version >= 0.9.1 |
| Fetch article | `actionbook browser fetch <url> --format markdown --wait-hint heavy` | Returns Markdown content |
| Translate | AI session directly | README_CN.md created |
| Open in Obsidian | `obsidian-cli open "path/index.md"` | File opens in Obsidian |

---

## Complete Export Workflow

**Goal:** Export web article to Obsidian directory with images and optional translation

**Success criteria:**
- Article directory created with README.md
- All images downloaded to images/
- index.md navigation file created
- Optional: README_CN.md translation
- Opened in Obsidian (if obsidian-cli available)

---

### Step 1: Fetch Article Content

**Execution:** Direct (Bash)

```bash
# Fetch article as Markdown (with log cleaning)
actionbook browser fetch "$URL" --format markdown --wait-hint heavy 2>/dev/null | \
  sed '/^[[:space:]]*$/d;/^\x1b\[/d;/^INFO/d' > /tmp/article.md
```

**Success criteria:**
- `/tmp/article.md` exists and size > 0 bytes
- First line starts with `#` (H1 heading exists)

**Rules:**
- Use `--wait-hint heavy` for Twitter, Medium, dynamic content
- Use `--wait-hint light` for static blogs
- `2>/dev/null` suppresses stderr logs
- `sed` removes ANSI codes, INFO lines, empty lines

**Twitter/X Special Handling**

Twitter uses non-semantic HTML, so `fetch` output loses all structure (headings become flat text, code blocks disappear). If the URL contains `x.com` or `twitter.com`:
1. Save fetch output to `/tmp/article_raw.md`
2. Use AI to reformat into proper Markdown (see `references/twitter-handling.md`)
3. Then continue from Step 2 with the reformatted content

---

### Step 2: Extract Metadata

**Execution:** Direct (Bash)

```bash
# Extract title (first H1 heading)
TITLE=$(grep -m 1 "^# " /tmp/article.md | sed 's/^# //')

# Extract image URLs (filter out data: URLs)
IMAGE_URLS=$(grep -o '!\[[^]]*\]([^)]*)' /tmp/article.md | \
    sed -E 's/!\[[^]]*\]\(([^)]*)\)/\1/' | \
    grep -v '^data:')
```

**Success criteria:**
- `$TITLE` is non-empty
- `$IMAGE_URLS` count matches expected (use `wc -l`)

---

### Step 3: Ask Output Directory

**Execution:** [human]
**Human checkpoint:** Confirm output location before creating files

Ask user: "Where should I save the exported article?"

Suggested paths:
- `~/Work/Write/Articles` (default)
- `~/Documents/Obsidian/Articles`
- `~/Notes/Imported`
- (or custom path from `$output_dir` argument)

**Success criteria:** User confirms output directory

**Artifacts:** `$OUTPUT_DIR` variable set

---

### Step 4: Create Directory Structure

**Execution:** Direct (Bash)

```bash
# Use argument if provided, otherwise use confirmed path
OUTPUT_DIR="${output_dir:-$USER_CONFIRMED_PATH}"

# Sanitize title for directory name
SAFE_TITLE=$(echo "$TITLE" | sed 's/[/:*?"<>|]//g' | cut -c1-100 | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

# Create output directory
ARTICLE_DIR="$OUTPUT_DIR/$SAFE_TITLE"
mkdir -p "$ARTICLE_DIR/images"
```

**Success criteria:**
- Directory `$ARTICLE_DIR` exists
- Subdirectory `images/` exists
- Directory is writable

**Rules:**
- Remove special characters: `/ : * ? " < > |`
- Limit title length to 100 characters
- Trim leading/trailing whitespace

---

### Step 5: Download Images (Parallel if possible)

**Execution:** Direct (Bash)

```bash
counter=1
for url in $IMAGE_URLS; do
    ext=$(echo "$url" | grep -oE '\.(jpg|jpeg|png|gif|webp|svg)' || echo ".jpg")
    curl -L -s "$url" -o "$ARTICLE_DIR/images/image_${counter}${ext}"

    # Check file size (detect 0-byte failures)
    if [ ! -s "$ARTICLE_DIR/images/image_${counter}${ext}" ]; then
        # Try alternative format (Twitter)
        curl -L -s "${url}?format=jpg&name=orig" -o "$ARTICLE_DIR/images/image_${counter}.jpg"
    fi

    counter=$((counter + 1))
done
```

**Success criteria:**
- All image files exist and size > 0 bytes
- File count matches `$IMAGE_URLS` count

**Rules:**
- Use `curl -L` to follow redirects
- Check file size after download
- Try alternative formats for Twitter images

---

### Step 6: Update Image References

**Execution:** Direct (Bash)

```bash
# Replace remote URLs with local paths
counter=1
for url in $IMAGE_URLS; do
    ext=$(echo "$url" | grep -oE '\.(jpg|jpeg|png|gif|webp|svg)' || echo ".jpg")
    sed -i.bak "s|$url|./images/image_${counter}${ext}|g" /tmp/article.md
    counter=$((counter + 1))
done

# Save updated markdown
cp /tmp/article.md "$ARTICLE_DIR/README.md"
rm /tmp/article.md.bak
```

**Success criteria:**
- `README.md` contains `./images/image_N.*` references
- No remote URLs remain in image links

---

### Step 7: AI Translation (Optional)

**Execution:** Direct (AI session)

**Human checkpoint:** Ask user: "Do you want to translate the article? (y/n)"

If yes:
1. Read `$ARTICLE_DIR/README.md`
2. Translate using AI capabilities (no external API)
3. Write to `$ARTICLE_DIR/README_CN.md` (or other language code)

**Translation Prompt Template:**
```
Translate the following Markdown article to [LANGUAGE] while preserving:
- All Markdown formatting (headings, lists, code blocks, tables)
- Image references exactly as-is: ![alt](./images/image_N.*)
- Links and URLs unchanged
- Code blocks and technical terms in original language

Only output the translated Markdown content.

---
[Paste README.md content]
```

**Success criteria:** Translation file exists and size ≈ original ± 20%

**Supported languages:** en, zh, es, fr, de, ja, ko

---

### Step 8: Create Navigation Index

**Execution:** Direct (Bash)

```bash
# Auto-detect source from URL
case "$URL" in
    *x.com*|*twitter.com*) SOURCE="X" ;;
    *medium.com*) SOURCE="Medium" ;;
    *dev.to*) SOURCE="Dev.to" ;;
    *openai.com*) SOURCE="OpenAI Blog" ;;
    *substack.com*) SOURCE="Substack" ;;
    *github.com*) SOURCE="GitHub" ;;
    *) SOURCE=$(echo "$URL" | sed 's|https\?://||' | cut -d/ -f1) ;;
esac

# Create index.md
cat > "$ARTICLE_DIR/index.md" <<EOF
# $TITLE

> **Export Date**: $(date +%Y-%m-%d)
> **Original URL**: $URL
> **Source**: $SOURCE

## 📚 Language Versions

- 🇬🇧 **English**: [[README]]
- 🇨🇳 **Chinese**: [[README_CN]]  <!-- if translated -->

## 📊 Metadata

| Property | Value |
|----------|-------|
| **Source** | $SOURCE |
| **Images** | $(ls images/ 2>/dev/null | wc -l) images |
| **Export Tool** | actionbook CLI |
| **Export Date** | $(date +%Y-%m-%d) |

---

**Exported using**: actionbook browser automation + AI assistant
EOF
```

**Success criteria:** `index.md` exists with metadata table

---

### Step 9: Open in Obsidian

**Execution:** Direct (Bash)

```bash
if command -v obsidian-cli &> /dev/null; then
    VAULT_ROOT="$OUTPUT_DIR"
    REL_PATH=$(echo "$ARTICLE_DIR" | sed "s|$VAULT_ROOT/||")
    obsidian-cli open "$REL_PATH/index.md"
    echo "✓ Opened in Obsidian: $REL_PATH/index.md"
else
    # Fallback: Open in file manager
    case "$(uname)" in
        Darwin)  open "$ARTICLE_DIR" ;;
        Linux)   xdg-open "$ARTICLE_DIR" ;;
        CYGWIN*|MINGW*|MSYS*) start "$ARTICLE_DIR" ;;
    esac
    echo "⚠️  Install obsidian-cli for automatic opening: npm install -g obsidian-cli"
fi
```

**Success criteria:**
- File opens in Obsidian OR directory opens in file manager
- User sees success message

---

### Step 10: Report Success

**Execution:** Direct (Output)

```bash
echo ""
echo "════════════════════════════════════════════"
echo "✓ Article exported successfully!"
echo ""
echo "📁 Location: $ARTICLE_DIR"
echo "📄 Files:"
echo "     - README.md (original)"
[ -f "$ARTICLE_DIR/README_CN.md" ] && echo "     - README_CN.md (translation)"
echo "     - index.md (navigation)"
echo "🖼️  Images: $(ls images/ 2>/dev/null | wc -l) files"
echo "════════════════════════════════════════════"
```

---

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| **"actionbook: command not found"** | CLI not installed | `npm install -g @actionbookdev/cli@latest` |
| **"unknown flag: --wait-hint"** | Version < 0.9.1 | Upgrade: `npm install -g @actionbookdev/cli@latest` |
| **Twitter format broken** | `fetch` loses structure | Use AI reformatting (see references/twitter-handling.md) |
| **Images 0 bytes** | URL expired | Try `?format=jpg&name=orig` |
| **obsidian-cli not found** | Not installed | `npm install -g obsidian-cli` |
| **Batch export blocked** | Too fast, flagged as bot | Add 3-5s `sleep` between requests |

**Detailed troubleshooting:** See `./references/troubleshooting.md`

---

## Edge Cases Handled

- Long titles → Auto-truncate to 100 chars
- Special characters → Sanitized (`/ : * ? " < > |` removed)
- No images → Steps 5-6 skip gracefully
- 0-byte images → Auto-retry with alternative formats
- Data URLs → Filtered out in Step 2

---

## When Using This Skill

1. **Check dependencies first** — `actionbook --version >= 0.9.1`
2. **Test with one article** — Verify before batch processing
3. **Twitter/X requires special handling** — See references/twitter-handling.md
4. **Respect ToS** — Personal use only, rate limit batch exports

---

## References (Progressive Disclosure)

For detailed documentation, see:

- `./references/twitter-handling.md` — Twitter/X special handling (AI reformatting)
- `./references/batch-export.md` — Batch export with rate limiting
- `./references/troubleshooting.md` — Detailed troubleshooting guide
- `./references/obsidian-setup.md` — obsidian-cli setup and configuration
- `./references/supported-websites.md` — Complete website compatibility list

---

**Last Updated**: 2026-03-13 | **Version**: 0.4.0
