---
id: 0017
title: Memory budget artifact, guardrail status, assist-Leave bug, graph-history design
stage: explainer
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [conversation-memory, context-budget, artifact, guardrails, notice-assist, graph-history, design]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr: history/adr/0022-stateful-deep-search-turn-lifecycle-checkpointing-resume-and-interruption.md
  pr:
files:
  - history/prompts/025-agentic-deep-search/0017-memory-budget-artifact-guardrails-assist-leave-graph-history.explainer.prompt.md
tests: []
---

## Prompt

we were at conversation memory so we will go for that now, and before deploying you need to test its all aspects locally first, but before any code create its graphical representation how will it work in artifacts must be interactive so i can understand its all spects like context window tokens each and every thing then we will decide to built it but first let me understand that, and one more thing is guardrails do we develop those or still pending? and one more other thing is that after we launch / commands assistant mode and while clicking on leave it just not move out from assitant mode it runs one chat query then comes out why? discuss these two and create artifacts then we move further

(follow-up) I also try to create a method where we can store previous chat in graph and extract that on demand if required previous chat records, all history upto n turn stores inside graph and grabs required details when in need never lives whole history inside context, i created an image on desktop with name GRAPH check that and understand the design, and if it is possible discuss that

## Response snapshot

**No code written.** Investigation + one interactive artifact + design discussion. Three threads:

### 1. Interactive artifact — conversation memory vs the 8192-token window
Published: https://claude.ai/code/artifact/33f19b34-10d2-4e9d-99fb-e6318ff3f8c6
(source: scratchpad `memory-budget.html`). Simulates `route_scoped_with_override` exactly:
`WINDOW 8192`, `GEN_RESERVE 1280` → `CAP 6912`, `SCAFFOLD 400`, `FLOOR 2400`, `TOPK_MIN 20`,
`SYS_BASE 1139`, deep `+142` sys and `×1.5` budget, `top_k = ctx_budget/150`.
Four presets: **Live today** · **The prod bug** (hardcoded 1500 → visible overflow + head/tail
truncation dropping the law) · **Re-land A** (4-pair) · **Re-land B** (facts + summary).
Ledger table maps every token to the code that emits it; pipeline panel shows write path (all live)
vs read path (recent_turns live; pinned facts + summary DROPPED).
Modelled (not measured) values flagged in the footer: pinned-fact and summary block sizes.

Key quantified finding: since `9f40d4aa` memory **cannot overflow** — it shrinks the law budget
instead. Re-land B ≈ **600 tok ≈ 4 chunks of law**. That is the whole trade.

### 2. Guardrails — SHIPPED, not pending
`assistant/mod.rs` — `is_greeting_or_intro` + `is_low_signal` short-circuit greetings/probes
(`a`, `test`, `123`, `asdf`) before retrieval → `NOVIQ_NUDGE`, no citations; also short-circuits the
F025 deep loop. Two gaps remain: (a) no cost-guard for 1-word REAL queries (`penalties` → full deep
loop), (b) PROBES/GREETINGS hardcoded, not TOML (Principle VI).

### 3. Notice-Assist "Leave" button — root-caused, NOT fixed
`ChatPage.tsx:1256` — `onClick={() => { setAssistMode(false); setInput("/cancel"); submit(); }}`.
Leave literally types `/cancel` and sends it: real user bubble + busy spinner + server round-trip +
assistant reply. Exactly the reported "runs one chat query then comes out". It is CHEAP (server
`is_exit` → `session::delete` → early return; `reply()` does NOT invoke the model), so it is UX
noise, not a slow turn.
**Constraint that rules out the naive fix:** the server owns authoritative state —
`notice_assist::session` row; `mod.rs:478` `assist_open` routes EVERY turn into the notice flow while
it exists. `assistMode()` is only a mirror. Dropping `submit()` → banner hides but session stays open
→ next real question silently hijacked. Same shape as the `/me` bug (one source of truth).
Latent second instance: page reload mid-assist hides the banner but leaves the session open.
Options put to user: (1) `POST /api/assist/exit` + derive `assistMode` from server session
[recommended], (2) suppress the turn's bubbles client-side.

### 4. GRAPH.png design — on-demand history retrieval
Located at `Desktop\ui\GRAPH.png` (root `graph.docx` is an empty Nov-2025 leftover). Design: turns
1..n as graph nodes → aggregate history store; context window holds system prompt + retrieved chunks
+ user query, with history entering ONLY via a narrow on-demand slot.
Assessment — possible, right direction, seams exist (`is_followup_query`, `load_for_turn`,
`prior_turns_block`, pgvector, GraphRAG build, `entity_extractor.rs`). Four findings:
- **Prize is reach, not cost.** History is already flat (~380 tok) behind the 2-pair cap. The win is
  decoupling reach from cost: turn 50 costs what turn 5 costs and can still reach turn 1.
- **Anaphora cannot be retrieved** ("compare this with that" — a pronoun has no content to embed)
  ⇒ needs an always-on 1-pair verbatim floor (~190 tok); graph handles everything older.
- **Ordinal questions do not embed** ("what was my first question?") ⇒ deterministic `seq` lookup,
  not vector search. Router must be tiered: ordinal→seq, semantic→vector, anaphora→floor.
- **Graph vs linked list:** nodes 1..n → one blob IS `chat_message.seq`. A graph only earns itself
  via traversable edges (turn→entity, turn→cited-provision). Open question put to user.
- New failure mode: "remembered wrong" replaces "forgot" — worse because invisible. Retrieved
  history must stay non-citable. Injection must land INSIDE `law_overhead` before packing, or the
  `9f40d4aa` overflow bug is rebuilt verbatim.

**Sequencing inverted by this design:** Re-land A (4-pair) CONFLICTS with "never live whole history
in context" and becomes throwaway → recommend DROP. Re-land B pinned facts (25 tok, always-on) fits
perfectly → land. Re-land B summary → fallback tier, land as v1.

## Outcome

- ✅ Impact: guardrails question closed (shipped); Leave bug root-caused with the server-state
  constraint that rules out the obvious fix; graph-history design assessed with 3 concrete
  amendments; re-land sequencing revised. One interactive artifact shipped.
- 🧪 Tests: none — no code changed. Constants verified by reading source, not recalled.
- 📁 Files: none in-repo besides this PHR; artifact source in scratchpad.
- 🔁 Next prompts: awaiting user on (a) which preset/config to build, (b) Leave fix option 1 vs 2,
  (c) turn→turn vs turn→entity edges, (d) whether to extend the artifact with a reach-vs-cost view.
  Graph-history needs spec + ADR before code.
- 🧠 Reflection: reading the code first paid — the Leave "just remove submit()" fix looks obvious and
  is actively harmful; only `mod.rs:478` reveals why. Same for guardrails, which memory said were
  shipped and the code confirmed.

## Evaluation notes (flywheel)

- Failure modes observed: none in this session. A stale `graph.docx` and a mis-scoped background
  `find` both cost a step — the user's pointer ("inside ui folder") resolved it.
- Graders run and results (PASS/FAIL): n/a — no code.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): land pinned facts alone (~25 tok) and watch the traced
  "Prompt overhead (measured)" move by exactly that much — the cheapest possible proof the
  measure-before-pack seam holds before any graph work starts.
