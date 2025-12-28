# @actionbookdev/knowledge-builder

Documentation crawler and knowledge extraction service for RAG (Retrieval-Augmented Generation).

## Features

- **Web Crawler**: Playwright-based crawler for documentation websites
- **Site Adapters**: Customizable content extraction per site
- **Document Chunking**: Smart chunking with heading hierarchy preservation
- **Vector Embeddings**: OpenAI text-embedding-3-small integration
- **Search**: Vector, full-text, and hybrid search (RRF fusion)
- **Shared Database**: Uses `@actionbookdev/db` for type-safe database operations

## Installation

```bash
pnpm install
```

## Configuration

Copy `.env.example` to `.env.local` and configure:

```bash
DATABASE_URL=postgresql://postgres:passwd@localhost:5432/actionbook_knowledge_lib
OPENAI_API_KEY=sk-...
OPENAI_BASE_URL=https://api.openai.com/v1  # optional
# Optional proxy settings
# HTTPS_PROXY=http://proxy:port
```

## Usage

### Build Knowledge (Crawl a Documentation Site)

```bash
pnpm build-knowledge -u <url> -n <source-name> [options]
```

Options:
- `-u, --url <url>`: Base URL to crawl (required)
- `-n, --name <name>`: Source name - must be a registered adapter (required)
- `-d, --depth <number>`: Max crawl depth (override adapter default)
- `-i, --include <patterns>`: Include URL patterns (override adapter default)
- `-e, --exclude <patterns>`: Exclude URL patterns (override adapter default)
- `--rate-limit <ms>`: Rate limit in ms (override adapter default)
- `--skip-embeddings`: Skip generating embeddings

Available sources:
- `airbnb`: Airbnb Help Center documentation

Example:
```bash
pnpm build-knowledge -u https://www.airbnb.com/help -n airbnb
```

### Search Documents

```bash
pnpm search "<query>" [options]
```

Options:
- `-t, --type <type>`: Search type (vector, fulltext, hybrid) (default: hybrid)
- `-l, --limit <number>`: Result limit (default: 10)
- `-s, --source-id <id>`: Filter by source ID
- `--context`: Return formatted context for LLM
- `--json`: Output as JSON

Example:
```bash
pnpm search "how to configure authentication" -t hybrid -l 5
```

## Architecture

```
Source → Document → Chunk (with embedding)
   ↓
CrawlLog (tracks crawl sessions)
```

### Adapters

Site-specific content extraction is handled by adapters in `src/crawler/adapters/`:

```
adapters/
├── config.ts          # Adapter registry
├── index.ts           # Exports and validation
├── types.ts           # Interface definitions
├── default.ts         # Base adapter class
└── airbnb/
    └── index.ts       # Airbnb-specific adapter
```

To add a new adapter:
1. Create `adapters/{name}/index.ts` extending `DefaultAdapter`
2. Register in `adapters/config.ts`

### Data Flow

1. Crawler extracts HTML from pages using Playwright
2. Site adapter extracts content based on site-specific selectors
3. Content is converted to Markdown
4. DocumentChunker splits content by headings
5. EmbeddingService generates vector embeddings
6. Data is stored in PostgreSQL via `@actionbookdev/db`
7. SearchService provides vector/fulltext/hybrid search
