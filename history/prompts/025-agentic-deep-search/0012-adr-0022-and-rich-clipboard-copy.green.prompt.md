---
id: 0012
title: ADR-0022, system prompt export, and rich-clipboard copy
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: /sp.adr stateful-deep-search-checkpointing (+ freeform)
labels: [adr, f026, deep-search, clipboard, ux, word-export]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket: n/a
  adr: history/adr/0022-stateful-deep-search-turn-lifecycle-checkpointing-resume-and-interruption.md
  pr: n/a (direct fast-forward push to release)
files:
  - history/adr/0022-stateful-deep-search-turn-lifecycle-checkpointing-resume-and-interruption.md
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
  - "C:/Users/hp/Desktop/system_prompt.docx (outside repo)"
tests:
  - "npx tsc --noEmit: clean (exit 0)"
  - "marked->HTML transform checked on a realistic E&D answer: <h2>/<h3>/<strong>/<em>/<ul><li> + Sources, & escaped, [n] left as text"
  - "post-deploy: ClipboardItem + text/html + <strong>Sources</strong> confirmed present in the served bundle"
  - "NOT tested: actual clipboard write / real Word paste in a browser"
---

## Prompt

write the ADR and also share current system prompt with me also or write it on desktop in system_prompt.docx file

(mid-turn follow-up: "and while user copy the response it copies in markdown i want to copy it for docx to paste
there so change it accordingly.")

## Response snapshot

Three deliverables.

**1. ADR-0022 — Stateful Deep-Search Turn Lifecycle: Checkpointing, Resume and Interruption** (Accepted).
Followed `/sp.adr`: loaded plan context, scanned `history/adr/`, applied the significance test (Impact —
durable state model + new table + resume/branch semantics; Alternatives — five weighed; Scope — cross-cutting
across turn lifecycle, scheduler, API, DB, frontend). Clustered rather than split: the state model, the additive
`deep_checkpoint` table with its `parent_id` branch DAG, the node-guarded `drive`, deterministic rebuild from
`state.passages`, and **cancellation-as-resumable-state** are one decision — the stop button is deliberately
folded in because it required no new architecture (the engine already had `request_cancel` +
`CancellationToken`), and because "a stopped turn is a resumable turn" is the state model, not a separate
choice. Extends ADR-0021 (which introduced the bounded loop); does not supersede it. Alternatives documented
with rejection rationale: adopt LangGraph (Python runtime vs our single Rust binary — we wanted its
*properties*, not its runtime, and those reduce to one table), event-sourced trace (fold must track loop
semantics forever; our 7 nodes are coarse), a workflow engine (disproportionate), in-memory-only (dies on
restart — one of the three motivating failures), and stop-button-only (leaves the 137s cost + no debugging
loop). Negatives recorded honestly: schema-less `state_json` with no version field, ~6 writes/turn, no TTL,
per-turn `thread_id` (so cross-process reconnect-resume still needs a stable turn id from the FE), no UI for
`resume_from`, and a driver whose correctness now depends on every super-step being node-guarded.

**2. `system_prompt.docx` on the Desktop** (45K, via python-docx, installed for this). Prose sections verbatim;
tool JSON Schemas summarized (name + purpose + load-bearing rules) with that stated up front; agents/skills
listed. **Redacted the two credentials** that appear verbatim in the injected MEMORY.md context (PG password,
admin password) — they are already plaintext in MEMORY.md, so copying them into a new Desktop document only
widens their spread for no benefit. Marked `[REDACTED — see MEMORY.md]` in place, and said so.

**3. Rich-clipboard copy (the follow-up ask).** Root cause: `copyAnswer` used
`navigator.clipboard.writeText` = **text/plain only**, so Word received literal markdown (`## Grounds`,
`**Rule 3**`). The answer already renders correctly on screen — only the clipboard was lossy. Fix: offer
**both** `text/html` and `text/plain` via `ClipboardItem`; Word/Docs prefer the HTML and render real
headings/bold/lists, Notepad still gets plain text. Reused `<AnswerBody>`'s existing pipeline (marked →
DOMPurify) rather than inventing a converter — `marked` was already a dependency. Two deliberate choices:
`gfm`/`breaks` passed **explicitly** rather than inherited from AnswerBody's module-level `setOptions` (import
side-effect = fragile), and the **raw** answer parsed rather than the linkified copy, so `[n]` markers paste as
text instead of dragging citation hover-popover markup into the document. Kept a `writeText` fallback because
`ClipboardItem` needs a secure context and isn't guaranteed in the Tauri webview — a Copy button that silently
no-ops would be worse than plain text.

Asked before deploying (F026 authorization didn't extend to this new change); user chose push. Deployed
`9e9b15d1` → release; verified `ClipboardItem` + `text/html` + the HTML Sources block are present in the
**served bundle**, not just that the timestamp moved.

## Outcome

- ✅ Impact: ADR-0022 closes the last artifact owed for F026. Copy-to-Word is live for all users — the answer
  pastes into Word as formatted text, preserving the copy-with-sources behaviour from `bdf3ed98`.
- 🧪 Tests: tsc clean; transform verified against a realistic E&D answer; post-deploy bundle grep confirms the
  code is served. The real Word paste is still unexercised — a browser click is the missing step.
- 📁 Files: ADR-0022 (new), `ChatPage.tsx` (+`answerHtmlForCopy`, `escapeHtml`, `copyAnswer` now takes the turn),
  CHANGELOG entry, `system_prompt.docx` on the Desktop (outside the repo).
- 🔁 Next prompts: click Copy in prod and paste into Word (the one thing I couldn't verify); consider the same
  rich flavour for `exportTurn` ("Copy draft"), which still writes plain text only; `deep_checkpoint` needs a
  `version` column + TTL before the flag goes on by default.
- 🧠 Reflection: the copy bug was a one-line diagnosis (`writeText` = text/plain) with a reuse answer sitting in
  the repo already — `marked` + the `<AnswerBody>` pipeline. The temptation was to write a markdown→HTML
  converter; the right move was to notice the app already had one and match it exactly, so the copy can't drift
  from the render.

## Evaluation notes (flywheel)

- Failure modes observed: (1) a stray `cat > file` with no heredoc hung on stdin and burned a 2-minute timeout —
  my error, not the tool's; (2) `create-adr.sh` needs `--title` as a flag, not positional; (3) node ESM can't
  import a `D:/...` path — must run from the package root so bare specifiers resolve.
- Graders run and results (PASS/FAIL): ADR significance checklist PASS (clustered, ≥1 alternative w/ rationale —
  five given, pros+cons both directions, references link back); no unresolved `{{placeholders}}` PASS; tsc PASS;
  transform PASS; served-bundle presence PASS; real Word paste NOT RUN.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): paste a copied answer into Word once — it either renders as headings
  or it doesn't, and that single click closes the only open question on this change.
