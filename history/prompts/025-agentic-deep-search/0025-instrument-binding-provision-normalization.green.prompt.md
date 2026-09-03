---
id: 0025
title: Instrument binding + provision-label normalization in the hybrid-union retrieval path
stage: green
date: 2026-07-18
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: engr.riaz2010@gmail.com
command: "resolve this issue"
labels: [retrieval, deep-search, grounding, flag-gated, trace-driven]
links:
  spec: n/a
  ticket: traces bd0d16e1 / 99cdbff2 / 020ac9ac
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/deep_search/assemble.rs
  - src-tauri/src/services/deep_search/mod.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/mod.rs
  - CHANGELOG_DEV.md
tests:
  - services::query_router::tests::instrument_boost_lifts_the_statute_the_query_names
  - services::query_router::tests::instrument_boost_noops_when_the_query_names_no_statute
  - services::query_router::tests::instrument_boost_is_off_by_default
  - services::query_router::tests::provision_norm_appends_the_bare_unit_label
  - services::query_router::tests::provision_norm_is_silent_when_it_would_add_nothing
  - services::deep_search::assemble::tests::instrument_priority_puts_the_named_statute_first
  - services::deep_search::assemble::tests::instrument_priority_noops_when_no_statute_named
---

## Prompt

resolve this issue.

(Context from the preceding turns: the architect asked "what was the issue of attached document
issue and rule 4(1) missing from document corpus", which led to a trace investigation of
`bd0d16e1`, `99cdbff2` and `020ac9ac`.)

## Response snapshot

Diagnosed two production traces as one root cause and fixed it inside the existing
hybrid-union retrieval path, behind two default-off flags.

**Root cause.** A bare provision token is ambiguous across the corpus — `Rule 4` exists in the
E&D Rules 1973, APT Rules 1974, Delegation of Financial Powers 2019, the SLGA Compendium and
SPPRA — and the main query path did nothing to bind it to the statute the user named.

- `bd0d16e1` (deep search): asked for Rule 4 "from sindh civil servants efficiency and
  disciplane rules 1973"; assembled 7 Compendium / 5 E&D / 5 Delegation / 4 APT passages and
  answered out of the **Sindh Food Authority Act** with 20 citations.
- `99cdbff2` (normal path): "rule 4(I)(a) sub rule 3" retrieved 32 chunks with **zero** Rule 4
  — the words "sub rule" matched every "sub-rule (N)" in the corpus. The LLM refused correctly.
- `020ac9ac` (Notice Assist): the *inverse* failure — it HAS a hard instrument filter
  (`instrument_scope`) that is too strict, so a rule that exists is reported as
  "not found in the indexed corpus". Excluded from this change by the architect's scope choice.

**Constraint honoured.** `query_router.rs:249` records the architect's 2026-07-03 ruling that
turned `citation_fast_path` OFF: *"every query uses the single hybrid-union retrieval path — no
special-case citation routing."* This change adds no bypass; instrument match is one more
ranking signal, and nothing is ever filtered out.

**Fix 1** — `ECHO_INSTRUMENT_BOOST` (default 1.0 = off): `instrument_ranks()` shared scorer,
`apply_instrument_boost()` wired as S3c after `apply_entity_boost`, and
`PassageSet::prioritize_instrument()` for deep search before `packed()`.

**Fix 2** — `ECHO_PROVISION_NORM` (default off): `augment_provision_terms()` appends the bare
`unit_label` form (`"Rule 4"`) to the retrieval query, reusing the existing `detect_citation`.

## Outcome

- ✅ Impact: both retrieval paths can bind a provision query to the named statute; traces now
  print the packed **source mix**, so this class of failure is visible instead of silent.
- 🧪 Tests: full lib suite **759 passed / 0 failed** on the VM
  (`--no-default-features --features server`); 7 new tests built from the real trace queries,
  including the user's "disciplane" misspelling.
- 📁 Files: 4 source files + CHANGELOG_DEV.md.
- 🔁 Next prompts: enable both flags on the VM and replay `bd0d16e1` / `99cdbff2`, reading the
  new "Source mix (packed)" trace line; then decide on the Notice Assist reason-enum fix.
- 🧠 Reflection: the changelog was decisive. Grepping it first surfaced that this exact fix had
  been attempted (`citation_fast_path`) and deliberately reverted, which changed the design from
  "filter/route" to "rank" and avoided re-introducing a known-bad pattern.

## Evaluation notes (flywheel)

- Failure modes observed:
  1. **My plan named the wrong helper.** I intended to reuse `expand::detect_instrument` as the
     query→statute extractor; reading it showed its regexes require capitalised text (the real
     query is lowercase) **and** it falls back to `"the applicable local-government rules"` for
     any input — it would have fired the boost on every query. Scoring the raw query against
     document names needs no extractor at all.
  2. **A guard suppressed the case it was meant to fix.** `query.contains(base)` returned `None`
     for `"rule 4(i)(a)"` because `"rule 4"` is a substring of it. Caught by the new test.
- Graders run and results: `cargo check --lib` PASS (exit 0, no new warnings);
  `cargo test --lib` PASS (759/0).
- Prompt variant: n/a.
- Next experiment (smallest change to try): set `ECHO_INSTRUMENT_BOOST=2.0` alone (leave
  `ECHO_PROVISION_NORM` off) and replay `bd0d16e1` — isolates whether re-ranking alone fixes the
  wrong-Act answer before adding query augmentation as a second variable.
