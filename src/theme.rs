//! Colour themes inspired by classic IRC clients.

use ratatui::style::{Color, Modifier, Style};

use crate::syntax::Kind;

pub struct Theme {
    /// Short lower-case id used by `/theme <id>`.
    pub id: &'static str,
    /// Display name shown in the Options menu.
    pub name: &'static str,
    /// One-line description shown next to the name in the menu.
    pub blurb: &'static str,
    pub bg: Color,
    pub fg: Color,
    pub bar: Style,
    pub bar_bracket: Style,
    pub bar_accent: Style,
    pub sidebar: Style,
    pub sidebar_active: Style,
    pub sidebar_focus: Style,
    pub sidebar_dirty: Style,
    pub sidebar_num: Style,
    pub border: Style,
    pub gutter: Style,
    pub gutter_current: Style,
    pub current_line_bg: Color,
    pub selection_bg: Color,
    pub search_bg: Color,
    pub bracket_match: Style,
    pub whitespace: Style,
    pub prompt: Style,
    pub info: Style,
    pub warn: Style,
    pub error: Style,
    pub popup: Style,
    pub popup_border: Style,
    pub popup_title: Style,
    pub popup_selected: Style,
    pub popup_dim: Style,
    syntax: [Style; 24],
}

impl Theme {
    pub fn kind(&self, k: Kind) -> Style {
        self.syntax[k as usize]
    }

    pub fn base(&self) -> Style {
        Style::new().fg(self.fg).bg(self.bg)
    }
}

const fn fg(c: Color) -> Style {
    Style::new().fg(c)
}

#[allow(clippy::too_many_arguments)]
const fn syntax_table(
    normal: Color,
    keyword: Color,
    ty: Color,
    builtin: Color,
    constant: Color,
    string: Color,
    number: Color,
    comment: Color,
    function: Color,
    tag: Color,
    attr: Color,
) -> [Style; 24] {
    [
        fg(normal),                                       // Normal
        fg(keyword).add_modifier(Modifier::BOLD),         // Keyword
        fg(ty),                                           // Type
        fg(builtin),                                      // Builtin
        fg(constant),                                     // Constant
        fg(string),                                       // String
        fg(constant).add_modifier(Modifier::BOLD),        // Escape
        fg(number),                                       // Number
        fg(comment).add_modifier(Modifier::ITALIC),       // Comment
        fg(function),                                     // Function
        fg(keyword),                                      // Operator
        fg(normal),                                       // Punct
        fg(tag).add_modifier(Modifier::BOLD),             // Tag
        fg(attr),                                         // Attribute
        fg(builtin),                                      // Property
        fg(function).add_modifier(Modifier::BOLD),        // Selector
        fg(constant),                                     // Preproc
        fg(tag).add_modifier(Modifier::BOLD),             // Heading
        fg(normal).add_modifier(Modifier::ITALIC),        // Emphasis
        fg(normal).add_modifier(Modifier::BOLD),          // Strong
        fg(string),                                       // Code
        fg(function).add_modifier(Modifier::UNDERLINED),  // Link
        fg(Color::Green),                                 // Added
        fg(Color::Red),                                   // Removed
    ]
}

/// Crimson: black and red, the default text_killer look.
pub static CRIMSON: Theme = Theme {
    id: "crimson",
    name: "Crimson",
    blurb: "red & black (default)",
    bg: Color::Black,
    fg: Color::Indexed(252),
    bar: Style::new().fg(Color::White).bg(Color::Indexed(88)),
    bar_bracket: Style::new().fg(Color::Indexed(210)).bg(Color::Indexed(88)).add_modifier(Modifier::BOLD),
    bar_accent: Style::new().fg(Color::White).bg(Color::Indexed(88)).add_modifier(Modifier::BOLD),
    sidebar: Style::new().fg(Color::Indexed(250)),
    sidebar_active: Style::new().fg(Color::White).bg(Color::Indexed(124)).add_modifier(Modifier::BOLD),
    sidebar_focus: Style::new().fg(Color::Black).bg(Color::Indexed(196)).add_modifier(Modifier::BOLD),
    sidebar_dirty: Style::new().fg(Color::Indexed(196)).add_modifier(Modifier::BOLD),
    sidebar_num: Style::new().fg(Color::Indexed(124)),
    border: Style::new().fg(Color::Indexed(124)),
    gutter: Style::new().fg(Color::Indexed(238)),
    gutter_current: Style::new().fg(Color::Indexed(196)).add_modifier(Modifier::BOLD),
    current_line_bg: Color::Indexed(233),
    selection_bg: Color::Indexed(52),
    search_bg: Color::Indexed(88),
    bracket_match: Style::new().fg(Color::Black).bg(Color::Indexed(196)),
    whitespace: Style::new().fg(Color::Indexed(236)),
    prompt: Style::new().fg(Color::Indexed(196)).add_modifier(Modifier::BOLD),
    info: Style::new().fg(Color::Indexed(203)),
    warn: Style::new().fg(Color::Indexed(209)),
    error: Style::new().fg(Color::Indexed(196)).add_modifier(Modifier::BOLD),
    popup: Style::new().fg(Color::Indexed(252)).bg(Color::Indexed(232)),
    popup_border: Style::new().fg(Color::Indexed(160)).bg(Color::Indexed(232)),
    popup_title: Style::new().fg(Color::Indexed(196)).bg(Color::Indexed(232)).add_modifier(Modifier::BOLD),
    popup_selected: Style::new().fg(Color::White).bg(Color::Indexed(124)).add_modifier(Modifier::BOLD),
    popup_dim: Style::new().fg(Color::Indexed(242)).bg(Color::Indexed(232)),
    syntax: syntax_table(
        Color::Indexed(252), // normal: light grey
        Color::Indexed(196), // keyword: bright red
        Color::Indexed(203), // type: salmon red
        Color::Indexed(167), // builtin: muted red
        Color::Indexed(209), // constant: orange-red
        Color::Indexed(217), // string: pale rose
        Color::Indexed(210), // number: light coral
        Color::Indexed(240), // comment: dark grey
        Color::White,        // function: white
        Color::Indexed(160), // tag: deep red
        Color::Indexed(174), // attribute: dusty pink
    ),
};

/// irssi-like: blue status bars on the terminal's own background.
pub static IRSSI: Theme = Theme {
    id: "irssi",
    name: "Irssi",
    blurb: "blue bars, terminal bg",
    bg: Color::Reset,
    fg: Color::Reset,
    bar: Style::new().fg(Color::White).bg(Color::Blue),
    bar_bracket: Style::new().fg(Color::Cyan).bg(Color::Blue),
    bar_accent: Style::new().fg(Color::Yellow).bg(Color::Blue).add_modifier(Modifier::BOLD),
    sidebar: Style::new().fg(Color::Gray),
    sidebar_active: Style::new().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD),
    sidebar_focus: Style::new().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
    sidebar_dirty: Style::new().fg(Color::LightRed).add_modifier(Modifier::BOLD),
    sidebar_num: Style::new().fg(Color::DarkGray),
    border: Style::new().fg(Color::Blue),
    gutter: Style::new().fg(Color::DarkGray),
    gutter_current: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    current_line_bg: Color::Indexed(235),
    selection_bg: Color::Indexed(24),
    search_bg: Color::Indexed(94),
    bracket_match: Style::new().fg(Color::Black).bg(Color::Cyan),
    whitespace: Style::new().fg(Color::Indexed(239)),
    prompt: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    info: Style::new().fg(Color::Cyan),
    warn: Style::new().fg(Color::Yellow),
    error: Style::new().fg(Color::LightRed).add_modifier(Modifier::BOLD),
    popup: Style::new().fg(Color::White).bg(Color::Indexed(234)),
    popup_border: Style::new().fg(Color::Cyan).bg(Color::Indexed(234)),
    popup_title: Style::new().fg(Color::Yellow).bg(Color::Indexed(234)).add_modifier(Modifier::BOLD),
    popup_selected: Style::new().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD),
    popup_dim: Style::new().fg(Color::DarkGray).bg(Color::Indexed(234)),
    syntax: syntax_table(
        Color::Reset,
        Color::LightMagenta,
        Color::LightCyan,
        Color::Cyan,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::DarkGray,
        Color::LightBlue,
        Color::LightBlue,
        Color::Yellow,
    ),
};

/// WeeChat-like: dark grey bars, green/cyan accents.
pub static WEECHAT: Theme = Theme {
    id: "weechat",
    name: "WeeChat",
    blurb: "grey & green",
    bg: Color::Reset,
    fg: Color::Reset,
    bar: Style::new().fg(Color::Gray).bg(Color::Indexed(236)),
    bar_bracket: Style::new().fg(Color::Cyan).bg(Color::Indexed(236)),
    bar_accent: Style::new().fg(Color::LightGreen).bg(Color::Indexed(236)).add_modifier(Modifier::BOLD),
    sidebar: Style::new().fg(Color::Gray),
    sidebar_active: Style::new().fg(Color::White).bg(Color::Indexed(22)).add_modifier(Modifier::BOLD),
    sidebar_focus: Style::new().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD),
    sidebar_dirty: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    sidebar_num: Style::new().fg(Color::Green),
    border: Style::new().fg(Color::Indexed(240)),
    gutter: Style::new().fg(Color::Indexed(242)),
    gutter_current: Style::new().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
    current_line_bg: Color::Indexed(234),
    selection_bg: Color::Indexed(238),
    search_bg: Color::Indexed(58),
    bracket_match: Style::new().fg(Color::Black).bg(Color::LightGreen),
    whitespace: Style::new().fg(Color::Indexed(238)),
    prompt: Style::new().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
    info: Style::new().fg(Color::LightGreen),
    warn: Style::new().fg(Color::Yellow),
    error: Style::new().fg(Color::LightRed).add_modifier(Modifier::BOLD),
    popup: Style::new().fg(Color::Gray).bg(Color::Indexed(235)),
    popup_border: Style::new().fg(Color::LightGreen).bg(Color::Indexed(235)),
    popup_title: Style::new().fg(Color::White).bg(Color::Indexed(235)).add_modifier(Modifier::BOLD),
    popup_selected: Style::new().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD),
    popup_dim: Style::new().fg(Color::Indexed(242)).bg(Color::Indexed(235)),
    syntax: syntax_table(
        Color::Reset,
        Color::LightGreen,
        Color::LightCyan,
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
        Color::LightMagenta,
        Color::Indexed(244),
        Color::LightBlue,
        Color::LightGreen,
        Color::Cyan,
    ),
};

/// Classic: light, high-contrast with the colours of classic IRC clients.
pub static MIRC: Theme = Theme {
    id: "classic",
    name: "Classic",
    blurb: "light, classic colours",
    bg: Color::White,
    fg: Color::Black,
    bar: Style::new().fg(Color::Black).bg(Color::Gray),
    bar_bracket: Style::new().fg(Color::Blue).bg(Color::Gray),
    bar_accent: Style::new().fg(Color::Red).bg(Color::Gray).add_modifier(Modifier::BOLD),
    sidebar: Style::new().fg(Color::Black).bg(Color::White),
    sidebar_active: Style::new().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD),
    sidebar_focus: Style::new().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD),
    sidebar_dirty: Style::new().fg(Color::Red).bg(Color::White).add_modifier(Modifier::BOLD),
    sidebar_num: Style::new().fg(Color::DarkGray).bg(Color::White),
    border: Style::new().fg(Color::DarkGray).bg(Color::White),
    gutter: Style::new().fg(Color::DarkGray).bg(Color::White),
    gutter_current: Style::new().fg(Color::Blue).bg(Color::White).add_modifier(Modifier::BOLD),
    current_line_bg: Color::Indexed(255),
    selection_bg: Color::Indexed(153),
    search_bg: Color::Indexed(228),
    bracket_match: Style::new().fg(Color::White).bg(Color::Magenta),
    whitespace: Style::new().fg(Color::Indexed(250)),
    prompt: Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD),
    info: Style::new().fg(Color::Blue),
    warn: Style::new().fg(Color::Indexed(130)),
    error: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
    popup: Style::new().fg(Color::Black).bg(Color::Indexed(254)),
    popup_border: Style::new().fg(Color::Blue).bg(Color::Indexed(254)),
    popup_title: Style::new().fg(Color::Red).bg(Color::Indexed(254)).add_modifier(Modifier::BOLD),
    popup_selected: Style::new().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD),
    popup_dim: Style::new().fg(Color::DarkGray).bg(Color::Indexed(254)),
    syntax: syntax_table(
        Color::Black,
        Color::Blue,
        Color::Indexed(30),
        Color::Indexed(90),
        Color::Red,
        Color::Indexed(28),
        Color::Indexed(130),
        Color::Indexed(244),
        Color::Indexed(25),
        Color::Indexed(18),
        Color::Indexed(127),
    ),
};

/// Colours for a theme built with [`build`].
struct Palette {
    id: &'static str,
    name: &'static str,
    blurb: &'static str,
    bg: Color,
    fg: Color,
    /// Status/topic bar background and text.
    bar_bg: Color,
    bar_fg: Color,
    /// Main accent (prompts, active items, borders' highlights).
    accent: Color,
    /// Secondary accent (brackets, borders).
    accent2: Color,
    /// Dim text (gutter, hints).
    dim: Color,
    current_line: Color,
    selection: Color,
    search: Color,
    popup_bg: Color,
    /// Text drawn on top of the accent colour.
    on_accent: Color,
    /// normal, keyword, type, builtin, constant, string, number, comment, function, tag, attribute
    syntax: [Color; 11],
}

const fn build(p: Palette) -> Theme {
    let s = p.syntax;
    Theme {
        id: p.id,
        name: p.name,
        blurb: p.blurb,
        bg: p.bg,
        fg: p.fg,
        bar: Style::new().fg(p.bar_fg).bg(p.bar_bg),
        bar_bracket: Style::new().fg(p.accent2).bg(p.bar_bg).add_modifier(Modifier::BOLD),
        bar_accent: Style::new().fg(p.accent).bg(p.bar_bg).add_modifier(Modifier::BOLD),
        sidebar: Style::new().fg(p.fg),
        sidebar_active: Style::new().fg(p.on_accent).bg(p.accent2).add_modifier(Modifier::BOLD),
        sidebar_focus: Style::new().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD),
        sidebar_dirty: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
        sidebar_num: Style::new().fg(p.dim),
        border: Style::new().fg(p.accent2),
        gutter: Style::new().fg(p.dim),
        gutter_current: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
        current_line_bg: p.current_line,
        selection_bg: p.selection,
        search_bg: p.search,
        bracket_match: Style::new().fg(p.on_accent).bg(p.accent),
        whitespace: Style::new().fg(p.current_line),
        prompt: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
        info: Style::new().fg(p.accent),
        warn: Style::new().fg(Color::Indexed(214)),
        error: Style::new().fg(Color::Indexed(203)).add_modifier(Modifier::BOLD),
        popup: Style::new().fg(p.fg).bg(p.popup_bg),
        popup_border: Style::new().fg(p.accent2).bg(p.popup_bg),
        popup_title: Style::new().fg(p.accent).bg(p.popup_bg).add_modifier(Modifier::BOLD),
        popup_selected: Style::new().fg(p.on_accent).bg(p.accent2).add_modifier(Modifier::BOLD),
        popup_dim: Style::new().fg(p.dim).bg(p.popup_bg),
        syntax: syntax_table(s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10]),
    }
}

const fn ix(n: u8) -> Color {
    Color::Indexed(n)
}

pub static MIDNIGHT: Theme = build(Palette {
    id: "midnight",
    name: "Midnight",
    blurb: "violet on deep navy",
    bg: ix(17),
    fg: ix(253),
    bar_bg: ix(54),
    bar_fg: ix(255),
    accent: ix(213),
    accent2: ix(98),
    dim: ix(61),
    current_line: ix(18),
    selection: ix(55),
    search: ix(90),
    popup_bg: ix(234),
    on_accent: Color::Black,
    syntax: [ix(253), ix(213), ix(147), ix(141), ix(219), ix(158), ix(222), ix(61), ix(117), ix(177), ix(183)],
});

pub static MATRIX: Theme = build(Palette {
    id: "matrix",
    name: "Matrix",
    blurb: "green phosphor",
    bg: Color::Black,
    fg: ix(46),
    bar_bg: ix(22),
    bar_fg: ix(120),
    accent: ix(82),
    accent2: ix(34),
    dim: ix(28),
    current_line: ix(233),
    selection: ix(22),
    search: ix(58),
    popup_bg: ix(232),
    on_accent: Color::Black,
    syntax: [ix(46), ix(118), ix(83), ix(77), ix(156), ix(193), ix(190), ix(28), ix(48), ix(119), ix(114)],
});

pub static AMBER: Theme = build(Palette {
    id: "amber",
    name: "Amber",
    blurb: "retro amber terminal",
    bg: ix(232),
    fg: ix(214),
    bar_bg: ix(94),
    bar_fg: ix(229),
    accent: ix(220),
    accent2: ix(172),
    dim: ix(130),
    current_line: ix(234),
    selection: ix(94),
    search: ix(130),
    popup_bg: ix(233),
    on_accent: Color::Black,
    syntax: [ix(214), ix(220), ix(215), ix(208), ix(228), ix(223), ix(229), ix(130), ix(222), ix(221), ix(180)],
});

pub static OCEAN: Theme = build(Palette {
    id: "ocean",
    name: "Ocean",
    blurb: "teal & sea blue",
    bg: ix(234),
    fg: ix(152),
    bar_bg: ix(24),
    bar_fg: ix(195),
    accent: ix(44),
    accent2: ix(31),
    dim: ix(66),
    current_line: ix(235),
    selection: ix(23),
    search: ix(30),
    popup_bg: ix(235),
    on_accent: Color::Black,
    syntax: [ix(152), ix(44), ix(80), ix(73), ix(216), ix(114), ix(209), ix(243), ix(117), ix(38), ix(110)],
});

pub static FROST: Theme = build(Palette {
    id: "frost",
    name: "Frost",
    blurb: "cool arctic greys",
    bg: Color::Rgb(46, 52, 64),
    fg: Color::Rgb(216, 222, 233),
    bar_bg: Color::Rgb(59, 66, 82),
    bar_fg: Color::Rgb(229, 233, 240),
    accent: Color::Rgb(136, 192, 208),
    accent2: Color::Rgb(94, 129, 172),
    dim: Color::Rgb(97, 110, 136),
    current_line: Color::Rgb(59, 66, 82),
    selection: Color::Rgb(67, 76, 94),
    search: Color::Rgb(94, 81, 64),
    popup_bg: Color::Rgb(59, 66, 82),
    on_accent: Color::Rgb(46, 52, 64),
    syntax: [
        Color::Rgb(216, 222, 233),
        Color::Rgb(129, 161, 193),
        Color::Rgb(143, 188, 187),
        Color::Rgb(136, 192, 208),
        Color::Rgb(180, 142, 173),
        Color::Rgb(163, 190, 140),
        Color::Rgb(180, 142, 173),
        Color::Rgb(97, 110, 136),
        Color::Rgb(136, 192, 208),
        Color::Rgb(129, 161, 193),
        Color::Rgb(143, 188, 187),
    ],
});

pub static SOLAR: Theme = build(Palette {
    id: "solar",
    name: "Solar",
    blurb: "warm solarized dark",
    bg: Color::Rgb(0, 43, 54),
    fg: Color::Rgb(147, 161, 161),
    bar_bg: Color::Rgb(7, 54, 66),
    bar_fg: Color::Rgb(238, 232, 213),
    accent: Color::Rgb(181, 137, 0),
    accent2: Color::Rgb(38, 139, 210),
    dim: Color::Rgb(88, 110, 117),
    current_line: Color::Rgb(7, 54, 66),
    selection: Color::Rgb(20, 70, 85),
    search: Color::Rgb(90, 75, 10),
    popup_bg: Color::Rgb(7, 54, 66),
    on_accent: Color::Rgb(0, 43, 54),
    syntax: [
        Color::Rgb(147, 161, 161),
        Color::Rgb(133, 153, 0),
        Color::Rgb(181, 137, 0),
        Color::Rgb(38, 139, 210),
        Color::Rgb(203, 75, 22),
        Color::Rgb(42, 161, 152),
        Color::Rgb(211, 54, 130),
        Color::Rgb(88, 110, 117),
        Color::Rgb(38, 139, 210),
        Color::Rgb(38, 139, 210),
        Color::Rgb(181, 137, 0),
    ],
});

pub static PAPER: Theme = build(Palette {
    id: "paper",
    name: "Paper",
    blurb: "light, black ink",
    bg: ix(255),
    fg: ix(235),
    bar_bg: ix(250),
    bar_fg: ix(232),
    accent: ix(160),
    accent2: ix(240),
    dim: ix(246),
    current_line: ix(254),
    selection: ix(252),
    search: ix(229),
    popup_bg: ix(254),
    on_accent: Color::White,
    syntax: [ix(235), ix(232), ix(238), ix(240), ix(160), ix(242), ix(160), ix(246), ix(232), ix(232), ix(240)],
});

/// All themes in menu order. The first is the default.
pub static THEMES: &[&Theme] = &[&CRIMSON, &MIDNIGHT, &MATRIX, &AMBER, &OCEAN, &FROST, &SOLAR, &IRSSI, &WEECHAT, &MIRC, &PAPER];

/// Finds a theme by id or display name (case-insensitive).
pub fn find(name: &str) -> Option<usize> {
    let n = name.trim();
    // "Classic" was called "mIRC" up to release 1.3.1; keep old settings working.
    let n = if n.eq_ignore_ascii_case("mirc") { "classic" } else { n };
    THEMES.iter().position(|t| t.id.eq_ignore_ascii_case(n) || t.name.eq_ignore_ascii_case(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_crimson_and_ids_unique() {
        assert_eq!(THEMES[0].name, "Crimson");
        assert_eq!(find("CRIMSON"), Some(0));
        assert_eq!(find("Frost"), Some(5));
        assert_eq!(find("mirc"), find("classic"));
        assert!(find("classic").is_some());
        let mut ids: Vec<_> = THEMES.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), THEMES.len());
    }
}
