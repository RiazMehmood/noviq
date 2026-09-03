# Contract — Qwen3-8B extraction JSON + tolerant parse + reply templates

## Analysis prompt (config fragment, Principle VI)

Role: STRICT reading-comprehension of the officer's own document. The prompt MUST:
- instruct the model to read the notice/para and return **only** a single JSON object;
- extract allegations, the rule/authority invoked against the officer, what the officer must
  explain/produce, the forum, and the deadline — all from the document text;
- **generate the questions** the officer must answer to rebut each allegation;
- FORBID inventing rules, legal conclusions, or citations (those come from the corpus later);
- run with `NOVIQ_NO_THINK=1` (no thinking tokens).

## Expected JSON shape (free-text, NOT llguidance)

```json
{
  "document_kind": "show_cause | warning | explanation_call | audit_para | other",
  "kind_confidence": 0.0,
  "allegations": [
    {
      "text": "what is alleged / the audit objection, in the document's words",
      "invoked_rule": "rule/section cited against the officer, or null",
      "invoked_authority": "issuing/charging authority, or null",
      "questions": [
        "targeted question the officer must answer to rebut THIS allegation"
      ]
    }
  ],
  "relief_or_explanation_sought": "what the officer must show/produce, or null",
  "forum": "deciding authority / DAC / PAC, or null",
  "deadline": "date | 'within N days' | null"
}
```

## Tolerant parsing rules (R2)

1. Strip markdown fences / leading prose; take the **first balanced `{ … }`** object.
2. `serde_json` into the `Frame` DTO with `#[serde(default)]` on every field; ignore unknown keys.
3. Coerce: `kind_confidence` clamp 0–1; `document_kind` unknown → `other`; null/absent optional
   fields → `None`; `allegations` absent/empty → trigger fallback (R4).
4. On parse failure: retry ONCE with a terse "return ONLY the JSON object" reminder; if it still
   fails → fallback single question (R4). Log raw output for eval.
5. Assign stable `id`s to allegations/questions after parse (model isn't asked to number them).

## Grounding (Ground phase)

For each `invoked_rule` (and any rule the officer names), retrieve verbatim from the **corpus**
(citation gate, R7). Never quote a rule from the model. Result → `CitedRule { found, verbatim,
citation }`. A miss ⇒ `found=false` ⇒ ⚠ line in the reply.

## Reply templates (config fragments)

### Show-cause / warning
```
1. Facts (from officer's answers, neutral)
2. Governing rule — <verbatim quote> [citation]
3. Application / authority relied on — how the action complied; sanction/approval/written order
4. Absence of mala fide & loss — no personal gain; bona fide; (written-orders position if any)
5. Relief sought — request to drop/close; alternative mitigation
── Documents to attach: <checklist; ⚠ for any gap>
```

### Audit para (DAC/PAC)
```
1. Para & objection (from the document)
2. Factual position (officer's answers)
3. Regulatory position — <verbatim rule quote> [citation]
4. Status sought — settled / regularized / recovery effected / placed before DAC
── Documents to attach: <checklist; ⚠ for any gap>
```

Composition reuses `services/drafting` + the citation gate; export via `export_draft` (plain
text). Confidential spans are masked client-side (F020) before submit; the reply is re-filled for
display only.

## Guardrails encoded here (FR-011, Principle XI)

- Legal text rendered ONLY from `CitedRule.verbatim`.
- Every analytical sentence rests on a cited provision or a stated officer fact.
- Gaps render as explicit ⚠ items — the model may not assert an unproven fact or an unlocated rule.
