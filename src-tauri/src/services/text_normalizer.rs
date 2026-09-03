//! OCR text normalization — fix common tesseract misreads before structure
//! detection. Idempotent: clean text passes through unchanged.
//!
//! Cap: ≤ 50 dictionary entries v1, each reviewed manually. Entries must
//! have ≥ 2-letter Levenshtein distance from any common English word.

use regex_lite::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

const MISREADS: &[(&str, &str)] = &[
    ("Govemor",   "Governor"),
    ("Govemment", "Government"),
    ("Sccretary", "Secretary"),
    ("Mimster",   "Minister"),
    ("Deparlment","Department"),
];

/// Generic words that OCR/PDF extraction commonly fractures with a stray
/// internal space ("o ne", "sha ll", "Gov ernment"). Lower-cased. The intra-word
/// repair pass (US2/FR-004) only ever DELETES a stray single space — it never
/// changes a letter — and only when the two adjacent fragments rejoin into one
/// of these listed words. This keeps the repair strictly verbatim-safe: a join
/// that is NOT a known word is left byte-for-byte untouched, so two legitimate
/// adjacent words ("in formation", "the cat") are never merged. Generic English
/// / legal-administrative vocabulary only — no document-specific terms.
const JOIN_TARGETS: &[&str] = &[
    // number words (amounts are the most-cited fractured tokens)
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    "ten", "eleven", "twelve", "twenty", "thirty", "forty", "fifty",
    "hundred", "thousand", "lakh", "lakhs", "million", "crore",
    // high-frequency legal / governance words
    // NOTE (013/D9): every entry MUST be a single word that was split mid-word
    // ("sha ll"→"shall"). NEVER add a two-word fusion like "shallnot": that would
    // turn the verbatim phrase "shall not" into "shallnot" and alter quoted law.
    "shall", "provided", "section", "rule", "schedule", "chapter",
    "clause", "ordinance", "government", "secretary", "minister", "department",
    "governor", "province", "provincial", "federal", "director", "notification",
    "procurement", "contracting", "competent", "authority", "financial",
    "committee", "approval", "tender", "distributor", "proprietary",
];

fn join_targets() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| JOIN_TARGETS.iter().copied().collect())
}

fn word_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[A-Za-z]+").unwrap())
}

/// Verbatim-safe intra-word-space repair (US2/FR-004). Deletes a stray SINGLE
/// space between two adjacent word fragments ONLY when they rejoin into a known
/// word ([`JOIN_TARGETS`]); otherwise the text is left byte-unchanged.
///
/// A manual left-to-right scan (not a single `replace_all`) is required: on a
/// non-match it advances by ONE word so the second fragment can still pair with
/// the next ("the Gov ernment": "the Gov" is not a word, but "Gov ernment" must
/// still rejoin to "Government"). Only single-space gaps are candidates, so
/// ≥2-space boundaries (the act.rs marginal-label `\s{2,}`) are never touched.
/// Idempotent; casing preserved (only the space is removed).
pub(crate) fn repair_spaced_words(input: &str) -> String {
    let targets = join_targets();
    let words: Vec<(usize, usize)> =
        word_re().find_iter(input).map(|m| (m.start(), m.end())).collect();
    let mut result = String::with_capacity(input.len());
    let mut last_end = 0;
    let mut i = 0;
    while i < words.len() {
        let (ws, we) = words[i];
        // Emit untouched text (whitespace/punctuation) preceding this word.
        result.push_str(&input[last_end..ws]);
        // Try to merge this word with the next when the gap is either a stray
        // single space ("sha ll") OR a same-line break-hyphen ("Govern- ment",
        // "Govern-ment") — i.e. a hyphen followed by optional horizontal
        // whitespace — and the rejoined form is a known word. RC-H (2026-07-01):
        // the hyphen case catches the residual broken words pdfplumber leaves on
        // ONE line (no newline for `hyphen_join` to see, e.g. Delegation's
        // "Govern- ment" ×14). Verbatim-safe: only the stray space and/or the
        // break-hyphen is removed, and only when the join is a known word — a
        // real hyphenated compound ("Director-General") is never in the list, so
        // it stays byte-for-byte untouched.
        if i + 1 < words.len() {
            let (ns, ne) = words[i + 1];
            let gap = &input[we..ns];
            let is_space_gap = gap == " ";
            let is_hyphen_gap =
                gap.starts_with('-') && gap[1..].bytes().all(|b| b == b' ' || b == b'\t');
            if is_space_gap || is_hyphen_gap {
                let joined = format!("{}{}", &input[ws..we], &input[ns..ne]);
                if targets.contains(joined.to_lowercase().as_str()) {
                    result.push_str(&joined);
                    last_end = ne;
                    i += 2;
                    continue;
                }
            }
        }
        result.push_str(&input[ws..we]);
        last_end = we;
        i += 1;
    }
    result.push_str(&input[last_end..]);
    result
}

fn brace_fix_open() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\{(?P<after>[a-z0-9])\)").unwrap())
}

fn hyphen_join() -> &'static Regex {
    // Line-break hyphen: a lowercase letter, a hyphen, then a newline and the
    // continuation letter. RC-H (2026-07-01): tolerate horizontal whitespace on
    // BOTH sides of the newline — PDF continuation lines are indented
    // (`Govern-\n        ment`) and the hyphen may carry a trailing space
    // (`Govern- \n ment`), which the original tight `-\n` pattern missed,
    // leaving ~150 residual broken words (constitution 42, Compendium 43,
    // RoB 16). Only the hyphen + surrounding break whitespace is removed.
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?P<a>[a-z])-[ \t]*\n[ \t]*(?P<b>[a-z])").unwrap())
}

fn blank_line_runs() -> &'static Regex {
    // Three-or-more consecutive newlines (each possibly bracketed by
    // spaces / tabs) → collapse to a single paragraph break.
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?:[ \t]*\n){3,}").unwrap())
}

fn trailing_spaces_per_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[ \t]+\n").unwrap())
}

fn leading_tabs_per_line() -> &'static Regex {
    // Tabs at the start of any line — pure noise (PDF column indent).
    // Interior tabs are left alone so they remain visible as separators.
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?m)^\t+").unwrap())
}

fn multi_space_runs_in_line() -> &'static Regex {
    // 3+ consecutive spaces inside a line (PDF column gutters preserved as
    // padding). Collapse to a single space. Two spaces are kept because some
    // act.rs heading patterns rely on `\s{2,}` between marginal label and
    // section number — that boundary needs ≥ 2 spaces to remain detectable.
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r" {3,}").unwrap())
}

/// Column-gutter handling (US8 tabular de-interleave, 2026-06-16).
///
/// `pdftotext -layout` aligns table columns with runs of spaces. Blanket-
/// collapsing every 3+ space gutter to a single space (the historical
/// behaviour) fuses a table row's cells into one run that reads as a false
/// sentence — the live Delegation-of-Financial-Powers garble ("…Full Powers
/// within the allocated expenditure on any budget … item year", where "item"
/// bled in from an adjacent column). That misleads a director copy-pasting a
/// cited cell, violating the citation-grade contract.
///
/// Generic geometry rule — no per-doc vocabulary, and provably a no-op on
/// ordinary prose:
///   * A *table row* is a line with **two or more** gutters that is **adjacent
///     to another** two-or-more-gutter line (real tables are multi-row). Its
///     gutters become single TABs; normalization preserves interior tabs, so
///     cells stay verbatim and never fuse into a fake clause, and the Fix-1
///     bare-number pattern can still see the row's leading `N. Title`.
///   * Every other line (prose, or a lone bloated line) keeps the original
///     single-space collapse, so the 2-space act.rs marginal-label boundary and
///     the existing collapse tests are untouched.
fn collapse_columns_per_line(input: &str) -> String {
    let gutter = multi_space_runs_in_line();
    let lines: Vec<&str> = input.split_inclusive('\n').collect();
    let multi: Vec<bool> = lines.iter().map(|l| gutter.find_iter(l).count() >= 2).collect();
    let mut out = String::with_capacity(input.len());
    for (i, line) in lines.iter().enumerate() {
        let table_row = multi[i]
            && ((i > 0 && multi[i - 1]) || (i + 1 < lines.len() && multi[i + 1]));
        let sep = if table_row { "\t" } else { " " };
        out.push_str(&gutter.replace_all(line, sep));
    }
    out
}

/// The space between a provision number and its text, when extraction ate it.
///
/// 2026-07-29: pdfplumber sometimes emits `142.Subject to the Constitution—`
/// where `pdftotext` gives `142. Subject …`. Measured on the Constitution, six
/// Articles were lost exactly this way — 113, 142, 152, 197, 217, 219 — while
/// their neighbours on the SAME page kept their space and were detected. Every
/// provision pattern requires whitespace (or a bracket) after the dot, so the
/// glued line never registers as a heading and its text is absorbed into the
/// previous provision.
///
/// Why the repair lives here and not in the heading patterns: those patterns
/// deliberately refuse a bare `\s*` after the dot, because that would let a
/// year match (`1996.The Act` → "Section 1996"). Restricting the repair to a
/// **1–3 digit** ordinal keeps that guard intact — a 4-digit year is never
/// touched — while fixing the real provisions.
fn provision_dot_space() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // line start, optional amendment bracket, 1–3 digit ordinal (+ optional
    // letter suffix like 19A), a dot, then IMMEDIATELY a capital letter — or an
    // opening QUOTE before that capital.
    //
    // 2026-07-29: the quote alternative. Secretariat Instructions Rule 131
    // prints as `131.“PUC” (i.e. fresh receipt) will be submitted…` — glued like
    // its neighbours 129/130/132, but opening on a quoted defined term instead
    // of a plain capital, so the repair skipped it and the provision was
    // absorbed into Rule 130. `normalize` folds curly quotes to ASCII before
    // this runs, so only `"` and `'` need matching here.
    //
    // The capital is still REQUIRED after the optional quote: the guard that
    // stops `1996.The Act` minting "Section 1996" is the 1–3 digit ordinal, and
    // loosening what follows the dot must not become a way around it.
    //
    // 2026-07-30: the sub-clause alternative. The SLGA Compendium's Election
    // Rules print `19.(1) The Returning Officer shall…` — glued straight onto an
    // opening PAREN, because the provision leads with its first sub-clause rather
    // than a catchline. Eight rules were lost exactly this way (19, 24, 25, 26,
    // 31, 44, 50, 53) while their spaced neighbours were detected. A bare `(`
    // cannot smuggle a year past the guard either: `1996.(a)` is four digits and
    // still refused by the 1–3 digit ordinal.
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^([ \t]*\[?[ \t]*[0-9]{1,3}[A-Z]?\.)(["']?[A-Z]|\()"#).unwrap()
    })
}

fn restore_provision_dot_space(input: &str) -> String {
    provision_dot_space().replace_all(input, "$1 $2").into_owned()
}

/// Normalize OCR'd or scanned text. Pure function; safe to apply twice.
pub fn normalize(input: &str) -> String {
    let mut out = input.to_string();
    // Only fix opening-brace misreads (`{c)`, `{a)`, `{(i)` style). Closing
    // `}` is intentionally left alone — real OCR `}` chars in our corpus are
    // misread `]`s (e.g. `STEVTA}|`), not misread `)`s, and blanket-converting
    // them damages legitimate brace syntax (`see {note}`).
    out = brace_fix_open().replace_all(&out, "($after)").into_owned();
    out = out
        .replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'");
    out = hyphen_join().replace_all(&out, "$a$b").into_owned();
    for &(bad, good) in MISREADS {
        if out.contains(bad) {
            out = out.replace(bad, good);
        }
    }
    // Intra-word-space repair (US2/FR-004): rejoin "o ne"→"one", "sha ll"→"shall"
    // before structure detection. Verbatim-safe — only deletes a stray single
    // space when the result is a known word; runs after hyphen-join so
    // line-broken words are already whole.
    out = repair_spaced_words(&out);
    // Restore the space a provision number sometimes loses in extraction
    // (`142.Subject` → `142. Subject`). See `provision_dot_space`.
    out = restore_provision_dot_space(&out);
    // Whitespace cleanup — PDFs preserve column gutters as runs of blank lines
    // and as long horizontal-space runs (APT Rule 11 chunks were >80%
    // whitespace; max single chunk 32 796 chars, 53 triple-newline runs).
    // Three passes, order-sensitive:
    //   1. Strip leading tabs (PDF column indent → pure noise at line start).
    //   2. Trim trailing horizontal whitespace per line so blank-but-not-empty
    //      lines fold correctly in the next step.
    //   3. Collapse 3+ blank lines to a paragraph break.
    //   4. Collapse 3+ inline spaces to a single space (gutters between
    //      columns get preserved as 5-15-space padding); 2-space boundaries
    //      are kept intact so the act.rs marginal-label heading pattern
    //      (`label\s{2,}N\.\s+\(N\)`) still anchors.
    out = leading_tabs_per_line().replace_all(&out, "").into_owned();
    out = trailing_spaces_per_line().replace_all(&out, "\n").into_owned();
    out = blank_line_runs().replace_all(&out, "\n\n").into_owned();
    //   4. Column gutters: collapse to a single space for prose, but keep a
    //      multi-row table's cell boundaries as interior TABs (US8) so its
    //      cells don't fuse into a false sentence.
    out = collapse_columns_per_line(&out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_opening_brace_for_paren_adjacent_to_alpha_or_digit() {
        assert_eq!(normalize("{c) text"), "(c) text");
        assert_eq!(normalize("{2)"), "(2)");
        assert_eq!(normalize("{a) one {b) two"), "(a) one (b) two");
    }

    #[test]
    fn preserves_braces_that_arent_paren_misreads() {
        // Closing `}` is never converted — see comment in normalize().
        // Opening `{` only converted when followed by `<one-alnum>)`, so word-bracketing braces survive.
        assert_eq!(normalize("see {note}"), "see {note}");
        assert_eq!(normalize("{ }"), "{ }");
        assert_eq!(normalize("STEVTA}|"), "STEVTA}|");
    }

    /// The live defect: Constitution Article 142 arrived glued
    /// (`142.Subject`) and was absorbed into Article 141.
    #[test]
    fn restores_the_space_a_provision_number_lost() {
        assert_eq!(
            normalize("142.Subject to the Constitution—"),
            "142. Subject to the Constitution—"
        );
        // Amendment-bracketed and indented forms seen on the same document.
        // NB the indent itself shrinks — `collapse_columns_per_line` folds a
        // 3+-space gutter to one space — so assert on the repaired join.
        assert_eq!(normalize("  [113.The qualifications"), "  [113. The qualifications");
        assert_eq!(normalize("   217.Acting Commissioner"), " 217. Acting Commissioner");
        // alpha-suffixed ordinals
        assert_eq!(normalize("19A.Right to information"), "19A. Right to information");
    }

    /// The guard the heading patterns rely on must survive: a 4-digit YEAR
    /// glued to a capital must NOT gain a space, or `1996.The Act` would be
    /// minted as "Section 1996".
    #[test]
    fn does_not_split_a_year_from_its_sentence() {
        assert_eq!(normalize("1996.The Act was passed"), "1996.The Act was passed");
        assert_eq!(normalize("2009.Procurement rules"), "2009.Procurement rules");
        // …including when the sentence opens on a quoted term, which is the
        // shape the 2026-07-29 quote alternative added.
        assert_eq!(normalize("1996.\u{201C}The Act\u{201D} means"), "1996.\"The Act\" means");
    }

    /// Secretariat Instructions Rule 131 prints as `131.“PUC” (i.e. fresh
    /// receipt)…` — glued exactly like its neighbours 129/130/132, but opening
    /// on a QUOTED defined term instead of a plain capital, so the repair
    /// skipped it and the rule was absorbed into Rule 130.
    #[test]
    fn restores_the_space_before_a_quoted_defined_term() {
        assert_eq!(
            normalize("131.\u{201C}PUC\u{201D} (i.e. fresh receipt) will be submitted"),
            "131. \"PUC\" (i.e. fresh receipt) will be submitted"
        );
        // single quotes too — `normalize` folds both curly forms to ASCII first
        assert_eq!(normalize("90.\u{2018}Secret\u{2019} papers"), "90. 'Secret' papers");
        // the neighbours that already worked must be unchanged
        assert_eq!(normalize("130.A part file may be opened"), "130. A part file may be opened");
    }

    /// SLGA Compendium, Election Rules: `19.(1) The Returning Officer…` — glued
    /// onto an opening PAREN because the provision leads with its first
    /// sub-clause instead of a catchline. Eight rules were lost this way.
    #[test]
    fn restores_the_space_before_an_opening_sub_clause() {
        assert_eq!(
            normalize("19.(1) The Returning Officer, shall after the scrutiny"),
            "19. (1) The Returning Officer, shall after the scrutiny"
        );
        // the other seven shapes from the same document
        assert_eq!(normalize("24.(1) If there are more contesting"), "24. (1) If there are more contesting");
        assert_eq!(normalize("53.(1) Where, after scrutiny"), "53. (1) Where, after scrutiny");
        // roman and alpha sub-clauses open the same way
        assert_eq!(normalize("31.(i) The Returning Officer"), "31. (i) The Returning Officer");
        assert_eq!(normalize("44.(a) Documents to be sent"), "44. (a) Documents to be sent");
        // amendment-bracketed, as with the capital form
        assert_eq!(normalize("[12.(2) The Commission may"), "[12. (2) The Commission may");
    }

    /// The year guard must hold for the paren alternative too — a 4-digit
    /// ordinal followed by a paren is not a provision.
    #[test]
    fn does_not_split_a_year_from_a_parenthesis() {
        assert_eq!(normalize("1996.(a) was repealed"), "1996.(a) was repealed");
        assert_eq!(normalize("2013.(1) of the Act"), "2013.(1) of the Act");
    }

    /// Only line-initial ordinals are repaired — a decimal or a mid-sentence
    /// reference is left exactly as written (verbatim contract).
    #[test]
    fn leaves_mid_line_and_decimal_text_alone() {
        assert_eq!(normalize("see rule 12.Applies here"), "see rule 12.Applies here");
        assert_eq!(normalize("1.5 million rupees"), "1.5 million rupees");
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
    fn rc_h_joins_line_break_hyphen_with_surrounding_whitespace() {
        // RC-H: continuation line indented, and/or a trailing space after the
        // hyphen — the residual cases the tight `-\n` pattern missed.
        assert_eq!(normalize("Govern-\n        ment"), "Government");
        assert_eq!(normalize("auto- \n  matic"), "automatic");
        assert_eq!(normalize("proce-\n\tdure"), "procedure");
    }

    #[test]
    fn rc_h_joins_same_line_break_hyphen_for_known_words_only() {
        // Delegation's "Govern- ment" (hyphen + space, no newline) rejoins;
        // a real hyphenated compound is never in JOIN_TARGETS so it survives.
        assert_eq!(repair_spaced_words("the Govern- ment"), "the Government");
        assert_eq!(repair_spaced_words("provin-cial"), "provincial");
        assert_eq!(repair_spaced_words("Director-General"), "Director-General");
        assert_eq!(repair_spaced_words("sub-section"), "sub-section");
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

    #[test]
    fn collapses_runs_of_three_or_more_blank_lines_to_one() {
        let bloated = "Para A.\n\n\n\n\nPara B.";
        assert_eq!(normalize(bloated), "Para A.\n\nPara B.");
    }

    #[test]
    fn preserves_normal_paragraph_breaks() {
        let normal = "Para A.\n\nPara B.\n\nPara C.";
        assert_eq!(normalize(normal), normal);
    }

    #[test]
    fn collapses_blank_lines_with_trailing_spaces() {
        // PDF column gutters: blank lines that contain only spaces / tabs.
        let bloated = "Para A.\n   \n\t\n   \nPara B.";
        assert_eq!(normalize(bloated), "Para A.\n\nPara B.");
    }

    #[test]
    fn strips_leading_tabs_per_line() {
        let bloated = "\t\tRule 3. Body text.\n\tMore body.\nClean line.";
        assert_eq!(
            normalize(bloated),
            "Rule 3. Body text.\nMore body.\nClean line."
        );
    }

    #[test]
    fn collapses_three_or_more_inline_spaces_to_one() {
        let bloated = "Word1     Word2          Word3.";
        assert_eq!(normalize(bloated), "Word1 Word2 Word3.");
    }

    #[test]
    fn repairs_intra_word_spaces_into_known_words() {
        // US2/FR-004: stray mid-word spaces rejoin into known words.
        assert_eq!(repair_spaced_words("o ne hundred"), "one hundred");
        assert_eq!(repair_spaced_words("sha ll"), "shall");
        assert_eq!(repair_spaced_words("the Gov ernment"), "the Government");
        // through the full normalize() pipeline too:
        assert_eq!(normalize("Pay o ne hundred rupees"), "Pay one hundred rupees");
    }

    #[test]
    fn intra_word_repair_is_verbatim_safe() {
        // Never joins two legitimate adjacent words (despaced form not a target).
        assert_eq!(repair_spaced_words("in formation"), "in formation");
        assert_eq!(repair_spaced_words("the cat sat"), "the cat sat");
        // Idempotent + byte-unchanged on already-clean text.
        let clean = "one hundred shall be paid to the Government";
        assert_eq!(repair_spaced_words(clean), clean);
        assert_eq!(repair_spaced_words(&repair_spaced_words("sha ll")), "shall");
    }

    #[test]
    fn never_fuses_two_word_legal_phrase_into_one_token() {
        // 013/D9 regression: the quoted phrase "shall not" must survive verbatim —
        // it was previously fused to "shallnot" by a bad JOIN_TARGETS entry.
        assert_eq!(repair_spaced_words("shall not"), "shall not");
        assert_eq!(normalize("a citizen shall not be deprived"), "a citizen shall not be deprived");
        // Guard property: no JOIN_TARGETS entry may itself contain two words fused
        // (i.e. be a concatenation of two other listed single words across a space).
        for &t in JOIN_TARGETS {
            let fuses_a_pair = (1..t.len()).any(|i| {
                join_targets().contains(&t[..i]) && join_targets().contains(&t[i..])
            });
            assert!(
                !fuses_a_pair,
                "JOIN_TARGETS entry {t:?} fuses two real words across a space — forbidden (D9)"
            );
        }
    }

    #[test]
    fn intra_word_repair_preserves_punctuation_and_double_spaces() {
        // Trailing punctuation stays; ≥2-space boundaries are never collapsed.
        assert_eq!(repair_spaced_words("o ne, two"), "one, two");
        assert_eq!(repair_spaced_words("Short title  1."), "Short title  1.");
    }

    #[test]
    fn us8_tabifies_multi_row_table_gutters_keeping_cells_verbatim() {
        // Two adjacent multi-column rows (Delegation-of-Financial-Powers shape):
        // gutters become interior tabs so cells never fuse into a false clause,
        // while each cell's text is preserved byte-for-byte.
        let table = "11. Power to declare    Director General    Full powers\n\
                     12. Power to sanction    Secretary    Within budget\n";
        let out = normalize(table);
        assert!(out.contains("11. Power to declare\tDirector General\tFull powers"), "got: {out:?}");
        assert!(out.contains("12. Power to sanction\tSecretary\tWithin budget"), "got: {out:?}");
        // No cross-cell fusion: the fake sentence must not appear.
        assert!(!out.contains("declare Director General"), "cells fused: {out:?}");
    }

    #[test]
    fn us8_lone_bloated_line_still_collapses_to_single_space() {
        // A single multi-gutter line with no multi-gutter neighbour is prose,
        // not a table — original collapse behaviour is preserved.
        assert_eq!(normalize("Word1     Word2          Word3."), "Word1 Word2 Word3.");
    }

    #[test]
    fn preserves_two_space_boundaries_for_act_marginal_label_pattern() {
        // act.rs requires `\s{2,}` between marginal label and section number.
        // Two-space boundary must survive normalization.
        let act_form = "Short title  1. (1) This Ordinance ...";
        assert_eq!(normalize(act_form), "Short title  1. (1) This Ordinance ...");
    }
}
