# rust-learner

Learn Rust language features and crate updates using Actionbook + agent-browser.

## Installation

Add the marketplace and install the plugin:

```bash
claude plugin marketplace add actionbook/actionbook
claude plugin install rust-learner@actionbook-marketplace
```

## ⚠️ Required Setup

**After installing, you MUST configure permissions for background agents:**

### Option 1: Quick Setup (Recommended)

Run this in your project directory:

```bash
mkdir -p .claude
cat >> .claude/settings.local.json << 'EOF'
{
  "permissions": {
    "allow": [
      "Bash(agent-browser *)"
    ]
  }
}
EOF
```

### Option 2: Manual Setup

Add to your project's `.claude/settings.local.json`:

```json
{
  "permissions": {
    "allow": [
      "Bash(agent-browser *)"
    ]
  }
}
```

### Option 3: Via Claude UI

Run `/permissions` and add:
- `Bash(agent-browser *)`

## Usage

After setup, you can:

- Ask about Rust versions: "What's new in Rust 1.83?"
- Query crates: "What's the latest tokio version?"
- Check dependencies: "Analyze my Cargo.toml dependencies"

## Commands

- `/rust-learner:rust-features [version]` - Get Rust version changelog
- `/rust-learner:crate-info <crate>` - Get crate information

## How it works

1. Uses **actionbook MCP** to get website selectors
2. Uses **agent-browser** (in background) to fetch web content
3. Summarizes information for you
