//! Markdown tokenizer (headings, lists, quotes, code, emphasis, links).

use super::{fill, starts_with_at, Ctx, Kind};

pub fn tokenize(chars: &[char], ctx: &mut Ctx, out: &mut [Kind]) {
    let end = chars.len();
    let ls = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(end);
    let is_fence = starts_with_at(chars, ls, "```") || starts_with_at(chars, ls, "~~~");

    match *ctx {
        Ctx::MultiString(_) => {
            fill(out, 0, end, Kind::Code);
            if is_fence {
                *ctx = Ctx::Normal;
            }
            return;
        }
        Ctx::BlockComment => {
            let close = (0..end).find(|&j| starts_with_at(chars, j, "-->"));
            match close {
                Some(j) => {
                    fill(out, 0, j + 3, Kind::Comment);
                    *ctx = Ctx::Normal;
                    inline(chars, j + 3, end, ctx, out);
                }
                None => fill(out, 0, end, Kind::Comment),
            }
            return;
        }
        Ctx::Normal => {}
    }

    if is_fence {
        fill(out, 0, end, Kind::Code);
        *ctx = Ctx::MultiString(0);
        return;
    }
    if ls == end {
        return;
    }
    // Indented code block (4+ spaces or a tab, not a list continuation).
    if (ls >= 4 || chars[0] == '\t') && !matches!(chars[ls], '-' | '*' | '+') && !chars[ls].is_ascii_digit() {
        fill(out, 0, end, Kind::Code);
        return;
    }
    let c = chars[ls];
    if c == '#' {
        let hashes = chars[ls..].iter().take_while(|&&h| h == '#').count();
        if hashes <= 6 && chars.get(ls + hashes).is_none_or(|c| c.is_whitespace()) {
            fill(out, 0, end, Kind::Heading);
            return;
        }
    }
    // Setext underline / horizontal rule.
    let rest: String = chars[ls..].iter().filter(|c| !c.is_whitespace()).collect();
    if rest.len() >= 3 && (rest.chars().all(|c| c == '-') || rest.chars().all(|c| c == '=') || rest.chars().all(|c| c == '*') || rest.chars().all(|c| c == '_')) {
        fill(out, 0, end, Kind::Punct);
        return;
    }
    if c == '>' {
        fill(out, 0, end, Kind::Comment);
        return;
    }
    if c == '|' {
        // Tables: highlight pipes.
        for (i, ch) in chars.iter().enumerate() {
            if *ch == '|' {
                out[i] = Kind::Punct;
            }
        }
    }
    let mut i = ls;
    // List markers.
    if matches!(c, '-' | '*' | '+') && chars.get(ls + 1) == Some(&' ') {
        out[ls] = Kind::Keyword;
        i = ls + 2;
        // Task list [ ] / [x]
        if starts_with_at(chars, i, "[ ]") || starts_with_at(chars, i, "[x]") || starts_with_at(chars, i, "[X]") {
            fill(out, i, i + 3, Kind::Keyword);
            i += 3;
        }
    } else if c.is_ascii_digit() {
        let mut j = ls;
        while j < end && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j < end && matches!(chars[j], '.' | ')') && chars.get(j + 1) == Some(&' ') {
            fill(out, ls, j + 1, Kind::Keyword);
            i = j + 1;
        }
    }
    inline(chars, i, end, ctx, out);
}

fn inline(chars: &[char], start: usize, end: usize, ctx: &mut Ctx, out: &mut [Kind]) {
    let mut i = start;
    while i < end {
        let c = chars[i];
        if starts_with_at(chars, i, "<!--") {
            match (i + 4..end).find(|&j| starts_with_at(chars, j, "-->")) {
                Some(j) => {
                    fill(out, i, j + 3, Kind::Comment);
                    i = j + 3;
                }
                None => {
                    fill(out, i, end, Kind::Comment);
                    *ctx = Ctx::BlockComment;
                    return;
                }
            }
            continue;
        }
        if c == '\\' && i + 1 < end {
            fill(out, i, i + 2, Kind::Escape);
            i += 2;
            continue;
        }
        if c == '`' {
            let ticks = chars[i..end].iter().take_while(|&&t| t == '`').count();
            let mut j = i + ticks;
            let mut found = None;
            while j < end {
                if chars[j] == '`' {
                    let run = chars[j..end].iter().take_while(|&&t| t == '`').count();
                    if run == ticks {
                        found = Some(j + run);
                        break;
                    }
                    j += run;
                } else {
                    j += 1;
                }
            }
            match found {
                Some(stop) => {
                    fill(out, i, stop, Kind::Code);
                    i = stop;
                }
                None => i += ticks,
            }
            continue;
        }
        if (c == '*' || c == '_') && i + 1 < end {
            let double = chars[i + 1] == c;
            let delim_len = if double { 2 } else { 1 };
            // `_` inside words (snake_case) is not emphasis.
            let word_inner = c == '_' && i > 0 && chars[i - 1].is_alphanumeric();
            let opens = chars.get(i + delim_len).is_some_and(|n| !n.is_whitespace());
            if opens && !word_inner {
                let mut j = i + delim_len + 1;
                let mut close = None;
                while j + delim_len <= end {
                    let at = (0..delim_len).all(|k| chars[j + k] == c);
                    if at && !chars[j - 1].is_whitespace() && (delim_len == 2 || chars.get(j + 1) != Some(&c)) {
                        close = Some(j);
                        break;
                    }
                    j += 1;
                }
                if let Some(cl) = close {
                    fill(out, i, cl + delim_len, if double { Kind::Strong } else { Kind::Emphasis });
                    i = cl + delim_len;
                    continue;
                }
            }
        }
        if c == '~' && starts_with_at(chars, i, "~~") {
            if let Some(cl) = (i + 2..end.saturating_sub(1)).find(|&j| starts_with_at(chars, j, "~~")) {
                fill(out, i, cl + 2, Kind::Comment);
                i = cl + 2;
                continue;
            }
        }
        if c == '[' || (c == '!' && chars.get(i + 1) == Some(&'[')) {
            let open = if c == '!' { i + 1 } else { i };
            if let Some(close) = (open + 1..end).find(|&j| chars[j] == ']') {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(paren) = (close + 2..end).find(|&j| chars[j] == ')') {
                        fill(out, i, close + 1, Kind::Link);
                        fill(out, close + 1, paren + 1, Kind::String);
                        i = paren + 1;
                        continue;
                    }
                } else if chars.get(close + 1) == Some(&'[') || chars.get(close + 1) == Some(&':') {
                    fill(out, i, close + 1, Kind::Link);
                    i = close + 1;
                    continue;
                }
            }
        }
        if c == '<' {
            if let Some(close) = (i + 1..end).find(|&j| chars[j] == '>') {
                let inner: String = chars[i + 1..close].iter().collect();
                if inner.starts_with("http") || inner.contains('@') {
                    fill(out, i, close + 1, Kind::Link);
                } else if inner.trim_start_matches('/').starts_with(char::is_alphabetic) {
                    fill(out, i, close + 1, Kind::Tag);
                }
                if out[i] != Kind::Normal {
                    i = close + 1;
                    continue;
                }
            }
        }
        if starts_with_at(chars, i, "http://") || starts_with_at(chars, i, "https://") {
            let mut j = i;
            while j < end && !chars[j].is_whitespace() && !matches!(chars[j], ')' | '>' | ']') {
                j += 1;
            }
            fill(out, i, j, Kind::Link);
            i = j;
            continue;
        }
        i += 1;
    }
}
