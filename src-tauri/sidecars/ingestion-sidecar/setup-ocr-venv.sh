#!/usr/bin/env bash
# Phase 9b — create the PaddleOCR venv the ingestion sidecar's `ocr` op runs in.
#
# OCR is heavy (PaddlePaddle) and indexing-time only; unlike the 9a table binary
# we do NOT freeze it — it runs from a venv. The IBA GPU server (where OCR
# belongs) runs Python directly, so a venv is the right shape; freezing buys
# nothing there. On CPU it's ~1.7 min/page; install a GPU paddlepaddle build on
# the server for the speedup (PaddleOCR auto-detects it).
#
# The Rust client (`ocr_sidecar.rs`) finds the interpreter via `ECHO_OCR_PYTHON`,
# else the default path created below. PaddleOCR is the DEFAULT once this venv
# exists (`ECHO_PADDLE_OCR=0` opts out); tesseract is the fallback.
#
# Usage:  cd src-tauri/sidecars/ingestion-sidecar && ./setup-ocr-venv.sh
set -euo pipefail

VENV="${ECHO_OCR_VENV:-$HOME/.cache/echo-ocr-venv-build/.ocr-venv}"
echo "==> creating PaddleOCR venv at: $VENV"
mkdir -p "$(dirname "$VENV")"

if command -v uv >/dev/null 2>&1; then
  uv venv "$VENV" --python 3.10
  VIRTUAL_ENV="$VENV" uv pip install "paddleocr==3.7.0" paddlepaddle
else
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install --upgrade pip
  "$VENV/bin/pip" install "paddleocr==3.7.0" paddlepaddle
fi

echo "==> verifying"
"$VENV/bin/python" -c "import paddleocr; print('paddleocr', paddleocr.__version__)"
echo "==> done. Enable OCR with ECHO_PADDLE_OCR=1 (point ECHO_OCR_PYTHON at $VENV/bin/python if non-default)."
echo "    Note: the first OCR run downloads PP-OCRv6 models (~tens of MB) into ~/.paddlex/."
echo "    CPU crash workaround is built into the sidecar (enable_mkldnn=False)."
