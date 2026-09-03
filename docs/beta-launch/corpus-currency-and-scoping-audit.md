# Corpus Currency Audit + Attachment Scoping Review

Compiled 2026-07-26. Three questions answered: (A) are attachments correctly
isolated per user, (B) is the corpus current, (C) what must be indexed to fix it.

**Headline: (A) is largely fine with one real gap. (B) is not fine — the
Constitution is thirteen years and seven amendments out of date, and this is
beta-blocking.**

---

# A. Attachment scoping — is a user's attached document leaking into retrieval?

## A.1 Short answer

**The per-user isolation you expected already exists and is enforced in one
place.** Attachments are not visible to other users, and are not visible in the
user's own other conversations. What they *are* visible in is the conversation
they were attached to — which is deliberate, and is what makes "ask a question
about this file" work at all.

So the behaviour you saw is by design, **but I found one genuine defect** (§A.3).

## A.2 The invariant, as implemented

`db/repositories/sources.rs::visible_source_ids()` is the single gate. Every
capability retrieves only within the id set it returns:

```sql
SELECT id FROM sources
 WHERE indexing_status = 'indexed' AND (
       (visibility = 'shared' AND conversation_id IS NULL)          -- 1 global corpus
    OR ($1::text IS NOT NULL AND owner_id = $1
        AND conversation_id IS NULL)                                -- 2 my documents
    OR ($2::text IS NOT NULL AND conversation_id = $2)              -- 3 this conversation
 )
```

| Document type | `owner_id` | `conversation_id` | `visibility` | Who can retrieve it |
|---|---|---|---|---|
| Admin/global corpus | NULL | NULL | shared | everyone |
| My private document | me | NULL | private | only me, in any of my conversations |
| My shared document | me | NULL | shared | everyone |
| **Conversation attachment** | me | conv-id | private | **only me, only in that conversation** |

Note arm 1 requires `conversation_id IS NULL`. So even if an attachment were
flipped to `shared`, it still would **not** enter the global corpus — that is a
good, deliberate belt-and-braces choice.

Promotion is explicit: `POST` promote sets `conversation_id = NULL, visibility =
'private'` and moves the file into the user's uploads directory. That is exactly
the "unless they index it and explicitly make it public" model you described.

The design is covered by tests, including `user_scope_excludes_foreign_private_and_all_attachments`
which asserts *"A's private doc leaked to B"* must not happen.

## A.3 ⚠️ The real defect — conversation id is not checked against the requester

`api/query.rs:110` builds the scope from the **client-supplied** conversation id:

```rust
let scope = RequestScope {
    user_id:         Some(user_id.clone()),
    conversation_id: Some(conversation_id.clone()),   // <- straight from req body
};
```

Arm 3 of the SQL then matches on `conversation_id = $2` **alone — with no owner
predicate**. So a user who supplies another user's conversation id retrieves that
conversation's attachments.

**Why this is a real finding, not theoretical:** every *other* query in
`api/documents.rs` that touches conversation attachments pairs the two columns —
`WHERE conversation_id = $1 AND owner_id = $2` (lines 749, 869). The retrieval
invariant is the one place that does not. That inconsistency is the tell.

**Severity: moderate, not critical.** Conversation ids are unguessable, so this is
not enumerable — it needs a leaked or shared id (a pasted URL, a screenshot, a
support ticket). But it is a privilege check missing from the exact function whose
docstring calls itself *"THE retrieval-privacy invariant"*.

**Fix — one line, no migration:**

```sql
OR ($1::text IS NOT NULL AND $2::text IS NOT NULL
    AND conversation_id = $2 AND owner_id = $1)
```

Add a test asserting user B passing user A's conversation id gets nothing. Worth
doing before any deployment where more than one department shares a server.

## A.4 What I could not verify

I could not query the production database — `psql` requires a password and
credential reads are blocked in my environment. So this is a **code-level review,
not a live data check**. If you are seeing an attachment appear somewhere it
should not, the useful next step is to inspect the actual row:

```sql
SELECT id, file_name, owner_id, conversation_id, visibility, indexing_status
FROM sources WHERE file_name = '<the document>';
```

If it shows `conversation_id IS NULL` and `visibility = 'shared'` on a document a
user merely attached, that is a different bug — in the upload path, not the
retrieval path — and I would want to see it.

---

# B. Corpus currency — the audit

I extracted the stated amendment date from each PDF on the production VM.

| Instrument | Our copy states | Status |
|---|---|---|
| **Constitution of Pakistan** | **"upto the 28th February, 2012"** | 🔴 **13 years stale — 7 amendments missing** |
| Sindh Civil Servants (APT) Rules 1974 | no date; **contains Rule 11-A** | 🔴 **Rule 11-A struck down + deleted 04.12.2024** |
| Removal from Service (Special Powers) Sindh Ordinance 2000 | no date stated | 🟠 repeal status unresolved — see B.3 |
| SPPRA Rules 2010 | "Revised 2013", "Amended vide SGA&CD Notification No…" | 🟠 amendments through 2025 not captured |
| Sindh Public Procurement Act 2009 | "Amended by:" (unresolved) | 🟠 verify |
| Sindh Rules of Business 1986 | "Amended up-to now" (undated — meaningless) | 🟠 verify |
| Sindh Civil Servants (E&D) Rules 1973 | no date stated | 🟠 verify |
| Sindh Local Government Act 2013 | no date stated | 🟠 verify |
| Compendium of Rules under SLGA 2013 | no date stated | 🟠 verify |
| Manual for Development Projects (PC-I) | "Revised 2019" | 🟢 plausibly current |
| Sindh Delegation of Financial Powers 2019 | no date stated | 🟠 verify |
| Manual of Secretariat Instructions 2010 | no date stated | 🟠 verify |

## B.1 🔴 The Constitution is missing seven amendments

Our copy stops at **28 February 2012**. Missing since:

| Amendment | Date | What it did |
|---|---|---|
| 21st | 7 Jan 2015 | Military courts for terrorism (National Action Plan) |
| 22nd | 8 Jun 2016 | ECP appointment reform |
| 23rd | 30 Mar 2017 | Extended military courts |
| 24th | 22 Dec 2017 | Constituencies per 2017 census |
| 25th | 31 May 2018 | **FATA merged into KP; FCR abolished** |
| 26th | 21 Oct 2024 | **Judicial appointments / constitutional bench** |
| 27th | 13 Nov 2025 | **Created the Federal Constitutional Court** |

**Why this is beta-blocking.** In researching the evidence dossier I found a
**Federal Constitutional Court** decision of 10 March 2026 rejecting a Sindh
government petition on deceased quota. That court was created by the 27th
Amendment — **it does not exist anywhere in Noviq's constitution.** If anyone in
the room asks a constitutional question, we answer from a 2012 world.

This also explains a symptom already on record: amendment-related constitutional
questions behaving oddly. The graph was suspected; the document being 13 years old
is the simpler explanation and should be ruled out first.

## B.2 🔴 APT Rules 1974 still carry deleted Rule 11-A

Already reported in the evidence dossier. Our PDF has `11-A` at line 656 with a
footnote recording only its **2011 substitution** — nothing about the Supreme
Court striking it down (*GPO Islamabad v. Muhammad Jalal*, PLD 2024 SC 1276) or
the Sindh Cabinet deleting it on **04.12.2024**.

## B.3 🟠 RSO 2000 — repeal status genuinely unresolved

- The **federal** Removal from Service (Special Powers) Ordinance 2000 was
  **repealed by Act III of 2010**.
- The **Sindh** ordinance is a separate provincial instrument. In 2017 the Sindh
  government announced a decision to abolish it "in the coming session".
- **I could not confirm whether that repeal was ever enacted.** Sources are silent
  after 2017.

This matters more than it looks: the entire "wrong instrument → proceedings
vitiated" scenario turns on whether RSO 2000 is still live law in Sindh. **Do not
build a demo scenario on it until this is settled** — ideally by asking the client
department directly, which is also a good discovery question.

## B.4 "Amended up-to now" is not a date

The Rules of Business 1986 copy claims to be amended "up-to now", with no date.
That phrase is worthless for currency purposes and is a good illustration of the
general problem: **most of our instruments carry no verifiable as-of date at all.**

---

# C. What to obtain and index

## C.1 Must-fix before beta

| # | Document | Where | Why |
|---|---|---|---|
| 1 | **Constitution amended up to 2025** (incl. 21st–27th) | pabalochistan.gov.pk hosts an "amended upto 21 October 2024" copy; a post-27th version is needed | 7 missing amendments; FCC does not exist in our copy |
| 2 | **Sindh APT Rules 1974, post-04.12.2024** | SGA&CD / Sindh Law Dept notification deleting Rule 11-A | We cite a struck-down rule |
| 3 | **PLD 2024 SC 1276** (*GPO Islamabad v. Muhammad Jalal*) | Supreme Court | Struck down Rule 11-A |
| 4 | **SC judgment, 3 Jul 2026** — contract service not countable toward promotion | Supreme Court (2 civil appeals from Sindh Service Tribunal) | Three weeks old; changes live promotion practice |
| 5 | **Sindh RSO 2000 repeal instrument, if any** | Sindh Assembly / Law Dept | Settles B.3 |

## C.2 Should-fix

| # | Document | Why |
|---|---|---|
| 6 | SPPRA notifications & policy letters to date | Board's 44th meeting (12 Feb 2025) and an amendment dispute of 20 Oct 2025 are not in our copy. Source: `pprasindh.gov.pk/ppra_notification.php` and `e.pprasindh.gov.pk/policies` |
| 7 | SPPRA Regulations for Blacklisting of Bidders/Contractors/Suppliers/Consultants **2023** | Entirely absent; blacklisting is a live procurement question |
| 8 | *Zahida Parveen v. Govt of KP*, 2025 SCP 107 | Clarifies PLD 2024 SC 1276 is prospective and preserves accrued rights — the nuance officers get wrong |
| 9 | SHC judgment declaring SPSC defunct / SPSC Act 1989 ultra vires | Affects every appointment question |
| 10 | SHC judgments barring OPS / additional-charge postings | Frequently-asked, frequently-breached |
| 11 | Sindh Civil Servants Act 1973 | The parent Act for APT and E&D rules — **currently absent from the corpus entirely** |
| 12 | Missing corpus positions **07, 08, 10** | Gaps in our own numbering; establish what was intended |

## C.3 Recommended process change — this will recur

The underlying problem is not these twelve documents; it is that **we have no
mechanism to know when our copy of a law goes stale.** Suggested minimum:

1. **Record an `as_of` date per source** at ingest, and surface it in every
   citation: *"Sindh APT Rules 1974, as held by Noviq: amended to 30-07-2011."*
   An officer can then see the currency and judge it. Silent staleness is the
   danger; visible staleness is merely a limitation.
2. **A currency review before each deployment** — check each instrument against
   its official source. Half a day.
3. **Flag judgment-affected provisions.** Where a court has struck down a
   provision we hold, that fact belongs next to the text.

Item 1 is the important one, and it is also a **selling point**: it is precisely
the discipline the departments themselves lack.

---

# D. Recommended order of work

1. **Re-index the Constitution** (post-27th Amendment) — largest gap, easiest fix.
2. **Re-index APT Rules 1974** (post-04.12.2024) + add PLD 2024 SC 1276.
3. **Add the 3 July 2026 promotion judgment** — most current, most compelling.
4. **Resolve the RSO 2000 repeal question** before scripting any disciplinary demo.
5. **Patch the `visible_source_ids` owner check** (§A.3) — one line + one test.
6. **Add Sindh Civil Servants Act 1973** — parent statute, currently missing.
7. Then run the demo-scenario verification, checking answer bodies not HTTP status.

---

# E. Sources

1. Twenty-sixth Amendment · https://en.wikipedia.org/wiki/Twenty-sixth_Amendment_to_the_Constitution_of_Pakistan
2. Twenty-seventh Amendment (Federal Constitutional Court) · https://en.wikipedia.org/wiki/Twenty-seventh_Amendment_to_the_Constitution_of_Pakistan
3. Constitution amended upto 21 October 2024 (PDF) · https://pabalochistan.gov.pk/storage/7796/67f8b1f9c93af_Constitution-amended-upto-the-21st-October,-2024.pdf
4. FAFEN — history of constitutional amendments · https://fafen.org/history-of-constitutional-amendments-in-pakistan/
5. Federal RSO 2000 repealed by Act III of 2010 · https://pakistancode.gov.pk/english/UY2FqaJw1-apaUY2Fqa-apaUY2NpaZdp-sg-jjjjjjjjjjjjj
6. The News — Sindh govt decides to abolish RSO 2000 (2017) · https://www.thenews.com.pk/print/215380-Sindh-govt-decides-to-abolish-Removal-from-Service-Ordinance-2000
7. SPPRA notifications · https://www.pprasindh.gov.pk/ppra_notification.php
8. SPPRA policy letters · http://e.pprasindh.gov.pk/policies
9. Transparency International Pakistan — SPPRA 2022 correspondence · https://transparency.org.pk/sppra-2022/
10. The Nation — contract service cannot be counted towards promotion (3 Jul 2026) · https://www.nation.com.pk/03-Jul-2026/contract-service-cannot-counted-towards-promotion-rules-sc
11. Zahida Parveen v. Govt of KP, 2025 SCP 107 · https://sahsol.lums.edu.pk/node/24186
