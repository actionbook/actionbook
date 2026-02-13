# Camoufox Integration Manual Testing Guide

## Prerequisites

- Node.js 18+ installed
- actionbook-rs built successfully
- Port 9377 available (default Camoufox port)

## Step 1: Install and Start Camoufox Browser Server

### Option A: Using npx (Recommended for quick testing)

```bash
# This will download and start camofox-browser on port 9377
npx @askjo/camoufox-browser
```

### Option B: Install locally

```bash
cd .test-camofox
npm install
npx camoufox-browser
```

Expected output:
```
🦊 Camoufox Browser Server starting...
🌐 REST API listening on http://localhost:9377
✓ Health check endpoint: http://localhost:9377/health
```

**Keep this terminal running** - the server needs to stay active for testing.

## Step 2: Verify Server is Running

In a new terminal:

```bash
# Test health endpoint
curl http://localhost:9377/health

# Expected response: {"status":"ok"}
```

## Step 3: Build actionbook-rs

```bash
cd packages/actionbook-rs
cargo build --release
```

## Step 4: Manual Testing Commands

### Test 1: Basic Connection Test

```bash
# Test that Camoufox backend can be selected
cd packages/actionbook-rs
cargo run -- --camofox --help

# Should work without errors, showing help text
```

### Test 2: Configuration Test

Create a test config:

```bash
mkdir -p ~/.actionbook
cat > ~/.actionbook/config.toml <<'EOF'
[browser]
backend = "camofox"

[browser.camofox]
port = 9377
user_id = "test-user"
session_key = "test-session"

[profiles.test-camofox]
backend = "camofox"
camofox_port = 9377
EOF
```

### Test 3: Browser Commands (After Phase 4 Integration)

**Note**: These commands will work AFTER Phase 4 integration is complete.
For now, they will use the default CDP backend unless explicitly routed.

```bash
cd packages/actionbook-rs

# Test navigation with Camoufox flag
cargo run -- --camofox browser open https://example.com

# Test clicking (requires Phase 4)
cargo run -- --camofox browser click "h1"

# Test typing (requires Phase 4)
cargo run -- --camofox browser type "#searchbox" "test query"

# Test screenshot (requires Phase 4)
cargo run -- --camofox browser screenshot test-screenshot.png
```

### Test 4: Profile-based Backend Selection

```bash
# Use the test-camofox profile
cargo run -- --profile test-camofox browser open https://example.com
```

### Test 5: Environment Variable Testing

```bash
# Test with environment variables
ACTIONBOOK_CAMOFOX=true cargo run -- browser open https://example.com

# Test with custom port
ACTIONBOOK_CAMOFOX=true ACTIONBOOK_CAMOFOX_PORT=9377 cargo run -- browser open https://example.com
```

## Step 5: Integration Tests (Requires Server Running)

```bash
cd packages/actionbook-rs

# Run integration tests (they are currently marked #[ignore])
cargo test --lib camofox -- --ignored --nocapture

# Expected: Should connect to server and create tabs
```

## Step 6: Anti-Bot Effectiveness Testing (After Phase 4)

Once Phase 4 is complete, test anti-bot capabilities:

```bash
# Test with bot detection sites
cargo run -- --camofox browser open https://bot.sannysoft.com
cargo run -- --camofox browser screenshot bot-test.png

# Open bot-test.png - should show green checks

# Test with CreepJS
cargo run -- --camofox browser open https://abrahamjuliot.github.io/creepjs/
cargo run -- --camofox browser screenshot creepjs-test.png

# Test with PixelScan
cargo run -- --camofox browser open https://pixelscan.net/
cargo run -- --camofox browser screenshot pixelscan-test.png
```

## Troubleshooting

### Error: "Camoufox server not reachable"

**Cause**: Server not running or wrong port

**Fix**:
1. Check server is running: `curl http://localhost:9377/health`
2. Verify port: `lsof -i :9377`
3. Check config: `cat ~/.actionbook/config.toml`

### Error: "No active tab"

**Cause**: Need to create a tab first (automatic in Phase 4)

**Fix**: Commands should automatically create tabs when needed.

### Error: "Element ref resolution failed"

**Cause**: CSS selector doesn't match any element in accessibility tree

**Fix**:
1. Check selector syntax
2. Use simpler selectors (e.g., role name: "button" instead of CSS)
3. View accessibility tree: `cargo run -- --camofox browser eval "console.log('tree')"`

## Current Implementation Status

✅ **Phase 1**: Foundation (types, client, session, snapshot parsing)
✅ **Phase 2**: Router (BrowserDriver multi-backend support)
⏳ **Phase 3**: Selector resolution enhancements
⏳ **Phase 4**: Command integration
⏳ **Phase 5**: Full testing and validation

**What works now:**
- Server connection and health checks
- Backend selection (CLI flag, config, profile)
- REST API client (all endpoints)
- CSS selector matching (unit tests)
- Session management with caching

**What needs Phase 4:**
- Actual browser command routing through BrowserDriver
- Integration with commands/browser.rs
- End-to-end browser automation

## Manual API Testing (Direct REST calls)

You can test the Camoufox REST API directly:

```bash
# Create a tab
curl -X POST http://localhost:9377/tabs \
  -H "Content-Type: application/json" \
  -d '{
    "userId": "test-user",
    "sessionKey": "test-session",
    "url": "https://example.com"
  }'

# Response: {"id":"<tab-id>","url":"https://example.com"}

# Get snapshot
curl http://localhost:9377/tabs/<tab-id>/snapshot

# Click element
curl -X POST http://localhost:9377/tabs/<tab-id>/click \
  -H "Content-Type: application/json" \
  -d '{
    "userId": "test-user",
    "elementRef": "e1"
  }'

# Take screenshot
curl http://localhost:9377/tabs/<tab-id>/screenshot
```

## Next Steps

To enable full browser automation:

1. **Implement Phase 3-4**: Integrate BrowserDriver into commands/browser.rs
2. **Test all commands**: goto, click, type, screenshot, etc.
3. **Validate anti-bot**: Test with protection sites
4. **Performance testing**: Compare CDP vs Camoufox latency

## Useful Commands

```bash
# Check what's listening on port 9377
lsof -i :9377

# Kill camofox-browser if stuck
pkill -f camoufox-browser

# View actionbook config
cat ~/.actionbook/config.toml

# Check build
cd packages/actionbook-rs && cargo check --quiet

# Run specific test
cargo test --lib camofox::tests::test_client_creation

# Build with verbose output
cargo build --release --verbose
```
