# Feature Specification: Notice Assist — /help Officer-Defense for Show-Cause Notices & Audit Paras

**Feature Branch**: `022-notice-assist`
**Created**: 2026-07-10
**Status**: Draft
**Input**: Architect description: "A simple `/` command (the slash twin of the `@` document
mention) — e.g. `/help` — that lets an officer facing a show-cause notice, warning, or audit
para get grounded, cited support to respond. The user should NOT have to know the document
type or structure; Noviq reads the attached document with the **local Qwen3-8B** model,
derives what facts it needs from the document itself (no predefined domain fields),
intelligently gathers the missing facts from the officer over a few turns, retrieves the
actual governing rules from the corpus, and assembles a cited reply plus a checklist of
documents to attach. Honest officers acting under political pressure — whose intent was not
illegal — must be safeguarded with accurate, procedurally-correct, rule-grounded responses,
never fabricated ones."

## Context

Executive officers routinely receive **show-cause notices**, **warnings/explanation-calls**,
and **audit paras** for actions taken under political or administrative pressure. Many acted
in good faith with no personal gain, but lack fast, accurate legal support and end up filing
weak, un-cited replies that worsen their position. Noviq already answers legal questions with
verbatim, corpus-grounded citations — exactly the leveller these officers need — but there is
no guided path from "here is the notice against me" to "here is my grounded, procedurally
correct reply."

The naive design (a fixed intake form) fails because the corpus of accountability documents is
open-ended: a one-paragraph warning, a multi-objection audit para, and a formal E&D show-cause
notice have different structures, cite different rules, and demand different rebuttals. We
therefore **do not predefine domain fields**. Instead we predefine only a small, universal
*argument frame* that every accountability document shares, and let the **local model derive the
specific content and the questions-to-ask from the document itself**. The law is always supplied
by corpus retrieval (never model memory); the facts are always supplied by the officer. The
model's freedom is confined to *reading comprehension and question generation*.

This feature builds directly on: conversation document-scoping + the `seeks_legal_authority`
gate (2026-07-10), conversation attachments (F021), conversation memory / pinned facts (F015),
the one-ask slot-filling session pattern (F010), the drafting/export layer (F009), confidential
masking (F020), and the single local model slot served by vLLM (F016, Qwen3-8B).

## Guiding principle (integrity leash)

Noviq helps an officer produce the **strongest reply the cited rules and the officer's real
facts actually support** — and no more. It MUST NOT fabricate law, invent facts, or manufacture
a defense the record doesn't sustain. Where a defense is weak or a document is missing, it says
so plainly and names the document to obtain. This is the existing evidence-leash applied to a new
surface: *stated law is cited verbatim from the corpus; analysis rests on cited provisions; gaps
are flagged, not filled.* The "political pressure" situation is handled the lawful way — through
the **written-orders doctrine** (record the instruction; seek it in writing; once in writing,
accountability shifts to the directing authority), never by concealment.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Start assistance with one command (Priority: P1)

As an officer in a chat, when I type `/` the composer shows a short command menu; picking
`/help` (optionally with the notice attached via `@`/paperclip) puts Noviq into "assist mode"
for this conversation and it begins by understanding my document.

**Why this priority**: it is the entry point; nothing else is reachable without it. It is also
independently demonstrable (command palette + mode entry) without the analysis pipeline.

**Independent Test**: Type `/` → the menu lists `/help` (+ `/reply`, `/audit` aliases). Select
`/help` → Noviq acknowledges assist mode and, if a document is attached, states the document
type it detected; if none is attached, it asks the officer to attach or paste the notice.

**Acceptance Scenarios**:
1. **Given** the composer is empty, **When** the officer types `/`, **Then** a command menu
   appears (keyboard-navigable, same interaction as the `@` document typeahead).
2. **Given** the menu is open, **When** the officer picks `/help`, **Then** the turn is routed
   to the NoticeAssist capability and a per-conversation assist session is created.
3. **Given** `/help` with no attachment, **When** it runs, **Then** Noviq asks for the notice
   (attach or paste) rather than guessing.

---

### User Story 2 — Noviq derives what it needs from the document (Priority: P1)

As an officer, I don't have to tell Noviq what kind of document this is or fill a form. Noviq
reads the attached notice with the local Qwen3-8B model and extracts, from the document itself,
what is alleged, which rule/authority is invoked against me, what I'm asked to explain, the
deadline/forum, and — crucially — the specific questions it must ask me to answer each point.

**Why this priority**: this is the "intelligence" that replaces predefined fields; it is what
makes the intake adapt to any document type/structure.

**Independent Test**: Attach a show-cause notice → the extraction returns ≥1 allegation, the
cited rule, the issuing authority, and a non-empty, per-allegation question list. Attach a
2-objection audit para → the extraction returns 2 objections with distinct questions. No
domain field list is hardcoded; the questions differ per document.

**Acceptance Scenarios**:
1. **Given** an attached accountability document, **When** analysis runs, **Then** the system
   fills the universal frame (allegations[], invoked rule/authority, relief/explanation sought,
   deadline/forum) with content taken from the document.
2. **Given** two structurally different documents, **When** each is analyzed, **Then** the
   generated officer-questions differ and correspond to that document's actual content.
3. **Given** a document Qwen classifies as *not* an accountability document, **Then** Noviq says
   so and offers ordinary chat instead of forcing the flow.
4. **Given** malformed model output, **When** parsing fails, **Then** the turn degrades to a
   single "tell me in your words what this notice alleges" question — never a crash or a
   fabricated frame.

---

### User Story 3 — Intelligent, minimal fact-gathering (Priority: P1)

As an officer, Noviq asks me only the facts it still needs — skipping anything the document
already answers — one or two questions at a time, and remembers my answers across the
conversation.

**Why this priority**: the officer's facts (authority relied on, written orders, approvals, any
benefit) are what turn cited law into a defensible reply; gathering them well is the core value.

**Independent Test**: Over a short exchange, Noviq asks the generated questions, does not
re-ask an item the officer already answered (or the document already supplied), and can be told
"I don't have that" to move on with a recorded gap.

**Acceptance Scenarios**:
1. **Given** an extracted frame with N open questions, **When** the officer answers one, **Then**
   that slot is marked filled and is not asked again.
2. **Given** the officer replies "I don't have that / not applicable", **Then** the slot is
   marked as a **gap** (not filled) and the loop continues.
3. **Given** all questions are filled or gapped (or the officer says "just draft it"), **Then**
   Noviq proceeds to grounding + drafting.
4. **Given** the officer leaves and returns to the same conversation, **Then** the gathered
   facts persist (session state) and the loop resumes where it left off.

---

### User Story 4 — Grounded, cited reply + document checklist (Priority: P1)

As an officer, Noviq retrieves the **actual** governing rule from the corpus (quoted verbatim),
checks what it really requires against my facts, and produces a structured, cited reply plus a
list of documents I should attach — flagging any weak point instead of papering over it.

**Why this priority**: this is the deliverable; without grounded output the flow has no value.

**Independent Test**: The reply quotes the cited rule verbatim with a corpus citation, follows
the fixed structure, and lists concrete attachments; where the officer had a gap, the reply
names the missing document rather than asserting a fact.

**Acceptance Scenarios**:
1. **Given** a cited rule in the notice, **When** drafting runs, **Then** the reply includes that
   rule quoted verbatim from the corpus with a citation (never from model memory); if the corpus
   lacks it, the reply says the rule text could not be located and asks the officer to supply it.
2. **Given** the assembled facts, **Then** the reply follows the structure **Facts → Governing
   rule (quoted) → Application / authority relied on → Absence of mala fide & loss → Relief
   sought**, and ends with a **Documents to attach** checklist.
3. **Given** an unfilled gap, **Then** the corresponding section flags it (e.g. "⚠ No written
   order is on record — obtain and attach the directing authority's written instruction").
4. **Given** the finished reply, **When** the officer exports it, **Then** it exports as
   copy-paste-ready plain text (reuse `export_draft`), with confidential details masked per F020
   before anything left the browser.

---

### User Story 5 — Audit-para variant (Priority: P2)

As an officer/DDO, when the attached document is an audit para, Noviq adapts the frame to the
audit context (objection, amount, sanction/authority relied on, DAC/PAC forum) and the reply to
the audit-response structure (para → objection → factual/regulatory position → status: settled /
regularized / recovery / for-DAC).

**Why this priority**: audit paras follow a distinct process (DAC → PAC); a strong v1 can ship
show-cause first and add this as a parallel template.

**Independent Test**: Attach an audit para → the generated questions and the final reply use the
audit-response structure, not the E&D show-cause structure.

## Universal Argument Frame *(the only predefined structure)*

Document-agnostic; every accountability document maps onto it. Content is filled by the model,
not by a fixed field list.

- **document_kind** — model's best label (show_cause | warning | explanation_call | audit_para |
  other) + confidence; used only to pick the reply template, never to gate extraction.
- **allegations[]** — each: `{ text, invoked_rule?, invoked_authority? }` (1..N).
- **relief_or_explanation_sought** — what the officer must show/produce.
- **forum** — issuing/deciding authority.
- **deadline** — date/'within N days'/none-found.
- **officer_facts[]** — gathered from the officer, each tied to an allegation:
  `{ question, answer?, status: open|filled|gap }`.
- **cited_rule_text[]** — verbatim provisions retrieved from the corpus, with citations.

## Local-model extraction contract *(Layer 2 — Qwen3-8B, local)*

- **Model**: the single local model slot (Qwen3-8B) via the existing inference seam
  (`services/inference` / `query_router` generation), served by vLLM (F016). **Not** the Mistral
  IR worker. Run with `NOVIQ_NO_THINK=1` for extraction (no thinking tokens).
- **Output**: **free-text JSON** (no llguidance — it crashes; proven). Parsing MUST be
  tolerant: extract the first JSON object, ignore prose/markdown fences, default missing fields,
  and fall back to US2 AS-4 on failure. No fabricated frame on parse error.
- **Prompt role**: STRICTLY comprehension — "read this document; extract allegations, invoked
  rules/authorities, what the officer must answer, deadline; propose the questions to ask the
  officer to rebut each allegation." The prompt MUST forbid inventing rules or legal conclusions
  (those come later from the corpus).
- **Input**: the attached document's extracted text (reuse the attachment's indexed
  chunks/IR; cap tokens; if long, analyze the first window + any section whose heading matches
  notice/objection cues).

## Ask-loop state machine

`Analyze → Ask(next open question) → Record → (repeat until no open questions | officer says
"draft") → Ground(retrieve cited rules) → Draft → Review/Export`. State persists per
conversation (reuse the `one_ask_session`-style `state_json` seam; new
`notice_assist_session(conversation_id PK, state_json, updated_at)`), so the loop is resumable
and survives reconnects (tunnel-safe, no reliance on live SSE).

## Reply templates

- **Show-cause / warning**: Facts → Governing rule (quoted, cited) → Application / authority
  relied on → Absence of mala fide & loss (incl. written-orders position, if any) → Relief
  sought → **Documents to attach**.
- **Audit para**: Para & objection → Factual position → Regulatory position (rule quoted, cited)
  → Status sought (settled / regularized / recovery / for-DAC) → **Documents to attach**.

## Functional Requirements

- **FR-001** The composer MUST open a keyboard-navigable command menu on `/` at input start,
  reusing the `@`-mention interaction; commands: `/help` (+ `/reply`, `/audit`).
- **FR-002** `/help` MUST route the turn to a NoticeAssist capability and create/resume a
  per-conversation assist session.
- **FR-003** With no document present, the flow MUST ask the officer to attach/paste the notice
  before analysis.
- **FR-004** Analysis MUST run on the local Qwen3-8B via the inference seam, produce the frame
  from the document's own content, and generate per-allegation officer-questions — with no
  predefined domain fields.
- **FR-005** Extraction output parsing MUST be tolerant and MUST degrade gracefully (US2 AS-4),
  never crashing or fabricating a frame.
- **FR-006** The loop MUST ask only unfilled questions, mark answered slots filled, allow "not
  applicable" → gap, and stop on completion or on officer request.
- **FR-007** Session state (frame + slots) MUST persist per conversation and be resumable.
- **FR-008** Governing rules in the reply MUST be retrieved from the corpus and quoted verbatim
  with citations; corpus misses MUST be flagged, not filled from model memory.
- **FR-009** The reply MUST follow the type-appropriate template and end with a Documents-to-
  attach checklist; gaps MUST be surfaced as explicit ⚠ items.
- **FR-010** Confidential details MUST be masked (F020) before any content leaves the browser;
  export MUST reuse `export_draft` (plain text).
- **FR-011** The flow MUST NOT fabricate law, facts, or defenses; weak positions MUST be stated
  honestly (integrity leash).
- **FR-012** Retrieval for grounding MUST use the corpus (the notice is premise, not sole
  source) — i.e. the `seeks_legal_authority` behavior, not attachment-only scope.

## Key Entities

- **NoticeAssistSession** — `{ conversation_id, frame, officer_facts[], phase, updated_at }`,
  persisted as `notice_assist_session.state_json`.
- **Frame** — the universal argument frame above.
- **SlotQuestion** — `{ allegation_ref, question, answer?, status }`.

## Non-Goals

- No legal advice beyond corpus-grounded, cited support; Noviq is a drafting aid, not counsel.
- No new model download — reuse the existing local Qwen3-8B slot.
- No automatic submission/filing; the officer reviews and sends.
- No case-law reasoning (the corpus has no judgments); the leash forbids inventing precedent.
- No grammar-constrained decoding (llguidance) for extraction.
- Audit-para template may land after show-cause (US5 is P2).

## Risks & Mitigations

- **Fabricated defense / hallucinated rule** → integrity leash + corpus-only grounding + verbatim
  citation gate; analysis prompt forbids legal conclusions.
- **Extraction unreliability (free-text JSON, no llguidance)** → tolerant parsing + graceful
  single-question fallback; tests on real sample notices/paras.
- **Long documents exceed context** → window + heading-cued section selection; cap tokens.
- **Latency (extra local generate on attach)** → single Qwen call, `NOVIQ_NO_THINK=1`; runs once
  per document, cached in session state.
- **Misuse to evade legitimate accountability** → honest-gap flagging; the tool strengthens
  *grounded* replies only and documents the lawful written-orders path.

## Reuse Map

| Need | Reuse |
| --- | --- |
| `/` command palette | `@`-mention pattern in `ConfidentialComposer` |
| Notice as premise + corpus for law | doc-scoping + `seeks_legal_authority` (2026-07-10) |
| Local extraction model | Qwen3-8B slot via `services/inference` (F016), `NOVIQ_NO_THINK=1` |
| Slot-filling session | one-ask `state_json` seam (F010) → `notice_assist_session` |
| Reply generation | drafting/compose + citation gate (F009/F015) |
| Export | `export_draft` (plain text) |
| Confidentiality | F020 masking |
| Attachment ingest/text | F021 conversation attachment chunks/IR |

## Acceptance Checklist

- [ ] `/` opens the command menu; `/help` enters assist mode (US1).
- [ ] Qwen3-8B derives frame + per-document questions from any attached document; no predefined
      domain fields; graceful fallback on bad output (US2).
- [ ] Loop asks only gaps, records answers, supports "N/A"→gap, persists/resumes (US3).
- [ ] Reply quotes cited rules verbatim from the corpus with citations, follows the template,
      lists documents to attach, flags gaps; export is masked plain text (US4).
- [ ] Audit-para variant uses the audit-response structure (US5, P2).
- [ ] No fabricated law/facts/defense; weak points stated honestly (FR-011).

## Open Questions

- Command naming: `/help` vs `/reply` vs `/respond` (aliases acceptable) — architect preference?
- Which corpus rulebooks must be indexed for grounding to be trustworthy (E&D rules, audit/PAC
  rules, Rules of Business, Civil Servants Act) — confirm coverage before GA.
- Should the prospective "seek orders in writing" note (prevention) be a sibling command in this
  feature or a follow-up?
