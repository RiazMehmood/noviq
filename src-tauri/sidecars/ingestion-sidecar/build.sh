#!/usr/bin/env bash
# Freeze the Echo-AI ingestion sidecar (Phase 9a — pdfplumber tables) into a
# single self-contained binary with PyInstaller, then drop it where the Rust
# pipeline discovers it (Tauri externalBin convention:
#   src-tauri/binaries/ingestion-sidecar-<target-triple>).
#
# Indexing-time only, optional: a build without this binary still indexes (tables
# fall back to the projection engine). Re-run after changing ingestion_sidecar.py.
#
# Usage:  cd src-tauri/sidecars/ingestion-sidecar && ./build.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
BINARIES="$REPO/src-tauri/binaries"
# Build on a NATIVE filesystem, not the NTFS data partition: ntfs-3g returns
# spurious ENOSPC on some pip/PyInstaller writes (e.g. cryptography's *.pyi)
# despite free space. Only the final frozen binary is copied back to BINARIES.
WORKROOT="${ECHO_SIDECAR_BUILD_DIR:-$HOME/.cache/echo-ingestion-sidecar-build}"
BUILD_VENV="$WORKROOT/.build-venv"
DIST="$WORKROOT/dist"
WORK="$WORKROOT/build"
mkdir -p "$WORKROOT"

# Rust host target triple → binary suffix Tauri expects.
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
OUT="ingestion-sidecar-$TRIPLE"

echo "==> target triple: $TRIPLE"
echo "==> building venv: $BUILD_VENV"
python3 -m venv "$BUILD_VENV"
# shellcheck disable=SC1091
source "$BUILD_VENV/bin/activate"
python -m pip install --upgrade pip >/dev/null
# Pin for reproducibility (G2 golden gate). pdfplumber is MIT; pdfminer.six MIT.
# 2026-06-27: pinned 0.11.5 (was 0.11.10). pdfplumber 0.11.6+ hard-require
# pdfminer.six versions (==20250327 / ==20260107 …) that have no installable
# files on PyPI, so the freeze fails to resolve. 0.11.5 pins pdfminer.six
# ==20231228 (published) and its table API is unchanged for our use.
python -m pip install "pdfplumber==0.11.5" "pyinstaller==6.11.1"

echo "==> recording bundled-wheel licenses"
python -m pip install pip-licenses >/dev/null 2>&1 || true
python -m piplicenses --format=markdown --with-urls \
  > "$HERE/BUNDLED_LICENSES.md" 2>/dev/null || \
  echo "(pip-licenses unavailable — record licenses manually)" > "$HERE/BUNDLED_LICENSES.md"

echo "==> freezing with PyInstaller"
rm -rf "$DIST" "$WORK" "$WORKROOT"/*.spec
pyinstaller \
  --onefile \
  --name "$OUT" \
  --collect-all pdfplumber \
  --collect-all pdfminer \
  --distpath "$DIST" \
  --workpath "$WORK" \
  --specpath "$WORKROOT" \
  "$HERE/ingestion_sidecar.py"

mkdir -p "$BINARIES"
cp "$DIST/$OUT" "$BINARIES/$OUT"
chmod +x "$BINARIES/$OUT"
deactivate

echo "==> done: $BINARIES/$OUT"
ls -lh "$BINARIES/$OUT"
echo "==> smoke test (SPPRA — expect ok with 0 table pages):"
echo '{"op":"extract_tables","pdf_path":"'"$HOME"'/Desktop/table_ingestion_test/02_SPPRA_Rules_2010_Revised_2013.pdf","pages":"all"}' \
  | "$BINARIES/$OUT" | head -c 200
echo
