# Contracts — Internal Ingestion Stage Interfaces

This feature has **no external/REST API** — it is a deterministic library pipeline. The "contracts"
here are the **stage boundaries** inside `src-tauri/src/services` and the invariants each stage must
uphold. Each is independently testable (`cargo test --lib`), and the whole chain is gated by the
frozen-fixture acceptance test. Signatures are illustrative of intent (evolve existing functions; do
not rewrite).

```
pdftotext (-bbox geometry) ─▶ Zoning ─▶ Structural parse ─▶ Vocabulary resolve ─▶ Persist ─▶ US5 graph capture
```

---

## C1. Zoning (P1 / D5, D6) — `parsers/chrome.rs`

**Contract**: given pre-tree `Block`s with geometry (`Block.y_band`, cross-page repetition), classify
each as `provision` | `chrome` (incl. footer page-number) | `footnote`.

```rust
// classify_blocks already exists; extend its BlockClass space to include Footnote.
pub fn classify_blocks(blocks: &[Block], total_pages: usize) -> Vec<Classification>;
```

- **MUST** use geometry (footer y-band + integer-increments-across-pages), never a bare-integer text
  regex (legal text has bare integers). [research D-2]
- **MUST NOT** reclassify a block carrying `has_provision_identity` as chrome/footnote (existing
  invariant `provision_identity_overrides_every_signal`).
- **Footnote** blocks are kept (retrievable), not dropped.
- **Test**: amendment-history strings ("Subs. by the Constitution", "ins. ibid", "w.e.f") never land
  in a `provision` block; no `provision` block equals/contains a footer page integer.

## C2. Structural parse (P2 / D2, D3, D4, D10) — `heading_detector/regex_pass.rs` + `structure_parser.rs`

**Contract**: produce a `DocNode` tree of identifier + `NodeLevel` only (no citation word).

- Article detection: `^\s*(\d+[A-Z]?)\.\s+\S` capturing `7`,`10A`,`19A`,`25A`, **guarded by sequence
  validation** (next-in-sequence / restart heuristics) so it doesn't over-fire on enumerated lists.
- Chapter detection: `^\s*CHAPTER\s+\d+` (tolerant of `–`/`—`/`-` + trailing title) → real `Chapter`
  node inserted between Part and Article. (Article 7 attaches directly to Part II.)
- Catchline↔body merge into one Article node (title metadata + body), **source spans preserved**.
- `unit_type` correct (catchlines `article`, not `clause`).
- **MUST NOT** auto-correct genuine source disorder (25/25A) → flag `unresolved`.
- **Test**: all Articles 7–40 (incl. 10A/19A/25A) present as Article nodes with correct parentage;
  no orphan bare-label chunks; verbatim-substring holds.

## C3. Vocabulary resolve (P3 / D1) — `doc_profiles/mod.rs` (resolver) + `generic.rs`

**Contract**: bind the citation word from layered evidence; conflict → unresolved.

```rust
enum Vocab { Word(String), Unresolved }
fn resolve_citation_word(node: &DocNode, profile: &Profile, doc_evidence: &Evidence) -> Vocab;
```

- Layers: profile → internal linguistic evidence → user override → `Unresolved`.
- `generic.rs` **MUST NOT** mint `Section {num}` during detection; the word is applied here.
- **Test**: no chunk emits `Section N` for an Articles document; emits `Article N` or `unresolved`,
  never the wrong word.

## C4. Persist (P0 / D12; P1 footnotes) — `structured_importer.rs` + `structure_parser::persist_tree`

**Contract**: write chunks with full provenance.

```rust
pub async fn persist_tree(pool, source_id, tree, page_offsets: &[(u32, usize)]) -> Result<Vec<Chunk>>;
async fn persist_footnote_chunks(pool, source_id, footnotes, base_index) -> Result<usize>;
```

- `page_number` set for every provision via `page_offsets` (monotonic, in range).
- Footnotes persisted with `block_class='footnote'`, `page_number`, anchor (or `unresolved`).
- **Test**: 100% provisions have valid page_number; footnote chunks exist and are retrievable.

## C5. Text normalization (P0 / D9) — `text_normalizer.rs`

**Contract**: repair only true intra-word splits; never fuse two real words across a legitimate space.

```rust
pub(crate) fn repair_spaced_words(input: &str) -> String; // JOIN_TARGETS audited
```

- **Test**: "shall not" stays "shall not"; "o ne"→"one", "sha ll"→"shall" still work; guard test
  rejects any 2-token legal phrase → non-dictionary single token.

## C6. US5 graph capture (FR-014, FR-015) — `graph_builder.rs` + `legal_ir/amendment_parser.rs`

**Contract**: emit verbatim relationships into existing graph store; flag, never invent.

```rust
fn cross_reference_edges(tree: &DocNode, nodes: &[GraphNode]) -> Vec<GraphEdge>;      // edge_type='cross_reference'
fn amendment_events_from_footnotes(footnotes: &[Footnote], nodes: &[GraphNode])
    -> (Vec<AmendmentEvent>, Vec<GraphEdge> /* amended_by */);
```

- Identifiers verbatim substrings of source; unresolvable target/anchor → flagged, not invented.
- **Test**: an Eighteenth-Amendment footnote → amendment_event on the right provision; an in-text
  Article-N reference → `cross_reference` edge between the two provisions; zero synthesized identifiers.

---

## Acceptance contract (FR-012) — `src-tauri/tests/ingestion_hardening_acceptance.rs`

Frozen fixture = 177 chunks of `fundamental_rights.pdf` + Doc 1 prose + Doc 2 table. CI **MUST FAIL**
if any of SC-001…SC-009 regress (article presence, chapter parentage, vocabulary, no-corruption
property test, footnote isolation, chrome/page-number exclusion, page_number validity, 25/25A flagged,
US5 verbatim links). No LLM dependency in the test.
