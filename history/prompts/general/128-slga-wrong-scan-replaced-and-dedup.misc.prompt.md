---
id: 128
title: SLGA wrong-scan replaced and corpus dedup
stage: misc
date: 2026-07-29
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform) "let us move step by step first the smallest one to fix then to large ones. and for sindh local govt act searchh the net for good copy if available." / "uploaded, check it"
labels: [corpus, ingestion, ocr, postgres, ops, data-quality]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: scripts/corpus_gap_check.sql
    change: executed (read-only)
  - path: (scratchpad) dedup_sources.sql
    change: created — graph-aware source delete
  - path: (scratchpad) drop_bad_slga.sql
    change: created — graph-aware source delete
tests:
  - name: corpus_gap_check.sql after both steps
    result: PASS (SLGA 161/161, missing 1..161 = NONE; SLGA off the gap report)
---

## Prompt

let us move step by step first the smallest one to fix then to large ones. and
for sindh local govt act searchh the net for good copy if available.

(then) uploaded, check it

## Response snapshot

Worked smallest-first, as asked.

### Step 1 — duplicate and dead sources (smallest)

Verified `b8ad8064…` and `aff75faf…` share `file_hash cc0602ae…` and byte size
14,854,914 — a true byte-level duplicate of the Compendium. The old one was
`visibility=private` under `/uploads/shafqat/` with 1 notebook link, so the
retrieval impact was narrower than first reported; the architect approved
deleting it anyway. `testdoc` (0 chunks, status `pending`) removed too.

**Schema defect found while doing it.** `DELETE FROM sources` failed:

```
ERROR: update or delete on table "chunks" violates foreign key constraint
       "graph_nodes_chunk_id_fkey" on table "graph_nodes"
```

`graph_nodes.chunk_id`, `graph_edges.from_node/to_node` and
`procedure_steps.source_node_id` are `ON DELETE NO ACTION` while every other FK
onto `sources` is `CASCADE`. The transaction rolled back cleanly (13940 chunks
unchanged); the delete then succeeded in dependency order
(`procedure_steps` → `graph_edges` → `graph_nodes` → `sources`).
**The Admin UI delete-document path very likely hits the same error for any
indexed document with graph nodes — needs verifying and fixing before beta.**

Result: −2480 chunks, −1823 graph nodes, −1880 edges; 19 → 17 sources;
0 orphaned edges or embeddings; surviving shared Compendium intact.

### Step 2 — the Sindh Local Government Act

Web search plus verification on the VM identified the real problem. The indexed
`Sindh Local Government Act 2013.pdf` is **byte-identical (md5 `403e1a51…`) to
lgdsindh.gov.pk's own `Act-2013.pdf`** — 8 pages, **no text layer whatsoever**
(`pdftotext` returns nothing), whose content is the *Sindh Local Government
(Amendment) Act, 2013 — Sindh Act No. LIII of 2013*, ss.1–31. The 21 sections in
the DB were OCR output, hence `IntheSindhLocalGovernmentAct,2013`. The false
`Section 75` label was the amendment quoting substituted s.75 "Commercial
Schemes" — proven by finding the identical sentence in the real Act at p.52.

Copies evaluated: Sindh High Court (150 pp, text-native, ss.1–161, consolidated
2 Mar 2015) — accepted; sindhlaws.gov.pk Sindh Code PUB-NEW-18-000163 (Amdt Act
LIII of 2013) and PUB-NEW-23-000062 (Amdt Act XXXII of 2021) — accepted;
lgdsindh.gov.pk — rejected (it is the bad scan); Scribd/Studocu — not
authoritative, not pursued. No post-2021 consolidation exists.

The bad source was dropped (−128 chunks, −45 nodes, −50 edges; the scan archived
to `/opt/noviq/attic-patches/`), the three PDFs staged to the architect's Desktop
and to `/opt/noviq/data/staging-corpus-update/`, and the architect uploaded and
indexed them.

Verified after upload:

| Document | Sections | Chunks |
|---|---|---|
| Sindh Local Government Act 2013 | **161/161**, `missing 1..161: NONE` | 1107 |
| Sindh LG (Amendment) Act 2013 | 31/31 | 134 |
| Sindh LG (Amendment) Act 2021 | 18/18 | 91 |

Zero glued-word chunks. SLGA no longer appears on the gap report at all.

## Outcome

- ✅ Impact: worst-coverage document in the corpus went from 21 garbled OCR
  sections of the wrong statute to 161/161 clean sections of the right one, plus
  both amendment acts. Duplicate and dead sources removed. A latent
  delete-cascade bug surfaced.
- 🧪 Tests: `corpus_gap_check.sql` — SLGA off the report; `missing 1..161: NONE`.
  No code changed, so no suite re-run.
- 📁 Files: none modified in-repo; two throwaway SQL scripts in scratchpad.
- 🔁 Next prompts: step 3 — sequence-sanity check for footnote false-labels
  (`Article 343`, `Section 1752`, `Section 156`); then the genuine small gaps
  (SPPRA Rules 56/59/60/69/86, Secretariat 90/131/153, APT 12, E&D 10, RoB 13);
  then fix the `graph_nodes` delete cascade.
- 🧠 Reflection: two corpus problems were masquerading as parser bugs. Checking
  what the document IS — page count, text layer, md5 against the upstream
  source — costs a minute and settled weeks of speculation.

## Evaluation notes (flywheel)

- Failure modes observed: a scanned PDF with no text layer was silently OCR'd
  and indexed as if it were a normal document, with no quality signal surfaced;
  the gap audit counts footnote-derived false labels as real provisions.
- Graders run and results (PASS/FAIL): PASS — SLGA 161/161.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): flag any source whose extracted
  character count per page is near zero as `scanned` at ingest time, so a
  text-layer-less PDF is visible in the Admin console instead of quietly
  producing garbled chunks.
