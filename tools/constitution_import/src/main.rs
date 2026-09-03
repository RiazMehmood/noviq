//! `constitution_import` — dev-only S1 importer (feature 003).
//!
//! Fetches the Constitution of Pakistan from pakistani.org's structured HTML
//! (one page per Part / Part-Chapter), parses the typographic markers into
//! `ConstitutionIR`, and emits `constitution_ir.json`. Runs offline against a
//! local `.cache/`; never bundled into the app (Constitution Principle I).
//!
//!   cargo run --manifest-path tools/constitution_import/Cargo.toml -- \
//!       --out tools/constitution_import/constitute_IR/constitution_ir.json
//!
//! Flags: `--out <path>` (default: crate-local), `--refresh` (re-download).

mod ir;

use anyhow::{bail, Context, Result};
use ir::*;
use scraper::{ElementRef, Html, Selector};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const BASE: &str = "https://pakistani.org/pakistan/constitution/";
const RETRIEVED: &str = "2026-05-21";
const UA: &str = "echo-ai-constitution-import/0.1 (dev tool; feature 003)";

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn cache_dir() -> PathBuf {
    crate_dir().join(".cache")
}

fn main() -> Result<()> {
    let mut out = crate_dir().join("constitution_ir.json");
    let mut refresh = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = PathBuf::from(args.next().context("--out needs a path")?),
            "--refresh" => refresh = true,
            "-h" | "--help" => {
                println!("usage: constitution_import [--out PATH] [--refresh]");
                return Ok(());
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    fs::create_dir_all(cache_dir()).context("creating .cache/")?;

    eprintln!("[1/4] fetching index + preamble ...");
    let index_html = fetch("index", BASE, refresh)?;
    let plan = build_plan(&index_html)?;
    let preamble = fetch_preamble(refresh)?;
    eprintln!(
        "       {} Parts planned; preamble {}",
        plan.len(),
        if preamble.is_some() {
            "captured"
        } else {
            "NOT FOUND"
        }
    );

    eprintln!("[2/4] fetching + parsing Part/Chapter pages ...");
    let mut parts = Vec::with_capacity(plan.len());
    let mut footnotes: Vec<Footnote> = Vec::new();
    let mut seen_fn: HashSet<String> = HashSet::new();
    for pp in &plan {
        parts.push(assemble_part(pp, refresh)?);
        // Footnote descriptions (Layer 2) live in each page's `references` div.
        for page in pp.standalone_page.iter().chain(pp.chapter_pages.iter()) {
            let html = fetch_page(page, refresh)?;
            for fnote in parse_references(&Html::parse_document(&html)) {
                if seen_fn.insert(fnote.id.clone()) {
                    footnotes.push(fnote);
                }
            }
        }
    }
    footnotes.sort_by_key(|f| f.id.parse::<u32>().unwrap_or(0));
    let amendments = parse_amendments(&index_html);
    eprintln!(
        "       {} footnote descriptions, {} catalogue entries",
        footnotes.len(),
        amendments.len()
    );

    let constitution = ConstitutionIR {
        title: "The Constitution of the Islamic Republic of Pakistan".into(),
        source_url: BASE.into(),
        retrieved: RETRIEVED.into(),
        amendments_through: "as published on pakistani.org (constitutional amendments \
            incorporated through the 27th Amendment)"
            .into(),
        preamble,
        amendments,
        footnotes,
        parts,
    };

    eprintln!("[3/4] writing {} ...", out.display());
    if let Some(p) = out.parent() {
        fs::create_dir_all(p).ok();
    }
    fs::write(&out, serde_json::to_string_pretty(&constitution)?)
        .with_context(|| format!("writing {}", out.display()))?;

    eprintln!("[4/4] cross-check:\n");
    let ok = report(&constitution);
    if !ok {
        bail!("S1 acceptance cross-check FAILED — see report above");
    }
    eprintln!("\nOK — {} written", out.display());
    Ok(())
}

// ─── fetching (with a local cache) ───────────────────────────────────────────

/// Fetch a URL, caching the raw HTML under `.cache/<key>.html`. A cached file is
/// reused unless `refresh` is set, keeping re-runs offline and deterministic.
fn fetch(key: &str, url: &str, refresh: bool) -> Result<String> {
    let file = cache_dir().join(format!("{key}.html"));
    if !refresh {
        if let Ok(s) = fs::read_to_string(&file) {
            return Ok(s);
        }
    }
    eprintln!("       GET {url}");
    let mut resp = ureq::get(url)
        .header("User-Agent", UA)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let body = resp
        .body_mut()
        .read_to_string()
        .with_context(|| format!("reading body of {url}"))?;
    fs::write(&file, &body).with_context(|| format!("caching {}", file.display()))?;
    std::thread::sleep(Duration::from_millis(300)); // be polite to the source
    Ok(body)
}

/// Fetch a Part/Chapter page, trying filename case variants. pakistani.org's
/// `<select>` lowercases the chapter letter (`ch1a`) but the actual file is
/// uppercased (`ch1A`); try the uppercased filename first, then as-is.
fn fetch_page(page_id: &str, refresh: bool) -> Result<String> {
    let mut candidates = Vec::new();
    let upper = uppercase_chapter_letter(page_id);
    if upper != page_id {
        candidates.push(upper);
    }
    candidates.push(page_id.to_string());

    let mut last_err = None;
    for cand in &candidates {
        match fetch(cand, &format!("{BASE}{cand}.html"), refresh) {
            Ok(html) => return Ok(html),
            Err(e) => {
                eprintln!("       (candidate '{cand}' failed: {e})");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap()).with_context(|| format!("could not fetch page '{page_id}'"))
}

/// `part7.ch1a` → `part7.ch1A`; leaves chapterless ids untouched.
fn uppercase_chapter_letter(s: &str) -> String {
    match s.split_once(".ch") {
        Some((head, chap)) => format!("{head}.ch{}", chap.to_uppercase()),
        None => s.to_string(),
    }
}

/// Fetch and parse the standalone preamble page. The preamble is the run of
/// `<p>` paragraphs immediately after the `<h2>Preamble</h2>` heading; they are
/// joined with blank lines. Returns `None` if the page or heading is absent.
fn fetch_preamble(refresh: bool) -> Result<Option<String>> {
    let html = fetch("preamble", &format!("{BASE}preamble.html"), refresh)?;
    let doc = Html::parse_document(&html);
    let h2_sel = Selector::parse("h2").unwrap();
    for h2 in doc.select(&h2_sel) {
        if !clean(&text_of(h2, &["sup"])).eq_ignore_ascii_case("preamble") {
            continue;
        }
        let mut paras = Vec::new();
        let mut sib = h2.next_sibling();
        while let Some(node) = sib {
            if let Some(e) = ElementRef::wrap(node) {
                if e.value().name() == "p" {
                    let t = clean(&text_of(e, &["sup"]));
                    if !t.is_empty() {
                        paras.push(t);
                    }
                } else {
                    break; // first non-<p> element ends the preamble block
                }
            }
            sib = node.next_sibling();
        }
        if !paras.is_empty() {
            return Ok(Some(paras.join("\n\n")));
        }
    }
    Ok(None)
}

// ─── plan: which pages make up each Part ─────────────────────────────────────

/// One Part's page layout, derived from the index `<select>`.
struct PartPlan {
    roman: String,
    /// Standalone page id, for Parts that are not split into chapters.
    standalone_page: Option<String>,
    /// Chapter page ids, in document order.
    chapter_pages: Vec<String>,
}

/// Parse the index page's `<a href>` links into a per-Part page plan. The index
/// links every Part/Chapter page as `…/partN.html` or `…/partN.chX.html`; a
/// `partN.html` link is a standalone (chapter-less) Part.
fn build_plan(index_html: &str) -> Result<Vec<PartPlan>> {
    let doc = Html::parse_document(index_html);
    let sel = Selector::parse("a[href]").unwrap();

    let mut parts: BTreeMap<u32, PartPlan> = BTreeMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for a in doc.select(&sel) {
        let href = a.value().attr("href").unwrap_or("");
        let base = href.rsplit('/').next().unwrap_or("");
        let Some(page_id) = base.strip_suffix(".html") else {
            continue;
        };
        // Accept only `partN` / `partN.chX`; skip preamble / annex / schedN /
        // amendment + order footnote links.
        let (part_seg, is_chapter) = match page_id.split_once('.') {
            Some((p, rest)) if rest.starts_with("ch") => (p, true),
            Some(_) => continue,
            None => (page_id, false),
        };
        let Some(num) = part_seg
            .strip_prefix("part")
            .and_then(|d| d.parse::<u32>().ok())
        else {
            continue;
        };
        if !seen.insert(page_id.to_string()) {
            continue; // index links some pages more than once
        }
        let entry = parts.entry(num).or_insert_with(|| PartPlan {
            roman: roman(num),
            standalone_page: None,
            chapter_pages: Vec::new(),
        });
        if is_chapter {
            entry.chapter_pages.push(page_id.to_string());
        } else {
            entry.standalone_page = Some(page_id.to_string());
        }
    }
    if parts.is_empty() {
        bail!("no Parts parsed from the index — pakistani.org markup may have changed");
    }
    Ok(parts.into_values().collect())
}

/// Roman numeral for Parts I–XII.
fn roman(n: u32) -> String {
    const R: [&str; 12] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
    ];
    R.get((n.max(1) - 1) as usize)
        .map(|s| s.to_string())
        .unwrap_or_else(|| n.to_string())
}

// ─── assembling a Part from its pages ────────────────────────────────────────

fn assemble_part(pp: &PartPlan, refresh: bool) -> Result<Part> {
    let mut part = Part {
        number: pp.roman.clone(),
        title: String::new(),
        source_page: pp.standalone_page.clone(),
        articles: Vec::new(),
        chapters: Vec::new(),
    };
    let mut seen_direct: HashSet<String> = HashSet::new();

    if let Some(page) = &pp.standalone_page {
        absorb_page(&mut part, &mut seen_direct, page, refresh)?;
    }
    for page in &pp.chapter_pages {
        absorb_page(&mut part, &mut seen_direct, page, refresh)?;
    }
    if part.title.is_empty() {
        part.title = format!("Part {}", pp.roman);
    }
    Ok(part)
}

/// Parse one page and merge its sections into `part`. Part-level sections feed
/// `part.articles` (deduped by article number); Chapter sections become chapters.
fn absorb_page(
    part: &mut Part,
    seen_direct: &mut HashSet<String>,
    page: &str,
    refresh: bool,
) -> Result<()> {
    let html = fetch_page(page, refresh)?;
    for sec in parse_page(&html, page)? {
        match sec.kind {
            SecKind::Part => {
                if part.title.is_empty() && !sec.title.is_empty() {
                    part.title = sec.title;
                }
                for a in sec.articles {
                    if seen_direct.insert(a.number.clone()) {
                        part.articles.push(a);
                    }
                }
            }
            SecKind::Chapter => {
                if sec.articles.is_empty() {
                    continue; // phantom heading with no article table
                }
                part.chapters.push(Chapter {
                    number: sec.number,
                    title: sec.title,
                    source_page: page.to_string(),
                    articles: sec.articles,
                });
            }
        }
    }
    Ok(())
}

// ─── page parsing ────────────────────────────────────────────────────────────

enum SecKind {
    Part,
    Chapter,
}

/// A heading + the articles in the table that follows it.
struct Section {
    kind: SecKind,
    number: String,
    title: String,
    articles: Vec<Article>,
}

/// Parse a Part/Chapter page into its `<h2>`-delimited sections. Each `<h2>` is
/// followed by a `<table>` of articles; a heading with no table (e.g. a repeated
/// "Part II" heading on a chapter page) yields no section.
fn parse_page(html: &str, page: &str) -> Result<Vec<Section>> {
    let doc = Html::parse_document(html);
    let h2_sel = Selector::parse("h2").unwrap();

    let mut sections = Vec::new();
    let mut total_articles = 0;
    for h2 in doc.select(&h2_sel) {
        // Strip `<sup>` amendment-footnote markers so they don't pollute titles.
        let heading = clean(&text_of(h2, &["sup"]));
        let (kind, number, title) = classify_heading(&heading);
        let mut articles = Vec::new();
        for (table, wrapper_refs) in following_tables(h2) {
            articles.extend(parse_articles(table, &wrapper_refs));
        }
        total_articles += articles.len();
        // Sections with no articles are still emitted so a Part heading's title
        // propagates even when the Part has no direct articles before Chapter 1.
        sections.push(Section {
            kind,
            number,
            title,
            articles,
        });
    }
    if total_articles == 0 {
        bail!("page '{page}': no articles found — markup may have changed");
    }
    Ok(sections)
}

/// Classify an `<h2>` as a Part or Chapter heading and split off its label/title.
fn classify_heading(h: &str) -> (SecKind, String, String) {
    let (head, title) = match h.split_once(':') {
        Some((a, b)) => (a.trim(), b.trim().to_string()),
        None => (h.trim(), String::new()),
    };
    if let Some(n) = head.strip_prefix("Chapter") {
        (SecKind::Chapter, n.trim().to_string(), title)
    } else if let Some(n) = head.strip_prefix("Part") {
        (SecKind::Part, n.trim().to_string(), title)
    } else {
        // Unknown heading — treat as a Part-level section with no number.
        let t = if title.is_empty() { h.to_string() } else { title };
        (SecKind::Part, String::new(), t)
    }
}

/// All `<table>` siblings between `h2` and the next `<h2>` (or the references
/// block), each paired with the amendment footnote ids of any `N[ … ]N` bracket
/// wrapping it. pakistani.org splits one Chapter's articles across many sibling
/// tables, and wraps a whole-Article amendment in a bracket *around* its table.
fn following_tables(h2: ElementRef) -> Vec<(ElementRef, Vec<String>)> {
    let mut tables = Vec::new();
    let mut open: Vec<String> = Vec::new(); // wrapper footnote ids currently open
    let mut sib = h2.next_sibling();
    while let Some(node) = sib {
        if let Some(e) = ElementRef::wrap(node) {
            match e.value().name() {
                "table" => tables.push((e, open.clone())),
                "h2" => break,
                "div"
                    if e.value()
                        .attr("class")
                        .is_some_and(|c| c.contains("references")) =>
                {
                    break
                }
                // A loose `<sup>` carries the footnote anchor of an `N[` / `]N`
                // bracket marker — toggle that id open/closed.
                "sup" => {
                    for id in footnote_refs_own(e) {
                        match open.iter().position(|x| *x == id) {
                            Some(p) => {
                                open.remove(p);
                            }
                            None => open.push(id),
                        }
                    }
                }
                _ => {}
            }
        }
        sib = node.next_sibling();
    }
    tables
}

/// Parse the article rows of a section table. Each article is a `<tr>` whose
/// first `<td>` holds `<nobr><b>NUM</b></nobr>`; layout/spacer rows are skipped.
/// `wrapper_refs` are the footnote ids of an `N[ … ]N` bracket wrapping the
/// whole table (an Article enacted/substituted as a unit, e.g. Article 204);
/// they are merged into every Article parsed from it.
fn parse_articles(table: ElementRef, wrapper_refs: &[String]) -> Vec<Article> {
    let mut articles = Vec::new();
    for tr in table_rows(table) {
        let tds: Vec<ElementRef> = child_elems(tr)
            .into_iter()
            .filter(|e| e.value().name() == "td")
            .collect();
        if tds.len() != 2 {
            continue; // spacer (`<td colspan=2>`) or other layout row
        }
        let number = clean(&text_of(tds[0], &["sup"]))
            .trim_end_matches('.')
            .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
            .to_string();
        // Article numbers start with a digit (`10`, `10A`, `175B`); anything
        // else in the first cell means this is not an article row.
        if !number.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let mut article = parse_article(number, tds[1]);
        if !wrapper_refs.is_empty() {
            article.footnote_refs.extend(wrapper_refs.iter().cloned());
            article.footnote_refs = norm_refs(std::mem::take(&mut article.footnote_refs));
        }
        articles.push(article);
    }
    articles
}

fn parse_article(number: String, content: ElementRef) -> Article {
    let marginal_note = first_child_named(content, "b")
        .map(|b| strip_brackets(&text_of(b, &["sup"])))
        .unwrap_or_default();

    // Article-level chapeau: all text minus the marginal note, nested clause
    // tables, and proviso/explanation blockquotes.
    let full = strip_brackets(&text_of(content, &["sup", "table", "blockquote"]));
    let chapeau = full
        .strip_prefix(&marginal_note)
        .unwrap_or(&full)
        .trim()
        .to_string();
    let text = (!chapeau.is_empty()).then_some(chapeau);

    // Clauses span every direct-child table, not just the first: pakistani.org
    // renders amendment-inserted clauses in their own sibling table.
    let clauses: Vec<Clause> = child_tables(content)
        .into_iter()
        .flat_map(parse_clauses)
        .collect();
    // Provisos/explanations that hang off the Article itself — flat articles
    // with no numbered clause to attach to (e.g. Article 18).
    let (provisos, explanations) = parse_blockquotes(content);

    Article {
        number,
        marginal_note,
        text,
        clauses,
        provisos,
        explanations,
        footnote_refs: footnote_refs_own(content),
    }
}

fn parse_clauses(table: ElementRef) -> Vec<Clause> {
    let mut clauses = Vec::new();
    for tr in table_rows(table) {
        let Some((id, content)) = enum_row(tr) else {
            continue;
        };
        let text = strip_brackets(&text_of(content, &["sup", "table", "blockquote"]));
        // Every direct-child table, not just the first — amendment-inserted
        // sub-clauses get their own sibling table (often inside literal `[ ]`).
        let subclauses: Vec<SubClause> = child_tables(content)
            .into_iter()
            .flat_map(parse_subclauses)
            .collect();
        let (provisos, explanations) = parse_blockquotes(content);
        clauses.push(Clause {
            id,
            text,
            subclauses,
            provisos,
            explanations,
            footnote_refs: footnote_refs_own(content),
        });
    }
    clauses
}

fn parse_subclauses(table: ElementRef) -> Vec<SubClause> {
    let mut subclauses = Vec::new();
    for tr in table_rows(table) {
        let Some((id, content)) = enum_row(tr) else {
            continue;
        };
        // Keep any deeper nested enumeration flattened into the leaf text
        // (do not stop at `table`) — sub-clause is the v1 retrieval leaf.
        let text = strip_brackets(&text_of(content, &["sup", "blockquote"]));
        let (provisos, explanations) = parse_blockquotes(content);
        subclauses.push(SubClause {
            id,
            text,
            provisos,
            explanations,
            footnote_refs: footnote_refs_own(content),
        });
    }
    subclauses
}

/// An enumerated row (`(1)` / `(a)` cell + content cell). `None` for spacers.
fn enum_row(tr: ElementRef) -> Option<(String, ElementRef)> {
    let tds: Vec<ElementRef> = child_elems(tr)
        .into_iter()
        .filter(|e| e.value().name() == "td")
        .collect();
    if tds.len() != 2 {
        return None;
    }
    // Strip amendment-bracket noise: pakistani.org wraps amendment-inserted
    // rows in literal `[ ]`, which otherwise bleeds into the id (`[(c)]`).
    let id = clean(&text_of(tds[0], &["sup"]))
        .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
        .to_string();
    if id.is_empty() {
        return None;
    }
    Some((id, tds[1]))
}

/// Classify the direct-child `<blockquote>` elements of a clause/sub-clause cell
/// into provisos and explanations.
fn parse_blockquotes(content: ElementRef) -> (Vec<Proviso>, Vec<Explanation>) {
    let mut provisos = Vec::new();
    let mut explanations = Vec::new();
    for bq in child_elems(content)
        .into_iter()
        .filter(|e| e.value().name() == "blockquote")
    {
        let text = clean(&text_of(bq, &["sup"]));
        if text.is_empty() {
            continue;
        }
        if let Some(rest) = text.strip_prefix("Explanation") {
            let label = rest
                .split(':')
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| c == '-' || c == '.' || c.is_whitespace())
                .to_string();
            explanations.push(Explanation { label, text });
        } else {
            provisos.push(Proviso { text });
        }
    }
    (provisos, explanations)
}

/// Footnote ids of amendment markers anchored directly in `el` — does NOT
/// descend into nested `<table>`s, so a clause never absorbs its sub-clauses'
/// refs (per-node positioning). Body anchors are `<a name="fNN">`.
fn footnote_refs_own(el: ElementRef) -> Vec<String> {
    let mut refs = Vec::new();
    collect_refs(el, &mut refs);
    norm_refs(refs)
}

fn collect_refs(el: ElementRef, out: &mut Vec<String>) {
    for child in child_elems(el) {
        if child.value().name() == "table" {
            continue; // a nested table is a child node — its refs are its own
        }
        if child.value().name() == "a" {
            if let Some(n) = child.value().attr("name").and_then(|n| n.strip_prefix('f')) {
                if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
                    out.push(n.to_string());
                }
            }
        }
        collect_refs(child, out);
    }
}

/// Sort footnote ids numerically and dedup.
fn norm_refs(mut v: Vec<String>) -> Vec<String> {
    v.sort_by_key(|s| s.parse::<u32>().unwrap_or(0));
    v.dedup();
    v
}

// ─── small HTML helpers ──────────────────────────────────────────────────────

/// Direct element children of `el` (skips text/comment nodes).
fn child_elems(el: ElementRef) -> Vec<ElementRef> {
    el.children().filter_map(ElementRef::wrap).collect()
}

/// First direct child element of `el` with the given tag name.
fn first_child_named<'a>(el: ElementRef<'a>, name: &str) -> Option<ElementRef<'a>> {
    child_elems(el).into_iter().find(|e| e.value().name() == name)
}

/// All direct-child `<table>` elements of `el`, in document order. Unlike
/// `first_child_named(_, "table")`, this keeps amendment-inserted clauses and
/// sub-clauses that pakistani.org renders in a second sibling table.
fn child_tables(el: ElementRef) -> Vec<ElementRef> {
    child_elems(el)
        .into_iter()
        .filter(|e| e.value().name() == "table")
        .collect()
}

/// The `<tr>` rows of a table, descending through an implicit `<tbody>`/`<thead>`
/// but NOT through nested tables.
fn table_rows(table: ElementRef) -> Vec<ElementRef> {
    let mut rows = Vec::new();
    for child in child_elems(table) {
        match child.value().name() {
            "tbody" | "thead" => {
                for tr in child_elems(child) {
                    if tr.value().name() == "tr" {
                        rows.push(tr);
                    }
                }
            }
            "tr" => rows.push(child),
            _ => {}
        }
    }
    rows
}

/// All descendant text of `el`, skipping any subtree whose root tag is in `stop`.
fn text_of(el: ElementRef, stop: &[&str]) -> String {
    let mut out = String::new();
    collect_text(el, stop, &mut out);
    out
}

fn collect_text(el: ElementRef, stop: &[&str], out: &mut String) {
    for child in el.children() {
        let v = child.value();
        if let Some(t) = v.as_text() {
            out.push_str(&t.text);
        } else if let Some(e) = v.as_element() {
            if stop.contains(&e.name()) {
                continue;
            }
            if let Some(child_el) = ElementRef::wrap(child) {
                out.push(' '); // element boundary → avoid joining words across tags
                collect_text(child_el, stop, out);
            }
        }
    }
}

/// Collapse all whitespace (incl. `&nbsp;`) to single spaces and trim.
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Remove pakistani.org's `[ ]` amendment-edit brackets. The amendment data is
/// captured structurally (per-node `footnote_refs` + `footnotes`), so the
/// bracket characters are redundant markup — drop them and re-collapse spaces.
fn strip_brackets(s: &str) -> String {
    clean(&s.replace(['[', ']'], ""))
}

// ─── amendment metadata: footnote descriptions + catalogue (Layers 2 & 3) ────

/// Classify a footnote description by its change verb.
fn classify_change(raw: &str) -> String {
    let low = raw.to_ascii_lowercase();
    // Editorial / reference footnotes ("For the Rules …, see Gazette …") are
    // not amendments — they point to external material, so they carry no
    // instrument by design.
    if low.starts_with("for ") {
        return "note".to_string();
    }
    // Renumbering ("Renumbered from \"(d)\". by …") is a distinct amendment
    // action — not a substitution/insertion of text.
    if low.contains("renumber") {
        return "renumbered".to_string();
    }
    if low.contains("substitut") || low.starts_with("subs") {
        "substituted"
    } else if low.contains("inserted") || low.starts_with("ins") {
        "inserted"
    } else if low.contains("omitted") || low.contains("deleted") || low.starts_with("omit") {
        "omitted"
    } else if low.contains("added") || low.starts_with("add") {
        "added"
    } else {
        "other"
    }
    .to_string()
}

/// Pull the instrument name out of a footnote description — the text after
/// "… by " up to the section/article citation or the effective-date clause.
/// pakistani.org hyperlinks the instrument for older amendments but leaves
/// newer ones (26th/27th) as plain text, so this parses the text directly.
fn extract_instrument(raw: &str) -> String {
    let Some((_, after)) = raw.split_once(" by ") else {
        return String::new();
    };
    let after = after.strip_prefix("the ").unwrap_or(after);
    let mut end = after.len();
    for marker in [
        ", Section", ", Article", ", Art ", ", Art.", ", Sch", ", s. ",
        "(with effect", " for ",
    ] {
        if let Some(i) = after.find(marker) {
            end = end.min(i);
        }
    }
    let mut inst = after[..end].trim().trim_end_matches(',').trim().to_string();
    // A bare trailing ", 52" is an unlabelled section ref — drop it; a 4-digit
    // year (", 2010") is part of the instrument and kept.
    if let Some((head, tail)) = inst.rsplit_once(", ") {
        if !tail.is_empty() && tail.len() <= 3 && tail.chars().all(|c| c.is_ascii_digit()) {
            inst = head.to_string();
        }
    }
    inst
}

/// Parse a page's `references` block (Layer 2) — one `<div id="N">` per
/// footnote, holding the change verb, the instrument (a link) and often the
/// pre-amendment text after "for".
fn parse_references(doc: &Html) -> Vec<Footnote> {
    let div_sel = Selector::parse("div.references > div[id]").unwrap();
    let mut out = Vec::new();
    for d in doc.select(&div_sel) {
        let id = d.value().attr("id").unwrap_or_default().to_string();
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let raw = clean(&text_of(d, &["sup"]));
        if raw.is_empty() {
            continue;
        }
        let instrument = extract_instrument(&raw);
        let effective_date = raw
            .split_once("with effect from")
            .and_then(|(_, t)| t.split(')').next())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() < 40);
        let previous_text = raw
            .split_once(" for ")
            .map(|(_, t)| strip_brackets(t))
            .filter(|s| !s.is_empty());
        out.push(Footnote {
            id,
            change: classify_change(&raw),
            instrument,
            effective_date,
            previous_text,
            raw,
        });
    }
    out
}

/// Parse the enacted-amendment catalogue (Layer 3) from the index page. Only the
/// list *before* the "Proposed Amendments" heading is enacted — the rest (never-
/// passed / withdrawn bills, which reuse numbers) is deliberately skipped.
fn parse_amendments(index_html: &str) -> Vec<Amendment> {
    let cutoff = index_html
        .find("Proposed Amendment")
        .unwrap_or(index_html.len());
    let doc = Html::parse_fragment(&index_html[..cutoff]);
    let li_sel = Selector::parse("li").unwrap();
    let mut out = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for li in doc.select(&li_sel) {
        // Stop at nested `<ul>` so an entry's passage-detail sub-bullets don't
        // bleed into its title/summary.
        let txt = clean(&text_of(li, &["sup", "ul"]));
        // Each enacted entry reads "<Ordinal> Amendment (YYYY) - <summary>".
        let Some(idx) = txt.find(" Amendment") else {
            continue;
        };
        let Some(number) = ordinal_to_num(txt[..idx].trim()) else {
            continue;
        };
        if !seen.insert(number) {
            continue; // an outer <li> also contains nested passage-detail items
        }
        let year = txt
            .split_once('(')
            .and_then(|(_, t)| t.split(')').next())
            .and_then(|y| y.trim().parse::<u32>().ok());
        let title = txt.split(" - ").next().unwrap_or(&txt).trim().to_string();
        let summary = txt
            .split_once(" - ")
            .map(|(_, s)| s.trim().to_string())
            .unwrap_or_default();
        out.push(Amendment {
            number,
            kind: "amendment".into(),
            year,
            title,
            summary,
        });
    }
    out.sort_by_key(|a| a.number);
    out
}

/// English ordinal word → number, for "First" … "Thirty-Ninth".
fn ordinal_to_num(s: &str) -> Option<u32> {
    const ONES: [(&str, u32); 20] = [
        ("first", 1), ("second", 2), ("third", 3), ("fourth", 4), ("fifth", 5),
        ("sixth", 6), ("seventh", 7), ("eighth", 8), ("ninth", 9), ("tenth", 10),
        ("eleventh", 11), ("twelfth", 12), ("thirteenth", 13), ("fourteenth", 14),
        ("fifteenth", 15), ("sixteenth", 16), ("seventeenth", 17), ("eighteenth", 18),
        ("nineteenth", 19), ("twentieth", 20),
    ];
    let key = s.to_ascii_lowercase();
    let key = key.trim();
    for (w, n) in ONES {
        if key == w {
            return Some(n);
        }
    }
    if key == "thirtieth" {
        return Some(30);
    }
    for (prefix, base) in [("twenty-", 20u32), ("thirty-", 30)] {
        if let Some(rest) = key.strip_prefix(prefix) {
            for (w, n) in ONES {
                if rest == w && n <= 9 {
                    return Some(base + n);
                }
            }
        }
    }
    None
}

// ─── cross-check report (T008) ───────────────────────────────────────────────

fn all_articles(c: &ConstitutionIR) -> Vec<&Article> {
    let mut v = Vec::new();
    for p in &c.parts {
        v.extend(&p.articles);
        for ch in &p.chapters {
            v.extend(&ch.articles);
        }
    }
    v
}

/// True if any of an Article's body text still contains a lone `[ ]` — the
/// scar a dropped nested table (or an amendment-omitted clause) leaves behind.
fn article_has_bracket_scar(a: &Article) -> bool {
    let mut texts: Vec<&str> = a.text.as_deref().into_iter().collect();
    for cl in &a.clauses {
        texts.push(&cl.text);
        for sc in &cl.subclauses {
            texts.push(&sc.text);
        }
    }
    texts.iter().any(|t| t.contains("[ ]"))
}

/// Print the structural cross-check and return whether the S1 acceptance counts
/// hold (~12 Parts, ~290 Articles, every Article has a marginal note).
fn report(c: &ConstitutionIR) -> bool {
    for part in &c.parts {
        let direct = part.articles.len();
        let in_chapters: usize = part.chapters.iter().map(|ch| ch.articles.len()).sum();
        eprintln!(
            "  Part {:<5} {:<46} {:>3} articles ({} direct, {} chapters)",
            part.number,
            truncate(&part.title, 46),
            direct + in_chapters,
            direct,
            part.chapters.len()
        );
        for ch in &part.chapters {
            let first = ch.articles.first().map_or("-", |a| a.number.as_str());
            let last = ch.articles.last().map_or("-", |a| a.number.as_str());
            eprintln!(
                "      Ch {:<4} {:<44} {:>3} articles  [{}-{}]",
                ch.number,
                truncate(&ch.title, 44),
                ch.articles.len(),
                first,
                last
            );
        }
    }

    let arts = all_articles(c);
    let total = arts.len();
    let (mut clauses, mut subclauses, mut provisos, mut explanations) = (0, 0, 0, 0);
    for a in &arts {
        clauses += a.clauses.len();
        provisos += a.provisos.len();
        explanations += a.explanations.len();
        for cl in &a.clauses {
            subclauses += cl.subclauses.len();
            provisos += cl.provisos.len();
            explanations += cl.explanations.len();
            for sc in &cl.subclauses {
                provisos += sc.provisos.len();
                explanations += sc.explanations.len();
            }
        }
    }
    let missing: Vec<&str> = arts
        .iter()
        .filter(|a| a.marginal_note.is_empty())
        .map(|a| a.number.as_str())
        .collect();
    // An Article with no chapeau, no clauses and no proviso captured nothing.
    let empty: Vec<&str> = arts
        .iter()
        .filter(|a| a.text.is_none() && a.clauses.is_empty() && a.provisos.is_empty())
        .map(|a| a.number.as_str())
        .collect();
    // A lone `[ ]` left in body text is the scar of a dropped nested table
    // (or an amendment-omitted clause) — surfaced so a regression is visible.
    let scarred: Vec<&str> = arts
        .iter()
        .filter(|a| article_has_bracket_scar(a))
        .map(|a| a.number.as_str())
        .collect();

    eprintln!("  {}", "-".repeat(72));
    eprintln!("  Parts: {}   Articles: {}", c.parts.len(), total);
    eprintln!(
        "  Clauses: {clauses}   Sub-clauses: {subclauses}   \
         Provisos: {provisos}   Explanations: {explanations}"
    );
    eprintln!(
        "  Preamble: {}   Bracket-scar articles: {}",
        if c.preamble.is_some() {
            "present"
        } else {
            "MISSING"
        },
        scarred.len()
    );
    if !scarred.is_empty() {
        eprintln!("    scarred: {}", scarred.join(", "));
    }
    eprintln!(
        "  Amendments catalogued: {}   Footnote descriptions: {}",
        c.amendments.len(),
        c.footnotes.len()
    );

    let mut ok = true;
    if c.footnotes.is_empty() {
        eprintln!("  FAIL: no footnote descriptions parsed (references blocks)");
        ok = false;
    }
    if c.preamble.is_none() {
        eprintln!("  FAIL: preamble not captured");
        ok = false;
    }
    if !empty.is_empty() {
        eprintln!(
            "  FAIL: {} Article(s) captured no content: {}",
            empty.len(),
            empty.join(", ")
        );
        ok = false;
    }
    if c.parts.len() != 12 {
        eprintln!("  FAIL: expected 12 Parts, got {}", c.parts.len());
        ok = false;
    }
    if !(270..=320).contains(&total) {
        eprintln!("  FAIL: expected ~290 Articles, got {total}");
        ok = false;
    }
    if !missing.is_empty() {
        eprintln!(
            "  FAIL: {} Article(s) missing a marginal note: {}",
            missing.len(),
            missing.join(", ")
        );
        ok = false;
    }
    eprintln!(
        "  {}",
        if ok {
            "PASS — S1 acceptance counts met (manual 10-article spot-check still required)"
        } else {
            "FAIL — see above"
        }
    );
    ok
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Art 8(3)-shaped: the clause cell holds TWO sub-clause tables — original
    /// `(a)` then amendment-inserted `(b)` in its own table inside `[ ]`.
    /// Pre-fix the parser keeps only the first table, dropping `(b)`.
    #[test]
    fn multi_table_subclauses_all_captured() {
        // The (b) row's id cell carries amendment-bracket noise (`[(b)]`) —
        // it must be normalised back to `(b)`.
        let html = r#"<table><tr><td><nobr>(3)</nobr></td><td>chapeau
            <table><tr><td><nobr>(a)</nobr></td><td>alpha</td></tr></table>
            <table><tr><td><nobr>[(b)]</nobr></td><td>bravo</td></tr></table>
        </td></tr></table>"#;
        let doc = Html::parse_fragment(html);
        let table = doc
            .select(&Selector::parse("table").unwrap())
            .next()
            .expect("clause table");
        let clauses = parse_clauses(table);
        assert_eq!(clauses.len(), 1, "one clause (3)");
        let ids: Vec<&str> = clauses[0]
            .subclauses
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["(a)", "(b)"],
            "amendment-inserted (b) in a second table must survive"
        );
    }

    /// Art 18-shaped: a flat article whose proviso hangs in a `<blockquote>` at
    /// the article level (no numbered clause). Pre-fix it is dropped entirely.
    #[test]
    fn article_level_proviso_captured() {
        let html = r#"<table><tr>
            <td><nobr><b>18</b></nobr></td>
            <td><b>Freedom of trade</b><br>chapeau ending in a colon:
                <blockquote>Provided that nothing in this Article shall prevent X.</blockquote>
            </td></tr></table>"#;
        let doc = Html::parse_fragment(html);
        let table = doc
            .select(&Selector::parse("table").unwrap())
            .next()
            .expect("section table");
        let arts = parse_articles(table, &[]);
        assert_eq!(arts.len(), 1);
        assert_eq!(arts[0].number, "18");
        assert_eq!(
            arts[0].provisos.len(),
            1,
            "flat-article proviso must be captured"
        );
    }

    /// Amendment footnote refs must be positioned per node — a clause keeps only
    /// the markers anchored in its own cell, not those inside its sub-clauses.
    #[test]
    fn footnote_refs_are_per_node() {
        // f9 sits in the clause chapeau; f7 inside sub-clause (a).
        let html = r#"<table><tr><td><nobr>(1)</nobr></td><td>chapeau
            <sup><a name="f9">9</a></sup>
            <table><tr><td><nobr>(a)</nobr></td>
              <td>alpha <sup><a name="f7">7</a></sup></td></tr></table>
        </td></tr></table>"#;
        let doc = Html::parse_fragment(html);
        let table = doc
            .select(&Selector::parse("table").unwrap())
            .next()
            .expect("clause table");
        let clauses = parse_clauses(table);
        assert_eq!(
            clauses[0].footnote_refs,
            vec!["9"],
            "clause keeps only its own marker, not the sub-clause's"
        );
        assert_eq!(
            clauses[0].subclauses[0].footnote_refs,
            vec!["7"],
            "sub-clause keeps its own marker"
        );
    }

    /// Footnote descriptions (Layer 2) parse from a `references` block.
    #[test]
    fn references_block_parses_footnotes() {
        let html = r#"<div class="references"><div id="667">
            <a name="667"><sup>667</sup></a>
            Substituted by <a href="x.html">Revival of Constitution Order, 1985</a>,
            Art 2 (with effect from March 2, 1985) for the old text.
        </div></div>"#;
        let doc = Html::parse_fragment(html);
        let fns = parse_references(&doc);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].id, "667");
        assert_eq!(fns[0].change, "substituted");
        assert!(fns[0].instrument.contains("Revival of Constitution"));
    }

    /// A newer-amendment footnote is plain text (no hyperlink) — the instrument
    /// must still be parsed out of the description.
    #[test]
    fn instrument_parsed_from_unlinked_text() {
        let html = r#"<div class="references"><div id="430">
            <a name="430"><sup>430</sup></a>
            Inserted by Constitution (Twenty-seventh Amendment) Act, 2025, Article 19 (with effect from November 13, 2025).
        </div></div>"#;
        let doc = Html::parse_fragment(html);
        let fns = parse_references(&doc);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].change, "inserted");
        assert_eq!(
            fns[0].instrument,
            "Constitution (Twenty-seventh Amendment) Act, 2025"
        );
    }

    /// An editorial "For … see Gazette …" footnote is a reference note, not an
    /// amendment, and is classified `note` rather than `other`.
    #[test]
    fn editorial_footnote_classified_as_note() {
        let html = r#"<div class="references"><div id="174">
            <a name="174"><sup>174</sup></a>
            For the Rules of Procedure, see Gazette of Pakistan, 1973.
        </div></div>"#;
        let doc = Html::parse_fragment(html);
        let fns = parse_references(&doc);
        assert_eq!(fns[0].change, "note");
    }

    /// A bare unlabelled section ref ("…, 52") must not leak into the instrument.
    #[test]
    fn bare_section_ref_trimmed_from_instrument() {
        assert_eq!(
            extract_instrument("Inserted by Constitution (Eighteenth Amendment) Act, 2010, 52 (with effect from April 19, 2010)."),
            "Constitution (Eighteenth Amendment) Act, 2010"
        );
    }
}
