# Feature 020 — Confidential-detail masking in chat

**Status:** draft / plan (2026-07-03)
**Owner:** architect

## 1. In plain words (what we're building)

When an officer types a question that contains **private information** — a person's
name, CNIC (ID card number), phone number, salary/amount, or file/case number — the
app **highlights those bits as they type**. The officer can:
- **turn off** a highlight the app got wrong ("this isn't private"),
- **mark** any other word themselves ("hide this too").

When they hit send, the app **swaps the private bits for simple labels**
(`[NAME]`, `[CNIC]`, `[AMOUNT]`) *before* the question is processed or saved. The
system works on the labelled version; when the answer comes back, the app **puts the
real values back on the officer's screen only**. So the officer reads a normal, full
answer, but the private details are **never written into the database or logs**.

Worked example:

| Stage | Text |
|---|---|
| Officer types | `Can Ahmed Khan, CNIC 42101-1234567-1, get a pension of Rs 50,000?` |
| Stored / processed | `Can [NAME_1], CNIC [CNIC_1], get a pension of [AMOUNT_1]?` |
| Shown back to officer | `Can Ahmed Khan, CNIC 42101-1234567-1, get a pension of Rs 50,000?` (re-filled on screen) |

## 2. Why (the goal)

The AI runs on **our own server**, so this is **not** about hiding data from an outside
company. The point is **data minimisation**: today every query is saved *verbatim* in
`query_log`, query traces, and `chat_message` — so any CNIC/name/salary an officer
types lives in our database forever and is visible to admins reviewing logs. Masking
keeps that private data **out of storage** while still letting the officer use the tool
normally.

## 3. Goals / non-goals

**Goals**
- Highlight private details live as the officer types.
- Officer has final control: unmark false hits, mark missed ones.
- Detect both **structured PII** (CNIC, phone, money, file no., email, dates) **and
  names** (people, and optionally offices/places).
- Real private values are **never persisted** — DB/logs store only the labelled text.
- The officer always sees the real values in their own answer.

**Non-goals**
- Not encryption of the whole conversation (separate concern).
- Not masking legal structure ("Rule 16", "Article 25") — those must pass through
  untouched or retrieval breaks.
- No change to how retrieval/answers work beyond receiving labelled text.

## 4. How detection works (two kinds, because names are harder)

**a) Structured PII — instant, on-device, every keystroke.**
Simple text patterns, fast enough to run live in the browser:
- CNIC `\d{5}-\d{7}-\d`
- Phone `03\d{2}-?\d{7}` (and landline forms)
- Money `Rs\.?\s?[\d,]+` / "rupees"
- File/case no. e.g. `No. F.1(2)/2023`
- Email, and long date strings.

**b) Names (people / offices / places) — needs a model, so NOT every keystroke.**
We have no fast name-detector today (the existing `entity_extractor.rs` Pass 2 uses the
big LLM — seconds per call, too slow to run live).

**DECISION (2026-07-03): B1 — dedicated small NER model.** Add a small, fast
name-detection model behind a new `POST /api/detect_pii`; call it **when the officer
pauses typing (~500 ms)** or when they open a "review before send" panel.
- **Recommended model:** **GLiNER** (small/multilingual, ONNX) — promptable entity
  types, so we ask specifically for `person`, `designation`, `organisation`, `location`;
  runs in <200 ms on CPU, ~50–200 MB. Reuses the ONNX runtime already vendored for the
  embedder. (Fallback if GLiNER integration is fiddly: a distil-BERT token-classifier
  NER, ONNX.)
- Rejected alternative (B2, reuse the big LLM per query) — too slow and adds load to the
  shared generation model; only a stop-gap, not shipped.

**Regex runs live in the browser; names run on the server on-pause** via `/api/detect_pii`,
and that endpoint **must not log the text it receives** (in-memory processing only).

**c) Manual override — always available.** Tap a highlight to remove it; select any text
→ "Mark as confidential" to add it. The officer's choices win over the detectors.

## 5. What masking does on send (reversible labels)

- Each confidential span → a typed label with a counter: `[NAME_1]`, `[CNIC_1]`,
  `[AMOUNT_1]` (same value re-used → same label, so the model can reason about "the
  same person").
- The **label → real-value map lives only in the browser session** (never sent to the
  server, never stored). It persists for the whole conversation so follow-up turns and
  the answer can be un-labelled correctly.
- On submit: replace spans → send labelled text through `/api/query` (embedding, FTS,
  LLM prompt, `query_log`, traces all see labels only).
- On answer: the browser swaps labels back to real values **for display only**.

## 6. Where it plugs into the current system

```
ChatPage composer
  ├─ live: regex detect (browser)  ─┐
  ├─ on-pause: POST /api/detect_pii ─┼─▶ highlighted spans + user toggles
  └─ manual select-to-mark ─────────┘
        │  on submit
        ▼
  mask spans → labelled text        (label→value map kept in browser session)
        │
        ▼
  transport.call("query", { text: labelledText })   ← server sees labels only
        │
        ▼
  answer (may contain [NAME_1]) → browser re-fills real values → display
```

**Frontend (most of the work):**
- `ChatPage.tsx` composer → a highlight **overlay** (a mirrored layer behind the text
  box, because a plain text box can't colour individual words) + chips + select-to-mark.
- `maskEngine.ts` — regex detectors, the label↔value map (session-scoped), mask +
  re-fill helpers.

**Backend (small):**
- `POST /api/detect_pii { text } → { spans:[{start,end,type,label}] }` — names only;
  **no logging**. (B1: small NER model; B2: LLM entity prompt.)
- No change to `/api/query`, retrieval, or storage — they simply receive labelled text.

## 7. Phased delivery

- **Phase 1 — structured PII, fully client-side.** Regex detect + overlay highlight +
  tap-to-unmark + select-to-mark + reversible labels + re-fill on the answer. **No
  backend change.** Ships the whole UX for CNIC/phone/money/file-no.
- **Phase 2 — names.** Add `POST /api/detect_pii` (recommended: small NER model),
  call on-pause, merge its spans into the same highlight/chip UI. Everything else
  (labels, map, re-fill) already built in Phase 1.
- **Phase 3 — polish.** Admin policy for mandatory categories; a "review before send"
  confirmation panel; per-category on/off toggles; optional mask-in-history redaction
  for already-stored messages.

## 8. Task outline

**Phase 1 (frontend only)**
1. `maskEngine.ts`: regex detectors (CNIC, phone, money, file-no, email, date); span model.
2. `maskEngine.ts`: label↔value map (session), `mask(text)→{labelled, map}`, `refill(text, map)`.
3. Composer overlay in `ChatPage.tsx`: render highlight spans under the textarea.
4. Chips + interactions: tap-to-unmark, select-to-mark, "N items will be masked" tray.
5. Wire submit → send labelled text; wire answer render → `refill`.
6. Persist the map for the conversation (session storage) so follow-ups re-fill.

**Phase 2 (names — B1, GLiNER)**
7. Add the GLiNER (small/ONNX) model asset + loader (reuse the embedder's ONNX runtime).
8. `POST /api/detect_pii` handler (no logging) + route; entity types person/designation/org/location.
9. `maskEngine`: debounced on-pause call, merge server spans with regex spans.
10. Loading/quiet-fail UX (names are best-effort; regex + manual still work if it's slow).

## 9. Risks / cautions

- **Text box can't style words** → the overlay/mirror is the one fiddly frontend piece.
- **False hits/misses** are unavoidable → manual override is mandatory, not optional.
- **Don't mask legal terms** → scope detectors to PII; explicitly exclude Rule/Article/
  Section tokens.
- **Follow-ups** → the label map must live for the whole conversation or re-fill breaks.
- **Detect endpoint privacy** → it may see raw text transiently; it must never log or
  store it. (Regex-only Phase 1 avoids this entirely.)
- **Names model cost** → keep it small/fast; never block sending on it (regex + manual
  are the guaranteed path).
```
