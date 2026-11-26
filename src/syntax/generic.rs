//! Table-driven tokenizer used by most programming / config languages.

use super::{fill, scan_string, starts_with_at, Ctx, Kind, Language};

fn is_ident_start(lang: &Language, c: char) -> bool {
    c.is_alphabetic() || c == '_' || (lang.ident_extra.contains(c) && c != '-' && c != '\'' && c != '.')
}

fn is_ident_char(lang: &Language, c: char) -> bool {
    c.is_alphanumeric() || c == '_' || lang.ident_extra.contains(c)
}

fn lookup(list: &[&str], word: &str, ci: bool) -> bool {
    if ci {
        list.iter().any(|k| k.eq_ignore_ascii_case(word))
    } else {
        list.contains(&word)
    }
}

fn next_non_space(chars: &[char], mut i: usize, end: usize) -> Option<char> {
    while i < end {
        if !chars[i].is_whitespace() {
            return Some(chars[i]);
        }
        i += 1;
    }
    None
}

/// Tokenizes `chars[start..end]` according to `lang`, updating `ctx`.
pub fn tokenize(lang: &Language, chars: &[char], start: usize, end: usize, ctx: &mut Ctx, out: &mut [Kind]) {
    let mut i = start;
    let line_start = chars[start..end].iter().position(|c| !c.is_whitespace()).map(|p| p + start);
    let mut prev_word = String::new();

    // Whole-line constructs for config formats.
    if let Some(ls) = line_start {
        if *ctx == Ctx::Normal && lang.sections && chars[ls] == '[' {
            if let Some(close) = (ls..end).rev().find(|&j| chars[j] == ']') {
                fill(out, ls, close + 1, Kind::Tag);
                i = close + 1;
            }
        }
    }

    while i < end {
        // Continue multi-line constructs.
        match *ctx {
            Ctx::BlockComment => {
                let (_, close) = lang.block_comment.expect("block comment ctx without delimiters");
                match (i..end).find(|&j| starts_with_at(chars, j, close)) {
                    Some(j) => {
                        let stop = j + close.chars().count();
                        fill(out, i, stop, Kind::Comment);
                        i = stop;
                        *ctx = Ctx::Normal;
                    }
                    None => {
                        fill(out, i, end, Kind::Comment);
                        return;
                    }
                }
                continue;
            }
            Ctx::MultiString(idx) => {
                let delim = lang.multiline_strings.get(idx as usize).copied().unwrap_or("\"");
                let close = if delim == "[[" { "]]" } else { delim };
                let raw = delim == "[[" || delim == "'''" && lang.name == "TOML";
                let mut j = i;
                loop {
                    if j >= end {
                        fill(out, i, end, Kind::String);
                        return;
                    }
                    if chars[j] == '\\' && !raw {
                        fill(out, j, (j + 2).min(end), Kind::Escape);
                        j += 2;
                        continue;
                    }
                    if starts_with_at(chars, j, close) {
                        let stop = j + close.chars().count();
                        for k in &mut out[i..stop] {
                            if *k != Kind::Escape {
                                *k = Kind::String;
                            }
                        }
                        i = stop;
                        *ctx = Ctx::Normal;
                        break;
                    }
                    if out[j] != Kind::Escape {
                        out[j] = Kind::String;
                    }
                    j += 1;
                }
                continue;
            }
            Ctx::Normal => {}
        }

        let c = chars[i];
        let prev = if i > start { Some(chars[i - 1]) } else { None };

        // Block comment start (checked before line comments: Lua `--[[`).
        if let Some((open, _)) = lang.block_comment {
            if starts_with_at(chars, i, open) {
                let n = open.chars().count();
                fill(out, i, i + n, Kind::Comment);
                i += n;
                *ctx = Ctx::BlockComment;
                continue;
            }
        }

        // Line comment.
        let is_line_comment = lang.line_comment.iter().any(|lc| {
            let matched = if lang.case_insensitive {
                super::starts_with_at_ci(chars, i, lc)
            } else {
                starts_with_at(chars, i, lc)
            };
            if !matched {
                return false;
            }
            // In shells, `#` only starts a comment at a word boundary.
            if *lc == "#" && lang.dollar_vars && prev.is_some_and(|p| !p.is_whitespace()) {
                return false;
            }
            // Word-like markers (`rem`) only count at the start of a line.
            !lc.starts_with(char::is_alphabetic) || Some(i) == line_start
        });
        if is_line_comment {
            fill(out, i, end, Kind::Comment);
            return;
        }

        // Preprocessor directive.
        if lang.preproc_hash && c == '#' && Some(i) == line_start {
            let mut j = i + 1;
            while j < end && (chars[j].is_alphanumeric() || chars[j] == ' ' && j == i + 1) {
                j += 1;
            }
            fill(out, i, j, Kind::Preproc);
            i = j;
            // `#include <foo.h>`
            if let Some(k) = (i..end).find(|&k| !chars[k].is_whitespace()) {
                if chars[k] == '<' {
                    if let Some(close) = (k..end).find(|&m| chars[m] == '>') {
                        fill(out, k, close + 1, Kind::String);
                        i = close + 1;
                    }
                }
            }
            continue;
        }

        // Multi-line strings.
        if let Some(idx) = lang.multiline_strings.iter().position(|d| starts_with_at(chars, i, d)) {
            let n = lang.multiline_strings[idx].chars().count();
            fill(out, i, i + n, Kind::String);
            i += n;
            *ctx = Ctx::MultiString(idx as u8);
            continue;
        }

        // Single-line strings.
        if lang.quotes.contains(c) {
            if c == '\'' && lang.char_literals {
                // Char literal 'x' or '\n' / '\u{..}'; otherwise a lifetime/label.
                let is_char = (chars.get(i + 2) == Some(&'\'') && chars.get(i + 1) != Some(&'\\'))
                    || (chars.get(i + 1) == Some(&'\\') && (i + 2..end.min(i + 12)).any(|j| chars[j] == '\''));
                if !is_char {
                    let mut j = i + 1;
                    while j < end && is_ident_char(lang, chars[j]) {
                        j += 1;
                    }
                    fill(out, i, j, Kind::Type);
                    i = j.max(i + 1);
                    continue;
                }
            }
            let stop = scan_string(chars, i, end, c, false, out);
            if lang.string_keys && next_non_space(chars, stop, end) == Some(':') {
                fill(out, i, stop, Kind::Property);
            }
            i = stop;
            continue;
        }

        // Numbers.
        let prev_is_ident = prev.is_some_and(|p| is_ident_char(lang, p));
        if !prev_is_ident
            && (c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit) && i + 1 < end))
        {
            let mut j = i + 1;
            while j < end {
                let d = chars[j];
                let exp_sign = (d == '+' || d == '-')
                    && matches!(chars[j - 1], 'e' | 'E')
                    && !(chars[i] == '0' && matches!(chars.get(i + 1), Some('x' | 'X')));
                let decimal_point = d == '.' && chars.get(j + 1).is_some_and(char::is_ascii_digit);
                if d.is_ascii_alphanumeric() || d == '_' || exp_sign || decimal_point {
                    j += 1;
                } else {
                    break;
                }
            }
            fill(out, i, j, Kind::Number);
            i = j;
            continue;
        }

        // Variables: $foo, ${foo}, $1, @array-ish in shells.
        if lang.dollar_vars && c == '$' {
            let mut j = i + 1;
            if chars.get(j) == Some(&'{') {
                while j < end && chars[j] != '}' {
                    j += 1;
                }
                j = (j + 1).min(end);
            } else if chars.get(j) == Some(&'(') {
                j = i + 1; // command substitution: just the `$`
            } else {
                while j < end && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                if j == i + 1 && j < end && "@*#?$!0-".contains(chars[j]) {
                    j += 1;
                }
            }
            fill(out, i, j, Kind::Constant);
            i = j;
            continue;
        }

        // Decorators / annotations.
        if lang.decorators && c == '@' {
            let mut j = i + 1;
            while j < end && (is_ident_char(lang, chars[j]) || chars[j] == '.') {
                j += 1;
            }
            fill(out, i, j, Kind::Preproc);
            i = j;
            continue;
        }

        // TeX / roff commands: \section, \fB, \\.
        if lang.backslash_commands && c == '\\' {
            let mut j = i + 1;
            while j < end && chars[j].is_alphabetic() {
                j += 1;
            }
            if j == i + 1 && j < end {
                j += 1; // control symbol such as \% or \\
            }
            fill(out, i, j, Kind::Keyword);
            i = j;
            continue;
        }

        // Identifiers and keywords.
        if is_ident_start(lang, c) {
            let mut j = i + 1;
            while j < end && is_ident_char(lang, chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();

            // String prefixes: f"..", r'..', b"..".
            if j < end
                && lang.quotes.contains(chars[j]) | lang.multiline_strings.iter().any(|d| starts_with_at(chars, j, d))
                && lookup(lang.string_prefixes, &word.to_ascii_lowercase(), false)
                && !(chars[j] == '\'' && lang.char_literals)
            {
                fill(out, i, j, Kind::String);
                let raw = word.to_ascii_lowercase().contains('r');
                if let Some(idx) = lang.multiline_strings.iter().position(|d| starts_with_at(chars, j, d)) {
                    let n = lang.multiline_strings[idx].chars().count();
                    fill(out, j, j + n, Kind::String);
                    i = j + n;
                    *ctx = Ctx::MultiString(idx as u8);
                } else {
                    i = scan_string(chars, j, end, chars[j], raw, out);
                }
                continue;
            }

            let ci = lang.case_insensitive;
            let next = next_non_space(chars, j, end);
            let key_pos = Some(i) == line_start || line_start.is_some_and(|ls| chars[ls] == '-' && chars[ls + 1..i].iter().all(|c| c.is_whitespace()) && i > ls + 1);
            let kind = if lang.kv_keys && key_pos && matches!(next, Some('=' | ':')) {
                Kind::Property
            } else if lookup(lang.keywords, &word, ci) {
                Kind::Keyword
            } else if lookup(lang.constants, &word, ci) {
                Kind::Constant
            } else if lookup(lang.types, &word, ci) {
                Kind::Type
            } else if matches!(prev_word.as_str(), "def" | "fn" | "func" | "function" | "fun" | "sub" | "defun" | "defn") {
                Kind::Function
            } else if matches!(
                prev_word.as_str(),
                "class" | "struct" | "enum" | "trait" | "interface" | "type" | "impl" | "union" | "record" | "module"
            ) {
                Kind::Type
            } else if next == Some('(') || (next == Some('!') && lang.name == "Rust") {
                if lookup(lang.builtins, &word, ci) { Kind::Builtin } else { Kind::Function }
            } else if lookup(lang.builtins, &word, ci) {
                Kind::Builtin
            } else if lang.caps_types && word.len() > 1 && word.chars().all(|ch| ch.is_uppercase() || ch == '_' || ch.is_ascii_digit()) && word.chars().any(char::is_uppercase) {
                Kind::Constant
            } else if lang.caps_types && word.starts_with(char::is_uppercase) {
                Kind::Type
            } else {
                Kind::Normal
            };
            fill(out, i, j, kind);
            prev_word = word;
            i = j;
            continue;
        }

        // Operators and punctuation.
        out[i] = if "+-*/%=<>!&|^~?:".contains(c) {
            Kind::Operator
        } else if "()[]{},;.".contains(c) {
            Kind::Punct
        } else {
            Kind::Normal
        };
        if !c.is_whitespace() && !matches!(c, '.' | ':') {
            prev_word.clear();
        }
        i += 1;
    }
}
