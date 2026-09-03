---
id: 130
title: Web search, NotebookLM benchmark, golden extraction set
stage: misc
date: 2026-07-30
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform, long session) corpus fixes → Next-steps offer → web search → NotebookLM benchmark → golden set
labels: [websearch, searxng, prompts, benchmark, extraction, testing, ops]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src-tauri/src/services/websearch/{mod,searxng}.rs
    change: new — self-hosted SearXNG lane
  - path: src-tauri/src/services/heading_detector/golden.rs
    change: new — golden extraction set (WIP, fixtures not generated)
  - path: scripts/build_golden_fixtures.py
    change: new — fixture generator, marker-located
  - path: src-tauri/config/personas/bureaucrat.toml
    change: contextual Next-steps offer; ordering vs the web section
  - path: src/pages/ChatPage.tsx
    change: Next-steps chip, web toggle + banner, web results section, scope-banner collapse
tests:
  - name: cargo test --lib (VM, NOVIQ_TEST_PG_INSTALL_DIR)
    result: PASS — 845 passed / 0 failed at last run
  - name: golden extraction set
    result: NOT YET EXERCISED — fixtures pending generation
---

## Prompt

Long session. In order: finish the corpus cleanup items (1–4, 8, 9); make the
"Next steps" offer contextual with a per-user opt-out; verify the provider tab;
build web search — free and independent of credits; then a series of defect
reports from live use; then "how can you prove NotebookLM is not perfect?"; then
the layout-extraction groundwork. Ending: "save progress upto now we will start
tomorrow from there".

## Response snapshot

### Corpus (all verified on prod after re-index)

Secretariat 184 → **186/187**, SPPRA **91/91**, E&D **14/14**, SLGA replaced with
the real 150-page principal Act (**161/161**) after finding the indexed file was
a scanned *amendment* act byte-identical to lgdsindh's own upload. Gap report
went **9 documents / 16 gaps → 4 / 5**, every survivor diagnosed. Two refusals
recorded with reasons (Secretariat 153 has no dot in the source; RoB 13 prints
its number twice and matching it would loosen the pattern corpus-wide).

### Features

- **Next steps**: contextual offer, per-user opt-out (`user_preference`,
  migration 017), rendered as a clickable chip that sends on click.
- **Web search**: self-hosted SearXNG, installed **rootless** (standalone Python
  3.11 + `systemctl --user`), free and credit-free. Exclusive mode with nothing
  attached; attachment + web when a file is attached, with the query built from
  the **instruments the document cites** rather than its title.
- Config: `ECHO_EMBED_GPU=0` (CUDA was never in the build — the log claimed
  `gpu=true` from the flag, not the provider), `ECHO_PROVISION_NORM=1`.

### The result that mattered most

The architect ran a five-test benchmark against NotebookLM on this corpus.
**It passed all five**, including two designed to expose passage-level citation
and two Noviq still fails. On the Delegation printing table it got all five
limits, correctly reconciled "Full Powers" against the Rs.500,000 **NOC
threshold**, cited `Second Schedule, Part-I, item 4(b)(viii)` (verified exact),
and correctly applied Rule 7 of the main rules to a Schedule item — while our own
extraction of that table has the **columns fused**.

My prediction was wrong and I said so. Consequence recorded: "our answers are
more accurate" is not a claim to make, this comparison must stay out of the
NIC/SEIC material, and layout extraction is a competitive gap rather than a
robustness nice-to-have.

### Golden set (WIP)

Twelve cases, each one real page that has broken this pipeline, asserting what the
scan must and must not produce. Normalises before scanning (several fixes live in
`normalize`); two cases `xfail` with reasons; a second test asserts the fixtures
exist so the set cannot pass on zero cases; fixtures located by marker text
because page numbers move. **Fixtures not generated** — the VM was unreachable at
session end.

## Outcome

- ✅ Impact: corpus effectively clean; web search live end to end; two config
  defects closed; a benchmark that corrects the product's competitive story.
- 🧪 Tests: 845/0 at last full run. Golden set not yet exercising anything.
- 📁 Files: as listed. Golden harness + generator **uncommitted**.
- 🔁 Next: generate the fixtures and commit; then the two citation-integrity bugs
  (renumbering instead of dropping; grounding score not enforced) — both affect
  ordinary legal answers, not just web mode.
- 🧠 Reflection: eleven deploys on web search, and most rounds were me correcting
  the previous round's assumption rather than a code defect — wrong format guessed
  instead of read from a trace, an instruction that told the model to do the
  opposite of what was wanted, a scope override that silently outranked a new
  rule. The golden set exists because that loop is too slow to keep paying for.

## Evaluation notes (flywheel)

- Failure modes observed: guessing an output format instead of reading it from a
  trace; adding an instruction without checking which existing instruction it
  competes with; changing an interaction (chip sends on click) without revisiting
  what the generated text must therefore be; `\` line-continuations inside a Rust
  RAW string silently breaking a regex.
- Graders run and results: corpus gap check PASS; NotebookLM benchmark 5/5 to
  the competitor.
- Next experiment: generate the fixtures, then attempt the layout extractor and
  judge it against them — better, or merely different.
