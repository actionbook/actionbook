#!/bin/bash
# Start Camoufox REST API Server

CAMOFOX_PORT=${CAMOFOX_PORT:-9377}
SERVER_SCRIPT="$(cd "$(dirname "$0")" && pwd)/camoufox-server.py"

echo "🦊 Starting Camoufox REST API Server"
echo "===================================="
echo "Port: ${CAMOFOX_PORT}"
echo "Script: ${SERVER_SCRIPT}"
echo

# Check if port is already in use
if lsof -Pi :${CAMOFOX_PORT} -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "⚠️  Port ${CAMOFOX_PORT} is already in use"
    echo
    echo "Process using the port:"
    lsof -Pi :${CAMOFOX_PORT} -sTCP:LISTEN
    echo
    echo "To kill the process:"
    echo "  kill $(lsof -Pi :${CAMOFOX_PORT} -sTCP:LISTEN -t)"
    exit 1
fi

# Check if Python is available
if ! command -v python3 &> /dev/null; then
    echo "❌ Python 3 not found"
    echo "Please install Python 3:"
    echo "  brew install python3"
    exit 1
fi

# Check if required packages are installed
echo "📦 Checking dependencies..."
if ! python3 -c "import fastapi" 2>/dev/null; then
    echo "⚠️  FastAPI not installed"
    echo "Installing dependencies..."
    pip3 install -r requirements.txt
fi

echo
echo "🚀 Starting server..."
echo "Press Ctrl+C to stop"
echo

# Start the server
export PORT=${CAMOFOX_PORT}
python3 "${SERVER_SCRIPT}"
