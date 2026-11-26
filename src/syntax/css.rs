//! CSS / SCSS / LESS tokenizer.

use super::{fill, find_at, scan_string, starts_with_at, Ctx, Kind, Language};

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// True when the rest of the segment looks like a selector (`{` before `;`).
fn selector_ahead(chars: &[char], i: usize, end: usize) -> bool {
    for &c in &chars[i..end] {
        match c {
            '{' => return true,
            ';' | '}' => return false,
            _ => {}
        }
    }
    false
}

pub fn tokenize(lang: &Language, chars: &[char], start: usize, end: usize, ctx: &mut Ctx, depth: &mut u16, out: &mut [Kind]) {
    let mut i = start;
    let mut in_value = false;
    while i < end {
        if *ctx == Ctx::BlockComment {
            match find_at(chars, i, end, "*/", false) {
                Some(j) => {
                    fill(out, i, j + 2, Kind::Comment);
                    i = j + 2;
                    *ctx = Ctx::Normal;
                }
                None => {
                    fill(out, i, end, Kind::Comment);
                    return;
                }
            }
            continue;
        }
        let c = chars[i];
        if starts_with_at(chars, i, "/*") {
            fill(out, i, i + 2, Kind::Comment);
            i += 2;
            *ctx = Ctx::BlockComment;
            continue;
        }
        if lang.line_comment.iter().any(|lc| starts_with_at(chars, i, lc))
            && !(i > 0 && chars[i - 1] == ':') // url(http://...)
        {
            fill(out, i, end, Kind::Comment);
            return;
        }
        let selector_ctx = *depth == 0 || (!in_value && selector_ahead(chars, i, end));
        match c {
            '"' | '\'' => {
                i = scan_string(chars, i, end, c, false, out);
            }
            '{' => {
                out[i] = Kind::Punct;
                *depth = depth.saturating_add(1);
                in_value = false;
                i += 1;
            }
            '}' => {
                out[i] = Kind::Punct;
                *depth = depth.saturating_sub(1);
                in_value = false;
                i += 1;
            }
            ';' => {
                out[i] = Kind::Punct;
                in_value = false;
                i += 1;
            }
            '@' => {
                let mut j = i + 1;
                while j < end && is_ident(chars[j]) {
                    j += 1;
                }
                fill(out, i, j, Kind::Keyword);
                i = j;
            }
            '$' if lang.dollar_vars => {
                let mut j = i + 1;
                while j < end && is_ident(chars[j]) {
                    j += 1;
                }
                fill(out, i, j, Kind::Constant);
                i = j;
            }
            '!' => {
                let mut j = i + 1;
                while j < end && chars[j].is_alphabetic() {
                    j += 1;
                }
                fill(out, i, j, Kind::Keyword);
                i = j;
            }
            ':' if !selector_ctx && !in_value => {
                out[i] = Kind::Punct;
                in_value = true;
                i += 1;
            }
            ':' => {
                // Pseudo-class / pseudo-element.
                let mut j = i + 1;
                if chars.get(j) == Some(&':') {
                    j += 1;
                }
                while j < end && is_ident(chars[j]) {
                    j += 1;
                }
                fill(out, i, j, Kind::Keyword);
                i = j;
            }
            '#' if in_value => {
                let mut j = i + 1;
                while j < end && chars[j].is_ascii_hexdigit() {
                    j += 1;
                }
                fill(out, i, j, Kind::Number);
                i = j;
            }
            '#' | '.' if selector_ctx && chars.get(i + 1).is_some_and(|&n| is_ident(n) && !n.is_ascii_digit()) => {
                let mut j = i + 1;
                while j < end && is_ident(chars[j]) {
                    j += 1;
                }
                fill(out, i, j, Kind::Selector);
                i = j;
            }
            '[' if selector_ctx => {
                let j = (i..end).find(|&j| chars[j] == ']').map_or(end, |j| j + 1);
                fill(out, i, j, Kind::Attribute);
                // Re-scan quoted values inside the brackets.
                let mut k = i;
                while k < j {
                    if chars[k] == '"' || chars[k] == '\'' {
                        k = scan_string(chars, k, j, chars[k], false, out);
                    } else {
                        k += 1;
                    }
                }
                i = j;
            }
            _ if c.is_ascii_digit()
                || ((c == '.' || c == '-' || c == '+')
                    && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit() || *n == '.')
                    && !selector_ctx) =>
            {
                let mut j = i + 1;
                while j < end && (chars[j].is_ascii_alphanumeric() || chars[j] == '.' || chars[j] == '%') {
                    j += 1;
                }
                fill(out, i, j, Kind::Number);
                i = j;
            }
            _ if is_ident(c) => {
                let mut j = i + 1;
                while j < end && is_ident(chars[j]) {
                    j += 1;
                }
                let next = chars.get(j).copied();
                let kind = if selector_ctx {
                    if c == '&' { Kind::Keyword } else { Kind::Tag }
                } else if in_value {
                    if next == Some('(') { Kind::Function } else { Kind::Constant }
                } else {
                    Kind::Property
                };
                fill(out, i, j, kind);
                // `url(...)` contents are a string.
                if kind == Kind::Function && chars[i..j].iter().collect::<String>().eq_ignore_ascii_case("url") {
                    if let Some(close) = (j..end).find(|&k| chars[k] == ')') {
                        if !matches!(chars.get(j + 1), Some('"' | '\'')) {
                            fill(out, j + 1, close, Kind::String);
                            i = close;
                            continue;
                        }
                    }
                }
                i = j;
            }
            '&' => {
                out[i] = Kind::Keyword;
                i += 1;
            }
            _ => {
                out[i] = if "(),".contains(c) {
                    Kind::Punct
                } else if ">+~*=|^".contains(c) {
                    Kind::Operator
                } else {
                    Kind::Normal
                };
                i += 1;
            }
        }
    }
}
