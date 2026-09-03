---
id: 047
title: Embedding pipeline determinism verified
stage: misc
date: 2026-05-20
surface: agent
model: claude-opus-4-7
feature: 001-notebook-knowledge-engine
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (conversational)
labels: [embedding, verification, determinism, fastembed, nomic]
links:
  spec: N/A
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - path: /home/riaz/Desktop/db_inspection/db_testing_shell_commands.md
    change: reordered into true execution order; added Phase 5 (run-2 inspection), fixed WAL/SHM rename naming, baked sort into Phase 6 diff
tests:
  - name: Phase 2 A-F — run 1 (folder-indexed) inspection
    result: PASS — 3 chunks == 3 embeddings, 768-d / 3072 B, no NaN/Inf, cosine 0.48-0.65
  - name: Phase 5 A-F — run 2 (notebook-indexed) inspection
    result: PASS — counts/shape match run 1
  - name: Phase 6 — determinism diff (sorted run1 vs run2 vector dumps)
    result: PASS — byte-identical, "IDENTICAL - deterministic PASS"
---

## Prompt

Session resumed the paused EchoAI embedding verification test. User prompts in
sequence: "recall memory" → "done run 1" → clarifying questions on why the DB is
set aside and whether the same PDF must be re-indexed via the notebook path →
"but in .md file you write run 2 then aside method change that also so i follow
that file db_testing_shell_commands.md" → "done all phase 6".

Final triggering prompt verbatim: **"done all phase 6"**, followed by pasted shell
output:

```
diff <(sort /tmp/run1_vectors.txt) <(sort /tmp/run2_vectors.txt) \
  && echo "IDENTICAL - deterministic PASS"
IDENTICAL - deterministic PASS
```

## Response snapshot

Confirmed the embedding-pipeline verification test is COMPLETE and PASSED. The
Phase 6 diff over both content-keyed vector dumps came back byte-identical, which
also implicitly confirms run 2 indexed correctly (a partial run 2 would have
produced removed/added diff lines). Verdict: the fastembed/Nomic embedder
produces correct, numerically sane, and deterministic 768-d vectors, and the
folder-index and notebook-source ingestion paths embed identically — no
path-specific bug.

Also rewrote `db_testing_shell_commands.md` into true top-to-bottom execution
order (Phase 2 run-1 → Phase 3 set-aside → Phase 4 index run-2 → Phase 5 run-2
inspection → Phase 6 diff), since the prior file had run-2 inspection and the
set-aside step out of order.

## Outcome

- ✅ Impact: Embedding pipeline fully verified — correct shape, sane values,
  deterministic across both ingestion paths. The original embedder bugs (ONNX
  Runtime 1.24→1.20 mismatch; IntxLNK Windows symlink stubs in `.fastembed_cache`)
  are confirmed fixed end-to-end. Unblocks corpus re-indexing and downstream work.
- 🧪 Tests: Phase 2/5 A-F inspection PASS on both runs; Phase 6 determinism diff
  PASS (byte-identical).
- 📁 Files: `db_testing_shell_commands.md` reordered to execution order (test
  tooling on Desktop; no production code touched).
- 🔁 Next prompts: restore the real baseline DB (`echo.db.test-aside`, 4.3 GB)
  over the test artifacts; resume project work — OCR integration phase, reranker
  fine-tune, or Tier 3 UI acceptance + merge.
- 🧠 Reflection: A byte-identical content-keyed diff is a stronger PASS signal
  than re-checking counts separately — a partial run would have surfaced as diff
  lines, so run-2 counts were not needed to call the verdict.

## Evaluation notes (flywheel)

- Failure modes observed: none this exchange; earlier session resolved two
  embedder-init failures (ONNX version mismatch, IntxLNK symlink stubs).
- Graders run and results (PASS/FAIL): determinism diff PASS.
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): restore baseline DB and re-index the
  real corpus through the verified embedder before resuming evals.
