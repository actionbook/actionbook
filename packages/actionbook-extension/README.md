# Actionbook Extension

Chrome extension that bridges the Actionbook CLI with your browser for AI-powered automation.

## Installation

### Option 1: Chrome Web Store (recommended)

1. Open [Actionbook on Chrome Web Store](https://chromewebstore.google.com/detail/actionbook/bebchpafpemheedhcdabookaifcijmfo)
2. Click **Add to Chrome**
3. Confirm **Add extension**

### Option 2: CLI fallback (local debug install)

```bash
actionbook extension install
```

On a default install, this writes the unpacked extension to `~/Actionbook/extension/`.
If you use a custom `ACTIONBOOK_HOME`, it stays inside that custom tree.

### Option 3: Manual download

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

The extension communicates with the CLI via a local WebSocket bridge that runs inside the **actionbook daemon**. The daemon **auto-starts** when you run browser commands.

**No manual bridge start needed** - just run commands:

```bash
actionbook browser start --set-session-id s1
actionbook browser goto "https://example.com" --session s1 --tab t1
# Daemon and bridge start automatically in the background
```

The CLI registers Native Messaging on install, so the extension connects automatically when the daemon starts.

### Verify connection

```bash
actionbook extension status
actionbook extension ping
```

### Run commands in extension mode

Recommended: run setup once and choose extension mode:

```bash
actionbook setup
```

After setup, run browser commands normally (no extra mode flags):

```bash
actionbook browser start --set-session-id s1
actionbook browser goto "https://example.com" --session s1 --tab t1
actionbook browser fill "#username" "demo" --session s1 --tab t1
actionbook browser click "button[type='submit']" --session s1 --tab t1
actionbook browser screenshot result.png --session s1 --tab t1
```

If you need to switch modes later, run `actionbook setup` again.

See the full command reference in the [main README](../../README.md).

## Cloud Mode (v0.5.0+)

Cloud Mode lets remote AI agents (Claude Desktop, Codex, claude.ai Connectors, etc.) drive your Chrome through the Actionbook edge server — **without running the local CLI daemon**. The extension connects via WebSocket to `edge.actionbook.dev`, authenticated by a short-lived OAuth token.

Local Mode (the default) is unchanged and remains fully offline-capable.

### Setup

1. Click the Actionbook toolbar icon → popup
2. Switch the **Mode** dropdown from **Local (CLI)** to **Cloud**
3. Click **Sign in to Actionbook**
4. A new tab opens the Actionbook sign-in page (Clerk) → sign in → approve the authorization prompt
5. Back in the popup, the **Bridge** indicator turns green and shows your device ID

Any MCP client pointed at `https://edge.actionbook.dev/mcp` can now drive your Chrome. No CLI daemon needed.

### Example: Claude Desktop

```jsonc
// ~/Library/Application Support/Claude/claude_desktop_config.json  (macOS)
{
  "mcpServers": {
    "actionbook": {
      "url": "https://edge.actionbook.dev/mcp"
    }
  }
}
```

Restart Claude Desktop → it will walk through OAuth sign-in on first use → the `actionbook` tool becomes available in your conversations.

### Switching back to Local

Popup → **Mode** dropdown → **Local (CLI)**. The extension reconnects to `ws://127.0.0.1:19222` as in previous versions. No sign-out required.

### Sign out of Cloud Mode

Popup (while in Cloud mode) → **Sign out**. Clears the stored access/refresh tokens and disconnects. To revoke the session server-side as well, use your account settings on actionbook.dev.

### Privacy summary

In Cloud Mode, CDP commands from your authorized AI agents transit `edge.actionbook.dev` (Cloudflare Workers + Durable Objects). The extension holds only a short-lived Clerk-signed JWT in `chrome.storage.local`; no passwords or long-lived credentials ever leave your machine. See [PRIVACY.md](./PRIVACY.md) for the detailed data-flow breakdown.

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

1. **`Ping failed` / `not running`** - The bridge (part of the actionbook daemon) auto-starts with browser commands. Ensure the extension is loaded in Chrome. Check status with `actionbook extension status`.

2. **Port conflict** - Browser mode uses fixed bridge address `ws://127.0.0.1:19222`. If startup fails, free that port and retry (macOS/Linux: `lsof -i :19222`).

3. **`No tab attached`** - Make sure Chrome has a visible tab. Run `open` or `goto` first.

4. **Web Store install failed** - Use fallback `actionbook extension install`, then load it from `chrome://extensions` with **Load unpacked**.

5. **Offline install** - Download the `.zip` from another machine, unzip it to any local folder, then load it as an unpacked extension in `chrome://extensions`.
