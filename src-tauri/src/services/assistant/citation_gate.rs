//! T009 — model-independent **citation gate** (FR-006/FR-007), re-homed from the
//! one-ask drafting verify gate (`drafting/one_ask/verify.rs`) into the unified
//! assistant.
//!
//! The guarantee lives HERE, not in the prompt (ADR-0016, D2): every legal
//! reference the model emits MUST appear **verbatim** in the retrieved context
//! for this turn. Anything unbacked is ungrounded — flagged with a refuse-or-flag
//! message that offers to index the missing document, never shipped as fact.
//! Because the gate operates on retrieved text (not the model), it makes even a
//! weak/hot-swapped model safe.

use regex_lite::Regex;
use std::sync::OnceLock;

/// Whitespace/hyphen-tolerant normalisation (mirrors `one_ask::verify::normalize`):
/// collapse whitespace + hyphens and lowercase, so `Rule-6(c)` ≡ `Rule 6(c)` and
/// a line-wrap inside a provision label never defeats the verbatim check.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        let c = if c == '-' || c.is_whitespace() { ' ' } else { c };
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c.to_ascii_lowercase());
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// Provision-reference pattern (mirrors the decompose / verify label family,
/// hyphen-tolerant). Matches `Section 18`, `Rule-6(c)`, `Article 25A`, etc.
fn provision_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(article|rule|section|regulation|clause|sub-clause|schedule|chapter|notification|order|part)[-\s]+\d+[0-9A-Za-z()\-]*",
        )
        .expect("provision reference regex compiles")
    })
}

/// One ungrounded legal reference the model emitted that the retrieved context
/// does not support verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UngroundedRef {
    pub reference: String,
}

/// Result of gating an answer against the retrieved context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateResult {
    pub grounded: bool,
    /// Each ungrounded reference → a refuse-or-flag + offer-to-index (FR-007).
    pub ungrounded: Vec<UngroundedRef>,
}

/// True when `needle` appears verbatim (normalised) within `haystack`. Used by
/// capability handlers to validate any quoted span against its source passage.
pub fn appears_verbatim(needle: &str, haystack: &str) -> bool {
    let n = normalize(needle.trim_end_matches('…'));
    !n.trim().is_empty() && normalize(haystack).contains(&n)
}

/// Gate an answer: every provision reference it states MUST appear verbatim in
/// the retrieved `context`. Unbacked references are returned as ungrounded
/// (deduplicated). `grounded == true` ⇒ safe to ship as-is.
pub fn gate(answer: &str, context: &str) -> GateResult {
    let ctx = normalize(context);
    let mut ungrounded = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in provision_re().find_iter(answer) {
        let key = normalize(m.as_str());
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        if !ctx.contains(&key) {
            ungrounded.push(UngroundedRef { reference: m.as_str().to_string() });
        }
    }
    GateResult { grounded: ungrounded.is_empty(), ungrounded }
}

/// One retrieved passage, kept ATTACHED to the instrument it came from.
///
/// `gate` flattens every passage into one string, which makes the grounding
/// check instrument-blind: `Rule 4` from any of the eighteen documents in the
/// corpus satisfies a claim about `Rule 4` of one specific instrument. That is
/// the live form of the known "Rule 4 collides across 5+ instruments" problem —
/// and it is exactly how, on 2026-08-07, an answer quoting an unrelated
/// provision passed the gate while attributing it to the E&D Rules.
#[derive(Debug, Clone, Copy)]
pub struct ContextUnit<'a> {
    /// Display name of the document (`file_name` / `display_name`).
    pub instrument: &'a str,
    pub content: &'a str,
}

/// How far after a provision reference to look for an instrument name. Long
/// enough for "Rule 4 of the Sindh Civil Servants Efficiency and Discipline
/// Rules 1973", short enough not to reach the next sentence's instrument.
const INSTRUMENT_WINDOW_CHARS: usize = 90;

/// Instrument-scoped grounding.
///
/// For each provision reference the answer states, if the answer ALSO names a
/// known instrument right after it, the provision must appear in a passage from
/// **that** instrument. When no instrument is named the check degrades to
/// `gate`'s flat behaviour, so this can only ever be stricter, never looser.
///
/// Deliberately NOT a semantic-support check: it stays deterministic and
/// model-independent (ADR-0016 D2). It closes the "right label, wrong statute"
/// hole, not the "right statute, paraphrased wrongly" one.
pub fn gate_scoped(answer: &str, units: &[ContextUnit]) -> GateResult {
    let flat = normalize(&units.iter().map(|u| u.content).collect::<Vec<_>>().join(" "));
    // Per-instrument haystacks, normalised once.
    let mut by_instrument: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for u in units {
        by_instrument
            .entry(normalize(u.instrument))
            .or_default()
            .push_str(&format!(" {}", normalize(u.content)));
    }

    let mut ungrounded = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in provision_re().find_iter(answer) {
        let key = normalize(m.as_str());
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        // Which instrument (if any) does the answer attribute this to? Look only
        // at the text immediately following the reference.
        let tail_start = m.end();
        let tail: String = answer[tail_start..]
            .chars()
            .take(INSTRUMENT_WINDOW_CHARS)
            .collect();
        let tail_n = normalize(&tail);
        let named = by_instrument
            .keys()
            .filter(|inst| !inst.is_empty() && tail_n.contains(inst.as_str()))
            // Longest instrument name wins, so "SPPRA Rules 2010" is not
            // shadowed by a shorter name that happens to be a prefix.
            .max_by_key(|inst| inst.len())
            .cloned();

        let supported = match named {
            Some(inst) => by_instrument
                .get(&inst)
                .map(|h| h.contains(&key))
                .unwrap_or(false),
            None => flat.contains(&key),
        };
        if !supported {
            ungrounded.push(UngroundedRef { reference: m.as_str().to_string() });
        }
    }
    GateResult { grounded: ungrounded.is_empty(), ungrounded }
}

/// Result of suppressing ungrounded claims from an answer (T018).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redaction {
    /// The answer with every sentence asserting an ungrounded provision removed.
    pub answer: String,
    /// The suppressed references (each → an offer-to-index flag).
    pub ungrounded: Vec<UngroundedRef>,
}

/// Split prose into sentence-ish units (on `.`/`!`/`?` followed by whitespace,
/// and on newlines) so suppression removes a whole claim, not a fragment.
fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        let boundary = matches!(c, '.' | '!' | '?' | '\n')
            && chars.peek().map_or(true, |n| n.is_whitespace());
        if boundary {
            let s = cur.trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
            cur.clear();
        }
    }
    let tail = cur.trim().to_string();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

/// Suppress (do not ship) every sentence stating a provision reference the
/// retrieved context does not support verbatim (FR-006/FR-007), returning the
/// cleaned answer plus the suppressed references. A fully grounded answer is
/// returned unchanged. This is the model-independent enforcement point: even a
/// weak/hot-swapped model cannot leak a fabricated citation into the prose.
pub fn redact_ungrounded(answer: &str, context: &str) -> Redaction {
    let result = gate(answer, context);
    if result.grounded {
        return Redaction { answer: answer.to_string(), ungrounded: result.ungrounded };
    }
    let keys: Vec<String> =
        result.ungrounded.iter().map(|u| normalize(&u.reference)).collect();
    let kept: Vec<String> = split_sentences(answer)
        .into_iter()
        .filter(|s| {
            let ns = normalize(s);
            !keys.iter().any(|k| !k.is_empty() && ns.contains(k))
        })
        .collect();
    Redaction { answer: kept.join(" "), ungrounded: result.ungrounded }
}

/// Refuse-or-flag message for an ungrounded reference (FR-007): states the gap
/// honestly and offers to index the missing document.
pub fn flag_message(r: &UngroundedRef) -> String {
    format!(
        "I could not find \"{}\" in the indexed documents, so I will not state it \
         as fact. If you have the source document, index it and I will cite it precisely.",
        r.reference
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounded_reference_passes() {
        let ctx = "Section 18 of the Act provides that the council shall...";
        let ans = "Under Section 18 you must apply in writing.";
        let r = gate(ans, ctx);
        assert!(r.grounded, "{r:?}");
        assert!(r.ungrounded.is_empty());
    }

    #[test]
    fn ungrounded_reference_is_flagged_with_offer_to_index() {
        let ctx = "Section 18 of the Act provides...";
        let ans = "This is governed by Section 99 and Article 4.";
        let r = gate(ans, ctx);
        assert!(!r.grounded);
        let refs: Vec<&str> = r.ungrounded.iter().map(|u| u.reference.as_str()).collect();
        assert!(refs.iter().any(|s| s.eq_ignore_ascii_case("Section 99")), "{refs:?}");
        assert!(refs.iter().any(|s| s.eq_ignore_ascii_case("Article 4")), "{refs:?}");
        let msg = flag_message(&r.ungrounded[0]);
        assert!(msg.contains("could not find"));
        assert!(msg.to_lowercase().contains("index"));
    }

    #[test]
    fn verbatim_check_is_hyphen_and_whitespace_tolerant() {
        let ctx = "Rule-6(c)  The competent\nauthority may act.";
        assert!(appears_verbatim("Rule 6(c) The competent authority", ctx));
        assert!(!appears_verbatim("Rule 6(c) shall never apply", ctx));
        // a hyphenated label in the answer still matches the context
        let r = gate("Per Rule 6(c) you proceed.", ctx);
        assert!(r.grounded, "{r:?}");
    }

    #[test]
    fn duplicate_ungrounded_reference_reported_once() {
        let ctx = "nothing relevant here";
        let ans = "Section 5 says X. Later, Section 5 also says Y.";
        let r = gate(ans, ctx);
        assert_eq!(r.ungrounded.len(), 1, "{r:?}");
    }

    #[test]
    fn answer_with_no_legal_reference_is_grounded() {
        let r = gate("You should apply in writing to your department.", "");
        assert!(r.grounded);
    }

    #[test]
    fn redact_suppresses_only_the_ungrounded_sentence() {
        let ctx = "Section 18 of the Act provides for leave.";
        let ans = "Under Section 18 you apply in writing. This is also governed by Section 99.";
        let red = redact_ungrounded(ans, ctx);
        // grounded sentence survives, fabricated one is gone
        assert!(red.answer.contains("Section 18"), "{red:?}");
        assert!(!red.answer.contains("Section 99"), "fabricated claim must be suppressed: {red:?}");
        assert_eq!(red.ungrounded.len(), 1);
        // the surviving prose itself re-gates as grounded (no fabricated ref leaks)
        assert!(gate(&red.answer, ctx).grounded);
    }

    // ── gate_scoped: instrument-aware grounding ─────────────────────────────

    fn unit<'a>(instrument: &'a str, content: &'a str) -> ContextUnit<'a> {
        ContextUnit { instrument, content }
    }

    /// THE HOLE. Two instruments both have a "Rule 4". The flat gate is happy
    /// because `Rule 4` appears *somewhere*; the scoped gate is not, because it
    /// does not appear in the instrument the answer names.
    #[test]
    fn scoped_gate_rejects_a_provision_borrowed_from_another_instrument() {
        let units = [
            unit("Sindh Civil Servants APT Rules 1974", "Rule 4 Appointments shall be made..."),
            unit("Sindh Civil Servants Efficiency and Discipline Rules 1973", "Rule 1 Short title. Preamble..."),
        ];
        let ans = "Rule 4 of the Sindh Civil Servants Efficiency and Discipline Rules 1973 \
                   provides for penalties.";

        // The existing flat gate passes this — that is the defect.
        let flat_ctx = units.iter().map(|u| u.content).collect::<Vec<_>>().join(" ");
        assert!(gate(ans, &flat_ctx).grounded, "flat gate should (wrongly) pass");

        let r = gate_scoped(ans, &units);
        assert!(!r.grounded, "scoped gate must reject a cross-instrument borrow: {r:?}");
        assert_eq!(r.ungrounded.len(), 1);
    }

    /// The same reference, correctly attributed, must still pass.
    #[test]
    fn scoped_gate_accepts_a_correctly_attributed_provision() {
        let units = [
            unit("Sindh Civil Servants APT Rules 1974", "Rule 4 Appointments shall be made..."),
            unit("Sindh Civil Servants Efficiency and Discipline Rules 1973", "Rule 4 Penalties..."),
        ];
        let ans = "Rule 4 of the Sindh Civil Servants Efficiency and Discipline Rules 1973 \
                   provides for penalties.";
        assert!(gate_scoped(ans, &units).grounded);
    }

    /// When the answer names no instrument, behaviour must be IDENTICAL to the
    /// flat gate — this can only ever be stricter, never looser.
    #[test]
    fn scoped_gate_degrades_to_flat_when_no_instrument_is_named() {
        let units = [unit("Some Act 1999", "Section 18 provides for leave.")];
        let flat_ctx = "Section 18 provides for leave.";
        for ans in ["Under Section 18 you apply in writing.", "See Section 99 instead."] {
            assert_eq!(
                gate_scoped(ans, &units).grounded,
                gate(ans, flat_ctx).grounded,
                "scoped and flat disagreed with no instrument named: {ans}",
            );
        }
    }

    /// A longer instrument name must win over a shorter one it contains, or the
    /// scoping silently resolves to the wrong document.
    #[test]
    fn scoped_gate_prefers_the_longest_matching_instrument_name() {
        let units = [
            unit("SPPRA Rules", "Rule 15 something unrelated."),
            unit("SPPRA Rules 2010 Amended 15 Jul 2022", "Rule 17 open competitive bidding."),
        ];
        let ans = "Rule 17 of the SPPRA Rules 2010 Amended 15 Jul 2022 sets the method.";
        assert!(gate_scoped(ans, &units).grounded, "longest instrument name must win");
    }

    /// An instrument named far away (next sentence) must not be captured.
    #[test]
    fn scoped_gate_does_not_reach_into_the_next_sentence() {
        let units = [
            unit("Act A", "Section 7 text here."),
            unit("Act B", "nothing relevant"),
        ];
        // "Section 7" is followed immediately by prose, and Act A appears only
        // much later — beyond the window — so this falls back to the flat check.
        let filler = "x".repeat(INSTRUMENT_WINDOW_CHARS + 20);
        let ans = format!("Section 7 applies. {filler} This is from Act A.");
        assert!(gate_scoped(&ans, &units).grounded, "should fall back to flat, not reject");
    }

    #[test]
    fn redact_leaves_a_fully_grounded_answer_unchanged() {
        let ctx = "Rule 6(c) sets the timeline.";
        let ans = "Per Rule 6(c) you proceed within the stated time.";
        let red = redact_ungrounded(ans, ctx);
        assert_eq!(red.answer, ans);
        assert!(red.ungrounded.is_empty());
    }
}
