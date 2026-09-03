# One-Ask Drafting — How a Draft Is Built, End to End

> **Purpose of this document**: a careful, accurate walk-through of exactly how the
> app turns *one sentence* from an officer into the 5-section summary you have been
> reviewing — every stage, the **actual prompt** sent to the model, every tuning
> constant, and (at the end) a precise map of **where the duplicates and the
> off-topic "extras" come from** in the code, with the levers available to fix them.
>
> Written 2026-06-16 from the live source. Code is cited as `file:line` so you can
> open any claim. Nothing here is from memory or assumption.
>
> Source files (all under `src-tauri/src/services/drafting/one_ask/`):
> `mod.rs` (orchestration) · `intent.rs` (parse the sentence) · `skeleton.rs` (the
> 5 sections) · `research.rs` (retrieval) · `compose.rs` (the prompt) ·
> `verify.rs` (assembly + the citation gate) · `state.rs` (data model).

---

## 0. The one rule everything is built around — the "two-lane" invariant

The model is **never trusted to produce facts**. It only decides *where* a provision
or a figure belongs, by emitting a marker — `[CITE:n]` for a provision, `[FIG:n]` for
an officer-stated amount. Deterministic Rust code owns the *content*: it splices the
verbatim quote, checks it character-for-character against the source, and inserts the
officer's exact figure. (`mod.rs:7-9`)

Consequence: a number the model *writes in prose itself* can only ever appear as a
**flagged** run — never as a citation, never silently. The guarantee lives in
`verify.rs`, not in the prompt (`verify.rs:1-6`).

---

## 1. End-to-end flow

```
officer's one sentence
      │
      ▼
[1] INTENT PARSE         intent.rs      regex only, no model
      │                                 → raw sentence + verbatim figures + action verb
      ▼
[2] SKELETON             skeleton.rs    fixed 5 sections (compile-time constant)
      │
      ▼
[3] RESEARCH             research.rs    FOR EACH of the 5 sections:
      │                                   • build a query (sentence + section role-hints)
      │                                   • vector (KNN) leg  +  lexical leg
      │                                   • fuse them (RRF), drop far hits, cap per section
      │                                 → a flat list of "provision cards" (the model's
      │                                   ENTIRE citation vocabulary)
      ▼
[4] COMPOSE              compose.rs     FOR EACH section: build one [INST] prompt with the
      │                                 subject + that section's cards + figures, send to the
      │                                 7B writer, parse [CITE]/[FIG] markers out of its prose
      ▼
[5] VERIFY / ASSEMBLE    verify.rs      splice verbatim quotes, run the gate, flag anything
      │                                 unbacked, number the paragraphs
      ▼
the 5 numbered paragraphs you read
```

Orchestrated in `mod.rs:48-131` (`run_pipeline`). State is saved after every stage so a
crash resumes as a re-runnable "Failed (interrupted)" (`mod.rs:145-156`).

If **no** card is found for the whole ask, the draft is refused outright with a
remediation message — it is never half-grounded (`mod.rs:79-90`).

---

## 2. Stage 1 — Intent parse (`intent.rs`)

Tier-1 discipline: **regex only, no model** (`intent.rs:1-4`).

- **Figures** are captured *verbatim* by `figure_regex()` — "4 billion", "Rs. 2,500
  million", "3.5 crore", "Rs 500" (`intent.rs:11-16`). They are lossless substrings of
  the sentence and re-enter the draft **only** through a `[FIG:n]` splice.
- **Action verb** is matched from a small keyword list (`get`, `approve`, `sanction`,
  `procure`, `launch`, …) for light classification (`intent.rs:18-22`).
- The **raw sentence** itself is kept — it is the retrieval seed for every section.

Output: `OneAskIntent { raw, figures, action }`.

---

## 3. Stage 2 — The skeleton (`skeleton.rs`)

The 5-section structure is **product domain knowledge, a compile-time constant** — not
a per-case uploaded template (that was the 009 failure mode) (`skeleton.rs:1-4`).

Each section carries three things that matter downstream: **`title`**, **`guidance`**
(the narrative instruction handed to the writer), and **`role_hints`** (extra query
words mixed into retrieval). Verbatim from `skeleton.rs:33-83`:

| # | Section | `guidance` (writer instruction) | `role_hints` (retrieval) | figures? |
|---|---------|-------------------------------|--------------------------|----------|
| 1 | **Background and Purpose** | "Introduce the matter and its purpose in formal secretariat narrative: what is sought and why it is before the competent authority. Do not discuss legal provisions or finances here." | `background purpose subject matter proposal scheme` | no |
| 2 | **Legal and Procedural Position** | "State the governing legal and procedural position using ONLY the provisions supplied as citation cards: which rules govern the proposal, the prescribed procedure, and how the matter is routed for approval." | `rules of business procedure routing competent authority approval` · `development project scheme approval process PC-I requirements` | no |
| 3 | **Financial Aspect** | "State the financial dimension: the funds sought and the sanctioning powers that apply, using ONLY supplied citation cards for any powers or limits." | `delegation of financial powers sanction funds expenditure budget allocation` | **yes** |
| 4 | **Proposal and Recommendation** | "State the concrete proposal and the recommendation placed before the competent authority, as connective narrative." | `proposal recommendation approval competent authority` | yes |
| 5 | **Prayer for Approval** | "Close with the formal prayer: the approval solicited from the competent authority, in one short paragraph." | `approval solicited orders requested` | no |

The **order is the output order**. Each section becomes exactly one numbered paragraph
in v1 (`verify.rs:233-236`).

---

## 4. Stage 3 — Research / retrieval (`research.rs`)

This is where the *citation vocabulary* is assembled. The writer can only ever cite
cards produced here — nothing else exists to it.

### 4.1 Per-section query
For each section: `query = officer's raw sentence + the section's role_hints`
(`research.rs:231-233`). So the Background query and the Financial query differ only by
their role-hint tail; the officer's sentence dominates all five.

### 4.2 Two retrieval legs, then fusion
For each section query (`research.rs:258-291`):

1. **Vector leg** — embed the query, KNN over the corpus, over-fetching
   `CARDS_PER_SECTION*2` with an `OVERFETCH` factor of 4 so there's room to re-rank
   (`research.rs:17-19, 263-264`).
2. **Distance margin** — keep only hits within `DISTANCE_MARGIN = 0.15` cosine of the
   section's *best* hit; the far tail (off-topic neighbours) is dropped
   (`research.rs:20-25, 212-220`). It's **relative**, not absolute, so no per-model
   calibration.
3. **Lexical leg** — a keyword search, but weighted (see 4.3).
4. **RRF fusion** — Reciprocal-Rank Fusion merges the two lists: a chunk that appears in
   **both** legs outranks a single-leg chunk of similar rank (`research.rs:185-210`).
   This is how vector-only "money-vocabulary" neighbours get pushed down when the lexical
   leg has a real subject match. Final budget per section: `CARDS_PER_SECTION = 6`.

### 4.3 The lexical leg is deliberately narrow (Fix #1 + Fix #3)
- **Only the officer's subject nouns** go on the lexical leg — money words and filler
  (`million`, `rupees`, `the`, `for`…) are stop-listed (`research.rs:87-110`), and
  **section role-hints are excluded** from it (they're generic governance vocabulary that
  matches boilerplate everywhere; they stay on the vector leg) (`research.rs:92-95`).
- **Fix #3 (corpus-rarity weighting)**: plain keyword rank has no IDF, so a chunk
  matching many *common* words ("secretary", "department") beats the one matching the
  *rare* words that actually identify the provision ("contracting", "distributor"). So
  the query keeps only **discriminative** terms — drop words matching nothing (`df==0`)
  and ubiquitous words (`df > 50%` of the corpus), keep the rarest 8
  (`research.rs:121-183`). One corpus document-frequency lookup per draft.

### 4.4 Labels are read, never generated
A card's label (e.g. "Rule 27(5)(i)") is resolved deterministically from the chunk's
stored `unit_label` / `unit_path`, falling back to parsing the content prefix
(`research.rs:40-80`). **The model never invents a rule number** — and this is also
where the live "Rule 2017"-style mislabels come from: a mis-captured year sitting in the
content prefix (the index-time bug documented separately).

### 4.5 How a chunk becomes a "card", and the de-dup that already exists
Walking the fused hits (`research.rs:307-353`): a new chunk becomes a `ProvisionCard`
(chunk id, source title, excerpt, resolved label, page, and the set of section ids it
serves). A chunk already seen in an earlier section is **not duplicated** — instead the
current section's id is *appended* to that one card's `section_ids` … **but only up to a
cap**:

> ```rust
> const MAX_SECTIONS_PER_CARD: usize = 2;   // research.rs:26-29
> ```
> *"A card may serve at most this many sections. Without the cap, one generic chunk that
> ranks near several section queries gets cited in nearly every paragraph (observed: same
> Rule 6 quote in 4 of 5 sections)."*

### 4.6 Anti-starvation exception
Card consumption is first-come-first-served in skeleton order (Background grabs first).
A later section whose every hit is already cap-consumed would draft **empty**, so it
**force-adopts its top-ranked hit** — one bounded exception to the cap
(`research.rs:354-376`).

**Output** of Stage 3: a flat `Vec<ProvisionCard>`, each tagged with the section(s) it
serves. `cards_for_section()` later filters this list per section, handing the writer
**global 1-based ids** so markers resolve against the whole vec (`research.rs:385-398`).

---

## 5. Stage 4 — Compose: the actual prompt (`compose.rs`)

### 5.1 The subject (US6 / FR-014)
Before composing, the officer's sentence is turned into a **subject** string injected
into *every* section, with monetary spans neutralised ("Rs 2 million" → "the stated
amount") so a raw number can never leak into prose (`compose.rs:12-19`). Computed once
(`mod.rs:94-96`).

### 5.2 The prompt template — verbatim
This is exactly what is built for each section and sent to the 7B writer
(`compose.rs:33-95`). Bracketed parts are filled per section:

```
[INST] You are drafting one section of a formal Government of Sindh summary.
Subject of this summary (the officer's request — every paragraph MUST be about THIS
 specific matter; the provisions below are its legal basis, NOT the topic): {SUBJECT}
Section: {SECTION TITLE}
Instruction: {SECTION GUIDANCE}

Available provisions (reference ONLY as [CITE:n] markers):
[CITE:{id}] {label} — "{first 300 chars of the chunk excerpt}"
[CITE:{id}] {label} — "{…}"
        ⋮   (this section's cards only; ids are GLOBAL across the draft)

Officer-stated figures (reference ONLY as [FIG:n] markers):      ← only if section wants figures
[FIG:1] {figure text}

Rules (mandatory):
 - Write 1-2 formal paragraphs that are ABOUT the subject above; explain how the
   provisions apply to THIS matter. Do NOT describe the provisions in the abstract.
 - Write 1-2 formal paragraphs of connective narrative only.
 - NEVER write any rule, section, article, regulation or clause number yourself:
   place a [CITE:n] marker where a provision belongs.
 - NEVER write any monetary figure or amount yourself: place a [FIG:n] marker
   where an officer-stated figure belongs.
 - NEVER quote source text yourself; the [CITE:n] marker inserts the quote.
 - Do not number the paragraphs; numbering is added later.
 - Do not add letterhead, addressee, signature, or subject lines.
[/INST]
```

Edge cases baked into the prompt:
- **No cards for this section** → the writer is told to write only connective narrative
  or say the position is to be confirmed, and *not* invent anything (`compose.rs:49-54`).
- **Financial section with no officer figure** → told to state that financial details
  are to be supplied and write *no* figures/estimates (`compose.rs:65-70`).

### 5.3 Parsing the writer's output
`parse_markers()` splits the raw text into ordered runs of `Prose` / `Cite(n)` / `Fig(n)`
(`compose.rs:135-151`). Malformed markers the model sometimes emits ("[FIG:x]",
"[CITE:]") are scrubbed so they never ship as literal prose (`compose.rs:111-130`).

The writer is the single shared `ModelSlot` (the 7B). In tests it's a canned-string stub
(`compose.rs:21-25`).

---

## 6. Stage 5 — Verify / assemble (`verify.rs`)

This is the gate that makes the citations trustworthy. For each section's runs
(`verify.rs:237-358`):

- **Prose run** → scanned by `foreign_number_in_prose()` (`verify.rs:190-231`). Any
  provision number (`Rule 7(2)`, `Article 27`…) or money figure the writer typed *itself*
  is allowed **only** if it is officer-stated **or** verbatim-backed by a card **assigned
  to this section**; otherwise the whole prose run becomes a **Flag** ("unverified
  reference … removed") (`verify.rs:272-281`). Money compares by canonical key so
  "Rs. 10 million" ≡ "Rs.10.00 million" (`verify.rs:150-181`).
- **`[CITE:n]` run** → resolve the global id to a card. Three guards:
  1. If the card belongs to a **different section** → Flag (a hallucinated global id can
     resolve to a real card elsewhere) (`verify.rs:286-295`).
  2. **Quote-stack / repeat-cite collapse**: a citation right after another citation, or
     a repeat of a card already quoted *in this paragraph*, is dropped as
     non-narrative (`verify.rs:297-305, 253-254`).
  3. Otherwise splice `citation_snippet()` — only the **leading operative span** of the
     excerpt (first sentence boundary past 6 words, hard cap 30 words, exact char-slice so
     the verbatim invariant holds), strip the duplicated leading label, and **verify it is
     a verbatim substring** of the card. Pass → `Citation` run; fail → Flag
     (`verify.rs:61-143, 306-321`).
- **`[FIG:n]` run** → insert the officer's exact figure, or Flag if the id doesn't exist
  (`verify.rs:329-338`).
- **Empty section** (writer returned nothing) → a Flag, never a blank paragraph
  (`verify.rs:256-268`).

Finally, **flags are moved to the end of the paragraph** (stable order) so they interrupt
the prayer/prose at the paragraph boundary rather than mid-sentence (`verify.rs:342-348`).

Run types in the data model: `Prose` · `Citation{label, quoted, verified}` · `Figure` ·
`Flag{reason, original}` (`state.rs`). A Flag is what you see as
`[FLAG: unverified reference "…" removed]`.

---

## 7. WHERE THE DUPLICATES COME FROM  ← read this for the feedback

Two distinct kinds, two distinct causes:

### (a) The *same provision quoted in two sections*
e.g. the single-source quote in §1 **and** §2; Form-of-Contract in §1 **and** §2;
Procurement-Plan in §2 **and** §4.

**Cause:** `MAX_SECTIONS_PER_CARD = 2` (`research.rs:26-29`) explicitly *allows* a card to
serve two sections, and the only dedup in assembly is **per-paragraph**, not
cross-section — `cited` is reset for every section (`verify.rs:253-254`). So a card quoted
in §1 and again in §2 is two different paragraphs → it is **not** deduped. The cap of 2 is
exactly why each repeated provision shows up in exactly two places.

There is also **no notion of "quote this provision once, reference it elsewhere"** — every
section that holds the card re-splices the full verbatim snippet.

### (b) The *same theme/sentence restated in every section*
e.g. "imminent e-daftar within a month / ten departments / sole distributor" reappears in
all five paragraphs.

**Cause:** US6/FR-014 injects the officer subject into **every** section prompt
(`compose.rs:41-45`, `mod.rs:94-96`) and tells each section "every paragraph MUST be about
THIS matter." That fixed the *old* problem (generic rule-digest) but means the 7B
re-introduces the matter from scratch in each section instead of assuming it's already
established.

---

## 8. WHERE THE "EXTRAS" (off-topic provisions) COME FROM

e.g. taxation-gazette "Rule 7(2)", secretariat "Section 6 — Return of Receipts", Ombudsman
advance-payment, SLGA contract-register Rule 5(4)(i) — none of them about procuring
scanners.

**Causes, in order of impact:**

1. **Fragmented corpus (the bigger, already-diagnosed root).** On the current index the
   real procurement provisions are shredded one-token-per-line, so there is often no
   coherent on-topic chunk to outrank the off-topic but *whole* paragraphs. Retrieval
   correctly returns the only complete passages that exist. *(Fixed by the
   extraction/re-index change pending; off-topic precision can only be judged fairly after
   that.)*
2. **The distance margin is relative, not absolute** (`research.rs:20-25, 212-220`). If a
   section's best hit is itself weak, the 0.15 band around it still admits weakly-relevant
   neighbours — there is **no floor** below which a card is rejected.
3. **The anti-starvation exception force-includes a card** to avoid an empty section
   (`research.rs:354-376`). That is frequently where a marginal/off-topic card enters.
4. **The writer then *glosses* a card it can't connect** — observed live: it claimed the
   taxation-gazette Rule 7(2) means "records of contracts maintained" (false). The verbatim
   gate passes it because the *quote* is real; only the *gloss* is wrong, and the gate
   doesn't judge glosses. Better to not feed the card than to ask the 7B to self-censor.

---

## 9. The levers available (for the "what do we do" decision)

Mapped to the causes above. Nothing here is implemented yet — this is the menu.

**For duplicates — structural, independent of the re-index (can do now):**
- **L1. Quote-once ledger.** Track which card has been quoted verbatim across the whole
  draft in `assemble_and_verify`; a later section that needs it emits a short **label
  reference** ("under Rule 4, cited above") instead of re-splicing the quote — or drops it.
  *(Behaviour change to the citation contract — needs sign-off.)* Touches `verify.rs`.
- **L2. Best-section assignment.** Replace first-come-first-served (`research.rs:307-376`)
  with assigning each card to its **argmax** section (highest-scoring), so the core
  provision lands in the right section, then quote it once. Pairs with lowering
  `MAX_SECTIONS_PER_CARD` to 1. Touches `research.rs`.
- **L3. Establish-subject-once prompt.** Tell §1 to set the matter and §2–§5 to assume it
  (focus on their angle, don't re-introduce it) — fixes the thematic repetition and trims
  filler. Touches `compose.rs` (`build_section_prompt`).

**For extras — precision, best judged AFTER the re-index:**
- **L4. Absolute relevance floor** in addition to the relative margin (`research.rs`).
- **L5. Stop force-padding** — let a section stay thin (a Flag) rather than adopt an
  off-topic card (`research.rs:354-376`).
- **L6. Auto doc-set narrowing** — pick the top-N most-relevant *source documents* for the
  matter and retrieve only within them, killing cross-statute noise generically (no manual
  curation). Touches `research.rs`.

**Recommended sequencing:** do L1–L3 now (structural, high-confidence, directly answers
"remove duplicates"); hold L4–L6 until the extraction re-index lands, so precision
thresholds are tuned on clean text rather than the current fragmented scores.

---

## 10. Tuning constants in one place (for reference)

| Constant | Value | Where | Meaning |
|----------|-------|-------|---------|
| `CARDS_PER_SECTION` | 6 | `research.rs:17` | max cards offered to a section |
| `OVERFETCH` | 4 | `research.rs:19` | KNN over-fetch factor |
| `DISTANCE_MARGIN` | 0.15 | `research.rs:25` | relative cosine band kept around best hit |
| `MAX_SECTIONS_PER_CARD` | **2** | `research.rs:29` | **how many sections one card may serve ← duplicate source** |
| `LEXICAL_MAX_TERMS` | 8 | `research.rs:119` | rarest discriminative terms kept |
| `RRF_K` | 60.0 | `research.rs:189` | RRF damping constant |
| `SNIPPET_MIN_WORDS` / `MAX_WORDS` | 6 / 30 | `verify.rs:71-72` | quoted-span length window |

---

*End. Read, mark up, and we decide which levers (L1–L6) to pull and in what order.*
