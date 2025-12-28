# Knowledge Builder Deployment Guide

## Overview

The Knowledge Builder service uses [cc-sdk](https://crates.io/crates/cc-sdk) (Claude Code SDK) for AI-powered handbook generation. The SDK supports multiple authentication methods depending on your environment.

## Claude Code CLI

The cc-sdk requires the Claude Code CLI to communicate with Claude AI. The SDK is configured with **automatic CLI download** enabled.

### Auto-Download Behavior

When the service starts, the SDK will:

1. **Search for existing CLI** in this order:

   - System PATH (`claude`, `claude-code`)
   - SDK cache directory
   - Common installation locations (`~/.npm-global/bin/`, `/usr/local/bin/`, etc.)

2. **Auto-download if not found**:

   - Downloads via npm: `@anthropic-ai/claude-code`
   - Or via official script: `https://claude.ai/install.sh`

3. **Cache location**:
   - macOS: `~/Library/Caches/cc-sdk/cli/claude`
   - Linux: `~/.cache/cc-sdk/cli/claude`
   - Windows: `%LOCALAPPDATA%\cc-sdk\cli\claude.exe`

### Manual Installation (Optional)

If you prefer to pre-install the CLI:

```bash
# Option 1: npm (recommended)
npm install -g @anthropic-ai/claude-code

# Option 2: Official script
curl -fsSL https://claude.ai/install.sh | bash
```

### Code Configuration

The auto-download is enabled in `src/analyzer.rs`:

```rust
let options = ClaudeCodeOptions::builder()
    .max_turns(3)
    .auto_download_cli(true)  // Enables automatic CLI download
    .build();
```

## LLM Configuration

### Local Development

For local development, you can use your **Claude Code subscription account**. The cc-sdk will automatically detect and use your local Claude Code authentication.

```bash
# No additional configuration needed if you have Claude Code installed and authenticated
cargo run -- worker
```

### Production Deployment

For production environments, use **AWS Bedrock** with a bearer token. This provides:

- Enterprise-grade reliability
- Usage tracking and billing
- No dependency on local Claude Code installation

#### Required Environment Variables

```bash
# AWS Bedrock Configuration (required for production)
AWS_ACCESS_KEY_ID=your_access_key_id
AWS_SECRET_ACCESS_KEY=your_secret_access_key
AWS_REGION=us-east-1
AWS_BEARER_TOKEN_BEDROCK=your_bearer_token
AWS_BEDROCK_MODEL=us.anthropic.claude-sonnet-4-20250514-v1:0
```

#### Available Claude Models on Bedrock

| Model             | Model ID                                       | Use Case                           |
| ----------------- | ---------------------------------------------- | ---------------------------------- |
| Claude Sonnet 4   | `us.anthropic.claude-sonnet-4-20250514-v1:0`   | Balanced performance (recommended) |
| Claude Opus 4.1   | `us.anthropic.claude-opus-4-1-20250805-v1:0`   | Complex analysis tasks             |
| Claude Sonnet 4.5 | `us.anthropic.claude-sonnet-4-5-20250929-v1:0` | Latest Sonnet                      |
| Claude Haiku 4.5  | `us.anthropic.claude-haiku-4-5-20251001-v1:0`  | Fast, cost-effective               |
| Claude 3.5 Sonnet | `us.anthropic.claude-3-5-sonnet-20241022-v2:0` | Previous generation                |

## Environment Files

### Local (.env)

```bash
# Database
DATABASE_URL=postgresql://postgres:passwd@localhost:5432/actionbook_knowledge

# Optional: HTTP Proxy (if needed)
# HTTPS_PROXY=http://127.0.0.1:7890
# HTTP_PROXY=http://127.0.0.1:7890

# cc-sdk will use local Claude Code authentication automatically
```

### Production (.env.production)

```bash
# Database (AWS RDS)
DATABASE_URL=postgres://user:pass@host:5432/db?sslmode=require

# HTTP Proxy (if needed for API access)
HTTPS_PROXY=http://127.0.0.1:7890
HTTP_PROXY=http://127.0.0.1:7890

# AWS Bedrock (required)
AWS_ACCESS_KEY_ID=your_access_key_id
AWS_SECRET_ACCESS_KEY=your_secret_access_key
AWS_REGION=us-east-1
AWS_BEARER_TOKEN_BEDROCK=your_bearer_token
AWS_BEDROCK_MODEL=us.anthropic.claude-sonnet-4-20250514-v1:0
```

## Deployment Steps

### 1. Build Release Binary

```bash
cd services/knowledge-builder-any
cargo build --release
```

### 2. Configure Environment

```bash
# Copy and edit production environment
cp .env.example .env.production
# Edit .env.production with your production values
```

### 3. Run Worker

```bash
# With production environment
cp .env.production .env
./target/release/handbook_builder worker --poll-interval 30
```

## cc-sdk Authentication Priority

The cc-sdk uses the following authentication priority:

1. **AWS Bedrock** - If `AWS_BEARER_TOKEN_BEDROCK` is set
2. **Local Claude Code** - Falls back to local Claude Code installation

This means:

- **Production**: Set `AWS_BEARER_TOKEN_BEDROCK` to use Bedrock
- **Local**: Leave it unset to use your Claude Code subscription

## Troubleshooting

### LLM Connection Issues

```bash
# Check if AWS credentials are set
echo $AWS_BEARER_TOKEN_BEDROCK

# Test with verbose logging
RUST_LOG=debug cargo run -- worker
```

### Proxy Issues

If you're behind a firewall or in regions with restricted access:

```bash
# Add proxy configuration
export HTTPS_PROXY=http://127.0.0.1:7890
export HTTP_PROXY=http://127.0.0.1:7890
```

### Model Not Available

If you get model availability errors, check:

1. Your AWS region supports the model
2. Your account has access to Claude models on Bedrock
3. The model ID is correctly formatted

## Monitoring

### Check Worker Status

```bash
# View running tasks
psql "$DATABASE_URL" -c "
SELECT id, source_url, stage, stage_status
FROM build_tasks
WHERE stage_status = 'running';"
```

### View Logs

```bash
# Run with detailed logging
RUST_LOG=handbook_builder=debug cargo run -- worker
```
