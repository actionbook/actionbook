# Knowledge Builder Any - Architecture Guide

## Overview

`knowledge-builder-any` is a component in the Actionbook build pipeline, responsible for processing websites with `source_category='any'`. It crawls websites, analyzes page structures using Claude AI, generates structured handbook documents, and stores the results in a database.

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         API Service                                  │
│                              │                                       │
│                    POST /api/build-tasks                             │
│                              │                                       │
│                              ▼                                       │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                      PostgreSQL                                │  │
│  │                                                                │  │
│  │   build_tasks ──► sources ──► documents ──► chunks             │  │
│  │   (stage=init)                                                 │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              ▲                                       │
│                              │                                       │
│  ┌───────────────────────────┴───────────────────────────────────┐  │
│  │              knowledge-builder-any (Worker)                    │  │
│  │                                                                │  │
│  │   1. Poll build_tasks (source_category='any')                  │  │
│  │   2. Crawl website                                             │  │
│  │   3. Analyze with Claude AI (cc-sdk)                           │  │
│  │   4. Generate handbook (action.md + overview.md)               │  │
│  │   5. Store to documents + chunks (with embeddings)             │  │
│  │   6. Update task status → knowledge_build:completed            │  │
│  └────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Module Structure

```
src/
├── main.rs              # CLI entry point (build/crawl/worker commands)
├── lib.rs               # Public API (build_handbook, build_handbook_simple)
│
├── crawler.rs           # Website crawling (HTTP + HTML parsing)
├── analyzer.rs          # Claude AI analysis (cc-sdk)
├── validator.rs         # Handbook quality validation
├── fixer.rs             # Auto-fix low quality handbooks
├── handbook.rs          # Data structure definitions
├── prompt_manager.rs    # Custom prompt management (CLI mode)
│
├── chunker.rs           # Markdown document chunking
├── embedding.rs         # OpenAI vector embeddings
├── error.rs             # Error type definitions
│
├── db/                  # Database module
│   ├── mod.rs           # Module exports
│   ├── connection.rs    # PostgreSQL connection pool
│   ├── models.rs        # Data models (BuildTask, Source, Document, Chunk)
│   ├── build_tasks.rs   # build_tasks table operations
│   ├── sources.rs       # sources table operations
│   ├── documents.rs     # documents table operations
│   └── chunks.rs        # chunks table operations (pgvector)
│
└── worker/              # Worker module
    ├── mod.rs           # Module exports
    ├── config.rs        # WorkerConfig configuration
    ├── task_runner.rs   # Polling loop + signal handling
    └── processor.rs     # Task processing logic
```

## Core Workflow

### 1. Worker Polling Flow

```
TaskRunner.run()
    │
    ▼
┌─────────────────────────────────────┐
│ fetch_pending_task()                │
│ WHERE source_category='any'         │
│   AND stage='init'                  │
│   AND stage_status='pending'        │
│ FOR UPDATE SKIP LOCKED              │
└─────────────────┬───────────────────┘
                  │
     ┌────────────┴────────────┐
     │ Task found?             │
     └──────┬──────────────────┘
            │
  Yes ─────►│◄────── No ──► sleep(poll_interval)
            │
┌───────────▼───────────────────┐
│ claim_task()                  │
│ SET stage='knowledge_build'   │
│     stage_status='running'    │
└───────────┬───────────────────┘
            │
┌───────────▼───────────────────┐
│ TaskProcessor.process()       │
└───────────┬───────────────────┘
            │
  ┌─────────┴─────────┐
  │                   │
Success             Error
  │                   │
  ▼                   ▼
complete_task()    error_task()
stage_status=      stage_status=
'completed'        'error'
```

### 2. Task Processing Flow (TaskProcessor)

```
process(task)
    │
    ├─► 1. build_handbook_simple(url)
    │       - Crawler.crawl()         → WebContext
    │       - Analyzer.analyze()      → HandbookOutput (via cc-sdk + Claude)
    │
    ├─► 2. ensure_source()
    │       - Check if URL exists in sources table
    │       - Create new record if not exists
    │       → source_id
    │
    ├─► 3. store_document() × 2
    │       - action.md → documents table
    │       - overview.md → documents table
    │       → document_ids
    │
    └─► 4. chunk_and_embed() × 2
            - DocumentChunker.chunk()     → chunks
            - EmbeddingClient.embed()     → vector embeddings
            - chunks::insert_chunks()     → store to chunks table
```

## Database Table Relationships

```
build_tasks                 sources                 documents              chunks
┌─────────────┐            ┌─────────────┐         ┌─────────────┐        ┌─────────────┐
│ id          │            │ id          │◄────────│ source_id   │        │ id          │
│ source_id   │───────────►│ name        │         │ url         │◄───────│ document_id │
│ source_url  │            │ base_url    │         │ title       │        │ content     │
│ stage       │            │ domain      │         │ content_md  │        │ embedding   │
│ stage_status│            │ description │         │ content_hash│        │ chunk_index │
└─────────────┘            └─────────────┘         └─────────────┘        └─────────────┘
```

## State Transitions

| Phase                | stage           | stage_status | Executor                         |
| -------------------- | --------------- | ------------ | -------------------------------- |
| Task created         | init            | pending      | API Service                      |
| Task claimed         | knowledge_build | running      | knowledge-builder-any            |
| Task completed       | knowledge_build | completed    | knowledge-builder-any            |
| Task failed          | knowledge_build | error        | knowledge-builder-any            |
| Follow-up processing | action_build    | running      | action-builder (not implemented) |

## Key Technologies

### 1. Concurrency Safety

Uses `FOR UPDATE SKIP LOCKED` to prevent multiple workers from claiming the same task:

```sql
SELECT * FROM build_tasks
WHERE source_category = 'any'
  AND stage = 'init'
  AND stage_status = 'pending'
ORDER BY created_at ASC
LIMIT 1
FOR UPDATE SKIP LOCKED
```

### 2. Vector Embeddings

Uses OpenAI `text-embedding-3-small` model to generate 1536-dimensional vectors:

```rust
let embedding = embedding_client.embed(&chunk.content).await?;
```

Storage uses pgvector extension:

```sql
embedding vector(1536)
```

### 3. Document Chunking

Chunks based on Markdown structure, preserving heading hierarchy:

```rust
let chunker = DocumentChunker::new(ChunkerOptions::default());
let chunks = chunker.chunk(content);
```

Default configuration:

- `chunk_size`: 1500 characters
- `chunk_overlap`: 200 characters

### 4. Claude AI Analysis

Calls Claude CLI via cc-sdk:

```rust
let mut stream = query(prompt.to_string(), options).await?;
// Collect response and parse JSON
```

## Environment Variables

| Variable       | Required | Description                            |
| -------------- | -------- | -------------------------------------- |
| DATABASE_URL   | Yes      | PostgreSQL connection string           |
| OPENAI_API_KEY | No       | OpenAI API Key (for vector embeddings) |

## CLI Commands

```bash
# CLI mode - single build
handbook-builder build --url <URL> [--output-dir <DIR>] [--name <NAME>]

# CLI mode - crawl only
handbook-builder crawl --url <URL>

# Worker mode - continuous polling
handbook-builder worker [--poll-interval <SECS>] [--no-embeddings] [--once]
```

### Worker Parameters

| Parameter       | Default | Description                    |
| --------------- | ------- | ------------------------------ |
| --poll-interval | 30      | Polling interval (seconds)     |
| --no-embeddings | false   | Disable vector embeddings      |
| --once          | false   | Exit after processing one task |
| --timeout       | 300     | Task timeout (seconds)         |

## Dependencies

```toml
# Core
tokio = { version = "1.41", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "json", "chrono"] }

# AI
cc_sdk = { path = "../../../FW/robius/github-fetch/crates/cc-sdk" }
async-openai = "0.25"

# Vector
pgvector = "0.4"

# Web
reqwest = { version = "0.12", features = ["json"] }
scraper = "0.21"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```
