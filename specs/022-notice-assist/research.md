# Phase 0 Research — Notice Assist

Resolves the spec's Open Questions and the plan's NEEDS-CLARIFICATION items.

## R1 — Extraction model & engine (local Qwen3-8B)

- **Decision**: Run document analysis on the **local Qwen3-8B** through the existing
  `services/inference` generator seam (the vLLM path, F016), with `NOVIQ_NO_THINK=1` for the
  extraction call. Do NOT use the Mistral IR worker (`doc_ir::mistral_builder`).
- **Rationale**: Architect directive; honors Principle III (no second resident model — Qwen is
  already warm); Qwen3-8B extracts more reliably than Mistral-7B; keeps everything offline
  (Principle I).
- **Engine-selection caveat**: prod currently routes *chat* generation through the runtime engine
  toggle (may be Groq/Gemini). NoticeAssist extraction must target the **local** model
  specifically. Decision: call the local generator explicitly (not `current_engine()`); if the
  local model/vLLM is unavailable, degrade to the single-question fallback (R4) rather than
  silently using a remote engine. A config flag (`NOVIQ_NOTICE_EXTRACT_ENGINE`, default `local`)
  keeps this overridable without code change (Principle VI spirit).
- **Alternatives considered**: (a) Mistral 7B demand-load — rejected (second model, weaker);
  (b) use whatever chat engine is active — rejected (could send the officer's notice to a remote
  API, violating the offline intent for this sensitive flow).

## R2 — Free-text JSON extraction without llguidance

- **Decision**: Prompt Qwen to emit a single JSON object; parse tolerantly: strip markdown
  fences, locate the first balanced `{…}` object, `serde_json` parse with `#[serde(default)]` on
  every field and `deny_unknown_fields` OFF (tolerate extra keys). On any parse failure, fall back
  to R4.
- **Rationale**: llguidance grammar-constrained sampling crashes on this stack (proven twice —
  F006/F009 notes); free-text JSON + defensive parsing is the established pattern (doc_ir,
  one-ask). Qwen3-8B follows a "respond with only JSON" instruction well under `NO_THINK`.
- **Robustness measures**: cap output tokens; retry once with a terse "return ONLY the JSON"
  reminder if the first parse fails before falling back; log raw output on failure for eval.
- **Alternatives**: llguidance (crashes); regex extraction of fields (brittle across document
  shapes — defeats the "no predefined structure" goal).

## R3 — Long-document handling

- **Decision**: Feed the attachment's already-extracted text (reuse F021 attachment chunks/IR).
  If it exceeds the extraction budget, analyze the first window PLUS any chunk whose heading/first
  line matches notice cues (`show ?cause|explanation|audit para|objection|charge|memo|notice`),
  concatenated up to the budget.
- **Rationale**: Notices state the allegation up front; audit paras label objections. Cue-guided
  selection captures the salient parts without a multi-pass sweep (cost + Principle II).
- **Alternatives**: full map-reduce over the document — rejected (latency; the notice is short by
  nature; overkill).

## R4 — Graceful degradation

- **Decision**: If extraction yields no usable frame (parse fail, empty allegations, model
  unavailable), the turn asks ONE open question — "In your own words, what does this notice allege,
  and which rule/section does it cite against you?" — records the answer into a minimal frame, and
  continues the loop. Never crash, never fabricate a frame.
- **Rationale**: Principle VII (degrade gracefully, user-visible), FR-005.

## R5 — Session state & resumability

- **Decision**: Persist per-conversation state as JSON in a new additive Postgres table
  `notice_assist_session(conversation_id PK, state_json, updated_at)`, mirroring the one-ask
  `state_json` seam (`services/drafting/one_ask/state.rs`). All access behind a repository-shaped
  `session.rs`. Loop is resumable and tunnel-safe (no reliance on live SSE; recovers like
  poll_turn).
- **Rationale**: Reuse a proven pattern; survives reconnects (the tunnel drops long requests).
- **Alternatives**: in-memory session — rejected (lost on reconnect / server restart).

## R6 — `/` command palette

- **Decision**: In `ConfidentialComposer`, detect `/` at input start → open a small fixed command
  list (keyboard-navigable), reusing the `@`-mention machinery (open/query/activeIdx/pick). Picking
  inserts nothing into the corpus scope; it sets a pending command the composer submits (e.g. text
  begins with `/help`). Backend `intent::is_notice_assist_command()` recognizes the leading token.
- **Rationale**: Same interaction users just learned for `@`; minimal new UI; the `/` list is a
  static array (no fetch).
- **Commands**: `/help` (primary), aliases `/reply`, `/audit` (audit-para variant preselects the
  audit template hint). Naming is an architect preference (Open Question) — `/help` default,
  aliases cost nothing.
- **Alternatives**: a toolbar button — rejected (less discoverable, breaks the composer-native
  flow the officer is already in).

## R7 — Grounding: corpus vs attachment

- **Decision**: Grounding retrieval uses the **corpus** (the notice is the premise, not the
  source) — i.e. the shipped `seeks_legal_authority` behavior, NOT attachment-only scope. The
  cited rule text quoted in the reply comes from corpus retrieval + the citation gate.
- **Rationale**: The governing rule lives in the corpus; the notice merely names it (often
  mis-citing). Matches the 2026-07-10 attachment-vs-corpus decision.

## R8 — Corpus coverage dependency (GA gate, not a build blocker)

- **Finding**: Grounding trustworthiness depends on the relevant rulebooks being indexed: **E&D
  (Efficiency & Discipline) rules, audit/PAC rules, Rules of Business, Civil Servants Act**, and
  the departmental rules commonly cited in notices. The build does not depend on this, but GA does.
- **Decision**: Treat corpus coverage as a **GA acceptance gate** (checklist item), and when a
  cited rule is not found, the reply explicitly says so and asks the officer to supply the text —
  never invents it (FR-008/FR-011). Flag for the architect to confirm indexed coverage before
  launch.

## R9 — Integrity & ethics stance

- **Decision**: The drafted defense is bounded to what the **cited rules + the officer's real
  facts** support; weak points and missing documents are flagged as ⚠ items. The political-pressure
  situation is handled via the lawful **written-orders doctrine** (record/obtain written orders;
  accountability shifts to the directing authority), never concealment. Reuse the existing
  evidence-leash persona + citation gate.
- **Rationale**: Keeps the feature a legitimate, defensible legal-support aid (Principle X/XI),
  not an accountability-evasion tool.

## Resolved Open Questions (from spec)

- Command naming → `/help` primary + `/reply`/`/audit` aliases (R6).
- Corpus rulebook coverage → GA gate, honest-miss behavior meanwhile (R8).
- Prospective "seek orders in writing" note → **out of scope for this feature**; capture as a
  follow-up sibling command (keeps v1 focused on responding).
