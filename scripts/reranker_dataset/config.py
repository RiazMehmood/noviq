"""Central config — paths, constants, seed. Override via env vars if needed."""
import os
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

# Input paths
ECHO_DB = Path(os.environ.get(
    "ECHO_DB",
    Path.home() / ".local/share/com.echoai.app/echo.db",
))
SINDH_DOCS_DIR = Path(os.environ.get(
    "SINDH_DOCS_DIR",
    Path.home() / "Desktop/Sindh Govt Documents",
))
TRAIN_JSONL = Path(os.environ.get(
    "TRAIN_JSONL",
    Path.home() / "Desktop/fine tunning research & work/train (3).jsonl",
))
TEST_RESULTS_DIR = REPO_ROOT / "history/prompts"  # narrowed in stage_c

# Output paths
OUT_DIR = REPO_ROOT / "data/reranker/v1"
OUT_DIR.mkdir(parents=True, exist_ok=True)

# Constants
EMBED_MODEL = "BAAI/bge-base-en-v1.5"  # 768-d, matches echo.db chunk dim
RERANKER_TARGET = "BAAI/bge-reranker-base"
MAX_TOKENS = 512
NEGATIVES_PER_POSITIVE = 7
MIN_NEGATIVES_PER_POSITIVE = 3
SCORE_FLOOR = 0.30      # below = trivially unrelated
SCORE_CEILING = 0.95    # above = near-duplicate, likely false negative
MINING_TOP_K = 15
SPLIT_TRAIN_FRAC = 0.90
SEED = 20260517
MIN_TOTAL_POSITIVES = 50  # halt if dataset would be too small

# Audit
AUDIT_OVERLAP_THRESHOLD = 0.45  # Jaccard token overlap required for "grounded"
