"""Verify Stage A — confirm all 6 Sindh govt docs are indexed."""
import sys
from scripts.reranker_dataset.config import SINDH_DOCS_DIR
from scripts.reranker_dataset.utils import db

EXPECTED_FORMATS = {".pdf", ".docx", ".txt", ".md"}

def main() -> int:
    if not SINDH_DOCS_DIR.exists():
        print(f"ERROR: {SINDH_DOCS_DIR} does not exist", file=sys.stderr)
        return 2
    expected = [
        p for p in SINDH_DOCS_DIR.iterdir()
        if p.is_file() and p.suffix.lower() in EXPECTED_FORMATS
    ]
    expected_names = {p.name for p in expected}
    print(f"Expecting {len(expected_names)} indexed docs: {sorted(expected_names)}")

    sources = db.load_sources()
    indexed_names = {s["file_name"] for s in sources}
    missing = expected_names - indexed_names
    if missing:
        print(f"NOT YET INDEXED: {sorted(missing)}")
        print("Open EchoAI app, add folder, wait for indexing to complete.")
        return 1

    chunks = db.load_chunks()
    by_src: dict[str, int] = {}
    for c in chunks:
        by_src[c["source_name"]] = by_src.get(c["source_name"], 0) + 1
    print("\nIndexed chunk counts per source:")
    for name in sorted(expected_names):
        print(f"  {name}: {by_src.get(name, 0)} chunks")
    total = sum(by_src.get(n, 0) for n in expected_names)
    print(f"\nTotal Sindh-acts chunks: {total}")
    if total < 100:
        print("WARN: low chunk count — re-check indexing settings", file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    sys.exit(main())
