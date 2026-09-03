---
id: 046
title: Fix fastembed embedder — ORT version and IntxLNK stubs
stage: misc
date: 2026-05-20
surface: agent
model: claude-opus-4-7
feature: general
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (none — direct request)
labels: [debugging, embedder, fastembed, onnxruntime, ntfs]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - run-dev.sh
  - .ort-runtime/libonnxruntime.so.1.20.1
  - .fastembed_cache/models--nomic-ai--nomic-embed-text-v1.5/snapshots/
  - CHANGELOG_DEV.md
tests:
  - none yet — end-to-end re-index verification pending (next session)
---

## Prompt

Session spanned several messages. Key prompts:
1. "why it is taking too much time to embed this simple single document?
   stuck at 35% not proceeding above this?"
2. "i shifted the folder to other drive at ubuntu home then again indexed now
   it is showing failed to load fasembed model why?"
3. "save the process upto now, we will continue from this point later"

## Response snapshot

Applied systematic-debugging. The "35% stuck" was not slowness — indexing
*failed* at the embedding step (`failed to initialise fastembed model`); chunks
were inserted but 0 embeddings written, and the UI never showed the failure.
Thread states confirmed nothing was computing.

Root cause came in two layers:
1. **ORT version mismatch.** `run-dev.sh` auto-globbed `ORT_DYLIB_PATH` to
   `libonnxruntime.so.1.24.3`; the Rust `ort 2.0.0-rc.9` / `fastembed 4.9.1`
   need ONNX Runtime 1.20.x. Vendored `libonnxruntime.so.1.20.1` into
   `.ort-runtime/`, patched `run-dev.sh`. Log confirmed `Loaded ONNX Runtime
   dylib with version '1.20.1'` — but the embedder STILL failed.
2. **IntxLNK symlink stubs.** The `.fastembed_cache` snapshot files were
   112/166-byte `IntxLNK\x01` Windows-style symlinks (model downloaded on the
   Windows side); the Linux ntfs3 driver can't follow them, so fastembed read
   junk instead of the model. Copied the 5 real blobs over the 5 snapshot
   stubs; verified `model.onnx` is now 547 MB with valid ONNX protobuf header.

Moving the test folder to the Ubuntu home had no effect — the failure is in the
embedder runtime/cache, not the source PDF location.

## Outcome

- ✅ Impact: both embedder failure causes fixed; embedder NOT yet verified
  end-to-end (re-index pending).
- 🧪 Tests: none — verification deferred to next session.
- 📁 Files: `run-dev.sh`, new `.ort-runtime/`, `.fastembed_cache` snapshot
  content, `CHANGELOG_DEV.md`.
- 🔁 Next prompts: clean the DB of junk sources, restart `./run-dev.sh`,
  re-index test PDF, confirm embeddings, then Phase 2 A-F inspection.
- 🧠 Reflection: the ORT version WARN was a real bug but a partial red herring —
  fixing it did not resolve the init failure. Should have pulled the underlying
  error / inspected the cache layout before concluding root cause the first time.

## Evaluation notes (flywheel)

- Failure modes observed: HF-hub symlinks created on Windows are unusable on
  Linux ntfs3; embedder error wrapping (`failed to initialise fastembed model`)
  hides the underlying cause — only `{e}` logged, not the source chain.
- Graders run and results (PASS/FAIL): n/a — verification pending.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): log `{e:#}` in embedder error paths
  so the underlying ort/fastembed error surfaces without code spelunking.
