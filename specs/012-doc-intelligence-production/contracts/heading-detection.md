# Contract — Heading Detection (US3)

**Module:** `src-tauri/src/services/heading_detector/regex_pass.rs` (extend) +
`structure_parser.rs` (split action). Builds on existing
`scan_with_profile`, `leading_provision_number`, `drop_out_of_sequence_articles`,
`looks_like_year`, `detect_label_mismatches`.

## Core
```rust
/// Existing: regex headings per profile + year-rule guard + out-of-sequence drop.
pub fn scan_with_profile(text: &str, profile: &DocProfile) -> Vec<LevelledHeading>;

/// NEW: numbering-sequence prior — recover a missed heading when the expected
/// next ordinal appears mid-block (degraded `N. Title - body`); reject restarts.
fn apply_sequence_prior(hits: &mut Vec<LevelledHeading>, text: &str);

/// NEW: boundary-stop — do not absorb a block whose leading token is a plausible
/// next-in-sequence provision number; start a new node.

/// PROMOTE detect_label_mismatches from warning to a split action in tree-build:
/// content begins with a provision number != its label => split (or flag).
```
`LevelledHeading` gains a `confidence: f32`; low confidence ⇒ `label_status='unresolved'` at persist, never a silent mislabel.

## Invariants
- Generic only — sequence prior uses ordinal expectation, **no keyword lists**.
- Year-rule guard retained (bare 1800–2099 never a Rule/Section ordinal).
- A real ordinal (260, 25A) still parses; form-field restart ("1." after rule 158) rejected.

## Acceptance
- Golden headings incl. degraded `N. Title - body` and plain `N. Title` detected.
- Zero label≠content on golden provisions (split action fires).
- `cargo test --lib heading_detector` + `structure_parser` green (sequence-recover, boundary-stop, restart-reject, split-on-mismatch).
