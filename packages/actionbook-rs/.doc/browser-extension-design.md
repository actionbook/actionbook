# CUE-323: CLI Browser Extension Mode Design

> **Version**: v0.1.0 | **Date**: 2026-02-07 | **Status**: Approved | **Linear**: CUE-323

---

## Overview

Let CLI control the user's own browser via a Chrome Extension instead of launching a new Chrome instance in debug mode. Reference implementations: Manus Browser Operator, Claude Code Chrome Extension.

## Architecture

```
+-----------------------------------------------------------+
|                    Actionbook CLI Process                   |
|                                                            |
|   Agent -> Browser Tool -> SessionManager                  |
|                              |                             |
|                   ExtensionTransport (WebSocket Client)     |
+------------------------------+-----------------------------+
                               | ws://localhost:19222
+------------------------------+-----------------------------+
|                  Chrome Extension (MV3)                     |
|                                                            |
|   Offscreen Document (WebSocket Server)                    |
|        |                                                   |
|        v                                                   |
|   Background Service Worker                                |
|        |                                                   |
|        +-- chrome.debugger API (CDP) --> Target Tab         |
|        +-- chrome.tabs API           --> Tab Management     |
|        +-- chrome.scripting API      --> Script Injection   |
|                                                            |
+------------------------------------------------------------+
```

**Key chain**: CLI -> WebSocket -> Offscreen Doc -> Service Worker -> `chrome.debugger` -> User's Browser Tab

## Technical Decisions (Finalized)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Communication | WebSocket localhost | Simpler setup, good debuggability, reuses existing tokio_tungstenite code. Migrate to Native Messaging if/when publishing to Web Store |
| Browser Control API | `chrome.debugger` (CDP) | CLI's 51 browser commands all use CDP. Using chrome.debugger gives identical CDP interface, minimal CLI-side changes |
| Permission Model | MVP: `<all_urls>` + disable toggle. V2: site whitelist | Avoids blocking MVP with permission UI engineering |
| Distribution | CLI sideload install | `actionbook browser install-extension` auto-loads. Web Store later |
| SW Keep-alive | Offscreen document holds WebSocket | SW stays alive while offscreen doc exists |
| Content Extraction | CDP + Content Script hybrid | CDP for screenshots/accessibility tree/network, Content Script for structured content |

## Message Protocol

JSON-RPC style, matching CDP format for minimal translation:

```json
// Request (CLI -> Extension)
{
  "id": 1,
  "method": "Page.navigate",
  "params": { "url": "https://example.com" }
}

// Response (Extension -> CLI)
{
  "id": 1,
  "result": { "frameId": "..." }
}

// Error
{
  "id": 1,
  "error": { "code": -32000, "message": "Tab not attached" }
}

// Event (Extension -> CLI, no id)
{
  "method": "Network.responseReceived",
  "params": { ... }
}
```

### Extension-specific Commands (non-CDP)

```json
// Attach to a tab
{ "id": 1, "method": "Extension.attachTab", "params": { "tabId": 123 } }

// List available tabs
{ "id": 2, "method": "Extension.listTabs", "params": {} }

// Ping
{ "id": 3, "method": "Extension.ping", "params": {} }

// Detach
{ "id": 4, "method": "Extension.detachTab", "params": {} }
```

## CLI-side Architecture Change

Introduce `BrowserTransport` trait to abstract the communication layer:

```rust
#[async_trait]
pub trait BrowserTransport: Send + Sync {
    async fn send_command(&self, method: &str, params: Value) -> Result<Value>;
    async fn close(&self) -> Result<()>;
}

// Current: direct CDP WebSocket
pub struct DirectCdpTransport { ws: WebSocketStream }

// New: via Extension WebSocket relay
pub struct ExtensionTransport { ws: WebSocketStream }
```

`SessionManager` methods remain unchanged - only the underlying transport switches.

## Development Phases

### Phase 1: Extension Skeleton + WebSocket Link (3-5 days)

**Goal**: CLI and Extension can exchange messages bidirectionally.

**Deliverables**:
- Chrome Extension (MV3): manifest.json + background.js + offscreen.html/js
- WebSocket server in offscreen document on `ws://localhost:19222`
- CLI `--extension` flag to connect via ExtensionTransport

**Verification**:
```bash
# Load extension (sideload), then:
actionbook browser connect --extension
actionbook browser eval "1 + 1"  # Returns 2 via extension relay
```

### Phase 2: CDP Command Bridge (3-5 days)

**Goal**: Extension receives CDP commands from CLI and executes via `chrome.debugger`.

**Deliverables**:
- Extension `chrome.debugger.attach()` to target tab
- CDP command forwarding: CLI -> WebSocket -> Extension SW -> chrome.debugger.sendCommand()
- Core commands: navigate, screenshot, eval

**Verification**:
```bash
actionbook browser connect --extension
actionbook browser goto "https://example.com"
actionbook browser screenshot ./test.png
actionbook browser eval "document.title"  # "Example Domain"
```

### Phase 3: Full Interaction + Tab Management (5-7 days)

**Goal**: All 51 browser commands work in extension mode.

**Deliverables**:
- Full CDP command passthrough (click, type, fill, select, hover, press, snapshot, etc.)
- Tab management (pages, switch, open)
- Cookie management
- Wait commands
- Event subscription mechanism (Console, Network)

**Verification**:
```bash
actionbook browser connect --extension
actionbook browser goto "https://github.com/login"
actionbook browser snapshot
actionbook browser fill "[ref=e3]" "username"
actionbook browser click "[ref=e7]"
actionbook browser wait-nav
```

### Phase 4: Install Experience + Stability + Distribution (3-5 days)

**Goal**: One-click extension install and robust error handling.

**Deliverables**:
- `actionbook browser install-extension` command
- Extension bundled in CLI distribution (or npm)
- Reconnection on WebSocket disconnect
- Error handling: extension not installed, tab crash recovery
- Extension popup UI: connection status, active tab, disable toggle

**Verification**:
```bash
actionbook setup  # includes extension install step
actionbook browser connect --extension
actionbook browser status  # shows extension mode + connection
```

## Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| `chrome.debugger` Web Store rejection | Medium | Sideload distribution bypasses this; document use-case for future submission |
| MV3 SW 30s idle termination | High | Offscreen document keeps alive; reconnect on wake |
| Multiple Chrome profiles/instances | Medium | Extension broadcasts on fixed port; CLI connects to first responder |
| Cross-platform path differences | Medium | Use `dirs` crate for platform-specific paths |
| Native Messaging 1MB message limit | N/A | Not using Native Messaging; WebSocket has no practical limit |
| Large screenshot base64 over WebSocket | Low | WebSocket handles multi-MB messages; chunk if needed |

## File Structure

```
packages/
  actionbook-extension/          # New Chrome Extension package
    manifest.json
    background.js                # Service Worker
    offscreen.html               # Offscreen document
    offscreen.js                 # WebSocket server
    popup.html                   # Extension popup UI
    popup.js
    content.js                   # Content script (Phase 3)
    icons/
      icon-16.png
      icon-48.png
      icon-128.png
  actionbook-rs/
    src/
      browser/
        transport.rs             # New: BrowserTransport trait + impls
        session.rs               # Modified: use BrowserTransport
```
