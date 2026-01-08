# Actionbook Action Builder

Capability Builder for recording website UI element selectors. Uses LLM + Stagehand to automatically discover UI elements and extract selectors.

## Requirements

- **Node.js**: `>=20.0.0 <21.0.0` (Node 20.x LTS)
  - ⚠️ **Important**: Node.js 21+ is currently **not supported** due to Stagehand dependency incompatibility
  - Stagehand's dependency `buffer-equal-constant-time` uses the deprecated `SlowBuffer` API, which was removed in Node.js 21+
  - Use `nvm use 20` to switch to the correct version

## Quick Start

### Option 1: Automated Mode (Recommended)

Uses Task Queue Coordinator to automatically process all pending build_tasks:

```bash
# Install dependencies
pnpm install

# Start coordinator (auto-processes all pending build_tasks)
pnpm coordinator

# Development mode with auto-reload
pnpm dev
```

### Option 2: Manual Mode (Task CLI)

For manual control over individual tasks:

```bash
# View task status
pnpm task:status

# Create tasks for a source
pnpm task:create 1 10

# Run pending tasks manually
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

## Task Queue Coordinator

Concurrent task execution architecture with stateless recovery and automatic retry.

### Architecture

The Task Queue Coordinator implements a 3-layer architecture:

```
┌─────────────────────────────────────────────────────────────────┐
│                        Coordinator                              │
│  • Manages multiple build_tasks concurrently (max N)           │
│  • Claims new build_tasks when slots available                 │
│  • Monitors metrics every 30s                                  │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────────┐  ┌──────────────────┐                    │
│  │ BuildTaskRunner  │  │ BuildTaskRunner  │  ... (N runners)   │
│  │ [build_task #1]  │  │ [build_task #2]  │                    │
│  │                  │  │                  │                    │
│  │ • Generate tasks │  │ • Generate tasks │                    │
│  │ • Poll status    │  │ • Poll status    │                    │
│  │ • Retry failed   │  │ • Retry failed   │                    │
│  └────────┬─────────┘  └────────┬─────────┘                    │
│           │                     │                               │
│           ▼                     ▼                               │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │            Database (Task Queue)                        │   │
│  │  recording_tasks: pending → running → completed/failed  │   │
│  └─────────────────────────────────────────────────────────┘   │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │       RecordingTaskQueueWorker (Global Consumer)        │   │
│  │  • Consumes pending tasks (all build_tasks)             │   │
│  │  • M concurrent execution slots                         │   │
│  │  • Each task = independent browser                      │   │
│  │  • Recovers stale tasks automatically                   │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Key Features

- ✅ **Concurrent build_tasks**: Process multiple sources simultaneously (max N tasks)
- ✅ **Global task queue**: All recording_tasks consumed from unified queue (max M concurrent)
- ✅ **Stateless recovery**: Automatic recovery after crashes (no lost progress)
- ✅ **Retry logic**: Failed tasks auto-retry up to max attempts
- ✅ **Stale detection**: Tasks with no heartbeat for 15+ minutes auto-recovered
- ✅ **Real-time monitoring**: Metrics output every 30 seconds
- ✅ **Graceful shutdown**: Wait for running tasks to complete (with timeout)

### Commands

```bash
# Start coordinator with default settings
pnpm coordinator

# Development mode with auto-reload
pnpm dev
```

### Configuration

Environment variables (all optional with sensible defaults):

| Variable | Default | Description |
|----------|---------|-------------|
| `ACTION_BUILDER_MAX_CONCURRENT_BUILD_TASKS` | `5` | Max concurrent build_tasks |
| `ACTION_BUILDER_BUILD_TASK_POLL_INTERVAL_SECONDS` | `5` | Build task polling interval |
| `ACTION_BUILDER_BUILD_TASK_STALE_TIMEOUT_MINUTES` | `15` | Stale build_task timeout (for crash recovery) |
| `ACTION_BUILDER_TASK_CONCURRENCY` | `3` | Recording task concurrency (global) |
| `ACTION_BUILDER_CHECK_INTERVAL_SECONDS` | `5` | Status check interval |
| `ACTION_BUILDER_MAX_ATTEMPTS` | `3` | Max retry attempts for failed tasks |
| `ACTION_BUILDER_STALE_TIMEOUT_MINUTES` | `15` | Stale recording_task timeout |
| `ACTION_BUILDER_TASK_TIMEOUT_MINUTES` | `10` | Single task execution timeout |
| `ACTION_BUILDER_HEADLESS` | `true` | Run browser in headless mode |
| `ACTION_BUILDER_MAX_TURNS` | `30` | Maximum LLM turns per recording task |
| `ACTION_BUILDER_QUIET` | `true` (dev/coordinator) | Quiet mode: only task-level logs to console, detailed logs to file |

### Quiet Mode

By default, `pnpm dev` and `pnpm coordinator` run in quiet mode (`ACTION_BUILDER_QUIET=true`):

**Console output (quiet mode):**
- ✅ Task-level logs: `[Coordinator]`, `[BuildTaskRunner]`, `[QueueWorker]`, `[Metrics]`
- ✅ Warnings and errors: Always shown
- ❌ ActionRecorder details: Browser operations, LLM calls, element registration

**File output:**
- ✅ All logs (including ActionRecorder details) written to `logs/action-builder_*.log`

**Commands:**
```bash
# Quiet mode (recommended for production)
pnpm coordinator              # Only task-level logs to console
pnpm dev                      # Same as above

# Verbose mode (for debugging)
pnpm coordinator:verbose      # All logs to console
pnpm dev:verbose              # Same as above
```

### Output Example (Quiet Mode)

```
==========================================================
Task Queue Coordinator
==========================================================
Configuration:
  Max Concurrent Build Tasks: 5
  Build Task Poll Interval: 5s
  Recording Task Concurrency: 3
  Stale Timeout: 15 minutes
  Task Timeout: 10 minutes
  Max Attempts: 3

[Coordinator] Starting with maxConcurrentBuildTasks=5
[QueueWorker] Starting with concurrency=3
[Metrics] build_tasks=0/5, recording_tasks=0/3, elapsed=0.0s

[Coordinator] Starting BuildTaskRunner #123
[BuildTaskRunner #123] Generating recording tasks...
[BuildTaskRunner #123] Generated 50 recording tasks
[QueueWorker] Starting task #2001
[QueueWorker] Starting task #2002
[QueueWorker] Starting task #2003

[Metrics] build_tasks=1/5, recording_tasks=3/3, elapsed=30.0s
  #123 [arxiv.org] tasks=2+0/50 (4.0%) elapsed=0.5min

[BuildTaskRunner #123] Status: pending=45, running=3, completed=2, failed=0
[QueueWorker] Task #2001 completed
[QueueWorker] Starting task #2004

[Metrics] build_tasks=1/5, recording_tasks=3/3, elapsed=30.0s
  #123 [arxiv.org] tasks=15+1/50 (32.0%) elapsed=1.5min

...

[BuildTaskRunner #123] All recording tasks finished
[BuildTaskRunner #123] Published version 2 (Blue-Green deployment)
[BuildTaskRunner #123] Completed successfully
[Coordinator] BuildTaskRunner #123 completed

[Metrics] build_tasks=0/5, recording_tasks=0/3, elapsed=30.0s
```

**Metrics format:**
- `build_tasks=X/Y`: X running build_tasks out of Y max concurrent
- `recording_tasks=X/Y`: X running recording_tasks out of Y concurrency limit
- Per build_task details:
  - `#123`: build_task ID
  - `[arxiv.org]`: source name
  - `tasks=15+1/50`: 15 completed + 1 failed / 50 total
  - `(32.0%)`: completion percentage
  - `elapsed=1.5min`: time since build_task started

**Detailed logs in file** (`logs/action-builder_20260108153000.log`):
```
[2026-01-08T15:30:00.000Z] [INFO] [ActionRecorder] Starting capability recording
[2026-01-08T15:30:01.234Z] [INFO] [ActionRecorder] --- Turn 1/30 --- URL: https://example.com
[2026-01-08T15:30:01.567Z] [INFO] [ActionRecorder] Executing: navigate(url=https://example.com)
[2026-01-08T15:30:02.890Z] [INFO] [ActionRecorder] Result: {"success":true,"url":"https://example.com"}
...
```

### Heartbeat Mechanism

The system uses different heartbeat mechanisms for different task types:

**build_task heartbeat:**
- Uses `updated_at` field (updated every 5s by BuildTaskRunner)
- Stale threshold: 15 minutes (default, configurable via `ACTION_BUILDER_BUILD_TASK_STALE_TIMEOUT_MINUTES`)
- Stale detection: Coordinator detects `action_build/running` tasks with `updated_at < threshold`
- Recovery: Stale build_tasks are re-claimed with priority over new tasks

**recording_task heartbeat:**
- Uses `last_heartbeat` field (updated every 5s during execution)
- Stale threshold: 15 minutes (default, configurable via `ACTION_BUILDER_STALE_TIMEOUT_MINUTES`)
- Stale detection: QueueWorker detects `running` tasks with `last_heartbeat < threshold`
- Recovery: Stale recording_tasks reset to `pending` (if attemptCount < max) or `failed` (if exhausted)

### Failure Handling

The system handles task failures with automatic retry:

**Recording task failures:**
- Failed tasks with `attemptCount < maxAttempts`: Auto-retry (reset to `pending`)
- Failed tasks with `attemptCount >= maxAttempts`: Marked as permanent failure (status = `failed`)
- Retry trigger: BuildTaskRunner polls and detects failed tasks, then resets retriable ones

**Build task completion:**
- **Partial success allowed**: build_task completes even with permanent failures
- Completion condition: `pending=0 AND running=0 AND retriedCount=0`
- Permanent failures (attemptCount >= maxAttempts) do NOT block build_task completion
- Final status: `action_build/completed` (check recording_tasks for individual results)

**Attempt counting:**
- `attemptCount` represents **execution count** (increments on each execution attempt)
- Normal failure: TaskExecutor marks `failed` + `attemptCount+1` → BuildTaskRunner retries (no increment) → Next execution `attemptCount+1` again
- Stale recovery: QueueWorker detects stale → `attemptCount+1` + reset to `pending` → Next execution continues

### Stateless Recovery

The system is fully stateless - all state stored in database:

**After crash/restart:**
1. Coordinator starts, detects stale running tasks (no heartbeat for 15+ min)
2. Stale tasks reset to `pending` (if attemptCount < max), or `failed` (if attempts exhausted)
3. QueueWorker resumes consuming pending tasks
4. BuildTaskRunners continue polling their build_tasks
5. No progress lost, execution continues seamlessly

## Database Schema

| Table | Description |
|-------|-------------|
| `sources` | Website metadata (domain, name, base_url) |
| `source_versions` | Version management for Blue-Green deployment |
| `documents` | Crawled pages |
| `chunks` | Document chunks with content |
| `build_tasks` | Build pipeline tasks (knowledge_build → action_build) |
| `recording_tasks` | Recording tasks for each chunk |
| `elements` | Discovered UI elements |

### Task Flow

**Coordinator Mode (Automated):**

```
build_tasks (knowledge_build/completed)
         ↓
    Coordinator claims & starts BuildTaskRunner
         ↓
    BuildTaskRunner generates recording_tasks (pending)
         ↓
    QueueWorker consumes & executes (→ running → completed/failed)
         ↓
    BuildTaskRunner polls & retries failures
         ↓
    All tasks finished → build_task (action_build/completed)
         ↓
    elements created in database + YAML output
         ↓
    source_versions updated (status: 'active')
    previous version archived (Blue-Green deployment)
```

**Manual Mode (Task CLI):**

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
│   ├── task-worker/       # Task Queue Architecture
│   │   ├── coordinator.ts           # Coordinator (main orchestrator)
│   │   ├── build-task-runner.ts     # BuildTaskRunner (per build_task)
│   │   ├── recording-task-queue-worker.ts  # QueueWorker (global consumer)
│   │   ├── task-generator.ts        # Task generation
│   │   ├── task-executor.ts         # Task execution
│   │   ├── task-query.ts            # Task queries
│   │   └── utils/
│   │       ├── prompt-builder.ts
│   │       └── chunk-detector.ts
│   ├── writers/           # Output writers
│   │   ├── YamlWriter.ts
│   │   └── DbWriter.ts
│   ├── ActionBuilder.ts   # Main coordinator
│   └── index.ts
├── scripts/
│   ├── coordinator.ts     # Coordinator entry script
│   └── task-cli.ts        # Task CLI
├── test/
│   ├── coordinator.ut.test.ts           # Coordinator unit tests
│   ├── coordinator.integration.it.test.ts  # Integration tests
│   ├── coordinator.benchmark.it.test.ts    # Performance benchmarks
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

# Development mode (auto-reload + coordinator, quiet mode)
pnpm dev  # Runs with ACTION_BUILDER_QUIET=true

# Development mode (verbose - all logs to console)
pnpm dev:verbose

# Build watch only (no coordinator)
pnpm dev:build

# Run tests (all 215 tests)
pnpm test

# Run specific test file
pnpm test test/coordinator.integration.it.test.ts

# Run E2E pipeline
pnpm firstround:pipeline
```

**Quiet Mode (default):**
- Console: Only task-level logs (Coordinator, BuildTaskRunner, QueueWorker, Metrics)
- File: All logs including ActionRecorder details (browser ops, LLM calls)
- Log files: `logs/action-builder_*.log`

**Verbose Mode:**
- All logs output to both console and file
- Useful for debugging specific issues

## License

MIT
