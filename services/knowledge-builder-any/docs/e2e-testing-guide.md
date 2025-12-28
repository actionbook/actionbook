# Actionbook Full Pipeline E2E Testing Guide

## Pipeline Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         FULL PIPELINE FLOW                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. API Service                                                             │
│     POST /api/build-tasks { sourceUrl }                                     │
│     └─► Creates build_task (stage=init, status=pending)                     │
│                              │                                              │
│                              ▼                                              │
│  2. Knowledge Builder (Rust)                                                │
│     cargo run -- worker                                                     │
│     ├─► Crawls website                                                      │
│     ├─► Claude AI analysis → handbook                                       │
│     ├─► Stores: sources, documents, chunks                                  │
│     └─► Updates: build_task (stage=knowledge_build, status=completed)       │
│                              │                                              │
│                              ▼                                              │
│  3. Action Builder (TypeScript)                                             │
│     pnpm worker:build-task                                                  │
│     ├─► Generates recording_tasks from chunks                               │
│     ├─► Stagehand browser automation                                        │
│     ├─► Records element selectors                                           │
│     ├─► Stores: pages, elements                                             │
│     └─► Updates: build_task (stage=action_build, status=completed)          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Environment Setup

### Directory Structure

```
services/
├── knowledge-builder-any/     # Rust - handbook generation
│   ├── .env                   # Local config
│   └── .env.production        # Production config
├── action-builder/            # TypeScript - selector recording
│   ├── .env                   # Local config
│   └── .env.production        # Production config
└── db/                        # Shared database schema
```

### Database URLs

| Environment    | URL                                                                                                                                        |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| **Local**      | `postgresql://postgres:passwd@localhost:5432/actionbook_knowledge`                                                                         |
| **Production** | `postgres://postgres:Rc5c45TBhtW0Q7lYaAWN@actionbookdev-lib.cu9kwc0o8f8y.us-east-1.rds.amazonaws.com:5432/actionbook_prod?sslmode=require` |

---

## Part 1: Local E2E Testing

### Step 1: Start Local Database

```bash
# Start PostgreSQL with pgvector
docker run -d \
  --name actionbook-postgres \
  -e POSTGRES_PASSWORD=passwd \
  -e POSTGRES_DB=actionbook_knowledge \
  -p 5432:5432 \
  pgvector/pgvector:pg15

# Verify
docker ps | grep actionbook-postgres

# Run migrations
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/db
pnpm migrate
```

### Step 2: Clean Database (Optional)

```bash
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
TRUNCATE build_tasks, recording_tasks, elements, pages, chunks, documents, sources, source_versions RESTART IDENTITY CASCADE;
"
```

### Step 3: Start API Service (Terminal 1)

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/apps/api-service
PORT=3100 pnpm dev
```

### Step 4: Start Knowledge Builder Worker (Terminal 2)

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/knowledge-builder-any

# Verify using local database
cat .env | grep DATABASE_URL

# Start worker
cargo run -- worker --poll-interval 10
```

### Step 5: Start Action Builder Worker (Terminal 3)

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/action-builder

# Install dependencies (first time only)
pnpm install

# Verify using local database
cat .env | grep DATABASE_URL

# Start worker
pnpm worker:build-task
```

### Step 6: Submit Test Task (Terminal 4)

```bash
# Submit a test website
curl -X POST http://localhost:3100/api/build-tasks \
  -H "Content-Type: application/json" \
  -d '{"sourceUrl": "https://news.ycombinator.com"}'
```

### Step 7: Monitor Pipeline Progress

```bash
# Watch the full pipeline status
watch -n 5 'docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT id,
       left(source_url, 40) as url,
       stage,
       stage_status as status,
       source_id
FROM build_tasks
ORDER BY created_at DESC
LIMIT 5;"'
```

### Step 8: Verify Results

```bash
# Check all tables
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT 'build_tasks' as tbl, count(*) FROM build_tasks
UNION ALL SELECT 'sources', count(*) FROM sources
UNION ALL SELECT 'documents', count(*) FROM documents
UNION ALL SELECT 'chunks', count(*) FROM chunks
UNION ALL SELECT 'recording_tasks', count(*) FROM recording_tasks
UNION ALL SELECT 'pages', count(*) FROM pages
UNION ALL SELECT 'elements', count(*) FROM elements;
"

# Check recorded elements
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge -c "
SELECT id, semantic_id, element_type, left(selectors::text, 60) as selectors
FROM elements
ORDER BY created_at DESC
LIMIT 10;
"
```

---

## Part 2: Production E2E Testing

### ⚠️ PRODUCTION WARNING

```
┌──────────────────────────────────────────────────────────────────┐
│  ⚠️  PRODUCTION DATABASE - ALL CHANGES ARE PERMANENT             │
│                                                                  │
│  • DO NOT run TRUNCATE on production                             │
│  • Test with real, useful URLs only                              │
│  • Monitor resource usage                                        │
└──────────────────────────────────────────────────────────────────┘
```

### Step 1: Switch Both Services to Production

```bash
# Knowledge Builder
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/knowledge-builder-any
cp .env .env.local.backup
cp .env.production .env

# Action Builder
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/action-builder
cp .env .env.local.backup
cp .env.production .env
```

### Step 2: Verify Production Connection

```bash
# Define production psql alias
alias prod_psql='psql "postgres://postgres:Rc5c45TBhtW0Q7lYaAWN@actionbookdev-lib.cu9kwc0o8f8y.us-east-1.rds.amazonaws.com:5432/actionbook_prod?sslmode=require"'

# Test connection
prod_psql -c "SELECT NOW();"

# Check current state
prod_psql -c "
SELECT 'build_tasks' as tbl, count(*) FROM build_tasks
UNION ALL SELECT 'sources', count(*) FROM sources
UNION ALL SELECT 'documents', count(*) FROM documents
UNION ALL SELECT 'chunks', count(*) FROM chunks
UNION ALL SELECT 'elements', count(*) FROM elements;
"
```

### Step 3: Start Production Workers

**Terminal 1 - Knowledge Builder:**

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/knowledge-builder-any
cargo run -- worker --poll-interval 30
```

**Terminal 2 - Action Builder:**

```bash
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/action-builder
pnpm worker:build-task
```

### Step 4: Submit Task to Production

```bash
# Start API Service with production DB
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/apps/api-service
DATABASE_URL="postgres://postgres:Rc5c45TBhtW0Q7lYaAWN@actionbookdev-lib.cu9kwc0o8f8y.us-east-1.rds.amazonaws.com:5432/actionbook_prod?sslmode=require" \
PORT=3100 pnpm dev

# Submit task
curl -X POST http://localhost:3100/api/build-tasks \
  -H "Content-Type: application/json" \
  -d '{"sourceUrl": "https://news.ycombinator.com"}'
```

### Step 5: Monitor Production Pipeline

```bash
# Watch task progress
watch -n 10 'psql "postgres://postgres:Rc5c45TBhtW0Q7lYaAWN@actionbookdev-lib.cu9kwc0o8f8y.us-east-1.rds.amazonaws.com:5432/actionbook_prod?sslmode=require" -c "
SELECT id, left(source_url, 35) as url, stage, stage_status, source_id
FROM build_tasks ORDER BY created_at DESC LIMIT 5;"'

# Check recording tasks
prod_psql -c "
SELECT id, source_id, status, left(start_url, 40) as url
FROM recording_tasks
ORDER BY created_at DESC
LIMIT 10;
"

# Check recorded elements
prod_psql -c "
SELECT e.id, e.semantic_id, e.element_type, p.page_type
FROM elements e
JOIN pages p ON e.page_id = p.id
ORDER BY e.created_at DESC
LIMIT 10;
"
```

### Step 6: Restore Local Environment

```bash
# Knowledge Builder
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/knowledge-builder-any
cp .env.local.backup .env

# Action Builder
cd /Users/zhangalex/Work/Projects/Grasp.ai/actionbook/services/action-builder
cp .env.local.backup .env
```

---

## Pipeline Stage Reference

| Stage              | stage             | stage_status | Service               |
| ------------------ | ----------------- | ------------ | --------------------- |
| Task Created       | `init`            | `pending`    | API Service           |
| Knowledge Building | `knowledge_build` | `running`    | knowledge-builder-any |
| Knowledge Complete | `knowledge_build` | `completed`  | knowledge-builder-any |
| Action Building    | `action_build`    | `running`    | action-builder        |
| Action Complete    | `action_build`    | `completed`  | action-builder        |
| Failed             | any               | `error`      | -                     |

---

## Test Websites

| Website         | URL                            | Notes             |
| --------------- | ------------------------------ | ----------------- |
| Hacker News     | `https://news.ycombinator.com` | Simple, fast      |
| Product Hunt    | `https://www.producthunt.com`  | Medium complexity |
| GitHub Trending | `https://github.com/trending`  | Good selectors    |
| Stack Overflow  | `https://stackoverflow.com`    | Complex           |

---

## Troubleshooting

### Task stuck at knowledge_build:completed

Action builder not picking up tasks. Check:

```bash
# Verify action-builder is running
ps aux | grep "worker:build-task"

# Check action-builder logs for errors

# Manual check: task should have source_id set
psql "$DATABASE_URL" -c "
SELECT id, source_id, stage, stage_status
FROM build_tasks
WHERE stage='knowledge_build' AND stage_status='completed';
"
```

### Recording tasks failing

```bash
# Check recording_task errors
psql "$DATABASE_URL" -c "
SELECT id, status, error_message
FROM recording_tasks
WHERE status='failed'
ORDER BY updated_at DESC
LIMIT 5;
"
```

### Browser automation issues

```bash
# Run action-builder with visible browser for debugging
HEADLESS=false pnpm worker:build-task
```

### Reset stuck tasks

```bash
# Reset knowledge_build running tasks
psql "$DATABASE_URL" -c "
UPDATE build_tasks
SET stage='init', stage_status='pending'
WHERE stage='knowledge_build' AND stage_status='running';
"

# Reset action_build running tasks
psql "$DATABASE_URL" -c "
UPDATE build_tasks
SET stage='knowledge_build', stage_status='completed'
WHERE stage='action_build' AND stage_status='running';
"
```

---

## Quick Commands Cheatsheet

```bash
# ========== LOCAL ==========

# Start all services (3 terminals)
# T1: cd apps/api-service && PORT=3100 pnpm dev
# T2: cd services/knowledge-builder-any && cargo run -- worker
# T3: cd services/action-builder && pnpm worker:build-task

# Submit task
curl -X POST http://localhost:3100/api/build-tasks \
  -H "Content-Type: application/json" \
  -d '{"sourceUrl": "https://news.ycombinator.com"}'

# Check status
docker exec actionbook-postgres psql -U postgres -d actionbook_knowledge \
  -c "SELECT id, source_url, stage, stage_status FROM build_tasks;"

# ========== PRODUCTION ==========

# Switch to production
cd services/knowledge-builder-any && cp .env.production .env
cd services/action-builder && cp .env.production .env

# Query production
psql "postgres://postgres:Rc5c45TBhtW0Q7lYaAWN@actionbookdev-lib.cu9kwc0o8f8y.us-east-1.rds.amazonaws.com:5432/actionbook_prod?sslmode=require" \
  -c "SELECT * FROM build_tasks ORDER BY created_at DESC LIMIT 5;"

# Restore local
cd services/knowledge-builder-any && cp .env.local.backup .env
cd services/action-builder && cp .env.local.backup .env
```
