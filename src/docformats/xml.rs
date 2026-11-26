//! A small, forgiving XML tokenizer for extracting text from document
//! formats. It is not validating; malformed input degrades gracefully.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Start { name: String, attrs: Vec<(String, String)> },
    End { name: String },
    Text(String),
}

impl Event {
    pub fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
        attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// Local name without namespace prefix: `text:p` -> `p`.
pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Decodes XML (and common HTML) character entities.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let decoded = if let Some(num) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
            u32::from_str_radix(num, 16).ok().and_then(char::from_u32)
        } else if let Some(num) = ent.strip_prefix('#') {
            num.parse().ok().and_then(char::from_u32)
        } else {
            named_entity(ent)
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "bull" => '•',
        "middot" => '·',
        "deg" => '°',
        "euro" => '€',
        "pound" => '£',
        "yen" => '¥',
        "sect" => '§',
        "para" => '¶',
        "times" => '×',
        "divide" => '÷',
        "shy" => '\u{ad}',
        _ => return None,
    })
}

/// Escapes text for inclusion in XML content or attribute values.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            // Characters not allowed in XML 1.0 are dropped.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        let ks = i;
        while i < b.len() && b[i] != b'=' && !(b[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        let key = &s[ks..i];
        while i < b.len() && (b[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            if !key.is_empty() {
                attrs.push((key.to_owned(), String::new()));
            }
            i += 1;
            continue;
        }
        i += 1;
        while i < b.len() && (b[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        let val = if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
            let q = b[i];
            let vs = i + 1;
            i = vs;
            while i < b.len() && b[i] != q {
                i += 1;
            }
            let v = &s[vs..i.min(b.len())];
            i += 1;
            v
        } else {
            let vs = i;
            while i < b.len() && !(b[i] as char).is_ascii_whitespace() {
                i += 1;
            }
            &s[vs..i]
        };
        if !key.is_empty() {
            attrs.push((key.to_owned(), unescape(val)));
        }
    }
    attrs
}

/// Tokenizes `xml` into a flat list of events. Self-closing tags produce a
/// Start immediately followed by an End.
pub fn parse(xml: &str) -> Vec<Event> {
    let mut events = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(&mut events, rest);
            break;
        };
        if lt > 0 {
            push_text(&mut events, &rest[..lt]);
        }
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |e| &r[e + 3..]);
        } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").unwrap_or(r.len());
            if end > 0 {
                events.push(Event::Text(r[..end].to_owned()));
            }
            rest = r.get(end + 3..).unwrap_or("");
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
        } else {
            // Find the closing '>' outside of quoted attribute values.
            let b = rest.as_bytes();
            let mut i = 1;
            let mut quote = 0u8;
            while i < b.len() {
                match b[i] {
                    q @ (b'"' | b'\'') if quote == 0 => quote = q,
                    q if q == quote => quote = 0,
                    b'>' if quote == 0 => break,
                    _ => {}
                }
                i += 1;
            }
            let inner = &rest[1..i.min(rest.len())];
            rest = rest.get(i + 1..).unwrap_or("");
            if let Some(name) = inner.strip_prefix('/') {
                events.push(Event::End { name: name.trim().to_owned() });
                continue;
            }
            let self_closing = inner.ends_with('/');
            let inner = inner.trim_end_matches('/');
            let split = inner.find(|c: char| c.is_ascii_whitespace()).unwrap_or(inner.len());
            let name = inner[..split].to_owned();
            if name.is_empty() {
                continue;
            }
            let attrs = parse_attrs(&inner[split..]);
            events.push(Event::Start { name: name.clone(), attrs });
            if self_closing {
                events.push(Event::End { name });
            }
        }
    }
    events
}

fn push_text(events: &mut Vec<Event>, raw: &str) {
    if raw.is_empty() {
        return;
    }
    events.push(Event::Text(unescape(raw)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes() {
        let ev = parse("<?xml version=\"1.0\"?><a x='1 &amp; 2'>hi &lt;b&gt;<b/><!-- c --><![CDATA[<raw>]]></a>");
        assert_eq!(
            ev,
            vec![
                Event::Start { name: "a".into(), attrs: vec![("x".into(), "1 & 2".into())] },
                Event::Text("hi <b>".into()),
                Event::Start { name: "b".into(), attrs: vec![] },
                Event::End { name: "b".into() },
                Event::Text("<raw>".into()),
                Event::End { name: "a".into() },
            ]
        );
    }

    #[test]
    fn entities_and_escape() {
        assert_eq!(unescape("&#65;&#x42;&nbsp;&bogus; & x"), "AB\u{a0}&bogus; & x");
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
        assert_eq!(local("text:p"), "p");
    }

    #[test]
    fn quoted_gt_in_attribute() {
        let ev = parse("<a t=\"x>y\">z</a>");
        assert_eq!(ev[1], Event::Text("z".into()));
    }
}
