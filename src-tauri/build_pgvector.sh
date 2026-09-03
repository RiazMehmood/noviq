#!/usr/bin/env bash
# T003 (Feature 011): vendor pgvector extension artifacts for offline install
# into the bundled PostgreSQL engine (research R2 / FR-004).
#
# Pinning: the pgvector artifact MUST match the bundled PostgreSQL MAJOR version
# (the archive embedded by postgresql_embedded's `bundled` feature). When the
# bundled PG major changes, re-run this script and commit the refreshed vendor
# tree. Source of artifacts: a local instance provisioned by tests/pg_spike.rs
# phase 1 (portal-corp "pgvector_compiled"), or a from-source PGXS build against
# the bundled engine's pg_config if portal-corp has no artifact for this
# platform (the spike tells you which).
#
# Usage:
#   ./build_pgvector.sh <pg-install-dir-with-pgvector> [dest=resources/pgvector/<platform>]
#
# Output layout mirrors the engine install root so the app can copy verbatim:
#   resources/pgvector/<platform>/lib/...vector*.so
#   resources/pgvector/<platform>/share/...extension/vector*
set -euo pipefail

SRC="${1:?usage: build_pgvector.sh <pg-install-dir-with-pgvector> [dest]}"
PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
DEST="${2:-$(dirname "$0")/resources/pgvector/${PLATFORM}}"

mapfile -t FILES < <(find "$SRC" -type f -name '*vector*' \
  \( -path '*/lib/*' -o -path '*/share/*' \) | sort)

if [ "${#FILES[@]}" -lt 2 ]; then
  echo "ERROR: expected pgvector .so + .control/.sql under $SRC (found ${#FILES[@]})" >&2
  exit 1
fi

echo "Vendoring ${#FILES[@]} pgvector artifacts -> $DEST"
for f in "${FILES[@]}"; do
  rel="${f#"$SRC"/}"
  mkdir -p "$DEST/$(dirname "$rel")"
  cp -v "$f" "$DEST/$rel"
done

echo "Done. Record the source pgvector version + bundled PG version below:"
"$SRC"/bin/pg_config --version 2>/dev/null || true
grep -h default_version "$DEST"/share/*/extension/vector.control "$DEST"/share/extension/vector.control 2>/dev/null || true
