//! EPUB e-books (read-only): chapters are converted from XHTML to text in
//! reading (spine) order.

use std::collections::HashMap;

use super::markup::{BlockKind, TextBuilder};
use super::odf::collapse_ws;
use super::xml::{self, local, Event};
use super::zip::ZipArchive;

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Converts an (X)HTML document's body to text, appending to `tb`.
pub fn html_to_text(html: &str, tb: &mut TextBuilder) {
    let mut skip: Vec<String> = Vec::new();
    let mut in_body = !html.contains("<body");
    let (mut bold, mut italic) = (0u32, 0u32);
    let mut list_depth = 0u8;
    let mut pre = 0u32;
    let mut cells: Option<Vec<String>> = None;
    let mut cell_tb: Option<TextBuilder> = None;
    for ev in xml::parse(html) {
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
            Event::Start { name, .. } => {
                let tag = local(&name).to_ascii_lowercase();
                match tag.as_str() {
                    "body" => in_body = true,
                    "head" | "script" | "style" | "template" | "svg" | "math" => skip.push(name),
                    _ if !in_body => {}
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        tb.start(BlockKind::Heading(tag.as_bytes()[1] - b'0'));
                    }
                    "p" | "div" | "section" | "article" | "blockquote" | "figcaption" | "dt" | "dd" | "header"
                    | "footer" | "aside" | "address" | "caption" => {
                        if cell_tb.is_none() {
                            tb.start(BlockKind::Para);
                        }
                    }
                    "pre" => {
                        pre += 1;
                        tb.start(BlockKind::Para);
                    }
                    "ul" | "ol" => list_depth = list_depth.saturating_add(1),
                    "li" => tb.start(BlockKind::Bullet(list_depth.clamp(1, 6))),
                    "br" => {
                        let t = cell_tb.as_mut().unwrap_or(tb);
                        t.push("\n", bold > 0, italic > 0);
                    }
                    "hr" => tb.raw_line("---".into()),
                    "b" | "strong" => bold += 1,
                    "i" | "em" | "cite" | "var" | "dfn" => italic += 1,
                    "tr" => cells = Some(Vec::new()),
                    "td" | "th" => {
                        let mut t = TextBuilder::default();
                        t.start(BlockKind::Para);
                        cell_tb = Some(t);
                    }
                    _ => {}
                }
            }
            Event::End { name } => {
                let tag = local(&name).to_ascii_lowercase();
                match tag.as_str() {
                    "body" => in_body = false,
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div" | "section" | "article" | "blockquote"
                    | "figcaption" | "dt" | "dd" | "li" | "header" | "footer" | "aside" | "address" | "caption" => {
                        if cell_tb.is_none() {
                            tb.end();
                        }
                    }
                    "pre" => {
                        pre = pre.saturating_sub(1);
                        tb.end();
                    }
                    "ul" | "ol" => list_depth = list_depth.saturating_sub(1),
                    "b" | "strong" => bold = bold.saturating_sub(1),
                    "i" | "em" | "cite" | "var" | "dfn" => italic = italic.saturating_sub(1),
                    "td" | "th" => {
                        if let (Some(mut t), Some(c)) = (cell_tb.take(), cells.as_mut()) {
                            c.push(t.take().trim().replace('\n', " "));
                        }
                    }
                    "tr" => {
                        if let Some(c) = cells.take() {
                            if c.iter().any(|x| !x.is_empty()) {
                                tb.raw_line(c.join(" | "));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if !in_body {
                    continue;
                }
                let text = if pre > 0 { t } else { collapse_ws(&t) };
                let target = cell_tb.as_mut().unwrap_or(tb);
                // Skip whitespace between block elements.
                if text.trim().is_empty() && !target.is_open() {
                    continue;
                }
                if !target.is_open() {
                    target.start(BlockKind::Para);
                }
                target.push(&text, bold > 0, italic > 0);
            }
        }
    }
    tb.end();
}

pub fn import_epub(data: &[u8]) -> Result<String, String> {
    let z = ZipArchive::parse(data)?;
    let mut chapters: Vec<String> = Vec::new();
    let mut title = None;
    if let Ok(container) = z.read_text("META-INF/container.xml") {
        let opf_path = xml::parse(&container).into_iter().find_map(|e| match e {
            Event::Start { name, attrs } if local(&name) == "rootfile" => Event::attr(&attrs, "full-path").map(str::to_owned),
            _ => None,
        });
        if let Some(opf_path) = opf_path {
            let base = opf_path.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
            if let Ok(opf) = z.read_text(&opf_path) {
                let mut manifest: HashMap<String, String> = HashMap::new();
                let mut spine = Vec::new();
                let mut in_title = false;
                for ev in xml::parse(&opf) {
                    match ev {
                        Event::Start { name, attrs } => match local(&name) {
                            "item" => {
                                if let (Some(id), Some(href)) = (Event::attr(&attrs, "id"), Event::attr(&attrs, "href")) {
                                    manifest.insert(id.to_owned(), href.to_owned());
                                }
                            }
                            "itemref" => {
                                if let Some(id) = Event::attr(&attrs, "idref") {
                                    spine.push(id.to_owned());
                                }
                            }
                            "title" if title.is_none() => in_title = true,
                            _ => {}
                        },
                        Event::Text(t) if in_title => {
                            title = Some(t.trim().to_owned());
                            in_title = false;
                        }
                        Event::End { .. } => in_title = false,
                        _ => {}
                    }
                }
                for id in spine {
                    if let Some(href) = manifest.get(&id) {
                        let href = percent_decode(href.split('#').next().unwrap_or(href));
                        chapters.push(if base.is_empty() { href } else { format!("{base}/{href}") });
                    }
                }
            }
        }
    }
    if chapters.is_empty() {
        // Fallback: every XHTML/HTML file in archive order.
        chapters = z
            .entries
            .iter()
            .map(|e| e.name.clone())
            .filter(|n| n.ends_with(".xhtml") || n.ends_with(".html") || n.ends_with(".htm"))
            .collect();
    }
    if chapters.is_empty() {
        return Err("no readable chapters found in e-book".into());
    }
    let mut tb = TextBuilder::default();
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        tb.raw_line(format!("# {t}"));
    }
    for ch in chapters {
        if let Ok(html) = z.read_text(&ch) {
            tb.blank();
            html_to_text(&html, &mut tb);
        }
    }
    let text = tb.finish();
    // Many books repeat the metadata title on a title page; keep one copy.
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(first) = lines.first().copied().filter(|l| l.starts_with("# ")) {
        if let Some(dup) = lines.iter().skip(1).position(|l| !l.trim().is_empty()) {
            if lines[dup + 1] == first {
                lines.drain(..=dup);
            }
        }
    }
    // Collapse runs of blank lines left between chapters and sections.
    lines.dedup_by(|a, b| a.trim().is_empty() && b.trim().is_empty());
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xhtml_conversion() {
        let mut tb = TextBuilder::default();
        html_to_text(
            "<html><head><title>x</title><style>p{}</style></head><body>\
             <h2>Chapter  One</h2><p>It was a <em>dark</em> and\n stormy <b>night</b>.</p>\
             <ul><li>a</li><li>b<ul><li>c</li></ul></li></ul><table><tr><td>1</td><td>2</td></tr></table></body></html>",
            &mut tb,
        );
        assert_eq!(tb.finish(), "## Chapter One\nIt was a *dark* and stormy **night**.\n- a\n- b\n  - c\n1 | 2\n");
        assert_eq!(percent_decode("ch%201.xhtml"), "ch 1.xhtml");
    }
}
