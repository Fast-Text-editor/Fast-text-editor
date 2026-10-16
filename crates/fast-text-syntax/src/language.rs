use crate::token::TokenKind;

/// A language definition driving the generic highlighter.
#[derive(Debug, Clone, Copy)]
pub struct Language {
    pub name: &'static str,
    /// The LSP `languageId` sent in `textDocument/didOpen`.
    pub language_id: &'static str,
    pub extensions: &'static [&'static str],
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub constants: &'static [&'static str],
    /// Line-comment markers, e.g. `//` or `#`.
    pub line_comment: &'static [&'static str],
    /// Block comment `(open, close)`.
    pub block_comment: Option<(&'static str, &'static str)>,
    /// Single-line string delimiters, e.g. `"` or `'`.
    pub string_delims: &'static [&'static str],
    /// Multi-line string delimiters, e.g. `"""` (checked before single-line).
    pub triple_strings: &'static [&'static str],
    /// Prefix marker for attributes/decorators, e.g. `#` in Rust or `@` in Python.
    pub attribute_prefix: Option<&'static str>,
    /// Whether the language supports escape sequences (`\n`) in strings.
    pub string_escapes: bool,
    /// Preferred language servers by name (resolved via the server table).
    pub lsp_servers: &'static [&'static str],
}

impl Language {
    /// True when this language claims the given file extension.
    pub fn has_ext(&self, ext: &str) -> bool {
        self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}

/// Identifies a language by a cheap, `Copy` reference to its definition.
///
/// Equality and hashing compare the identity of the definition, so a
/// `LanguageId` is stable and cheap to use as a map key.
#[derive(Clone, Copy)]
pub struct LanguageId(&'static Language);

impl PartialEq for LanguageId {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for LanguageId {}

impl std::hash::Hash for LanguageId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(self.0, state);
    }
}

impl std::fmt::Debug for LanguageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LanguageId({})", self.0.name)
    }
}

#[allow(non_upper_case_globals)]
impl LanguageId {
    // The hand-curated languages (kept for richer highlighting).
    pub const Plain: LanguageId = LanguageId(&PLAIN);
    pub const Rust: LanguageId = LanguageId(&RUST);
    pub const JavaScript: LanguageId = LanguageId(&JAVASCRIPT);
    pub const TypeScript: LanguageId = LanguageId(&TYPESCRIPT);
    pub const Python: LanguageId = LanguageId(&PYTHON);
    pub const Ruby: LanguageId = LanguageId(&RUBY);
    pub const Java: LanguageId = LanguageId(&JAVA);
    pub const CSharp: LanguageId = LanguageId(&CSHARP);
    pub const Json: LanguageId = LanguageId(&JSON);
    pub const Toml: LanguageId = LanguageId(&TOML);
    pub const Markdown: LanguageId = LanguageId(&MARKDOWN);
    pub const Shell: LanguageId = LanguageId(&SHELL);
    pub const C: LanguageId = LanguageId(&C);
    pub const Cpp: LanguageId = LanguageId(&CPP);
    pub const Go: LanguageId = LanguageId(&GO);
    pub const Html: LanguageId = LanguageId(&HTML);
    pub const Css: LanguageId = LanguageId(&CSS);
    pub const TextWrite: LanguageId = LanguageId(&TEXTWRITE);

    /// Detects a language from a file name / path.
    pub fn from_path(path: &str) -> Self {
        let base = path.rsplit(['/', '\\']).next().unwrap_or(path);

        // A few common extension-less files are recognized by name.
        match base {
            "Rakefile" | "Gemfile" | "Guardfile" | "Capfile" | "Vagrantfile" => return Self::Ruby,
            "Cargo.lock" => return Self::Toml,
            _ => {}
        }

        let ext = path.rsplit('.').next().filter(|e| *e != path).unwrap_or("");
        if !ext.is_empty() {
            let id = Self::from_extension(ext);
            if id != Self::Plain {
                return id;
            }
        }

        // Fall back to basename/glob matching for extension-less files.
        match crate::generated::language_for_glob(base) {
            Some(lang) => LanguageId(lang),
            None => Self::Plain,
        }
    }

    /// Detects a language from a file extension (without the dot).
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Self::Rust,
            "js" | "mjs" | "cjs" | "jsx" => Self::JavaScript,
            "ts" | "tsx" | "mts" | "cts" => Self::TypeScript,
            "py" | "pyi" | "pyw" => Self::Python,
            "rb" | "rbw" | "rake" | "gemspec" | "ru" => Self::Ruby,
            "java" => Self::Java,
            "cs" | "csx" => Self::CSharp,
            "json" | "jsonc" => Self::Json,
            "toml" => Self::Toml,
            "md" | "markdown" | "mdown" => Self::Markdown,
            "sh" | "bash" | "zsh" | "fish" => Self::Shell,
            // C headers (.h) plus C sources.
            "c" | "h" => Self::C,
            // C++ sources and headers (including the common header spellings).
            "cpp" | "cxx" | "cc" | "c++" | "hpp" | "hxx" | "hh" | "h++" | "inl" | "ipp" | "tpp"
            | "tcc" => Self::Cpp,
            "go" => Self::Go,
            "html" | "htm" => Self::Html,
            "css" => Self::Css,
            "tw" => Self::TextWrite,
            other => match crate::generated::language_for_extension(other) {
                Some(lang) => LanguageId(lang),
                None => Self::Plain,
            },
        }
    }

    /// Detects a language from a human-readable name (e.g. `"rust"`, `"Python"`).
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "rust" | "rs" => Self::Rust,
            "javascript" | "js" => Self::JavaScript,
            "typescript" | "ts" => Self::TypeScript,
            "python" | "py" => Self::Python,
            "ruby" | "rb" => Self::Ruby,
            "java" => Self::Java,
            "c#" | "csharp" | "cs" => Self::CSharp,
            "json" => Self::Json,
            "toml" => Self::Toml,
            "markdown" | "md" => Self::Markdown,
            "shell" | "sh" | "bash" => Self::Shell,
            "c" => Self::C,
            "cpp" | "c++" | "cplusplus" => Self::Cpp,
            "go" => Self::Go,
            "html" => Self::Html,
            "css" => Self::Css,
            "textwrite" | "tw" => Self::TextWrite,
            "plain" | "text" | "plaintext" => Self::Plain,
            other => match crate::generated::language_for_name(other) {
                Some(lang) => LanguageId(lang),
                None => Self::from_extension(other),
            },
        }
    }

    /// The full definition for this language.
    pub fn def(self) -> &'static Language {
        self.0
    }

    /// The definition, by value (it is `Copy`).
    pub fn language(self) -> Language {
        *self.0
    }

    /// A human-readable name.
    pub fn name(self) -> &'static str {
        self.0.name
    }

    /// The LSP `languageId` string for this language, if it has a server.
    pub fn lsp_id(self) -> Option<&'static str> {
        if self == Self::Plain {
            None
        } else {
            Some(self.0.language_id)
        }
    }

    /// The preferred language servers for this language, by name.
    pub fn lsp_servers(self) -> &'static [&'static str] {
        self.0.lsp_servers
    }
}

/// The hand-curated language set, in a stable order.
pub static CURATED_LANGUAGES: &[LanguageId] = &[
    LanguageId::Plain,
    LanguageId::Rust,
    LanguageId::JavaScript,
    LanguageId::TypeScript,
    LanguageId::Python,
    LanguageId::Ruby,
    LanguageId::Java,
    LanguageId::CSharp,
    LanguageId::Json,
    LanguageId::Toml,
    LanguageId::Markdown,
    LanguageId::Shell,
    LanguageId::C,
    LanguageId::Cpp,
    LanguageId::Go,
    LanguageId::Html,
    LanguageId::Css,
    LanguageId::TextWrite,
];

/// Every selectable language name (curated plus imported), sorted and unique.
pub fn all_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = CURATED_LANGUAGES.iter().map(|l| l.name()).collect();
    names.extend(crate::generated::LANGUAGES.iter().map(|l| l.name));
    names.sort_unstable();
    names.dedup();
    names
}

static PLAIN: Language = Language {
    name: "Plain Text",
    language_id: "plaintext",
    extensions: &["txt", "log", ""],
    keywords: &[],
    types: &[],
    constants: &[],
    line_comment: &[],
    block_comment: None,
    string_delims: &[],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: false,
    lsp_servers: &[],
};

static TEXTWRITE: Language = Language {
    name: "TextWrite",
    language_id: "textwrite",
    extensions: &["tw"],
    keywords: &[
        "let", "set", "fn", "return", "if", "else", "while", "for", "in", "repeat", "times",
        "break", "continue", "and", "or", "not", "is", "import", "as",
    ],
    types: &[
        "int", "float", "string", "bool", "list", "map", "function", "none",
    ],
    constants: &["true", "false", "none"],
    // TextWrite accepts `#`, `//`, and `/* ... */` comments; the lexer treats
    // all three as comments, so highlight them the same way.
    line_comment: &["#", "//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'"],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &[],
};

static RUST: Language = Language {
    name: "Rust",
    language_id: "rust",
    extensions: &["rs"],
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while", "union",
    ],
    types: &[
        "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "String",
        "u8", "u16", "u32", "u64", "u128", "usize", "Vec", "Option", "Result", "Box", "Rc", "Arc",
    ],
    constants: &["true", "false", "None", "Some", "Ok", "Err"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\""],
    triple_strings: &[],
    attribute_prefix: Some("#"),
    string_escapes: true,
    lsp_servers: &["rust-analyzer"],
};

static JAVASCRIPT: Language = Language {
    name: "JavaScript",
    language_id: "javascript",
    extensions: &["js", "mjs", "cjs", "jsx"],
    keywords: &[
        "as",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "export",
        "extends",
        "finally",
        "for",
        "from",
        "function",
        "get",
        "if",
        "import",
        "in",
        "instanceof",
        "let",
        "new",
        "of",
        "return",
        "set",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
    ],
    types: &[
        "Array", "Boolean", "Date", "Map", "Number", "Object", "Promise", "Set", "String", "Symbol",
    ],
    constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'", "`"],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &["typescript-language-server"],
};

static TYPESCRIPT: Language = Language {
    name: "TypeScript",
    language_id: "typescript",
    extensions: &["ts", "tsx", "mts", "cts"],
    keywords: &[
        "as",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "declare",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "finally",
        "for",
        "from",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "keyof",
        "let",
        "namespace",
        "new",
        "of",
        "readonly",
        "return",
        "satisfies",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "try",
        "type",
        "typeof",
        "var",
        "void",
        "while",
        "yield",
    ],
    types: &[
        "any", "bigint", "boolean", "never", "number", "object", "string", "symbol", "unknown",
        "void", "Array", "Map", "Promise", "Record", "Set",
    ],
    constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'", "`"],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &["typescript-language-server"],
};

static PYTHON: Language = Language {
    name: "Python",
    language_id: "python",
    extensions: &["py", "pyi", "pyw"],
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield", "match", "case",
    ],
    types: &[
        "bool",
        "bytes",
        "dict",
        "float",
        "frozenset",
        "int",
        "list",
        "set",
        "str",
        "tuple",
    ],
    constants: &[
        "True",
        "False",
        "None",
        "self",
        "cls",
        "NotImplemented",
        "Ellipsis",
    ],
    line_comment: &["#"],
    block_comment: None,
    string_delims: &["\"", "'"],
    triple_strings: &["\"\"\"", "'''"],
    attribute_prefix: Some("@"),
    string_escapes: true,
    lsp_servers: &["pylsp", "jedi"],
};

static RUBY: Language = Language {
    name: "Ruby",
    language_id: "ruby",
    extensions: &["rb", "rbw", "rake", "gemspec", "ru"],
    keywords: &[
        "BEGIN", "END", "alias", "and", "begin", "break", "case", "class", "def", "do", "else",
        "elsif", "end", "ensure", "for", "if", "in", "module", "next", "not", "or", "redo",
        "rescue", "retry", "return", "self", "super", "then", "undef", "unless", "until", "when",
        "while", "yield",
    ],
    types: &[
        "Array",
        "Class",
        "Comparable",
        "Enumerable",
        "Float",
        "Hash",
        "Integer",
        "Lambda",
        "Module",
        "Object",
        "Proc",
        "Range",
        "Regexp",
        "String",
        "Struct",
        "Symbol",
        "Time",
    ],
    constants: &["true", "false", "nil", "__FILE__", "__LINE__", "__dir__"],
    line_comment: &["#"],
    block_comment: None,
    string_delims: &["\"", "'"],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &["ruby-lsp", "solargraph"],
};

static JAVA: Language = Language {
    name: "Java",
    language_id: "java",
    extensions: &["java"],
    keywords: &[
        "abstract",
        "assert",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "else",
        "enum",
        "extends",
        "final",
        "finally",
        "for",
        "goto",
        "if",
        "implements",
        "import",
        "instanceof",
        "interface",
        "native",
        "new",
        "package",
        "permits",
        "private",
        "protected",
        "public",
        "record",
        "return",
        "sealed",
        "static",
        "strictfp",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "var",
        "volatile",
        "while",
        "yield",
    ],
    types: &[
        "boolean",
        "byte",
        "char",
        "double",
        "float",
        "int",
        "long",
        "short",
        "void",
        "String",
        "Integer",
        "Long",
        "Double",
        "Float",
        "Boolean",
        "Character",
        "Byte",
        "Short",
        "Object",
        "List",
        "Map",
        "Set",
        "ArrayList",
        "HashMap",
        "HashSet",
        "Optional",
        "Stream",
        "Class",
    ],
    constants: &["true", "false", "null"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'"],
    triple_strings: &["\"\"\""],
    attribute_prefix: Some("@"),
    string_escapes: true,
    lsp_servers: &["jdtls"],
};

static CSHARP: Language = Language {
    name: "C#",
    language_id: "csharp",
    extensions: &["cs", "csx"],
    keywords: &[
        "abstract",
        "as",
        "async",
        "await",
        "base",
        "break",
        "case",
        "catch",
        "checked",
        "class",
        "const",
        "continue",
        "default",
        "delegate",
        "do",
        "else",
        "enum",
        "event",
        "explicit",
        "extern",
        "finally",
        "fixed",
        "for",
        "foreach",
        "get",
        "goto",
        "if",
        "implicit",
        "in",
        "init",
        "interface",
        "internal",
        "is",
        "lock",
        "namespace",
        "new",
        "operator",
        "out",
        "override",
        "params",
        "partial",
        "private",
        "protected",
        "public",
        "readonly",
        "record",
        "ref",
        "return",
        "sealed",
        "set",
        "sizeof",
        "stackalloc",
        "static",
        "struct",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "unchecked",
        "unsafe",
        "using",
        "var",
        "virtual",
        "volatile",
        "when",
        "where",
        "while",
        "with",
        "yield",
    ],
    types: &[
        "bool",
        "byte",
        "char",
        "decimal",
        "double",
        "float",
        "int",
        "long",
        "object",
        "sbyte",
        "short",
        "string",
        "uint",
        "ulong",
        "ushort",
        "void",
        "dynamic",
        "nint",
        "nuint",
        "String",
        "Int32",
        "Int64",
        "Boolean",
        "Double",
        "List",
        "Dictionary",
        "Task",
        "IEnumerable",
    ],
    constants: &["true", "false", "null", "nameof"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'"],
    triple_strings: &["\"\"\""],
    attribute_prefix: Some("#"),
    string_escapes: true,
    lsp_servers: &["csharp-ls", "omnisharp"],
};

static JSON: Language = Language {
    name: "JSON",
    language_id: "json",
    extensions: &["json", "jsonc"],
    keywords: &[],
    types: &[],
    constants: &["true", "false", "null"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\""],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &["vscode-json-language-server"],
};

static TOML: Language = Language {
    name: "TOML",
    language_id: "toml",
    extensions: &["toml"],
    keywords: &[],
    types: &[],
    constants: &["true", "false"],
    line_comment: &["#"],
    block_comment: None,
    string_delims: &["\"", "'"],
    triple_strings: &["\"\"\"", "'''"],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &["taplo"],
};

static MARKDOWN: Language = Language {
    name: "Markdown",
    language_id: "markdown",
    extensions: &["md", "markdown", "mdown"],
    keywords: &[],
    types: &[],
    constants: &[],
    line_comment: &[],
    block_comment: None,
    string_delims: &["`"],
    triple_strings: &["```"],
    attribute_prefix: Some("#"),
    string_escapes: false,
    lsp_servers: &["marksman"],
};

static SHELL: Language = Language {
    name: "Shell",
    language_id: "shellscript",
    extensions: &["sh", "bash", "zsh", "fish"],
    keywords: &[
        "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in", "then",
        "until", "while", "select", "time", "coproc", "local", "return", "export", "source",
    ],
    types: &[],
    constants: &["true", "false"],
    line_comment: &["#"],
    block_comment: None,
    string_delims: &["\"", "'"],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &["bash-language-server"],
};

static C: Language = Language {
    name: "C",
    language_id: "c",
    extensions: &["c", "h"],
    keywords: &[
        "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern",
        "for", "goto", "if", "inline", "register", "restrict", "return", "sizeof", "static",
        "struct", "switch", "typedef", "union", "volatile", "while",
    ],
    types: &[
        "char", "double", "float", "int", "long", "short", "signed", "unsigned", "void", "size_t",
        "ssize_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t",
        "int64_t", "bool", "FILE",
    ],
    constants: &["NULL", "true", "false"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'"],
    triple_strings: &[],
    attribute_prefix: Some("#"),
    string_escapes: true,
    lsp_servers: &["clangd"],
};

static CPP: Language = Language {
    name: "C++",
    language_id: "cpp",
    extensions: &[
        "cpp", "cxx", "cc", "c++", "hpp", "hxx", "hh", "h++", "inl", "ipp", "tpp", "tcc",
    ],
    keywords: &[
        "alignas",
        "alignof",
        "auto",
        "break",
        "case",
        "catch",
        "class",
        "concept",
        "const",
        "consteval",
        "constexpr",
        "continue",
        "decltype",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "explicit",
        "export",
        "extern",
        "false",
        "for",
        "friend",
        "goto",
        "if",
        "inline",
        "mutable",
        "namespace",
        "new",
        "noexcept",
        "operator",
        "override",
        "private",
        "protected",
        "public",
        "return",
        "sizeof",
        "static",
        "struct",
        "switch",
        "template",
        "this",
        "throw",
        "true",
        "try",
        "typedef",
        "typename",
        "union",
        "using",
        "virtual",
        "volatile",
        "while",
    ],
    types: &[
        "bool",
        "char",
        "double",
        "float",
        "int",
        "long",
        "short",
        "signed",
        "unsigned",
        "void",
        "size_t",
        "std",
        "string",
        "vector",
        "map",
        "set",
        "unique_ptr",
        "shared_ptr",
    ],
    constants: &["nullptr", "NULL", "true", "false"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'"],
    triple_strings: &[],
    attribute_prefix: Some("#"),
    string_escapes: true,
    lsp_servers: &["clangd"],
};

static GO: Language = Language {
    name: "Go",
    language_id: "go",
    extensions: &["go"],
    keywords: &[
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "type",
        "var",
    ],
    types: &[
        "bool",
        "byte",
        "complex64",
        "complex128",
        "error",
        "float32",
        "float64",
        "int",
        "int8",
        "int16",
        "int32",
        "int64",
        "rune",
        "string",
        "uint",
        "uint8",
        "uint16",
        "uint32",
        "uint64",
        "uintptr",
        "any",
    ],
    constants: &["true", "false", "nil", "iota"],
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'", "`"],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: true,
    lsp_servers: &["gopls"],
};

static HTML: Language = Language {
    name: "HTML",
    language_id: "html",
    extensions: &["html", "htm"],
    keywords: &[],
    types: &[],
    constants: &[],
    line_comment: &[],
    block_comment: Some(("<!--", "-->")),
    string_delims: &["\"", "'"],
    triple_strings: &[],
    attribute_prefix: None,
    string_escapes: false,
    lsp_servers: &["vscode-html-language-server"],
};

static CSS: Language = Language {
    name: "CSS",
    language_id: "css",
    extensions: &["css"],
    keywords: &[
        "important",
        "media",
        "import",
        "keyframes",
        "supports",
        "font-face",
    ],
    types: &[],
    constants: &[],
    line_comment: &[],
    block_comment: Some(("/*", "*/")),
    string_delims: &["\"", "'"],
    triple_strings: &[],
    attribute_prefix: Some("@"),
    string_escapes: true,
    lsp_servers: &["vscode-css-language-server"],
};

impl Language {
    pub(crate) fn is_keyword(&self, word: &str) -> bool {
        self.keywords.contains(&word)
    }
    pub(crate) fn is_type(&self, word: &str) -> bool {
        self.types.contains(&word)
    }
    pub(crate) fn is_constant(&self, word: &str) -> bool {
        self.constants.contains(&word)
    }
}

/// The token kind a bare word should receive in this language.
pub(crate) fn classify_word(lang: &Language, word: &str) -> TokenKind {
    if lang.is_constant(word) {
        TokenKind::Constant
    } else if lang.is_keyword(word) {
        TokenKind::Keyword
    } else if lang.is_type(word) {
        TokenKind::Type
    } else {
        TokenKind::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_new_languages_by_extension() {
        assert_eq!(LanguageId::from_path("Main.java"), LanguageId::Java);
        assert_eq!(LanguageId::from_path("app.rb"), LanguageId::Ruby);
        assert_eq!(LanguageId::from_path("lib/thing.gemspec"), LanguageId::Ruby);
        assert_eq!(LanguageId::from_path("Program.cs"), LanguageId::CSharp);
        assert_eq!(LanguageId::from_path("Script.csx"), LanguageId::CSharp);
    }

    #[test]
    fn detects_c_and_cpp_headers() {
        // C headers stay C; C++ headers are recognized by all common spellings.
        assert_eq!(LanguageId::from_path("stdio.h"), LanguageId::C);
        assert_eq!(LanguageId::from_path("main.c"), LanguageId::C);
        assert_eq!(LanguageId::from_path("vector.hpp"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_path("vector.hxx"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_path("thing.hh"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_path("detail.h++"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_path("impl.ipp"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_path("impl.tpp"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_path("impl.tcc"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_path("widget.c++"), LanguageId::Cpp);
    }

    #[test]
    fn detects_extensionless_ruby_scripts() {
        assert_eq!(LanguageId::from_path("Rakefile"), LanguageId::Ruby);
        assert_eq!(LanguageId::from_path("Gemfile"), LanguageId::Ruby);
        assert_eq!(
            LanguageId::from_path("/home/me/proj/Rakefile"),
            LanguageId::Ruby
        );
    }

    #[test]
    fn detects_textwrite() {
        assert_eq!(LanguageId::from_path("script.tw"), LanguageId::TextWrite);
        assert_eq!(LanguageId::from_name("textwrite"), LanguageId::TextWrite);
        assert_eq!(LanguageId::from_name("tw"), LanguageId::TextWrite);
        assert_eq!(LanguageId::TextWrite.name(), "TextWrite");
        assert_eq!(LanguageId::TextWrite.def().extensions, &["tw"]);
        assert!(LanguageId::TextWrite.def().keywords.contains(&"repeat"));
        assert!(LanguageId::TextWrite.def().line_comment.contains(&"#"));
    }

    #[test]
    fn detects_by_name() {
        assert_eq!(LanguageId::from_name("Java"), LanguageId::Java);
        assert_eq!(LanguageId::from_name("ruby"), LanguageId::Ruby);
        assert_eq!(LanguageId::from_name("c#"), LanguageId::CSharp);
        assert_eq!(LanguageId::from_name("csharp"), LanguageId::CSharp);
        assert_eq!(LanguageId::from_name("cpp"), LanguageId::Cpp);
        assert_eq!(LanguageId::from_name("c++"), LanguageId::Cpp);
    }

    #[test]
    fn new_languages_have_names_and_lsp_ids() {
        assert_eq!(LanguageId::Java.name(), "Java");
        assert_eq!(LanguageId::Ruby.name(), "Ruby");
        assert_eq!(LanguageId::CSharp.name(), "C#");
        assert_eq!(LanguageId::Java.lsp_id(), Some("java"));
        assert_eq!(LanguageId::Ruby.lsp_id(), Some("ruby"));
        assert_eq!(LanguageId::CSharp.lsp_id(), Some("csharp"));
    }

    #[test]
    fn imported_registry_is_populated() {
        // The Helix-derived registry should carry hundreds of languages.
        assert!(
            crate::imported_languages().len() > 300,
            "expected >300 imported languages, got {}",
            crate::imported_languages().len()
        );
    }

    #[test]
    fn detects_imported_languages_by_extension() {
        // A spread of languages that only exist via the imported registry.
        for (file, name) in [
            ("main.zig", "Zig"),
            ("Main.kt", "Kotlin"),
            ("Main.kts", "Kotlin"),
            ("app.hs", "Haskell"),
            ("config.yaml", "YAML"),
            ("config.yml", "YAML"),
            ("page.vue", "Vue"),
            ("widget.svelte", "Svelte"),
            ("query.sql", "SQL"),
            ("init.lua", "Lua"),
            ("script.pl", "Perl"),
            ("build.gradle", "Groovy"),
            ("analysis.R", "R"),
            ("stats.jl", "Julia"),
            ("main.dart", "Dart"),
            ("app.ex", "Elixir"),
            ("core.clj", "Clojure"),
            ("settings.ini", "INI"),
            ("data.xml", "XML"),
            ("schema.graphql", "GraphQL"),
            ("Deploy.tf", "HCL"),
            ("notes.org", "Org"),
        ] {
            let id = LanguageId::from_path(file);
            assert_eq!(id.name(), name, "{file} should detect as {name}");
            assert_ne!(id, LanguageId::Plain, "{file} should not be plain text");
        }
    }

    #[test]
    fn imported_languages_carry_comment_and_keyword_data() {
        let zig = LanguageId::from_path("main.zig");
        assert!(zig.def().line_comment.contains(&"//"));
        assert!(
            zig.def().keywords.contains(&"const"),
            "zig keywords missing"
        );

        let haskell = LanguageId::from_path("app.hs");
        assert_eq!(haskell.def().line_comment, &["--"]);
        assert!(haskell.def().keywords.contains(&"where"));

        let sql = LanguageId::from_path("query.sql");
        assert_eq!(sql.def().line_comment, &["--"]);

        let lua = LanguageId::from_path("init.lua");
        assert!(lua.def().line_comment.contains(&"--"));
    }

    #[test]
    fn detects_imported_languages_by_name() {
        assert_eq!(LanguageId::from_name("zig").name(), "Zig");
        assert_eq!(LanguageId::from_name("Kotlin").name(), "Kotlin");
        assert_eq!(LanguageId::from_name("haskell").name(), "Haskell");
        assert_eq!(LanguageId::from_name("yaml").name(), "YAML");
        assert_eq!(LanguageId::from_name("lua").name(), "Lua");
    }

    #[test]
    fn imported_languages_resolve_lsp_servers() {
        // Server names resolve through the generated Helix server table.
        let zig = LanguageId::from_path("main.zig");
        assert_eq!(zig.lsp_servers(), &["zls"]);

        let haskell = LanguageId::from_path("app.hs");
        assert_eq!(haskell.lsp_servers(), &["haskell-language-server"]);

        assert_eq!(crate::language_server("gopls"), Some(("gopls", &[][..])));
        assert_eq!(crate::language_server("clangd"), Some(("clangd", &[][..])));
        assert_eq!(crate::language_server("definitely-not-a-server"), None);
    }

    #[test]
    fn detects_extensionless_files_by_glob() {
        assert_eq!(LanguageId::from_path("Makefile").name(), "Make");
        assert_eq!(LanguageId::from_path("Dockerfile").name(), "Dockerfile");
        assert_eq!(LanguageId::from_path("project/Cargo.lock").name(), "TOML");
    }
}
