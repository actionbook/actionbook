# Supported Websites

## Full Compatibility List

| Website | Auto-Detected Source | Special Handling | Notes |
|---------|---------------------|------------------|-------|
| **X/Twitter** | `X` | **AI reformatting required** | See references/twitter-handling.md |
| **Medium** | `Medium` | None | Native Markdown works, handles paywalled (if logged in) |
| **Dev.to** | `Dev.to` | None | Preserves code blocks well |
| **OpenAI Blog** | `OpenAI Blog` | None | Technical articles, good structure |
| **Substack** | `Substack` | None | Newsletter content |
| **GitHub** | `GitHub` | None | README and docs |
| **Any website** | Domain name | None | Universal fallback |

## Notes by Platform

### Twitter/X

⚠️ **CRITICAL**: Requires AI reformatting for long posts
- Problem: `fetch` loses all Markdown structure
- Solution: See references/twitter-handling.md
- Alternative: Use snapshot + AI parsing for threads

### Medium

✅ Works well
- Native Markdown extraction works
- Handles paywalled articles (if you're logged in via browser session)
- Code blocks preserved

### Dev.to

✅ Works well
- Excellent code block preservation
- Front matter usually clean

### OpenAI Blog

✅ Works well
- Technical content exports cleanly
- Good heading structure

### Substack

✅ Works well
- Newsletter format exports well
- Email-style content preserved

### GitHub

✅ Works well
- README files export cleanly
- Markdown native, no conversion needed

### Generic Websites

⚠️ Variable quality
- Static blogs: Usually work well
- Dynamic SPAs: May need `--wait-hint heavy`
- Paywalled content: Respects paywall (cannot bypass)

## Feature Support Matrix

| Feature | Twitter/X | Medium | Dev.to | Others |
|---------|-----------|--------|--------|--------|
| Markdown structure | ❌ (need AI) | ✅ | ✅ | ✅ |
| Code blocks | ❌ (need AI) | ✅ | ✅ | ⚠️ Variable |
| Images | ✅ | ✅ | ✅ | ✅ |
| Tables | ❌ (need AI) | ✅ | ✅ | ⚠️ Variable |
| Metadata extraction | ✅ | ✅ | ✅ | ✅ |

---

**Last Updated**: 2026-03-13
