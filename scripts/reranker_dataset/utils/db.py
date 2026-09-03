"""Read-only access to echo.db. Never opens for write."""
import sqlite3
from pathlib import Path
from scripts.reranker_dataset.config import ECHO_DB

def _conn() -> sqlite3.Connection:
    if not ECHO_DB.exists():
        raise FileNotFoundError(f"echo.db not found at {ECHO_DB}")
    c = sqlite3.connect(f"file:{ECHO_DB}?mode=ro", uri=True)
    c.row_factory = sqlite3.Row
    return c

def load_sources() -> list[dict]:
    with _conn() as c:
        rows = c.execute(
            "SELECT id, file_name, file_path, format, indexing_status "
            "FROM sources WHERE indexing_status = 'indexed'"
        ).fetchall()
    return [dict(r) for r in rows]

def load_chunks() -> list[dict]:
    """Return leaf chunks only (the units actually retrieved)."""
    with _conn() as c:
        rows = c.execute(
            "SELECT chunks.id, chunks.source_id, chunks.content, "
            "       chunks.chunk_index, chunks.page_number, "
            "       sources.file_name AS source_name "
            "FROM chunks JOIN sources ON sources.id = chunks.source_id "
            "WHERE chunks.unit_type = 'leaf' "
            "  AND sources.indexing_status = 'indexed'"
        ).fetchall()
    return [dict(r) for r in rows]

def fts_search(query: str, limit: int = 20) -> list[dict]:
    """BM25 via SQLite FTS5. Returns chunk dicts with bm25 score."""
    sanitized = " ".join(t for t in query.split() if t.isalnum() or "-" in t)
    if not sanitized:
        return []
    with _conn() as c:
        rows = c.execute(
            "SELECT chunks.id, chunks.source_id, chunks.content, "
            "       sources.file_name AS source_name, "
            "       bm25(chunks_fts) AS score "
            "FROM chunks_fts "
            "JOIN chunks ON chunks.rowid = chunks_fts.rowid "
            "JOIN sources ON sources.id = chunks.source_id "
            "WHERE chunks_fts MATCH ? AND chunks.unit_type = 'leaf' "
            "ORDER BY score LIMIT ?",
            (sanitized, limit),
        ).fetchall()
    return [dict(r) for r in rows]
