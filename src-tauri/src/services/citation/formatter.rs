//! Path and title formatting helpers. Pure functions; no I/O.
//!
//! All citation strings in the codebase flow through these helpers via
//! `LegalCitation::format_*`. Reviewers — do not add a competing formatter
//! elsewhere; that would violate constitution §XI.

use super::types::PathSegment;

/// Joins all path segments into the comma-prefixed suffix of an officer-grade
/// citation. Returns "" for an empty path. The instrument_title is used to
/// pick between "Section" (Acts/Ordinances) and "Rule" (Rules instruments)
/// — Pakistan-gazette style.
pub fn format_path_segment_list(path: &[PathSegment], instrument_title: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let is_rules = instrument_title_is_rules(instrument_title);
    let mut out = String::new();
    let mut first = true;
    let mut idx = 0;
    while idx < path.len() {
        let seg = &path[idx];
        match seg {
            PathSegment::Preamble => {
                push_comma(&mut out, &mut first);
                out.push_str("Preamble");
            }
            PathSegment::Chapter(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Chapter ");
                out.push_str(n);
            }
            PathSegment::Part(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Part ");
                out.push_str(n);
            }
            PathSegment::Article(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Article ");
                out.push_str(n);
                idx += 1;
                // Absorb consecutive numeric/letter sub-segments.
                while idx < path.len() {
                    match &path[idx] {
                        PathSegment::SubSection(v)
                        | PathSegment::Clause(v)
                        | PathSegment::SubClause(v) => {
                            out.push('(');
                            out.push_str(v);
                            out.push(')');
                            idx += 1;
                        }
                        _ => break,
                    }
                }
                continue;
            }
            PathSegment::Section(n) => {
                push_comma(&mut out, &mut first);
                if is_rules {
                    out.push_str("Rule ");
                } else {
                    out.push_str("Section ");
                }
                out.push_str(n);
                idx += 1;
                // Absorb consecutive Sub-section / Clause / Sub-clause segments
                // into parenthesised suffixes.
                while idx < path.len() {
                    match &path[idx] {
                        PathSegment::SubSection(v)
                        | PathSegment::Clause(v)
                        | PathSegment::SubClause(v) => {
                            out.push('(');
                            out.push_str(v);
                            out.push(')');
                            idx += 1;
                        }
                        _ => break,
                    }
                }
                continue;
            }
            PathSegment::SubSection(v) => {
                // Bare sub-section without preceding Section — print as "(v)".
                push_comma(&mut out, &mut first);
                out.push('(');
                out.push_str(v);
                out.push(')');
            }
            PathSegment::Clause(v) => {
                push_comma(&mut out, &mut first);
                out.push_str("clause (");
                out.push_str(v);
                out.push(')');
            }
            PathSegment::SubClause(v) => {
                push_comma(&mut out, &mut first);
                out.push_str("sub-clause (");
                out.push_str(v);
                out.push(')');
            }
            PathSegment::Proviso(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Proviso ");
                out.push_str(n);
            }
            PathSegment::Explanation(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Explanation ");
                out.push_str(n);
            }
            PathSegment::Illustration(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Illustration ");
                out.push_str(n);
            }
            PathSegment::Schedule(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Schedule ");
                out.push_str(n);
            }
            PathSegment::ScheduleItem(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Item ");
                out.push_str(n);
            }
        }
        idx += 1;
    }
    out
}

fn push_comma(out: &mut String, _first: &mut bool) {
    out.push_str(", ");
    *_first = false;
}

fn instrument_title_is_rules(title: &str) -> bool {
    let t = title.to_ascii_lowercase();
    t.contains(" rules,") || t.ends_with(" rules") || t.contains(" rules ")
}

/// Build the short pill path like "§6(2)(a)".
pub fn format_short_path(path: &[PathSegment]) -> String {
    let mut out = String::from("§");
    let mut idx = 0;
    while idx < path.len() {
        match &path[idx] {
            PathSegment::Section(n) | PathSegment::Article(n) => {
                out.push_str(n);
                idx += 1;
                while idx < path.len() {
                    match &path[idx] {
                        PathSegment::SubSection(v)
                        | PathSegment::Clause(v)
                        | PathSegment::SubClause(v) => {
                            out.push('(');
                            out.push_str(v);
                            out.push(')');
                            idx += 1;
                        }
                        _ => break,
                    }
                }
                return out;
            }
            _ => idx += 1,
        }
    }
    // No Section/Article in path → return empty so the caller can drop the prefix.
    if out == "§" {
        return String::new();
    }
    out
}

/// Heuristic abbreviation of instrument titles for the short form.
/// "The Right of Access to Information Act, 2017" → "RTI Act 2017".
pub fn abbreviate_title(title: &str, year: Option<u32>) -> String {
    // Special-case the well-known Pakistan instruments to keep their conventional acronyms.
    // Add more here as needed; goldens are the spec.
    let lower_title = title.to_ascii_lowercase();
    if lower_title.contains("right of access to information") {
        return match year {
            Some(y) => format!("RTI Act {y}"),
            None => "RTI Act".to_string(),
        };
    }

    let t = title.strip_prefix("The ").unwrap_or(title);
    let (name_part, parsed_year_opt) = match t.rsplit_once(", ") {
        Some((n, y)) if y.chars().all(|c| c.is_ascii_digit()) => {
            (n, y.parse::<u32>().ok())
        }
        _ => (t, None),
    };
    let effective_year = year.or(parsed_year_opt);
    let acronym = build_acronym(name_part);
    match effective_year {
        Some(y) => format!("{acronym} {y}"),
        None => acronym,
    }
}

fn build_acronym(name_part: &str) -> String {
    let lower = name_part.to_ascii_lowercase();
    let kind_word = ["act", "ordinance", "rules", "policy", "regulations"]
        .iter()
        .find(|k| lower.contains(*k))
        .copied();
    let Some(kind) = kind_word else {
        return name_part.to_string();
    };
    let cap_kind = match kind {
        "act" => "Act",
        "ordinance" => "Ordinance",
        "rules" => "Rules",
        "policy" => "Policy",
        "regulations" => "Regulations",
        _ => kind,
    };
    let descriptive_end = lower.find(kind).unwrap();
    let descriptive = &name_part[..descriptive_end];
    let mut acronym = String::new();
    for word in descriptive.split_whitespace() {
        let lw = word.to_ascii_lowercase();
        if matches!(
            lw.as_str(),
            "of" | "to" | "the" | "and" | "for" | "on" | "in"
        ) {
            continue;
        }
        if let Some(c) = word.chars().next() {
            acronym.push(c.to_ascii_uppercase());
        }
    }
    if acronym.is_empty() {
        cap_kind.to_string()
    } else {
        format!("{acronym} {cap_kind}")
    }
}
