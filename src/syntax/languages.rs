//! Language definitions and detection.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Plain,
    Generic,
    Html,
    Css,
    Markdown,
    Diff,
    Hex,
}

/// Static description of a language's lexical structure.
pub struct Language {
    pub name: &'static str,
    /// Lower-case aliases usable with `/lang`.
    pub aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub shebangs: &'static [&'static str],
    pub mode: Mode,
    pub line_comment: &'static [&'static str],
    pub block_comment: Option<(&'static str, &'static str)>,
    /// Single-line string delimiters.
    pub quotes: &'static str,
    /// Delimiters for strings that may span lines (e.g. `"""`, `` ` ``).
    pub multiline_strings: &'static [&'static str],
    /// Identifier prefixes allowed directly before a quote (`f"..."`, `r"..."`).
    pub string_prefixes: &'static [&'static str],
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub constants: &'static [&'static str],
    pub builtins: &'static [&'static str],
    pub case_insensitive: bool,
    /// `@name` is a decorator / annotation.
    pub decorators: bool,
    /// `#...` at line start is a preprocessor directive.
    pub preproc_hash: bool,
    /// `key = value` / `key: value` at line start highlights the key.
    pub kv_keys: bool,
    /// `[section]` lines are headers.
    pub sections: bool,
    /// `$var` is a variable.
    pub dollar_vars: bool,
    /// `"key":` strings are properties (JSON).
    pub string_keys: bool,
    /// `'x'` is a char literal; other `'` are lifetimes / operators.
    pub char_literals: bool,
    /// Capitalised identifiers are types.
    pub caps_types: bool,
    /// Extra characters allowed in identifiers.
    pub ident_extra: &'static str,
    /// Typing these on a blank line dedents.
    pub brace_dedent: bool,
    /// Line endings (after trim) that trigger extra indent on newline.
    pub indent_triggers: &'static [&'static str],
    pub tab_width: usize,
    /// `\command` names are keywords (TeX, roff).
    pub backslash_commands: bool,
}

impl Language {
    pub fn auto_dedent_char(&self, c: char) -> Option<char> {
        (self.brace_dedent && matches!(c, '}' | ']' | ')')).then_some(c)
    }

    pub fn indent_after(&self, trimmed: &str) -> bool {
        if self.mode == Mode::Html {
            return html_opens_block(trimmed);
        }
        let code = strip_trailing_comment(self, trimmed);
        self.indent_triggers.iter().any(|t| code.ends_with(t))
    }

    /// Comment markers used by the toggle-comment command.
    pub fn comment_markers(&self) -> Option<(&'static str, &'static str)> {
        if let Some(lc) = self.line_comment.first() {
            Some((lc, ""))
        } else {
            self.block_comment
        }
    }
}

fn strip_trailing_comment<'a>(lang: &Language, s: &'a str) -> &'a str {
    for lc in lang.line_comment {
        if let Some(i) = s.rfind(lc) {
            let before = &s[..i];
            // Only strip when the marker is not inside a string literal.
            let quotes = before.chars().filter(|c| lang.quotes.contains(*c)).count();
            if quotes % 2 == 0 {
                return before.trim_end();
            }
        }
    }
    s
}

const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
    "!doctype",
];

fn html_opens_block(s: &str) -> bool {
    if !s.ends_with('>') || s.ends_with("/>") || s.ends_with("-->") {
        return false;
    }
    let Some(open) = s.rfind('<') else { return false };
    let tag = &s[open + 1..];
    if tag.starts_with('/') || tag.starts_with('?') {
        return false;
    }
    let name: String = tag
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '!')
        .collect::<String>()
        .to_ascii_lowercase();
    !name.is_empty() && !VOID_TAGS.contains(&name.as_str())
}

const BASE: Language = Language {
    name: "Plain Text",
    aliases: &[],
    extensions: &[],
    filenames: &[],
    shebangs: &[],
    mode: Mode::Generic,
    line_comment: &[],
    block_comment: None,
    quotes: "\"'",
    multiline_strings: &[],
    string_prefixes: &[],
    keywords: &[],
    types: &[],
    constants: &[],
    builtins: &[],
    case_insensitive: false,
    decorators: false,
    preproc_hash: false,
    kv_keys: false,
    sections: false,
    dollar_vars: false,
    string_keys: false,
    char_literals: false,
    caps_types: false,
    ident_extra: "",
    brace_dedent: false,
    indent_triggers: &[],
    tab_width: 4,
    backslash_commands: false,
};

const C_FAMILY: Language = Language {
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    caps_types: true,
    brace_dedent: true,
    indent_triggers: &["{", "[", "("],
    ..BASE
};

pub static PLAIN: Language = Language {
    name: "Plain Text",
    aliases: &["text", "txt", "plain"],
    extensions: &["txt", "text", "log", "nfo", "asc", "readme", "me", "out"],
    filenames: &["readme", "license", "copying", "authors", "changelog", "todo"],
    mode: Mode::Plain,
    ..BASE
};

pub static PYTHON: Language = Language {
    name: "Python",
    aliases: &["python", "py"],
    extensions: &["py", "pyw", "pyi", "pyx", "pxd", "gyp", "gypi", "bzl", "star", "bazel", "sconstruct", "rpy", "pyde"],
    filenames: &["sconstruct", "sconscript", "build", "workspace", "build.bazel", "workspace.bazel", "tiltfile", "snakefile"],
    shebangs: &["python"],
    line_comment: &["#"],
    multiline_strings: &["\"\"\"", "'''"],
    string_prefixes: &["r", "b", "f", "u", "rb", "br", "fr", "rf"],
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else",
        "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not",
        "or", "pass", "raise", "return", "try", "while", "with", "yield", "match", "case",
    ],
    types: &[
        "int", "float", "str", "bool", "list", "dict", "set", "tuple", "bytes", "bytearray", "object", "complex",
        "frozenset", "type", "Exception", "ValueError", "TypeError", "KeyError", "IndexError", "RuntimeError",
        "OSError", "IOError", "StopIteration", "AttributeError", "NotImplementedError", "ImportError",
    ],
    constants: &["True", "False", "None", "self", "cls", "__name__", "__main__", "__file__", "NotImplemented", "Ellipsis"],
    builtins: &[
        "print", "len", "range", "open", "input", "enumerate", "zip", "map", "filter", "sorted", "reversed",
        "isinstance", "issubclass", "hasattr", "getattr", "setattr", "super", "min", "max", "sum", "abs", "any",
        "all", "iter", "next", "repr", "round", "format", "id", "hash", "vars", "dir", "callable", "property",
        "staticmethod", "classmethod",
    ],
    decorators: true,
    caps_types: true,
    indent_triggers: &[":"],
    ..BASE
};

pub static HTML: Language = Language {
    name: "HTML",
    aliases: &["html", "htm", "xhtml"],
    extensions: &[
        "html", "htm", "xhtml", "shtml", "vue", "svelte", "astro", "jinja", "jinja2", "j2", "hbs", "handlebars",
        "mustache", "ejs", "tmpl", "twig", "liquid", "njk", "cshtml", "razor", "jsp", "asp", "aspx", "gohtml", "heex",
        "leex", "eex", "erb",
    ],
    shebangs: &[],
    mode: Mode::Html,
    block_comment: Some(("<!--", "-->")),
    tab_width: 2,
    ..BASE
};

pub static XML: Language = Language {
    name: "XML",
    aliases: &["xml", "svg"],
    extensions: &[
        "xml", "svg", "xsd", "xsl", "xslt", "rss", "atom", "plist", "csproj", "vcxproj", "fsproj", "pom", "xaml",
        "kml", "gpx", "wsdl", "resx", "nuspec", "storyboard", "xib", "opml", "xlf", "xliff", "ui", "fxml", "xul",
        "tmx", "mxml", "glade", "props", "targets", "manifest", "xaml", "dtd", "ent", "rdf", "owl", "gml",
        "graphml", "musicxml", "mathml", "smil", "tei", "dbk", "docbook",
    ],
    mode: Mode::Html,
    block_comment: Some(("<!--", "-->")),
    tab_width: 2,
    ..BASE
};

pub static CSS: Language = Language {
    name: "CSS",
    aliases: &["css"],
    extensions: &["css"],
    mode: Mode::Css,
    block_comment: Some(("/*", "*/")),
    brace_dedent: true,
    indent_triggers: &["{", "("],
    ..BASE
};

pub static SCSS: Language = Language {
    name: "SCSS",
    aliases: &["scss", "sass", "less"],
    extensions: &["scss", "sass", "less", "styl"],
    mode: Mode::Css,
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    brace_dedent: true,
    indent_triggers: &["{", "("],
    dollar_vars: true,
    ..BASE
};

const JS_KEYWORDS: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else",
    "export", "extends", "finally", "for", "function", "if", "import", "in", "instanceof", "let", "new", "of",
    "return", "static", "super", "switch", "throw", "try", "typeof", "var", "void", "while", "with", "yield",
    "async", "await", "from", "as", "get", "set",
];

pub static JAVASCRIPT: Language = Language {
    name: "JavaScript",
    aliases: &["javascript", "js", "jsx"],
    extensions: &["js", "mjs", "cjs", "jsx", "es6", "gs"],
    shebangs: &["node", "deno", "bun"],
    multiline_strings: &["`"],
    keywords: JS_KEYWORDS,
    types: &["Array", "Object", "String", "Number", "Boolean", "Map", "Set", "Promise", "Date", "RegExp", "Error", "Symbol", "BigInt", "JSON", "Math"],
    constants: &["true", "false", "null", "undefined", "this", "NaN", "Infinity"],
    builtins: &["console", "window", "document", "require", "module", "exports", "globalThis", "process", "setTimeout", "setInterval", "fetch"],
    ident_extra: "$",
    ..C_FAMILY
};

pub static TYPESCRIPT: Language = Language {
    name: "TypeScript",
    aliases: &["typescript", "ts", "tsx"],
    extensions: &["ts", "tsx", "mts", "cts"],
    shebangs: &["ts-node"],
    multiline_strings: &["`"],
    keywords: &[
        "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else",
        "export", "extends", "finally", "for", "function", "if", "import", "in", "instanceof", "let", "new", "of",
        "return", "static", "super", "switch", "throw", "try", "typeof", "var", "void", "while", "with", "yield",
        "async", "await", "from", "as", "interface", "type", "enum", "implements", "namespace", "declare",
        "abstract", "private", "public", "protected", "readonly", "keyof", "infer", "is", "satisfies",
    ],
    types: &["string", "number", "boolean", "any", "unknown", "never", "object", "symbol", "bigint", "Array", "Promise", "Record", "Partial", "Readonly"],
    constants: &["true", "false", "null", "undefined", "this"],
    builtins: &["console", "window", "document", "require", "module", "process"],
    decorators: true,
    ident_extra: "$",
    ..C_FAMILY
};

pub static JSON: Language = Language {
    name: "JSON",
    aliases: &["json", "jsonc", "json5"],
    extensions: &[
        "json", "jsonc", "json5", "geojson", "topojson", "webmanifest", "ipynb", "har", "jsonl", "ndjson", "jsonld",
        "avsc", "code-workspace", "sublime-settings", "sublime-project", "mcmeta", "gltf", "jsonnet", "libsonnet",
    ],
    filenames: &[".babelrc", ".eslintrc", ".prettierrc", "composer.lock"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"",
    constants: &["true", "false", "null"],
    string_keys: true,
    brace_dedent: true,
    indent_triggers: &["{", "["],
    tab_width: 2,
    ..BASE
};

pub static RUST: Language = Language {
    name: "Rust",
    aliases: &["rust", "rs"],
    extensions: &["rs"],
    multiline_strings: &["\""],
    quotes: "'",
    string_prefixes: &["b", "r", "br", "c"],
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn", "for",
        "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static",
        "struct", "super", "trait", "type", "unsafe", "use", "where", "while", "macro_rules", "union", "yield",
    ],
    types: &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64",
        "bool", "char", "str", "Self",
    ],
    constants: &["true", "false", "self", "None", "Some", "Ok", "Err"],
    char_literals: true,
    preproc_hash: false,
    ..C_FAMILY
};

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern", "for", "goto", "if",
    "inline", "register", "restrict", "return", "sizeof", "static", "struct", "switch", "typedef", "union",
    "volatile", "while", "_Alignas", "_Alignof", "_Atomic", "_Generic", "_Noreturn", "_Static_assert",
];
const C_TYPES: &[&str] = &[
    "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "bool", "_Bool", "size_t",
    "ssize_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t",
    "uintptr_t", "intptr_t", "ptrdiff_t", "FILE", "wchar_t",
];

pub static C: Language = Language {
    name: "C",
    aliases: &["c", "h"],
    extensions: &["c", "h", "ino"],
    keywords: C_KEYWORDS,
    types: C_TYPES,
    constants: &["NULL", "true", "false", "EOF", "stdin", "stdout", "stderr"],
    char_literals: true,
    preproc_hash: true,
    ..C_FAMILY
};

pub static CPP: Language = Language {
    name: "C++",
    aliases: &["cpp", "c++", "cxx", "hpp"],
    extensions: &["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++", "ipp", "tpp", "cu", "cuh"],
    keywords: &[
        "alignas", "alignof", "auto", "break", "case", "catch", "class", "const", "constexpr", "consteval",
        "constinit", "const_cast", "continue", "co_await", "co_return", "co_yield", "decltype", "default",
        "delete", "do", "dynamic_cast", "else", "enum", "explicit", "export", "extern", "final", "for", "friend",
        "goto", "if", "inline", "mutable", "namespace", "new", "noexcept", "operator", "override", "private",
        "protected", "public", "register", "reinterpret_cast", "requires", "return", "sizeof", "static",
        "static_assert", "static_cast", "struct", "switch", "template", "this", "thread_local", "throw", "try",
        "typedef", "typeid", "typename", "union", "using", "virtual", "volatile", "while", "concept",
    ],
    types: &[
        "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "bool", "size_t",
        "wchar_t", "char8_t", "char16_t", "char32_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t",
        "uint16_t", "uint32_t", "uint64_t", "string", "vector", "map", "set", "unique_ptr", "shared_ptr",
    ],
    constants: &["nullptr", "NULL", "true", "false", "std"],
    char_literals: true,
    preproc_hash: true,
    ..C_FAMILY
};

pub static CSHARP: Language = Language {
    name: "C#",
    aliases: &["csharp", "cs", "c#"],
    extensions: &["cs", "csx"],
    keywords: &[
        "abstract", "as", "base", "break", "case", "catch", "checked", "class", "const", "continue", "default",
        "delegate", "do", "else", "enum", "event", "explicit", "extern", "finally", "fixed", "for", "foreach",
        "goto", "if", "implicit", "in", "interface", "internal", "is", "lock", "namespace", "new", "operator",
        "out", "override", "params", "private", "protected", "public", "readonly", "ref", "return", "sealed",
        "sizeof", "stackalloc", "static", "struct", "switch", "throw", "try", "typeof", "unchecked", "unsafe",
        "using", "virtual", "volatile", "while", "async", "await", "var", "record", "get", "set", "init", "yield",
    ],
    types: &["bool", "byte", "char", "decimal", "double", "float", "int", "long", "object", "sbyte", "short", "string", "uint", "ulong", "ushort", "void", "dynamic"],
    constants: &["true", "false", "null", "this"],
    char_literals: true,
    preproc_hash: true,
    ..C_FAMILY
};

pub static JAVA: Language = Language {
    name: "Java",
    aliases: &["java"],
    extensions: &["java", "jav"],
    keywords: &[
        "abstract", "assert", "break", "case", "catch", "class", "const", "continue", "default", "do", "else",
        "enum", "extends", "final", "finally", "for", "goto", "if", "implements", "import", "instanceof",
        "interface", "native", "new", "package", "private", "protected", "public", "return", "static",
        "strictfp", "super", "switch", "synchronized", "throw", "throws", "transient", "try", "volatile",
        "while", "var", "record", "sealed", "permits", "yield",
    ],
    types: &["boolean", "byte", "char", "short", "int", "long", "float", "double", "void", "String", "Object", "Integer", "List", "Map"],
    constants: &["true", "false", "null", "this"],
    char_literals: true,
    decorators: true,
    ..C_FAMILY
};

pub static KOTLIN: Language = Language {
    name: "Kotlin",
    aliases: &["kotlin", "kt"],
    extensions: &["kt", "kts"],
    multiline_strings: &["\"\"\""],
    keywords: &[
        "as", "break", "class", "continue", "do", "else", "for", "fun", "if", "in", "interface", "is", "object",
        "package", "return", "super", "throw", "try", "typealias", "val", "var", "when", "while", "by", "catch",
        "constructor", "finally", "get", "import", "init", "set", "where", "data", "sealed", "override", "open",
        "private", "public", "internal", "protected", "companion", "suspend", "lateinit", "enum", "abstract",
    ],
    types: &["Int", "Long", "Short", "Byte", "Double", "Float", "Boolean", "Char", "String", "Unit", "Any", "Nothing"],
    constants: &["true", "false", "null", "this"],
    char_literals: true,
    decorators: true,
    ..C_FAMILY
};

pub static GO: Language = Language {
    name: "Go",
    aliases: &["go", "golang"],
    extensions: &["go"],
    multiline_strings: &["`"],
    keywords: &[
        "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "for", "func",
        "go", "goto", "if", "import", "interface", "map", "package", "range", "return", "select", "struct",
        "switch", "type", "var",
    ],
    types: &[
        "bool", "byte", "complex64", "complex128", "error", "float32", "float64", "int", "int8", "int16", "int32",
        "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64", "uintptr", "any",
    ],
    constants: &["true", "false", "nil", "iota"],
    builtins: &["append", "cap", "close", "copy", "delete", "len", "make", "new", "panic", "print", "println", "recover"],
    char_literals: true,
    tab_width: 4,
    ..C_FAMILY
};

pub static SWIFT: Language = Language {
    name: "Swift",
    aliases: &["swift"],
    extensions: &["swift"],
    multiline_strings: &["\"\"\""],
    quotes: "\"",
    keywords: &[
        "associatedtype", "class", "deinit", "enum", "extension", "func", "import", "init", "inout", "let",
        "operator", "protocol", "static", "struct", "subscript", "typealias", "var", "break", "case", "continue",
        "default", "defer", "do", "else", "fallthrough", "for", "guard", "if", "in", "repeat", "return", "switch",
        "where", "while", "as", "catch", "is", "rethrows", "throw", "throws", "try", "async", "await", "public",
        "private", "internal", "fileprivate", "open", "override", "mutating", "weak", "lazy",
    ],
    types: &["Int", "Double", "Float", "String", "Bool", "Character", "Array", "Dictionary", "Set", "Optional", "Any", "Void"],
    constants: &["true", "false", "nil", "self", "Self", "super"],
    decorators: true,
    ..C_FAMILY
};

pub static PHP: Language = Language {
    name: "PHP",
    aliases: &["php"],
    extensions: &["php", "phtml", "php3", "php4", "php5", "phps"],
    shebangs: &["php"],
    line_comment: &["//", "#"],
    keywords: &[
        "abstract", "and", "as", "break", "case", "catch", "class", "clone", "const", "continue", "declare",
        "default", "do", "echo", "else", "elseif", "enddeclare", "endfor", "endforeach", "endif", "endswitch",
        "endwhile", "extends", "final", "finally", "fn", "for", "foreach", "function", "global", "goto", "if",
        "implements", "include", "include_once", "instanceof", "insteadof", "interface", "match", "namespace",
        "new", "or", "print", "private", "protected", "public", "readonly", "require", "require_once", "return",
        "static", "switch", "throw", "trait", "try", "use", "var", "while", "xor", "yield",
    ],
    types: &["int", "float", "bool", "string", "array", "object", "callable", "iterable", "void", "mixed"],
    constants: &["true", "false", "null", "TRUE", "FALSE", "NULL"],
    dollar_vars: true,
    ..C_FAMILY
};

pub static RUBY: Language = Language {
    name: "Ruby",
    aliases: &["ruby", "rb"],
    extensions: &["rb", "rake", "gemspec", "ru", "rbw", "podspec"],
    filenames: &["rakefile", "gemfile", "vagrantfile", "podfile", "guardfile"],
    shebangs: &["ruby"],
    line_comment: &["#"],
    keywords: &[
        "alias", "and", "begin", "break", "case", "class", "def", "defined?", "do", "else", "elsif", "end",
        "ensure", "for", "if", "in", "module", "next", "not", "or", "redo", "rescue", "retry", "return", "super",
        "then", "undef", "unless", "until", "when", "while", "yield", "require", "require_relative", "include",
        "attr_accessor", "attr_reader", "attr_writer", "private", "protected", "public",
    ],
    constants: &["true", "false", "nil", "self"],
    builtins: &["puts", "print", "p", "raise", "lambda", "proc", "loop"],
    caps_types: true,
    indent_triggers: &["do", "|", "then"],
    tab_width: 2,
    ..BASE
};

pub static SHELL: Language = Language {
    name: "Shell",
    aliases: &["sh", "bash", "shell", "zsh"],
    extensions: &["sh", "bash", "zsh", "ksh", "fish", "command", "bats", "ebuild", "eclass", "zsh-theme", "tmux"],
    filenames: &[
        ".bashrc", ".bash_profile", ".bash_aliases", ".profile", ".zshrc", ".zprofile", ".zshenv", "pkgbuild",
        ".envrc", "configure", ".bash_logout", ".xinitrc", ".xprofile", ".xsession", "apkbuild", ".zlogin",
        ".zlogout", ".kshrc", ".mkshrc",
    ],
    shebangs: &["sh", "bash", "zsh", "ksh", "dash", "fish"],
    line_comment: &["#"],
    keywords: &[
        "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "until", "do", "done", "in",
        "function", "select", "return", "exit", "break", "continue", "local", "export", "readonly", "declare",
        "unset", "shift", "source", "trap", "eval", "exec",
    ],
    builtins: &[
        "echo", "printf", "read", "cd", "pwd", "test", "set", "alias", "true", "false", "cat", "grep", "sed",
        "awk", "ls", "rm", "cp", "mv", "mkdir", "chmod", "sudo",
    ],
    dollar_vars: true,
    ident_extra: "-",
    indent_triggers: &["then", "do", "{"],
    ..BASE
};

pub static POWERSHELL: Language = Language {
    name: "PowerShell",
    aliases: &["powershell", "ps1", "pwsh"],
    extensions: &["ps1", "psm1", "psd1"],
    line_comment: &["#"],
    block_comment: Some(("<#", "#>")),
    keywords: &[
        "begin", "break", "catch", "class", "continue", "data", "do", "dynamicparam", "else", "elseif", "end",
        "exit", "filter", "finally", "for", "foreach", "from", "function", "if", "in", "param", "process",
        "return", "switch", "throw", "trap", "try", "until", "using", "var", "while",
    ],
    constants: &["$true", "$false", "$null"],
    case_insensitive: true,
    dollar_vars: true,
    brace_dedent: true,
    indent_triggers: &["{", "("],
    ident_extra: "-",
    ..BASE
};

pub static BATCH: Language = Language {
    name: "Batch",
    aliases: &["bat", "cmd", "batch"],
    extensions: &["bat", "cmd"],
    line_comment: &["rem ", "::", "REM "],
    keywords: &[
        "echo", "set", "if", "else", "for", "in", "do", "goto", "call", "exit", "not", "exist", "defined",
        "errorlevel", "setlocal", "endlocal", "shift", "pause", "cd", "start",
    ],
    case_insensitive: true,
    ..BASE
};

pub static LUA: Language = Language {
    name: "Lua",
    aliases: &["lua"],
    extensions: &["lua", "luau", "rockspec"],
    shebangs: &["lua"],
    line_comment: &["--"],
    block_comment: Some(("--[[", "]]")),
    multiline_strings: &["[["],
    keywords: &[
        "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local", "not",
        "or", "repeat", "return", "then", "until", "while",
    ],
    constants: &["true", "false", "nil", "self"],
    builtins: &["print", "pairs", "ipairs", "require", "type", "tostring", "tonumber", "table", "string", "math", "os", "io", "setmetatable", "pcall", "error"],
    indent_triggers: &["then", "do", "else", "function()", "{"],
    ..BASE
};

pub static SQL: Language = Language {
    name: "SQL",
    aliases: &["sql", "mysql", "psql", "sqlite"],
    extensions: &["sql", "ddl", "dml", "pgsql", "psql", "hql"],
    line_comment: &["--"],
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create", "table",
        "drop", "alter", "add", "column", "index", "view", "join", "inner", "left", "right", "outer", "full",
        "cross", "on", "as", "and", "or", "not", "null", "is", "in", "between", "like", "order", "by", "group",
        "having", "limit", "offset", "union", "all", "distinct", "case", "when", "then", "else", "end",
        "primary", "key", "foreign", "references", "default", "unique", "check", "constraint", "exists", "begin",
        "commit", "rollback", "transaction", "with", "returning", "if", "database", "schema", "grant", "revoke",
        "trigger", "procedure", "function", "asc", "desc", "cascade",
    ],
    types: &[
        "int", "integer", "bigint", "smallint", "tinyint", "decimal", "numeric", "float", "real", "double",
        "char", "varchar", "text", "date", "time", "timestamp", "datetime", "boolean", "bool", "blob", "json",
        "uuid", "serial",
    ],
    constants: &["true", "false"],
    builtins: &["count", "sum", "avg", "min", "max", "coalesce", "now", "cast", "lower", "upper", "length", "substr"],
    case_insensitive: true,
    ..BASE
};

pub static YAML: Language = Language {
    name: "YAML",
    aliases: &["yaml", "yml"],
    extensions: &["yaml", "yml", "clang-format", "clangd"],
    filenames: &[".clang-format", ".travis.yml"],
    line_comment: &["#"],
    constants: &["true", "false", "null", "yes", "no", "on", "off", "~"],
    kv_keys: true,
    ident_extra: "-.",
    indent_triggers: &[":", "|", ">"],
    tab_width: 2,
    ..BASE
};

pub static TOML: Language = Language {
    name: "TOML",
    aliases: &["toml"],
    extensions: &["toml"],
    filenames: &["cargo.lock", "pipfile", "poetry.lock"],
    line_comment: &["#"],
    multiline_strings: &["\"\"\"", "'''"],
    constants: &["true", "false", "inf", "nan"],
    kv_keys: true,
    sections: true,
    ident_extra: "-.",
    ..BASE
};

pub static INI: Language = Language {
    name: "INI",
    aliases: &["ini", "conf", "cfg", "properties", "env"],
    extensions: &[
        "ini", "cfg", "conf", "config", "properties", "prefs", "inf", "reg", "desktop", "service", "socket", "timer",
        "mount", "target", "slice", "env", "editorconfig", "gitconfig", "npmrc", "url", "pls", "m3u8", "ovpn", "cnf", "pypirc", "wgconf",
    ],
    filenames: &[".gitconfig", ".editorconfig", ".npmrc", ".env", ".gitmodules", ".flake8", "setup.cfg", "tox.ini"],
    line_comment: &["#", ";"],
    constants: &["true", "false", "yes", "no", "on", "off"],
    kv_keys: true,
    sections: true,
    ident_extra: "-.",
    ..BASE
};

pub static MARKDOWN: Language = Language {
    name: "Markdown",
    aliases: &["markdown", "md"],
    extensions: &["md", "markdown", "mdown", "mkd", "mkdn", "mdx", "rst", "adoc", "org"],
    mode: Mode::Markdown,
    block_comment: Some(("<!--", "-->")),
    tab_width: 2,
    ..BASE
};

pub static MAKEFILE: Language = Language {
    name: "Makefile",
    aliases: &["make", "makefile", "mk"],
    extensions: &["mk", "mak", "make"],
    filenames: &["makefile", "gnumakefile", "bsdmakefile"],
    line_comment: &["#"],
    keywords: &["ifeq", "ifneq", "ifdef", "ifndef", "else", "endif", "include", "define", "endef", "export", "override", ".PHONY"],
    dollar_vars: true,
    kv_keys: true,
    ident_extra: "-.",
    ..BASE
};

pub static DOCKERFILE: Language = Language {
    name: "Dockerfile",
    aliases: &["dockerfile", "docker", "containerfile"],
    extensions: &["dockerfile", "containerfile"],
    filenames: &["dockerfile", "containerfile"],
    line_comment: &["#"],
    keywords: &[
        "FROM", "RUN", "CMD", "LABEL", "MAINTAINER", "EXPOSE", "ENV", "ADD", "COPY", "ENTRYPOINT", "VOLUME",
        "USER", "WORKDIR", "ARG", "ONBUILD", "STOPSIGNAL", "HEALTHCHECK", "SHELL", "AS",
    ],
    case_insensitive: true,
    dollar_vars: true,
    ..BASE
};

pub static PERL: Language = Language {
    name: "Perl",
    aliases: &["perl", "pl"],
    extensions: &["pl", "pm", "t", "pod"],
    shebangs: &["perl"],
    line_comment: &["#"],
    keywords: &[
        "my", "our", "local", "sub", "if", "elsif", "else", "unless", "while", "until", "for", "foreach", "do",
        "last", "next", "redo", "return", "use", "require", "package", "and", "or", "not", "eq", "ne", "lt", "gt",
    ],
    builtins: &["print", "printf", "die", "open", "close", "push", "pop", "shift", "split", "join", "keys", "values"],
    dollar_vars: true,
    brace_dedent: true,
    indent_triggers: &["{", "("],
    ..BASE
};

pub static R: Language = Language {
    name: "R",
    aliases: &["r"],
    extensions: &["r", "rmd"],
    shebangs: &["rscript"],
    line_comment: &["#"],
    keywords: &["if", "else", "repeat", "while", "function", "for", "in", "next", "break", "return", "library"],
    constants: &["TRUE", "FALSE", "NULL", "NA", "Inf", "NaN"],
    brace_dedent: true,
    indent_triggers: &["{", "("],
    ..BASE
};

pub static HASKELL: Language = Language {
    name: "Haskell",
    aliases: &["haskell", "hs"],
    extensions: &["hs", "lhs"],
    line_comment: &["--"],
    block_comment: Some(("{-", "-}")),
    quotes: "\"",
    keywords: &[
        "case", "class", "data", "default", "deriving", "do", "else", "if", "import", "in", "infix", "infixl",
        "infixr", "instance", "let", "module", "newtype", "of", "then", "type", "where", "qualified", "as",
        "hiding",
    ],
    constants: &["True", "False", "Nothing", "Just"],
    caps_types: true,
    ident_extra: "'",
    indent_triggers: &["where", "do", "of", "="],
    tab_width: 2,
    ..BASE
};

pub static LISP: Language = Language {
    name: "Lisp",
    aliases: &["lisp", "clojure", "scheme", "elisp", "racket"],
    extensions: &["lisp", "lsp", "cl", "el", "clj", "cljs", "cljc", "edn", "scm", "ss", "rkt", "fnl"],
    line_comment: &[";"],
    quotes: "\"",
    keywords: &[
        "defun", "defn", "defmacro", "define", "let", "let*", "lambda", "fn", "if", "cond", "when", "unless",
        "progn", "do", "loop", "setq", "setf", "def", "ns", "require", "quote", "and", "or", "not",
    ],
    constants: &["nil", "t", "true", "false", "#t", "#f"],
    ident_extra: "-*!?<>=+/",
    tab_width: 2,
    ..BASE
};

pub static DIFF: Language = Language {
    name: "Diff",
    aliases: &["diff", "patch"],
    extensions: &["diff", "patch", "rej"],
    mode: Mode::Diff,
    ..BASE
};

pub static CSV: Language = Language {
    name: "CSV",
    aliases: &["csv", "tsv"],
    extensions: &["csv", "tsv", "psv"],
    quotes: "\"",
    mode: Mode::Generic,
    ..BASE
};

pub static HEX: Language = Language {
    name: "Hex",
    aliases: &["hex", "binary"],
    mode: Mode::Hex,
    ..BASE
};

// ----------------------------------------------------------------------
// Additional languages
// ----------------------------------------------------------------------

pub static ZIG: Language = Language {
    name: "Zig",
    aliases: &["zig"],
    extensions: &["zig", "zon"],
    keywords: &[
        "addrspace", "align", "allowzero", "and", "anyframe", "anytype", "asm", "async", "await", "break",
        "callconv", "catch", "comptime", "const", "continue", "defer", "else", "enum", "errdefer", "error", "export",
        "extern", "fn", "for", "if", "inline", "linksection", "noalias", "noinline", "nosuspend", "opaque", "or",
        "orelse", "packed", "pub", "resume", "return", "struct", "suspend", "switch", "test", "threadlocal", "try",
        "union", "unreachable", "usingnamespace", "var", "volatile", "while",
    ],
    types: &["i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "isize", "usize", "f32", "f64", "bool", "void", "type", "anyerror", "noreturn", "c_int"],
    constants: &["true", "false", "null", "undefined"],
    decorators: true,
    ..C_FAMILY
};

pub static DART: Language = Language {
    name: "Dart",
    aliases: &["dart"],
    extensions: &["dart"],
    multiline_strings: &["\"\"\"", "'''"],
    string_prefixes: &["r"],
    keywords: &[
        "abstract", "as", "assert", "async", "await", "base", "break", "case", "catch", "class", "const", "continue",
        "covariant", "default", "deferred", "do", "dynamic", "else", "enum", "export", "extends", "extension",
        "external", "factory", "final", "finally", "for", "get", "hide", "if", "implements", "import", "in",
        "interface", "is", "late", "library", "mixin", "new", "of", "on", "operator", "part", "required", "rethrow",
        "return", "sealed", "set", "show", "static", "super", "switch", "sync", "throw", "try", "typedef", "var",
        "when", "while", "with", "yield",
    ],
    types: &["int", "double", "num", "bool", "String", "List", "Map", "Set", "Future", "Stream", "void", "Object"],
    constants: &["true", "false", "null", "this"],
    decorators: true,
    dollar_vars: true,
    tab_width: 2,
    ..C_FAMILY
};

pub static SCALA: Language = Language {
    name: "Scala",
    aliases: &["scala"],
    extensions: &["scala", "sc", "sbt"],
    multiline_strings: &["\"\"\""],
    keywords: &[
        "abstract", "case", "catch", "class", "def", "do", "else", "enum", "export", "extends", "final", "finally",
        "for", "forSome", "given", "if", "implicit", "import", "lazy", "match", "new", "object", "override",
        "package", "private", "protected", "return", "sealed", "super", "then", "throw", "trait", "try", "type",
        "using", "val", "var", "while", "with", "yield",
    ],
    types: &["Int", "Long", "Double", "Float", "Boolean", "Char", "String", "Unit", "Any", "AnyRef", "Nothing", "Option", "List", "Map", "Seq"],
    constants: &["true", "false", "null", "this", "None", "Some", "Nil"],
    char_literals: true,
    decorators: true,
    tab_width: 2,
    ..C_FAMILY
};

pub static GROOVY: Language = Language {
    name: "Groovy",
    aliases: &["groovy", "gradle"],
    extensions: &["groovy", "gvy", "gy", "gsh", "gradle"],
    filenames: &["jenkinsfile"],
    shebangs: &["groovy"],
    multiline_strings: &["\"\"\"", "'''"],
    keywords: &[
        "as", "assert", "break", "case", "catch", "class", "const", "continue", "def", "default", "do", "else",
        "enum", "extends", "finally", "for", "goto", "if", "implements", "import", "in", "instanceof", "interface",
        "new", "package", "return", "super", "switch", "throw", "throws", "trait", "try", "var", "while",
        "pipeline", "stage", "stages", "steps", "agent",
    ],
    types: &["boolean", "byte", "char", "short", "int", "long", "float", "double", "void", "String", "Object", "List", "Map"],
    constants: &["true", "false", "null", "this"],
    decorators: true,
    dollar_vars: true,
    ..C_FAMILY
};

pub static D: Language = Language {
    name: "D",
    aliases: &["d", "dlang"],
    extensions: &["d", "di"],
    keywords: &[
        "abstract", "alias", "align", "asm", "assert", "auto", "break", "case", "cast", "catch", "class", "const",
        "continue", "debug", "default", "delegate", "delete", "deprecated", "do", "else", "enum", "export", "extern",
        "final", "finally", "for", "foreach", "foreach_reverse", "function", "goto", "if", "immutable", "import",
        "in", "inout", "interface", "invariant", "is", "lazy", "mixin", "module", "new", "nothrow", "out",
        "override", "package", "pragma", "private", "protected", "public", "pure", "ref", "return", "scope",
        "shared", "static", "struct", "super", "switch", "synchronized", "template", "throw", "try", "typeof",
        "union", "unittest", "version", "while", "with",
    ],
    types: &["bool", "byte", "ubyte", "short", "ushort", "int", "uint", "long", "ulong", "float", "double", "real", "char", "wchar", "dchar", "string", "void", "size_t"],
    constants: &["true", "false", "null", "this"],
    char_literals: true,
    decorators: true,
    ..C_FAMILY
};

pub static OBJC: Language = Language {
    name: "Objective-C",
    aliases: &["objc", "objective-c", "objectivec"],
    extensions: &["m", "mm"],
    keywords: &[
        "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern", "for", "goto", "if",
        "inline", "register", "return", "sizeof", "static", "struct", "switch", "typedef", "union", "volatile",
        "while", "self", "super", "id", "Class", "SEL", "IMP", "BOOL", "nil", "Nil", "YES", "NO", "in", "out",
        "inout", "bycopy", "byref", "oneway", "__block", "__weak", "__strong", "nonatomic", "atomic", "strong",
        "weak", "assign", "copy", "readonly", "readwrite", "nullable", "nonnull",
    ],
    types: &["void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "NSString", "NSInteger", "NSUInteger", "CGFloat", "NSArray", "NSDictionary"],
    constants: &["YES", "NO", "nil", "NULL", "true", "false"],
    char_literals: true,
    decorators: true,
    preproc_hash: true,
    ..C_FAMILY
};

pub static SOLIDITY: Language = Language {
    name: "Solidity",
    aliases: &["solidity", "sol"],
    extensions: &["sol"],
    keywords: &[
        "pragma", "solidity", "import", "contract", "interface", "library", "abstract", "function", "modifier",
        "event", "emit", "struct", "enum", "mapping", "public", "private", "internal", "external", "pure", "view",
        "payable", "constant", "immutable", "virtual", "override", "returns", "return", "if", "else", "for",
        "while", "do", "break", "continue", "require", "revert", "assert", "memory", "storage", "calldata", "new",
        "delete", "using", "is", "constructor", "receive", "fallback", "try", "catch", "error", "unchecked",
    ],
    types: &["address", "bool", "string", "bytes", "uint", "int", "uint8", "uint256", "int256", "bytes32", "byte"],
    constants: &["true", "false", "msg", "block", "tx", "this", "wei", "gwei", "ether"],
    ..C_FAMILY
};

pub static SHADER: Language = Language {
    name: "Shader (GLSL/HLSL)",
    aliases: &["glsl", "hlsl", "shader", "wgsl", "metal"],
    extensions: &["glsl", "vert", "frag", "geom", "comp", "tesc", "tese", "hlsl", "hlsli", "fx", "fxh", "wgsl", "metal", "shader", "cginc", "compute"],
    keywords: &[
        "attribute", "const", "uniform", "varying", "buffer", "shared", "coherent", "volatile", "restrict", "readonly",
        "writeonly", "layout", "centroid", "flat", "smooth", "noperspective", "patch", "sample", "break", "continue",
        "do", "for", "while", "switch", "case", "default", "if", "else", "subroutine", "in", "out", "inout",
        "invariant", "precise", "discard", "return", "struct", "precision", "highp", "mediump", "lowp", "cbuffer",
        "register", "fn", "let", "var", "kernel", "vertex", "fragment", "Shader", "Pass", "SubShader", "Properties",
    ],
    types: &[
        "void", "bool", "int", "uint", "float", "double", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4", "uvec2",
        "uvec3", "uvec4", "bvec2", "bvec3", "bvec4", "mat2", "mat3", "mat4", "sampler2D", "sampler3D",
        "samplerCube", "float2", "float3", "float4", "float4x4", "half", "half3", "half4", "Texture2D",
        "SamplerState", "f32", "i32", "u32", "vec2f", "vec3f", "vec4f",
    ],
    constants: &["true", "false"],
    builtins: &["gl_Position", "gl_FragColor", "gl_FragCoord", "texture", "mix", "clamp", "dot", "cross", "normalize", "length", "smoothstep", "step", "mul", "saturate", "lerp"],
    preproc_hash: true,
    decorators: true,
    ..C_FAMILY
};

pub static PROTOBUF: Language = Language {
    name: "Protocol Buffers",
    aliases: &["protobuf", "proto"],
    extensions: &["proto", "proto3"],
    keywords: &[
        "syntax", "edition", "package", "import", "public", "weak", "option", "message", "enum", "service", "rpc",
        "returns", "stream", "repeated", "optional", "required", "reserved", "extensions", "extend", "oneof", "map",
        "to", "max",
    ],
    types: &["double", "float", "int32", "int64", "uint32", "uint64", "sint32", "sint64", "fixed32", "fixed64", "sfixed32", "sfixed64", "bool", "string", "bytes"],
    constants: &["true", "false"],
    tab_width: 2,
    ..C_FAMILY
};

pub static VERILOG: Language = Language {
    name: "Verilog",
    aliases: &["verilog", "systemverilog", "sv"],
    extensions: &["v", "vh", "sv", "svh"],
    keywords: &[
        "module", "endmodule", "input", "output", "inout", "wire", "reg", "logic", "assign", "always", "always_ff",
        "always_comb", "always_latch", "initial", "begin", "end", "if", "else", "case", "casez", "casex",
        "endcase", "default", "for", "while", "repeat", "forever", "function", "endfunction", "task", "endtask",
        "parameter", "localparam", "generate", "endgenerate", "genvar", "posedge", "negedge", "or", "and", "not",
        "integer", "real", "time", "interface", "endinterface", "package", "endpackage", "import", "typedef",
        "enum", "struct", "class", "endclass",
    ],
    constants: &["1'b0", "1'b1"],
    quotes: "\"",
    caps_types: false,
    ..C_FAMILY
};

pub static GRAPHVIZ: Language = Language {
    name: "Graphviz DOT",
    aliases: &["dot", "graphviz", "gv"],
    extensions: &["dot", "gv"],
    keywords: &["strict", "graph", "digraph", "subgraph", "node", "edge"],
    quotes: "\"",
    caps_types: false,
    preproc_hash: true,
    ..C_FAMILY
};

pub static VALA: Language = Language {
    name: "Vala",
    aliases: &["vala"],
    extensions: &["vala", "vapi"],
    multiline_strings: &["\"\"\""],
    keywords: &[
        "abstract", "as", "async", "base", "break", "case", "catch", "class", "const", "construct", "continue",
        "default", "delegate", "delete", "do", "else", "enum", "errordomain", "extern", "finally", "for", "foreach",
        "get", "if", "in", "inline", "interface", "internal", "is", "lock", "namespace", "new", "out", "override",
        "owned", "private", "protected", "public", "ref", "return", "set", "signal", "sizeof", "static", "struct",
        "switch", "throw", "throws", "try", "typeof", "unowned", "using", "var", "virtual", "weak", "while", "yield",
    ],
    types: &["bool", "char", "int", "uint", "long", "ulong", "float", "double", "string", "void", "uint8", "int64"],
    constants: &["true", "false", "null", "this"],
    char_literals: true,
    preproc_hash: true,
    ..C_FAMILY
};

pub static HAXE: Language = Language {
    name: "Haxe",
    aliases: &["haxe", "hx"],
    extensions: &["hx", "hxml"],
    keywords: &[
        "abstract", "break", "case", "cast", "catch", "class", "continue", "default", "do", "dynamic", "else",
        "enum", "extends", "extern", "final", "for", "function", "if", "implements", "import", "in", "inline",
        "interface", "macro", "new", "operator", "overload", "override", "package", "private", "public", "return",
        "static", "switch", "throw", "try", "typedef", "untyped", "using", "var", "while",
    ],
    types: &["Int", "Float", "Bool", "String", "Void", "Dynamic", "Array", "Map"],
    constants: &["true", "false", "null", "this"],
    decorators: true,
    preproc_hash: true,
    ..C_FAMILY
};

pub static GLEAM: Language = Language {
    name: "Gleam",
    aliases: &["gleam"],
    extensions: &["gleam"],
    quotes: "\"",
    keywords: &["as", "assert", "case", "const", "external", "fn", "if", "import", "let", "opaque", "panic", "pub", "todo", "type", "use"],
    types: &["Int", "Float", "String", "Bool", "List", "Result", "Nil"],
    constants: &["True", "False", "Ok", "Error", "Nil"],
    decorators: true,
    tab_width: 2,
    ..C_FAMILY
};

pub static BICEP: Language = Language {
    name: "Bicep",
    aliases: &["bicep"],
    extensions: &["bicep", "bicepparam"],
    quotes: "'",
    multiline_strings: &["'''"],
    keywords: &["targetScope", "param", "var", "resource", "module", "output", "existing", "for", "in", "if", "import", "type", "func", "using", "metadata"],
    types: &["string", "int", "bool", "object", "array"],
    constants: &["true", "false", "null"],
    decorators: true,
    tab_width: 2,
    ..C_FAMILY
};

pub static HCL: Language = Language {
    name: "HCL / Terraform",
    aliases: &["hcl", "terraform", "tf"],
    extensions: &["tf", "tfvars", "hcl", "nomad", "tftest"],
    filenames: &[".terraformrc", "terraform.rc"],
    line_comment: &["#", "//"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"",
    keywords: &["resource", "data", "variable", "output", "locals", "module", "provider", "terraform", "for_each", "count", "depends_on", "lifecycle", "dynamic", "for", "in", "if", "else", "endif", "endfor", "moved", "import", "check"],
    types: &["string", "number", "bool", "list", "map", "set", "object", "tuple", "any"],
    constants: &["true", "false", "null"],
    kv_keys: true,
    brace_dedent: true,
    indent_triggers: &["{", "[", "("],
    tab_width: 2,
    ..BASE
};

pub static NIX: Language = Language {
    name: "Nix",
    aliases: &["nix"],
    extensions: &["nix"],
    line_comment: &["#"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"",
    multiline_strings: &["''"],
    keywords: &["let", "in", "with", "rec", "inherit", "if", "then", "else", "assert", "or", "import"],
    constants: &["true", "false", "null"],
    builtins: &["builtins", "derivation", "fetchurl", "fetchTarball", "map", "toString", "throw", "abort"],
    dollar_vars: true,
    ident_extra: "-'",
    brace_dedent: true,
    indent_triggers: &["{", "[", "(", "="],
    tab_width: 2,
    ..BASE
};

pub static GRAPHQL: Language = Language {
    name: "GraphQL",
    aliases: &["graphql", "gql"],
    extensions: &["graphql", "gql", "graphqls"],
    line_comment: &["#"],
    quotes: "\"",
    multiline_strings: &["\"\"\""],
    keywords: &["query", "mutation", "subscription", "fragment", "on", "type", "interface", "union", "enum", "input", "scalar", "schema", "extend", "implements", "directive", "repeatable"],
    types: &["Int", "Float", "String", "Boolean", "ID"],
    constants: &["true", "false", "null"],
    decorators: true,
    dollar_vars: true,
    caps_types: true,
    brace_dedent: true,
    indent_triggers: &["{", "("],
    tab_width: 2,
    ..BASE
};

pub static ELIXIR: Language = Language {
    name: "Elixir",
    aliases: &["elixir", "ex", "exs"],
    extensions: &["ex", "exs"],
    filenames: &["mix.lock"],
    shebangs: &["elixir"],
    line_comment: &["#"],
    quotes: "\"'",
    multiline_strings: &["\"\"\"", "'''"],
    keywords: &[
        "def", "defp", "defmodule", "defmacro", "defmacrop", "defstruct", "defprotocol", "defimpl", "defdelegate",
        "defexception", "defguard", "do", "end", "fn", "if", "else", "unless", "cond", "case", "when", "with",
        "for", "try", "catch", "rescue", "after", "raise", "receive", "quote", "unquote", "import", "require",
        "alias", "use", "and", "or", "not", "in",
    ],
    constants: &["true", "false", "nil"],
    decorators: true,
    caps_types: true,
    ident_extra: "?!",
    indent_triggers: &["do", "->", "fn"],
    tab_width: 2,
    ..BASE
};

pub static ERLANG: Language = Language {
    name: "Erlang",
    aliases: &["erlang", "erl"],
    extensions: &["erl", "hrl", "escript", "app.src"],
    filenames: &["rebar.config", "rebar.lock"],
    shebangs: &["escript"],
    line_comment: &["%"],
    quotes: "\"",
    keywords: &[
        "after", "and", "andalso", "band", "begin", "bnot", "bor", "bsl", "bsr", "bxor", "case", "catch", "cond",
        "div", "end", "fun", "if", "let", "maybe", "not", "of", "or", "orelse", "receive", "rem", "try", "when", "xor",
    ],
    constants: &["true", "false", "ok", "error", "undefined"],
    caps_types: true,
    indent_triggers: &["->", "of", "begin"],
    ..BASE
};

pub static OCAML: Language = Language {
    name: "OCaml",
    aliases: &["ocaml", "ml", "reasonml"],
    extensions: &["ml", "mli", "mll", "mly", "re", "rei"],
    block_comment: Some(("(*", "*)")),
    quotes: "\"",
    keywords: &[
        "and", "as", "assert", "begin", "class", "constraint", "do", "done", "downto", "else", "end", "exception",
        "external", "for", "fun", "function", "functor", "if", "in", "include", "inherit", "initializer", "lazy",
        "let", "match", "method", "module", "mutable", "new", "object", "of", "open", "or", "private", "rec", "sig",
        "struct", "then", "to", "try", "type", "val", "virtual", "when", "while", "with",
    ],
    types: &["int", "float", "bool", "char", "string", "unit", "list", "array", "option", "ref"],
    constants: &["true", "false", "None", "Some"],
    caps_types: true,
    ident_extra: "'",
    indent_triggers: &["=", "->", "in", "then", "else", "do", "struct", "sig", "begin"],
    tab_width: 2,
    ..BASE
};

pub static FSHARP: Language = Language {
    name: "F#",
    aliases: &["fsharp", "f#", "fs"],
    extensions: &["fs", "fsi", "fsx", "fsscript"],
    line_comment: &["//"],
    block_comment: Some(("(*", "*)")),
    quotes: "\"",
    multiline_strings: &["\"\"\""],
    keywords: &[
        "abstract", "and", "as", "assert", "base", "begin", "class", "default", "delegate", "do", "done", "downcast",
        "downto", "elif", "else", "end", "exception", "extern", "for", "fun", "function", "global", "if", "in",
        "inherit", "inline", "interface", "internal", "lazy", "let", "match", "member", "module", "mutable",
        "namespace", "new", "of", "open", "or", "override", "private", "public", "rec", "return", "static", "struct",
        "then", "to", "try", "type", "upcast", "use", "val", "void", "when", "while", "with", "yield", "async",
    ],
    types: &["int", "float", "bool", "char", "string", "unit", "list", "array", "seq", "option", "decimal", "byte"],
    constants: &["true", "false", "null", "None", "Some"],
    caps_types: true,
    preproc_hash: true,
    decorators: false,
    indent_triggers: &["=", "->", "then", "else", "do", "with"],
    ..BASE
};

pub static JULIA: Language = Language {
    name: "Julia",
    aliases: &["julia", "jl"],
    extensions: &["jl"],
    shebangs: &["julia"],
    line_comment: &["#"],
    block_comment: Some(("#=", "=#")),
    quotes: "\"",
    multiline_strings: &["\"\"\""],
    keywords: &[
        "baremodule", "begin", "break", "catch", "const", "continue", "do", "else", "elseif", "end", "export",
        "finally", "for", "function", "global", "if", "import", "let", "local", "macro", "module", "quote", "return",
        "struct", "try", "using", "while", "abstract", "mutable", "primitive", "type", "where", "in", "isa",
    ],
    types: &["Int", "Int64", "Float64", "Bool", "String", "Char", "Vector", "Matrix", "Array", "Dict", "Any", "Nothing", "Real", "Number"],
    constants: &["true", "false", "nothing", "missing", "NaN", "Inf", "pi"],
    builtins: &["println", "print", "length", "push!", "map", "filter", "collect", "zeros", "ones", "size"],
    decorators: true,
    dollar_vars: true,
    ident_extra: "!",
    caps_types: true,
    indent_triggers: &["function", "do", "begin", "then"],
    ..BASE
};

pub static NIM: Language = Language {
    name: "Nim",
    aliases: &["nim"],
    extensions: &["nim", "nims", "nimble"],
    shebangs: &["nim"],
    line_comment: &["#"],
    block_comment: Some(("#[", "]#")),
    quotes: "\"'",
    multiline_strings: &["\"\"\""],
    string_prefixes: &["r", "fmt"],
    keywords: &[
        "addr", "and", "as", "asm", "bind", "block", "break", "case", "cast", "concept", "const", "continue",
        "converter", "defer", "discard", "distinct", "div", "do", "elif", "else", "end", "enum", "except", "export",
        "finally", "for", "from", "func", "if", "import", "in", "include", "interface", "is", "isnot", "iterator",
        "let", "macro", "method", "mixin", "mod", "nil", "not", "notin", "object", "of", "or", "out", "proc", "ptr",
        "raise", "ref", "return", "shl", "shr", "static", "template", "try", "tuple", "type", "using", "var", "when",
        "while", "xor", "yield",
    ],
    types: &["int", "int8", "int16", "int32", "int64", "uint", "uint8", "float", "float32", "float64", "bool", "char", "string", "seq", "array", "void"],
    constants: &["true", "false", "nil"],
    builtins: &["echo", "len", "add", "high", "low", "inc", "dec", "new", "repr"],
    char_literals: true,
    caps_types: true,
    indent_triggers: &[":", "=", "->"],
    tab_width: 2,
    ..BASE
};

pub static CRYSTAL: Language = Language {
    name: "Crystal",
    aliases: &["crystal", "cr"],
    extensions: &["cr"],
    shebangs: &["crystal"],
    line_comment: &["#"],
    keywords: &[
        "abstract", "alias", "annotation", "as", "begin", "break", "case", "class", "def", "do", "else", "elsif",
        "end", "ensure", "enum", "extend", "for", "fun", "if", "in", "include", "lib", "macro", "module", "next", "of",
        "out", "private", "protected", "require", "rescue", "return", "select", "struct", "super", "then", "type",
        "typeof", "union", "unless", "until", "when", "while", "with", "yield",
    ],
    constants: &["true", "false", "nil", "self"],
    builtins: &["puts", "print", "p", "raise", "loop", "spawn"],
    decorators: true,
    caps_types: true,
    ident_extra: "?!",
    indent_triggers: &["do", "|", "then"],
    tab_width: 2,
    ..BASE
};

pub static ELM: Language = Language {
    name: "Elm",
    aliases: &["elm", "purescript", "purs"],
    extensions: &["elm", "purs"],
    line_comment: &["--"],
    block_comment: Some(("{-", "-}")),
    quotes: "\"'",
    multiline_strings: &["\"\"\""],
    keywords: &["module", "exposing", "import", "as", "type", "alias", "port", "if", "then", "else", "case", "of", "let", "in", "where", "do", "forall", "foreign", "data", "class", "instance"],
    constants: &["True", "False", "Nothing", "Just"],
    char_literals: true,
    caps_types: true,
    ident_extra: "'",
    indent_triggers: &["=", "->", "of", "let", "in", "then", "else"],
    ..BASE
};

pub static FORTRAN: Language = Language {
    name: "Fortran",
    aliases: &["fortran", "f90"],
    extensions: &["f", "for", "ftn", "f77", "f90", "f95", "f03", "f08", "fpp"],
    line_comment: &["!"],
    keywords: &[
        "program", "end", "module", "use", "implicit", "none", "contains", "subroutine", "function", "call",
        "return", "if", "then", "else", "elseif", "endif", "do", "enddo", "while", "select", "case", "default",
        "exit", "cycle", "stop", "print", "write", "read", "open", "close", "allocate", "deallocate", "intent", "in",
        "out", "inout", "parameter", "dimension", "allocatable", "pointer", "target", "type", "interface",
        "result", "recursive", "pure", "elemental", "where", "forall", "go", "to", "continue", "format", "save",
        "data", "common", "include",
    ],
    types: &["integer", "real", "double", "precision", "complex", "logical", "character"],
    constants: &[".true.", ".false."],
    case_insensitive: true,
    ident_extra: ".",
    indent_triggers: &["then", "do"],
    tab_width: 3,
    ..BASE
};

pub static PASCAL: Language = Language {
    name: "Pascal / Delphi",
    aliases: &["pascal", "delphi", "pas", "freepascal"],
    extensions: &["pas", "pp", "dpr", "lpr", "dpk", "lpk"],
    line_comment: &["//"],
    block_comment: Some(("{", "}")),
    quotes: "'",
    keywords: &[
        "and", "array", "as", "asm", "begin", "case", "class", "const", "constructor", "destructor", "div", "do",
        "downto", "else", "end", "except", "exports", "file", "finalization", "finally", "for", "function", "goto",
        "if", "implementation", "in", "inherited", "initialization", "inline", "interface", "is", "label",
        "library", "mod", "nil", "not", "object", "of", "or", "out", "packed", "procedure", "program", "property",
        "raise", "record", "repeat", "set", "shl", "shr", "then", "to", "try", "type", "unit", "until", "uses",
        "var", "while", "with", "xor", "private", "public", "protected", "published", "override", "virtual",
    ],
    types: &["integer", "cardinal", "shortint", "smallint", "longint", "int64", "byte", "word", "real", "double", "single", "extended", "boolean", "char", "string", "pointer"],
    constants: &["true", "false", "nil", "self"],
    builtins: &["writeln", "write", "readln", "read", "length", "inc", "dec", "ord", "chr", "high", "low", "setlength"],
    case_insensitive: true,
    indent_triggers: &["begin", "then", "do", "of", "else", "record", "repeat", "try"],
    tab_width: 2,
    ..BASE
};

pub static ADA: Language = Language {
    name: "Ada",
    aliases: &["ada"],
    extensions: &["adb", "ads", "ada"],
    line_comment: &["--"],
    quotes: "\"",
    keywords: &[
        "abort", "abs", "abstract", "accept", "access", "aliased", "all", "and", "array", "at", "begin", "body",
        "case", "constant", "declare", "delay", "delta", "digits", "do", "else", "elsif", "end", "entry",
        "exception", "exit", "for", "function", "generic", "goto", "if", "in", "interface", "is", "limited", "loop",
        "mod", "new", "not", "null", "of", "or", "others", "out", "overriding", "package", "pragma", "private",
        "procedure", "protected", "raise", "range", "record", "rem", "renames", "requeue", "return", "reverse",
        "select", "separate", "some", "subtype", "synchronized", "tagged", "task", "terminate", "then", "type",
        "until", "use", "when", "while", "with", "xor",
    ],
    types: &["Integer", "Natural", "Positive", "Float", "Boolean", "Character", "String", "Duration"],
    constants: &["True", "False"],
    case_insensitive: true,
    indent_triggers: &["is", "begin", "then", "loop", "else", "record", "declare"],
    tab_width: 3,
    ..BASE
};

pub static VB: Language = Language {
    name: "Visual Basic",
    aliases: &["vb", "vbnet", "vbscript", "vba", "basic"],
    extensions: &["vb", "vbs", "bas", "frm", "vba", "vbhtml"],
    line_comment: &["'", "rem "],
    quotes: "\"",
    keywords: &[
        "addhandler", "addressof", "alias", "and", "andalso", "as", "byref", "byval", "call", "case", "catch",
        "class", "const", "continue", "declare", "default", "delegate", "dim", "do", "each", "else", "elseif",
        "end", "enum", "erase", "error", "event", "exit", "finally", "for", "friend", "function", "get", "global",
        "gosub", "goto", "handles", "if", "implements", "imports", "in", "inherits", "interface", "is", "isnot",
        "let", "lib", "like", "loop", "me", "mod", "module", "mustinherit", "mustoverride", "mybase", "namespace",
        "new", "next", "not", "of", "on", "option", "optional", "or", "orelse", "overloads", "overridable",
        "overrides", "paramarray", "private", "property", "protected", "public", "raiseevent", "readonly", "redim",
        "resume", "return", "select", "set", "shared", "static", "step", "stop", "structure", "sub", "then",
        "throw", "to", "try", "typeof", "until", "using", "wend", "when", "while", "with", "withevents", "xor",
    ],
    types: &["boolean", "byte", "char", "date", "decimal", "double", "integer", "long", "object", "short", "single", "string", "variant"],
    constants: &["true", "false", "nothing", "null", "empty"],
    builtins: &["msgbox", "inputbox", "console", "debug", "len", "mid", "left", "right", "cstr", "cint"],
    case_insensitive: true,
    indent_triggers: &["then", "else"],
    ..BASE
};

pub static COBOL: Language = Language {
    name: "COBOL",
    aliases: &["cobol", "cob"],
    extensions: &["cob", "cbl", "cpy", "cobol"],
    line_comment: &["*>"],
    quotes: "\"'",
    keywords: &[
        "identification", "division", "program-id", "environment", "data", "working-storage", "section",
        "procedure", "perform", "until", "varying", "move", "to", "display", "accept", "compute", "add", "subtract",
        "multiply", "divide", "giving", "if", "else", "end-if", "evaluate", "when", "end-evaluate", "stop", "run",
        "pic", "picture", "value", "call", "using", "file", "fd", "open", "close", "read", "write", "goback",
        "exit", "copy", "linkage", "file-control", "select", "assign", "input-output", "configuration", "times",
    ],
    constants: &["zero", "zeros", "zeroes", "space", "spaces", "high-value", "low-value", "true", "false"],
    case_insensitive: true,
    ident_extra: "-",
    ..BASE
};

pub static ASSEMBLY: Language = Language {
    name: "Assembly",
    aliases: &["asm", "assembly", "nasm", "gas"],
    extensions: &["asm", "s", "nasm", "a51", "inc", "z80", "6502"],
    line_comment: &[";", "//"],
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "mov", "movzx", "movsx", "lea", "add", "sub", "mul", "imul", "div", "idiv", "inc", "dec", "neg", "and", "or",
        "xor", "not", "shl", "shr", "sar", "rol", "ror", "cmp", "test", "jmp", "je", "jne", "jz", "jnz", "jg", "jge",
        "jl", "jle", "ja", "jae", "jb", "jbe", "call", "ret", "push", "pop", "nop", "int", "syscall", "hlt", "loop",
        "ldr", "str", "ldp", "stp", "b", "bl", "bx", "blr", "cbz", "cbnz", "beq", "bne", "li", "la", "lw", "sw",
        "addi", "jal", "jr", "section", "global", "extern", "db", "dw", "dd", "dq", "resb", "resw", "equ", "times",
        "bits", "org", "align",
    ],
    types: &["byte", "word", "dword", "qword", "ptr"],
    builtins: &[
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "r8", "r9", "r10", "r11", "r12", "r13", "r14",
        "r15", "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "ax", "bx", "cx", "dx", "al", "ah", "bl",
        "cl", "dl", "sp", "lr", "pc", "x0", "x1", "x2", "x3", "w0", "w1",
    ],
    case_insensitive: true,
    preproc_hash: true,
    ident_extra: ".",
    tab_width: 8,
    ..BASE
};

pub static VHDL: Language = Language {
    name: "VHDL",
    aliases: &["vhdl", "vhd"],
    extensions: &["vhd", "vhdl", "vho", "vht"],
    line_comment: &["--"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"",
    keywords: &[
        "abs", "access", "after", "alias", "all", "and", "architecture", "array", "assert", "attribute", "begin",
        "block", "body", "buffer", "bus", "case", "component", "configuration", "constant", "downto", "else",
        "elsif", "end", "entity", "exit", "file", "for", "function", "generate", "generic", "if", "in", "inout",
        "is", "library", "loop", "map", "mod", "nand", "next", "nor", "not", "null", "of", "on", "or", "others",
        "out", "package", "port", "procedure", "process", "range", "record", "report", "return", "select",
        "severity", "signal", "subtype", "then", "to", "type", "until", "use", "variable", "wait", "when", "while",
        "with", "xor", "rising_edge", "falling_edge",
    ],
    types: &["std_logic", "std_logic_vector", "integer", "natural", "positive", "boolean", "bit", "bit_vector", "signed", "unsigned", "real", "time", "string"],
    constants: &["true", "false"],
    case_insensitive: true,
    indent_triggers: &["is", "begin", "then", "loop", "generate"],
    ..BASE
};

pub static PROLOG: Language = Language {
    name: "Prolog",
    aliases: &["prolog"],
    extensions: &["pro", "prolog"],
    line_comment: &["%"],
    block_comment: Some(("/*", "*/")),
    keywords: &["is", "not", "fail", "true", "false", "halt", "assert", "asserta", "assertz", "retract", "findall", "bagof", "setof", "module", "use_module", "dynamic", "discontiguous"],
    builtins: &["write", "writeln", "nl", "format", "atom", "number", "length", "append", "member", "call"],
    caps_types: true,
    indent_triggers: &[":-", "-->"],
    ..BASE
};

pub static TCL: Language = Language {
    name: "Tcl",
    aliases: &["tcl", "tk"],
    extensions: &["tcl", "tk", "itcl", "tm", "exp"],
    shebangs: &["tclsh", "wish", "expect"],
    line_comment: &["#"],
    quotes: "\"",
    keywords: &[
        "proc", "set", "unset", "if", "elseif", "else", "for", "foreach", "while", "switch", "return", "break",
        "continue", "namespace", "variable", "global", "upvar", "uplevel", "expr", "eval", "catch", "try", "on",
        "finally", "package", "source", "list", "lappend", "lindex", "llength", "dict", "string", "puts", "gets",
        "open", "close", "incr", "append", "array", "regexp", "regsub", "after", "exec",
    ],
    dollar_vars: true,
    brace_dedent: true,
    indent_triggers: &["{"],
    ..BASE
};

pub static AWK: Language = Language {
    name: "Awk",
    aliases: &["awk", "gawk"],
    extensions: &["awk", "gawk", "mawk"],
    shebangs: &["awk", "gawk", "mawk", "nawk"],
    line_comment: &["#"],
    quotes: "\"",
    keywords: &["BEGIN", "END", "BEGINFILE", "ENDFILE", "if", "else", "while", "for", "do", "break", "continue", "next", "nextfile", "exit", "return", "delete", "in", "function", "getline", "print", "printf"],
    builtins: &["length", "substr", "index", "split", "sub", "gsub", "match", "sprintf", "tolower", "toupper", "system", "close", "srand", "rand", "int"],
    constants: &["NR", "NF", "FS", "OFS", "RS", "ORS", "FILENAME", "FNR", "SUBSEP", "RSTART", "RLENGTH", "ENVIRON", "ARGV", "ARGC"],
    dollar_vars: true,
    brace_dedent: true,
    indent_triggers: &["{"],
    ..BASE
};

pub static VIMSCRIPT: Language = Language {
    name: "Vim script",
    aliases: &["vim", "vimscript", "viml"],
    extensions: &["vim", "vimrc"],
    filenames: &[".vimrc", "_vimrc", ".gvimrc", "_gvimrc", ".exrc", "init.vim"],
    line_comment: &["\""],
    quotes: "'",
    keywords: &[
        "if", "else", "elseif", "endif", "for", "endfor", "while", "endwhile", "function", "endfunction", "return",
        "let", "unlet", "call", "set", "setlocal", "map", "nmap", "vmap", "imap", "noremap", "nnoremap", "vnoremap",
        "inoremap", "autocmd", "augroup", "command", "syntax", "highlight", "filetype", "source", "execute", "echo",
        "try", "catch", "finally", "endtry", "abort", "silent",
    ],
    constants: &["v:true", "v:false", "v:null"],
    ident_extra: ":#",
    indent_triggers: &[],
    tab_width: 2,
    ..BASE
};

pub static CMAKE: Language = Language {
    name: "CMake",
    aliases: &["cmake"],
    extensions: &["cmake"],
    filenames: &["cmakelists.txt", "cmakecache.txt"],
    line_comment: &["#"],
    block_comment: Some(("#[[", "]]")),
    quotes: "\"",
    keywords: &[
        "cmake_minimum_required", "project", "add_executable", "add_library", "target_link_libraries",
        "target_include_directories", "target_compile_options", "target_compile_definitions", "include_directories",
        "link_directories", "find_package", "find_library", "find_path", "set", "unset", "option", "if", "elseif",
        "else", "endif", "foreach", "endforeach", "while", "endwhile", "function", "endfunction", "macro",
        "endmacro", "include", "add_subdirectory", "install", "message", "list", "string", "file", "configure_file",
        "add_custom_command", "add_custom_target", "add_test", "enable_testing", "return",
    ],
    constants: &["ON", "OFF", "TRUE", "FALSE", "YES", "NO", "PUBLIC", "PRIVATE", "INTERFACE", "REQUIRED", "STATIC", "SHARED"],
    case_insensitive: true,
    dollar_vars: true,
    indent_triggers: &["("],
    ..BASE
};

pub static MESON: Language = Language {
    name: "Meson",
    aliases: &["meson"],
    filenames: &["meson.build", "meson_options.txt", "meson.options"],
    line_comment: &["#"],
    quotes: "'",
    multiline_strings: &["'''"],
    keywords: &["if", "elif", "else", "endif", "foreach", "endforeach", "and", "or", "not", "in", "break", "continue"],
    constants: &["true", "false"],
    builtins: &["project", "executable", "library", "shared_library", "static_library", "dependency", "subdir", "files", "include_directories", "configure_file", "install_headers", "test", "message", "find_program", "declare_dependency"],
    indent_triggers: &["(", "[", "{"],
    tab_width: 2,
    ..BASE
};

pub static LATEX: Language = Language {
    name: "LaTeX",
    aliases: &["latex", "tex", "context"],
    extensions: &["tex", "ltx", "sty", "cls", "dtx", "ins", "bbx", "cbx", "lbx", "mkiv", "mkii"],
    line_comment: &["%"],
    quotes: "",
    backslash_commands: true,
    tab_width: 2,
    ..BASE
};

pub static BIBTEX: Language = Language {
    name: "BibTeX",
    aliases: &["bibtex", "bib", "biblatex"],
    extensions: &["bib"],
    line_comment: &["%"],
    quotes: "\"",
    decorators: true,
    kv_keys: true,
    backslash_commands: true,
    tab_width: 2,
    ..BASE
};

pub static ROFF: Language = Language {
    name: "Roff / man page",
    aliases: &["roff", "man", "troff", "groff", "nroff"],
    extensions: &["man", "roff", "mdoc", "1", "2", "3", "4", "5", "6", "7", "8", "9", "1m", "3p", "ms"],
    line_comment: &[".\\\"", "\\\"", "'\\\""],
    quotes: "",
    backslash_commands: true,
    sections: false,
    ..BASE
};

pub static WAT: Language = Language {
    name: "WebAssembly text",
    aliases: &["wat", "wast", "wasm"],
    extensions: &["wat", "wast"],
    line_comment: &[";;"],
    block_comment: Some(("(;", ";)")),
    quotes: "\"",
    keywords: &["module", "func", "param", "result", "local", "global", "memory", "table", "elem", "data", "type", "import", "export", "start", "block", "loop", "if", "then", "else", "end", "br", "br_if", "br_table", "return", "call", "call_indirect", "drop", "select", "mut"],
    types: &["i32", "i64", "f32", "f64", "v128", "funcref", "externref"],
    dollar_vars: true,
    ident_extra: ".",
    tab_width: 2,
    ..BASE
};

pub static PO: Language = Language {
    name: "Gettext PO",
    aliases: &["po", "pot", "gettext"],
    extensions: &["po", "pot"],
    line_comment: &["#"],
    quotes: "\"",
    keywords: &["msgid", "msgstr", "msgctxt", "msgid_plural"],
    ident_extra: "_",
    ..BASE
};

pub static SUBTITLES: Language = Language {
    name: "Subtitles (SRT/VTT)",
    aliases: &["srt", "vtt", "subtitles", "webvtt"],
    extensions: &["srt", "vtt", "sbv", "sub"],
    quotes: "",
    keywords: &["WEBVTT", "NOTE", "STYLE", "REGION"],
    ..BASE
};

pub static GNUPLOT: Language = Language {
    name: "Gnuplot",
    aliases: &["gnuplot", "gp"],
    extensions: &["gp", "gnuplot", "plt", "gpi"],
    line_comment: &["#"],
    keywords: &["plot", "splot", "replot", "set", "unset", "show", "reset", "using", "with", "title", "notitle", "lines", "points", "linespoints", "for", "do", "if", "else", "while", "load", "call", "fit", "via", "print", "pause"],
    builtins: &["sin", "cos", "tan", "exp", "log", "sqrt", "abs", "floor", "ceil"],
    ..BASE
};

pub static MERMAID: Language = Language {
    name: "Mermaid",
    aliases: &["mermaid", "mmd"],
    extensions: &["mmd", "mermaid"],
    line_comment: &["%%"],
    quotes: "\"",
    keywords: &["graph", "flowchart", "sequenceDiagram", "classDiagram", "stateDiagram", "stateDiagram-v2", "erDiagram", "gantt", "pie", "journey", "gitGraph", "mindmap", "timeline", "subgraph", "end", "participant", "actor", "loop", "alt", "else", "opt", "note", "class", "section", "title", "TD", "TB", "LR", "RL", "BT"],
    ident_extra: "-",
    tab_width: 2,
    ..BASE
};

pub static PLANTUML: Language = Language {
    name: "PlantUML",
    aliases: &["plantuml", "puml"],
    extensions: &["puml", "plantuml", "pu", "iuml"],
    line_comment: &["'"],
    block_comment: Some(("/'", "'/")),
    quotes: "\"",
    keywords: &["@startuml", "@enduml", "class", "interface", "abstract", "enum", "actor", "participant", "usecase", "package", "namespace", "note", "end", "as", "skinparam", "title", "left", "right", "of", "over", "activate", "deactivate", "alt", "else", "loop", "group", "start", "stop", "if", "then", "endif", "while", "endwhile"],
    decorators: true,
    tab_width: 2,
    ..BASE
};

pub static AUTOHOTKEY: Language = Language {
    name: "AutoHotkey",
    aliases: &["autohotkey", "ahk"],
    extensions: &["ahk", "ahk2"],
    line_comment: &[";"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"",
    keywords: &["if", "else", "loop", "while", "for", "in", "return", "break", "continue", "goto", "gosub", "global", "local", "static", "class", "extends", "new", "try", "catch", "finally", "throw", "send", "sleep", "msgbox", "run", "hotkey", "exitapp"],
    constants: &["true", "false", "A_ScriptDir", "A_Now", "A_Index"],
    case_insensitive: true,
    preproc_hash: true,
    brace_dedent: true,
    indent_triggers: &["{"],
    ..BASE
};

pub static RAKU: Language = Language {
    name: "Raku",
    aliases: &["raku", "perl6"],
    extensions: &["raku", "rakumod", "rakutest", "p6", "pm6", "pl6"],
    shebangs: &["raku", "perl6"],
    line_comment: &["#"],
    keywords: &["my", "our", "has", "sub", "method", "class", "role", "grammar", "token", "rule", "regex", "module", "use", "if", "elsif", "else", "unless", "for", "while", "until", "loop", "given", "when", "default", "return", "say", "print", "is", "does", "multi", "proto", "unit", "constant"],
    constants: &["True", "False", "Nil", "self"],
    dollar_vars: true,
    ident_extra: "-",
    brace_dedent: true,
    indent_triggers: &["{", "("],
    ..BASE
};

pub static JUST: Language = Language {
    name: "Justfile",
    aliases: &["just", "justfile"],
    filenames: &["justfile", ".justfile"],
    extensions: &["just"],
    line_comment: &["#"],
    keywords: &["set", "alias", "export", "import", "mod", "if", "else"],
    constants: &["true", "false"],
    dollar_vars: true,
    kv_keys: true,
    ident_extra: "-",
    ..BASE
};

pub static CONFIG: Language = Language {
    name: "Server config",
    aliases: &["nginx", "apache", "caddy", "htaccess", "config"],
    extensions: &["nginx", "vhost", "htaccess"],
    filenames: &[".htaccess", "httpd.conf", "apache2.conf", "nginx.conf", "caddyfile", "procfile", "haproxy.cfg", "squid.conf", "sshd_config", "ssh_config", "hosts", "fstab", "crontab", "sudoers"],
    line_comment: &["#"],
    quotes: "\"'",
    keywords: &[
        "server", "location", "listen", "server_name", "root", "index", "proxy_pass", "return", "rewrite", "include",
        "upstream", "http", "events", "if", "set", "try_files", "VirtualHost", "Directory", "DocumentRoot",
        "ServerName", "RewriteEngine", "RewriteRule", "RewriteCond", "Options", "AllowOverride", "Require",
        "Host", "HostName", "User", "Port", "IdentityFile", "Match",
    ],
    constants: &["on", "off", "yes", "no", "true", "false"],
    dollar_vars: true,
    kv_keys: false,
    brace_dedent: true,
    indent_triggers: &["{"],
    ..BASE
};

pub static IGNORE: Language = Language {
    name: "Ignore / attributes list",
    aliases: &["gitignore", "ignore", "gitattributes", "codeowners"],
    extensions: &["gitignore", "dockerignore", "npmignore", "eslintignore", "prettierignore", "hgignore", "gitattributes"],
    filenames: &[".gitignore", ".dockerignore", ".npmignore", ".eslintignore", ".prettierignore", ".hgignore", ".gitattributes", "codeowners", ".gitkeep", ".ignore", ".rgignore", ".stignore"],
    line_comment: &["#"],
    quotes: "",
    ..BASE
};

pub static GIT_COMMIT: Language = Language {
    name: "Git commit message",
    aliases: &["gitcommit", "commit"],
    filenames: &["commit_editmsg", "merge_msg", "tag_editmsg", "git-rebase-todo", "squash_msg"],
    line_comment: &["#"],
    quotes: "",
    keywords: &["pick", "reword", "edit", "squash", "fixup", "exec", "drop", "label", "reset", "merge", "break"],
    mode: Mode::Generic,
    ..BASE
};

pub static HTTP: Language = Language {
    name: "HTTP requests",
    aliases: &["http", "rest"],
    extensions: &["http", "rest"],
    line_comment: &["#", "//"],
    quotes: "\"",
    keywords: &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "HTTP/1.1", "HTTP/2"],
    kv_keys: true,
    decorators: true,
    ident_extra: "-",
    ..BASE
};

pub static MATLAB: Language = Language {
    name: "MATLAB / Octave",
    aliases: &["matlab", "octave"],
    extensions: &["octave", "octaverc"],
    filenames: &[".octaverc"],
    shebangs: &["octave"],
    line_comment: &["%", "#"],
    block_comment: Some(("%{", "%}")),
    quotes: "\"",
    keywords: &["function", "end", "endfunction", "if", "elseif", "else", "endif", "for", "endfor", "while", "endwhile", "do", "until", "switch", "case", "otherwise", "break", "continue", "return", "try", "catch", "global", "persistent", "classdef", "properties", "methods", "events"],
    constants: &["true", "false", "pi", "eps", "Inf", "NaN", "inf", "nan"],
    builtins: &["disp", "printf", "fprintf", "zeros", "ones", "size", "length", "numel", "plot", "figure", "hold", "xlabel", "ylabel", "sum", "max", "min"],
    indent_triggers: &[],
    ..BASE
};

/// All languages, in the order shown by `/lang`.
pub static LANGUAGES: &[&Language] = &[
    &PLAIN, &PYTHON, &HTML, &CSS, &JAVASCRIPT, &TYPESCRIPT, &JSON, &MARKDOWN, &XML, &SCSS, &RUST, &C, &CPP,
    &CSHARP, &JAVA, &KOTLIN, &GO, &SWIFT, &PHP, &RUBY, &SHELL, &POWERSHELL, &BATCH, &LUA, &SQL, &YAML, &TOML,
    &INI, &MAKEFILE, &DOCKERFILE, &PERL, &R, &HASKELL, &LISP, &DIFF, &CSV,
    // Additional languages (alphabetical).
    &ADA, &ASSEMBLY, &AUTOHOTKEY, &AWK, &BIBTEX, &BICEP, &CMAKE, &COBOL, &CRYSTAL, &D, &DART, &ELIXIR, &ELM,
    &ERLANG, &FSHARP, &FORTRAN, &GIT_COMMIT, &GLEAM, &GNUPLOT, &GRAPHQL, &GRAPHVIZ, &GROOVY, &HAXE, &HCL, &HTTP,
    &IGNORE, &JULIA, &JUST, &LATEX, &MATLAB, &MERMAID, &MESON, &NIM, &NIX, &OBJC, &OCAML, &PASCAL, &PLANTUML,
    &PO, &PROLOG, &PROTOBUF, &RAKU, &ROFF, &SCALA, &CONFIG, &SHADER, &SOLIDITY, &SUBTITLES, &TCL, &VALA,
    &VERILOG, &VHDL, &VB, &VIMSCRIPT, &WAT, &ZIG, &HEX,
];

pub fn by_name(name: &str) -> Option<&'static Language> {
    let n = name.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .copied()
        .find(|l| l.name.eq_ignore_ascii_case(&n) || l.aliases.contains(&n.as_str()))
}

/// Picks a language from a file name and (optionally) its first line.
pub fn detect(file_name: &str, first_line: &str) -> &'static Language {
    let lower = file_name.to_ascii_lowercase();
    let base = Path::new(&lower).file_name().and_then(|s| s.to_str()).unwrap_or(&lower).to_owned();
    if let Some(l) = LANGUAGES.iter().find(|l| l.filenames.contains(&base.as_str())) {
        return l;
    }
    // Dockerfile.dev, Makefile.am etc.
    for prefix in ["dockerfile", "containerfile", "makefile"] {
        if base.starts_with(prefix) {
            return by_name(prefix).unwrap_or(&PLAIN);
        }
    }
    let ext = Path::new(&base).extension().and_then(|e| e.to_str()).unwrap_or("");
    let ext = if ext.is_empty() { base.trim_start_matches('.') } else { ext };
    // `.m` is shared by Objective-C and MATLAB/Octave: MATLAB files usually
    // start with a `%` comment or a function definition.
    if ext == "m" {
        let fl = first_line.trim_start();
        if fl.starts_with('%') || fl.starts_with("function") || fl.starts_with("clear") || fl.starts_with("clc") {
            return &MATLAB;
        }
    }
    if let Some(l) = LANGUAGES.iter().find(|l| l.extensions.contains(&ext)) {
        return l;
    }
    if let Some(rest) = first_line.strip_prefix("#!") {
        let interp = rest
            .split_whitespace()
            .find(|w| !w.ends_with("/env") && !w.starts_with('-'))
            .map(|w| w.rsplit('/').next().unwrap_or(w))
            .unwrap_or("");
        let interp = interp.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        if let Some(l) = LANGUAGES.iter().find(|l| l.shebangs.contains(&interp)) {
            return l;
        }
    }
    let fl = first_line.trim_start().to_ascii_lowercase();
    if fl.starts_with("<!doctype html") || fl.starts_with("<html") {
        return &HTML;
    }
    if fl.starts_with("<?xml") {
        return &XML;
    }
    if fl.starts_with("<?php") {
        return &PHP;
    }
    if fl.starts_with("diff --git") || fl.starts_with("--- ") {
        return &DIFF;
    }
    &PLAIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection() {
        assert_eq!(detect("main.py", "").name, "Python");
        assert_eq!(detect("INDEX.HTML", "").name, "HTML");
        assert_eq!(detect("style.css", "").name, "CSS");
        assert_eq!(detect("notes.txt", "").name, "Plain Text");
        assert_eq!(detect("Makefile", "").name, "Makefile");
        assert_eq!(detect("script", "#!/usr/bin/env python3").name, "Python");
        assert_eq!(detect("run", "#!/bin/bash -e").name, "Shell");
        assert_eq!(detect(".bashrc", "").name, "Shell");
        assert_eq!(detect("Dockerfile.prod", "").name, "Dockerfile");
        assert_eq!(detect("page", "<!DOCTYPE html>").name, "HTML");
        assert_eq!(detect("main.zig", "").name, "Zig");
        assert_eq!(detect("CMakeLists.txt", "").name, "CMake");
        assert_eq!(detect("paper.tex", "").name, "LaTeX");
        assert_eq!(detect("main.tf", "").name, "HCL / Terraform");
        assert_eq!(detect(".gitignore", "").name, "Ignore / attributes list");
        assert_eq!(detect("COMMIT_EDITMSG", "").name, "Git commit message");
        assert_eq!(detect("App.svelte", "").name, "HTML");
        assert_eq!(detect("solve.m", "% solve the system").name, "MATLAB / Octave");
        assert_eq!(detect("View.m", "#import <Foundation/Foundation.h>").name, "Objective-C");
        assert_eq!(detect("Jenkinsfile", "").name, "Groovy");
        assert_eq!(detect("go.mod", "").name, "Plain Text");
        assert_eq!(detect("ls.1", "").name, "Roff / man page");
    }

    #[test]
    fn extensions_are_unambiguous() {
        let mut seen = std::collections::HashMap::new();
        for l in LANGUAGES {
            for e in l.extensions {
                if let Some(prev) = seen.insert(*e, l.name) {
                    // Duplicates within one language are harmless.
                    assert_eq!(prev, l.name, "extension .{e} claimed by both {prev} and {}", l.name);
                }
            }
        }
        assert!(LANGUAGES.len() >= 90, "only {} languages", LANGUAGES.len());
    }

    #[test]
    fn indent_rules() {
        assert!(PYTHON.indent_after("def f(x):"));
        assert!(PYTHON.indent_after("if x:  # comment"));
        assert!(!PYTHON.indent_after("x = 1"));
        assert!(HTML.indent_after("<div class=\"a\">"));
        assert!(!HTML.indent_after("<br>"));
        assert!(!HTML.indent_after("<p>hi</p>"));
        assert!(CSS.indent_after("body {"));
    }
}
