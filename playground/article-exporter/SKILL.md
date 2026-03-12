---
name: article-exporter
description: |
  CRITICAL: Use for exporting web articles to Obsidian with AI translation. Triggers on:
  export article, save article, article archiving, markdown export, web scraping,
  medium export, dev.to export, x article, twitter thread, openai blog,
  obsidian workflow, knowledge base, content archiving,
  导出文章, 保存文章, 文章归档, obsidian 导出, 知识库,
  导出 medium, 导出 x 文章, 网页保存, markdown 导出,
  "how to export articles", "save to obsidian", "export medium article",
  "保存文章到 obsidian", "怎么导出文章"
---

# Article Exporter - Export Articles from Any Website to Obsidian

> **Version:** 0.2.0 | **Last Updated:** 2026-03-12

You are an expert at web content archiving and Obsidian workflow automation. Help users by:
- **Exporting articles**: Fetch and save articles from any website (X, Medium, Dev.to, etc.)
- **Translation**: Translate articles using your current AI session (no API key required)
- **Organization**: Create Obsidian-compatible directory structures with images
- **Troubleshooting**: Fix common export, download, and translation issues

Export articles from any website (X, Medium, Dev.to, OpenAI blog, etc.) to Obsidian-compatible Markdown format with AI translation.

---

## ⚠️ IMPORTANT: Terms of Service Compliance

**This tool is intended for personal knowledge management only.**

### ✅ Acceptable Use

- **Personal archiving**: Save articles you've read for personal reference and learning
- **Knowledge management**: Organize your reading materials in Obsidian
- **Academic research**: Personal notes and annotations for study
- **Offline reading**: Archive content for personal offline access

### ❌ NOT Acceptable Use

- **Large-scale scraping**: Automated bulk downloading of content
- **Commercial use**: Data collection, resale, or commercial redistribution
- **Public mirrors**: Creating publicly accessible copies or databases
- **ToS violations**: Bypassing paywalls, access restrictions, or rate limits
- **Copyright infringement**: Removing attribution or claiming content as your own

### 📜 User Responsibility

**You must comply with the source website's Terms of Service.**

This tool:
- Uses browser automation (similar to manual browsing)
- Preserves original URLs and author attribution
- Does NOT grant rights beyond what the source website allows

**By using this tool, you agree to:**
1. Use exported content for **personal use only**
2. Respect **copyright** and **attribution requirements**
3. Comply with **rate limits** and **ToS** of source websites
4. **NOT** use for commercial purposes or redistribution

### 🛡️ Respecting Content Creators

- ✅ Always keep the original author attribution
- ✅ Preserve the original URL reference
- ✅ Consider supporting premium content creators
- ✅ Use exported content for personal learning only

**If in doubt, ask for permission before exporting content.**

---

## Quick Reference

| Task | Command | When to Use |
|------|---------|-------------|
| **Check dependencies** | `actionbook --version` | Verify CLI is installed |
| **Fetch article** | `actionbook browser fetch <url> --format markdown` | Get article content |
| **Export with images** | Full workflow (see Pattern 1) | Complete export with translation |
| **Custom output** | Use `--output` flag in mkdir | Organize by topic |
| **Translate content** | Use AI session directly | No API key needed |

---

## IMPORTANT: Pre-flight Checks

**Before executing export commands, you MUST:**

### 1. Check CLI Dependencies and Versions

```bash
# Check actionbook CLI (REQUIRED)
if ! command -v actionbook &> /dev/null; then
    echo "❌ actionbook CLI not found"
    echo "Install: npm install -g @actionbookdev/cli"
    exit 1
fi

# Verify version (MUST be >= 0.9.1)
CURRENT_VERSION=$(actionbook --version 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
REQUIRED_VERSION="0.9.1"

if [ -z "$CURRENT_VERSION" ]; then
    echo "⚠️  Cannot detect actionbook version"
elif [ "$(printf '%s\n' "$REQUIRED_VERSION" "$CURRENT_VERSION" | sort -V | head -n1)" != "$REQUIRED_VERSION" ]; then
    echo "❌ actionbook version too old: $CURRENT_VERSION"
    echo "   Required: >= $REQUIRED_VERSION"
    echo ""
    echo "   Upgrade command:"
    echo "   npm install -g @actionbookdev/cli@latest"
    exit 1
else
    echo "✅ actionbook version: $CURRENT_VERSION"
fi

# Check obsidian-cli (REQUIRED for Obsidian integration)
if ! command -v obsidian-cli &> /dev/null; then
    echo "⚠️  obsidian-cli not found (optional but recommended)"
    echo "   Install: npm install -g obsidian-cli"
    echo ""
    echo "   Without obsidian-cli, articles will be saved but not auto-opened in Obsidian"
fi
```

**Required**:
- ✅ `actionbook` CLI >= 0.9.1 - **Critical** for fetching articles
  - Install: `npm install -g @actionbookdev/cli@latest`
  - Test: `actionbook --version`
  - ⚠️ **Version 0.9.1+ required** for `--wait-hint` parameter

**Recommended**:
- ✅ `obsidian-cli` - **Recommended** for automatic Obsidian integration
  - Install: `npm install -g obsidian-cli`
  - Setup: `obsidian-cli set-default --vault "Your Vault Name"`
  - Use case: Auto-open exported articles in Obsidian

### 2. Check Documentation Files

1. Read `./TROUBLESHOOTING.md` for common issues
2. If file read fails: Inform user "本地文档不完整，建议更新相关文件"
3. Still answer based on SKILL.md patterns + built-in knowledge

---

## Key Pattern: Complete Article Export Workflow

**When to use**: User wants to export an article with images and translation

### Step 1: Fetch Article Content

```bash
# Fetch article as Markdown (with log cleaning)
actionbook browser fetch "$URL" --format markdown --wait-hint heavy 2>/dev/null | \
  sed '/^[[:space:]]*$/d;/^\x1b\[/d;/^INFO/d' > /tmp/article.md
```

**Tips**:
- Use `--wait-hint heavy` for pages with dynamic content (Twitter, Medium)
- Use `--format markdown` for clean text extraction
- `2>/dev/null` suppresses stderr logs
- `sed` removes ANSI codes, INFO lines, and empty lines

### Step 2: Extract Metadata

```bash
# Extract title (first H1 heading)
TITLE=$(grep -m 1 "^# " /tmp/article.md | sed 's/^# //')

# Extract all image URLs (filter out data: URLs)
IMAGE_URLS=$(grep -o '!\[[^]]*\]([^)]*)' /tmp/article.md | \
    sed -E 's/!\[[^]]*\]\(([^)]*)\)/\1/' | \
    grep -v '^data:')
```

### Step 3: Create Directory Structure

**IMPORTANT**: Before creating directories, ask the user for the output path.

**AI Assistant Action**:
```
Ask user: "Where should I save the exported article?"

Suggested paths:
- ~/Work/Write/Articles (default)
- ~/Documents/Obsidian/Articles
- ~/Notes/Imported
- (or custom path)

If user doesn't specify, use: ~/Work/Write/Articles
```

```bash
# User specifies OUTPUT_DIR (or use default)
OUTPUT_DIR="${USER_OUTPUT_DIR:-$HOME/Work/Write/Articles}"

# Sanitize title for directory name
SAFE_TITLE=$(echo "$TITLE" | sed 's/[/:*?"<>|]//g' | cut -c1-100 | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

# Create output directory
ARTICLE_DIR="$OUTPUT_DIR/$SAFE_TITLE"
mkdir -p "$ARTICLE_DIR/images"

echo "✓ Output directory: $ARTICLE_DIR"
```

**Best Practices**:
- **Always ask user for output path** - Don't assume default
- Remove special characters: `/ : * ? " < > |`
- Limit title length to 100 characters
- Trim leading/trailing whitespace
- Verify directory is writable before proceeding

### Step 4: Download Images

```bash
counter=1
for url in $IMAGE_URLS; do
    # Detect extension from URL
    ext=$(echo "$url" | grep -oE '\.(jpg|jpeg|png|gif|webp|svg)' || echo ".jpg")

    # Download image with curl
    curl -L -s "$url" -o "$ARTICLE_DIR/images/image_${counter}${ext}"

    # Check if download succeeded (file size > 0)
    if [ ! -s "$ARTICLE_DIR/images/image_${counter}${ext}" ]; then
        echo "⚠️  Failed to download image $counter, trying alternative formats..."
        # Try JPG format
        curl -L -s "${url}?format=jpg&name=orig" -o "$ARTICLE_DIR/images/image_${counter}.jpg"
    fi

    counter=$((counter + 1))
done

echo "✓ Downloaded $(($counter - 1)) images"
```

**Error Handling**:
- Check file size after download (detect 0-byte failures)
- Try alternative URL formats (e.g., `?format=jpg&name=orig` for Twitter)
- Log warnings for failed downloads

### Step 5: Update Image References

```bash
# Replace remote URLs with local paths in markdown
counter=1
for url in $IMAGE_URLS; do
    ext=$(echo "$url" | grep -oE '\.(jpg|jpeg|png|gif|webp|svg)' || echo ".jpg")
    sed -i.bak "s|$url|./images/image_${counter}${ext}|g" /tmp/article.md
    counter=$((counter + 1))
done

# Save updated markdown
cp /tmp/article.md "$ARTICLE_DIR/README.md"
rm /tmp/article.md.bak  # Clean up backup file
```

### Step 6: AI Translation (No API Key Required!)

**When translation is requested**, you (the AI assistant) should:

1. **Read** the `README.md` content
2. **Translate** using your current session (no external API calls)
3. **Write** translated content to `README_<LANG>.md`

**Translation Prompt Template**:

```
Translate the following Markdown article to [TARGET_LANGUAGE] while preserving:
- All Markdown formatting (headings, lists, code blocks, tables)
- Image references exactly as-is: ![alt](./images/image_N.*)
- Links and URLs unchanged
- Code blocks and technical terms in original language
- Proper nouns and brand names in original language

Only output the translated Markdown content, nothing else.

---

[Paste README.md content here]
```

**Supported Languages**:
- `en` - English
- `zh` - Chinese (中文)
- `es` - Spanish (Español)
- `fr` - French (Français)
- `de` - German (Deutsch)
- `ja` - Japanese (日本語)
- `ko` - Korean (한국어)

**Example**:

```bash
# User asks: "Translate to Chinese"
# 1. You read README.md
# 2. You translate it to Chinese using your AI capabilities
# 3. You write README_CN.md
```

### Step 7: Create Navigation Index

```bash
# Detect source website from URL
case "$URL" in
    *x.com*|*twitter.com*) SOURCE="X" ;;
    *medium.com*) SOURCE="Medium" ;;
    *dev.to*) SOURCE="Dev.to" ;;
    *openai.com*) SOURCE="OpenAI Blog" ;;
    *substack.com*) SOURCE="Substack" ;;
    *github.com*) SOURCE="GitHub" ;;
    *) SOURCE=$(echo "$URL" | sed 's|https\?://||' | cut -d/ -f1) ;;
esac

# Create index.md with metadata
cat > "$ARTICLE_DIR/index.md" <<EOF
# $TITLE

> **Export Date**: $(date +%Y-%m-%d)
> **Original URL**: $URL
> **Source**: $SOURCE

---

## 📚 Language Versions

- 🇬🇧 **English**: [[README]]
- 🇨🇳 **中文**: [[README_CN]]  <!-- if translated -->

## 📊 Metadata

| Property | Value |
|----------|-------|
| **Source** | $SOURCE |
| **Images** | $(ls images/ | wc -l) images |
| **Export Tool** | actionbook CLI |
| **Export Date** | $(date +%Y-%m-%d) |

---

**Exported using**: actionbook browser automation + AI assistant
EOF
```

### Step 8: Open in Obsidian (obsidian-cli)

**Complete the loop**: Automatically open the exported article in Obsidian

```bash
# Method 1: Using obsidian-cli (recommended)
if command -v obsidian-cli &> /dev/null; then
    # Get relative path from Obsidian vault root
    # Use the OUTPUT_DIR from Step 3 as vault root
    VAULT_ROOT="$OUTPUT_DIR"
    REL_PATH=$(echo "$ARTICLE_DIR" | sed "s|$VAULT_ROOT/||")

    # Open index.md in Obsidian
    obsidian-cli open "$REL_PATH/index.md"
    echo "✓ Opened in Obsidian: $REL_PATH/index.md"
else
    # Fallback: Open in Finder/Explorer
    echo "⚠️  obsidian-cli not found, opening in file manager instead"
    case "$(uname)" in
        Darwin)  open "$ARTICLE_DIR" ;;
        Linux)   xdg-open "$ARTICLE_DIR" ;;
        CYGWIN*|MINGW*|MSYS*) start "$ARTICLE_DIR" ;;
    esac
    echo "✓ Opened directory: $ARTICLE_DIR"
    echo "   Install obsidian-cli for automatic Obsidian opening:"
    echo "   npm install -g obsidian-cli"
fi
```

**obsidian-cli Setup (First-time only)**:

```bash
# Set default vault (matches the OUTPUT_DIR from Step 3)
obsidian-cli set-default --vault "$(basename "$OUTPUT_DIR")"

# Example:
# If OUTPUT_DIR is ~/Work/Write/Articles
# Then vault name is "Articles"
```

**obsidian-cli Commands**:

```bash
# Open article index
obsidian-cli open "Article Title/index.md"

# Open with specific vault (if you have multiple vaults)
obsidian-cli open "Article Title/index.md" --vault "Articles"

# Open specific section (heading)
obsidian-cli open "Article Title/README.md" --section "Introduction"
```

**Tips**:
- **First-time setup**: Run `obsidian-cli set-default --vault "YourVaultName"` once
- **Vault name**: Use the basename of OUTPUT_DIR (e.g., "Articles" from ~/Work/Write/Articles)
- **Fallback**: If obsidian-cli is not installed, the script opens Finder/Explorer instead
- **Path matching**: The OUTPUT_DIR from Step 3 should match your Obsidian vault root

### Step 9: Report Success

```bash
echo ""
echo "════════════════════════════════════════════════════════════"
echo "✓ Article exported successfully!"
echo ""
echo "📁 Location: $ARTICLE_DIR"
echo "📄 Files:"
echo "     - README.md (original)"
echo "     - README_CN.md (translation, if requested)"
echo "     - index.md (navigation)"
echo "🖼️  Images: $(ls images/ | wc -l) files"
echo ""
echo "✓ Opened in Obsidian"
echo "════════════════════════════════════════════════════════════"
```

---

## Batch Export (Multiple Articles)

**When to use**: User wants to export multiple articles at once

⚠️ **IMPORTANT**: Batch export MUST include rate limiting to comply with ToS and avoid being flagged as a bot.

```bash
# Create array of URLs
urls=(
  "https://medium.com/@author/post1"
  "https://dev.to/author/post2"
  "https://x.com/user/status/123"
)

# Loop through URLs with rate limiting
for url in "${urls[@]}"; do
    echo "Processing: $url"

    # Execute Steps 1-8 for each URL
    # (Full workflow from above)

    echo "✓ Completed: $url"

    # CRITICAL: Rate limiting to avoid ToS violations
    # Wait 3-5 seconds between requests (random to appear more human-like)
    if [ "${url}" != "${urls[-1]}" ]; then  # Don't wait after last URL
        DELAY=$((3 + RANDOM % 3))  # Random delay: 3-5 seconds
        echo "⏱️  Waiting ${DELAY}s before next article (rate limiting)..."
        sleep $DELAY
    fi

    echo ""
done

echo "✓ Batch export completed: ${#urls[@]} articles"
```

**Best Practices**:
- ✅ **Always test with one article first** before batch processing
- ✅ **Add 3-5 second delays** between requests (required for ToS compliance)
- ✅ **Limit batch size** to 5-10 articles per session
- ✅ **Use random delays** to appear more human-like
- ⚠️ **Never remove the sleep delay** - this prevents ToS violations

**Rate Limiting Guidelines**:
| Batch Size | Recommended Delay | Total Time |
|------------|-------------------|------------|
| 5 articles | 3-5 seconds | ~20-30 seconds |
| 10 articles | 4-6 seconds | ~45-60 seconds |
| 20+ articles | **NOT RECOMMENDED** | Consider manual export |

**Why Rate Limiting Matters**:
- Protects you from being flagged as a bot
- Respects website server load
- Complies with Terms of Service
- Maintains tool availability for everyone

---

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| **"actionbook: command not found"** | actionbook CLI not installed | `npm install -g @actionbookdev/cli@latest` |
| **"unknown flag: --wait-hint"** | actionbook version < 0.9.1 | Upgrade: `npm install -g @actionbookdev/cli@latest` |
| **Version check fails** | actionbook too old | Must be >= 0.9.1, upgrade required |
| **Images downloading as 0 bytes** | URL expired or format issue | Try alternative format: `?format=jpg&name=orig` |
| **Translation not working** | AI session issue | Retry translation request, or translate manually |
| **"Directory already exists"** | Article already exported | User decides: overwrite or skip |
| **Fetch timeout** | Slow website | Use `--wait-hint heavy`, increase timeout if needed |
| **Special chars in title** | Invalid filename characters | Auto-sanitized in Step 3 |
| **"obsidian-cli: command not found"** | obsidian-cli not installed | `npm install -g obsidian-cli` (optional) |
| **"Unable to find vault"** | Vault not configured | Run `obsidian-cli set-default --vault "VaultName"` |
| **Batch export blocked/rate limited** | Too fast, flagged as bot | Add 3-5s `sleep` between requests (see Batch Export section) |
| **"Access denied" or 429 errors** | Rate limit exceeded | Wait 5-10 minutes, reduce batch size, add longer delays |

**For detailed troubleshooting**: See `./TROUBLESHOOTING.md`

**For ToS compliance**: See "Terms of Service Compliance" section at the top

---

## Supported Websites

| Website | Auto-Detected Source | Notes |
|---------|---------------------|-------|
| X/Twitter | `X` | Works with status URLs |
| Medium | `Medium` | Handles paywalled articles (if logged in) |
| Dev.to | `Dev.to` | Preserves code blocks |
| OpenAI Blog | `OpenAI Blog` | Technical articles |
| Substack | `Substack` | Newsletter content |
| GitHub | `GitHub` | README and docs |
| **Any website** | Domain name | Universal fallback |

---

## Edge Cases Handled

- **Long titles**: Auto-truncate to 100 chars in Step 3
- **Special characters**: Sanitized in Step 3 (`/ : * ? " < > |` removed)
- **No images**: Steps 4-5 skip gracefully
- **0-byte images**: Auto-retry with alternative formats
- **Relative image URLs**: `curl -L` follows redirects
- **Data URLs**: Filtered out in Step 2 (`grep -v '^data:'`)

---

## Parameters Reference

| Parameter | Used In | Description | Default |
|-----------|---------|-------------|---------|
| `$URL` | Step 1 | Article URL | (required) |
| `$OUTPUT_DIR` | Step 3 | Output base directory | **Ask user** (default: `~/Work/Write/Articles`) |
| `$TITLE` | Step 2 | Article title from H1 | Auto-extracted |
| `TARGET_LANGUAGE` | Step 6 | Translation language | User specifies |
| `CURRENT_VERSION` | Pre-flight | actionbook CLI version | Auto-detected via `--version` |
| `REQUIRED_VERSION` | Pre-flight | Minimum actionbook version | `0.9.1` |

---

## Example Output Structure

```
~/Work/Write/Articles/Prompt Caching - Claude 92% Cache Hit Rate/
├── images/
│   ├── image_1.jpg (42 KB)
│   ├── image_2.jpg (141 KB)
│   └── ... (8 images total)
├── README.md (12 KB - original English)
├── README_CN.md (11 KB - Chinese translation)
└── index.md (2 KB - navigation)

Total: 11 files, ~800 KB
```

---

## When Using This Skill

1. **Check dependencies first** - Ensure `actionbook` CLI is installed
2. **Test with one article** - Verify workflow before batch processing
3. **Use Quick Reference** - Find commands quickly
4. **Customize output directory** - Organize by topic/category
5. **Translate after export** - Allows review before translation
6. **Refer to TROUBLESHOOTING.md** - For common issues

---

## Performance Tips

### For Speed
- Use `--wait-hint light` for static pages, `heavy` for dynamic content
- Download images in parallel (advanced: use `xargs -P 4`)
- Clean logs with `sed` to reduce file size

### For Reliability
- Always check file sizes after download (detect 0-byte failures)
- Use `curl -L` to follow redirects
- Add 1-2s delay between image downloads (avoid rate limiting)
- Redirect stderr: `2>/dev/null` to suppress browser logs

### For Quality
- Verify article title extraction before proceeding
- Check image count matches expected count
- Review README.md before translation
- Remove ANSI codes and INFO lines for clean Markdown

---

## Future Enhancements

Potential improvements for this workflow:

- [ ] Parallel image downloads (faster batch processing)
- [ ] Image optimization (compress before saving)
- [ ] Twitter thread support (multi-tweet articles)
- [ ] Custom markdown templates
- [ ] Automatic Obsidian tagging
- [ ] Export to other formats (PDF, HTML)

---

**Last Updated**: 2026-03-12 | **Version**: 0.2.0
