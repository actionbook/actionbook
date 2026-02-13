# Camoufox REST API Server

Real Camoufox browser automation server providing REST API for browser control.

## Features

- ✅ Real Camoufox browser (anti-bot fingerprint spoofing)
- ✅ Accessibility tree extraction (optimized for AI agents)
- ✅ Tab management (create, navigate, screenshot)
- ✅ Element interaction (click, type by element ref)
- ✅ REST API compatible with mock server

## Prerequisites

- Python 3.10 or higher
- pip package manager

## Installation

### 1. Create Virtual Environment

```bash
cd packages/actionbook-rs/python/camoufox-server
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate
```

### 2. Install Dependencies

```bash
pip install -r requirements.txt
```

### 3. Install Playwright Firefox

Camoufox uses Firefox, so you need to install Playwright's Firefox browser:

```bash
playwright install firefox
```

## Usage

### Start Server

```bash
python main.py
```

Server will start on `http://0.0.0.0:9377`

### Test with curl

```bash
# Health check
curl http://localhost:9377/health

# Create tab
TAB_ID=$(curl -s -X POST http://localhost:9377/tabs \
  -H "Content-Type: application/json" \
  -d '{"userId":"test","sessionKey":"test","url":"https://example.com"}' \
  | jq -r '.id')

echo "Tab ID: $TAB_ID"

# Get accessibility tree
curl "http://localhost:9377/tabs/$TAB_ID/snapshot?user_id=test"

# Take screenshot
curl "http://localhost:9377/tabs/$TAB_ID/screenshot?user_id=test"

# Click element (e1 from accessibility tree)
curl -X POST "http://localhost:9377/tabs/$TAB_ID/click" \
  -H "Content-Type: application/json" \
  -d '{"userId":"test","elementRef":"e1"}'

# Type text
curl -X POST "http://localhost:9377/tabs/$TAB_ID/type" \
  -H "Content-Type: application/json" \
  -d '{"userId":"test","elementRef":"e2","text":"hello world"}'

# Navigate to new URL
curl -X POST "http://localhost:9377/tabs/$TAB_ID/navigate" \
  -H "Content-Type: application/json" \
  -d '{"userId":"test","url":"https://httpbin.org/html"}'
```

## Integration with actionbook-rs

### 1. Start Camoufox Server

```bash
cd packages/actionbook-rs/python/camoufox-server
source venv/bin/activate
python main.py
```

### 2. Start Extension Bridge

```bash
# In another terminal
cd packages/actionbook-rs
cargo run --bin actionbook -- --extension browser bridge start
```

### 3. Run Commands

```bash
# In another terminal
cd packages/actionbook-rs

# Navigate
cargo run --bin actionbook -- --extension --camofox browser goto https://example.com

# Screenshot (real screenshot now!)
cargo run --bin actionbook -- --extension --camofox browser screenshot test.png

# Get accessibility tree
cargo run --bin actionbook -- --extension --camofox browser html
```

## API Endpoints

### GET /health
Health check endpoint.

**Response:**
```json
{
  "server": "camoufox-rest-api",
  "status": "ok",
  "version": "0.6.2",
  "browser": "real"
}
```

### POST /tabs
Create a new tab and navigate to URL.

**Request:**
```json
{
  "user_id": "string",
  "session_key": "string",
  "url": "https://example.com"
}
```

**Response:**
```json
{
  "id": "uuid",
  "url": "https://example.com"
}
```

### GET /tabs/:id/snapshot
Get accessibility tree for tab.

**Query Params:**
- `user_id` (string, required)

**Response:**
```json
{
  "tree": {
    "role": "document",
    "name": "Example Domain",
    "children": [
      {
        "role": "heading",
        "name": "Example Domain",
        "elementRef": "e1"
      }
    ]
  }
}
```

### POST /tabs/:id/click
Click element by element ref.

**Request:**
```json
{
  "user_id": "string",
  "element_ref": "e1"
}
```

### POST /tabs/:id/type
Type text into element.

**Request:**
```json
{
  "user_id": "string",
  "element_ref": "e2",
  "text": "hello"
}
```

### POST /tabs/:id/navigate
Navigate tab to new URL.

**Request:**
```json
{
  "user_id": "string",
  "url": "https://httpbin.org"
}
```

### GET /tabs/:id/screenshot
Take screenshot and return base64 PNG.

**Query Params:**
- `user_id` (string, required)

**Response:**
```json
{
  "data": "base64_encoded_png_data"
}
```

## Architecture

```
FastAPI Server (port 9377)
    ↓
CamofoxBrowserManager
    ↓
Playwright + Camoufox
    ↓
Firefox Browser (Camoufox)
```

## Anti-Bot Testing

Test on bot detection sites:

```bash
# Terminal 1: Start server
python main.py

# Terminal 2: Test
curl -X POST http://localhost:9377/tabs \
  -H "Content-Type: application/json" \
  -d '{"userId":"test","sessionKey":"test","url":"https://bot.sannysoft.com"}' \
  | jq -r '.id'

# Take screenshot
TAB_ID="<tab-id-from-above>"
curl "http://localhost:9377/tabs/$TAB_ID/screenshot?user_id=test" \
  | jq -r '.data' | base64 -d > bot-test.png

# View screenshot
open bot-test.png  # Should show green checks (undetected)
```

## Troubleshooting

### Import Error: camoufox not found

```bash
pip install camoufox
```

### Browser Not Found

```bash
playwright install firefox
```

### Port Already in Use

Change port in `main.py`:
```python
uvicorn.run(app, host="0.0.0.0", port=9378)  # Use different port
```

## Development

### Running Tests

```bash
pytest
```

### Type Checking

```bash
mypy main.py browser.py models.py
```

### Linting

```bash
ruff check .
```

## License

Same as actionbook-rs (parent project)
