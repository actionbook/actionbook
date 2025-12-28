# Actionbook MCP Server

MCP server for Claude / Cursor and other AI Agents, providing two-step query capabilities:
- `search_actions`: Search for site / page / element / scenario metadata
- `get_action_by_id`: Get complete content by ID (Markdown, optional JSON)

## Installation & Running

### Using Published Package
```bash
npx @actionbookdev/mcp --api-url http://localhost:3100
```

### Using Local Source (Monorepo)
```bash
# Build
pnpm -C packages/mcp build

# Run
node packages/mcp/dist/cli.js --api-url http://localhost:3100
```

## Configuration

Environment variables and CLI arguments (CLI arguments take precedence over environment variables):

| Env Variable | CLI Argument | Description | Default |
|---|---|---|---|
| `ACTIONBOOK_API_URL` | `--api-url <url>` | API service URL | `https://api.actionbook.dev` |
| `ACTIONBOOK_API_KEY` | `--api-key <key>` | API Key | - |
| `ACTIONBOOK_LOG_LEVEL` | `--log-level <level>` | Log level (debug/info/warn/error) | `info` |
| `ACTIONBOOK_TIMEOUT` | `--timeout <ms>` | Request timeout (ms) | `30000` |
| `ACTIONBOOK_RETRY_MAX` | `--retry-max <n>` | Max retry attempts | `3` |
| `ACTIONBOOK_RETRY_DELAY` | `--retry-delay <ms>` | Retry delay (ms) | `1000` |
| `ACTIONBOOK_TRANSPORT` | `--transport <type>` | Transport mode (stdio/http) | `stdio` |
| `ACTIONBOOK_HTTP_PORT` | `--http-port <port>` | HTTP server port (http mode only) | `3001` |
| `ACTIONBOOK_HTTP_HOST` | `--http-host <host>` | HTTP server host (http mode only) | `0.0.0.0` |
| `ACTIONBOOK_HTTP_CORS` | `--http-cors <origins>` | CORS origins (comma-separated, http mode only) | `*` |

### Example Usage

**Using environment variables:**
```bash
export ACTIONBOOK_API_URL=http://localhost:3100
export ACTIONBOOK_API_KEY=your-key
export ACTIONBOOK_LOG_LEVEL=debug
npx @actionbookdev/mcp
```

**Using CLI arguments:**
```bash
npx @actionbookdev/mcp --api-url http://localhost:3100 --api-key your-key --log-level debug
```

**Using HTTP transport:**
```bash
# Environment variables
export ACTIONBOOK_TRANSPORT=http
export ACTIONBOOK_HTTP_PORT=3001
npx @actionbookdev/mcp

# CLI arguments (overrides environment)
npx @actionbookdev/mcp --transport http --http-port 3001 --http-host localhost
```

## Development & Publishing

### 1. Local Development & Debugging
**Claude Desktop Configuration (Recommended)**
Use absolute path to reference local build artifacts for stability:
```json
{
  "mcpServers": {
    "actionbook-local": {
      "command": "node",
      "args": [
        "/absolute/path/to/actionbook/packages/mcp/dist/cli.js",
        "--api-url", "http://localhost:3000"
      ]
    }
  }
}
```
*Tip: Use `pnpm -C packages/mcp build --watch` to enable watch mode. Changes will auto-compile (Claude restart required).*

**Using npm link**
```bash
cd packages/mcp
npm link

# Test command
actionbook-mcp --help
```

### 2. Publish to NPM
```bash
cd packages/mcp

# 1. Login
npm login

# 2. Update version
npm version patch   # 0.1.0 → 0.1.1 (bug fix)
npm version minor   # 0.1.0 → 0.2.0 (new feature)
npm version major   # 0.1.0 → 1.0.0 (breaking change)

# 3. Build and publish (prepublishOnly auto-runs build)
npm publish
```

### 3. Install Published Version
```bash
# Global install (for CLI)
npm install -g @actionbookdev/mcp

# Or as project dependency
npm install @actionbookdev/mcp

# Or run directly with npx (no install needed)
npx @actionbookdev/mcp --api-url http://localhost:3100
```

If previously used `npm link`, clean up before installing:
```bash
npm rm -g @actionbookdev/mcp
npm cache clean --force
npm install -g @actionbookdev/mcp
```

### 4. Monorepo Internal Integration Testing
No publishing needed. Reference directly in other workspaces (e.g., `services/action-builder`):
1. Add dependency to `package.json`: `"@actionbookdev/mcp": "workspace:*"`
2. Write test scripts:
```typescript
import { ActionbookMcpServer } from '@actionbookdev/mcp';
// ...use class for integration testing
```

## MCP Client Configuration

### Claude Desktop
```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp", "--api-url", "http://localhost:3100"],
      "env": {
        "ACTIONBOOK_API_KEY": "your-key"
      }
    }
  }
}
```

### Cursor
```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp", "--api-url", "http://localhost:3100"]
    }
  }
}
```

## Test Coverage
- Core libs: config/protocol/errors/logger/formatter/schema/types/api-client
- Tools: search_actions / get_action_by_id
- Server: tool registration and invocation
- Integration: local HTTP stub to verify tool calls with actual API client
