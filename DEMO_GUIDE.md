# EchoAI Demo & Testing Guide

**Branch**: `feature/foreground-scheduler`  
**Last validated**: 2026-04-24 (T20 — foreground-priority scheduler + chat response efficiency)

---

## What's New in This Branch

### Foreground-Priority Scheduler (chat response efficiency)

EchoAI now has a **foreground-priority scheduler** that ensures chat queries always get the CPU first. Key behaviours:

| Situation | What happens |
|-----------|-------------|
| You switch to **Chat** tab | Background indexing/NER automatically **pauses** |
| You switch away from Chat | Background work **resumes** from where it stopped |
| Chat query is running | Model slot is reserved — indexing cannot take it |
| Long index job is running | **WorkloadBar** shows file count, ETA, Pause/Cancel |
| Any background work is active | **NavBadge** in the top nav shows a live count |

This replaces the old behaviour where a background index job could saturate the CPU/RAM while you were trying to ask a question.

---

## Before You Start

### What you need running

| Terminal | Command | Purpose |
|----------|---------|---------|
| Terminal 1 | `./run-dev.sh` | Starts the app (foreground-scheduler branch) |
| Terminal 2 | `./monitor-dev.sh` | Live RAM/CPU/network watch (optional but recommended) |

### Start the app

```bash
cd "/media/riaz/New Volume/echo-AI/.worktrees/foreground-scheduler"
./run-dev.sh
```

> **Note**: Run from the worktree directory, not the main repo root, so you get the scheduler branch build.

### Wipe the database (clean-slate testing)

```bash
rm ~/.local/share/com.echoai.app/echo.db \
   ~/.local/share/com.echoai.app/echo.db-shm \
   ~/.local/share/com.echoai.app/echo.db-wal 2>/dev/null; echo "DB wiped"
```

---

## Scenario P1 — Notebook + Cited Chat

**What it proves**: Offline document Q&A with source citations.

### Steps

1. Click **"New Notebook"** in the left sidebar → name it `"Test"` → confirm
2. Drag a PDF (10+ pages) into the **SourceList** drop zone on the left panel
3. Watch the **SourceStatusBadge** next to the file:
   - Grey = `pending`
   - Blue spinner = `processing`
   - **Green = `indexed`** ← wait for this before asking
4. Type in the chat: `What is the main topic of this document?`
5. Hit send

### Expected result ✓

- A meaningful answer appears referencing the document content
- At least one **CitationChip** (e.g. `[filename p.3]`) is shown inline
- Response arrives in **4–10 seconds** (vs 30–120 s before this branch)

### Timing improvements (Fix A + B + C)

| Fix | What changed | Speedup |
|-----|-------------|---------|
| A — async embed | `spawn_blocking` on ONNX embed | UI no longer freezes during query |
| B — smaller prompt | TOP_K 16→6, n_ctx 16384→4096 | Prefill: 30–90 s → 3–8 s |
| C — persistent KV cache | `clear_kv_cache()` vs `new_context()` | Saves 2–6 s + 200–500 MB per query |

---

## Scenario P2 — Semantic Folder Indexing & Global Recall

**What it proves**: Index an entire folder; find content by concept, not exact words.

### Steps

1. Go to **Settings** → **Indexed Folders** → click **Add Folder**
2. Pick a folder with **10+ mixed files** (PDFs, docs, text)
3. Watch the **NavBadge** appear in the top navigation bar — it shows `indexing N/M` live
4. Switch to the **Chat** tab while indexing is running

### Expected result ✓ (scheduler behaviour)

- The **NavBadge** updates: indexing shows as **paused** while you are in Chat
- Chat query still responds normally (model slot is not blocked)
- Switch back to Dashboard → NavBadge resumes counting

5. Wait for all files to index (badge disappears or shows 100%)
6. Open **Global Search** (top search bar, or `Ctrl+K`)
7. Type a **conceptual phrase** — an idea, not an exact sentence from the files

### Expected result ✓ (search)

- Relevant files listed with file path + excerpt within 3 seconds
- Results reflect meaning/concept, not just keyword matches

---

## Scenario P2b — WorkloadBar (Pause / Resume / Cancel)

**What it proves**: User control over long-running background index jobs.

### Steps

1. Start indexing a large folder (50+ files) via Settings → Indexed Folders → Add Folder
2. Navigate to **Dashboard** — the **WorkloadBar** appears below the notebook list
3. Observe: file count (processed / total), percentage, and **ETA** displayed
4. Click **Pause** on the WorkloadBar

### Expected result ✓

- Progress bar stops advancing
- ETA text changes to `"Paused — resumes when you return to indexing"`
- File watcher and NER workloads also pause

5. Click **Resume** → progress continues from where it stopped
6. Click **Cancel** → confirm prompt → job stops; already-indexed files remain

---

## Scenario P3 — Hardware Constraint / Quality Notice

**What it proves**: App runs on constrained hardware and notifies the user.

### ⚠️ Requires `ECHO_FORCE_TIER=3`

```bash
ECHO_FORCE_TIER=3 ./run-dev.sh
```

1. Open a notebook with 2+ indexed documents
2. Go to the **Analysis** tab → click **"Compare Sources"**

### Expected result ✓

- Template runs (not greyed out)
- Yellow banner: *"This analysis used a constrained model due to hardware limits…"*

---

## Scenario P4 — Folder Revocation

**What it proves**: Privacy — revoking a folder wipes all its content from search.

### Steps

1. Ensure P2's indexed folder is still present in **Settings → Indexed Folders**
2. **Search** → type a phrase from that folder → confirm results appear
3. **Settings → Indexed Folders** → click **Revoke** → confirm
4. **Search** → type the same phrase again

### Expected result ✓

- Search returns empty within 5 seconds of revocation

---

## Full Validation Run (All Scenarios)

```
Wipe DB → P1 → P2 (watch NavBadge + scheduler pause) → P2b (WorkloadBar) → P3 (ECHO_FORCE_TIER=3) → P4
```

Keep `monitor-dev.sh` running throughout. Confirm **online ticks = 0** throughout.

---

## Quick Reference — Commands

```bash
# Start app (foreground-scheduler branch)
cd "/media/riaz/New Volume/echo-AI/.worktrees/foreground-scheduler"
./run-dev.sh

# Start app (P3 hardware simulation only)
ECHO_FORCE_TIER=3 ./run-dev.sh

# Start monitor (separate terminal)
./monitor-dev.sh

# Wipe database
rm ~/.local/share/com.echoai.app/echo.db \
   ~/.local/share/com.echoai.app/echo.db-shm \
   ~/.local/share/com.echoai.app/echo.db-wal 2>/dev/null

# Check scheduler branch commits
cd "/media/riaz/New Volume/echo-AI/.worktrees/foreground-scheduler"
git log --oneline
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| App closes mid-indexing | systemd-oomd killed the terminal | `run-dev.sh` uses `systemd-run --scope` — already handled |
| Chat slower than expected | Index job running concurrently | Switch to Chat tab — scheduler will pause indexing automatically |
| NavBadge not appearing | No background workload registered | Start an index job first |
| WorkloadBar missing | No active index job | Add a folder with 10+ files |
| Indexing stuck after Resume | Tokio runtime issue | Stop and restart; already-indexed files will be skipped (resume plan) |
| ETA shows "Calibrating…" | Fewer than 10 files processed | Normal — EWMA warmup window is 10 samples |
| Chat says "No content found" | File not yet `indexed` | Wait for green SourceStatusBadge |
| Quality notice missing in P3 | `ECHO_FORCE_TIER=3` not set | Restart with the env var |
