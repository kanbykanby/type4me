#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SERVER_DIR="$PROJECT_DIR/sensevoice-server"

echo "=== Building sensevoice-server standalone binary ==="

cd "$SERVER_DIR"

# Ensure venv exists
if [ ! -d .venv ]; then
    echo "Creating venv..."
    /opt/homebrew/bin/python3.12 -m venv .venv
fi

source .venv/bin/activate

# Install dependencies + pyinstaller
echo "Installing dependencies..."
pip install -q -r requirements.txt
pip install -q pyinstaller

# Clean previous build
rm -rf build dist *.spec

# Build standalone binary
echo "Running PyInstaller..."
pyinstaller \
    --onedir \
    --name sensevoice-server \
    --hidden-import=funasr \
    --hidden-import=funasr.models \
    --hidden-import=asr_decoder \
    --hidden-import=asr_decoder.ctc_decoder \
    --hidden-import=asr_decoder.context_graph \
    --hidden-import=online_fbank \
    --hidden-import=pysilero \
    --hidden-import=sentencepiece \
    --hidden-import=torch \
    --hidden-import=torchaudio \
    --hidden-import=soundfile \
    --hidden-import=uvicorn \
    --hidden-import=uvicorn.logging \
    --hidden-import=uvicorn.loops \
    --hidden-import=uvicorn.loops.auto \
    --hidden-import=uvicorn.protocols \
    --hidden-import=uvicorn.protocols.http \
    --hidden-import=uvicorn.protocols.http.auto \
    --hidden-import=uvicorn.protocols.websockets \
    --hidden-import=uvicorn.protocols.websockets.auto \
    --hidden-import=uvicorn.lifespan \
    --hidden-import=uvicorn.lifespan.on \
    --collect-all funasr \
    --collect-all asr_decoder \
    --collect-all online_fbank \
    --noconfirm \
    server.py

echo ""
echo "=== Build complete ==="
DIST="$SERVER_DIR/dist/sensevoice-server"
echo "Output: $DIST"
du -sh "$DIST" 2>/dev/null || true
echo ""
echo "Test with:"
echo "  $DIST/sensevoice-server --model-dir iic/SenseVoiceSmall --port 8765"
