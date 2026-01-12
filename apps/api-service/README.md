# @actionbookdev/api-service

REST API service for Actionbook platform. Provides endpoints for MCP Server to query actions.

## Development

```bash
# Install dependencies (from monorepo root)
pnpm install

# Start development server
pnpm dev
# Server runs at http://localhost:3100
```

## Production

```bash
# Build for production
pnpm build

# Start production server
# Default port: Controlled by PORT env var (or 3000 if not set)
PORT=80 pnpm start
```

## Authentication

All endpoints (except `/api/health`) require API Key authentication.

**Headers:**
- `x-api-key`: Your secret API key

**Environment Variables:**
- `API_SERVICE_KEY`: The secret key configured on the server.

## API Endpoints

### Health Check (Public)

```
GET /api/health
```

Response:
```json
{
  "status": "healthy",
  "timestamp": "2024-03-20T10:00:00.000Z",
  "version": "0.1.0",
  "services": {
    "database": true,
    "cache": true
  }
}
```

### Search Actions (Authenticated)

```
GET /api/actions/search?q={query}&site={domain}&limit={limit}
```

**Headers:**
`x-api-key: your-api-key`

Parameters:
- `q` (required): Search query
- `site` (optional): Filter by domain
- `limit` (optional): Max results (default: 20, max: 100)

---

## Build Tasks API

Build tasks manage the automated pipeline for crawling websites and building action capabilities.

### Create Build Task

```
POST /api/build-tasks
```

**Request Body:**
```json
{
  "sourceUrl": "https://help.example.com",
  "sourceCategory": "help",
  "sourceName": "Example Help Center",
  "config": {
    "maxPages": 100,
    "maxDepth": 2,
    "rateLimit": 2000,
    "includePatterns": ["/docs/*", "/guide/*"],
    "excludePatterns": ["/api/*", "/internal/*"]
  }
}
```

### Build Task Config Options

The `config` field controls crawling behavior in the knowledge-builder stage:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `maxPages` | number | 500 | Maximum number of pages to crawl |
| `maxDepth` | number | 3 | Maximum crawl depth from start URL |
| `rateLimit` | number | 1000 | Delay between requests (milliseconds) |
| `includePatterns` | string[] | [] | URL patterns to include (glob-style) |
| `excludePatterns` | string[] | [] | URL patterns to exclude (glob-style) |

**Example: Limit crawling to 50 pages with depth 2**
```json
{
  "sourceUrl": "https://docs.example.com",
  "sourceCategory": "help",
  "config": {
    "maxPages": 50,
    "maxDepth": 2
  }
}
```

### List Build Tasks

```
GET /api/build-tasks?stage={stage}&status={status}&limit={limit}
```

Parameters:
- `stage` (optional): Filter by stage (`init`, `knowledge_build`, `action_build`, `completed`, `error`)
- `status` (optional): Filter by status (`pending`, `running`, `completed`, `error`)
- `limit` (optional): Max results (default: 20)

### Get Build Task

```
GET /api/build-tasks/{id}
```

### Build Task Stages

Build tasks progress through the following stages:

```
init → knowledge_build → action_build → completed
                    ↘ error (on failure)
```

| Stage | Description |
|-------|-------------|
| `init` | Task created, waiting for knowledge-builder |
| `knowledge_build` | Crawling website and extracting knowledge |
| `action_build` | Generating UI element actions from chunks |
| `completed` | All stages finished successfully |
| `error` | Task failed after max retries |

---

## Environment Variables

Create a `.env` file based on `.env.example`:

- `PORT` - Service port (default: 3100 for dev, dynamic for prod)
- `API_SERVICE_KEY` - Secret key for API authentication (Required)
- `DATABASE_URL` - PostgreSQL connection string (Required)

## Dependencies

- `@actionbookdev/db` - Shared database package
