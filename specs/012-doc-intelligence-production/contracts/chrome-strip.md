# Contract — Chrome-Strip (US1)

**Module:** `src-tauri/src/services/parsers/chrome.rs` (new), wired into
`parsers/pdf.rs` Tier-1 between bbox extraction and tree-build.

## Pure core
```rust
pub enum BlockClass { Provision, Chrome, TableRow }
pub enum ChromeReason { Repetition, Margin, FormField }

/// Classify pre-tree blocks using generic structural signals (NO keyword lists).
/// Input: per-page words/blocks with geometry (from WordBox). Output: a class
/// per block. Pure; no IO. Conservative keep-bias.
pub fn classify_blocks(pages: &[PageBlocks]) -> Vec<BlockClass>;

/// Signal 1 (strongest): text recurring at the same vertical band across >= K
/// pages = Chrome. K = max(3, 40% of pages).
fn repeated_across_pages(blocks: &[Block], k: usize) -> HashSet<BlockId>;
```

## Invariants
- A block with a provision identity (heading marker / sequence-valid ordinal) is **never** classified `Chrome` (keep-bias).
- Chrome blocks are **kept** (persisted with `block_class='chrome'`, `is_boilerplate=TRUE`), never deleted, never attached under a provision node.
- No per-document keyword/string list anywhere in the module.

## Acceptance
- Masthead `Rule 345(3)(b)` notification, "P a g e" markers, and "Contact Us" chunks → `block_class='chrome'` (excluded from retrieval).
- Golden provisions unaffected (G2 diff clean).
- `cargo test --lib parsers::chrome` green (positional-repetition, margin, form-field, and provision-keep cases).
