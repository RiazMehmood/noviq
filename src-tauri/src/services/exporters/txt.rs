//! Analysis result → plain-text exporter — T062.
//!
//! Writes title, body, and citation markers `[N]` to a `.txt` file.

use anyhow::Result;

use crate::services::analysis_engine::AnalysisResult;

/// Write an AnalysisResult to a plain-text file at `dest_path`.
pub fn export(result: &AnalysisResult, dest_path: &str) -> Result<()> {
    let mut out = String::new();

    // Header.
    out.push_str(&format!("{}\n", result.title));
    out.push_str(&"=".repeat(result.title.len()));
    out.push('\n');
    out.push_str(&format!("Template: {}\n", result.template.replace('_', " ")));
    out.push('\n');

    // Body.
    out.push_str(&result.output);
    out.push('\n');

    // Citations.
    if !result.citations.is_empty() {
        out.push('\n');
        out.push_str("References\n");
        out.push_str("----------\n");
        for (i, cit) in result.citations.iter().enumerate() {
            let excerpt_short: String = cit.excerpt.chars().take(120).collect();
            let page_info = cit.page.map(|p| format!(" (p. {})", p)).unwrap_or_default();
            out.push_str(&format!("[{}] {}{}\n", i + 1, excerpt_short, page_info));
        }
    }

    // Quality notice.
    if result.quality_constrained {
        out.push('\n');
        out.push_str(
            "NOTE: This analysis was generated with a constrained model due to hardware \
             limits. Results may be less detailed than a full model would provide.\n",
        );
    }

    std::fs::write(dest_path, &out)?;
    log::info!("Exported analysis TXT → {dest_path}");
    Ok(())
}
