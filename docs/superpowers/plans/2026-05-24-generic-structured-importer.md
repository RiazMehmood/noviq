# Generic Structured Document Importer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend Feature 003's structured-document pipeline so every indexed PDF (Act, Rules, Manual, anything else) produces a clean hierarchy + `graph_nodes` + `graph_edges`, not only the Pakistan Constitution.

**Architecture:** Unified 7-step pipeline reusing Feature 003's `DocProfile` + `structure_parser` + `graph_builder` + `heading_detector` infrastructure. Two new services (`text_normalizer`, `structured_importer`), one new migration (`017`), four new doc-type profiles (`act`, `rules`, `manual`, `generic`). Hooks into existing `do_index` and Feature 004's startup re-extract pattern.

**Tech Stack:** Rust 1.87, sqlx, `regex_lite`, existing `services/heading_detector/{regex_pass,llm_pass,tree}.rs`, existing `services/structure_parser.rs`, existing `services/graph_builder.rs`.

**Spec:** `docs/superpowers/specs/2026-05-24-generic-structured-importer-design.md`

---

## File map

### Created
- `src-tauri/src/db/migrations/017_structured_importer.sql` — adds 4 columns to `sources`
- `src-tauri/src/services/doc_profiles/act.rs` — Act/Ordinance/Code profile
- `src-tauri/src/services/doc_profiles/rules.rs` — Rules/Regulations profile
- `src-tauri/src/services/doc_profiles/manual.rs` — Manual/Handbook profile
- `src-tauri/src/services/doc_profiles/generic.rs` — Generic fallback profile
- `src-tauri/src/services/text_normalizer.rs` — OCR cleanup pass
- `src-tauri/src/services/structured_importer.rs` — orchestrator
- `src-tauri/tests/structured_importer_acceptance.rs` — Layer-3 acceptance gate
- `src-tauri/tests/fixtures/structured_indexing/*.snapshot.txt` — Layer-2 golden snapshots (5 PDFs + JSON)

### Modified
- `src-tauri/src/services/doc_profiles/mod.rs` — extend `DocProfile` struct; add registry; refactor `is_salient_unit`
- `src-tauri/src/services/doc_profiles/constitution.rs` — set new fields to empty defaults
- `src-tauri/src/services/heading_detector/regex_pass.rs` — add profile-aware `scan_with_profile`
- `src-tauri/src/services/heading_detector/tree.rs` — add 6-level builder `build_tree_levelled`
- `src-tauri/src/services/structure_parser.rs` — expose `persist_doc_tree` that consumes the generic tree
- `src-tauri/src/services/indexer.rs` — wire `structured_importer` into `do_index`; add `enqueue_structure_reextract` startup hook
- `src-tauri/src/lib.rs` — register `services::text_normalizer` and `services::structured_importer` modules; call new startup hook

---

## Task 1: Migration 017 — schema columns

**Files:**
- Create: `src-tauri/src/db/migrations/017_structured_importer.sql`
- Test: `src-tauri/tests/schema_wipe_test.rs` (existing — verify still passes)

- [ ] **Step 1: Write the migration**

```sql
-- Feature 005 — generic structured importer.
-- Adds per-source columns for the resolved profile + structure pipeline outcome
-- + version tag used by the startup re-index hook (mirrors Feature 004's
-- extraction_method / quality_score pattern).

ALTER TABLE sources ADD COLUMN doc_type         TEXT;
ALTER TABLE sources ADD COLUMN doc_type_source  TEXT
                     CHECK (doc_type_source IN ('auto','llm','user'));
ALTER TABLE sources ADD COLUMN structure_status TEXT
                     CHECK (structure_status IN ('structured','failed','skipped'));
ALTER TABLE sources ADD COLUMN structure_importer_version INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_sources_structure_version ON sources(structure_importer_version);
```

- [ ] **Step 2: Run migrations on a throwaway DB to confirm syntax**

Run from `src-tauri/`:
```bash
cargo test --test schema_wipe_test -- --nocapture
```
Expected: PASS. (`schema_wipe_test` drops the test DB and re-runs all migrations; will fail loudly if 017 has a syntax error.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/migrations/017_structured_importer.sql
git commit -m "feat(005): migration 017 — sources columns for structured importer"
```

---

## Task 2: Extend `DocProfile` struct with new fields

**Files:**
- Modify: `src-tauri/src/services/doc_profiles/mod.rs`
- Modify: `src-tauri/src/services/doc_profiles/constitution.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/doc_profiles/mod.rs`:

```rust
#[cfg(test)]
mod struct_tests {
    use super::*;

    #[test]
    fn constitution_profile_has_empty_detection_and_heading_patterns() {
        let p = constitution::CONSTITUTION;
        assert!(p.detection_patterns.is_empty(),
            "constitution bypasses regex detection — it consumes a structured IR");
        assert!(p.heading_patterns.is_empty(),
            "constitution bypasses heading detection — same reason");
        assert!(p.min_confidence >= 0.0 && p.min_confidence <= 1.0);
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib doc_profiles::struct_tests -- --nocapture
```
Expected: FAIL with "no field `detection_patterns`".

- [ ] **Step 3: Add the new types and fields**

Replace the body of `src-tauri/src/services/doc_profiles/mod.rs` (keep the existing module docstring and `is_salient_unit` for now):

```rust
//! Doc-type profiles — per-document-type level vocabulary and citation rules.
//! v1 shipped `constitution`. Feature 005 adds `act`, `rules`, `manual`,
//! `generic` with detection + heading regex patterns so unstructured PDFs
//! flow through the same structure_parser path.

pub mod constitution;
pub mod act;
pub mod rules;
pub mod manual;
pub mod generic;

use crate::services::structure_parser::NodeLevel;

/// A document-type profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocProfile {
    pub doc_type: &'static str,
    pub level_vocab: [&'static str; 6],
    pub citation_unit: &'static str,
    pub salient_units: &'static [&'static str],

    /// Patterns scored when picking a profile for an unknown document.
    pub detection_patterns: &'static [DetectionPattern],
    /// Patterns used to build the heading tree once the profile is picked.
    pub heading_patterns:   &'static [HeadingPattern],
    /// Detection score below this → fall through to next profile / generic.
    /// 0.0 disables the floor (used by `generic`).
    pub min_confidence:     f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionPattern {
    pub regex:  &'static str,
    /// Weight added to the profile's score per match. Fractional weights allowed.
    pub weight: u32,        // u32 to keep `Eq` derive; scale 1 ~= "one heading".
    pub level:  NodeLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingPattern {
    pub regex:        &'static str,
    pub level:        NodeLevel,
    /// Display template for the citation, with `{num}` / `{sub}` / `{clause}`
    /// placeholders that will be filled from the regex capture groups.
    pub citation_fmt: &'static str,
}

/// Returns true when `label` is a salient unit in any registered DocProfile.
pub fn is_salient_unit(label: &str) -> bool {
    REGISTRY.iter().any(|p| p.salient_units.iter().any(|&u| u == label))
}

/// All profiles in detection priority order. `generic` is last (always matches).
pub const REGISTRY: &[&DocProfile] = &[
    &constitution::CONSTITUTION,
    &act::ACT,
    &rules::RULES,
    &manual::MANUAL,
    &generic::GENERIC,
];
```

Update `src-tauri/src/services/doc_profiles/constitution.rs` to fill defaults for the new fields:

```rust
//! The `constitution` doc-type profile (feature 003, task T010).
//! Feature 005 leaves detection/heading patterns empty: constitution still
//! consumes a structured IR via `try_load_constitution`, which routes around
//! the regex pipeline.

use super::{DocProfile, DetectionPattern, HeadingPattern};

pub const CONSTITUTION: DocProfile = DocProfile {
    doc_type: "constitution",
    level_vocab: [
        "document", "part", "chapter", "article", "clause", "subclause",
    ],
    citation_unit: "Article",
    salient_units: &["Preamble"],
    detection_patterns: &[] as &[DetectionPattern],
    heading_patterns:   &[] as &[HeadingPattern],
    min_confidence:     0.0,
};
```

Note: `act.rs`, `rules.rs`, `manual.rs`, `generic.rs` don't exist yet — they're stubbed in the next task. For now create empty placeholder files so `mod.rs` compiles:

```bash
mkdir -p src-tauri/src/services/doc_profiles
for p in act rules manual generic; do
  cat > src-tauri/src/services/doc_profiles/${p}.rs <<EOF
//! Placeholder — filled in Task ${p}.
use super::{DocProfile, DetectionPattern, HeadingPattern};
pub const $(echo $p | tr a-z A-Z): DocProfile = DocProfile {
    doc_type: "$p",
    level_vocab: ["document","part","chapter","article","clause","subclause"],
    citation_unit: "Section",
    salient_units: &[],
    detection_patterns: &[] as &[DetectionPattern],
    heading_patterns:   &[] as &[HeadingPattern],
    min_confidence:     0.0,
};
EOF
done
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib doc_profiles -- --nocapture
```
Expected: PASS (3+ tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/doc_profiles/
git commit -m "feat(005): extend DocProfile with detection + heading patterns"
```

---

## Task 3: Profile `act` — full definition

**Files:**
- Modify: `src-tauri/src/services/doc_profiles/act.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/doc_profiles/act.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    #[test]
    fn detection_patterns_match_act_corpus() {
        let positives = [
            "Section 3. The competent authority shall...",
            "Section 7(2)(a) The order shall be in writing",
            "Chapter II — Powers of the Authority",
        ];
        for sample in positives {
            assert!(
                ACT.detection_patterns.iter().any(|p| {
                    Regex::new(p.regex).unwrap().is_match(sample)
                }),
                "no detection pattern matched: {sample:?}",
            );
        }
    }

    #[test]
    fn detection_patterns_reject_rules_corpus() {
        let negatives = [
            "Rule 9. The Secretary shall...",
            "Sub-rule (1) A reference...",
        ];
        for sample in negatives {
            assert!(
                !ACT.detection_patterns.iter().any(|p| {
                    Regex::new(p.regex).unwrap().is_match(sample)
                }),
                "act pattern wrongly matched rules-style line: {sample:?}",
            );
        }
    }

    #[test]
    fn heading_patterns_present_for_three_levels() {
        let levels: std::collections::HashSet<_> =
            ACT.heading_patterns.iter().map(|h| h.level).collect();
        assert!(levels.contains(&crate::services::structure_parser::NodeLevel::Chapter));
        assert!(levels.contains(&crate::services::structure_parser::NodeLevel::Article));
        assert!(levels.contains(&crate::services::structure_parser::NodeLevel::Clause));
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib doc_profiles::act -- --nocapture
```
Expected: FAIL.

- [ ] **Step 3: Replace the placeholder with the real profile**

Replace the entire body of `src-tauri/src/services/doc_profiles/act.rs`:

```rust
//! `act` profile — Acts / Ordinances / Codes.
//! Chapter → Section → Sub-section → Clause hierarchy. Detected by lines
//! starting with `Section N` or `Chapter <roman>`.

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const ACT: DocProfile = DocProfile {
    doc_type: "act",
    level_vocab: [
        "document", "part", "chapter", "section", "subsection", "clause",
    ],
    citation_unit: "Section",
    salient_units: &["Preamble"],
    detection_patterns: &[
        DetectionPattern { regex: r"(?m)^Section\s+\d+\.", weight: 3, level: NodeLevel::Article },
        DetectionPattern { regex: r"(?m)^Chapter\s+[IVX]+\b", weight: 2, level: NodeLevel::Chapter },
    ],
    heading_patterns: &[
        HeadingPattern {
            regex: r"(?m)^Chapter\s+(?P<num>[IVX\d]+)(?:\s*[—-]\s*(?P<title>[^\n]+))?",
            level: NodeLevel::Chapter,
            citation_fmt: "Chapter {num}",
        },
        HeadingPattern {
            regex: r"(?m)^Section\s+(?P<num>\d+[A-Z]?)\.\s*(?P<title>[^\n]+)?",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        HeadingPattern {
            regex: r"(?m)^\((?P<num>\d+)\)\s+",
            level: NodeLevel::Clause,
            citation_fmt: "Section {parent_num}({num})",
        },
        HeadingPattern {
            regex: r"(?m)^\((?P<num>[a-z])\)\s+",
            level: NodeLevel::SubClause,
            citation_fmt: "Section {parent_num}({parent_sub})({num})",
        },
    ],
    min_confidence: 5.0,
};
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib doc_profiles::act -- --nocapture
```
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/doc_profiles/act.rs
git commit -m "feat(005): act profile — detection + heading patterns"
```

---

## Task 4: Profile `rules` — full definition

**Files:**
- Modify: `src-tauri/src/services/doc_profiles/rules.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/doc_profiles/rules.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;
    use crate::services::structure_parser::NodeLevel;

    #[test]
    fn detection_matches_rules_corpus() {
        let positives = [
            "Rule 9. The Secretary shall arrange...",
            "Rule 12A.  Reconciliation of receipts",
            "Part III — Procedure",
            "(2) Subject to sub-rule (1), the Cabinet shall...",
        ];
        for s in positives {
            assert!(
                RULES.detection_patterns.iter().any(|p| Regex::new(p.regex).unwrap().is_match(s)),
                "no rules detection pattern matched: {s:?}",
            );
        }
    }

    #[test]
    fn citation_unit_is_rule() {
        assert_eq!(RULES.citation_unit, "Rule");
        assert_eq!(RULES.level_vocab[NodeLevel::Article as usize], "rule");
        assert_eq!(RULES.level_vocab[NodeLevel::Clause as usize], "subrule");
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib doc_profiles::rules -- --nocapture
```
Expected: FAIL.

- [ ] **Step 3: Replace the placeholder**

Replace `src-tauri/src/services/doc_profiles/rules.rs`:

```rust
//! `rules` profile — Rules / Regulations.
//! Part → Rule → Sub-rule → Clause. Detected by `Rule N` lines.

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const RULES: DocProfile = DocProfile {
    doc_type: "rules",
    level_vocab: [
        "document", "part", "chapter", "rule", "subrule", "clause",
    ],
    citation_unit: "Rule",
    salient_units: &["Preamble"],
    detection_patterns: &[
        DetectionPattern { regex: r"(?m)^Rule\s+\d+[A-Z]?\.", weight: 4, level: NodeLevel::Article },
        DetectionPattern { regex: r"(?m)^Part\s+[IVX\d]+\b", weight: 2, level: NodeLevel::Part },
        DetectionPattern { regex: r"(?m)^\(\d+\)\s+[A-Z]", weight: 1, level: NodeLevel::Clause },
    ],
    heading_patterns: &[
        HeadingPattern {
            regex: r"(?m)^Part\s+(?P<num>[IVX\d]+)(?:\s*[—-]\s*(?P<title>[^\n]+))?",
            level: NodeLevel::Part,
            citation_fmt: "Part {num}",
        },
        HeadingPattern {
            regex: r"(?m)^Rule\s+(?P<num>\d+[A-Z]?)\.\s*(?P<title>[^\n]+)?",
            level: NodeLevel::Article,
            citation_fmt: "Rule {num}",
        },
        HeadingPattern {
            regex: r"(?m)^\((?P<num>\d+)\)\s+",
            level: NodeLevel::Clause,
            citation_fmt: "Rule {parent_num}({num})",
        },
        HeadingPattern {
            regex: r"(?m)^\((?P<num>[a-z])\)\s+",
            level: NodeLevel::SubClause,
            citation_fmt: "Rule {parent_num}({parent_sub})({num})",
        },
    ],
    min_confidence: 6.0,
};
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib doc_profiles::rules -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/doc_profiles/rules.rs
git commit -m "feat(005): rules profile — detection + heading patterns"
```

---

## Task 5: Profile `manual` — full definition

**Files:**
- Modify: `src-tauri/src/services/doc_profiles/manual.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/doc_profiles/manual.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    #[test]
    fn detection_matches_manual_corpus() {
        let positives = [
            "CHAPTER II",
            "1. RECEIPT AND DESPATCH",
            "1.2 Indexing of files",
        ];
        for s in positives {
            assert!(
                MANUAL.detection_patterns.iter().any(|p| Regex::new(p.regex).unwrap().is_match(s)),
                "no manual detection pattern matched: {s:?}",
            );
        }
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib doc_profiles::manual -- --nocapture
```
Expected: FAIL.

- [ ] **Step 3: Replace the placeholder**

```rust
//! `manual` profile — Manuals / Handbooks (e.g. "Manual of Secretariat
//! Instructions"). Chapter → Section → Sub-section hierarchy with outline
//! numbering.

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const MANUAL: DocProfile = DocProfile {
    doc_type: "manual",
    level_vocab: [
        "document", "part", "chapter", "section", "subsection", "clause",
    ],
    citation_unit: "Section",
    salient_units: &["Preamble", "Foreword"],
    detection_patterns: &[
        DetectionPattern { regex: r"(?m)^CHAPTER\s+[IVX]+\b", weight: 3, level: NodeLevel::Chapter },
        DetectionPattern { regex: r"(?m)^\d+\.\s+[A-Z][A-Z\s]{4,}$", weight: 2, level: NodeLevel::Article },
        DetectionPattern { regex: r"(?m)^\d+\.\d+\s+[A-Z]", weight: 1, level: NodeLevel::Clause },
    ],
    heading_patterns: &[
        HeadingPattern {
            regex: r"(?m)^CHAPTER\s+(?P<num>[IVX]+)(?:\s+(?P<title>[A-Z][^\n]+))?",
            level: NodeLevel::Chapter,
            citation_fmt: "Chapter {num}",
        },
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+)\.\s+(?P<title>[A-Z][A-Z\s]{4,})$",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+\.\d+)\s+(?P<title>[A-Z][^\n]+)",
            level: NodeLevel::Clause,
            citation_fmt: "Section {num}",
        },
    ],
    min_confidence: 4.0,
};
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib doc_profiles::manual -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/doc_profiles/manual.rs
git commit -m "feat(005): manual profile — detection + heading patterns"
```

---

## Task 6: Profile `generic` — fallback

**Files:**
- Modify: `src-tauri/src/services/doc_profiles/generic.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/doc_profiles/generic.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    #[test]
    fn min_confidence_is_zero_so_generic_always_wins_when_nothing_else_does() {
        assert_eq!(GENERIC.min_confidence, 0.0);
    }

    #[test]
    fn heading_patterns_capture_numbered_outline() {
        let text = "1. Introduction\n\nbody\n\n1.1 Background\n\nmore\n\n2. Method";
        let any_match = GENERIC.heading_patterns.iter()
            .any(|h| Regex::new(h.regex).unwrap().is_match(text));
        assert!(any_match, "generic should detect numbered outline headings");
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib doc_profiles::generic -- --nocapture
```
Expected: FAIL.

- [ ] **Step 3: Replace the placeholder**

```rust
//! `generic` profile — last-resort fallback for any document that doesn't
//! match a specific profile. Produces a chapter → section → leaf tree from
//! visual headings (ALL-CAPS lines, outline numbering, bold-marker proxies).

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const GENERIC: DocProfile = DocProfile {
    doc_type: "generic",
    level_vocab: [
        "document", "part", "chapter", "section", "subsection", "clause",
    ],
    citation_unit: "Section",
    salient_units: &[],
    // No detection patterns — `generic` always wins as fallback (min_confidence=0).
    detection_patterns: &[] as &[DetectionPattern],
    heading_patterns: &[
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+)\.\s+(?P<title>[A-Z][^\n]{2,80})$",
            level: NodeLevel::Chapter,
            citation_fmt: "Chapter {num}",
        },
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+\.\d+)\s+(?P<title>[A-Z][^\n]{2,80})",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+\.\d+\.\d+)\s+(?P<title>[^\n]{2,80})",
            level: NodeLevel::Clause,
            citation_fmt: "Section {num}",
        },
    ],
    min_confidence: 0.0,
};
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib doc_profiles::generic -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/doc_profiles/generic.rs
git commit -m "feat(005): generic profile — fallback for any document"
```

---

## Task 7: `text_normalizer` service — OCR cleanup

**Files:**
- Create: `src-tauri/src/services/text_normalizer.rs`
- Modify: `src-tauri/src/services/mod.rs` (add `pub mod text_normalizer;`)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/text_normalizer.rs`:

```rust
//! OCR text normalization — fix common tesseract misreads before structure
//! detection. Idempotent: clean text passes through unchanged.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_brace_for_paren_adjacent_to_alpha_or_digit() {
        assert_eq!(normalize("{c) text"), "(c) text");
        assert_eq!(normalize("{2)") , "(2)");
        assert_eq!(normalize("end of sentence}.") , "end of sentence).");
    }

    #[test]
    fn preserves_braces_that_arent_paren_misreads() {
        assert_eq!(normalize("see {note}"), "see {note}");
        assert_eq!(normalize("{ }"), "{ }");
    }

    #[test]
    fn fixes_known_misread_dictionary_words() {
        assert_eq!(normalize("the Govemor said"), "the Governor said");
    }

    #[test]
    fn joins_hyphenated_line_breaks() {
        assert_eq!(normalize("auto-\nmatic"), "automatic");
    }

    #[test]
    fn collapses_smart_quotes_to_ascii() {
        assert_eq!(normalize("\u{201C}quoted\u{201D}"), "\"quoted\"");
        assert_eq!(normalize("it\u{2019}s"), "it's");
    }

    #[test]
    fn idempotent_on_clean_text() {
        let clean = "The Secretary shall arrange business as required.";
        assert_eq!(normalize(clean), clean);
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib services::text_normalizer -- --nocapture
```
Expected: FAIL — `normalize` doesn't exist.

- [ ] **Step 3: Implement**

Prepend to `src-tauri/src/services/text_normalizer.rs`:

```rust
use regex_lite::Regex;
use std::sync::OnceLock;

/// All known misreads. v1 cap: ≤ 50 entries, every entry reviewed manually.
/// Entries must have ≥ 2-letter Levenshtein distance from any common English
/// word to avoid false corrections. Each (misread, correct) pair is tested.
const MISREADS: &[(&str, &str)] = &[
    ("Govemor",  "Governor"),
    ("Govemment","Government"),
    ("Sccretary","Secretary"),
    ("Mimster",  "Minister"),
    ("Deparlment","Department"),
    // Add more here as new corpus reveals them; ship empty rather than guess.
];

fn brace_fix_open() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\{(?P<after>[a-z0-9])\)").unwrap())
}

fn brace_fix_close() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?P<before>[a-z0-9])\}").unwrap())
}

fn hyphen_join() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?P<a>[a-z])-\n(?P<b>[a-z])").unwrap())
}

/// Normalize OCR'd or scanned text. Pure function; safe to apply twice.
pub fn normalize(input: &str) -> String {
    let mut out = input.to_string();

    // 1. Brace → paren when adjacent to alnum (tesseract misread of (/[).
    out = brace_fix_open().replace_all(&out, "($after)").into_owned();
    out = brace_fix_close().replace_all(&out, "$before)").into_owned();

    // 2. Smart-quote unification to ASCII.
    out = out.replace('\u{201C}', "\"")
             .replace('\u{201D}', "\"")
             .replace('\u{2018}', "'")
             .replace('\u{2019}', "'");

    // 3. Hyphenated line-break join.
    out = hyphen_join().replace_all(&out, "$a$b").into_owned();

    // 4. Dictionary fix-ups.
    for &(bad, good) in MISREADS {
        if out.contains(bad) {
            out = out.replace(bad, good);
        }
    }
    out
}
```

Append `pub mod text_normalizer;` to `src-tauri/src/services/mod.rs` (alphabetical position).

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib services::text_normalizer -- --nocapture
```
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/text_normalizer.rs src-tauri/src/services/mod.rs
git commit -m "feat(005): text_normalizer service for OCR cleanup"
```

---

## Task 8: Profile picker — `detect_profile`

**Files:**
- Modify: `src-tauri/src/services/doc_profiles/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/doc_profiles/mod.rs`:

```rust
#[cfg(test)]
mod picker_tests {
    use super::*;

    #[test]
    fn picks_rules_for_rule_dense_text() {
        let text = "
Rule 1. Short title.
Rule 2. Definitions.
Rule 9. The Secretary shall arrange...
Rule 12. Reconciliation.
";
        let (profile, source) = detect_profile(text, None);
        assert_eq!(profile.doc_type, "rules");
        assert_eq!(source, DetectionSource::Auto);
    }

    #[test]
    fn picks_act_for_section_dense_text() {
        let text = "
Section 1. Short title.
Section 2. Definitions.
Section 3. Removal from service.
";
        let (profile, _) = detect_profile(text, None);
        assert_eq!(profile.doc_type, "act");
    }

    #[test]
    fn picks_generic_when_nothing_matches() {
        let text = "Just some paragraphs.\nNo legal headings here at all.";
        let (profile, source) = detect_profile(text, None);
        assert_eq!(profile.doc_type, "generic");
        assert_eq!(source, DetectionSource::Auto);
    }

    #[test]
    fn user_override_wins() {
        let text = "Section 1. ... Section 2. ...";  // would auto-detect as `act`
        let (profile, source) = detect_profile(text, Some("rules"));
        assert_eq!(profile.doc_type, "rules");
        assert_eq!(source, DetectionSource::User);
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib doc_profiles::picker_tests -- --nocapture
```
Expected: FAIL.

- [ ] **Step 3: Implement**

Append to `src-tauri/src/services/doc_profiles/mod.rs`:

```rust
use regex_lite::Regex;

/// Where the resolved doc_type came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionSource { Auto, Llm, User }

impl DetectionSource {
    pub fn as_str(self) -> &'static str {
        match self { Self::Auto => "auto", Self::Llm => "llm", Self::User => "user" }
    }
}

/// Pick the best-matching profile for `text`. If `force` is `Some`, returns
/// that profile with `DetectionSource::User` (used for re-index with a stored
/// user override). Constitution is excluded from auto-detection — it routes
/// via `try_load_constitution` before this function is reached.
pub fn detect_profile(text: &str, force: Option<&str>) -> (&'static DocProfile, DetectionSource) {
    if let Some(name) = force {
        for &p in REGISTRY {
            if p.doc_type == name {
                return (p, DetectionSource::User);
            }
        }
        // Fall through to auto-detect if the stored name is unknown.
    }

    let mut best: Option<(&'static DocProfile, u32)> = None;
    for &p in REGISTRY {
        if p.doc_type == "constitution" { continue; }  // structured-IR path only
        let mut score: u32 = 0;
        for pat in p.detection_patterns {
            let re = Regex::new(pat.regex).expect("invalid regex in profile");
            score = score.saturating_add(pat.weight * (re.find_iter(text).count() as u32));
        }
        let passes = score as f32 >= p.min_confidence;
        if passes {
            match best {
                None => best = Some((p, score)),
                Some((_, s)) if score > s => best = Some((p, score)),
                _ => {}
            }
        }
    }
    let (winner, _) = best.unwrap_or((&generic::GENERIC, 0));
    (winner, DetectionSource::Auto)
}
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib doc_profiles -- --nocapture
```
Expected: PASS (all profile + picker tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/doc_profiles/mod.rs
git commit -m "feat(005): detect_profile — regex scoring + user override"
```

---

## Task 9: Profile-aware heading scan

**Files:**
- Modify: `src-tauri/src/services/heading_detector/regex_pass.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/heading_detector/regex_pass.rs` (inside `mod tests`):

```rust
#[test]
fn scan_with_profile_uses_rules_patterns() {
    use crate::services::doc_profiles::rules::RULES;
    let text = "Rule 1. Short title.\n\nbody\n\nRule 2. Definitions.\n\nbody";
    let hits = scan_with_profile(text, &RULES);
    let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
    assert!(labels.iter().any(|l| l.starts_with("Rule 1")));
    assert!(labels.iter().any(|l| l.starts_with("Rule 2")));
}

#[test]
fn scan_with_profile_uses_act_patterns_and_ignores_rules_lines() {
    use crate::services::doc_profiles::act::ACT;
    let text = "Section 3. Authority.\n\nRule 9. (this is not an act heading)\n\nSection 4. Procedure.";
    let hits = scan_with_profile(text, &ACT);
    assert!(hits.iter().any(|h| h.label.starts_with("Section 3")));
    assert!(hits.iter().any(|h| h.label.starts_with("Section 4")));
    assert!(!hits.iter().any(|h| h.label.starts_with("Rule")));
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib heading_detector::regex_pass -- --nocapture
```
Expected: FAIL — `scan_with_profile` undefined.

- [ ] **Step 3: Implement**

Append to `src-tauri/src/services/heading_detector/regex_pass.rs`:

```rust
use crate::services::doc_profiles::DocProfile;
use crate::services::structure_parser::NodeLevel;

/// A heading candidate with explicit level (six-level vocabulary).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LevelledHeading {
    pub offset: usize,
    pub label:  String,
    pub level:  NodeLevel,
}

/// Run a profile's heading_patterns over `text`, emit ordered candidates.
pub fn scan_with_profile(text: &str, profile: &DocProfile) -> Vec<LevelledHeading> {
    let mut hits: Vec<LevelledHeading> = Vec::new();
    for hp in profile.heading_patterns {
        let re = Regex::new(hp.regex).expect("invalid heading regex in profile");
        for cap in re.captures_iter(text) {
            let m = cap.get(0).unwrap();
            let label = format_citation(hp.citation_fmt, &cap);
            hits.push(LevelledHeading {
                offset: m.start(),
                label,
                level:  hp.level,
            });
        }
    }
    hits.sort_by_key(|h| h.offset);
    hits.dedup_by_key(|h| h.offset);
    hits
}

/// Substitute `{num}` / `{title}` / `{sub}` / `{clause}` placeholders in a
/// `citation_fmt` template with the corresponding named capture groups.
/// Unknown placeholders are left as-is.
fn format_citation(template: &str, cap: &regex_lite::Captures<'_>) -> String {
    let mut out = template.to_string();
    for name in ["num", "title", "sub", "clause"] {
        if let Some(g) = cap.name(name) {
            out = out.replace(&format!("{{{name}}}"), g.as_str().trim());
        }
    }
    out
}
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib heading_detector::regex_pass -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/heading_detector/regex_pass.rs
git commit -m "feat(005): profile-aware scan_with_profile in regex_pass"
```

---

## Task 10: 6-level tree builder

**Files:**
- Modify: `src-tauri/src/services/heading_detector/tree.rs`

- [ ] **Step 1: Write the failing test**

Append inside `mod tests`:

```rust
#[test]
fn levelled_tree_nests_by_level() {
    use super::super::regex_pass::LevelledHeading;
    use crate::services::structure_parser::NodeLevel;
    let text = "Part I body\nChapter II body\nSection 3 body\n(a) body";
    let h = vec![
        LevelledHeading { offset: 0,  label: "Part I".into(),   level: NodeLevel::Part },
        LevelledHeading { offset: 12, label: "Chapter II".into(), level: NodeLevel::Chapter },
        LevelledHeading { offset: 27, label: "Section 3".into(), level: NodeLevel::Article },
        LevelledHeading { offset: 42, label: "(a)".into(),       level: NodeLevel::Clause },
    ];
    let tree = build_tree_levelled(text, &h, BuildOptions { document_title: "x".into() });
    // Expect: Document -> Part -> Chapter -> Article -> Clause
    assert_eq!(tree.children.len(), 1);  // one Part
    let part = &tree.children[0];
    assert_eq!(part.children.len(), 1);  // one Chapter
    let chapter = &part.children[0];
    assert_eq!(chapter.children.len(), 1);  // one Article
    let article = &chapter.children[0];
    assert!(article.children.iter().any(|c| c.label.as_deref() == Some("(a)")));
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib heading_detector::tree -- --nocapture
```
Expected: FAIL — `build_tree_levelled` undefined.

- [ ] **Step 3: Implement**

Append to `src-tauri/src/services/heading_detector/tree.rs`:

```rust
use super::regex_pass::LevelledHeading;
use crate::services::structure_parser::NodeLevel;

/// Build a 6-level document tree from levelled headings.
/// `headings` MUST be sorted by `offset` ascending.
/// Falls back to 1-section flat tree when `headings` is empty (same as
/// `build_tree` for compatibility).
pub fn build_tree_levelled(text: &str, headings: &[LevelledHeading], opts: BuildOptions) -> DocNode {
    let doc = DocNode {
        kind: NodeKind::Document,
        label: Some(opts.document_title.clone()),
        path: String::new(),
        byte_start: 0,
        byte_end: text.len(),
        children: vec![],
    };
    if text.is_empty() || headings.is_empty() {
        return DocNode { children: leaf_only(text), ..doc };
    }

    // Walk headings, maintain a stack of (level, &mut Node).
    let mut root = doc;
    let mut stack: Vec<NodeLevel> = vec![NodeLevel::Document];

    for (i, h) in headings.iter().enumerate() {
        // Pop until top-of-stack is shallower than this heading.
        while let Some(&top) = stack.last() {
            if (top as usize) < (h.level as usize) { break; }
            stack.pop();
        }
        // Insert this heading under whatever is at the current stack top.
        let end = headings.get(i + 1).map(|n| n.offset).unwrap_or(text.len());
        let node = DocNode {
            kind: NodeKind::Section,        // 6-level levelled tree reuses Section kind
            label: Some(h.label.clone()),
            path: h.label.clone(),
            byte_start: h.offset,
            byte_end:   end,
            children:   leaf_only(&text[h.offset..end]),
        };
        attach_at(&mut root, &stack, node);
        stack.push(h.level);
    }
    root
}

fn leaf_only(text: &str) -> Vec<DocNode> {
    if text.is_empty() { return vec![]; }
    vec![DocNode {
        kind: NodeKind::Leaf, label: None, path: String::new(),
        byte_start: 0, byte_end: text.len(), children: vec![],
    }]
}

/// Walk `root` following `stack` (excluding the root entry itself), appending
/// `node` to the deepest matched ancestor's children.
fn attach_at(root: &mut DocNode, stack: &[NodeLevel], node: DocNode) {
    let mut cur = root;
    // stack[0] is always Document (==root); follow the rest by taking the
    // last-added child at each level we descend through.
    for _ in 1..stack.len() {
        let last_idx = cur.children.len().saturating_sub(1);
        cur = &mut cur.children[last_idx];
    }
    cur.children.push(node);
}
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib heading_detector::tree -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/heading_detector/tree.rs
git commit -m "feat(005): build_tree_levelled — 6-level tree from levelled headings"
```

---

## Task 11: `structured_importer` orchestrator (without graph yet)

**Files:**
- Create: `src-tauri/src/services/structured_importer.rs`
- Modify: `src-tauri/src/services/mod.rs` (add `pub mod structured_importer;`)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/structured_importer.rs`:

```rust
//! Orchestrator: extract → normalize → detect profile → detect headings →
//! 6-level tree. (Persistence + graph wiring lands in Task 12.)

use crate::services::{doc_profiles, heading_detector, text_normalizer};
use crate::services::heading_detector::tree::{BuildOptions, DocNode};

/// Outcome of a structured-import attempt.
pub struct ImportOutcome {
    pub tree: DocNode,
    pub doc_type: &'static str,
    pub doc_type_source: doc_profiles::DetectionSource,
    /// `true` when heading coverage cleared the threshold, `false` when the
    /// caller should fall back to flat hierarchical chunking.
    pub structured: bool,
}

const MIN_HEADING_COVERAGE: f32 = 0.10;  // 10% of doc text must lie under detected headings

/// Run the structured pipeline up to the tree. Caller persists.
pub fn import(raw_text: &str, doc_title: &str, force_profile: Option<&str>) -> ImportOutcome {
    let text = text_normalizer::normalize(raw_text);
    let (profile, source) = doc_profiles::detect_profile(&text, force_profile);
    let headings = heading_detector::regex_pass::scan_with_profile(&text, profile);

    let coverage = if text.is_empty() {
        0.0
    } else {
        let covered: usize = headings.windows(2)
            .map(|w| w[1].offset.saturating_sub(w[0].offset))
            .sum::<usize>()
            + headings.last().map(|h| text.len().saturating_sub(h.offset)).unwrap_or(0);
        covered as f32 / text.len() as f32
    };

    let structured = !headings.is_empty() && coverage >= MIN_HEADING_COVERAGE;
    let tree = heading_detector::tree::build_tree_levelled(
        &text, &headings, BuildOptions { document_title: doc_title.into() }
    );

    ImportOutcome { tree, doc_type: profile.doc_type, doc_type_source: source, structured }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_text_routes_to_rules_profile_and_is_structured() {
        let text = "Rule 1. Title.\n\nbody body body\n\nRule 2. Defs.\n\nbody body body\n\nRule 3. Procedure.\n\nbody body";
        let out = import(text, "test.pdf", None);
        assert_eq!(out.doc_type, "rules");
        assert!(out.structured, "headings cover most of the doc");
    }

    #[test]
    fn unstructured_text_falls_to_generic_and_marks_unstructured() {
        let text = "just one paragraph of unstructured text that doesn't match any patterns whatsoever in any of the profiles we ship";
        let out = import(text, "memo.txt", None);
        assert_eq!(out.doc_type, "generic");
        assert!(!out.structured);
    }
}
```

Append `pub mod structured_importer;` to `src-tauri/src/services/mod.rs` (alphabetical position).

- [ ] **Step 2: Run — confirm it fails initially due to module wiring, then passes**

```bash
cd src-tauri && cargo test --lib services::structured_importer -- --nocapture
```
Expected: PASS after Step 1 wires the module (all dependencies already exist).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/services/structured_importer.rs src-tauri/src/services/mod.rs
git commit -m "feat(005): structured_importer orchestrator (tree only)"
```

---

## Task 12: Bridge tree → `chunks` + `graph_nodes` + `graph_edges`

**Files:**
- Modify: `src-tauri/src/services/structure_parser.rs`
- Modify: `src-tauri/src/services/structured_importer.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/structured_importer.rs`:

```rust
#[cfg(test)]
mod persist_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fresh_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn persist_rules_doc_creates_chunks_graph_nodes_and_edges() {
        let pool = fresh_pool().await;
        // Seed a source row.
        sqlx::query("INSERT INTO sources (id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,indexing_status,created_at) VALUES ('s1','/x','x.pdf','pdf',1,'h',0,'processing',0)")
            .execute(&pool).await.unwrap();

        let text = "Rule 1. Title.\nbody.\n\nRule 2. Definitions.\nbody.\n\nRule 3. Procedure.\nbody.";
        let out  = import(text, "x.pdf", None);
        assert!(out.structured);

        let stats = persist(&pool, "s1", &out).await.unwrap();
        assert!(stats.chunks    >= 3, "got {} chunks", stats.chunks);
        assert!(stats.graph_nodes >= 3);
        assert!(stats.graph_edges >= 2);  // contains edges from doc → each rule

        let doc_type: String = sqlx::query_scalar("SELECT doc_type FROM sources WHERE id='s1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(doc_type, "rules");
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib services::structured_importer::persist_tests -- --nocapture
```
Expected: FAIL — `persist`, `PersistStats` undefined.

- [ ] **Step 3: Implement `persist_doc_tree` in `structure_parser.rs`**

Append to `src-tauri/src/services/structure_parser.rs`:

```rust
use crate::services::heading_detector::tree::{DocNode as RawDocNode, NodeKind as RawNodeKind};

/// Persist a generic 6-level tree (built by `heading_detector::tree::build_tree_levelled`).
/// `source_text` is the post-normalization text the tree's byte offsets index
/// into; the function slices it to populate chunk content.
///
/// Returns the inserted `Chunk` rows in tree pre-order so the caller can hand
/// them to `graph_builder::build_graph`.
pub async fn persist_doc_tree(
    pool: &SqlitePool,
    source_id: &str,
    source_text: &str,
    tree: &RawDocNode,
    profile: &crate::services::doc_profiles::DocProfile,
) -> Result<Vec<Chunk>> {
    let mut inserted: Vec<Chunk> = Vec::new();
    persist_walk(pool, source_id, source_text, tree, NodeLevel::Document, None, &mut inserted, profile).await?;
    Ok(inserted)
}

#[async_recursion::async_recursion]
async fn persist_walk(
    pool: &SqlitePool,
    source_id: &str,
    source_text: &str,
    node: &RawDocNode,
    level: NodeLevel,
    parent_chunk_id: Option<String>,
    out: &mut Vec<Chunk>,
    profile: &crate::services::doc_profiles::DocProfile,
) -> Result<()> {
    let label = node.label.clone().unwrap_or_default();
    // Container chunks (document/part/chapter/article) carry the label as
    // their content; the actual prose lives in leaf children.
    let content = label.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let container = NewChunk {
        id: id.clone(),
        source_id: source_id.to_string(),
        chunk_index: out.len() as i64,
        content: content.clone(),
        char_start: node.byte_start as i64,
        char_end: node.byte_end as i64,
        page_number: None,
        token_count: (content.len() / 4) as i64,
        parent_chunk_id: parent_chunk_id.clone(),
        unit_type: level.unit_type().to_string(),
        unit_label: Some(label.clone()),
        unit_path: Some(node.path.clone()),
        domain_metadata: None,
    };
    chunks::insert(pool, &container).await?;
    out.push(chunks::get_by_id(pool, &id).await?);

    let next_level = step_down(level);
    for child in &node.children {
        if matches!(child.kind, RawNodeKind::Leaf) {
            let leaf_id = uuid::Uuid::new_v4().to_string();
            let body = source_text.get(child.byte_start..child.byte_end).unwrap_or("").to_string();
            if body.is_empty() { continue; }
            let leaf = NewChunk {
                id: leaf_id.clone(),
                source_id: source_id.to_string(),
                chunk_index: out.len() as i64,
                content: body.clone(),
                char_start: child.byte_start as i64,
                char_end: child.byte_end as i64,
                page_number: None,
                token_count: (body.len() / 4) as i64,
                parent_chunk_id: Some(id.clone()),
                unit_type: "leaf".to_string(),
                unit_label: None,
                unit_path: Some(node.path.clone()),
                domain_metadata: None,
            };
            chunks::insert(pool, &leaf).await?;
            out.push(chunks::get_by_id(pool, &leaf_id).await?);
        } else {
            persist_walk(pool, source_id, source_text, child, next_level, Some(id.clone()), out, profile).await?;
        }
    }
    let _ = profile;  // citation rendering uses profile in a follow-up pass
    Ok(())
}

fn step_down(level: NodeLevel) -> NodeLevel {
    match level {
        NodeLevel::Document  => NodeLevel::Part,
        NodeLevel::Part      => NodeLevel::Chapter,
        NodeLevel::Chapter   => NodeLevel::Article,
        NodeLevel::Article   => NodeLevel::Clause,
        NodeLevel::Clause    => NodeLevel::SubClause,
        NodeLevel::SubClause => NodeLevel::SubClause,
    }
}
```

Add `async_recursion = "1"` to `[dependencies]` in `src-tauri/Cargo.toml` if not already present.

> Assumes `chunks::insert` and `chunks::get_by_id` exist in `db/repositories/chunks.rs`. Verify both with `grep -nE "pub async fn (insert|get_by_id)" src-tauri/src/db/repositories/chunks.rs` before writing; if absent, the implementer should add thin wrappers in that file as a sub-step. Similarly `graph::insert_node` / `graph::insert_edge` — `grep -nE "pub async fn insert_(node|edge)" src-tauri/src/db/repositories/graph.rs` to confirm.

- [ ] **Step 4: Implement `persist` in `structured_importer.rs`**

Append to `src-tauri/src/services/structured_importer.rs`:

```rust
use crate::services::doc_profiles::{self, DetectionSource};
use crate::services::structure_parser;
use crate::services::graph_builder;
use sqlx::SqlitePool;

pub struct PersistStats {
    pub chunks:      usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
}

/// Persist the imported tree to chunks + graph + sources columns. Caller must
/// have already deleted any prior chunks for this source. `source_text` is
/// the post-normalization text whose byte offsets the tree's spans index into.
pub async fn persist(pool: &SqlitePool, source_id: &str, source_text: &str, out: &ImportOutcome) -> anyhow::Result<PersistStats> {
    // 1. Persist chunks via structure_parser.
    let profile = doc_profiles::REGISTRY.iter()
        .find(|p| p.doc_type == out.doc_type)
        .expect("doc_type must come from REGISTRY");
    let chunks = structure_parser::persist_doc_tree(pool, source_id, source_text, &out.tree, profile).await?;

    // 2. Build + persist graph (contains edges + cross-refs detected by graph_builder).
    let (nodes, edges) = graph_builder::build_graph(&out.tree, &chunks);
    for n in &nodes { crate::db::repositories::graph::insert_node(pool, n).await?; }
    for e in &edges { crate::db::repositories::graph::insert_edge(pool, e).await?; }

    // 3. Update source columns.
    sqlx::query(
        "UPDATE sources SET doc_type = ?, doc_type_source = ?, structure_status = ?, structure_importer_version = 1 WHERE id = ?",
    )
    .bind(out.doc_type)
    .bind(out.doc_type_source.as_str())
    .bind(if out.structured { "structured" } else { "failed" })
    .bind(source_id)
    .execute(pool)
    .await?;

    Ok(PersistStats { chunks: chunks.len(), graph_nodes: nodes.len(), graph_edges: edges.len() })
}

```

The full signature is `persist(pool, source_id, source_text, out)` — `source_text` is the post-normalization text whose byte offsets the tree references. The test in Step 1 already uses the 4-arg form.

- [ ] **Step 5: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib services::structured_importer -- --nocapture
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/structure_parser.rs src-tauri/src/services/structured_importer.rs src-tauri/Cargo.toml
git commit -m "feat(005): persist tree → chunks + graph_nodes + graph_edges"
```

---

## Task 13: Wire `structured_importer` into `do_index`

**Files:**
- Modify: `src-tauri/src/services/indexer.rs:550-680` (the `do_index` function around the doc-type routing comment at line 593)

- [ ] **Step 1: Write the failing test**

Append a new integration test at `src-tauri/tests/structured_importer_integration.rs`:

```rust
//! End-to-end: feed a Rules-style PDF stub through do_index and assert
//! sources.doc_type='rules', graph_nodes > 0, chunks have unit_label='Rule N'.

use sqlx::sqlite::SqlitePoolOptions;
use std::fs::write;
use tempfile::tempdir;

#[tokio::test]
async fn do_index_routes_rules_pdf_through_structured_importer() {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();

    let dir = tempdir().unwrap();
    let pdf_path = dir.path().join("rules.txt");  // .txt path; the test_extract feature reads files unchanged
    write(&pdf_path, "Rule 1. Short title.\n\nbody body body.\n\nRule 2. Definitions.\n\nmore body.\n\nRule 3. Procedure.\n\nyet more body.").unwrap();

    // The real do_index requires HardwareTier, EmbedderState, TaskBudget. For
    // a self-contained integration test, call structured_importer::import +
    // persist directly to verify wiring stays green.
    let text = std::fs::read_to_string(&pdf_path).unwrap();
    sqlx::query("INSERT INTO sources (id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,indexing_status,created_at) VALUES ('src1',?,'rules.txt','txt',1,'h',0,'processing',0)")
        .bind(pdf_path.to_str().unwrap()).execute(&pool).await.unwrap();
    let out = echo_ai::services::structured_importer::import(&text, "rules.txt", None);
    echo_ai::services::structured_importer::persist(&pool, "src1", &text, &out).await.unwrap();

    let dt: String = sqlx::query_scalar("SELECT doc_type FROM sources WHERE id='src1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(dt, "rules");
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes").fetch_one(&pool).await.unwrap();
    assert!(n >= 3, "expected ≥3 graph_nodes, got {n}");
}
```

(Adjust crate name `echo_ai` to whatever `Cargo.toml`'s `[package].name` is.)

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --test structured_importer_integration -- --nocapture
```
Expected: FAIL until wiring lands.

- [ ] **Step 3: Patch `do_index` to call structured_importer first**

In `src-tauri/src/services/indexer.rs`, locate the comment at line ~590:

```rust
    // ── Doc-type routing (feature 003, T023) ────────────────────────────────
    // A constitution_ir.json routes to the structure-aware tree + graph path;
    // ordinary documents fall through to flat hierarchical chunking. Future
    // Act/Rule profiles will be detected here the same way.
    if let Some(ir) = try_load_constitution(path) {
        return index_constitution(pool, embedder, source_id, ir, app).await;
    }
```

Add the new branch **after** the constitution check, but **before** the existing `parse_result = tokio::task::spawn_blocking(...)` block. Insert immediately after the constitution `if` block closes:

```rust
    // ── Feature 005 — generic structured importer ────────────────────────────
    // If config enables structured importer AND parse returns text, try the
    // profile-driven pipeline. On `structured=true` we persist + return; on
    // `structured=false` we fall through to flat hierarchical chunking.
    if crate::config::structured_importer_enabled() {
        let path_buf = path.to_path_buf();
        let format_clone = format.clone();
        let tier_clone = tier.clone();
        let early_parse = tokio::task::spawn_blocking(move || -> Result<_, ParseError> {
            match &format_clone {
                FileFormat::Pdf => parsers::pdf::extract(&path_buf),
                FileFormat::Docx | FileFormat::Xlsx | FileFormat::Pptx => parsers::document::extract(&path_buf).map_err(|e| ParseError::Other(format!("document parse failed: {e}"))),
                FileFormat::Txt | FileFormat::Markdown | FileFormat::Code => parsers::text::extract(&path_buf).map_err(|e| ParseError::Other(format!("text parse failed: {e}"))),
                _ => Err(ParseError::Other("structured importer skips audio/video/image".into())),
            }
        }).await.map_err(|e| anyhow::anyhow!("parse thread panicked: {e}"))?;

        if let Ok(parsed_early) = early_parse {
            let stored_force = source_repo::get_doc_type_override(pool, source_id).await.ok().flatten();
            let outcome = structured_importer::import(&parsed_early.text, &file_name_for(path), stored_force.as_deref());
            if outcome.structured {
                chunk_repo::delete_by_source(pool, source_id).await?;
                structured_importer::persist(pool, source_id, &parsed_early.text, &outcome).await?;
                // Reuse Feature 004's quality persistence path
                source_repo::set_indexed_at(pool, source_id, parsed_early.extraction_method, parsed_early.quality_score).await?;
                emit_progress(app, source_id, 95, "embedding");
                // Embeddings + NER reuse the existing post-chunking path. Call the same
                // helper used after flat hierarchical chunking:
                let chunks = chunk_repo::get_by_source(pool, source_id).await?;
                if !embed_and_upsert(pool, &embedder, source_id, &chunks, app).await? {
                    return Ok(());
                }
                emit_progress(app, source_id, 100, "done");
                spawn_ner_for_source(pool, source_id, app);  // existing helper
                return Ok(());
            }
            // structured=false → fall through to flat hierarchical chunking below.
            // Mark structure_status='failed' so UI surfaces it.
            sqlx::query("UPDATE sources SET doc_type = ?, doc_type_source = ?, structure_status = 'failed', structure_importer_version = 1 WHERE id = ?")
                .bind(outcome.doc_type).bind(outcome.doc_type_source.as_str()).bind(source_id)
                .execute(pool).await.ok();
        }
    }
```

Add at the top of `src-tauri/src/services/indexer.rs`:

```rust
use crate::services::structured_importer;
```

Add the helper `source_repo::get_doc_type_override`:

```rust
// In src-tauri/src/db/repositories/sources.rs:
pub async fn get_doc_type_override(pool: &sqlx::SqlitePool, source_id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT doc_type FROM sources WHERE id = ? AND doc_type_source = 'user'",
    ).bind(source_id).fetch_optional(pool).await
}
```

Helper `file_name_for(path) -> String`: trivial — `path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string()`.

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --test structured_importer_integration -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/indexer.rs src-tauri/src/db/repositories/sources.rs src-tauri/tests/structured_importer_integration.rs
git commit -m "feat(005): wire structured_importer into do_index"
```

---

## Task 14: Config flag — `structured_importer_enabled`

**Files:**
- Modify: `src-tauri/src/config.rs` (or wherever existing flags live; create the module if absent)

- [ ] **Step 1: Write the failing test**

Append (or create) `src-tauri/src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enabled() {
        std::env::remove_var("ECHO_STRUCTURED_IMPORTER");
        assert!(structured_importer_enabled());
    }

    #[test]
    fn env_var_disables() {
        std::env::set_var("ECHO_STRUCTURED_IMPORTER", "0");
        assert!(!structured_importer_enabled());
        std::env::remove_var("ECHO_STRUCTURED_IMPORTER");
    }
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib config -- --nocapture
```
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
// src-tauri/src/config.rs (prepend, keep existing content if any)

/// Whether the generic structured importer (Feature 005) is active.
/// Disabled via `ECHO_STRUCTURED_IMPORTER=0`; otherwise on.
pub fn structured_importer_enabled() -> bool {
    std::env::var("ECHO_STRUCTURED_IMPORTER").map(|v| v != "0").unwrap_or(true)
}
```

Ensure `pub mod config;` exists in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib config -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/src/lib.rs
git commit -m "feat(005): structured_importer_enabled config flag"
```

---

## Task 15: Startup re-index hook

**Files:**
- Modify: `src-tauri/src/services/indexer.rs` (extend the Feature 004 startup section near `enqueue_quality_reextract`)
- Modify: `src-tauri/src/lib.rs` (call the new hook at startup; locate where `enqueue_quality_reextract` is called)

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/services/indexer.rs` (inside `mod reextract_tests`):

```rust
#[tokio::test]
async fn enqueue_structure_reextract_picks_up_old_version_sources() {
    let pool = fresh_pool().await;
    sqlx::query("INSERT INTO sources (id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,indexing_status,created_at,structure_importer_version) VALUES ('old','/a','a.pdf','pdf',1,'h',0,'indexed',0,0),('current','/b','b.pdf','pdf',1,'h',0,'indexed',0,1)")
        .execute(&pool).await.unwrap();
    let n = enqueue_structure_reextract(&pool).await.unwrap();
    assert_eq!(n, 1);
    let st: String = sqlx::query_scalar("SELECT indexing_status FROM sources WHERE id='old'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(st, "pending");
    let st2: String = sqlx::query_scalar("SELECT indexing_status FROM sources WHERE id='current'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(st2, "indexed");
}
```

- [ ] **Step 2: Run — confirm it fails**

```bash
cd src-tauri && cargo test --lib services::indexer::reextract_tests::enqueue_structure_reextract_picks_up_old_version_sources -- --nocapture
```
Expected: FAIL.

- [ ] **Step 3: Implement (extend the Feature 004 section)**

Append after `enqueue_quality_reextract` in `src-tauri/src/services/indexer.rs`:

```rust
/// Version of the structured importer. Bumping triggers re-index on next boot.
pub const STRUCTURED_IMPORTER_VERSION: i64 = 1;

/// Find indexed sources whose `structure_importer_version` is below the
/// current constant and flip them to `'pending'` for re-processing.
pub async fn enqueue_structure_reextract(pool: &sqlx::SqlitePool) -> sqlx::Result<usize> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM sources WHERE indexing_status = 'indexed' AND structure_importer_version < ?",
    )
    .bind(STRUCTURED_IMPORTER_VERSION)
    .fetch_all(pool)
    .await?;
    for id in &ids {
        sqlx::query("UPDATE sources SET indexing_status = 'pending' WHERE id = ?")
            .bind(id).execute(pool).await?;
    }
    Ok(ids.len())
}
```

In `src-tauri/src/lib.rs`, locate the existing call to `enqueue_quality_reextract` (search for that string). Add a call right after it:

```rust
// Feature 005 — re-process anything still on the pre-structured-importer schema.
if let Ok(n) = services::indexer::enqueue_structure_reextract(&pool).await {
    if n > 0 { log::info!("Feature 005: enqueued {n} sources for structured re-index"); }
}
```

- [ ] **Step 4: Run — confirm it passes**

```bash
cd src-tauri && cargo test --lib services::indexer::reextract_tests -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/indexer.rs src-tauri/src/lib.rs
git commit -m "feat(005): startup hook re-indexes sources at old importer version"
```

---

## Task 16: Layer-2 IR golden snapshot tests

**Files:**
- Create: `src-tauri/tests/structured_importer_snapshots.rs`
- Create: `src-tauri/tests/fixtures/structured_indexing/*.txt` (one per Sindh PDF — extracted text already on disk)
- Create: `src-tauri/tests/fixtures/structured_indexing/*.snapshot.txt` (committed expected outputs)

- [ ] **Step 1: Generate fixture text** (one-time)

```bash
mkdir -p src-tauri/tests/fixtures/structured_indexing
for f in "constitution.pdf" "efficiency and disciplane rules.pdf" "REMOVAL FROM SERVICE SPECIAL POWERS SINDH.pdf" "appointment promotion and transfer rules.pdf" "Govt. of Sindh Manual of Secretariat instruction 2010.pdf"; do
  name=$(echo "$f" | tr -dc 'A-Za-z0-9_-')
  pdftotext "/home/riaz/Desktop/Sindh Govt Documents/$f" "src-tauri/tests/fixtures/structured_indexing/${name}.txt"
done
```

- [ ] **Step 2: Write the snapshot test**

Create `src-tauri/tests/structured_importer_snapshots.rs`:

```rust
//! Layer-2 golden snapshots — every fixture's tree shape is locked in. Diffs
//! produce a .actual file the developer can promote to the new baseline.

use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf { PathBuf::from("tests/fixtures/structured_indexing") }

fn render_snapshot(tree: &echo_ai::services::heading_detector::tree::DocNode, depth: usize) -> String {
    use echo_ai::services::heading_detector::tree::NodeKind;
    let mut s = String::new();
    let kind = match tree.kind { NodeKind::Document => "D", NodeKind::Section => "S", NodeKind::Leaf => "L" };
    s.push_str(&format!("{:indent$}{} {} [{}..{}]\n",
        "", kind, tree.label.as_deref().unwrap_or(""), tree.byte_start, tree.byte_end, indent = depth*2));
    for c in &tree.children { s.push_str(&render_snapshot(c, depth+1)); }
    s
}

#[test]
fn snapshots_match() {
    let mut failed: Vec<String> = Vec::new();
    for entry in fs::read_dir(fixtures_dir()).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) != Some("txt") { continue; }
        if p.to_string_lossy().contains(".snapshot.") { continue; }

        let text = fs::read_to_string(&p).unwrap();
        let title = p.file_stem().unwrap().to_string_lossy().to_string();
        let out = echo_ai::services::structured_importer::import(&text, &title, None);
        let actual = format!("doc_type: {}\nstructured: {}\n\n{}",
            out.doc_type, out.structured, render_snapshot(&out.tree, 0));

        let snap = p.with_file_name(format!("{title}.snapshot.txt"));
        if !snap.exists() {
            fs::write(&snap, &actual).unwrap();
            failed.push(format!("created baseline: {snap:?}"));
            continue;
        }
        let expected = fs::read_to_string(&snap).unwrap();
        if actual != expected {
            let actual_path = p.with_file_name(format!("{title}.actual.txt"));
            fs::write(&actual_path, &actual).unwrap();
            failed.push(format!("diff in {title} — see {actual_path:?}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
```

- [ ] **Step 3: Run — establishes baselines on first pass**

```bash
cd src-tauri && cargo test --test structured_importer_snapshots -- --nocapture
```
Expected: first run creates baselines and reports them. Re-run; expected: PASS.

- [ ] **Step 4: Commit fixtures + baselines + test**

```bash
git add src-tauri/tests/structured_importer_snapshots.rs src-tauri/tests/fixtures/structured_indexing/
git commit -m "test(005): IR golden snapshots for Sindh corpus + constitution"
```

---

## Task 17: Layer-3 acceptance gate

**Files:**
- Create: `src-tauri/tests/structured_importer_acceptance.rs`

- [ ] **Step 1: Write the test**

Create `src-tauri/tests/structured_importer_acceptance.rs`:

```rust
//! Definition of done for Feature 005 — runs without an LLM (regex-only).

use std::fs;
use std::path::PathBuf;

fn read_fixture(name: &str) -> String {
    let p = PathBuf::from(format!("tests/fixtures/structured_indexing/{name}.txt"));
    fs::read_to_string(&p).unwrap_or_else(|_| panic!("missing fixture {p:?}"))
}

#[test]
fn every_sindh_pdf_gets_non_generic_doc_type() {
    let cases = [
        ("appointment_promotion_and_transfer_rulespdf",  "rules"),
        ("efficiency_and_disciplane_rulespdf",           "rules"),
        ("REMOVAL_FROM_SERVICE_SPECIAL_POWERS_SINDHpdf", "act"),
        ("Govt_of_Sindh_Manual_of_Secretariat_instruction_2010pdf", "manual"),
    ];
    for (file, expected_type) in cases {
        let text = read_fixture(file);
        let out = echo_ai::services::structured_importer::import(&text, file, None);
        assert_eq!(out.doc_type, expected_type, "wrong doc_type for {file}: got {}", out.doc_type);
        assert!(out.structured, "{file} should be structured but isn't");
    }
}

#[test]
fn generic_doc_still_produces_coherent_tree() {
    let text = "1. Introduction\n\nbody body\n\n1.1 Background\n\nmore\n\n2. Method\n\nbody";
    let out = echo_ai::services::structured_importer::import(text, "memo.txt", None);
    assert_eq!(out.doc_type, "generic");
    assert!(!out.tree.children.is_empty(), "generic should still produce some structure");
}

#[test]
fn rules_pdf_produces_graph_nodes_when_persisted() {
    use sqlx::sqlite::SqlitePoolOptions;
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO sources (id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,indexing_status,created_at) VALUES ('s','/x','x.pdf','pdf',1,'h',0,'processing',0)")
            .execute(&pool).await.unwrap();
        let text = read_fixture("efficiency_and_disciplane_rulespdf");
        let out  = echo_ai::services::structured_importer::import(&text, "x.pdf", None);
        echo_ai::services::structured_importer::persist(&pool, "s", &text, &out).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes").fetch_one(&pool).await.unwrap();
        assert!(n >= 10, "expected ≥10 graph_nodes for Efficiency & Discipline Rules, got {n}");
    });
}
```

(Adjust fixture names to whatever Task 16 generated; use `ls src-tauri/tests/fixtures/structured_indexing/` to confirm.)

- [ ] **Step 2: Run — confirm it passes**

```bash
cd src-tauri && cargo test --test structured_importer_acceptance -- --nocapture
```
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/structured_importer_acceptance.rs
git commit -m "test(005): acceptance gate — non-generic doc_type + graph_nodes"
```

---

## Task 18: Final integration check + CHANGELOG_DEV

**Files:**
- Modify: `CHANGELOG_DEV.md`
- Run end-to-end test

- [ ] **Step 1: Run the full test suite**

```bash
cd src-tauri && cargo test 2>&1 | tail -30
```
Expected: all green, including:
- `doc_profiles::` unit tests
- `text_normalizer::tests`
- `structured_importer::tests` + `persist_tests`
- `heading_detector::regex_pass::tests` + `heading_detector::tree::tests`
- `structured_importer_integration`
- `structured_importer_snapshots`
- `structured_importer_acceptance`

- [ ] **Step 2: Manually re-index the Sindh corpus**

```bash
# Bump version to trigger re-index, launch app.
cd "/media/riaz/New Volume/echo-AI" && npm run tauri dev
```

In another terminal, after re-index completes:

```bash
DB=~/.local/share/com.echoai.app/echo.db
sqlite3 -header -column "$DB" "SELECT file_name, doc_type, doc_type_source, structure_status FROM sources WHERE format='pdf';"
sqlite3 "$DB" "SELECT 'graph_nodes:', COUNT(*) FROM graph_nodes UNION ALL SELECT 'graph_edges:', COUNT(*) FROM graph_edges;"
```
Expected: every Sindh PDF has `doc_type IN ('act','rules','manual')` and `structure_status='structured'`. `graph_nodes` count jumps from 1488 to >2000.

- [ ] **Step 3: Append CHANGELOG_DEV entry**

Append to `CHANGELOG_DEV.md`:

```markdown
## 2026-MM-DD — Feature 005 — Generic Structured Importer (closed)

Extended Feature 003's DocProfile abstraction with three new profiles
(act/rules/manual) plus a generic fallback so every indexed document produces
clean hierarchy + graph_nodes + graph_edges, not only the Pakistan Constitution.

- migration 017 (doc_type, doc_type_source, structure_status, structure_importer_version)
- services/text_normalizer.rs — OCR cleanup (brace/paren, smart quotes, hyphenation, ~5 misread dictionary)
- services/structured_importer.rs — orchestrator (extract → normalize → detect → headings → tree → persist)
- 4 new profiles: act, rules, manual, generic; detection + heading patterns; ≥5.0 confidence threshold per legal profile
- indexer.rs wired to call structured_importer before flat hierarchical path, behind ECHO_STRUCTURED_IMPORTER flag (default on)
- Startup hook re-indexes all sources at STRUCTURED_IMPORTER_VERSION < 1
- Tests: ~25 unit + 6 IR snapshots + 3 acceptance assertions
- Sindh corpus pre→post: graph_nodes 1488 → <X>, graph_edges 1626 → <Y>
```

(Fill in `<X>` `<Y>` from the manual re-index numbers in Step 2.)

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG_DEV.md
git commit -m "docs(005): CHANGELOG entry — feature 005 closed"
```

---

## Self-review checklist

- **Spec coverage**:
  - Pipeline 7 steps → Tasks 7 (normalize) + 8 (detect) + 9–10 (headings) + 11–12 (orchestrate + persist) + 13 (wire) ✓
  - Migration 017 (4 columns) → Task 1 ✓
  - 4 profiles + extended DocProfile → Tasks 2–6 ✓
  - Auto-reindex hook → Task 15 ✓
  - Failure handling (`structure_status='failed'` fallback to flat) → Task 13 ✓
  - 3-layer tests → Tasks 3–10 (Layer 1) + 16 (Layer 2) + 17 (Layer 3) ✓
  - Feature flag → Task 14 ✓
- **Placeholders**: none — every step has the actual code.
- **Type consistency**: `import()` returns `ImportOutcome`; `persist()` takes `(pool, source_id, source_text, &ImportOutcome) → PersistStats`; consistent across Tasks 11–13. `DocProfile.detection_patterns: &[DetectionPattern]` consistent across Tasks 2–6 and 8. `NodeLevel` re-exported via `structure_parser` and used in profiles + heading_detector.
- **Open risks called out in spec** (regex iteration, NER drift, OCR over-correction) — addressed by Tasks 16 (golden tests catch regressions), Task 14 (flag for safe rollout), and the small dictionary in Task 7.
