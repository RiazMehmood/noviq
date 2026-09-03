---
id: 045
title: Force GNU bfd linker to fix ntfs3 lld EINVAL
stage: misc
date: 2026-05-20
surface: agent
model: claude-opus-4-7
feature: general
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (none — direct request)
labels: [build, ntfs, linker, tooling]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - .cargo/config.toml
  - CHANGELOG_DEV.md
tests:
  - cargo build --no-default-features --features llm (full cold build, exit 0)
---

## Prompt

recall the memory still there is same problem while running the app — `./run-dev.sh`
fails at the link step:

```
rust-lld: error: failed to write output '.../libderive_more-...so': Invalid argument
collect2: error: ld returned 1 exit status
error: could not compile `derive_more` (lib) due to 1 previous error
```

Follow-up: "does there was not an option to get working code from git?"

## Response snapshot

Recalled the paused-embedding-test memory: the `ntfs3` kernel driver rejects
`rust-lld`'s mmap-based output writes with EINVAL. Confirmed the volume is still
mounted `ntfs3`, root partition still 95% full (Option B blocked), and Rust 1.94
defaults to `rust-lld`. Explained git cannot fix this — the build target dir is
not version-controlled and the source code is fine; the failure is environmental.

Applied Option C (chosen by user): added a `[target.x86_64-unknown-linux-gnu]`
block to `.cargo/config.toml` with `rustflags = ["-C", "link-arg=-fuse-ld=bfd"]`.
The corrupt half-written `.so` had an unkillable directory entry (EINVAL on
`ls`/`rm`), so renamed the whole target dir aside to `echo-AI-target.corrupt-20260520`
for a clean rebuild. Ran a full verification build.

## Outcome

- ✅ Impact: build unblocked on the ntfs3 volume — full cold build succeeded
  (30m09s, 0 errors), 96 MB `echo-ai` binary linked clean; `libderive_more-*.so`
  written without EINVAL.
- 🧪 Tests: `cargo build --no-default-features --features llm` → `Finished dev
  profile in 30m09s`, exit 0.
- 📁 Files: `.cargo/config.toml` (linker block added), `CHANGELOG_DEV.md` (entry).
- 🔁 Next prompts: resume embedding verification test Phase 1 — `./run-dev.sh` →
  GUI Index Folder → `test_index_dir`.
- 🧠 Reflection: bfd link is slower than lld; acceptable vs. no build. Real fix
  is ntfs-3g/ext4 (no-sudo workaround chosen here).

## Evaluation notes (flywheel)

- Failure modes observed: ntfs3 driver corrupts inodes on lld mmap-writes,
  leaving directory entries that resist `rm`.
- Graders run and results (PASS/FAIL): PASS — verification build exit 0.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): if incremental dev-build link time
  becomes painful, evaluate `mold` (needs install) over `bfd`.
