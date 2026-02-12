# Actionbook Extension

Chrome extension that bridges the Actionbook CLI with your browser for AI-powered automation.

## Installation

### Option 1: CLI (recommended)

```bash
actionbook extension install
```

This downloads the latest release from GitHub and installs it to `~/.config/actionbook/extension/`.

### Option 2: Manual download

1. Go to [GitHub Releases](https://github.com/actionbook/actionbook/releases)
2. Find the latest `actionbook-extension-v*` release
3. Download the `.zip` file
4. Unzip to a local folder

### Load in Chrome

1. Open `chrome://extensions`
2. Enable **Developer mode**
3. Click **Load unpacked**
4. Select the extension directory (`actionbook extension path` to find it)

## Usage

### Use the extension

The extension communicates with the CLI via a local WebSocket bridge that **auto-starts** when you run browser commands.

**No manual bridge start needed** - just run commands:

```bash
actionbook browser open https://example.com
# Bridge starts automatically in the background
```

The CLI registers Native Messaging on install, so the extension connects automatically when the bridge starts.

### Verify connection

```bash
actionbook extension status
actionbook extension ping
```

### Run commands in extension mode

Add `--extension` to any browser command (or set `ACTIONBOOK_EXTENSION=1`):

```bash
actionbook --extension browser open "https://example.com"
actionbook --extension browser fill "#username" "demo"
actionbook --extension browser click "button[type='submit']"
actionbook --extension browser screenshot result.png
```

See the full command reference in the [main README](../../README.md).

## Releasing a new version

The extension has its own independent release cycle, separate from the CLI.

### Steps

1. Make changes in `packages/actionbook-extension/`
2. Update `version` in `manifest.json` (e.g. `0.2.0` -> `0.3.0`)
3. Commit:
   ```bash
   git commit -m "[packages/actionbook-extension]feat: description of change"
   ```
4. Tag:
   ```bash
   git tag actionbook-extension-v0.3.0
   ```
5. Push:
   ```bash
   git push origin main --tags
   ```
6. GitHub Actions automatically:
   - Verifies tag version matches `manifest.json` version
   - Packages the extension as a `.zip`
   - Creates a GitHub Release with the `.zip` and install instructions

### Local packaging

```bash
cd packages/actionbook-extension
npm run package
```

Output: `dist/actionbook-extension-v{version}.zip`

## Version compatibility

The CLI and extension are versioned independently. Compatibility is guaranteed by the **bridge protocol version** exchanged during the WebSocket hello handshake. As long as both sides speak the same protocol version, they work together regardless of their individual version numbers.

## Troubleshooting

1. **`Ping failed` / `not running`** - The bridge auto-starts with browser commands. Ensure the extension is loaded in Chrome. Check status with `actionbook extension status`.

2. **Port conflict** - If the bridge fails to start, check if port 19222 is in use. Stop conflicting processes or use a different port in `~/.actionbook/config.toml`.

3. **`No tab attached`** - Make sure Chrome has a visible tab. Run `open` or `goto` first.

4. **GitHub rate limit on install** - Anonymous API allows 60 requests/hour. Wait and retry, or download the `.zip` manually from the [Releases page](https://github.com/actionbook/actionbook/releases).

5. **Offline install** - Download the `.zip` from another machine, unzip to `~/.config/actionbook/extension/`, then run `actionbook extension install --force` to register native messaging.
