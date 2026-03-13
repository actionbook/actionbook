# Obsidian CLI Setup

## Installation

```bash
npm install -g obsidian-cli
```

## First-time Setup

```bash
# Set default vault (matches OUTPUT_DIR basename)
obsidian-cli set-default --vault "$(basename "$OUTPUT_DIR")"

# Example:
# If OUTPUT_DIR = ~/Work/Write/Articles
# Then vault name = "Articles"
```

## Commands

```bash
# Open article index
obsidian-cli open "Article Title/index.md"

# Open with specific vault (multiple vaults)
obsidian-cli open "Article Title/index.md" --vault "Articles"

# Open specific section (heading)
obsidian-cli open "Article Title/README.md" --section "Introduction"
```

## Tips

- **First-time setup**: Run `obsidian-cli set-default --vault "YourVaultName"` once
- **Vault name**: Use basename of OUTPUT_DIR
- **Fallback**: If not installed, script opens Finder/Explorer instead
- **Path matching**: OUTPUT_DIR should match Obsidian vault root

## Troubleshooting

### "Cannot find vault config"

```bash
# Re-run setup with correct vault name
obsidian-cli set-default --vault "YourActualVaultName"
```

### Article doesn't open

```bash
# Verify vault path
obsidian-cli list

# Verify relative path format
echo "$OUTPUT_DIR/$SAFE_TITLE" | sed "s|$OUTPUT_DIR/||"
```

---

**Last Updated**: 2026-03-13
