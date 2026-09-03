//! T018 — Per-`DocType` system prompts for the LLM IR builder.
//!
//! Each prompt is CoT-style with one few-shot example (per
//! `feedback_prompt_engineering`). The LLM emits JSON conforming to
//! `schema.json` — wrapped via llguidance at sampling time, so prompts focus
//! on *what to include / how to think about structure*, not JSON syntax.

use crate::services::doc_ir::types::DocType;

pub const IR_BUILDER_SYSTEM_PROMPT_VERSION: u32 = 1;

/// Return the system prompt for a given doc type.
pub fn system_prompt_for(doc_type: DocType) -> &'static str {
    match doc_type {
        DocType::Constitution => PROMPT_CONSTITUTION,
        DocType::Act => PROMPT_ACT,
        DocType::Rules => PROMPT_RULES,
        DocType::Sro => PROMPT_SRO,
        DocType::Contract => PROMPT_CONTRACT,
        DocType::Generic => PROMPT_GENERIC,
    }
}

/// Short classifier prompt that picks one of six `doc_type` strings.
pub const DOC_TYPE_CLASSIFIER_PROMPT: &str = r#"You are classifying a legal/structured document by its top-of-document text.

Pick exactly one label that best matches:
- "constitution": a national or provincial constitution (Articles, Parts, Chapters, amendments)
- "act": an enacted statute / law (Sections, Chapters, Schedules, Acts of Parliament)
- "rules": subordinate rules made under an Act (Rule 1, Rule 2, forms appended)
- "sro": a Statutory Regulatory Order / gazette notification with an SRO number
- "contract": a contract / agreement (Parties, Recitals, Clauses, signature blocks)
- "generic": none of the above — a free-form document with simple headings

Reply with a JSON object: {"doc_type": "<label>"}.
"#;

const PROMPT_HEADER: &str = r#"You are a legal-document structural extractor.

Goal: convert one chunk of a PDF document's extracted text into a JSON object
that wraps an array of structural nodes, one per section / clause / sub-clause /
proviso / explanation / footnote / paragraph in the source text.

OUTPUT FORMAT — you MUST respond with exactly this shape (no prose, no markdown
fences, no commentary, just JSON):

{
  "nodes": [
    {
      "id": "n0001",
      "parent_id": null,
      "kind": "section",
      "number": "1",
      "title": "Short title",
      "text": "<body text of this node, or empty string for pure containers>",
      "page": 1,
      "quality_flag": "ok"
    },
    {
      "id": "n0002",
      "parent_id": "n0001",
      "kind": "clause",
      "number": "a",
      "title": null,
      "text": "<clause body>",
      "page": 1,
      "quality_flag": "ok"
    }
  ]
}

Each node MUST have at minimum: `id`, `kind`, `text`, `page`, `quality_flag`.
Optional fields: `parent_id`, `number`, `title`, `annotations`, `footnote_refs`,
`metadata`.

JSON SYNTAX RULES (strict — failure to follow these makes the output unparseable):
- Use double quotes (") for ALL strings. NEVER use single quotes (').
- Escape every literal double quote that appears INSIDE a string value as \".
  Example: the source phrase  said "hello".  becomes  "text": "said \"hello\"."
- Escape every newline inside a string value as \n (not a raw line break).
- Do not include trailing commas, comments, or Python-style syntax.
- Do not invent extra top-level fields on a node — only the fields listed above.

Rules you MUST follow:
1. Cover ≥90% of the source text. Do not drop body paragraphs.
2. Use `id` values of the form `n0001`, `n0002`, ... unique within the doc.
3. `parent_id` must reference an `id` that exists (in this chunk or an earlier
   one — the user message tells you the latest `id` seen).
4. `kind` must be one of: part, chapter, section, subsection, clause,
   sub_clause, proviso, explanation, schedule, preamble, paragraph, heading,
   footnote.
5. `page` is the 1-based page index where the node starts. Use 1 if unsure.
6. Omissions (text marked "omitted", "***", etc.) appear as a node with
   `text=""` and `annotations: [{"type":"omission","by":"..."}]`.
7. Amendments / substitutions / insertions go in `annotations` on the node
   they apply to. `type` is one of insertion|substitution|omission|renumbering|proviso_added.
8. Footnote bodies become `kind: "footnote"` nodes; nodes that cite them set
   `footnote_refs: ["fNNNN"]`.
9. Set `quality_flag: "ok"` for every node you emit.

Begin your response with `{` and end with `}`. Nothing else.
"#;

const PROMPT_CONSTITUTION: &str = r#"DOC_TYPE: constitution.

Structural conventions you will see:
- Parts (Part I, II, ...), Chapters within Parts (Chapter 1, 2, ...), Articles
  inside Chapters, Clauses (1), (2), (3) inside Articles, Sub-clauses
  (a), (b), (c) inside Clauses, Provisos, Explanations.
- A Preamble at the start (kind: preamble).
- Footnotes referenced in superscript and printed at page bottom or end.
- Amendments noted in footnotes ("Subs. by Act XLII of 2019, s. 2") — capture
  as annotations.type = "substitution" on the affected node with
  by = "Act XLII of 2019".

Example chunk → output:

INPUT:
  PART III — FUNDAMENTAL RIGHTS
  Chapter 1
  Article 8. Laws inconsistent with Fundamental Rights to be void.
   (1) Any law, or any custom or usage having the force of law, in so far as
       it is inconsistent with the rights conferred by this Chapter, shall, to
       the extent of such inconsistency, be void.

OUTPUT:
{"nodes":[
  {"id":"n0001","parent_id":null,"kind":"part","number":"III","title":"Fundamental Rights","text":"","page":12,"quality_flag":"ok"},
  {"id":"n0002","parent_id":"n0001","kind":"chapter","number":"1","title":null,"text":"","page":12,"quality_flag":"ok"},
  {"id":"n0003","parent_id":"n0002","kind":"section","number":"8","title":"Laws inconsistent with Fundamental Rights to be void.","text":"","page":12,"quality_flag":"ok"},
  {"id":"n0004","parent_id":"n0003","kind":"clause","number":"1","title":null,"text":"Any law, or any custom or usage having the force of law, in so far as it is inconsistent with the rights conferred by this Chapter, shall, to the extent of such inconsistency, be void.","page":12,"quality_flag":"ok"}
]}
"#;

const PROMPT_ACT: &str = r#"DOC_TYPE: act.

Structural conventions:
- Chapter / Part headings (Chapter I, II, ...), Sections numbered 1, 2, 3,
  with bold titles; Sub-sections (1), (2), (3); Clauses (a), (b), (c) inside
  sub-sections; Provisos ("Provided that ..."), Explanations.
- A Schedule (kind: schedule) usually at the end.
- Amendment markers like [*], [1] with footnote bodies — emit those as
  kind: footnote nodes and link via footnote_refs.

Example:

INPUT:
  Section 3. Definitions.
   (1) In this Act, unless the context otherwise requires —
       (a) "applicant" means a person who applies for registration;

OUTPUT:
{"nodes":[
  {"id":"n0001","parent_id":null,"kind":"section","number":"3","title":"Definitions","text":"","page":2,"quality_flag":"ok"},
  {"id":"n0002","parent_id":"n0001","kind":"subsection","number":"1","title":null,"text":"In this Act, unless the context otherwise requires —","page":2,"quality_flag":"ok"},
  {"id":"n0003","parent_id":"n0002","kind":"clause","number":"a","title":null,"text":"\"applicant\" means a person who applies for registration;","page":2,"quality_flag":"ok"}
]}
"#;

const PROMPT_RULES: &str = r#"DOC_TYPE: rules.

Structural conventions:
- Rules numbered Rule 1, Rule 2, ... (use kind: section with number "1").
- Sub-rules (1), (2); paragraphs (a), (b), (c).
- Forms appended at the end — emit as kind: schedule.
- Parent Act usually cited at top — capture in doc_metadata if obvious.

Example:

INPUT:
  Rule 5. Manner of filing.
   (1) Every application shall be filed in Form A.

OUTPUT:
{"nodes":[
  {"id":"n0001","parent_id":null,"kind":"section","number":"5","title":"Manner of filing","text":"","page":3,"quality_flag":"ok"},
  {"id":"n0002","parent_id":"n0001","kind":"subsection","number":"1","title":null,"text":"Every application shall be filed in Form A.","page":3,"quality_flag":"ok"}
]}
"#;

const PROMPT_SRO: &str = r#"DOC_TYPE: sro.

Structural conventions:
- Header: "S.R.O. <number>(<I>)/<year>" + "Islamabad, the <date>" + issuing
  ministry.
- Body usually a single notification with numbered paragraphs.
- doc_metadata should carry sro_number, gazette_date, issuing_authority
  if discoverable in the chunk.
- Treat each numbered paragraph as kind: section if numbered, else
  kind: paragraph.

Output only the JSON object matching the schema. No prose.
"#;

const PROMPT_CONTRACT: &str = r#"DOC_TYPE: contract.

Structural conventions:
- Parties / Recitals at the top — capture as kind: preamble.
- Numbered clauses (1, 1.1, 1.1.1) — map to kind: clause (top-level),
  kind: sub_clause (deeper), nest via parent_id.
- Schedules / annexures appended — kind: schedule.
- Signature block at end — kind: paragraph with title "Signatures".

doc_metadata may carry party_a, party_b, effective_date when visible.

Output only the JSON object matching the schema. No prose.
"#;

const PROMPT_GENERIC: &str = r#"DOC_TYPE: generic.

The document has no recognised statutory structure. Emit one node per heading
(kind: heading) and one node per paragraph beneath (kind: paragraph), nested
via parent_id. Skip annotation/footnote machinery.

Output only the JSON object matching the schema. No prose.
"#;

/// Compose the full system prompt: header + per-type block.
pub fn full_system_prompt(doc_type: DocType) -> String {
    format!("{PROMPT_HEADER}\n{}", system_prompt_for(doc_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_doc_type_has_a_prompt() {
        for dt in [
            DocType::Constitution,
            DocType::Act,
            DocType::Rules,
            DocType::Sro,
            DocType::Contract,
            DocType::Generic,
        ] {
            let p = system_prompt_for(dt);
            assert!(p.len() > 100, "prompt too short for {dt:?}");
        }
    }

    #[test]
    fn full_prompt_contains_header_and_body() {
        let p = full_system_prompt(DocType::Constitution);
        // Header was rewritten in round 5 (slice B verification) to show the
        // explicit `{"nodes":[...]}` wrapper instead of mentioning
        // "StructureNode" by name. Assertion now checks for the wrapper.
        assert!(p.contains("\"nodes\""));
        assert!(p.contains("DOC_TYPE: constitution"));
    }
}
