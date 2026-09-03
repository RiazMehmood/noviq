# Meeting Agent — Phase 2 Design (Capture + Persistence)

**Date:** 2026-05-03
**Status:** Draft for user review
**Predecessor:** Phase 1 (STT sidecar) — complete and verified

## Goal

End-to-end meeting capture: user clicks **Start Meeting** → app captures
microphone audio, segments it on natural pauses, transcribes each segment
via the whisper sidecar, and persists `(t0_ms, t1_ms, text)` rows to
SQLite. **Stop Meeting** finalizes the row. No UI overlay yet (Phase 3),
no diarization yet (Phase 3), no compliance/intervention yet (Phase 4+).

Success = a recorded conversation produces an ordered list of segment
rows with non-empty text matching the spoken words within whisper's
accuracy envelope, with no audio dropped while the sidecar is keeping up.

## Non-Goals (Phase 2)

- Overlay window UI (deferred to Phase 3)
- Speaker diarization
- Live transcript streaming to a frontend view
- Compliance / intervention triggers
- Minutes-of-meeting summarization
- Multi-device or system-audio capture (mic only)
- Cross-platform: Linux ALSA only this phase

## Architecture

```
+-----------------+     +-----------------+     +-------------------+     +----------------+
|  cpal capture   | --> |  VAD segmenter  | --> | sidecar transcribe| --> | MeetingsRepo   |
|  (audio thread) |     |  (worker thread)|     |  (async worker)   |     | append_segment |
+-----------------+     +-----------------+     +-------------------+     +----------------+
        |                       |                        |                        |
   ring buffer            tokio::mpsc<Segment>       SttSidecar           SQLite (echo.db)
   (200 ms preroll)         (bounded, cap=8)        (Phase 1 client)
```

Five distinct units, each with one responsibility and a narrow interface:

| Unit | File | Responsibility | Public surface |
|---|---|---|---|
| `MeetingsRepo` | `src-tauri/src/db/repositories/meetings.rs` | CRUD on meetings + segments | `start`, `stop`, `append_segment`, `get`, `list` |
| `AudioCapture` | `src-tauri/src/services/meeting/audio_capture.rs` | cpal mic → 16 kHz mono f32 frames | `start(tx) -> CaptureHandle`, `stop()` |
| `Segmenter` | `src-tauri/src/services/meeting/segmenter.rs` | VAD-gated segments with pre-roll + caps | `run(rx_audio, tx_segments)` |
| `TranscribeWorker` | `src-tauri/src/services/meeting/transcribe_worker.rs` | Pull segments → call sidecar → write to repo | `run(rx_segments, sidecar, repo, meeting_id)` |
| `MeetingService` | `src-tauri/src/services/meeting/mod.rs` | Orchestrates the four units; lifecycle owner | `start_meeting(...)`, `stop_meeting(id)` |

`SttSidecar` (Phase 1) is a lazy-spawned singleton in app state, reused
across meetings.

## Data Model

Migration `src-tauri/migrations/012_meetings.sql` (additive, no changes
to existing tables):

```sql
CREATE TABLE meetings (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title        TEXT NOT NULL,
    started_at   TEXT NOT NULL,           -- ISO 8601 UTC
    ended_at     TEXT,                    -- NULL while active
    language     TEXT NOT NULL,           -- e.g. "en", "auto"
    model_path   TEXT NOT NULL,           -- absolute path to whisper ggml
    status       TEXT NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active','stopped','error')),
    error        TEXT
);

CREATE TABLE meeting_segments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id   INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    t0_ms        INTEGER NOT NULL,        -- offset from meeting.started_at
    t1_ms        INTEGER NOT NULL,
    text         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_meeting_segments_meeting_id
    ON meeting_segments(meeting_id, t0_ms);
```

No `speaker` column yet — added when diarization lands in Phase 3.

## VAD-Gated Capture Design

**Audio path:**
- cpal default input device, native sample rate
- Resample to **16 kHz mono f32** in the capture callback (whisper native rate)
- Push 30 ms frames (480 samples) into an mpsc to the segmenter

**Resampler choice:** `rubato` crate (sinc, high quality, pure Rust). One
`SincFixedIn` instance per capture session.

**Segmenter state machine** (`webrtc-vad` aggressiveness level 2):
- `Idle` → silence frames discarded; keep last 200 ms in ring buffer (preroll)
- `Voiced` → on first voiced frame, emit preroll into the active segment buffer, then append
- Stay `Voiced` until **500 ms of trailing silence** OR **30 s segment cap**
- On flush: drop segments shorter than **300 ms** (cough/click filter), otherwise send `Segment { t0_ms, t1_ms, samples }` to the transcribe worker
- Reset to `Idle`

**Backpressure:** transcribe channel is `tokio::mpsc` capacity **8 segments**.
If full, log a `warn!` and drop the oldest by `try_recv` before sending.
A counter `dropped_segments` is stored in the `MeetingService` and exposed
later via a status command (Phase 3 UI consumes it). For Phase 2 we just
log; user sees nothing.

## Lifecycle

**`start_meeting(title, language, model_path) -> meeting_id`:**
1. `MeetingsRepo::start` inserts row with `status='active'`, returns id
2. Acquire `SttSidecar` from app state (lazy-spawn if first call)
3. `sidecar.load(model_path, language)` — fail fast → mark `status='error'` + return error
4. Spawn `AudioCapture` thread (cpal stream)
5. Spawn `Segmenter` task
6. Spawn `TranscribeWorker` task bound to `meeting_id`
7. Store the `MeetingHandle { capture, seg_task, worker_task, started_at }`
   in `MeetingService` indexed by `meeting_id`
8. Return id

**`stop_meeting(id) -> ()`:**
1. Look up handle, return error if unknown
2. Stop cpal stream → close audio channel → segmenter drains and exits → worker drains and exits
3. Await all three with a 5 s timeout (transcribe of last segment)
4. `MeetingsRepo::stop(id)` sets `ended_at` + `status='stopped'`
5. Drop handle

The whisper model stays loaded in the sidecar between meetings — no
unload on stop.

## Error Handling

| Failure | Behavior |
|---|---|
| Sidecar load fails | Meeting row marked `status='error'`, error stored, command returns Err |
| Sidecar transcribe call fails mid-meeting | Log warn, skip segment, continue; counter `transcribe_errors` |
| Sidecar process dies mid-meeting | Worker detects EOF on stdout → marks meeting `status='error'`, sets error text, stops capture |
| cpal device disappears | cpal stream error → mark `status='error'`, stop |
| DB write fails | Log error, continue capturing (don't lose audio); failure surfaced in next status query |
| Resampler error | Treated as fatal — same path as cpal error |

Phase 2 has no UI to display errors; they go to the existing tracing logs
and the meeting row's `error` column. Phase 3 surfaces them.

## Threading & Concurrency

- cpal callback runs on its own OS thread (cpal owns it)
- Segmenter, transcribe worker, and `MeetingService` lifecycle live on
  the tokio runtime
- `SttSidecar` already serializes its NDJSON stdio behind `Arc<Mutex<>>`
  (Phase 1) — safe to share across meetings, but we only run **one
  meeting at a time** in Phase 2 (enforced in `MeetingService`).

## Tauri Commands

```rust
#[tauri::command]
async fn start_meeting(
    state: State<'_, AppState>,
    title: String,
    language: String,        // "en", "ur", "auto", ...
    model_path: String,      // absolute path
) -> Result<i64, String>;

#[tauri::command]
async fn stop_meeting(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String>;
```

Registered in `src-tauri/src/lib.rs` invoke handler.
Capability: no new permission needed (no fs/shell access from frontend
beyond what's already granted).

## Dependencies Added (Cargo.toml)

```toml
cpal = "0.15"
webrtc-vad = "0.4"
rubato = "0.15"
```

All pure Rust at link time. Build dep on Linux: `libasound2-dev` (already
present on most dev machines; add to README troubleshooting).

## Testing Strategy

Phase 2 is largely I/O-bound (mic, subprocess, SQLite). The plan:

- **Unit tests, pure logic:**
  - `Segmenter` — feed synthetic 16 kHz sine + silence patterns, assert
    correct segment boundaries, preroll, min-length filter, hard cap
  - `MeetingsRepo` — in-memory SQLite (existing test pattern), CRUD round-trip
- **Integration smoke test (manual, recorded in PHR):**
  - Start meeting → speak ~30 s → stop → assert ≥1 segment row with
    non-empty text
- **No tests for cpal stream itself** — would need audio mocking
  framework that doesn't justify its complexity for Phase 2

## Risks & Mitigations

1. **Sidecar throughput < real-time** — backpressure drops oldest segment;
   warn logged. Mitigation: surface `dropped_segments` to UI in Phase 3 so
   user knows to switch to a smaller whisper model.
2. **VAD false negatives on very soft speech** — aggressiveness level 2
   chosen as the conservative default; level 1 available if reports come in.
3. **ALSA-only excludes some setups** — most PipeWire/Pulse users get
   passthrough; if a tester reports no devices, add JACK feature flag in
   a follow-up.

## Out-of-Scope Reminders (do not implement)

- Speaker diarization
- Overlay UI / live transcript view
- Multi-meeting concurrency
- System audio capture
- Compliance, interventions, summarization
- Anything in `specs/002-agent-task-overlay/` (cancelled spec, ignore)

## Acceptance Checklist

- [ ] Migration `012_meetings.sql` applies cleanly on a fresh DB
- [ ] `MeetingsRepo` CRUD round-trip test passes
- [ ] Segmenter unit tests pass (preroll, silence flush, hard cap, min-length)
- [ ] `cargo check -p echo-ai` clean (no `--features stt`)
- [ ] Manual smoke: ~30 s recording produces ≥1 non-empty segment row
- [ ] `stop_meeting` sets `ended_at` + `status='stopped'`
- [ ] PHR appended for the session
- [ ] `CHANGELOG_DEV.md` entry appended
