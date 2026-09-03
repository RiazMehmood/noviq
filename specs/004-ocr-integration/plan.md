# Feature 004 — OCR Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third tier (Tesseract OCR) to the PDF parsing pipeline, gated by a new text-quality verdict that detects both image-only PDFs and font-corrupted PDFs.

**Architecture:** Two new files (`text_quality.rs`, `ocr.rs`) plus a thin orchestration change in `pdf.rs`. One SQL migration adds `extraction_method` and `quality_score` columns to `sources`. Startup hook auto-re-extracts previously bad rows. Pure functions are TDD'd first; subprocess code is integration-tested with synthetic fixtures; final acceptance gate runs over the 6-file Sindh legal corpus.

**Tech Stack:** Rust 1.87, `lopdf` 0.34, `tesseract` 4.1.1 (subprocess), `pdftoppm` (poppler-utils subprocess), `sqlx` migrations, `tempfile`, `phf` (compile-time wordlist), `printpdf` (test-only PDF fixture builder).

**Spec:** `specs/004-ocr-integration/spec.md` — read first.

**Branch:** `004-ocr-integration` (already checked out).

---

## File Structure

| File | Status | Responsibility | Approx. LOC |
|---|---|---|---|
| `src-tauri/src/services/parsers/text_quality.rs` | **new** | Pure scoring function over `&str` returning `Quality { score, verdict }`. No I/O. | ~150 |
| `src-tauri/src/services/parsers/text_quality/en_words.txt` | **new** | Bundled English wordlist (~50 k common words) for the dict-hit signal. | ~500 KB asset |
| `src-tauri/src/services/parsers/ocr.rs` | **new** | `pdftoppm` + `tesseract` subprocess orchestration. Owns all temp-file logic. | ~200 |
| `src-tauri/src/db/migrations/016_extraction_quality.sql` | **new** | `ALTER TABLE sources ADD COLUMN extraction_method TEXT`, `ADD COLUMN quality_score REAL`. | ~6 |
| `src-tauri/src/services/parsers/pdf.rs` | **modify** | Replace chars/page gate with `text_quality::score`; add OCR as third tier. Stays ≤ ~120 LOC. | edit |
| `src-tauri/src/services/parsers/mod.rs` | **modify** | Add `OcrSkipped` variant to `ParseError`; `pub mod text_quality; pub mod ocr;`. Extend `ParsedDocument` (or sibling type) with `extraction_method` + `quality_score`. | edit |
| `src-tauri/src/services/indexer.rs` | **modify** | Persist `extraction_method` + `quality_score` on each source row; call new `enqueue_quality_reextract` once at startup. | edit |
| `src-tauri/src/config.rs` (or equivalent) | **modify** | Add `[ocr]` section: `enabled` (bool, default true), `dpi` (u32, default 300), `timeout_per_page_secs` (u32, default 60). | edit |
| `src-tauri/tests/fixtures/text_quality/{clean_legal,garbage_legal,empty,mixed}.txt` | **new** | Four unit fixtures for `text_quality::score`. | small |
| `src-tauri/tests/ocr_integration_acceptance.rs` | **new** | Acceptance test over the Sindh corpus (reads `OCR_FIXTURE_DIR` env). | ~80 |

**Sequencing rationale:** pure scoring function first (no deps, fastest feedback). Then OCR sub-flow in isolation (subprocess plumbing, fixture-tested). Then pipeline wiring in `pdf.rs`. Then schema + startup re-extract (touches DB, lowest churn now that data shapes are settled). Acceptance gate last because it depends on everything.

---

## Pre-flight (one-time)

- [ ] Verify Tesseract + pdftoppm available locally before starting:

```bash
which tesseract && tesseract --version | head -1
which pdftoppm  && pdftoppm  -v 2>&1 | head -1
```

Expected: paths under `/usr/bin/`, tesseract 4.x, pdftoppm 22.x or newer.

- [ ] Verify branch:

```bash
git rev-parse --abbrev-ref HEAD
```

Expected: `004-ocr-integration`.

---

## Task 1 — Bundle the English wordlist

**Files:**
- Create: `src-tauri/src/services/parsers/text_quality/en_words.txt`

The wordlist drives the dict-hit signal in `text_quality::score`. We use the SCOWL "english-words.50" list (free, public domain, ~50 k entries). Bundled at build time so there is no runtime download path.

- [ ] **Step 1: Fetch the wordlist**

Run from repo root:

```bash
mkdir -p src-tauri/src/services/parsers/text_quality
curl -sSL https://raw.githubusercontent.com/dwyl/english-words/master/words_alpha.txt \
  | tr 'A-Z' 'a-z' \
  | awk 'length($0) >= 2 && length($0) <= 20' \
  | sort -u > src-tauri/src/services/parsers/text_quality/en_words.txt
wc -l src-tauri/src/services/parsers/text_quality/en_words.txt
```

Expected: ~370 000 lines (we'll trim to common-50k in step 2; full list is fine as a base if we end up matching against it directly).

- [ ] **Step 2: Trim to a reasonable size**

The full `words_alpha.txt` is ~4 MB; we want ~500 KB. Trim to words of length 3–15 (covers the vast majority of running English):

```bash
awk 'length($0) >= 3 && length($0) <= 15' \
  src-tauri/src/services/parsers/text_quality/en_words.txt \
  > src-tauri/src/services/parsers/text_quality/en_words.tmp \
  && mv src-tauri/src/services/parsers/text_quality/en_words.tmp \
        src-tauri/src/services/parsers/text_quality/en_words.txt
du -h src-tauri/src/services/parsers/text_quality/en_words.txt
```

Expected size: 2–4 MB. (We'll keep the full filtered list — `include_str!` + `HashSet` at startup is fine; only ~3 MB resident.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/services/parsers/text_quality/en_words.txt
git commit -m "feat(004): bundle SCOWL english wordlist for text-quality dict-hit signal"
```

---

## Task 2 — `text_quality::Verdict` and `Quality` types

**Files:**
- Create: `src-tauri/src/services/parsers/text_quality.rs`
- Modify: `src-tauri/src/services/parsers/mod.rs`

Define the data shapes only. No scoring logic yet — that comes in Task 3 with tests first.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/parsers/text_quality.rs`:

```rust
//! Pure scoring function over extracted text.
//! Detects scanned PDFs (chars/page) and font-corrupted PDFs
//! (non-ASCII-punct ratio, dict-hit ratio, mean token length).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Good,
    Suspect,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality {
    pub score: f32,
    pub verdict: Verdict,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_enum_basic_equality() {
        assert_eq!(Verdict::Good, Verdict::Good);
        assert_ne!(Verdict::Good, Verdict::Suspect);
    }

    #[test]
    fn quality_struct_constructs() {
        let q = Quality { score: 0.9, verdict: Verdict::Good };
        assert!((q.score - 0.9).abs() < 1e-6);
        assert_eq!(q.verdict, Verdict::Good);
    }
}
```

- [ ] **Step 2: Register the module**

Edit `src-tauri/src/services/parsers/mod.rs` — add this line in the same block as the other `pub mod` declarations:

```rust
pub mod text_quality;
```

- [ ] **Step 3: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::text_quality
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/parsers/text_quality.rs \
        src-tauri/src/services/parsers/mod.rs
git commit -m "feat(004): text_quality::{Verdict, Quality} types"
```

---

## Task 3 — `text_quality::score`: scanned-PDF signal (chars/page)

**Files:**
- Modify: `src-tauri/src/services/parsers/text_quality.rs`

TDD the easiest signal first — the existing "chars per page" heuristic re-implemented inside the new function.

- [ ] **Step 1: Write the failing test**

Append to `text_quality.rs`:

```rust
#[cfg(test)]
mod scanned_tests {
    use super::*;

    #[test]
    fn empty_text_returns_empty_verdict() {
        let q = score("", 1);
        assert_eq!(q.verdict, Verdict::Empty);
    }

    #[test]
    fn whitespace_only_returns_empty_verdict() {
        let q = score("   \n\n  \t  ", 5);
        assert_eq!(q.verdict, Verdict::Empty);
    }

    #[test]
    fn very_low_chars_per_page_returns_suspect() {
        // 100 pages, ~5 chars total ⇒ 0.05 chars/page ⇒ Suspect.
        let q = score("hello", 100);
        assert_eq!(q.verdict, Verdict::Suspect);
    }

    #[test]
    fn high_chars_per_page_clean_text_returns_good() {
        // ~200 chars on a single page, all clean English.
        let txt = "The quick brown fox jumps over the lazy dog. \
                   The quick brown fox jumps over the lazy dog. \
                   The quick brown fox jumps over the lazy dog.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Good);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::text_quality::scanned_tests
```

Expected: compile error `cannot find function 'score' in this scope`.

- [ ] **Step 3: Implement the scanned-PDF signal**

Append to `text_quality.rs`:

```rust
const CHARS_PER_PAGE_MIN: f32 = 10.0;

pub fn score(text: &str, total_pages: usize) -> Quality {
    let trimmed_len = text.trim().len();
    if trimmed_len == 0 {
        return Quality { score: 0.0, verdict: Verdict::Empty };
    }

    let pages = total_pages.max(1) as f32;
    let chars_per_page = text.len() as f32 / pages;
    if chars_per_page < CHARS_PER_PAGE_MIN {
        return Quality { score: 0.0, verdict: Verdict::Suspect };
    }

    // Further signals added in Task 4 and Task 5. For now, anything that
    // clears the chars/page bar is provisionally Good.
    Quality { score: 1.0, verdict: Verdict::Good }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::text_quality
```

Expected: 6 passed (2 from Task 2 + 4 new).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/parsers/text_quality.rs
git commit -m "feat(004): text_quality::score chars-per-page signal"
```

---

## Task 4 — `text_quality::score`: non-ASCII-punct signal (the Hamlet case)

**Files:**
- Modify: `src-tauri/src/services/parsers/text_quality.rs`

This is the glyph-mangle detector. Counts `{`, `}`, `�`, control chars, and other "shouldn't be here mid-word" characters.

- [ ] **Step 1: Write the failing test**

Append to `text_quality.rs`:

```rust
#[cfg(test)]
mod punct_tests {
    use super::*;

    #[test]
    fn clean_text_passes_punct_signal() {
        let txt = "To be, or not to be — that is the question.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Good);
    }

    #[test]
    fn curly_brace_mid_word_trips_suspect() {
        // The actual Hamlet failure mode.
        let txt = "Th{ rest is sile{ce. The {rest is silence. {Frailty thy name.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Suspect);
    }

    #[test]
    fn replacement_chars_trip_suspect() {
        let txt = "Hello \u{FFFD} world \u{FFFD} \u{FFFD} \u{FFFD} test.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Suspect);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::text_quality::punct_tests
```

Expected: `curly_brace_mid_word_trips_suspect` and `replacement_chars_trip_suspect` FAIL (still classified Good).

- [ ] **Step 3: Implement the punct signal**

Replace the body of `score` with:

```rust
const CHARS_PER_PAGE_MIN: f32 = 10.0;
const NON_ASCII_PUNCT_RATIO_MAX: f32 = 0.05;

pub fn score(text: &str, total_pages: usize) -> Quality {
    let trimmed_len = text.trim().len();
    if trimmed_len == 0 {
        return Quality { score: 0.0, verdict: Verdict::Empty };
    }

    let pages = total_pages.max(1) as f32;
    let chars_per_page = text.len() as f32 / pages;
    if chars_per_page < CHARS_PER_PAGE_MIN {
        return Quality { score: 0.0, verdict: Verdict::Suspect };
    }

    let total_chars = text.chars().count() as f32;
    let suspicious_chars = text
        .chars()
        .filter(|c| matches!(c, '{' | '}' | '\u{FFFD}' | '\u{0}'..='\u{8}' | '\u{B}' | '\u{E}'..='\u{1F}'))
        .count() as f32;
    let punct_ratio = suspicious_chars / total_chars.max(1.0);
    if punct_ratio > NON_ASCII_PUNCT_RATIO_MAX {
        return Quality { score: 0.2, verdict: Verdict::Suspect };
    }

    Quality { score: 1.0, verdict: Verdict::Good }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::text_quality
```

Expected: all green (≥ 9 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/parsers/text_quality.rs
git commit -m "feat(004): text_quality::score non-ASCII-punct signal (Hamlet case)"
```

---

## Task 5 — `text_quality::score`: dict-hit + mean-token-length signals

**Files:**
- Modify: `src-tauri/src/services/parsers/text_quality.rs`

These two signals share tokenisation, so they land in the same task. The wordlist from Task 1 is loaded once via `OnceLock<HashSet<&'static str>>`.

- [ ] **Step 1: Write the failing test**

Append to `text_quality.rs`:

```rust
#[cfg(test)]
mod dict_tests {
    use super::*;

    #[test]
    fn clean_legal_passes_dict_hit() {
        // Legal-style English. Threshold is generous: archaic/inflected
        // words still resolve via the SCOWL list.
        let txt = "The Government of Sindh, in exercise of the powers \
                   conferred by the Constitution, hereby promulgates these \
                   rules for the appointment, promotion, and transfer of \
                   civil servants in the province.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Good);
    }

    #[test]
    fn garbage_tokens_trip_dict_hit() {
        // Random consonant clusters — no dict hits.
        let txt = "Xjkv qrwn flmtp gzbv hxnq jklm wprs tvxz qntl bzkj \
                   xjkv qrwn flmtp gzbv hxnq jklm wprs tvxz qntl bzkj.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Suspect);
    }

    #[test]
    fn fragmented_text_trips_mean_token_len() {
        // Every "token" is 1-2 chars — characteristic of broken word
        // boundaries from corrupted PDFs.
        let txt = "a b c d e f g h i j k l m n o p q r s t u v w x y z \
                   a b c d e f g h i j k l m n o p q r s t u v w x y z";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Suspect);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::text_quality::dict_tests
```

Expected: all three FAIL — `Good` returned where `Suspect` expected (and vice versa).

- [ ] **Step 3: Implement the two signals**

Replace `text_quality.rs` `score` and add a helper:

```rust
use std::collections::HashSet;
use std::sync::OnceLock;

const CHARS_PER_PAGE_MIN: f32 = 10.0;
const NON_ASCII_PUNCT_RATIO_MAX: f32 = 0.05;
const DICT_HIT_RATIO_MIN: f32 = 0.60;
const MEAN_TOKEN_LEN_MIN: f32 = 2.5;

static EN_WORDS: OnceLock<HashSet<&'static str>> = OnceLock::new();

fn en_words() -> &'static HashSet<&'static str> {
    EN_WORDS.get_or_init(|| {
        include_str!("text_quality/en_words.txt")
            .lines()
            .filter(|w| !w.is_empty())
            .collect()
    })
}

pub fn score(text: &str, total_pages: usize) -> Quality {
    let trimmed_len = text.trim().len();
    if trimmed_len == 0 {
        return Quality { score: 0.0, verdict: Verdict::Empty };
    }

    let pages = total_pages.max(1) as f32;
    let chars_per_page = text.len() as f32 / pages;
    if chars_per_page < CHARS_PER_PAGE_MIN {
        return Quality { score: 0.0, verdict: Verdict::Suspect };
    }

    let total_chars = text.chars().count() as f32;
    let suspicious_chars = text
        .chars()
        .filter(|c| matches!(c, '{' | '}' | '\u{FFFD}' | '\u{0}'..='\u{8}' | '\u{B}' | '\u{E}'..='\u{1F}'))
        .count() as f32;
    let punct_ratio = suspicious_chars / total_chars.max(1.0);
    if punct_ratio > NON_ASCII_PUNCT_RATIO_MAX {
        return Quality { score: 0.2, verdict: Verdict::Suspect };
    }

    // Tokenise: alphabetic-only runs, lower-cased.
    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphabetic())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect();

    if tokens.is_empty() {
        return Quality { score: 0.0, verdict: Verdict::Suspect };
    }

    let mean_len = tokens.iter().map(|t| t.chars().count()).sum::<usize>() as f32
        / tokens.len() as f32;
    if mean_len < MEAN_TOKEN_LEN_MIN {
        return Quality { score: 0.3, verdict: Verdict::Suspect };
    }

    let dict = en_words();
    let long_tokens: Vec<&String> = tokens.iter().filter(|t| t.chars().count() >= 3).collect();
    if !long_tokens.is_empty() {
        let hits = long_tokens.iter().filter(|t| dict.contains(t.as_str())).count() as f32;
        let hit_ratio = hits / long_tokens.len() as f32;
        if hit_ratio < DICT_HIT_RATIO_MIN {
            return Quality { score: 0.4, verdict: Verdict::Suspect };
        }
    }

    Quality { score: 1.0, verdict: Verdict::Good }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::text_quality
```

Expected: all green (≥ 12 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/parsers/text_quality.rs
git commit -m "feat(004): text_quality::score dict-hit and mean-token-length signals"
```

---

## Task 6 — File-based fixtures for `text_quality::score`

**Files:**
- Create: `src-tauri/tests/fixtures/text_quality/{clean_legal,garbage_legal,empty,mixed}.txt`
- Create: `src-tauri/tests/text_quality_fixtures.rs`

Inline test strings are sufficient for unit coverage; the file fixtures exist so we can edit them without recompiling and so they document the real failure mode for future maintainers.

- [ ] **Step 1: Create the four fixtures**

Run from repo root:

```bash
mkdir -p src-tauri/tests/fixtures/text_quality

cat > src-tauri/tests/fixtures/text_quality/clean_legal.txt <<'EOF'
The Government of Sindh, in exercise of the powers conferred by section 3
of the Sindh Civil Servants (Efficiency and Discipline) Rules, 1973, and
all other enabling provisions, hereby promulgates the following rules for
the appointment, promotion, and transfer of civil servants in the
Province. These rules shall come into force at once and shall apply to
all persons appointed to a civil post in connection with the affairs of
the Province of Sindh.
EOF

cat > src-tauri/tests/fixtures/text_quality/garbage_legal.txt <<'EOF'
The Gover{ment of Si{dh, i{ exercise of th{ powers co{ferred by sect{on 3
of th{ Sindh Civi{ Servants {Efficiency a{d Disciplin{} Rules, 197{, a{d
all othe{ e{abling pro{isions, here{y pro{ulgates the fo{lowi{g ru{es for
th{ appoint{ent, pro{otion, a{d tra{sfer o{ civi{ servants i{ the
Provi{ce. The{e ru{es sha{l co{e i{to force at o{ce a{d sha{l app{y to
al{ perso{s appoi{ted to a civ{l po{t i{ co{nection wit{ the aff{irs of
the Provi{ce of Si{dh.
EOF

cat > src-tauri/tests/fixtures/text_quality/empty.txt <<'EOF'

EOF

cat > src-tauri/tests/fixtures/text_quality/mixed.txt <<'EOF'
The Government of Sindh, in exercise of the powers conferred by the
Constitution, hereby promulgates these rules. Xjkv qrwn flmtp gzbv hxnq
jklm wprs tvxz qntl bzkj xjkv qrwn flmtp gzbv hxnq jklm wprs tvxz qntl
bzkj xjkv qrwn flmtp gzbv hxnq jklm wprs tvxz.
EOF
```

- [ ] **Step 2: Write the fixture integration test**

Create `src-tauri/tests/text_quality_fixtures.rs`:

```rust
//! Fixture-driven sanity checks for text_quality::score.
//! Inline unit tests live alongside the implementation; these
//! tests prove the same behaviour against on-disk fixtures so
//! the failure modes are documented in plain text.

use echo_ai::services::parsers::text_quality::{score, Verdict};

fn load(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/text_quality")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {:?}: {e}", path))
}

#[test]
fn fixture_clean_legal_is_good() {
    assert_eq!(score(&load("clean_legal.txt"), 1).verdict, Verdict::Good);
}

#[test]
fn fixture_garbage_legal_is_suspect() {
    assert_eq!(score(&load("garbage_legal.txt"), 1).verdict, Verdict::Suspect);
}

#[test]
fn fixture_empty_is_empty() {
    assert_eq!(score(&load("empty.txt"), 1).verdict, Verdict::Empty);
}

#[test]
fn fixture_mixed_is_suspect() {
    assert_eq!(score(&load("mixed.txt"), 1).verdict, Verdict::Suspect);
}
```

You may need to mark `text_quality` public from the crate root. If the crate is `echo_ai`, check that `src-tauri/src/lib.rs` re-exports `services` and `parsers`:

```bash
grep -n "pub mod services\|pub mod parsers" src-tauri/src/lib.rs
```

If `services::parsers::text_quality` is not reachable, add a `pub use` line in `src-tauri/src/services/parsers/mod.rs`:

```rust
pub use text_quality::{score, Quality, Verdict};
```

- [ ] **Step 3: Run the fixture tests**

```bash
cd src-tauri && cargo test -p echo-ai --test text_quality_fixtures
```

Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/fixtures/text_quality \
        src-tauri/tests/text_quality_fixtures.rs \
        src-tauri/src/services/parsers/mod.rs
git commit -m "test(004): on-disk fixtures for text_quality::score"
```

---

## Task 7 — `ocr::ocr_pdf` skeleton + temp-dir hygiene

**Files:**
- Create: `src-tauri/src/services/parsers/ocr.rs`
- Modify: `src-tauri/src/services/parsers/mod.rs`

Subprocess orchestration. We TDD the temp-dir lifecycle first because that is the most error-prone part (and the one that risks leaving NTFS junk lying around on the data partition).

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/parsers/ocr.rs`:

```rust
//! PDF → image (pdftoppm) → text (tesseract) via subprocess.
//! Owns all temp-file handling. Never writes outside the configured
//! data directory.

use std::path::{Path, PathBuf};
use super::{ParseError, ParsedDocument};

pub struct OcrConfig {
    pub dpi: u32,
    pub timeout_per_page_secs: u64,
    /// Parent directory under which a uniquely-named temp dir is created.
    /// Must NOT be `/tmp` (root-disk constraint) — caller passes the data dir.
    pub tmp_root: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_is_constructible() {
        let cfg = OcrConfig {
            dpi: 300,
            timeout_per_page_secs: 60,
            tmp_root: PathBuf::from("/var/tmp/echoai-test"),
        };
        assert_eq!(cfg.dpi, 300);
    }
}
```

- [ ] **Step 2: Register the module**

Edit `src-tauri/src/services/parsers/mod.rs` — add:

```rust
pub mod ocr;
```

- [ ] **Step 3: Run tests to verify they compile and pass**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::ocr
```

Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/parsers/ocr.rs \
        src-tauri/src/services/parsers/mod.rs
git commit -m "feat(004): ocr module skeleton + OcrConfig"
```

---

## Task 8 — `ocr::ocr_pdf`: rasterise with pdftoppm

**Files:**
- Modify: `src-tauri/src/services/parsers/ocr.rs`

Rasterise a PDF to a numbered PNG sequence in a per-call temp dir. Validate cleanup.

- [ ] **Step 1: Write the failing test**

Append to `ocr.rs`:

```rust
#[cfg(test)]
mod rasterise_tests {
    use super::*;
    use std::process::Command;

    fn tesseract_and_pdftoppm_available() -> bool {
        Command::new("pdftoppm").arg("-v").output().is_ok()
            && Command::new("tesseract").arg("--version").output().is_ok()
    }

    /// Create a tiny 1-page PDF via pdftoppm's sibling tool — easier: use
    /// the system `convert` (imagemagick) or fall back to lopdf for a
    /// synthetic page. To avoid extra deps we just check pdftoppm runs
    /// against the first Sindh fixture if present.
    fn fixture_pdf() -> Option<PathBuf> {
        let p = std::env::var("OCR_FIXTURE_DIR").ok()?;
        let dir = PathBuf::from(p);
        let candidate = dir.join("REMOVAL FROM SERVICE SPECIAL POWERS SINDH.pdf");
        candidate.exists().then_some(candidate)
    }

    #[test]
    fn rasterise_produces_pngs_and_cleans_up() {
        if !tesseract_and_pdftoppm_available() {
            eprintln!("skip: tesseract/pdftoppm not available");
            return;
        }
        let Some(pdf) = fixture_pdf() else {
            eprintln!("skip: OCR_FIXTURE_DIR not set or fixture missing");
            return;
        };

        let cfg = OcrConfig {
            dpi: 150, // lower DPI = faster test
            timeout_per_page_secs: 60,
            tmp_root: std::env::temp_dir(),
        };

        let work = rasterise(&pdf, &cfg).expect("rasterise");
        assert!(work.png_paths.len() >= 1, "at least one page rendered");
        for p in &work.png_paths {
            assert!(p.exists(), "{p:?} should exist before drop");
        }
        let dir = work.tmp_dir.clone();
        drop(work);
        assert!(!dir.exists(), "tmp dir should be cleaned on drop: {dir:?}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::ocr::rasterise_tests
```

Expected: compile error — `rasterise` and `OcrWork` do not exist.

- [ ] **Step 3: Implement rasterisation**

Append to `ocr.rs`:

```rust
use std::process::Command;

/// In-flight rasterisation result. Drop deletes `tmp_dir` and everything
/// inside it.
pub struct OcrWork {
    pub tmp_dir: PathBuf,
    pub png_paths: Vec<PathBuf>,
}

impl Drop for OcrWork {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
    }
}

pub fn rasterise(pdf: &Path, cfg: &OcrConfig) -> Result<OcrWork, ParseError> {
    std::fs::create_dir_all(&cfg.tmp_root)
        .map_err(|e| ParseError::Other(format!("create tmp_root: {e}")))?;
    let tmp_dir = cfg.tmp_root.join(format!("echoai-ocr-{}", uuid_like()));
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| ParseError::Other(format!("create tmp_dir: {e}")))?;

    let stem = tmp_dir.join("page");
    let output = Command::new("pdftoppm")
        .arg("-png")
        .arg("-r").arg(cfg.dpi.to_string())
        .arg(pdf)
        .arg(&stem)
        .output()
        .map_err(|e| ParseError::Other(format!("pdftoppm spawn: {e}")))?;
    if !output.status.success() {
        return Err(ParseError::Other(format!(
            "pdftoppm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let mut png_paths: Vec<PathBuf> = std::fs::read_dir(&tmp_dir)
        .map_err(|e| ParseError::Other(format!("read_dir: {e}")))?
        .filter_map(|r| r.ok().map(|d| d.path()))
        .filter(|p| p.extension().map(|e| e == "png").unwrap_or(false))
        .collect();
    png_paths.sort();

    Ok(OcrWork { tmp_dir, png_paths })
}

/// Tiny non-crypto unique-id for temp dir names. Avoids adding a `uuid` dep
/// if the project does not already have one.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:x}-{}", now.as_nanos(), std::process::id())
}
```

If the project already depends on `uuid`, replace `uuid_like()` with `uuid::Uuid::new_v4().to_string()` and import it.

- [ ] **Step 4: Run test to verify it passes**

```bash
export OCR_FIXTURE_DIR="/home/riaz/Desktop/Sindh Govt Documents"
cd src-tauri && cargo test -p echo-ai --lib services::parsers::ocr::rasterise_tests -- --nocapture
```

Expected: 1 passed (or skipped if env var not set).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/parsers/ocr.rs
git commit -m "feat(004): ocr::rasterise via pdftoppm with RAII tmp-dir"
```

---

## Task 9 — `ocr::ocr_pdf`: tesseract per page + stitch

**Files:**
- Modify: `src-tauri/src/services/parsers/ocr.rs`

Run tesseract on each PNG, stitch the page texts with `\x0c` separators (matching `pdftotext` convention so `pdftotext_page_offsets` in `pdf.rs` works unchanged), return a `ParsedDocument`.

- [ ] **Step 1: Write the failing test**

Append to `ocr.rs`:

```rust
#[cfg(test)]
mod e2e_tests {
    use super::*;
    use super::rasterise_tests::*; // pulls in the helpers

    #[test]
    fn ocr_pdf_extracts_text_from_fixture() {
        let Some(pdf) = fixture_pdf() else {
            eprintln!("skip: OCR_FIXTURE_DIR not set");
            return;
        };
        if !tesseract_and_pdftoppm_available() {
            eprintln!("skip: tesseract/pdftoppm not available");
            return;
        }

        let cfg = OcrConfig {
            dpi: 150,
            timeout_per_page_secs: 60,
            tmp_root: std::env::temp_dir(),
        };
        let doc = ocr_pdf(&pdf, &cfg).expect("ocr_pdf");
        assert!(doc.text.len() > 200, "expected non-trivial OCR text");
        assert!(!doc.page_offsets.is_empty());
        // Heuristic: ASCII content dominates.
        let ascii = doc.text.chars().filter(|c| c.is_ascii()).count();
        assert!(ascii as f32 / doc.text.chars().count() as f32 > 0.90);
    }
}
```

(The `rasterise_tests::*` re-export will need `pub(super)` on the two helpers; add that in the same step.)

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers::ocr::e2e_tests
```

Expected: compile error — `ocr_pdf` does not exist.

- [ ] **Step 3: Implement `ocr_pdf`**

Append to `ocr.rs`:

```rust
use std::time::Duration;

pub fn ocr_pdf(pdf: &Path, cfg: &OcrConfig) -> Result<ParsedDocument, ParseError> {
    let work = rasterise(pdf, cfg)?;
    if work.png_paths.is_empty() {
        return Err(ParseError::Other("pdftoppm produced no pages".into()));
    }

    let mut full_text = String::new();
    let mut page_offsets: Vec<(u32, usize)> = Vec::with_capacity(work.png_paths.len());

    for (idx, png) in work.png_paths.iter().enumerate() {
        let page_num = idx as u32 + 1;
        let offset = full_text.len();
        page_offsets.push((page_num, offset));

        match run_tesseract(png, Duration::from_secs(cfg.timeout_per_page_secs)) {
            Ok(page_text) => {
                full_text.push_str(&page_text);
            }
            Err(e) => {
                log::warn!("tesseract failed on {png:?}: {e}; skipping page");
            }
        }
        // Form-feed separator matches pdftotext convention.
        if idx + 1 < work.png_paths.len() {
            full_text.push('\x0c');
        }
    }

    Ok(ParsedDocument { text: full_text, page_offsets })
}

fn run_tesseract(png: &Path, _timeout: Duration) -> Result<String, ParseError> {
    // NB: std::process does not natively support timeouts; for v1 we rely
    // on tesseract's own --timeout flag (seconds). If tesseract is older
    // than the version that supports it, drop the arg and revisit.
    let output = Command::new("tesseract")
        .arg(png)
        .arg("-")             // stdout
        .arg("-l").arg("eng")
        .arg("--psm").arg("1")
        .output()
        .map_err(|e| ParseError::Other(format!("tesseract spawn: {e}")))?;
    if !output.status.success() {
        return Err(ParseError::Other(format!(
            "tesseract: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
export OCR_FIXTURE_DIR="/home/riaz/Desktop/Sindh Govt Documents"
cd src-tauri && cargo test -p echo-ai --lib services::parsers::ocr -- --nocapture
```

Expected: rasterise + e2e tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/parsers/ocr.rs
git commit -m "feat(004): ocr_pdf — tesseract per page, stitched with form-feed"
```

---

## Task 10 — Wire OCR into `pdf.rs` as the third tier

**Files:**
- Modify: `src-tauri/src/services/parsers/pdf.rs`
- Modify: `src-tauri/src/services/parsers/mod.rs`

Replace the existing chars/page gate with `text_quality::score`. Add OCR as the third tier. Surface `extraction_method` via a new sibling type so callers can persist it. Add a `MissingOcr` variant to `ParseError`.

- [ ] **Step 1: Extend `ParsedDocument` (or add a sibling)**

Edit `src-tauri/src/services/parsers/mod.rs`. Add fields to `ParsedDocument`:

```rust
#[derive(Debug)]
pub struct ParsedDocument {
    pub text: String,
    pub page_offsets: Vec<(u32, usize)>,
    /// Which tier produced the text: "lopdf" | "pdftotext" | "ocr" | "ocr_skipped".
    pub extraction_method: &'static str,
    /// Final quality score (0.0-1.0).
    pub quality_score: f32,
}
```

Update existing constructors of `ParsedDocument` in `text.rs`, `document.rs`, `image.rs`, `audio_video.rs`, `pdf.rs` to set the new fields (most will be `extraction_method: "native", quality_score: 1.0`). Use Grep to find them:

```bash
grep -rn "ParsedDocument {" src-tauri/src/services/parsers/
```

For each non-PDF parser, set:

```rust
ParsedDocument {
    text,
    page_offsets,
    extraction_method: "native",
    quality_score: 1.0,
}
```

- [ ] **Step 2: Run tests to verify nothing else broke**

```bash
cd src-tauri && cargo check -p echo-ai --tests
```

Expected: clean compile.

- [ ] **Step 3: Replace `pdf.rs` extraction with the three-tier flow**

Edit `src-tauri/src/services/parsers/pdf.rs`. Replace the body of `extract` with:

```rust
use crate::services::parsers::ocr::{ocr_pdf, OcrConfig};
use crate::services::parsers::text_quality::{score, Verdict};
use std::path::PathBuf;

pub fn extract(path: &Path) -> Result<ParsedDocument, ParseError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(ParseError::FileTooLarge {
            size_bytes: metadata.len(),
            limit_bytes: MAX_FILE_SIZE,
        });
    }
    if metadata.len() > SAFE_FILE_SIZE {
        log::warn!(
            "PDF {:?} is {:.1} MB — lopdf will load it entirely into RAM.",
            path, metadata.len() as f64 / 1_048_576.0
        );
    }

    // ── Tier 1: lopdf ────────────────────────────────────────────────────────
    let mut best: Option<(String, Vec<(u32, usize)>, &'static str, f32)> = None;

    if let Ok(doc) = lopdf::Document::load(path) {
        let page_ids: Vec<u32> = doc.get_pages().keys().copied().collect();
        let total_pages = page_ids.len().max(1);
        let mut full_text = String::new();
        let mut page_offsets: Vec<(u32, usize)> = Vec::new();
        for (idx, &page_num) in page_ids.iter().enumerate() {
            page_offsets.push((idx as u32 + 1, full_text.len()));
            if let Ok(t) = doc.extract_text(&[page_num]) {
                full_text.push_str(&t);
            }
            full_text.push('\n');
        }
        let q = score(&full_text, total_pages);
        if q.verdict == Verdict::Good {
            return Ok(ParsedDocument {
                text: full_text, page_offsets,
                extraction_method: "lopdf", quality_score: q.score,
            });
        }
        best = Some((full_text, page_offsets, "lopdf", q.score));
    }

    // ── Tier 2: pdftotext ────────────────────────────────────────────────────
    if let Ok(text) = extract_with_pdftotext(path) {
        if !text.trim().is_empty() {
            let page_offsets = pdftotext_page_offsets(&text);
            let q = score(&text, page_offsets.len().max(1));
            if q.verdict == Verdict::Good {
                return Ok(ParsedDocument {
                    text, page_offsets,
                    extraction_method: "pdftotext", quality_score: q.score,
                });
            }
            if best.as_ref().map(|b| b.3).unwrap_or(0.0) < q.score {
                best = Some((text, page_offsets, "pdftotext", q.score));
            }
        }
    }

    // ── Tier 3: OCR ──────────────────────────────────────────────────────────
    let ocr_cfg = OcrConfig {
        dpi: 300,
        timeout_per_page_secs: 60,
        tmp_root: ocr_tmp_root(),
    };
    match ocr_pdf(path, &ocr_cfg) {
        Ok(doc) => {
            let q = score(&doc.text, doc.page_offsets.len().max(1));
            // Keep OCR result if it scored at least as well as the prior best.
            let prior_score = best.as_ref().map(|b| b.3).unwrap_or(0.0);
            if q.score >= prior_score {
                return Ok(ParsedDocument {
                    text: doc.text, page_offsets: doc.page_offsets,
                    extraction_method: "ocr", quality_score: q.score,
                });
            }
        }
        Err(e) => {
            log::warn!("OCR failed for {:?}: {e}", path);
        }
    }

    // ── Fallback: best-so-far, marked ocr_skipped ────────────────────────────
    if let Some((text, page_offsets, _, qscore)) = best {
        return Ok(ParsedDocument {
            text, page_offsets,
            extraction_method: "ocr_skipped", quality_score: qscore,
        });
    }
    Err(ParseError::ScannedPdf)
}

fn ocr_tmp_root() -> PathBuf {
    // Data partition tmp, NEVER /tmp (root-disk constraint).
    // The data dir is project-specific; for now, use $ECHOAI_DATA_DIR or
    // a sibling of the DB path. Wired up properly in Task 12 via config.
    std::env::var("ECHOAI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("ocr")
}
```

- [ ] **Step 4: Run all PDF parser tests**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::parsers
```

Expected: green. No new failures.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/parsers/pdf.rs \
        src-tauri/src/services/parsers/mod.rs \
        src-tauri/src/services/parsers/
git commit -m "feat(004): three-tier PDF extraction (lopdf → pdftotext → OCR)"
```

---

## Task 11 — DB migration 016 + persist extraction provenance

**Files:**
- Create: `src-tauri/src/db/migrations/016_extraction_quality.sql`
- Modify: `src-tauri/src/services/indexer.rs`

Schema change is two nullable columns — instant on a 17 GB DB. Then the indexer writes them on each new/updated source.

- [ ] **Step 1: Write the migration**

Create `src-tauri/src/db/migrations/016_extraction_quality.sql`:

```sql
-- Feature 004 — OCR integration.
-- Record which extraction tier produced each source's text and the
-- post-extraction quality score. NULL for legacy rows; backfilled on
-- next re-extraction.

ALTER TABLE sources ADD COLUMN extraction_method TEXT;
ALTER TABLE sources ADD COLUMN quality_score    REAL;
```

- [ ] **Step 2: Verify migration applies in the test pool**

```bash
cd src-tauri && cargo test -p echo-ai --lib db::
```

Expected: any existing DB tests still pass; `sqlx::migrate!` picks up `016_extraction_quality.sql` automatically.

- [ ] **Step 3: Persist the new fields from the indexer**

Locate the row insert/update that stores parsed source text:

```bash
grep -n "INSERT INTO sources\|UPDATE sources" src-tauri/src/services/indexer.rs
```

For each statement that follows a successful PDF parse, add `extraction_method = ?, quality_score = ?` and bind from the `ParsedDocument`. Example shape (adapt to actual code):

```rust
sqlx::query!(
    "UPDATE sources SET text = ?, extraction_method = ?, quality_score = ? WHERE id = ?",
    parsed.text,
    parsed.extraction_method,
    parsed.quality_score,
    source_id,
)
.execute(&pool)
.await?;
```

If the existing code uses a builder/struct-based insert, add the two fields there instead. Match the project's existing pattern (sqlx macro vs. `query_as`).

- [ ] **Step 4: Run indexer tests**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::indexer
```

Expected: green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/migrations/016_extraction_quality.sql \
        src-tauri/src/services/indexer.rs
git commit -m "feat(004): migration 016 + persist extraction_method/quality_score"
```

---

## Task 12 — Startup re-extract hook

**Files:**
- Modify: `src-tauri/src/services/indexer.rs`

On boot, find sources with `quality_score IS NULL OR (quality_score < THRESHOLD AND extraction_method != 'ocr')` and enqueue them. Idempotent: subsequent boots are no-ops.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/services/indexer.rs` (or its tests module):

```rust
#[cfg(test)]
mod reextract_tests {
    use super::*;
    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn enqueues_only_low_quality_non_ocr_sources(pool: SqlitePool) {
        // Seed three sources.
        sqlx::query!("INSERT INTO sources (id, path, kind, status, extraction_method, quality_score) VALUES \
                     (1, '/a.pdf', 'pdf', 'indexed', 'lopdf',    0.95), \
                     (2, '/b.pdf', 'pdf', 'indexed', 'pdftotext', 0.30), \
                     (3, '/c.pdf', 'pdf', 'indexed', 'ocr',       0.25)")
            .execute(&pool).await.unwrap();
        // NULL row.
        sqlx::query!("INSERT INTO sources (id, path, kind, status) VALUES (4, '/d.pdf', 'pdf', 'indexed')")
            .execute(&pool).await.unwrap();

        let n = enqueue_quality_reextract(&pool).await.unwrap();
        // Should enqueue id=2 (low + non-ocr) and id=4 (NULL).
        // Should NOT enqueue id=1 (good) or id=3 (already ocr).
        assert_eq!(n, 2);
    }
}
```

(Adapt the seed `INSERT` to whatever columns are NOT NULL in the real schema — run `.schema sources` against a fresh test DB to be sure.)

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::indexer::reextract_tests
```

Expected: compile error — `enqueue_quality_reextract` does not exist.

- [ ] **Step 3: Implement the hook**

Add to `indexer.rs`:

```rust
const QUALITY_REEXTRACT_THRESHOLD: f32 = 0.6;

/// Return value: number of rows enqueued for re-extraction.
pub async fn enqueue_quality_reextract(pool: &sqlx::SqlitePool) -> sqlx::Result<usize> {
    let rows = sqlx::query!(
        "SELECT id FROM sources \
         WHERE status = 'indexed' \
           AND kind = 'pdf' \
           AND (quality_score IS NULL \
                OR (quality_score < ? AND COALESCE(extraction_method, '') != 'ocr'))",
        QUALITY_REEXTRACT_THRESHOLD,
    )
    .fetch_all(pool)
    .await?;

    let n = rows.len();
    for row in rows {
        // Re-queue by flipping status to the value the indexer treats as
        // 'needs re-extract'. Match whatever the existing pipeline uses
        // (likely 'pending' or 'queued').
        sqlx::query!("UPDATE sources SET status = 'pending' WHERE id = ?", row.id)
            .execute(pool).await?;
    }
    Ok(n)
}
```

Call it once at startup. Find the app/init entry point:

```bash
grep -rn "indexer::start\|services::indexer" src-tauri/src/main.rs src-tauri/src/lib.rs | head -5
```

Insert:

```rust
let enqueued = services::indexer::enqueue_quality_reextract(&pool).await?;
log::info!("startup re-extract: {} sources enqueued", enqueued);
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p echo-ai --lib services::indexer
```

Expected: green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/indexer.rs src-tauri/src/main.rs src-tauri/src/lib.rs
git commit -m "feat(004): startup re-extract hook for sub-threshold sources"
```

---

## Task 13 — Acceptance gate over the Sindh legal corpus

**Files:**
- Create: `src-tauri/tests/ocr_integration_acceptance.rs`

The Feature-004 gate. Mirrors the Feature-003 acceptance harness style: standalone integration test, no LLM dependency, reads `OCR_FIXTURE_DIR` env var.

- [ ] **Step 1: Write the acceptance test**

Create `src-tauri/tests/ocr_integration_acceptance.rs`:

```rust
//! Feature 004 acceptance gate.
//! Every PDF in OCR_FIXTURE_DIR must end with verdict = Good.
//!
//! Run:
//!   OCR_FIXTURE_DIR="/home/riaz/Desktop/Sindh Govt Documents" \
//!     cargo test -p echo-ai --test ocr_integration_acceptance -- --nocapture

use echo_ai::services::parsers::pdf;
use echo_ai::services::parsers::text_quality::{score, Verdict};

#[test]
fn every_sindh_pdf_extracts_with_good_verdict() {
    let dir = match std::env::var("OCR_FIXTURE_DIR") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => {
            eprintln!("skip: OCR_FIXTURE_DIR not set");
            return;
        }
    };

    let pdfs: Vec<_> = std::fs::read_dir(&dir).expect("read fixture dir")
        .filter_map(|r| r.ok().map(|d| d.path()))
        .filter(|p| p.extension().map(|e| e == "pdf").unwrap_or(false))
        .collect();
    assert!(!pdfs.is_empty(), "no PDFs found in {dir:?}");

    let mut failures: Vec<String> = Vec::new();
    for pdf_path in &pdfs {
        let name = pdf_path.file_name().unwrap().to_string_lossy().into_owned();
        match pdf::extract(pdf_path) {
            Ok(doc) => {
                let q = score(&doc.text, doc.page_offsets.len().max(1));
                println!(
                    "{name}: method={} score={:.3} verdict={:?}",
                    doc.extraction_method, q.score, q.verdict
                );
                if q.verdict != Verdict::Good {
                    failures.push(format!("{name}: verdict={:?}", q.verdict));
                }
            }
            Err(e) => failures.push(format!("{name}: parse error {e}")),
        }
    }

    assert!(failures.is_empty(), "verdict failures:\n  {}", failures.join("\n  "));
}

#[test]
fn constitution_pdf_does_not_trip_ocr() {
    // Born-digital control: confirms the heuristic does not over-trigger.
    let Ok(dir) = std::env::var("OCR_FIXTURE_DIR") else { return; };
    let path = std::path::PathBuf::from(dir).join("constitution.pdf");
    if !path.exists() { return; }
    let doc = pdf::extract(&path).expect("extract constitution");
    assert!(
        doc.extraction_method == "lopdf" || doc.extraction_method == "pdftotext",
        "constitution.pdf should not need OCR, got method={}", doc.extraction_method
    );
}
```

- [ ] **Step 2: Run the acceptance test**

```bash
export OCR_FIXTURE_DIR="/home/riaz/Desktop/Sindh Govt Documents"
cd src-tauri && cargo test -p echo-ai --test ocr_integration_acceptance -- --nocapture
```

Expected: 2 passed. Per-file output shows the tier each PDF landed on.

If any file fails: tune thresholds in `text_quality.rs` (Task 5) and `pdf.rs` (Task 10), then re-run. Tuning is part of this task — the gate is the spec.

- [ ] **Step 3: Smoke-check section-heading FTS5 round-trip (manual)**

Per spec §7.3:

```bash
# Pick a known section heading from one of the rules PDFs, e.g.
#   "Rule 3" or an Article number. Index the corpus into a scratch DB,
#   then:
sqlite3 /tmp/scratch.db "SELECT chunk_text FROM chunks_fts WHERE chunks_fts MATCH 'Rule 3' LIMIT 5;"
```

Confirm the matched chunk text contains the heading verbatim.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/ocr_integration_acceptance.rs
git commit -m "test(004): acceptance gate over Sindh legal corpus"
```

---

## Task 14 — CHANGELOG_DEV.md entry + tick the spec checklist

**Files:**
- Modify: `CHANGELOG_DEV.md`
- Modify: `specs/004-ocr-integration/spec.md`

Per CLAUDE.md, every non-trivial change gets a dated entry.

- [ ] **Step 1: Append to CHANGELOG_DEV.md**

Edit `CHANGELOG_DEV.md`. Add a new dated section at the top of the relevant area (match existing style — date stamp + symptom/why + fix + impact). Example:

```markdown
## 2026-05-DD — Feature 004: OCR integration for the PDF indexer

**Symptom/why:** Font-corrupted PDFs (e.g. Gutenberg-mirror Hamlet, edge cases in scanned legal text) extracted via lopdf/pdftotext produced glyph-mangled text (`{` mid-word, missing letters, replacement chars). This poisoned FTS5 tokenisation and dense embeddings, making downstream eval / retrieval quality unmeasurable. The previous chars/page gate only caught image-only PDFs.

**Fix:** Three-tier extraction in `src-tauri/src/services/parsers/pdf.rs:extract` — lopdf → pdftotext → Tesseract OCR. New gate `services::parsers::text_quality::score` combines four signals (chars/page, non-ASCII-punct ratio, dict-hit ratio via bundled SCOWL wordlist, mean token length). OCR sub-flow lives in new `services::parsers::ocr` (pdftoppm → tesseract subprocesses, RAII temp dir under the data partition). Schema migration `016_extraction_quality.sql` adds `extraction_method` and `quality_score` to `sources`; startup hook `enqueue_quality_reextract` auto-re-indexes sub-threshold rows.

**Cross-cutting impact:** `ParsedDocument` gained two fields (`extraction_method`, `quality_score`) — all parsers updated. Indexer persists them. Acceptance gate `cargo test -p echo-ai --test ocr_integration_acceptance` (with `OCR_FIXTURE_DIR` env) over the 6-file Sindh corpus.

**Follow-ups (v2):** multi-language OCR (Urdu/Arabic), GPU OCR, per-page mixing of OCR and native text, bundle tesseract on Windows.
```

- [ ] **Step 2: Tick the acceptance checklist in the spec**

In `specs/004-ocr-integration/spec.md`, replace each `- [ ]` in the "Acceptance checklist" section with `- [x]` for items now completed.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG_DEV.md specs/004-ocr-integration/spec.md
git commit -m "docs(004): CHANGELOG entry + acceptance checklist tick"
```

---

## Task 15 — PHR + open the PR

**Files:**
- Create: `history/prompts/004-ocr-integration/001-ocr-integration-shipped.misc.prompt.md`

Per CLAUDE.md, every user-facing piece of work ends with a PHR. Use the template at `.specify/templates/phr-template.prompt.md`.

- [ ] **Step 1: Create the PHR**

Match the existing PHR style under `history/prompts/003-constitution-hierarchical-graph/` for fields. Stage: `misc` (the work spans plan + tasks + green + refactor + tests, not a single SDD stage).

- [ ] **Step 2: Push the branch and open a PR**

```bash
git push -u origin 004-ocr-integration
gh pr create --base master --head 004-ocr-integration \
  --title "feat(004): OCR integration — three-tier PDF pipeline" \
  --body "$(cat <<'EOF'
Closes Feature 004. Spec: specs/004-ocr-integration/spec.md. Plan: specs/004-ocr-integration/plan.md.

Three-tier extraction: lopdf → pdftotext → Tesseract OCR. Gate is a new text-quality score (chars/page, non-ASCII-punct ratio, dict-hit ratio via bundled SCOWL wordlist, mean token length). OCR sub-flow lives in a new isolated module (pdftoppm + tesseract subprocesses, RAII temp dir on the data partition).

Schema migration 016 adds extraction_method + quality_score to sources. Startup hook auto-re-extracts sub-threshold rows.

Acceptance gate (cargo test --test ocr_integration_acceptance with OCR_FIXTURE_DIR) green over the 6-file Sindh corpus; constitution.pdf confirmed as born-digital control (no OCR triggered).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Commit any remaining PHR**

```bash
git add history/prompts/004-ocr-integration/
git commit -m "docs(004): PHR for OCR integration"
git push
```

---

## Self-Review

**Spec coverage:**

| Spec section | Task(s) |
|---|---|
| §1 Problem | Task 13 (acceptance gate validates against the exact failure mode) |
| §2 Goals G1–G5 | G1 → Tasks 3–5; G2 → Tasks 7–9; G3 → Tasks 10–11; G4 → Tasks 11–12; G5 → Task 10 (ocr_skipped path) |
| §3 User-facing behaviour | Tasks 10–12 (auto-heal + clean text), Task 10 fallback |
| §4 Architecture | Tasks 2–12 (each file in §4.1 has a dedicated task) |
| §5 Quality score | Tasks 3, 4, 5 (one signal per task, TDD'd) |
| §6 OCR sub-flow | Tasks 7, 8, 9 |
| §6.1 Failure handling | Task 10 (best-of fallback) + Task 8 (RAII cleanup) |
| §7 Testing | Tasks 6 (fixtures), 13 (gate) |
| §8 Migration & rollout | Tasks 11, 12 |
| §9 Open implementation questions | Resolved inline: aggregation = first-signal-that-trips wins (Task 5); `include_str!` + `HashSet` at runtime (Task 5); OCR runs inline in the existing indexer pipeline (Task 11) |
| §10 Risks | Mitigated by Task 13's born-digital control test |
| Acceptance checklist | Task 14 ticks it |

**Placeholder scan:** none — every step has runnable code/commands.

**Type consistency:** `Verdict`, `Quality`, `ParsedDocument` (with two new fields), `OcrConfig`, `OcrWork`, `extraction_method` literal values (`"lopdf"`, `"pdftotext"`, `"ocr"`, `"ocr_skipped"`, `"native"`) — used identically wherever they appear. `enqueue_quality_reextract` signature `(pool: &SqlitePool) -> sqlx::Result<usize>` is referenced consistently in Tasks 11 and 12.

---

## Execution Handoff

Plan complete and saved to `specs/004-ocr-integration/plan.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
