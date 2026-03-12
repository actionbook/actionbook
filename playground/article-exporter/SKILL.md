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

### 1. Check CLI Dependencies

```bash
# Check actionbook CLI (REQUIRED)
if ! command -v actionbook &> /dev/null; then
    echo "❌ actionbook CLI not found"
    echo "Install: npm install -g @actionbookdev/cli"
    exit 1
fi

# Verify version
actionbook --version
```

**Required**:
- ✅ `actionbook` CLI - **Critical** for fetching articles
  - Install: `npm install -g @actionbookdev/cli`
  - Test: `actionbook --version`

**Optional**:
- ⚠️ `obsidian` CLI - **Nice to have** for automatic Obsidian integration
  - Install: `npm install -g obsidian-cli`
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
# Fetch article as Markdown
actionbook browser fetch "$URL" --format markdown --wait-idle > /tmp/article.md
```

**Tips**:
- Always use `--wait-idle` to ensure page fully loads
- Use `--format markdown` for clean text extraction

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

```bash
# Sanitize title for directory name
SAFE_TITLE=$(echo "$TITLE" | sed 's/[/:*?"<>|]//g' | cut -c1-100 | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

# Create output directory
OUTPUT_DIR="$HOME/Work/Write/Articles"
ARTICLE_DIR="$OUTPUT_DIR/$SAFE_TITLE"
mkdir -p "$ARTICLE_DIR/images"
```

**Best Practices**:
- Remove special characters: `/ : * ? " < > |`
- Limit title length to 100 characters
- Trim leading/trailing whitespace

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
# Method 1: Using obsidian-cli (if installed)
if command -v obsidian-cli &> /dev/null; then
    # Get relative path from Obsidian vault root
    VAULT_ROOT="$HOME/Work/Write/Articles"  # Or your vault root
    REL_PATH=$(echo "$ARTICLE_DIR" | sed "s|$VAULT_ROOT/||")

    # Open index.md in Obsidian
    obsidian-cli open "$REL_PATH/index.md"
    echo "✓ Opened in Obsidian: $REL_PATH/index.md"
else
    # Fallback: Use Obsidian URI protocol
    OBSIDIAN_URI="obsidian://open?path=$(echo "$ARTICLE_DIR/index.md" | sed 's/ /%20/g')"

    case "$(uname)" in
        Darwin)  open "$OBSIDIAN_URI" ;;
        Linux)   xdg-open "$OBSIDIAN_URI" ;;
        CYGWIN*|MINGW*|MSYS*) start "$OBSIDIAN_URI" ;;
    esac

    echo "✓ Opening in Obsidian via URI: $OBSIDIAN_URI"
fi
```

**obsidian-cli commands**:

```bash
# Open article index
obsidian-cli open "Article Title/index.md"

# Open with specific vault
obsidian-cli open "Article Title/index.md" --vault "My Vault"

# Open specific section (heading)
obsidian-cli open "Article Title/README.md" --section "Introduction"

# Set default vault (one-time setup)
obsidian-cli set-default --vault "My Vault"
```

**Tips**:
- Install obsidian-cli: `npm install -g obsidian-cli` (optional but recommended)
- Set default vault once: `obsidian-cli set-default --vault "Articles"`
- Then just use: `obsidian-cli open "$REL_PATH/index.md"`

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

```bash
# Create array of URLs
urls=(
  "https://medium.com/@author/post1"
  "https://dev.to/author/post2"
  "https://x.com/user/status/123"
)

# Loop through URLs
for url in "${urls[@]}"; do
    echo "Processing: $url"

    # Execute Steps 1-8 for each URL
    # (Full workflow from above)

    echo "✓ Completed: $url"
    echo ""
done

echo "✓ Batch export completed: ${#urls[@]} articles"
```

**Best Practice**: Test with one article first, then batch process

---

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| **"actionbook: command not found"** | actionbook CLI not installed | `npm install -g @actionbookdev/cli` |
| **Images downloading as 0 bytes** | URL expired or format issue | Try alternative format: `?format=jpg&name=orig` |
| **Translation not working** | AI session issue | Retry translation request, or translate manually |
| **"Directory already exists"** | Article already exported | User decides: overwrite or skip |
| **Fetch timeout** | Slow website | Already using `--wait-idle`, increase timeout if needed |
| **Special chars in title** | Invalid filename characters | Auto-sanitized in Step 3 |

**For detailed troubleshooting**: See `./TROUBLESHOOTING.md`

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
| `$OUTPUT_DIR` | Step 3 | Output base directory | `~/Work/Write/Articles` |
| `$TITLE` | Step 2 | Article title from H1 | Auto-extracted |
| `TARGET_LANGUAGE` | Step 6 | Translation language | User specifies |

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
- Use `--wait-idle` only when needed (some sites load faster without)
- Download images in parallel (advanced: use `xargs -P 4`)

### For Reliability
- Always check file sizes after download (detect 0-byte failures)
- Use `curl -L` to follow redirects
- Add 1-2s delay between image downloads (avoid rate limiting)

### For Quality
- Verify article title extraction before proceeding
- Check image count matches expected count
- Review README.md before translation

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
