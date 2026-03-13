# Batch Export (Multiple Articles)

⚠️ **CRITICAL**: Batch export MUST include rate limiting to comply with ToS.

## Rate Limiting Requirements

| Batch Size | Recommended Delay | Total Time |
|------------|-------------------|------------|
| 5 articles | 3-5 seconds | ~20-30 seconds |
| 10 articles | 4-6 seconds | ~45-60 seconds |
| 20+ articles | **NOT RECOMMENDED** | Manual export |

## Complete Batch Script

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

    # Execute full workflow for each URL
    # (Steps 1-10 from main SKILL.md)

    echo "✓ Completed: $url"

    # CRITICAL: Rate limiting (NEVER remove this)
    if [ "${url}" != "${urls[-1]}" ]; then
        DELAY=$((3 + RANDOM % 3))  # Random 3-5s
        echo "⏱️  Waiting ${DELAY}s (rate limiting)..."
        sleep $DELAY
    fi

    echo ""
done

echo "✓ Batch export completed: ${#urls[@]} articles"
```

## Why Rate Limiting Matters

- Protects from being flagged as bot
- Respects website server load
- Complies with Terms of Service
- Maintains tool availability

## Best Practices

✅ **Always test with one article first**
✅ **Add 3-5 second delays** between requests
✅ **Limit batch size** to 5-10 articles
✅ **Use random delays** (appear human-like)
⚠️ **NEVER remove sleep delay**

---

**Last Updated**: 2026-03-13
