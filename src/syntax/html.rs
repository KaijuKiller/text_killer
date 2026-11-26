//! HTML / XML tokenizer with embedded `<style>` (CSS) and `<script>` (JS).

use super::languages::{CSS, JAVASCRIPT};
use super::{css, fill, find_at, generic, starts_with_at, Ctx, HtmlMode, Kind, State};

pub fn tokenize(chars: &[char], st: &mut State, out: &mut [Kind]) {
    let end = chars.len();
    let mut i = 0;
    while i < end {
        match st.html {
            HtmlMode::Comment => match find_at(chars, i, end, "-->", false) {
                Some(j) => {
                    fill(out, i, j + 3, Kind::Comment);
                    i = j + 3;
                    st.html = HtmlMode::Text;
                }
                None => {
                    fill(out, i, end, Kind::Comment);
                    return;
                }
            },
            HtmlMode::Script | HtmlMode::Style => {
                let close = if st.html == HtmlMode::Script { "</script" } else { "</style" };
                let stop = find_at(chars, i, end, close, true);
                let seg_end = stop.unwrap_or(end);
                if st.html == HtmlMode::Script {
                    generic::tokenize(&JAVASCRIPT, chars, i, seg_end, &mut st.ctx, out);
                } else {
                    css::tokenize(&CSS, chars, i, seg_end, &mut st.ctx, &mut st.depth, out);
                }
                match stop {
                    Some(j) => {
                        st.html = HtmlMode::Text;
                        st.ctx = Ctx::Normal;
                        st.depth = 0;
                        i = j;
                    }
                    None => return,
                }
            }
            HtmlMode::Tag => {
                let c = chars[i];
                if c.is_whitespace() {
                    i += 1;
                } else if c == '>' {
                    out[i] = Kind::Punct;
                    i += 1;
                    st.html = if st.pending == HtmlMode::Text { HtmlMode::Text } else { st.pending };
                    st.pending = HtmlMode::Text;
                } else if starts_with_at(chars, i, "/>") || starts_with_at(chars, i, "?>") {
                    fill(out, i, i + 2, Kind::Punct);
                    i += 2;
                    st.html = HtmlMode::Text;
                    st.pending = HtmlMode::Text;
                } else if c == '"' || c == '\'' {
                    let mut j = i + 1;
                    while j < end && chars[j] != c {
                        j += 1;
                    }
                    let stop = (j + 1).min(end);
                    fill(out, i, stop, Kind::String);
                    i = stop;
                } else if c == '=' {
                    out[i] = Kind::Operator;
                    i += 1;
                } else {
                    let mut j = i;
                    while j < end && !chars[j].is_whitespace() && !matches!(chars[j], '=' | '>' | '"' | '\'')
                        && !starts_with_at(chars, j, "/>")
                    {
                        j += 1;
                    }
                    if j == i {
                        out[i] = Kind::Punct;
                        j = i + 1;
                    } else {
                        fill(out, i, j, Kind::Attribute);
                    }
                    i = j;
                }
            }
            HtmlMode::Text => {
                if starts_with_at(chars, i, "<!--") {
                    fill(out, i, i + 4, Kind::Comment);
                    i += 4;
                    st.html = HtmlMode::Comment;
                    continue;
                }
                let c = chars[i];
                let next = chars.get(i + 1).copied();
                if c == '<' && next.is_some_and(|n| n.is_alphabetic() || matches!(n, '/' | '!' | '?')) {
                    out[i] = Kind::Punct;
                    let mut j = i + 1;
                    let closing = chars[j] == '/';
                    let special = matches!(chars[j], '!' | '?');
                    if closing {
                        out[j] = Kind::Punct;
                        j += 1;
                    }
                    let name_start = j;
                    if special {
                        j += 1;
                    }
                    while j < end && (chars[j].is_alphanumeric() || matches!(chars[j], '-' | ':' | '_' | '.')) {
                        j += 1;
                    }
                    fill(out, name_start, j, if special { Kind::Preproc } else { Kind::Tag });
                    let name: String = chars[name_start..j].iter().collect::<String>().to_ascii_lowercase();
                    st.pending = match (closing, name.as_str()) {
                        (false, "script") => HtmlMode::Script,
                        (false, "style") => HtmlMode::Style,
                        _ => HtmlMode::Text,
                    };
                    st.html = HtmlMode::Tag;
                    i = j;
                } else if c == '&' {
                    let mut j = i + 1;
                    while j < end && j - i < 12 && (chars[j].is_alphanumeric() || chars[j] == '#') {
                        j += 1;
                    }
                    if j < end && chars[j] == ';' && j > i + 1 {
                        fill(out, i, j + 1, Kind::Constant);
                        i = j + 1;
                    } else {
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
}
