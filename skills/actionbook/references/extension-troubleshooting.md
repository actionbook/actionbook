# Extension Troubleshooting

Common issues and fixes for the Chrome Extension bridge.

## Bridge not running

The bridge **auto-starts** when you run browser commands - no manual start needed:

```bash
actionbook browser open https://example.com    # Bridge starts automatically
```

Check bridge status:

```bash
actionbook extension status                   # Check if running
```

## Extension not responding

```bash
actionbook extension ping                     # Check connectivity
```

If ping fails, verify:
1. Extension is loaded in Chrome (`chrome://extensions`) and enabled
2. Bridge auto-started correctly (check `actionbook extension status`)
3. No port conflicts on port 19222

## Port conflict

**Symptoms:** "Bridge did not start" or "port already in use" errors.

**Fix:** Check if another process is using port 19222:

```bash
lsof -i :19222                               # macOS/Linux
netstat -ano | findstr :19222                # Windows
```

Stop the conflicting process or change the port in `~/.actionbook/config.toml`:

```toml
[browser.extension]
port = 19223  # Use a different port
```

## Stale bridge process

**Symptoms:** Bridge appears running but unresponsive.

**Fix:** Stop and let it auto-restart:

```bash
actionbook extension stop                     # Kill bridge process
actionbook browser open https://example.com   # Auto-restarts fresh
```

## Extension not installed

```bash
actionbook extension install            # Install extension files
actionbook extension path               # Get directory for Chrome "Load unpacked"
```

Then load in Chrome: `chrome://extensions` → Developer mode → Load unpacked → select the path.

## Uninstall extension

```bash
actionbook extension uninstall          # Remove extension files and native host registration
```
