//! Pure scoring function over extracted text.
//! Detects scanned PDFs (chars/page) and font-corrupted PDFs
//! (non-ASCII-punct ratio, dict-hit ratio, mean token length).

use std::collections::HashSet;
use std::sync::OnceLock;

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

const CHARS_PER_PAGE_MIN: f32 = 10.0;
const NON_ASCII_PUNCT_RATIO_MAX: f32 = 0.05;
const DICT_HIT_RATIO_MIN: f32 = 0.60;
const MEAN_TOKEN_LEN_MIN: f32 = 2.5;
/// Per-line fragmentation thresholds. A re-extractable PDF whose text is split
/// one-token-per-line trips `Suspect` so a cleaner tier (pdftotext/OCR) is
/// tried. Measured margin: lopdf 42% tiny lines vs pdftotext -layout 2%.
const TINY_LINE_RATIO_MAX: f32 = 0.30;
const FRAGMENT_MIN_LINES: usize = 20;

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

    // Per-line fragmentation signal (US2 root cause). Some extractors (lopdf on
    // column-laid PDFs) shatter words one-per-line ("Procurements over\no\nne\n
    // hundred thousand"). The token-level checks below miss this because they
    // split across newlines, so a fractured page still tokenises into mostly
    // whole words. Measure it directly: clean extractors keep full lines
    // (pdftotext -layout on a real SPPRA page: ~2% of lines ≤3 chars, mean 71
    // chars), whereas the same page via lopdf is 42% tiny lines, mean 16. A high
    // tiny-line ratio means a cleaner tier should win. Generic — no
    // document-specific terms. Gated on ≥ MIN line count so short snippets
    // (and unit fixtures) are never judged on line shape.
    let non_empty: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if non_empty.len() >= FRAGMENT_MIN_LINES {
        let tiny = non_empty.iter().filter(|l| l.chars().count() <= 3).count() as f32;
        if tiny / non_empty.len() as f32 > TINY_LINE_RATIO_MAX {
            return Quality { score: 0.35, verdict: Verdict::Suspect };
        }
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

#[cfg(test)]
mod dict_tests {
    use super::*;

    #[test]
    fn clean_legal_passes_dict_hit() {
        let txt = "The Government of Sindh, in exercise of the powers \
                   conferred by the Constitution, hereby promulgates these \
                   rules for the appointment, promotion, and transfer of \
                   civil servants in the province.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Good);
    }

    #[test]
    fn garbage_tokens_trip_dict_hit() {
        let txt = "Xjkv qrwn flmtp gzbv hxnq jklm wprs tvxz qntl bzkj \
                   xjkv qrwn flmtp gzbv hxnq jklm wprs tvxz qntl bzkj.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Suspect);
    }

    #[test]
    fn fragmented_text_trips_mean_token_len() {
        let txt = "a b c d e f g h i j k l m n o p q r s t u v w x y z \
                   a b c d e f g h i j k l m n o p q r s t u v w x y z";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Suspect);
    }
}

#[cfg(test)]
mod fragmentation_tests {
    use super::*;

    #[test]
    fn per_line_column_explosion_trips_suspect() {
        // lopdf column-explosion shape: words shattered one-per-line, the way
        // "Procurements over one hundred thousand rupees" is actually stored.
        // ~36% of lines are ≤3 chars over many lines ⇒ Suspect so pdftotext wins.
        let block = "Procurements over\no\nne\nhundred thousand\nrupees\nand up\nto\none\nmillion\nrupees\nshall be advertised by timely notifications\n";
        let txt = block.repeat(10);
        let q = score(&txt, 1);
        assert_eq!(q.verdict, Verdict::Suspect);
    }

    #[test]
    fn clean_full_line_text_stays_good() {
        // pdftotext -layout shape: full lines, ~2% tiny. Must remain Good.
        let line = "Procurements over one hundred thousand rupees and up to one million rupees shall be advertised by timely notifications on the Authority website.\n";
        let txt = line.repeat(30);
        let q = score(&txt, 1);
        assert_eq!(q.verdict, Verdict::Good);
    }

    #[test]
    fn short_clean_snippet_not_judged_on_line_shape() {
        // Under FRAGMENT_MIN_LINES — line-shape gate must not fire on snippets
        // even if a couple of lines are short.
        let txt = "Rule 1.\nThe Government shall arrange business as required by law.\nRule 2.\nThe Secretary shall record minutes of every meeting held.";
        let q = score(txt, 1);
        assert_eq!(q.verdict, Verdict::Good);
    }
}
