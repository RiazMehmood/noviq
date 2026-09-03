pub mod regex_pass;
pub mod llm_pass;
pub mod tree;
/// Golden extraction set (2026-07-30): the real pages that have broken this
/// pipeline, with what the scan must and must not make of them. Test-only —
/// the fast loop that replaces "re-index prod and read SQL".
mod golden;
