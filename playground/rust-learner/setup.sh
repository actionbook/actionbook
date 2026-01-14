#!/bin/bash
# rust-learner setup script
# Run this in your project directory to configure required permissions

set -e

echo "Setting up rust-learner permissions..."

# Create .claude directory if not exists
mkdir -p .claude

# Check if settings.local.json exists
if [ -f ".claude/settings.local.json" ]; then
    # Check if permissions already configured
    if grep -q "agent-browser" .claude/settings.local.json 2>/dev/null; then
        echo "✓ Permissions already configured"
        exit 0
    fi

    echo "⚠️ .claude/settings.local.json exists. Please manually add:"
    echo '  "permissions": { "allow": ["Bash(agent-browser *)"] }'
    exit 1
fi

# Create new settings file
cat > .claude/settings.local.json << 'EOF'
{
  "permissions": {
    "allow": [
      "Bash(agent-browser *)"
    ]
  }
}
EOF

echo "✓ Permissions configured in .claude/settings.local.json"
echo ""
echo "Restart Claude to apply changes."
