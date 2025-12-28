# Knowledge Builder Any - Local Testing Guide

## Prerequisites

### 1. Database

Ensure PostgreSQL is running with pgvector extension enabled:

```bash
# Start Docker PostgreSQL (with pgvector)
docker run -d \
  --name actionbook-postgres \
  -e POSTGRES_PASSWORD=passwd \
  -e POSTGRES_DB=actionbook_knowledge \
  -p 5432:5432 \
  pgvector/pgvector:pg15

# Verify running status
docker ps | grep actionbook-postgres
```

### 2. Environment Variables

```bash
# knowledge-builder-any/.env
DATABASE_URL=postgresql://postgres:passwd@localhost:5432/actionbook_knowledge
OPENAI_API_KEY=sk-proj-xxx  # Optional, for vector embeddings
```

```bash
# apps/api-service/.env
DATABASE_URL=postgresql://postgres:passwd@localhost:5432/actionbook_knowledge
```

### 3. Build Dependencies

```bash
# In monorepo root directory
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook
pnpm install
pnpm build --filter=@actionbookdev/db
```

---

## End-to-End Testing Flow

### Step 1: Clean Database

```bash
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
TRUNCATE build_tasks, chunks, documents, sources RESTART IDENTITY CASCADE;
"
```

### Step 2: Start Worker (Terminal 1)

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/knowledge-builder-any
cargo run -- worker --poll-interval 10
```

### Step 3: Start API Service (Terminal 2)

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/apps/api-service
PORT=3100 pnpm dev
```

### Step 4: Submit Test Tasks (Terminal 3)

```bash
# Test website 1: FirstRound Companies (company directory)
curl -X POST http://localhost:3100/api/build-tasks \
  -H "Content-Type: application/json" \
  -d '{"sourceUrl": "https://www.firstround.com/companies"}'

# Test website 2: Hacker News (news aggregator)
curl -X POST http://localhost:3100/api/build-tasks \
  -H "Content-Type: application/json" \
  -d '{"sourceUrl": "https://news.ycombinator.com"}'

# Test website 3: GitHub Trending (code repositories)
curl -X POST http://localhost:3100/api/build-tasks \
  -H "Content-Type: application/json" \
  -d '{"sourceUrl": "https://github.com/trending"}'
```

### Step 5: Monitor Task Status

```bash
# Via API
curl http://localhost:3100/api/build-tasks | jq

# Or query database directly
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT id, source_url, stage, stage_status, source_id FROM build_tasks;
"
```

### Step 6: Verify Results

```bash
# Count records in each table
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT 'build_tasks' as tbl, count(*) FROM build_tasks
UNION ALL SELECT 'sources', count(*) FROM sources
UNION ALL SELECT 'documents', count(*) FROM documents
UNION ALL SELECT 'chunks', count(*) FROM chunks;
"

# View generated handbook
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT id, title, url, length(content_md) as content_len FROM documents;
"

# Check vector embedding status
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT id, chunk_index,
  CASE WHEN embedding IS NULL THEN 'NULL'
  ELSE 'dim=' || array_length(embedding::real[], 1)::text END as embedding_status
FROM chunks;
"
```

---

## Unit Testing

### Worker Single-Run Mode

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/knowledge-builder-any

# First create a test task
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
INSERT INTO build_tasks (source_url, source_name, source_category, stage, stage_status)
VALUES ('https://example.com', 'Example', 'any', 'init', 'pending');
"

# Run once and exit
cargo run -- worker --once
```

### CLI Mode Testing

```bash
# Crawl only
cargo run -- crawl --url "https://news.ycombinator.com"

# Full build (output to local files)
cargo run -- build --url "https://news.ycombinator.com" --name "hackernews"
```

---

## Common Database Commands

### View Tasks

```bash
# All tasks
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT id, source_url, source_category, stage, stage_status FROM build_tasks;
"

# Pending tasks
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT * FROM build_tasks
WHERE source_category = 'any' AND stage = 'init' AND stage_status = 'pending';
"

# Failed tasks
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT id, source_url, config->>'last_error' as error FROM build_tasks
WHERE stage_status = 'error';
"
```

### View Generated Content

```bash
# Preview handbook content
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT title, left(content_md, 500) as preview FROM documents;
"

# Export full handbook to file
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -t -A -c \
  "SELECT content_md FROM documents WHERE title LIKE '%Action%' LIMIT 1;" > /tmp/action.md
```

### Reset Tasks

```bash
# Reset single task
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
UPDATE build_tasks
SET stage = 'init', stage_status = 'pending',
    knowledge_started_at = NULL, knowledge_completed_at = NULL,
    config = '{}'
WHERE id = 1;
"

# Clean associated data then reset
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
DELETE FROM chunks WHERE document_id IN (SELECT id FROM documents WHERE source_id = 1);
DELETE FROM documents WHERE source_id = 1;
DELETE FROM sources WHERE id = 1;
UPDATE build_tasks SET source_id = NULL, stage = 'init', stage_status = 'pending' WHERE id = 1;
"
```

### Full Cleanup

```bash
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
TRUNCATE build_tasks, chunks, documents, sources RESTART IDENTITY CASCADE;
"
```

---

## Troubleshooting

### 1. Worker Cannot Connect to Database

```
Error: Database connection failed
```

**Solution**:

```bash
# Check if Docker is running
docker ps | grep actionbook-postgres

# Check .env file
cat .env

# Test connection
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "SELECT 1;"
```

### 2. Claude AI Returns Non-JSON

```
Error: Failed to parse Claude response as JSON
```

**Cause**: Custom prompt file interfering with JSON format requirements

**Solution**: Worker mode uses `build_handbook_simple()` which skips custom prompts. If still having issues:

```bash
# Delete custom prompt files
rm -rf handbooks/*/prompt.md
```

### 3. Vector Embedding Fails

```
WARN: Embeddings enabled but OPENAI_API_KEY not set
```

**Solution**:

```bash
# Set environment variable
export OPENAI_API_KEY=sk-proj-xxx

# Or disable embeddings
cargo run -- worker --no-embeddings
```

### 4. Task Stuck in Running Status

May be caused by a previous worker exiting abnormally.

**Solution**:

```bash
# Reset stuck tasks
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
UPDATE build_tasks
SET stage = 'init', stage_status = 'pending'
WHERE stage_status = 'running';
"
```

### 5. API Service Returns 404

```
404: This page could not be found
```

**Cause**: May have run the wrong project, or port is occupied

**Solution**:

```bash
# Check port usage
lsof -i :3100

# Ensure starting from correct directory
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/apps/api-service
PORT=3100 pnpm dev
```

---

## Expected Results

After successfully processing one task, the database should contain:

| Table       | Record Count | Description                             |
| ----------- | ------------ | --------------------------------------- |
| build_tasks | 1            | stage=knowledge_build, status=completed |
| sources     | 1            | Website basic info                      |
| documents   | 2            | action.md + overview.md                 |
| chunks      | 5-20         | Depends on document size                |

Task state transition:

```
init:pending → knowledge_build:running → knowledge_build:completed
```
