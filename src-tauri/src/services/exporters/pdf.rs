//! Analysis result → PDF exporter — T061.
//!
//! Renders title, body text, and numbered citation footnotes using `printpdf`.

use anyhow::Result;
use printpdf::*;
use std::fs::File;
use std::io::BufWriter;

use crate::services::analysis_engine::AnalysisResult;

/// Render an AnalysisResult to a PDF file at `dest_path`.
pub fn export(result: &AnalysisResult, dest_path: &str) -> Result<()> {
    let (doc, page1, layer1) = PdfDocument::new(
        &result.title,
        Mm(210.0),
        Mm(297.0),
        "Layer 1",
    );
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // Load built-in fonts.
    let title_font = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;
    let body_font  = doc.add_builtin_font(BuiltinFont::Helvetica)?;

    // Title.
    current_layer.use_text(&result.title, 20.0, Mm(20.0), Mm(277.0), &title_font);

    // Template label.
    let label = format!("Template: {}", result.template.replace('_', " "));
    current_layer.use_text(&label, 10.0, Mm(20.0), Mm(268.0), &body_font);

    // Separator.
    current_layer.use_text(
        &"─".repeat(60),
        9.0,
        Mm(20.0),
        Mm(263.0),
        &body_font,
    );

    // Body text — word-wrap at 85 chars.
    let mut y = 255.0f32;
    let max_y   = 25.0f32;
    let line_h  = 5.5f32;

    for line in wrap_text(&result.output, 85) {
        if y < max_y { break; }
        current_layer.use_text(&line, 10.0, Mm(20.0), Mm(y), &body_font);
        y -= line_h;
    }

    // Citation footnotes.
    if !result.citations.is_empty() {
        y -= 5.0;
        if y > max_y {
            current_layer.use_text("References:", 11.0, Mm(20.0), Mm(y), &title_font);
            y -= line_h;
        }
        for (i, cit) in result.citations.iter().enumerate() {
            if y < max_y { break; }
            let excerpt_short: String = cit.excerpt.chars().take(80).collect();
            let footnote = if let Some(pg) = cit.page {
                format!("[{}] {} (p. {})", i + 1, excerpt_short, pg)
            } else {
                format!("[{}] {}", i + 1, excerpt_short)
            };
            current_layer.use_text(&footnote, 9.0, Mm(20.0), Mm(y), &body_font);
            y -= line_h;
        }
    }

    // Quality notice.
    if result.quality_constrained && y > max_y {
        y -= 5.0;
        let notice = "Note: Analysis used a constrained model due to hardware limits.";
        current_layer.use_text(notice, 9.0, Mm(20.0), Mm(y), &body_font);
    }

    // Write to disk.
    let file = File::create(dest_path)?;
    doc.save(&mut BufWriter::new(file))?;
    log::info!("Exported analysis PDF → {dest_path}");
    Ok(())
}

/// Naive word-wrap: break long lines at whitespace boundaries.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut result = Vec::new();
    for line in text.lines() {
        if line.len() <= max_chars {
            result.push(line.to_string());
            continue;
        }
        let mut start = 0;
        while start < line.len() {
            let end = (start + max_chars).min(line.len());
            let break_at = line[start..end]
                .rfind(' ')
                .map(|i| start + i)
                .unwrap_or(end);
            result.push(line[start..break_at].to_string());
            start = break_at.saturating_add(1);
        }
    }
    result
}
