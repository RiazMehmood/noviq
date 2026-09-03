---
id: 0010
title: CPU core scaling test — doubling cores made it 40% slower
stage: misc
date: 2026-09-03
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (conversational)
labels: [perf, cpu, avx512, rapid-compute, benchmark, docker]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - deploy/docker/RAPID-COMPUTE-VMS.md
  - deploy/docker/env.example
  - CHANGELOG_DEV.md
  - "Desktop/Noviq_CPU_Deployment_Test_Report_2026-09-03.docx"
  - "Desktop/Noviq_CPU_Core_Scaling_Test_2026-09-03.docx"
tests:
  - name: 16 official Solid Waste PDFs indexed
    result: PASS — 16/16 in 3 min 52 s, 2,933 chunks, no OCR needed
  - name: Grounded answering with citations (browser + API)
    result: PASS
  - name: Single-user latency, 32-core Ice Lake baseline
    result: 114.2 s mean, 33.7 chars/s
  - name: Single-user latency, 64-core EPYC-Rome
    result: 134.9 s mean, 20.2 chars/s — 40% WORSE
---

## Prompt

Across the session: index the Solid Waste PDFs (official PDFs only); record indexing
time; run it in Chrome; test concurrency with a second user; segregate response time
into LLM inference versus other work, as a table; report core and RAM utilisation;
check whether Rapid Compute increased the cores; re-test; produce one-page DOCX
reports on the Desktop; single-user data only; do not create another user.

## Response snapshot

**Corpus.** The 16 official Solid Waste PDFs indexed in **3 min 52 s** — 2,933
structural chunks, 4,692 embedding vectors, all classified `act`/`rules`, none
needing OCR. ⚠️ On CPU indexing is **embed-bound (92% of wall clock, 22 chunks/s)**,
the reverse of the GPU box, so capacity must be estimated from chunk counts.

**Where response time goes.** From the app's own query trace: retrieval and
preparation **84–100 ms**, LLM inference **114–135 s**, post-processing **1–2 ms**.
Model inference is **99.9%** of a response; scope resolution, question embedding,
vector + keyword search, ranking, context build, prompt assembly, grounding
validation, reviewer judge, citation verification and compose together total under
100 ms. Nothing outside the model is worth optimising.

**The core-count test, and its result.** Rapid Compute doubled VM-1 to 64 vCPU and
94 GB — and the system became **~40% slower** (output rate 33.7 → 20.2 chars/s;
decode 13.2 → 7.2 tok/s; prefill only −9%). They had not added cores to the same
machine: they moved it to **AMD EPYC-Rome (Zen 2, 2019 — an older generation)**,
losing AVX-512/VNNI. Confirmed in `/proc/<pid>/maps`: llama.cpp dropped from
`libggml-cpu-icelake.so` to `libggml-cpu-haswell.so`. Decode is largely
memory-bandwidth-bound so extra cores add little, while the weaker instruction set
costs a lot. **A 32-core AVX-512 box beat a 64-core box without it.**

**Two traps caught before they became wrong answers.** (1) The first re-run returned
44 ms / 49 ms / 8.3 s marked `cache_hit` with byte-identical answers — Noviq's
Tier-0 semantic cache, on by default in release. Any benchmark that repeats a
question measures the cache. Fixed with `ECHO_QUERY_CACHE=0`. (2) I had claimed from
a `grep` that PG 18.6 was embedded; only running the container actually established
it. Earlier I also reported a 75/25 decode/prefill split from a cumulative reading
dominated by a concurrent run — clean single-user is 59/41.

**Operational.** The reconfiguration rebooted VM-1, killing llama.cpp (tmux does not
survive a reboot): port 8080 dead, Noviq with no backend, no alarm. `-t 32` was
hardcoded, so a plain restart would have used half the new cores.

## Outcome

- ✅ Impact: a clear, actionable procurement finding — ask for AVX-512, not cores —
  plus a measured latency decomposition showing the application tier is irrelevant
  to response time.
- 🧪 Tests: indexing PASS; grounded answering PASS; both latency baselines recorded.
- 📁 Files: `RAPID-COMPUTE-VMS.md` (sections 7–12), `env.example`, `CHANGELOG_DEV.md`,
  two DOCX reports on the Desktop.
- 🔁 Next prompts: ask Rapid Compute for an AVX-512 host; make llama.cpp a systemd
  unit; decide whether to re-enable `ECHO_QUERY_CACHE`; remove the `loadtest` account.
- 🧠 Reflection: every important result today came from measuring rather than
  reasoning — the cache hit, the CPU swap and the PG version would all have been
  reported wrongly on inference alone.

## Evaluation notes (flywheel)

- Failure modes observed: I twice stated a conclusion from weak evidence (a `grep`
  for the PG version; a cumulative metrics reading for the decode split) and had to
  correct both. Also created a `loadtest` user for concurrency, which the architect
  later did not want.
- Graders run and results (PASS/FAIL): ingestion PASS; latency decomposition PASS;
  core-scaling comparison PASS but confounded (three variables changed at once).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): get an Ice Lake or Genoa host and rerun
  the identical four questions with `ECHO_QUERY_CACHE=0` — that isolates the ISA as
  the cause rather than inferring it.
