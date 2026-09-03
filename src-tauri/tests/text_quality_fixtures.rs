//! Fixture-driven sanity checks for text_quality::score.
//! Inline unit tests live alongside the implementation; these
//! tests prove the same behaviour against on-disk fixtures so
//! the failure modes are documented in plain text.

use echo_ai_lib::services::parsers::text_quality::{score, Verdict};

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
