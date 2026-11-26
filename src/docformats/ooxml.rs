//! Office Open XML (ECMA-376 / ISO/IEC 29500): word-processing documents
//! (.docx, read and write), spreadsheets (.xlsx) and presentations (.pptx).

use std::collections::HashMap;

use super::markup::{parse_blocks, BlockKind, Run, TextBuilder};
use super::xml::{self, Event};
use super::zip::{ZipArchive, ZipWriter};

fn flag_on(attrs: &[(String, String)]) -> bool {
    !matches!(Event::attr(attrs, "w:val"), Some("0" | "false" | "off" | "none"))
}

/// Paragraph content collected before its kind is known (pPr may come first,
/// but runs are only emitted at the end of the paragraph).
#[derive(Default)]
struct Para {
    runs: Vec<Run>,
    heading: Option<u8>,
    list: Option<u8>,
    break_before: bool,
    break_after: bool,
}

impl Para {
    fn push(&mut self, t: &str, b: bool, i: bool) {
        match self.runs.last_mut() {
            Some(r) if r.bold == b && r.italic == i => r.text.push_str(t),
            _ => self.runs.push(Run { text: t.to_owned(), bold: b, italic: i }),
        }
    }

    fn kind(&self) -> BlockKind {
        match (self.heading, self.list) {
            (Some(h), _) => BlockKind::Heading(h.clamp(1, 6)),
            (None, Some(d)) => BlockKind::Bullet(d.clamp(1, 6)),
            _ => BlockKind::Para,
        }
    }
}

/// Converts word/document.xml to text.
pub fn docx_to_text(document: &str) -> String {
    let mut tb = TextBuilder::default();
    let mut para: Option<Para> = None;
    let mut para_depth = 0usize;
    let (mut in_ppr, mut in_rpr) = (false, false);
    let (mut rb, mut ri) = (false, false);
    let mut in_text = false;
    let mut skip: Vec<String> = Vec::new();
    let mut table_depth = 0usize;
    let mut cell: Option<String> = None;
    let mut row: Vec<String> = Vec::new();

    for ev in xml::parse(document) {
        if let Some(top) = skip.last() {
            match &ev {
                Event::Start { name, .. } if name == top => skip.push(name.clone()),
                Event::End { name } if name == top => {
                    skip.pop();
                }
                _ => {}
            }
            continue;
        }
        match ev {
            Event::Start { name, attrs } => match name.as_str() {
                "w:del" | "mc:Fallback" | "w:instrText" | "w:delText" | "w:commentRangeStart" | "w:footnotes" => {
                    skip.push(name)
                }
                "w:p" => {
                    para_depth += 1;
                    if para_depth == 1 {
                        para = Some(Para::default());
                    } else if let Some(p) = &mut para {
                        p.push(" ", false, false);
                    }
                }
                "w:pPr" => in_ppr = true,
                "w:rPr" if !in_ppr => in_rpr = true,
                "w:r" => {
                    rb = false;
                    ri = false;
                }
                "w:b" if in_rpr => rb = flag_on(&attrs),
                "w:i" if in_rpr => ri = flag_on(&attrs),
                "w:pStyle" if in_ppr && para_depth == 1 => {
                    let v = Event::attr(&attrs, "w:val").unwrap_or("").to_ascii_lowercase();
                    if let Some(p) = &mut para {
                        if let Some(n) = v.strip_prefix("heading").and_then(|n| n.trim().parse::<u8>().ok()) {
                            p.heading = Some(n);
                        } else if v == "title" {
                            p.heading = Some(1);
                        } else if v == "subtitle" {
                            p.heading = Some(2);
                        } else if v.starts_with("listbullet") || v.starts_with("listparagraph") {
                            p.list.get_or_insert(1);
                        }
                    }
                }
                "w:outlineLvl" if in_ppr => {
                    if let (Some(p), Some(n)) = (&mut para, Event::attr(&attrs, "w:val").and_then(|v| v.parse::<u8>().ok())) {
                        if n < 9 {
                            p.heading = Some(n + 1);
                        }
                    }
                }
                "w:numPr" if in_ppr => {
                    if let Some(p) = &mut para {
                        p.list.get_or_insert(1);
                    }
                }
                "w:ilvl" if in_ppr => {
                    if let (Some(p), Some(n)) = (&mut para, Event::attr(&attrs, "w:val").and_then(|v| v.parse::<u8>().ok())) {
                        p.list = Some(n + 1);
                    }
                }
                "w:t" => in_text = true,
                "w:tab" if !in_ppr => {
                    if let Some(p) = &mut para {
                        p.push("\t", rb, ri);
                    }
                }
                "w:br" if Event::attr(&attrs, "w:type") == Some("page") => {
                    if let Some(p) = &mut para {
                        // Before or after the text, depending on where it sits.
                        if p.runs.iter().all(|r| r.text.trim().is_empty()) {
                            p.break_before = true;
                        } else {
                            p.break_after = true;
                        }
                    }
                }
                "w:pageBreakBefore" if in_ppr && flag_on(&attrs) => {
                    if let Some(p) = &mut para {
                        p.break_before = true;
                    }
                }
                "w:br" | "w:cr" => {
                    if let Some(p) = &mut para {
                        p.push("\n", rb, ri);
                    }
                }
                "w:noBreakHyphen" => {
                    if let Some(p) = &mut para {
                        p.push("-", rb, ri);
                    }
                }
                "w:tbl" => {
                    table_depth += 1;
                    if table_depth == 1 {
                        tb.blank();
                    }
                }
                "w:tr" if table_depth == 1 => row.clear(),
                "w:tc" if table_depth == 1 => cell = Some(String::new()),
                _ => {}
            },
            Event::End { name } => match name.as_str() {
                "w:pPr" => in_ppr = false,
                "w:rPr" => in_rpr = false,
                "w:t" => in_text = false,
                "w:p" => {
                    para_depth = para_depth.saturating_sub(1);
                    if para_depth > 0 {
                        continue;
                    }
                    let Some(p) = para.take() else { continue };
                    if let Some(c) = &mut cell {
                        let mut t = TextBuilder::default();
                        t.start(BlockKind::Para);
                        for r in &p.runs {
                            t.push(&r.text, r.bold, r.italic);
                        }
                        let text = t.take().replace('\n', " ");
                        if !c.is_empty() && !text.is_empty() {
                            c.push(' ');
                        }
                        c.push_str(&text);
                    } else {
                        if p.break_before {
                            tb.page_break();
                        }
                        let only_break = (p.break_before || p.break_after) && p.runs.iter().all(|r| r.text.trim().is_empty());
                        if !only_break {
                            tb.start(p.kind());
                            for r in &p.runs {
                                tb.push(&r.text, r.bold, r.italic);
                            }
                            tb.end();
                        }
                        if p.break_after {
                            tb.page_break();
                        }
                    }
                }
                "w:tc" if table_depth == 1 => {
                    if let Some(c) = cell.take() {
                        row.push(c);
                    }
                }
                "w:tr" if table_depth == 1 => {
                    while row.last().is_some_and(String::is_empty) {
                        row.pop();
                    }
                    if !row.is_empty() {
                        tb.raw_line(row.join(" | "));
                    }
                }
                "w:tbl" => {
                    if table_depth == 1 {
                        tb.blank();
                    }
                    table_depth = table_depth.saturating_sub(1);
                }
                _ => {}
            },
            Event::Text(t) => {
                if in_text {
                    if let Some(p) = &mut para {
                        p.push(&t, rb, ri);
                    }
                }
            }
        }
    }
    tb.finish()
}

pub fn import_docx(data: &[u8]) -> Result<String, String> {
    let z = ZipArchive::parse(data)?;
    Ok(docx_to_text(&z.read_text("word/document.xml")?))
}

/// Parses a .rels file into Id -> Target.
fn rels(xml_text: &str) -> HashMap<String, String> {
    xml::parse(xml_text)
        .into_iter()
        .filter_map(|e| match e {
            Event::Start { name, attrs } if xml::local(&name) == "Relationship" => Some((
                Event::attr(&attrs, "Id")?.to_owned(),
                Event::attr(&attrs, "Target")?.to_owned(),
            )),
            _ => None,
        })
        .collect()
}

fn resolve(base_dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_owned();
    }
    let mut parts: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Trailing number of a part name: "ppt/slides/slide12.xml" -> 12.
fn natural_key(name: &str) -> u64 {
    let stem = name.trim_end_matches(".xml");
    let digits = &stem[stem.trim_end_matches(|c: char| c.is_ascii_digit()).len()..];
    digits.parse().unwrap_or(0)
}

/// Converts a column reference like "AB12" to a zero-based column index.
fn column_index(cell_ref: &str) -> Option<usize> {
    let letters: String = cell_ref.chars().take_while(char::is_ascii_alphabetic).collect();
    if letters.is_empty() {
        return None;
    }
    Some(letters.bytes().fold(0usize, |acc, b| acc * 26 + (b.to_ascii_uppercase() - b'A' + 1) as usize) - 1)
}

pub fn import_xlsx(data: &[u8]) -> Result<String, String> {
    let z = ZipArchive::parse(data)?;
    let mut shared: Vec<String> = Vec::new();
    if let Ok(s) = z.read_text("xl/sharedStrings.xml") {
        let mut cur: Option<String> = None;
        let (mut in_t, mut skip_rph) = (false, false);
        for ev in xml::parse(&s) {
            match ev {
                Event::Start { name, .. } if name == "si" => cur = Some(String::new()),
                Event::Start { name, .. } if name == "rPh" => skip_rph = true,
                Event::End { name } if name == "rPh" => skip_rph = false,
                Event::Start { name, .. } if name == "t" => in_t = true,
                Event::End { name } if name == "t" => in_t = false,
                Event::Text(t) if in_t && !skip_rph => {
                    if let Some(c) = &mut cur {
                        c.push_str(&t);
                    }
                }
                Event::End { name } if name == "si" => shared.push(cur.take().unwrap_or_default()),
                _ => {}
            }
        }
    }
    let wb = z.read_text("xl/workbook.xml")?;
    let wb_rels = z.read_text("xl/_rels/workbook.xml.rels").map(|r| rels(&r)).unwrap_or_default();
    let mut sheets: Vec<(String, String)> = Vec::new();
    for ev in xml::parse(&wb) {
        if let Event::Start { name, attrs } = ev {
            if xml::local(&name) == "sheet" {
                let sname = Event::attr(&attrs, "name").unwrap_or("Sheet").to_owned();
                let path = Event::attr(&attrs, "r:id")
                    .and_then(|id| wb_rels.get(id))
                    .map(|t| resolve("xl", t))
                    .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", sheets.len() + 1));
                sheets.push((sname, path));
            }
        }
    }
    let mut tb = TextBuilder::default();
    for (sname, path) in sheets {
        let Ok(xml_text) = z.read_text(&path) else { continue };
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut row_idx = 0usize;
        let mut col = 0usize;
        let mut ctype = String::new();
        let mut value = String::new();
        let (mut in_v, mut in_t) = (false, false);
        for ev in xml::parse(&xml_text) {
            match ev {
                Event::Start { name, attrs } if name == "row" => {
                    row_idx = Event::attr(&attrs, "r").and_then(|r| r.parse::<usize>().ok()).map_or(rows.len(), |r| r - 1);
                    col = 0;
                }
                Event::Start { name, attrs } if name == "c" => {
                    if let Some(c) = Event::attr(&attrs, "r").and_then(column_index) {
                        col = c;
                    }
                    ctype = Event::attr(&attrs, "t").unwrap_or("n").to_owned();
                    value.clear();
                }
                Event::Start { name, .. } if name == "v" => in_v = true,
                Event::End { name } if name == "v" => in_v = false,
                Event::Start { name, .. } if name == "t" => in_t = true,
                Event::End { name } if name == "t" => in_t = false,
                Event::Text(t) if in_v || in_t => value.push_str(&t),
                Event::End { name } if name == "c" => {
                    let text = match ctype.as_str() {
                        "s" => value.trim().parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
                        "b" => if value.trim() == "1" { "TRUE".into() } else { "FALSE".into() },
                        _ => value.clone(),
                    };
                    if row_idx < 200_000 && col < 1000 {
                        if rows.len() <= row_idx {
                            rows.resize(row_idx + 1, Vec::new());
                        }
                        let r = &mut rows[row_idx];
                        if r.len() <= col {
                            r.resize(col + 1, String::new());
                        }
                        r[col] = text.replace(['\n', '\t'], " ");
                    }
                    col += 1;
                }
                _ => {}
            }
        }
        tb.blank();
        tb.raw_line(format!("## Sheet: {sname}"));
        while rows.last().is_some_and(|r| r.iter().all(String::is_empty)) {
            rows.pop();
        }
        for r in rows {
            tb.raw_line(r.join("\t").trim_end_matches('\t').to_owned());
        }
    }
    Ok(tb.finish())
}

pub fn import_pptx(data: &[u8]) -> Result<String, String> {
    let z = ZipArchive::parse(data)?;
    let mut slides: Vec<String> = Vec::new();
    if let (Ok(pres), Ok(r)) = (z.read_text("ppt/presentation.xml"), z.read_text("ppt/_rels/presentation.xml.rels")) {
        let map = rels(&r);
        for ev in xml::parse(&pres) {
            if let Event::Start { name, attrs } = ev {
                if name == "p:sldId" {
                    if let Some(t) = Event::attr(&attrs, "r:id").and_then(|id| map.get(id)) {
                        slides.push(resolve("ppt", t));
                    }
                }
            }
        }
    }
    if slides.is_empty() {
        slides = z
            .entries
            .iter()
            .map(|e| e.name.clone())
            .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
            .collect();
        slides.sort_by_key(|n| natural_key(n));
    }
    let mut tb = TextBuilder::default();
    for (i, path) in slides.iter().enumerate() {
        let Ok(x) = z.read_text(path) else { continue };
        tb.blank();
        tb.raw_line(format!("## Slide {}", i + 1));
        let (mut is_title, mut body_ph) = (false, false);
        let mut depth: u8 = 1;
        let mut para: Option<Para> = None;
        let (mut rb, mut ri, mut in_t) = (false, false, false);
        let mut cells: Option<Vec<String>> = None;
        for ev in xml::parse(&x) {
            match ev {
                Event::Start { name, attrs } => match name.as_str() {
                    "p:sp" => {
                        is_title = false;
                        body_ph = false;
                    }
                    "p:ph" => {
                        let t = Event::attr(&attrs, "type");
                        is_title = matches!(t, Some("title" | "ctrTitle"));
                        // Body / content placeholders show bullets by default.
                        body_ph = !is_title && !matches!(t, Some("subTitle" | "dt" | "ftr" | "sldNum" | "hdr" | "pic"));
                    }
                    "a:tr" => cells = Some(Vec::new()),
                    "a:p" => {
                        let mut p = Para::default();
                        depth = 1;
                        if is_title {
                            p.heading = Some(3);
                        } else if body_ph {
                            p.list = Some(1);
                        }
                        para = Some(p);
                    }
                    "a:pPr" if !is_title => {
                        depth = Event::attr(&attrs, "lvl").and_then(|v| v.parse::<u8>().ok()).map_or(1, |l| l + 1);
                        if let Some(p) = &mut para {
                            if p.list.is_some() {
                                p.list = Some(depth);
                            }
                        }
                    }
                    "a:buNone" => {
                        if let Some(p) = &mut para {
                            p.list = None;
                        }
                    }
                    "a:buChar" | "a:buAutoNum" | "a:buBlip" if !is_title => {
                        if let Some(p) = &mut para {
                            p.list = Some(depth);
                        }
                    }
                    "a:rPr" | "a:endParaRPr" => {
                        rb = matches!(Event::attr(&attrs, "b"), Some("1" | "true"));
                        ri = matches!(Event::attr(&attrs, "i"), Some("1" | "true"));
                    }
                    "a:r" | "a:fld" => {
                        rb = false;
                        ri = false;
                    }
                    "a:t" => in_t = true,
                    "a:br" => {
                        if let Some(p) = &mut para {
                            p.push("\n", rb, ri);
                        }
                    }
                    _ => {}
                },
                Event::End { name } => match name.as_str() {
                    "a:t" => in_t = false,
                    "a:p" => {
                        if let Some(p) = para.take() {
                            let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
                            if let Some(c) = &mut cells {
                                c.push(text.replace('\n', " "));
                            } else if !text.trim().is_empty() {
                                tb.start(p.kind());
                                for r in &p.runs {
                                    tb.push(&r.text, r.bold, r.italic);
                                }
                                tb.end();
                            }
                        }
                    }
                    "a:tr" => {
                        if let Some(c) = cells.take() {
                            tb.raw_line(c.join(" | "));
                        }
                    }
                    _ => {}
                },
                Event::Text(t) if in_t => {
                    if let Some(p) = &mut para {
                        p.push(&t, rb, ri);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(tb.finish())
}

// ----------------------------------------------------------------------
// Export (.docx)
// ----------------------------------------------------------------------

fn run_xml(out: &mut String, r: &Run) {
    let props = match (r.bold, r.italic) {
        (true, true) => "<w:rPr><w:b/><w:i/></w:rPr>",
        (true, false) => "<w:rPr><w:b/></w:rPr>",
        (false, true) => "<w:rPr><w:i/></w:rPr>",
        _ => "",
    };
    out.push_str("<w:r>");
    out.push_str(props);
    for (li, line) in r.text.split('\n').enumerate() {
        if li > 0 {
            out.push_str("<w:br/>");
        }
        for (ti, piece) in line.split('\t').enumerate() {
            if ti > 0 {
                out.push_str("<w:tab/>");
            }
            if !piece.is_empty() {
                out.push_str("<w:t xml:space=\"preserve\">");
                out.push_str(&xml::escape(piece));
                out.push_str("</w:t>");
            }
        }
    }
    out.push_str("</w:r>");
}

fn document_xml(text: &str) -> String {
    let mut body = String::new();
    for b in parse_blocks(text) {
        if b.kind == BlockKind::PageBreak {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
            continue;
        }
        body.push_str("<w:p>");
        match b.kind {
            BlockKind::Heading(n) => body.push_str(&format!("<w:pPr><w:pStyle w:val=\"Heading{n}\"/></w:pPr>")),
            BlockKind::Bullet(d) => body.push_str(&format!(
                "<w:pPr><w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>",
                d - 1
            )),
            BlockKind::Para | BlockKind::PageBreak => {}
        }
        for r in &b.runs {
            run_xml(&mut body, r);
        }
        body.push_str("</w:p>");
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}\
         <w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr>\
         </w:body></w:document>"
    )
}

fn styles_docx() -> String {
    let mut headings = String::new();
    for (n, half_points) in [(1, 48), (2, 40), (3, 32), (4, 28), (5, 24), (6, 22)] {
        headings.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{n}\"><w:name w:val=\"heading {n}\"/><w:basedOn w:val=\"Normal\"/>\
             <w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"{}\"/></w:pPr>\
             <w:rPr><w:b/><w:sz w:val=\"{half_points}\"/></w:rPr></w:style>",
            n - 1
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"Calibri\" w:cs=\"Calibri\"/>\
         <w:sz w:val=\"24\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"120\"/></w:pPr></w:pPrDefault></w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/>\
         <w:pPr><w:spacing w:after=\"40\"/><w:contextualSpacing/></w:pPr></w:style>{headings}</w:styles>"
    )
}

fn numbering_docx() -> String {
    let mut levels = String::new();
    for l in 0..9 {
        let indent = 720 * (l + 1);
        let ch = ["•", "◦", "▪"][l % 3];
        levels.push_str(&format!(
            "<w:lvl w:ilvl=\"{l}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"{ch}\"/>\
             <w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{indent}\" w:hanging=\"360\"/></w:pPr></w:lvl>"
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>{levels}</w:abstractNum>\
         <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num></w:numbering>"
    )
}

pub fn export_docx(text: &str) -> Vec<u8> {
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
        <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
        <Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>\
        </Types>";
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
        </Relationships>";
    let doc_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
        <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>\
        </Relationships>";
    let mut z = ZipWriter::default();
    z.add("[Content_Types].xml", content_types.as_bytes());
    z.add("_rels/.rels", root_rels.as_bytes());
    z.add("word/_rels/document.xml.rels", doc_rels.as_bytes());
    z.add("word/document.xml", document_xml(text).as_bytes());
    z.add("word/styles.xml", styles_docx().as_bytes());
    z.add("word/numbering.xml", numbering_docx().as_bytes());
    z.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_roundtrip() {
        let src = "# Report\nPlain **bold** *it*\n- a\n  - b\n\nTab\there\n";
        let back = import_docx(&export_docx(src)).unwrap();
        assert_eq!(back, src);
    }

    #[test]
    fn docx_page_breaks() {
        let src = "One\n\u{c}\n# Two\n";
        assert_eq!(import_docx(&export_docx(src)).unwrap(), src);
        // Word's own form: a break run at the end of a paragraph.
        let x = r#"<w:document><w:body><w:p><w:r><w:t>A</w:t><w:br w:type="page"/></w:r></w:p>
            <w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>B</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(docx_to_text(x), "A\n\u{c}\n\u{c}\nB\n");
    }

    #[test]
    fn docx_tables_and_deleted_text() {
        let x = r#"<w:document><w:body>
            <w:p><w:r><w:t>Keep</w:t></w:r><w:del><w:r><w:delText>gone</w:delText></w:r></w:del></w:p>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            </w:body></w:document>"#;
        assert_eq!(docx_to_text(x), "Keep\n\nA | B\n");
    }

    #[test]
    fn helpers() {
        assert_eq!(column_index("A1"), Some(0));
        assert_eq!(column_index("AB7"), Some(27));
        assert_eq!(resolve("ppt", "slides/slide2.xml"), "ppt/slides/slide2.xml");
        assert_eq!(resolve("xl", "/xl/worksheets/s.xml"), "xl/worksheets/s.xml");
        assert_eq!(resolve("ppt/slides", "../media/a.png"), "ppt/media/a.png");
    }
}
