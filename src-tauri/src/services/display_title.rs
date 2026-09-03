//! Display titles (2026-07-28): derive a human-readable document name from a
//! stored file name, for citations and title-first classification.
//!
//! `CompendiumOfRulesFramedUnderSindhLocalGovernmentAct-2013.pdf` →
//! `Compendium Of Rules Framed Under Sindh Local Government Act 2013`.
//!
//! Why classification cares: `doc_profiles::doc_type_from_title` tokenizes on
//! non-alphanumerics, so a CamelCase filename is ONE token and `has("rules")`
//! silently fails (live case: a user-uploaded compendium fell through to
//! body-word scoring). Splitting the words here fixes the name AND the
//! classifier in one place.

/// Derive a display title from a file name. Deterministic and idempotent —
/// an already-clean name ("Compendium Rules under SLGA 2013.pdf") passes
/// through unchanged apart from the extension.
pub fn display_title(file_name: &str) -> String {
    let name = file_name.trim();
    // Strip ONE trailing extension: a dot + 1–5 alphanumerics ("pdf", "docx").
    let stem = match name.rsplit_once('.') {
        Some((s, ext))
            if !s.is_empty()
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            s
        }
        _ => name,
    };
    let chars: Vec<char> = stem.chars().collect();
    let mut out = String::with_capacity(stem.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        // Separator characters become spaces ("Act-2013", "Public_Procurement").
        if c == '_' || c == '-' {
            out.push(' ');
            continue;
        }
        if i > 0 {
            let p = chars[i - 1];
            let boundary =
                // camelCase: "…dhLocal…" → split at h|L
                (p.is_lowercase() && c.is_uppercase())
                // acronym→word: "SLGARules" → split at A|R (next char lowercase)
                || (p.is_uppercase()
                    && c.is_uppercase()
                    && chars.get(i + 1).is_some_and(|n| n.is_lowercase()))
                // letter↔digit: "Act2013" / "2013Act"
                || (p.is_ascii_alphabetic() && c.is_ascii_digit())
                || (p.is_ascii_digit() && c.is_ascii_alphabetic());
            if boundary && !out.ends_with(' ') {
                out.push(' ');
            }
        }
        out.push(c);
    }
    let mut words: Vec<&str> = out.split_whitespace().collect();
    // Drop a leading pure-number sort prefix ("03 Sindh …" → "Sindh …"), but
    // only when real words follow — a name that IS a number stays intact.
    if words.len() > 1 && words[0].len() <= 3 && words[0].chars().all(|c| c.is_ascii_digit()) {
        words.remove(0);
    }
    let cleaned = words.join(" ");
    if cleaned.is_empty() { name.to_string() } else { cleaned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_camelcase_upload_names() {
        assert_eq!(
            display_title("CompendiumOfRulesFramedUnderSindhLocalGovernmentAct-2013.pdf"),
            "Compendium Of Rules Framed Under Sindh Local Government Act 2013"
        );
    }

    #[test]
    fn keeps_acronyms_whole_and_splits_after_them() {
        assert_eq!(display_title("SLGARules2013.pdf"), "SLGA Rules 2013");
    }

    #[test]
    fn underscores_and_sort_prefix() {
        assert_eq!(
            display_title("03_Sindh_Public_Procurement_Act_2009.pdf"),
            "Sindh Public Procurement Act 2009"
        );
    }

    #[test]
    fn clean_names_pass_through_minus_extension() {
        assert_eq!(
            display_title("Compendium Rules under SLGA 2013.pdf"),
            "Compendium Rules under SLGA 2013"
        );
    }

    #[test]
    fn no_extension_and_degenerate_inputs() {
        assert_eq!(display_title("notes"), "notes");
        assert_eq!(display_title("2013"), "2013"); // pure number kept, not stripped
        assert_eq!(display_title(""), "");
    }

    #[test]
    fn classification_tokens_now_visible() {
        // The live defect: has("rules") must succeed on the split title.
        let t = display_title("CompendiumOfRulesFramedUnderSindhLocalGovernmentAct-2013.pdf");
        assert!(t.split_whitespace().any(|w| w.eq_ignore_ascii_case("rules")));
    }
}
