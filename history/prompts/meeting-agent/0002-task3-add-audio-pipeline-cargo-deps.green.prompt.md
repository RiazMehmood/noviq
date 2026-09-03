---
id: "0002"
title: "Task 3 — Add audio pipeline Cargo deps"
stage: green
date: "2026-05-03"
surface: agent
model: claude-sonnet-4-6
feature: meeting-agent
branch: master
user: engr.riaz2010@gmail.com
command: "Meeting Agent Phase 2 Task 3 — add cpal/webrtc-vad/rubato/hound/base64 to Cargo.toml"
labels: ["cargo", "deps", "audio", "cpal", "vad", "rubato", "hound", "base64", "phase2"]
links:
  spec: "null"
  ticket: "null"
  adr: "null"
  pr: "null"
files_created: []
files_modified:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - CHANGELOG_DEV.md
tests_run: []
---

## Prompt (verbatim)

You are implementing Task 3 of the Meeting Agent Phase 2 plan in `/media/riaz/New Volume/echo-AI`.
The previous attempt was BLOCKED on a missing `libasound2-dev`; that has now been installed. Re-attempt cleanly.

The repo has many prior uncommitted files — only stage `Cargo.toml` and `Cargo.lock`.

**Steps:**

1. Read `src-tauri/Cargo.toml` `[dependencies]` section. Note whether `base64` is already present.

2. Append the missing deps to `[dependencies]` in `src-tauri/Cargo.toml`:
   ```toml
   cpal = "0.15"
   webrtc-vad = "0.4"
   rubato = "0.15"
   hound = "3.5"
   ```
   And add `base64 = "0.22"` ONLY if absent.

3. Run cargo check (this will be slow — pulling new crates + alsa-sys binding; use 600000ms timeout):
   ```
   cd "/media/riaz/New Volume/echo-AI/src-tauri" && cargo check 2>&1 | tail -30
   ```
   Expected: clean compile (warnings ok). `libasound2-dev` is now installed so the alsa-sys build should succeed.

4. If clean, commit:
   ```
   cd "/media/riaz/New Volume/echo-AI"
   git add src-tauri/Cargo.toml src-tauri/Cargo.lock
   git commit -m "deps(meetings): add cpal, webrtc-vad, rubato, hound (and base64 if missing)"
   ```

5. Verify with `git show --stat HEAD` that exactly 2 files were committed.

**Report:**
- Status: DONE | DONE_WITH_CONCERNS | BLOCKED
- Commit SHA
- Whether `base64` was already present
- `cargo check` last 5 lines
- Anything unexpected

Do NOT touch any other file.

## Response (key output)

- Read `src-tauri/Cargo.toml`: `base64` was **absent**.
- Appended `cpal = "0.15"`, `webrtc-vad = "0.4"`, `rubato = "0.15"`, `hound = "3.5"`, `base64 = "0.22"` under a new `# --- Audio capture & processing (Meeting Agent Phase 2) ---` comment block.
- `cargo check` completed in 59.95s with 1 pre-existing deprecation warning (`tauri_plugin_shell::Shell::open`), zero errors.
- Committed exactly 2 files (`Cargo.toml`, `Cargo.lock`): commit `0665592cbc7574146f53b8c784b13d0efbbb2259`.
- `git show --stat HEAD` confirmed 2 files changed.

## Outcome

Status: **DONE**
Commit SHA: `0665592cbc7574146f53b8c784b13d0efbbb2259`
`base64` was absent before this task — added.
`cargo check` last 5 lines:
```
    Checking echo-ai v0.1.0 (/media/riaz/New Volume/echo-AI/src-tauri)
warning: use of deprecated method `tauri_plugin_shell::Shell::<R>::open`: Use tauri-plugin-opener instead.
   = note: `#[warn(deprecated)]` on by default
warning: `echo-ai` (lib) generated 1 warning
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 59.95s
```
Nothing unexpected. Task 4 (SttSidecar::transcribe_samples) is next.
---
