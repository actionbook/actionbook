# Knowledge Builder Any - How It Works

## Overview

`knowledge-builder-any` is the first stage of the Actionbook build pipeline, responsible for transforming any website into a structured knowledge base (handbook) for use by the action-builder and AI Agents.

## Workflow

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Poll for tasks                                              │
│     SELECT FROM build_tasks                                     │
│     WHERE source_category='any' AND stage='init'                │
│           AND stage_status='pending'                            │
│     FOR UPDATE SKIP LOCKED                                      │
└─────────────────────┬───────────────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────────────┐
│  2. Crawl website (Crawler)                                     │
│     - HTTP GET to fetch HTML                                    │
│     - Parse page structure                                      │
│     - Extract: interactive elements, content blocks, nav, forms │
│     → Output: WebContext                                        │
└─────────────────────┬───────────────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────────────┐
│  3. AI Analysis (Analyzer + cc-sdk)                             │
│     - Build analysis prompt (includes page structure JSON)      │
│     - Call Claude CLI to generate handbook                      │
│     - Parse JSON response                                       │
│     → Output: HandbookOutput { action, overview }               │
└─────────────────────┬───────────────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────────────┐
│  4. Store to database                                           │
│     a) sources table: create website record                     │
│     b) documents table: store action.md + overview.md           │
│     c) chunks table: chunking + OpenAI vector embeddings        │
└─────────────────────┬───────────────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────────────┐
│  5. Update task status                                          │
│     UPDATE build_tasks                                          │
│     SET stage='knowledge_build', stage_status='completed'       │
└─────────────────────────────────────────────────────────────────┘
```

## Core Module Responsibilities

| Module              | Responsibility                     | Input → Output                    |
| ------------------- | ---------------------------------- | --------------------------------- |
| **Crawler**         | Crawl website, parse HTML          | URL → WebContext                  |
| **Analyzer**        | Call Claude to generate handbook   | WebContext → HandbookOutput       |
| **DocumentChunker** | Chunk Markdown documents           | content → chunks[]                |
| **EmbeddingClient** | Generate vector embeddings         | text → Vec<f32> (1536 dimensions) |
| **TaskRunner**      | Polling loop + signal handling     | -                                 |
| **TaskProcessor**   | Orchestrate entire processing flow | BuildTask → source_id             |

## Generated Handbook Structure

### action.md - Action Guide

```yaml
title: Page title
intro: Introduction
elements:
  - name: Element name
    description: Element description
    states: Element state list
    interactions: Interaction methods
actions:
  - name: Action name
    description: Action description
    element: Target element
    location: Element location
    steps: Action steps
best_practices:
  - title: Practice title
    description: Practice content
error_handling:
  - scenario: Error scenario
    solution: Solution
```

### overview.md - Page Overview

```yaml
title: Page title
url: Page URL
overview: Functional description
features: Main feature list
important_notes: Important notes
url_patterns: URL patterns
navigation: Navigation structure
filter_categories: Filter categories
```

## Data Flow

```
Website URL
    │
    ▼
WebContext (HTML + structured elements)
    │
    ▼
HandbookOutput (action + overview JSON)
    │
    ├──► sources table (website info)
    │
    ├──► documents table (2 records: action.md, overview.md)
    │
    └──► chunks table (5-20 records, with vector embeddings)
```

## Relationship with Other Components

```
API Service                    knowledge-builder-any           action-builder
     │                                  │                            │
     │ POST /api/build-tasks            │                            │
     │ (create task)                    │                            │
     ▼                                  │                            │
build_tasks ──────────────────────────►│                            │
(stage=init, status=pending)           │                            │
                                       │ poll + process              │
                                       ▼                            │
build_tasks ◄─────────────────────────│                            │
(stage=knowledge_build,                │                            │
 status=completed)                     │                            │
                                       │                            │
sources ◄──────────────────────────────┤                            │
documents ◄────────────────────────────┤                            │
chunks ◄───────────────────────────────┘                            │
     │                                                              │
     └──────────────────────────────────────────────────────────────►
                            (follow-up processing, not implemented)
```

## Detailed Processing Steps

### Step 1: Poll for Tasks

```rust
// src/db/build_tasks.rs
pub async fn fetch_pending_task(pool: &DbPool) -> Result<Option<BuildTask>> {
    sqlx::query_as::<_, BuildTask>(
        r#"
        SELECT * FROM build_tasks
        WHERE source_category = 'any'
          AND stage = 'init'
          AND stage_status = 'pending'
        ORDER BY created_at ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_optional(pool)
    .await?
}
```

Uses `FOR UPDATE SKIP LOCKED` for concurrency safety - multiple Workers won't claim the same task.

### Step 2: Crawl Website

```rust
// src/crawler.rs
pub async fn crawl(&self, url: &str) -> Result<WebContext> {
    // 1. HTTP GET to fetch HTML
    let response = self.client.get(url).send().await?;
    let html = response.text().await?;

    // 2. Parse HTML
    let document = Html::parse_document(&html);

    // 3. Extract structured data
    let interactive_elements = self.extract_interactive_elements(&document);
    let content_blocks = self.extract_content_blocks(&document);
    let navigation = self.extract_navigation(&document);

    Ok(WebContext {
        base_url: url.to_string(),
        title: self.extract_title(&document),
        interactive_elements,
        content_blocks,
        navigation,
        // ...
    })
}
```

Extracted content includes:

- **Interactive elements**: Buttons, input fields, dropdowns, links
- **Content blocks**: Articles, lists, cards, and other info sections
- **Navigation**: Menus, breadcrumbs
- **Forms**: Search boxes, filters

### Step 3: AI Analysis

```rust
// src/analyzer.rs
pub async fn analyze(&self, context: &WebContext) -> Result<HandbookOutput> {
    // 1. Build prompt
    let prompt = self.build_analysis_prompt(context);

    // 2. Call Claude CLI (via cc-sdk)
    let mut stream = query(prompt, self.options.clone()).await?;

    // 3. Collect response
    let mut response_text = String::new();
    while let Some(result) = stream.next().await {
        // Extract text content
    }

    // 4. Parse JSON response
    self.parse_response(&response_text, context)
}
```

The prompt includes:

- Page URL, title, description
- Interactive elements JSON
- Content blocks JSON
- HTML snippets
- Output format requirements

### Step 4: Store to Database

```rust
// src/worker/processor.rs
pub async fn process(&self, pool: &DbPool, task: &BuildTask) -> Result<i32> {
    // 1. Generate handbook
    let handbook = build_handbook_simple(url).await?;

    // 2. Create source record
    let source_id = self.ensure_source(pool, url, &handbook).await?;

    // 3. Store documents
    let action_doc_id = self.store_document(pool, source_id, "action.md", &action_md).await?;
    let overview_doc_id = self.store_document(pool, source_id, "overview.md", &overview_md).await?;

    // 4. Chunk + embed
    self.chunk_and_embed(pool, action_doc_id, &action_md).await?;
    self.chunk_and_embed(pool, overview_doc_id, &overview_md).await?;

    Ok(source_id)
}
```

### Step 5: Chunking and Embedding

```rust
// src/chunker.rs
pub fn chunk(&self, content: &str) -> Vec<ChunkData> {
    // Chunk based on Markdown structure
    // Preserve heading hierarchy
    // Default: chunk_size=1500, overlap=200
}

// src/embedding.rs
pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
    // Call OpenAI text-embedding-3-small
    // Returns 1536-dimensional vector
}
```

## State Transitions

| Phase                | stage           | stage_status | Description                            |
| -------------------- | --------------- | ------------ | -------------------------------------- |
| Task created         | init            | pending      | Created by API Service                 |
| Task claimed         | knowledge_build | running      | Worker starts processing               |
| Processing succeeded | knowledge_build | completed    | Waiting for action-builder             |
| Processing failed    | knowledge_build | error        | Error info stored in config.last_error |

## Output Example

After processing is complete, the database will contain:

```
build_tasks:
  id=1, stage=knowledge_build, status=completed, source_id=1

sources:
  id=1, name=firstround, base_url=https://www.firstround.com

documents:
  id=1, source_id=1, title=Action Handbook, url=.../action.md
  id=2, source_id=1, title=Overview, url=.../overview.md

chunks:
  id=1, document_id=1, chunk_index=0, embedding=[0.1, 0.2, ...]
  id=2, document_id=1, chunk_index=1, embedding=[0.3, 0.4, ...]
  ...
```
