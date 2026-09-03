//! Document parsers — DOCX, XLSX/XLSM, PPTX.

use std::path::Path;
use super::{ParseError, ParsedDocument};

const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024; // 500 MB limit for office docs

pub fn extract(path: &Path) -> Result<ParsedDocument, ParseError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(ParseError::FileTooLarge {
            size_bytes: metadata.len(),
            limit_bytes: MAX_FILE_SIZE,
        });
    }

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();

    let text = match ext.as_str() {
        "docx" => extract_docx(path)?,
        "xlsx" | "xlsm" => extract_xlsx(path)?,
        "pptx" => extract_pptx(path)?,
        _ => return Err(ParseError::Other(format!("unsupported format: {ext}"))),
    };

    Ok(ParsedDocument {
        text,
        page_offsets: vec![],
        extraction_method: "native",
        quality_score: 1.0,
        reconstructed_rows: Vec::new(),
        reconstruction_method: "",
        tables: Vec::new(),
        provision_titles: Default::default(),
    })
}

fn extract_docx(path: &Path) -> Result<String, ParseError> {
    let data = std::fs::read(path)?;
    let docx = docx_rs::read_docx(&data)
        .map_err(|e| ParseError::Other(format!("docx-rs: {e}")))?;

    let mut out = String::new();
    // docx_rs 0.4: docx.document.children (Vec<DocumentChild>)
    for child in &docx.document.children {
        match child {
            docx_rs::DocumentChild::Paragraph(p) => {
                for run_child in &p.children {
                    if let docx_rs::ParagraphChild::Run(run) = run_child {
                        for rc in &run.children {
                            if let docx_rs::RunChild::Text(t) = rc {
                                out.push_str(&t.text);
                            }
                        }
                    }
                }
                out.push('\n');
            }
            docx_rs::DocumentChild::Table(table) => {
                for row_child in &table.rows {
                    #[allow(irrefutable_let_patterns)]
                    if let docx_rs::TableChild::TableRow(row) = row_child {
                        for cell_child in &row.cells {
                            #[allow(irrefutable_let_patterns)]
                            if let docx_rs::TableRowChild::TableCell(cell) = cell_child {
                                for content in &cell.children {
                                    if let docx_rs::TableCellContent::Paragraph(p) = content {
                                        for run_child in &p.children {
                                            if let docx_rs::ParagraphChild::Run(run) = run_child {
                                                for rc in &run.children {
                                                    if let docx_rs::RunChild::Text(t) = rc {
                                                        out.push_str(&t.text);
                                                        out.push('\t');
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                out.push('|');
                            }
                        }
                        out.push('\n');
                    }
                }
            }
            _ => {}
        }
    }

    Ok(out)
}

fn extract_xlsx(path: &Path) -> Result<String, ParseError> {
    use calamine::Reader;
    let mut workbook: calamine::Xlsx<_> = calamine::open_workbook(path)
        .map_err(|e| ParseError::Other(format!("calamine: {e}")))?;

    let mut out = String::new();
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

    for sheet_name in sheet_names {
        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            out.push_str(&format!("## Sheet: {sheet_name}\n"));
            for row in range.rows() {
                let row_str: Vec<String> = row.iter()
                    .map(|cell| cell.to_string())
                    .collect();
                out.push_str(&row_str.join("\t"));
                out.push('\n');
            }
            out.push('\n');
        }
    }

    Ok(out)
}

fn extract_pptx(path: &Path) -> Result<String, ParseError> {
    // PPTX is a ZIP of XML files. Extract text from slide XML.
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ParseError::Other(format!("zip: {e}")))?;

    let mut slide_texts: Vec<(usize, String)> = Vec::new();

    for i in 0..archive.len() {
        let entry = archive.by_index(i)
            .map_err(|e| ParseError::Other(format!("zip entry: {e}")))?;

        let name = entry.name().to_string();
        if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
            let slide_num: usize = name
                .trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml")
                .parse()
                .unwrap_or(0);

            let content = std::io::read_to_string(entry)
                .map_err(|e| ParseError::Other(format!("slide read: {e}")))?;

            // Very naive XML text extraction: grab content between <a:t> tags.
            let text = extract_xml_text(&content);
            slide_texts.push((slide_num, text));
        }
    }

    slide_texts.sort_by_key(|(n, _)| *n);

    let mut out = String::new();
    for (num, text) in slide_texts {
        out.push_str(&format!("## Slide {num}\n{text}\n\n"));
    }

    Ok(out)
}

/// Extract text content from XML by finding `<a:t>` or `<t>` elements.
fn extract_xml_text(xml: &str) -> String {
    let mut out = String::new();
    let mut inside_tag = false;
    let mut tag_content = String::new();

    let mut chars = xml.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            // Collect tag name.
            tag_content.clear();
            while let Some(&next) = chars.peek() {
                if next == '>' {
                    chars.next();
                    break;
                }
                tag_content.push(chars.next().unwrap());
            }
            inside_tag = tag_content.trim_start_matches('/') == "a:t"
                || tag_content.trim_start_matches('/') == "t";
        } else if inside_tag {
            out.push(ch);
        }
    }

    out
}
