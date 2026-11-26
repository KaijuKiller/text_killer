//! OpenDocument (ODF, ISO/IEC 26300): text documents (.odt), spreadsheets
//! (.ods), presentations (.odp) and their flat-XML variants (.fodt etc.).
//! Text documents can also be written.

use std::collections::HashMap;

use super::markup::{parse_blocks, BlockKind, Run, TextBuilder};
use super::xml::{self, Event};
use super::zip::{ZipArchive, ZipWriter};

/// Collects bold/italic flags of named styles from styles.xml / content.xml.
/// (page break before, page break after) for paragraph styles.
type Breaks = HashMap<String, (bool, bool)>;

fn collect_styles(
    xml_text: &str,
    map: &mut HashMap<String, (bool, bool)>,
    parents: &mut HashMap<String, String>,
    breaks: &mut Breaks,
) {
    let mut current: Option<String> = None;
    for ev in xml::parse(xml_text) {
        match ev {
            Event::Start { name, attrs } if name == "style:style" => {
                current = Event::attr(&attrs, "style:name").map(str::to_owned);
                if let (Some(c), Some(p)) = (&current, Event::attr(&attrs, "style:parent-style-name")) {
                    parents.insert(c.clone(), p.to_owned());
                }
                // Inherit from a parent style that was already seen.
                if let (Some(c), Some(p)) = (&current, Event::attr(&attrs, "style:parent-style-name")) {
                    if let Some(&flags) = map.get(p) {
                        map.insert(c.clone(), flags);
                    }
                }
            }
            Event::Start { name, attrs } if name == "style:text-properties" => {
                if let Some(c) = &current {
                    let e = map.entry(c.clone()).or_default();
                    if let Some(w) = Event::attr(&attrs, "fo:font-weight") {
                        e.0 = w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600);
                    }
                    if let Some(s) = Event::attr(&attrs, "fo:font-style") {
                        e.1 = s == "italic" || s == "oblique";
                    }
                }
            }
            Event::Start { name, attrs } if name == "style:paragraph-properties" => {
                if let Some(c) = &current {
                    let before = Event::attr(&attrs, "fo:break-before") == Some("page");
                    let after = Event::attr(&attrs, "fo:break-after") == Some("page");
                    if before || after {
                        breaks.insert(c.clone(), (before, after));
                    }
                }
            }
            Event::End { name } if name == "style:style" => current = None,
            _ => {}
        }
    }
}

/// Heading level implied by a paragraph style or one of its ancestors
/// ("Title", "Subtitle", "Heading_20_2", ...).
fn style_heading(style: &str, parents: &HashMap<String, String>) -> Option<u8> {
    let mut cur = style;
    for _ in 0..12 {
        let n = cur.replace("_20_", " ");
        match n.as_str() {
            "Title" => return Some(1),
            "Subtitle" => return Some(2),
            _ => {}
        }
        if let Some(l) = n.strip_prefix("Heading ").and_then(|l| l.parse::<u8>().ok()) {
            return Some(l);
        }
        cur = parents.get(cur)?;
    }
    None
}

/// Elements whose content is not part of the visible document text.
const SKIP: &[&str] = &[
    "text:note",
    "office:annotation",
    "text:tracked-changes",
    "svg:title",
    "svg:desc",
    "text:sequence-decls",
    "office:forms",
    "presentation:notes",
    "office:scripts",
    "table:shapes",
    "text:table-of-content-source",
    "text:alphabetical-index-source",
];

#[derive(PartialEq, Eq, Clone, Copy)]
enum DocType {
    Text,
    Sheet,
    Slides,
}

struct Cell {
    tb: TextBuilder,
    repeat: usize,
}

/// Converts ODF XML (content.xml or a flat .fod* document) to text.
pub fn to_text(content: &str, styles: Option<&str>) -> String {
    let mut map = HashMap::new();
    let mut parents = HashMap::new();
    let mut breaks = Breaks::new();
    if let Some(s) = styles {
        collect_styles(s, &mut map, &mut parents, &mut breaks);
    }
    collect_styles(content, &mut map, &mut parents, &mut breaks);
    let mut break_after = false;
    let mut frame_class = String::new();

    let mut tb = TextBuilder::default();
    let mut doc_type = DocType::Text;
    let mut in_body = false;
    let mut skip: Vec<String> = Vec::new();
    let mut fmt: Vec<(bool, bool)> = vec![(false, false)];
    let mut list_depth: u8 = 0;
    let mut pending_bullet = false;
    let mut para_depth = 0usize;
    let mut cell: Option<Cell> = None;
    let mut row: Vec<String> = Vec::new();
    let mut row_repeat = 1usize;
    let mut sheet_rows: Vec<String> = Vec::new();
    let mut table_depth = 0usize;
    let mut slide_no = 0usize;

    for ev in xml::parse(content) {
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
            Event::Start { name, attrs } => {
                if !in_body {
                    match name.as_str() {
                        "office:body" => in_body = true,
                        _ => continue,
                    }
                    continue;
                }
                if SKIP.contains(&name.as_str()) {
                    skip.push(name);
                    continue;
                }
                match name.as_str() {
                    "office:spreadsheet" => doc_type = DocType::Sheet,
                    "office:presentation" => doc_type = DocType::Slides,
                    "draw:page" if doc_type == DocType::Slides => {
                        slide_no += 1;
                        tb.blank();
                        let pname = Event::attr(&attrs, "draw:name").unwrap_or("");
                        let default_name = pname.starts_with("page") && pname[4..].chars().all(|c| c.is_ascii_digit());
                        let title = if pname.is_empty() || default_name { String::new() } else { format!(" — {pname}") };
                        tb.raw_line(format!("## Slide {slide_no}{title}"));
                    }
                    "draw:frame" => {
                        frame_class = Event::attr(&attrs, "presentation:class").unwrap_or("").to_owned();
                    }
                    "table:table" => {
                        table_depth += 1;
                        if table_depth == 1 {
                            tb.blank();
                            if doc_type == DocType::Sheet {
                                let tname = Event::attr(&attrs, "table:name").unwrap_or("Sheet");
                                tb.raw_line(format!("## Sheet: {tname}"));
                                sheet_rows.clear();
                            }
                        }
                    }
                    "table:table-row" if table_depth == 1 => {
                        row.clear();
                        row_repeat = Event::attr(&attrs, "table:number-rows-repeated")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1);
                    }
                    "table:table-cell" | "table:covered-table-cell" if table_depth == 1 => {
                        let repeat = Event::attr(&attrs, "table:number-columns-repeated")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1usize);
                        cell = Some(Cell { tb: TextBuilder::default(), repeat });
                    }
                    "text:list" => list_depth = list_depth.saturating_add(1),
                    "text:list-item" | "text:list-header" => pending_bullet = name == "text:list-item",
                    "text:p" | "text:h" => {
                        para_depth += 1;
                        let style_name = Event::attr(&attrs, "text:style-name").unwrap_or("");
                        let (pb_before, pb_after) = breaks.get(style_name).copied().unwrap_or_default();
                        if para_depth == 1 && cell.is_none() {
                            if pb_before && !tb.lines.is_empty() {
                                tb.page_break();
                            }
                            break_after = pb_after;
                        }
                        let style = map.get(style_name).copied();
                        let style_level = style_heading(style_name, &parents);
                        let base = *fmt.last().unwrap_or(&(false, false));
                        // Heading styles are usually bold; that is implied by "#".
                        let is_heading = name == "text:h" || style_level.is_some() || frame_class == "title";
                        fmt.push(if is_heading { base } else { style.unwrap_or(base) });
                        if para_depth > 1 {
                            // Paragraph nested in a frame inside a paragraph.
                            target(&mut tb, &mut cell).push(" ", false, false);
                            continue;
                        }
                        let kind = if name == "text:h" {
                            let lvl = Event::attr(&attrs, "text:outline-level")
                                .and_then(|v| v.parse().ok())
                                .or(style_level)
                                .unwrap_or(1u8);
                            BlockKind::Heading(lvl.clamp(1, 6))
                        } else if let Some(l) = style_level {
                            BlockKind::Heading(l.clamp(1, 6))
                        } else if doc_type == DocType::Slides && frame_class == "title" {
                            BlockKind::Heading(3)
                        } else if doc_type == DocType::Slides && frame_class == "outline" {
                            BlockKind::Bullet(list_depth.clamp(1, 6))
                        } else if list_depth > 0 && pending_bullet {
                            pending_bullet = false;
                            BlockKind::Bullet(list_depth.min(6))
                        } else {
                            BlockKind::Para
                        };
                        match &mut cell {
                            Some(c) => {
                                if c.tb.is_open() {
                                    c.tb.push(" ", false, false);
                                } else {
                                    c.tb.start(BlockKind::Para);
                                }
                            }
                            None => tb.start(kind),
                        }
                    }
                    "text:span" | "text:a" => {
                        let style = Event::attr(&attrs, "text:style-name").and_then(|s| map.get(s)).copied();
                        let base = *fmt.last().unwrap_or(&(false, false));
                        fmt.push(match style {
                            Some((b, i)) => (b || base.0, i || base.1),
                            None => base,
                        });
                    }
                    "text:s" => {
                        let n = Event::attr(&attrs, "text:c").and_then(|v| v.parse().ok()).unwrap_or(1usize).min(200);
                        let (b, i) = *fmt.last().unwrap_or(&(false, false));
                        target(&mut tb, &mut cell).push(&" ".repeat(n), b, i);
                    }
                    "text:tab" => {
                        let (b, i) = *fmt.last().unwrap_or(&(false, false));
                        target(&mut tb, &mut cell).push("\t", b, i);
                    }
                    "text:line-break" => {
                        let (b, i) = *fmt.last().unwrap_or(&(false, false));
                        target(&mut tb, &mut cell).push("\n", b, i);
                    }
                    _ => {}
                }
            }
            Event::End { name } => {
                if !in_body {
                    continue;
                }
                match name.as_str() {
                    "office:body" => in_body = false,
                    "text:p" | "text:h" => {
                        fmt.pop();
                        para_depth = para_depth.saturating_sub(1);
                        if para_depth == 0 && cell.is_none() {
                            // Slides have many empty placeholder paragraphs.
                            if doc_type == DocType::Slides && tb.is_blank() {
                                tb.discard();
                                continue;
                            }
                            // Keep empty paragraphs as blank lines.
                            if !tb.is_open() {
                                tb.start(BlockKind::Para);
                            }
                            tb.end();
                            if std::mem::take(&mut break_after) {
                                tb.page_break();
                            }
                        }
                    }
                    "text:span" | "text:a" => {
                        fmt.pop();
                    }
                    "text:list" => list_depth = list_depth.saturating_sub(1),
                    "draw:frame" => frame_class.clear(),
                    "table:table-cell" | "table:covered-table-cell" if table_depth == 1 => {
                        if let Some(mut c) = cell.take() {
                            let text = c.tb.take().replace('\n', " ");
                            let n = c.repeat.min(if text.is_empty() { 1000 } else { 100 });
                            for _ in 0..n {
                                if row.len() >= 1000 {
                                    break;
                                }
                                row.push(text.clone());
                            }
                        }
                    }
                    "table:table-row" if table_depth == 1 => {
                        while row.last().is_some_and(String::is_empty) {
                            row.pop();
                        }
                        let sep = if doc_type == DocType::Sheet { "\t" } else { " | " };
                        let line = row.join(sep);
                        if doc_type == DocType::Sheet {
                            let n = if line.is_empty() { row_repeat.min(1000) } else { row_repeat.min(10_000) };
                            for _ in 0..n {
                                sheet_rows.push(line.clone());
                            }
                        } else if !line.is_empty() {
                            tb.raw_line(line);
                        }
                    }
                    "table:table" => {
                        if table_depth == 1 {
                            if doc_type == DocType::Sheet {
                                while sheet_rows.last().is_some_and(String::is_empty) {
                                    sheet_rows.pop();
                                }
                                for r in sheet_rows.drain(..) {
                                    tb.raw_line(r);
                                }
                            }
                            tb.blank();
                        }
                        table_depth = table_depth.saturating_sub(1);
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if !in_body || para_depth == 0 {
                    continue;
                }
                // ODF collapses whitespace in paragraph content.
                let collapsed = collapse_ws(&t);
                let (b, i) = *fmt.last().unwrap_or(&(false, false));
                target(&mut tb, &mut cell).push(&collapsed, b, i);
            }
        }
    }
    tb.finish()
}

fn target<'a>(tb: &'a mut TextBuilder, cell: &'a mut Option<Cell>) -> &'a mut TextBuilder {
    match cell {
        Some(c) => &mut c.tb,
        None => tb,
    }
}

pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

pub fn import_package(data: &[u8]) -> Result<String, String> {
    let z = ZipArchive::parse(data)?;
    let content = z.read_text("content.xml")?;
    let styles = z.read_text("styles.xml").ok();
    Ok(to_text(&content, styles.as_deref()))
}

// ----------------------------------------------------------------------
// Export (.odt)
// ----------------------------------------------------------------------

const NS: &str = concat!(
    r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
    r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
    r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
    r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
    r#"xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" "#,
    r#"xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" "#,
    r#"xmlns:dc="http://purl.org/dc/elements/1.1/" office:version="1.3""#
);

/// Writes run text, encoding tabs, line breaks and repeated spaces the way
/// ODF requires. `at_start` tracks whether we are at a paragraph start.
fn write_text(out: &mut String, s: &str, at_start: &mut bool) {
    let mut spaces = 0usize;
    let flush = |out: &mut String, spaces: &mut usize, at_start: &mut bool| {
        if *spaces == 0 {
            return;
        }
        if *at_start {
            out.push_str(&format!("<text:s text:c=\"{spaces}\"/>"));
        } else {
            out.push(' ');
            if *spaces > 1 {
                out.push_str(&format!("<text:s text:c=\"{}\"/>", *spaces - 1));
            }
        }
        *spaces = 0;
        *at_start = false;
    };
    for c in s.chars() {
        match c {
            ' ' => spaces += 1,
            '\t' => {
                flush(out, &mut spaces, at_start);
                out.push_str("<text:tab/>");
                *at_start = false;
            }
            '\n' => {
                flush(out, &mut spaces, at_start);
                out.push_str("<text:line-break/>");
                *at_start = true;
            }
            c => {
                flush(out, &mut spaces, at_start);
                out.push_str(&xml::escape(&c.to_string()));
                *at_start = false;
            }
        }
    }
    flush(out, &mut spaces, at_start);
}

fn write_runs(out: &mut String, runs: &[Run]) {
    let mut at_start = true;
    for r in runs {
        let style = match (r.bold, r.italic) {
            (true, true) => Some("T3"),
            (true, false) => Some("T1"),
            (false, true) => Some("T2"),
            _ => None,
        };
        if let Some(s) = style {
            out.push_str(&format!("<text:span text:style-name=\"{s}\">"));
        }
        write_text(out, &r.text, &mut at_start);
        if style.is_some() {
            out.push_str("</text:span>");
        }
    }
}

fn content_xml(text: &str) -> String {
    let mut body = String::new();
    let mut depth = 0u8;
    // A page break is written as "break before" on the next paragraph.
    let mut pending_break = false;
    for b in parse_blocks(text) {
        let brk = pending_break && b.kind != BlockKind::PageBreak;
        if brk {
            pending_break = false;
        }
        match b.kind {
            BlockKind::Bullet(d) => {
                if depth > 0 && d <= depth {
                    while depth > d {
                        body.push_str("</text:list-item></text:list>");
                        depth -= 1;
                    }
                    body.push_str("</text:list-item><text:list-item>");
                } else {
                    while depth < d {
                        body.push_str(if depth == 0 { "<text:list text:style-name=\"L1\">" } else { "<text:list>" });
                        body.push_str("<text:list-item>");
                        depth += 1;
                    }
                }
                body.push_str(if brk {
                    "<text:p text:style-name=\"Lbreak\">"
                } else {
                    "<text:p text:style-name=\"List_20_Paragraph\">"
                });
                write_runs(&mut body, &b.runs);
                body.push_str("</text:p>");
                continue;
            }
            _ => {
                while depth > 0 {
                    body.push_str("</text:list-item></text:list>");
                    depth -= 1;
                }
            }
        }
        match b.kind {
            BlockKind::PageBreak => pending_break = true,
            BlockKind::Heading(n) => {
                let style = if brk { format!("Hbreak{n}") } else { format!("Heading_20_{n}") };
                body.push_str(&format!("<text:h text:style-name=\"{style}\" text:outline-level=\"{n}\">"));
                write_runs(&mut body, &b.runs);
                body.push_str("</text:h>");
            }
            _ => {
                body.push_str(if brk { "<text:p text:style-name=\"Pbreak\">" } else { "<text:p text:style-name=\"Standard\">" });
                write_runs(&mut body, &b.runs);
                body.push_str("</text:p>");
            }
        }
    }
    while depth > 0 {
        body.push_str("</text:list-item></text:list>");
        depth -= 1;
    }
    if pending_break {
        body.push_str("<text:p text:style-name=\"Pbreak\"/>");
    }

    let mut break_styles = String::from(
        "<style:style style:name=\"Pbreak\" style:family=\"paragraph\" style:parent-style-name=\"Standard\">\
         <style:paragraph-properties fo:break-before=\"page\"/></style:style>\
         <style:style style:name=\"Lbreak\" style:family=\"paragraph\" style:parent-style-name=\"List_20_Paragraph\">\
         <style:paragraph-properties fo:break-before=\"page\"/></style:style>",
    );
    for n in 1..=6 {
        break_styles.push_str(&format!(
            "<style:style style:name=\"Hbreak{n}\" style:family=\"paragraph\" style:parent-style-name=\"Heading_20_{n}\">\
             <style:paragraph-properties fo:break-before=\"page\"/></style:style>"
        ));
    }

    let mut levels = String::new();
    for n in 1..=6 {
        let margin = 0.25 * n as f64 + 0.25;
        levels.push_str(&format!(
            "<text:list-level-style-bullet text:level=\"{n}\" text:bullet-char=\"{}\">\
             <style:list-level-properties text:list-level-position-and-space-mode=\"label-alignment\">\
             <style:list-level-label-alignment text:label-followed-by=\"listtab\" \
             text:list-tab-stop-position=\"{margin:.2}in\" fo:text-indent=\"-0.25in\" fo:margin-left=\"{margin:.2}in\"/>\
             </style:list-level-properties></text:list-level-style-bullet>",
            ["•", "◦", "▪", "•", "◦", "▪"][n - 1]
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS}>\
         <office:automatic-styles>\
         <style:style style:name=\"T1\" style:family=\"text\"><style:text-properties fo:font-weight=\"bold\" style:font-weight-asian=\"bold\" style:font-weight-complex=\"bold\"/></style:style>\
         <style:style style:name=\"T2\" style:family=\"text\"><style:text-properties fo:font-style=\"italic\" style:font-style-asian=\"italic\" style:font-style-complex=\"italic\"/></style:style>\
         <style:style style:name=\"T3\" style:family=\"text\"><style:text-properties fo:font-weight=\"bold\" fo:font-style=\"italic\" style:font-weight-asian=\"bold\" style:font-style-asian=\"italic\"/></style:style>\
         {break_styles}<text:list-style style:name=\"L1\">{levels}</text:list-style>\
         </office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    )
}

fn styles_xml() -> String {
    let mut headings = String::new();
    for (n, size) in [(1, 24), (2, 20), (3, 16), (4, 14), (5, 12), (6, 11)] {
        headings.push_str(&format!(
            "<style:style style:name=\"Heading_20_{n}\" style:display-name=\"Heading {n}\" style:family=\"paragraph\" \
             style:parent-style-name=\"Heading\" style:next-style-name=\"Standard\" style:default-outline-level=\"{n}\" style:class=\"text\">\
             <style:text-properties fo:font-size=\"{size}pt\" fo:font-weight=\"bold\" style:font-weight-asian=\"bold\" style:font-weight-complex=\"bold\"/></style:style>"
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS}>\
         <office:font-face-decls>\
         <style:font-face style:name=\"Liberation Serif\" svg:font-family=\"'Liberation Serif'\" style:font-family-generic=\"roman\"/>\
         <style:font-face style:name=\"Liberation Sans\" svg:font-family=\"'Liberation Sans'\" style:font-family-generic=\"swiss\"/>\
         </office:font-face-decls><office:styles>\
         <style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:margin-top=\"0in\" fo:margin-bottom=\"0.1in\"/>\
         <style:text-properties style:font-name=\"Liberation Serif\" fo:font-size=\"12pt\"/></style:default-style>\
         <style:style style:name=\"Standard\" style:family=\"paragraph\" style:class=\"text\"/>\
         <style:style style:name=\"List_20_Paragraph\" style:display-name=\"List Paragraph\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:class=\"list\">\
         <style:paragraph-properties fo:margin-bottom=\"0.04in\"/></style:style>\
         <style:style style:name=\"Heading\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:next-style-name=\"Standard\" style:class=\"text\">\
         <style:paragraph-properties fo:margin-top=\"0.17in\" fo:margin-bottom=\"0.08in\" fo:keep-with-next=\"always\"/>\
         <style:text-properties style:font-name=\"Liberation Sans\"/></style:style>{headings}\
         </office:styles><office:automatic-styles><style:page-layout style:name=\"pm1\">\
         <style:page-layout-properties fo:page-width=\"8.5in\" fo:page-height=\"11in\" fo:margin-top=\"0.79in\" fo:margin-bottom=\"0.79in\" fo:margin-left=\"0.79in\" fo:margin-right=\"0.79in\"/>\
         </style:page-layout></office:automatic-styles><office:master-styles>\
         <style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\"/></office:master-styles></office:document-styles>"
    )
}

pub fn export_odt(text: &str) -> Vec<u8> {
    let manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\">\
        <manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.3\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/>\
        <manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>\
        </manifest:manifest>";
    let meta = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta {NS}><office:meta>\
         <meta:generator>text_killer/{}</meta:generator></office:meta></office:document-meta>",
        env!("CARGO_PKG_VERSION")
    );
    let mut z = ZipWriter::default();
    // The mimetype entry must come first and be stored uncompressed.
    z.add("mimetype", b"application/vnd.oasis.opendocument.text");
    z.add("META-INF/manifest.xml", manifest.as_bytes());
    z.add("content.xml", content_xml(text).as_bytes());
    z.add("styles.xml", styles_xml().as_bytes());
    z.add("meta.xml", meta.as_bytes());
    z.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odt_roundtrip() {
        let src = "# Title\n\nIntro with **bold** and *italic*  spaced.\n- one\n  - two\n- three\n\tTabbed\n";
        let bytes = export_odt(src);
        let back = import_package(&bytes).unwrap();
        assert_eq!(back, src);
    }

    #[test]
    fn page_breaks_roundtrip() {
        let src = "Intro\n\u{c}\n# Chapter\ntext\n\u{c}\n- item\n";
        assert_eq!(import_package(&export_odt(src)).unwrap(), src);
    }

    #[test]
    fn spreadsheet_and_slides() {
        let ods = r#"<office:document-content><office:body><office:spreadsheet>
            <table:table table:name="Budget"><table:table-row><table:table-cell><text:p>Item</text:p></table:table-cell>
            <table:table-cell table:number-columns-repeated="2"><text:p>x</text:p></table:table-cell>
            <table:table-cell table:number-columns-repeated="1000"/></table:table-row>
            <table:table-row table:number-rows-repeated="1048000"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>
            </table:table></office:spreadsheet></office:body></office:document-content>"#;
        assert_eq!(to_text(ods, None), "## Sheet: Budget\nItem\tx\tx\n");
        let odp = r#"<office:document-content><office:body><office:presentation>
            <draw:page draw:name="page1"><draw:frame><draw:text-box><text:p>Hello</text:p></draw:text-box></draw:frame>
            <presentation:notes><draw:frame><draw:text-box><text:p>secret notes</text:p></draw:text-box></draw:frame></presentation:notes></draw:page>
            <draw:page draw:name="Intro"><draw:frame><draw:text-box><text:p>World</text:p></draw:text-box></draw:frame></draw:page>
            </office:presentation></office:body></office:document-content>"#;
        assert_eq!(to_text(odp, None), "## Slide 1\nHello\n\n## Slide 2 — Intro\nWorld\n");
    }

    #[test]
    fn styled_spans_from_automatic_styles() {
        let x = r#"<office:document-content><office:automatic-styles>
            <style:style style:name="T9" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
            </office:automatic-styles><office:body><office:text>
            <text:p>a <text:span text:style-name="T9">b</text:span><text:s text:c="2"/>c<text:note><text:note-body><text:p>fn</text:p></text:note-body></text:note></text:p>
            </office:text></office:body></office:document-content>"#;
        assert_eq!(to_text(x, None), "a **b**  c\n");
    }
}
