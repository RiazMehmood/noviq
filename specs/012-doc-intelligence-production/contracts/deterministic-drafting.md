# Contract — Deterministic Drafting (US4)

**Modules:** `src-tauri/src/services/drafting/one_ask/{compose.rs, skeleton.rs,
verify.rs}` (evolve the US10 move-per-paragraph skeleton). Wave B — **no re-index**.

## Core
```rust
/// Compose from fixed slot+template frames. The LLM authors NO paragraph prose.
/// Slots filled deterministically from verified spans.
fn compose_deterministic(frames: &[Frame], slots: &Slots) -> Draft;

struct Slots {
    subject: VerbatimSpan,            // officer's verbatim subject
    provisions: Vec<(QuoteSpan, ResolvedLabel)>, // verbatim quote + resolved label
    figures: Vec<VerbatimSpan>,       // verbatim monetary/numeric
    stakeholders: Vec<ExtractedSpan>, // extracted, not authored
    urgency: Option<VerbatimSpan>,
}

/// Verification = decidable whitelist check: every token is either template
/// boilerplate, an allowed connector, or a verbatim slot value (G1).
fn verify_whitelist(draft: &Draft, slots: &Slots, connectors: &ConnectorWhitelist) -> VerifyResult;
```

## Invariants
- **No LLM-authored prose.** Composition is template + verbatim slots only. (Optional later concession: sentence-level connectors from a fixed whitelist — never free prose.)
- G1: every number / quote / reference / figure / cell in the draft is a verbatim substring of a source span.
- Verification is **decidable** (whitelist membership), not an open-ended fabrication judgment.

## Acceptance
- Zero fabricated glosses, zero advocacy, zero scaffolding leakage; prayer is one sentence.
- Every number/quote/ref passes G1; a non-whitelisted token fails the gate (refuse-or-flag).
- `cargo test --lib drafting::one_ask` green (slot fill, whitelist verify, G1 substring, conditional-financial omit).
