# Troubleshooting Guide

## Dependency Issues

### "actionbook: command not found"

**Cause:** actionbook CLI not installed

**Solution:**
```bash
npm install -g @actionbookdev/cli@latest
```

### "unknown flag: --wait-hint"

**Cause:** actionbook version < 0.9.1

**Solution:**
```bash
npm install -g @actionbookdev/cli@latest
actionbook --version  # Verify >= 0.9.1
```

### Version check fails

**Cause:** actionbook too old

**Solution:** Must be >= 0.9.1, upgrade required

---

## Content Issues

### Twitter/X article format broken

**Cause:** `fetch` loses Markdown structure

**Solution:** Use AI reformatting (see references/twitter-handling.md)

### Twitter UI noise in article

**Cause:** First 7 lines are author/stats

**Solution:** AI reformatting removes automatically

### Code blocks not formatted

**Cause:** `fetch` doesn't preserve markers

**Solution:** AI reformatting adds ```json, ```typescript markers

### Trailing junk in article

**Cause:** Last 10 lines are timestamp/stats

**Solution:** AI reformatting removes automatically

---

## Image Download Issues

### Images downloading as 0 bytes

**Cause:** URL expired or format issue

**Solution:**
```bash
# Try alternative format (Twitter)
curl -L -s "${url}?format=jpg&name=orig" -o "image.jpg"
```

### Special chars in title

**Cause:** Invalid filename characters

**Solution:** Auto-sanitized in Step 4 (`/ : * ? " < > |` removed)

---

## Translation Issues

### Translation not working

**Cause:** AI session issue

**Solution:** Retry translation request, or translate manually

---

## Directory Issues

### "Directory already exists"

**Cause:** Article already exported

**Solution:** User decides: overwrite or skip

---

## Execution Issues

### Fetch timeout

**Cause:** Slow website

**Solution:**
```bash
# Use wait-hint heavy, increase timeout
actionbook browser fetch "$URL" --format markdown --wait-hint heavy
```

---

## Obsidian Integration

### "obsidian-cli: command not found"

**Cause:** obsidian-cli not installed

**Solution:**
```bash
npm install -g obsidian-cli
obsidian-cli set-default --vault "VaultName"
```

### "Unable to find vault"

**Cause:** Vault not configured

**Solution:**
```bash
obsidian-cli set-default --vault "YourVaultName"
```

---

## Batch Export Issues

### Batch export blocked/rate limited

**Cause:** Too fast, flagged as bot

**Solution:** Add 3-5s `sleep` between requests (see references/batch-export.md)

### "Access denied" or 429 errors

**Cause:** Rate limit exceeded

**Solution:**
- Wait 5-10 minutes
- Reduce batch size
- Add longer delays (5-10s)

---

**Last Updated**: 2026-03-13
