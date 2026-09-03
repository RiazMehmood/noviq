//! T012 — persona + per-capability prompt loading (Constitution VI: persona is
//! config, never hardcoded). The bureaucrat persona and the seven capability
//! prompt fragments live in `config/personas/bureaucrat.toml`; this module is
//! the typed loader the orchestrator and capability handlers read from.
//!
//! The default config is embedded (`include_str!`) so it is always available
//! regardless of the runtime working directory; `ECHO_PERSONA_TOML` overrides
//! with an on-disk file for tuning (T048) without a rebuild.

use crate::services::assistant::intent::Capability;
use serde::Deserialize;
use std::sync::OnceLock;

/// The embedded default persona config (tracked at `config/personas/bureaucrat.toml`).
const EMBEDDED: &str = include_str!("../../../config/personas/bureaucrat.toml");

/// One capability's prompt fragment.
#[derive(Debug, Clone, Deserialize)]
pub struct Fragment {
    pub prompt: String,
    /// Optional follow-up-offer instruction, appended after `prompt` only when
    /// the user has the offer enabled (2026-07-29). Optional and defaulted so an
    /// on-disk `ECHO_PERSONA_TOML` written before this field existed still
    /// parses — a parse failure here falls back to the EMBEDDED default
    /// silently, which would revert the operator's whole persona.
    #[serde(default)]
    pub offer: Option<String>,
}

/// The always-on voice, with a compact variant for small windows.
#[derive(Debug, Clone, Deserialize)]
pub struct PersonaText {
    pub full: String,
    pub compact: String,
}

/// Per-capability prompt fragments appended after the persona.
#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityPrompts {
    pub guidance: Fragment,
    pub draft: Fragment,
    pub summarize_doc: Fragment,
    pub compare: Fragment,
    pub conflict: Fragment,
    pub amendment_chain: Fragment,
    pub action_items: Fragment,
}

/// Tuning defaults (token reserves + memory knobs), refined in T048.
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub gen_reserve_qa: usize,
    pub gen_reserve_draft: usize,
    pub recent_turns_verbatim: usize,
    pub context_fill_threshold: f32,
}

/// Fast-path guardrail vocabularies (Constitution VI: config, never hardcoded).
///
/// Read by `intent::is_greeting_or_intro` / `intent::is_low_signal`, which
/// short-circuit greetings and content-free probes BEFORE retrieval so they never
/// cost a full inference. Kept here so the probe list can be extended from real
/// `query_log` traffic via `ECHO_PERSONA_TOML` without a rebuild.
///
/// Every field defaults, so a `[guardrails]` section is OPTIONAL: an existing
/// on-disk `ECHO_PERSONA_TOML` written before this section existed still parses.
/// That matters because a parse failure falls back to the embedded default
/// **silently** (see [`PersonaConfig::get`]) — a required field here would revert
/// a tuned persona with nothing but a log line to show for it.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Guardrails {
    /// Bare greetings, matched against the whole trimmed message (exact).
    pub greetings: Vec<String>,
    /// "Who/what are you" phrases, matched as a substring of a ≤10-word message.
    pub about: Vec<String>,
    /// Content-free probes ("test", "asdf"), matched against the whole message.
    pub probes: Vec<String>,
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

impl Default for Guardrails {
    fn default() -> Self {
        Self {
            greetings: strs(&[
                "hi", "hii", "hey", "hello", "helo", "salam", "assalam o alaikum",
                "assalam-o-alaikum", "aoa", "good morning", "good afternoon", "good evening",
            ]),
            about: strs(&[
                "who are you", "what are you", "tell me about yourself", "about yourself",
                "introduce yourself", "what can you do", "what do you do", "how can you help",
                "what can i ask", "what can i do here", "help me", "how do you work",
                "what is noviq", "who is noviq", "about noviq", "what is this",
            ]),
            probes: strs(&[
                "test", "testing", "tester", "test test", "just testing", "testing 123",
                "asdf", "asdfgh", "asdfasdf", "asdfjkl", "qwerty", "qwe", "qwert",
                "abc", "abcd", "abcde", "abcdef", "xyz", "xyzzy", "blah", "blah blah",
                "foo", "bar", "baz", "foobar", "dummy", "sample", "ping", "hello test",
                "random", "gibberish", "lorem ipsum", "nothing", "na", "n/a",
            ]),
        }
    }
}

/// The whole persona config (`bureaucrat.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct PersonaConfig {
    pub persona: PersonaText,
    pub capability: CapabilityPrompts,
    pub settings: Settings,
    /// Optional — see [`Guardrails`]; absent section ⇒ the built-in lists.
    #[serde(default)]
    pub guardrails: Guardrails,
}

impl PersonaConfig {
    /// Parse a TOML string into a config (used by [`Self::load`] and tests).
    pub fn parse(toml_src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_src)
    }

    /// The process-wide config: `ECHO_PERSONA_TOML` file if set + parseable,
    /// else the embedded default. Cached after first load.
    pub fn get() -> &'static PersonaConfig {
        static CFG: OnceLock<PersonaConfig> = OnceLock::new();
        CFG.get_or_init(|| {
            if let Ok(path) = std::env::var("ECHO_PERSONA_TOML") {
                if let Ok(src) = std::fs::read_to_string(&path) {
                    match Self::parse(&src) {
                        Ok(c) => {
                            log::info!("persona config loaded from {path}");
                            return c;
                        }
                        Err(e) => log::warn!("ECHO_PERSONA_TOML parse failed ({e}); using embedded default"),
                    }
                }
            }
            Self::parse(EMBEDDED).expect("embedded bureaucrat.toml is valid")
        })
    }

    /// Persona voice; the `compact` variant is selected on small windows (the
    /// allocator reports the available budget).
    pub fn persona_text(&self, compact: bool) -> &str {
        if compact { &self.persona.compact } else { &self.persona.full }
    }

    /// The prompt fragment for a capability.
    pub fn capability_prompt(&self, cap: Capability) -> &str {
        let c = &self.capability;
        match cap {
            Capability::Guidance => &c.guidance.prompt,
            Capability::Draft => &c.draft.prompt,
            Capability::SummarizeDoc => &c.summarize_doc.prompt,
            Capability::Compare => &c.compare.prompt,
            Capability::Conflict => &c.conflict.prompt,
            Capability::AmendmentChain => &c.amendment_chain.prompt,
            Capability::ActionItems => &c.action_items.prompt,
            // Count and Tabular are deterministic (SQL / Rust arithmetic, no
            // LLM answer) — neither ever asks for a prompt; fall back to the
            // guidance fragment so the match stays total.
            Capability::Count | Capability::Tabular => &c.guidance.prompt,
        }
    }

    /// The follow-up-offer fragment for a capability, when the persona defines
    /// one. Only `guidance` carries an offer today; every other capability
    /// returns `None`, so enabling the preference cannot make a draft or a
    /// comparison sprout a "Next steps" section.
    pub fn capability_offer(&self, cap: Capability) -> Option<&str> {
        let c = &self.capability;
        let frag = match cap {
            Capability::Guidance => &c.guidance,
            Capability::Draft => &c.draft,
            Capability::SummarizeDoc => &c.summarize_doc,
            Capability::Compare => &c.compare,
            Capability::Conflict => &c.conflict,
            Capability::AmendmentChain => &c.amendment_chain,
            Capability::ActionItems => &c.action_items,
            Capability::Count | Capability::Tabular => &c.guidance,
        };
        frag.offer.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }

    /// Generation reserve (tokens) for a capability — drafts get the larger
    /// reserve (FR-013).
    pub fn gen_reserve(&self, cap: Capability) -> usize {
        match cap {
            Capability::Draft => self.settings.gen_reserve_draft,
            _ => self.settings.gen_reserve_qa,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_config_parses() {
        let cfg = PersonaConfig::parse(EMBEDDED).expect("embedded toml parses");
        assert!(cfg.persona.full.to_lowercase().contains("bureaucrat"));
        assert!(!cfg.persona.compact.is_empty());
        assert!(cfg.settings.gen_reserve_draft > cfg.settings.gen_reserve_qa);
        assert!(cfg.settings.context_fill_threshold > 0.0 && cfg.settings.context_fill_threshold < 1.0);
    }

    /// Moving the guardrail lists from Rust consts into the TOML must not change
    /// which turns get short-circuited. The embedded config IS the old hardcoded
    /// list — if someone edits one and not the other, this fails.
    #[test]
    fn embedded_guardrails_match_the_builtin_defaults() {
        let cfg = PersonaConfig::parse(EMBEDDED).unwrap();
        let d = Guardrails::default();
        assert_eq!(cfg.guardrails.greetings, d.greetings, "greetings drifted from the built-in list");
        assert_eq!(cfg.guardrails.about, d.about, "about phrases drifted from the built-in list");
        assert_eq!(cfg.guardrails.probes, d.probes, "probes drifted from the built-in list");
    }

    /// A `[guardrails]` section is optional. This is the compatibility guarantee for
    /// any on-disk ECHO_PERSONA_TOML written before the section existed: without the
    /// serde default it would fail to parse and `get()` would fall back to the
    /// embedded config **silently**, reverting a tuned persona with no visible error.
    #[test]
    fn guardrails_section_is_optional_for_older_override_files() {
        let src = EMBEDDED
            .split_once("[guardrails]")
            .expect("embedded config has a [guardrails] section")
            .0;
        let cfg = PersonaConfig::parse(src).expect("a config without [guardrails] must still parse");
        assert_eq!(cfg.guardrails.probes, Guardrails::default().probes);
        assert!(cfg.persona.full.to_lowercase().contains("bureaucrat"), "persona survives");
    }

    #[test]
    fn every_capability_has_a_prompt() {
        let cfg = PersonaConfig::parse(EMBEDDED).unwrap();
        for cap in [
            Capability::Guidance,
            Capability::Draft,
            Capability::SummarizeDoc,
            Capability::Compare,
            Capability::Conflict,
            Capability::AmendmentChain,
            Capability::ActionItems,
        ] {
            assert!(!cfg.capability_prompt(cap).trim().is_empty(), "{cap:?} prompt empty");
        }
    }

    #[test]
    fn compact_persona_selected_on_small_window() {
        let cfg = PersonaConfig::parse(EMBEDDED).unwrap();
        assert_ne!(cfg.persona_text(true), cfg.persona_text(false));
        assert!(cfg.persona_text(true).len() < cfg.persona_text(false).len());
    }

    #[test]
    fn draft_reserve_is_larger() {
        let cfg = PersonaConfig::parse(EMBEDDED).unwrap();
        assert_eq!(cfg.gen_reserve(Capability::Draft), cfg.settings.gen_reserve_draft);
        assert_eq!(cfg.gen_reserve(Capability::Guidance), cfg.settings.gen_reserve_qa);
    }

    #[test]
    fn get_returns_cached_singleton() {
        let a = PersonaConfig::get();
        let b = PersonaConfig::get();
        assert!(std::ptr::eq(a, b));
    }
}
