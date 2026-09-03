//! Doc-type profiles — per-document-type level vocabulary and citation rules.
//! v1 (Feature 003) shipped `constitution`. Feature 005 adds `act`, `rules`,
//! `manual`, `generic` with detection + heading regex patterns so unstructured
//! PDFs flow through the same structure_parser path.

pub mod constitution;
pub mod act;
pub mod rules;
pub mod manual;
pub mod generic;

use crate::services::structure_parser::NodeLevel;

/// A document-type profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocProfile {
    pub doc_type: &'static str,
    pub level_vocab: [&'static str; 6],
    pub citation_unit: &'static str,
    pub salient_units: &'static [&'static str],

    /// Patterns scored when picking a profile for an unknown document.
    pub detection_patterns: &'static [DetectionPattern],
    /// Patterns used to build the heading tree once the profile is picked.
    pub heading_patterns:   &'static [HeadingPattern],
    /// Detection score below this → fall through to next profile / generic.
    /// 0.0 disables the floor (used by `generic`).
    pub min_confidence:     f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionPattern {
    pub regex:  &'static str,
    /// Weight added to the profile's score per match. Integer to keep `Eq`.
    pub weight: u32,
    pub level:  NodeLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingPattern {
    pub regex:        &'static str,
    pub level:        NodeLevel,
    /// Display template for the citation, with `{num}` / `{title}` / `{sub}` /
    /// `{clause}` placeholders filled from named capture groups.
    pub citation_fmt: &'static str,
}

/// All profiles in detection priority order. `generic` is last (always matches).
pub const REGISTRY: &[&DocProfile] = &[
    &constitution::CONSTITUTION,
    &act::ACT,
    &rules::RULES,
    &manual::MANUAL,
    &generic::GENERIC,
];

/// Returns true when `label` is a salient unit in any registered DocProfile.
pub fn is_salient_unit(label: &str) -> bool {
    REGISTRY.iter().any(|p| p.salient_units.iter().any(|&u| u == label))
}

/// Where the resolved doc_type came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionSource { Auto, Llm, User }

impl DetectionSource {
    pub fn as_str(self) -> &'static str {
        match self { Self::Auto => "auto", Self::Llm => "llm", Self::User => "user" }
    }
}

/// Pick the best-matching profile for `text`. If `force` is `Some`, returns
/// that profile with `DetectionSource::User`. Constitution is excluded from
/// auto-detection — it routes via `try_load_constitution` before this is
/// called.
pub fn detect_profile(text: &str, force: Option<&str>) -> (&'static DocProfile, DetectionSource) {
    use regex_lite::Regex;

    if let Some(name) = force {
        for &p in REGISTRY {
            if p.doc_type == name {
                return (p, DetectionSource::User);
            }
        }
    }

    let mut best: Option<(&'static DocProfile, u32)> = None;
    for &p in REGISTRY {
        if p.doc_type == "constitution" { continue; }
        let mut score: u32 = 0;
        for pat in p.detection_patterns {
            let re = Regex::new(pat.regex).expect("invalid regex in profile");
            score = score.saturating_add(pat.weight * (re.find_iter(text).count() as u32));
        }
        if (score as f32) >= p.min_confidence {
            match best {
                None => best = Some((p, score)),
                Some((_, s)) if score > s => best = Some((p, score)),
                _ => {}
            }
        }
    }
    let (winner, _) = best.unwrap_or((&generic::GENERIC, 0));
    (winner, DetectionSource::Auto)
}

/// Classify a document from its title/filename alone, when the title names the
/// instrument type unambiguously. This is deliberately AUTHORITATIVE over the
/// body-word scoring of [`detect_profile`], because body frequency misleads on
/// two live cases (2026-07-11 prod audit):
///  * `03_Sindh_Public_Procurement_Act_2009` (an **Act**) was scored `manual` —
///    its body repeats "instructions" / uppercase `CHAPTER` headings and never
///    says "Ordinance", so the manual profile out-scored act.
///  * `05_Compendium_Rules_under_SLGA_2013` (legitimately **rules**) says
///    "this Act" 47× because it is a compendium *of rules made under an Act* —
///    any "this Act" body signal would wrongly pull it to `act`.
///
/// The title has none of that noise. Priority resolves titles that name more
/// than one type ("Rules under the X **Act**" → `rules`): the most specific
/// derivative instrument wins — rules/regulations, then manual, then
/// act/ordinance. Returns `None` when the title states no type (→ fall back to
/// body scoring).
pub fn doc_type_from_title(title: &str) -> Option<&'static str> {
    // Tokenize on any non-alphanumeric so filename separators (`_`, `-`, `.`,
    // space) all delimit words. A regex `\b` is WRONG here: `\b` treats `_` as a
    // word char, so `\bAct\b` fails to match `..._Act_2009...` — the exact live
    // filenames are underscore-separated.
    let toks: Vec<&str> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let has = |w: &str| toks.iter().any(|t| t.eq_ignore_ascii_case(w));
    if has("rules") || has("regulations") || has("regulation") {
        Some("rules")
    } else if has("manual") {
        Some("manual")
    } else if has("act") || has("ordinance") {
        Some("act")
    } else {
        None
    }
}

/// Like [`detect_profile`], but consults the document **title** first. An
/// explicit `force` still wins (user override); otherwise a title that names the
/// instrument type is authoritative (see [`doc_type_from_title`]); only a
/// type-less title falls through to body-word scoring. This is the entry point
/// the ingest path uses so a document is classified by what it *is* (its title),
/// not by which words its body happens to repeat.
pub fn detect_profile_titled(
    text: &str,
    title: &str,
    force: Option<&str>,
) -> (&'static DocProfile, DetectionSource) {
    if force.is_some() {
        return detect_profile(text, force);
    }
    if let Some(dt) = doc_type_from_title(title) {
        if let Some(&p) = REGISTRY.iter().find(|p| p.doc_type == dt) {
            return (p, DetectionSource::Auto);
        }
    }
    detect_profile(text, None)
}

// ── Feature 013 / Phase 5 (D1) — citation-unit vocabulary resolver ───────────

/// The citation unit word resolved for a document. A document's provisions carry
/// ONE unit word ("Article" / "Section" / "Rule"); the profile only supplies a
/// *default* guess (e.g. `manual` defaults to "Section"). Internal
/// self-references ("in this Article", "Article 251") are stronger evidence of
/// the real word and override the default. When two units are both strongly
/// attested the word is genuinely ambiguous and the caller must flag
/// `label_status='unresolved'` rather than mint the wrong word (refuse-or-flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationUnit {
    /// A confident unit word to apply to provision labels.
    Word(&'static str),
    /// Evidence conflicts — caller leaves the default but flags the provisions.
    Unresolved,
}

/// Candidate provision unit words. Each must be a `'static` string so a resolved
/// override can flow into a `&'static` label template.
const UNIT_CANDIDATES: &[&str] = &["Article", "Section", "Rule", "Regulation"];

/// Minimum self-reference hits before internal evidence is trusted over the
/// profile default. Below this the document simply doesn't talk about its own
/// units enough to overrule the profile.
const UNIT_MIN_EVIDENCE: u32 = 3;

/// Evidence that a document calls its OWN provisions `unit`. Returns
/// `(self_ref, bare_ref)`:
///   - `self_ref` = "this <Unit>" — an unambiguous SELF reference;
///   - `bare_ref` = "<Unit> 12" — weaker, since it may be a CROSS reference to
///     another instrument ("Article 5 of the Agreement").
/// The doc-06 misfire was exactly this: 5 bare "Article N" cross-refs (no "this
/// Article") wrongly flipped a development-projects manual from Section→Article.
fn unit_evidence(text: &str, unit: &str) -> (u32, u32) {
    use regex_lite::Regex;
    let count = |pat: String| {
        Regex::new(&pat).map(|re| re.find_iter(text).count() as u32).unwrap_or(0)
    };
    let self_ref = count(format!(r"(?i)\bthis\s+{unit}\b"));
    let bare_ref = count(format!(r"(?i)\b{unit}\s+\d"));
    (self_ref, bare_ref)
}

/// Weighted score: a self-reference counts much more than a bare reference.
fn unit_score(ev: (u32, u32)) -> u32 {
    ev.0.saturating_mul(3).saturating_add(ev.1)
}

/// Resolve the citation unit word for `text` under `profile` (Phase 5 / D1).
/// Layering (FR): profile default → internal linguistic evidence → unresolved on
/// genuine conflict. Overriding the default requires either a genuine
/// SELF-reference ("this <Unit>") or overwhelming bare evidence — so a handful of
/// cross-references to other instruments never flips the word (doc-06).
pub fn resolve_citation_unit(text: &str, profile: &DocProfile) -> CitationUnit {
    let default: &'static str = UNIT_CANDIDATES
        .iter()
        .copied()
        .find(|&u| u.eq_ignore_ascii_case(profile.citation_unit))
        .unwrap_or("Section");

    // (unit, self_ref, score) for the top two by score.
    let mut best: (&'static str, u32, u32) = (default, 0, 0);
    let mut runner_score: u32 = 0;
    for &u in UNIT_CANDIDATES {
        let ev = unit_evidence(text, u);
        let s = unit_score(ev);
        if s > best.2 {
            runner_score = best.2;
            best = (u, ev.0, s);
        } else if s > runner_score {
            runner_score = s;
        }
    }

    // Too little evidence, or evidence agrees with the default → trust default.
    if best.2 < UNIT_MIN_EVIDENCE || best.0.eq_ignore_ascii_case(default) {
        return CitationUnit::Word(default);
    }
    // Evidence points to a DIFFERENT word.
    // Genuine conflict: the runner-up is also strongly attested and close.
    if runner_score >= UNIT_MIN_EVIDENCE && best.2 < runner_score.saturating_mul(2) {
        return CitationUnit::Unresolved;
    }
    // Override only with a real self-reference + dominance, OR overwhelming bare
    // evidence; otherwise the signal is cross-references → keep the profile default.
    let self_referenced = best.1 >= 1 && best.2 >= runner_score.saturating_mul(2);
    let overwhelming = best.2 >= 10 && best.2 >= runner_score.saturating_mul(3);
    if self_referenced || overwhelming {
        CitationUnit::Word(best.0)
    } else {
        CitationUnit::Word(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constitution_profile_has_empty_detection_and_heading_patterns() {
        let p = constitution::CONSTITUTION;
        assert!(p.detection_patterns.is_empty());
        assert!(p.heading_patterns.is_empty());
        assert!(p.min_confidence >= 0.0 && p.min_confidence <= 1.0);
    }

    #[test]
    fn picks_rules_for_rule_dense_text() {
        let text = "
Rule 1. Short title.
Rule 2. Definitions.
Rule 9. The Secretary shall arrange...
Rule 12. Reconciliation.
";
        let (profile, source) = detect_profile(text, None);
        assert_eq!(profile.doc_type, "rules");
        assert_eq!(source, DetectionSource::Auto);
    }

    #[test]
    fn picks_act_for_section_dense_text() {
        let text = "
Section 1. Short title.
Section 2. Definitions.
Section 3. Removal from service.
Section 4. Procedure.
Section 5. Appeal.
";
        let (profile, _) = detect_profile(text, None);
        assert_eq!(profile.doc_type, "act");
    }

    #[test]
    fn picks_generic_when_nothing_matches() {
        let text = "Just some paragraphs.\nNo legal headings at all.";
        let (profile, source) = detect_profile(text, None);
        assert_eq!(profile.doc_type, "generic");
        assert_eq!(source, DetectionSource::Auto);
    }

    #[test]
    fn user_override_wins() {
        let text = "Section 1. ... Section 2. ...";
        let (profile, source) = detect_profile(text, Some("rules"));
        assert_eq!(profile.doc_type, "rules");
        assert_eq!(source, DetectionSource::User);
    }

    #[test]
    fn title_classifies_real_corpus_filenames() {
        // The exact live filenames from the 2026-07-11 prod audit.
        let cases = [
            // The two documents body-scoring got WRONG:
            ("03_Sindh_Public_Procurement_Act_2009.pdf", Some("act")), // was manual
            ("01_Sindh_Rules_of_Business_1986.pdf", Some("rules")),    // was manual
            // The regression trap: a rules compendium that says "this Act" 47×.
            ("05_Compendium_Rules_under_SLGA_2013.pdf", Some("rules")),
            // Everything already-correct must stay correct:
            ("04_Sindh_Local_Government_Act_2013.pdf", Some("act")),
            ("Removal_from_Service_Special_Powers_Sindh_Ordinance_2000.pdf", Some("act")),
            ("02_SPPRA_Rules_2010_Revised_2013.pdf", Some("rules")),
            ("11_Sindh_Civil_Servants_APT_Rules_1974.pdf", Some("rules")),
            ("06_Manual_for_Development_Projects_2019_PC-I.pdf", Some("manual")),
            ("Govt. of Sindh Manual of Secretariat instruction 2010.pdf", Some("manual")),
            // Type-less titles → None → fall back to body scoring:
            ("09_Sindh_Delegation_of_Financial_Powers_2019.pdf", None),
            ("quota-a.pdf", None),
            ("constitution.pdf", None),
        ];
        for (name, want) in cases {
            assert_eq!(doc_type_from_title(name), want, "title={name}");
        }
    }

    #[test]
    fn titled_detection_overrides_misleading_body() {
        // Body screams "manual" (Manual/instruction/Secretariat) but the title
        // says Act → title wins.
        let body = "Manual instructions to the Secretariat. Manual. Instructions.";
        let (p, src) =
            detect_profile_titled(body, "Sindh Public Procurement Act 2009", None);
        assert_eq!(p.doc_type, "act");
        assert_eq!(src, DetectionSource::Auto);

        // Rules compendium quoting "this Act" a lot, but titled a Compendium of
        // Rules → stays rules (the regression guard).
        let body = "under this Act, this Act, this Act the powers are delegated";
        let (p, _) =
            detect_profile_titled(body, "Compendium of Rules under SLGA 2013", None);
        assert_eq!(p.doc_type, "rules");

        // A user force still wins over the title.
        let (p, src) =
            detect_profile_titled("whatever", "X Manual", Some("act"));
        assert_eq!(p.doc_type, "act");
        assert_eq!(src, DetectionSource::User);

        // Type-less title → body scoring decides.
        let (p, _) = detect_profile_titled("Rule 1. Rule 2. Rule 3.", "quota-a.pdf", None);
        assert_eq!(p.doc_type, "rules");
    }

    /// Phase 5 / D1: a document classified `manual` (default unit "Section") that
    /// refers to its own provisions as Articles must resolve to "Article", not
    /// leak the wrong word — the live fundamental_rights.pdf failure.
    #[test]
    fn resolver_overrides_section_default_when_doc_uses_articles() {
        let text = "8. Laws inconsistent with Fundamental Rights.\n\
                    In this Article the State means…\n\
                    Subject to Article 251 any section of citizens…\n\
                    New Article 19A was inserted; Article 25A guarantees education.";
        let unit = resolve_citation_unit(text, &manual::MANUAL);
        assert_eq!(unit, CitationUnit::Word("Article"), "got {unit:?}");
    }

    /// With no self-reference evidence the profile default is trusted (no override).
    #[test]
    fn resolver_trusts_profile_default_without_evidence() {
        let text = "1. Short title.\n2. Definitions.\n3. Scope of these provisions.";
        let unit = resolve_citation_unit(text, &manual::MANUAL);
        assert_eq!(unit, CitationUnit::Word("Section"), "got {unit:?}");
    }

    /// doc-06: "Article N" appears only as CROSS-references to other instruments
    /// (no "this Article") — the manual default "Section" must be KEPT, not flipped
    /// to "Article" by a handful of cross-refs.
    #[test]
    fn resolver_keeps_default_for_cross_reference_only_evidence() {
        let text = "Refer to Article 5 of the Agreement and Article 12 of the PPP Act. \
                    Article 3 of the Constitution and Article 9 of the Order also apply. \
                    See Article 7 of the said agreement.";
        let unit = resolve_citation_unit(text, &manual::MANUAL);
        assert_eq!(unit, CitationUnit::Word("Section"), "got {unit:?}");
    }

    /// A document that strongly mixes two units is ambiguous → Unresolved, never
    /// a confident-but-wrong pick.
    #[test]
    fn resolver_flags_unresolved_on_unit_conflict() {
        let text = "Article 5 and Article 6 and Article 7 apply. \
                    Section 1 and Section 2 and Section 3 also apply.";
        let unit = resolve_citation_unit(text, &generic::GENERIC);
        assert_eq!(unit, CitationUnit::Unresolved, "got {unit:?}");
    }
}
