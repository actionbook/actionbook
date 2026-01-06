# Actionbook Action Builder

Capability Builder for recording website UI element selectors. Uses LLM + Stagehand to automatically discover UI elements and extract selectors.

## Requirements

- **Node.js**: `>=20.0.0 <21.0.0` (Node 20.x LTS)
  - ⚠️ **Important**: Node.js 21+ is currently **not supported** due to Stagehand dependency incompatibility
  - Stagehand's dependency `buffer-equal-constant-time` uses the deprecated `SlowBuffer` API, which was removed in Node.js 21+
  - Use `nvm use 20` to switch to the correct version

## Quick Start

```bash
# Install dependencies
pnpm install

# View task status
pnpm task:status

# Create tasks for a source
pnpm task:create 1 10

# Run pending tasks
pnpm task:run 1 2
```

## Environment Variables

Create a `.env` file (see `.env.example` for full documentation):

```bash
# Required - Database
DATABASE_URL=postgres://user:pass@localhost:5432/actionbook

# LLM Provider (choose ONE - auto-detected by priority)
# Priority: OPENROUTER > OPENAI > ANTHROPIC > BEDROCK

# Option 1: OpenRouter (recommended - access to all models)
OPENROUTER_API_KEY=sk-or-v1-xxxxx
OPENROUTER_MODEL=anthropic/claude-sonnet-4

# Option 2: OpenAI directly
# OPENAI_API_KEY=sk-your-openai-key
# OPENAI_MODEL=gpt-4o

# Option 3: Anthropic directly
# ANTHROPIC_API_KEY=sk-ant-your-key
# ANTHROPIC_MODEL=claude-sonnet-4-5

# Option 4: AWS Bedrock
# AWS_ACCESS_KEY_ID=your-access-key-id
# AWS_SECRET_ACCESS_KEY=your-secret-access-key
# AWS_REGION=us-east-1
# AWS_BEDROCK_MODEL=anthropic.claude-3-5-sonnet-20241022-v2:0

# Stagehand Browser Model (optional override)
STAGEHAND_MODEL=gpt-4o

# HTTP Proxy (optional - for network-restricted environments)
# HTTPS_PROXY=http://127.0.0.1:7890
```

### Provider Notes

| Provider | AIClient | Stagehand | Proxy Support |
|----------|----------|-----------|---------------|
| OpenRouter | ✅ Yes | ✅ Yes | ✅ Yes |
| OpenAI | ✅ Yes | ✅ Yes | ✅ Yes |
| Anthropic | ✅ Yes | ✅ Yes | ❌ No |
| Bedrock | ✅ Yes | ✅ Yes | ✅ Yes |

**Note**: Stagehand uses `AISdkClient` with Vercel AI SDK to support AWS Bedrock, bypassing the model name whitelist validation.

## Task CLI

Simple task management for recording UI elements from chunks.

### Commands

```bash
# Show help
pnpm task

# Create tasks from chunks without existing tasks
pnpm task:create <source_id> [limit]

# View task status
pnpm task:status [source_id]

# Run pending tasks
pnpm task:run <source_id> [limit]

# Clear all tasks for a source
pnpm task:clear <source_id>
```

### Examples

```bash
# Create 10 tasks for source 1
pnpm task:create 1 10

# View all sources status
pnpm task:status

# View source 1 status only
pnpm task:status 1

# Run 2 pending tasks for source 1
pnpm task:run 1 2

# Clear tasks for source 1
pnpm task:clear 1
```

### Task Status Output

```
📊 Task Status

📁 Source 1: www.firstround.com
   Total: 5
   ⏳ Pending:   2
   🔄 Running:   1
   ✅ Completed: 2
   ❌ Failed:    0

   Tasks:
   ✅ Task 45: chunk=1, type=exploratory, status=completed
   ✅ Task 46: chunk=2, type=task_driven, status=completed
   ⏳ Task 47: chunk=3, type=exploratory, status=pending
   🔄 Task 48: chunk=4, type=task_driven, status=running
   ⏳ Task 49: chunk=5, type=exploratory, status=pending
```

## Build Task Worker

The Build Task Worker is a continuous polling worker that orchestrates the `action_build` stage of the build pipeline.

### Commands

```bash
# Start worker with default 30s polling interval
pnpm worker:build-task

# Run once and exit (for testing or manual runs)
pnpm worker:build-task --once

# Custom polling interval (in seconds)
pnpm worker:build-task --interval 60
```

### Worker Workflow

```
build_tasks (knowledge_build completed)
         ↓
    1. Claim task (atomic, concurrent-safe)
         ↓
    2. Generate recording_tasks from chunks
         ↓
    3. Execute all recording_tasks
         ↓
    4. Complete build_task with stats
         ↓
    5. Publish version (Blue-Green deployment)
```

### Environment Variables

The worker uses the same LLM configuration as the main ActionBuilder. Additional worker-specific options:

| Variable | Default | Description |
|----------|---------|-------------|
| `ACTION_BUILDER_HEADLESS` | `true` | Run browser in headless mode |
| `ACTION_BUILDER_MAX_TURNS` | `30` | Maximum LLM turns per recording task |
| `ACTION_BUILDER_RECORDING_TASK_LIMIT` | `500` | Max recording tasks to create per build |
| `ACTION_BUILDER_MAX_ATTEMPTS` | `3` | Max retry attempts for failed/stale tasks |
| `ACTION_BUILDER_STALE_TIMEOUT_MINUTES` | `30` | Tasks running longer than this are considered stale |
| `ACTION_BUILDER_TASK_CONCURRENCY` | `3` | Number of concurrent workers (each with own browser) |

### Output Example

```
===========================================
  Build Task Worker
===========================================
  Mode: Continuous polling
  Poll interval: 30s
  Concurrency: 3 workers
===========================================

[2024-01-15T10:30:00.000Z] Checking for tasks...
[TaskStats] knowledge_build(P:0/R:0) action_build(P:1/R:0) recording_tasks(P:15/R:0)
[WorkerStats] workers(B:0/I:3/T:3)

[WorkerPool] Starting execution for source 1 with 3 workers
[WorkerPool] Worker 0 claiming task 101 (workers: 1 busy, 2 idle)
[WorkerPool] Worker 1 claiming task 102 (workers: 2 busy, 1 idle)
[WorkerPool] Worker 2 claiming task 103 (workers: 3 busy, 0 idle)
...

✅ [BuildTaskWorker] Task 123 completed successfully!
   Recording tasks created: 15
   Recording tasks completed: 14
   Recording tasks failed: 1
   Elements created: 42
   Duration: 125.3s

[BuildTaskWorker] Published version 456 for source 1, archived version 455
[BuildTaskWorker] Sleeping for 30s...
```

## Database Schema

| Table | Description |
|-------|-------------|
| `sources` | Website metadata (domain, name, base_url) |
| `documents` | Crawled pages |
| `chunks` | Document chunks with content |
| `recording_tasks` | Recording tasks for each chunk |
| `elements` | Discovered UI elements |

### Task Flow

```
chunks (no tasks) → task:create → recording_tasks (pending)
                                         ↓
                                    task:run
                                         ↓
                               recording_tasks (completed)
                                         ↓
                                    elements (created)
```

## Chunk Types

Tasks are automatically categorized based on chunk content:

| Type | Description | Focus |
|------|-------------|-------|
| `task_driven` | Action-oriented content | Pattern selectors, repeating elements |
| `exploratory` | Overview content | All interactive elements |

## Output

### YAML Files

Capabilities are saved to `output/sites/{domain}/`:

```
output/
└── sites/
    └── www.firstround.com/
        ├── site.yaml
        └── pages/
            └── companies_directory.yaml
```

### Element Format

```yaml
elements:
  company_card:
    id: company_card
    selectors:
      - type: css
        value: main ul li
        priority: 1
        confidence: 0.75
    description: Individual company card
    element_type: list_item
    allow_methods:
      - click
      - extract
    is_repeating: true

  company_name_field:
    id: company_name_field
    selectors:
      - type: css
        value: main ul li div button h2
    description: Company name (always visible)
    element_type: data_field
    allow_methods:
      - extract
    data_key: company_name
    is_repeating: true

  company_founders_field:
    id: company_founders_field
    selectors:
      - type: xpath
        value: //dl//dt[contains(text(), 'Founder')]/../dd
    description: Company founders (visible after expanding)
    element_type: data_field
    allow_methods:
      - extract
    depends_on: company_expand_button
    visibility_condition: after_click:company_expand_button
```

## Programmatic Usage

```typescript
import { ActionBuilder } from "@actionbookdev/action-builder";

// LLM provider is auto-detected from environment variables
// Priority: OPENROUTER > OPENAI > ANTHROPIC > BEDROCK
const builder = new ActionBuilder({
  outputDir: "./output",
  headless: true,
  maxTurns: 30,
  databaseUrl: process.env.DATABASE_URL,
});

await builder.initialize();

const result = await builder.build(
  "https://www.example.com/",
  "example_scenario",
  { siteName: "Example" }
);

console.log(`Success: ${result.success}`);
console.log(`Elements: ${result.siteCapability?.pages?.home?.elements?.length}`);

await builder.close();
```

## Project Structure

```
services/action-builder/
├── src/
│   ├── browser/           # Stagehand browser wrapper
│   ├── llm/               # LLM client
│   ├── recorder/          # ActionRecorder (LLM tool loop)
│   ├── task-worker/       # Task management
│   │   ├── task-generator.ts
│   │   ├── task-executor.ts
│   │   ├── task-query.ts
│   │   └── utils/
│   │       ├── prompt-builder.ts
│   │       └── chunk-detector.ts
│   ├── writers/           # Output writers
│   │   ├── YamlWriter.ts
│   │   └── DbWriter.ts
│   ├── ActionBuilder.ts   # Main coordinator
│   └── index.ts
├── scripts/
│   └── task-cli.ts        # Task CLI
├── test/
│   └── e2e/               # E2E tests
├── output/                # Generated YAML
└── logs/                  # Log files
```

## Known Issues

1. **Anthropic proxy limitation**: Anthropic SDK does not support HTTP proxy natively. Use OpenRouter, OpenAI, or Bedrock when proxy is required.

2. **Bedrock on-demand models**: Some newer Bedrock models (e.g., Claude 4.x, Haiku 4.5) require inference profiles and don't support on-demand invocation. Use `anthropic.claude-3-5-sonnet-20241022-v2:0` or `anthropic.claude-3-haiku-20240307-v1:0`.

3. **observe_page JSON parse errors**: May occur when page has too many elements. LLM will retry.

4. **Navigation timeout**: Set to 60 seconds for slow-loading sites.

## Development

```bash
# Build
pnpm build

# Watch mode
pnpm dev

# Run tests
pnpm test

# Run E2E pipeline
pnpm firstround:pipeline
```

## License

MIT
