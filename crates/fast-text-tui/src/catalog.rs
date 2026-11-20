//! The first-party plugin catalog.
//!
//! These TextWrite plugins are compiled into the editor, so the plugin manager
//! can list them under **Available** and install them for real — no network
//! connection and no separate download. Installing writes the source into the
//! user's plugin folder, where it is discovered like any other `.tw` file and
//! can be toggled, deleted or edited there.
//!
//! Keeping the sources as real `.tw` files (rather than Rust string literals)
//! means they are linted, run and readable exactly like the ones a user writes.

/// One plugin the editor can install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogPlugin {
    /// The file name written into the plugin folder.
    pub file_name: &'static str,
    /// A short title for the manager list.
    pub title: &'static str,
    /// One-line summary shown in the details pane.
    pub description: &'static str,
    /// The plugin's TextWrite source.
    pub source: &'static str,
}

impl CatalogPlugin {
    /// The first comment line of the source, used if `description` is empty.
    pub fn summary(&self) -> &'static str {
        self.description
    }
}

/// Every plugin the editor ships and can install, in menu order.
pub const CATALOG: &[CatalogPlugin] = &[
    CatalogPlugin {
        file_name: "themes.tw",
        title: "Theme pack",
        description: "Every built-in theme as a command, two extra palettes defined in \
                      TextWrite, and Alt+T to cycle through them",
        source: include_str!("../plugin-catalog/themes.tw"),
    },
    CatalogPlugin {
        file_name: "case_and_lines.tw",
        title: "Case and lines",
        description: "Upper/lower/title case plus sort, reverse, de-duplicate and number \
                      the selected lines",
        source: include_str!("../plugin-catalog/case_and_lines.tw"),
    },
    CatalogPlugin {
        file_name: "stats.tw",
        title: "Statistics",
        description: "Word, character and line counts for the selection or the whole \
                      document, with a reading-time estimate",
        source: include_str!("../plugin-catalog/stats.tw"),
    },
    CatalogPlugin {
        file_name: "markdown_pro.tw",
        title: "Markdown pro",
        description: "Heading levels, a table skeleton, fenced code, links, quotes and \
                      thematic breaks",
        source: include_str!("../plugin-catalog/markdown_pro.tw"),
    },
    CatalogPlugin {
        file_name: "whitespace.tw",
        title: "Whitespace",
        description: "Trim trailing spaces, convert tabs and spaces, and collapse runs of \
                      blank lines",
        source: include_str!("../plugin-catalog/whitespace.tw"),
    },
    CatalogPlugin {
        file_name: "snippets.tw",
        title: "Snippets",
        description: "Section banners, dividers, note blocks, TODO markers and checkboxes",
        source: include_str!("../plugin-catalog/snippets.tw"),
    },
    CatalogPlugin {
        file_name: "comments.tw",
        title: "Comments",
        description: "Toggle line comments, wrap blocks and build commented banners \
                      for over thirty languages",
        source: include_str!("../plugin-catalog/comments.tw"),
    },
    CatalogPlugin {
        file_name: "csv_tools.tw",
        title: "CSV tools",
        description: "Pad columns into alignment, sort by a column, count fields, \
                      swap columns and render a bordered table",
        source: include_str!("../plugin-catalog/csv_tools.tw"),
    },
    CatalogPlugin {
        file_name: "markdown_extras.tw",
        title: "Markdown extras",
        description: "Task lists, footnotes, definition lists, reference links, \
                      strikethrough and a generated table of contents",
        source: include_str!("../plugin-catalog/markdown_extras.tw"),
    },
    CatalogPlugin {
        file_name: "date_time.tw",
        title: "Date converter",
        description: "Read a date from the selection and rewrite it as ISO, long, \
                      short, slash or compact form, with the weekday",
        source: include_str!("../plugin-catalog/date_time.tw"),
    },
    CatalogPlugin {
        file_name: "identifier_case.tw",
        title: "Identifier cases",
        description: "Convert names between snake, camel, Pascal, kebab, CONSTANT \
                      and dot.case, or split them into words",
        source: include_str!("../plugin-catalog/identifier_case.tw"),
    },
    CatalogPlugin {
        file_name: "word_frequency.tw",
        title: "Word frequency",
        description: "Count the selection's words, rank them, chart them as a \
                      comment block and estimate reading time",
        source: include_str!("../plugin-catalog/word_frequency.tw"),
    },
    CatalogPlugin {
        file_name: "boxes.tw",
        title: "Box drawing",
        description: "Surround the selection with a single, rounded, double or heavy \
                      Unicode box",
        source: include_str!("../plugin-catalog/boxes.tw"),
    },
    CatalogPlugin {
        file_name: "align.tw",
        title: "Align",
        description: "Line the selected rows up on their first =, :, => or | so the \
                      delimiter starts in the same column",
        source: include_str!("../plugin-catalog/align.tw"),
    },
    CatalogPlugin {
        file_name: "cipher.tw",
        title: "Ciphers",
        description: "ROT13 and Atbash over the selection, preserving case and \
                      leaving punctuation alone",
        source: include_str!("../plugin-catalog/cipher.tw"),
    },
    CatalogPlugin {
        file_name: "todo_report.tw",
        title: "TODO report",
        description: "Group every TODO, FIXME, HACK, XXX and NOTE in the document \
                      into a report at the cursor, with line numbers",
        source: include_str!("../plugin-catalog/todo_report.tw"),
    },
    CatalogPlugin {
        file_name: "wrap.tw",
        title: "Wrap",
        description: "Reflow the selection to 72 or 80 columns, or unwrap paragraphs \
                      back into one line each",
        source: include_str!("../plugin-catalog/wrap.tw"),
    },
    CatalogPlugin {
        file_name: "path_tools.tw",
        title: "Path tools",
        description: "Switch path separators, keep base names or directories, and \
                      quote or unquote the selected paths",
        source: include_str!("../plugin-catalog/path_tools.tw"),
    },
    CatalogPlugin {
        file_name: "brackets.tw",
        title: "Smart brackets",
        description: "Auto-close and wrap with (), [], {} and quotes, stepping over \
                      an existing closing character",
        source: include_str!("../plugin-catalog/brackets.tw"),
    },
    CatalogPlugin {
        file_name: "markdown_preview.tw",
        title: "Markdown preview",
        description: "Toggle a Markdown buffer into a readable plain-text preview and \
                      back, with an optional heading outline",
        source: include_str!("../plugin-catalog/markdown_preview.tw"),
    },
    CatalogPlugin {
        file_name: "status_line.tw",
        title: "Status formats",
        description: "Four custom status-line formats driven by editor globals, with \
                      open/save hooks and an on-demand refresh",
        source: include_str!("../plugin-catalog/status_line.tw"),
    },
    CatalogPlugin {
        file_name: "toc.tw",
        title: "Linked TOC",
        description: "Insert a linked Markdown table of contents with GitHub-style \
                      anchors, skipping fenced code blocks",
        source: include_str!("../plugin-catalog/toc.tw"),
    },
    CatalogPlugin {
        file_name: "brace_format.tw",
        title: "Brace formatter",
        description: "Indent JSON, CSS and other brace-delimited text or compact it \
                      to one line, without touching string contents",
        source: include_str!("../plugin-catalog/brace_format.tw"),
    },
    CatalogPlugin {
        file_name: "sql_case.tw",
        title: "SQL casing",
        description: "Upper- or lower-case SQL keywords in a selection while leaving \
                      identifiers and quoted strings untouched",
        source: include_str!("../plugin-catalog/sql_case.tw"),
    },
    CatalogPlugin {
        file_name: "markdown_tables.tw",
        title: "Markdown tables",
        description: "Turn TSV or CSV rows into an aligned Markdown table and \
                      re-align an existing pipe table",
        source: include_str!("../plugin-catalog/markdown_tables.tw"),
    },
    CatalogPlugin {
        file_name: "log_summary.tw",
        title: "Log summary",
        description: "Count selected log lines by severity with a sample of each, or \
                      keep only the error lines",
        source: include_str!("../plugin-catalog/log_summary.tw"),
    },
    CatalogPlugin {
        file_name: "lorem.tw",
        title: "Lorem ipsum",
        description: "Insert placeholder sentences, paragraphs, lists and titles for \
                      drafting and layout testing",
        source: include_str!("../plugin-catalog/lorem.tw"),
    },
    CatalogPlugin {
        file_name: "slugify.tw",
        title: "Slugify",
        description: "Convert selected text into URL slugs and filename-safe fragments \
                      with hyphens, underscores or dots",
        source: include_str!("../plugin-catalog/slugify.tw"),
    },
    CatalogPlugin {
        file_name: "escape.tw",
        title: "Escape tools",
        description: "Escape, unescape and quote strings using JSON-style backslash \
                      sequences",
        source: include_str!("../plugin-catalog/escape.tw"),
    },
    CatalogPlugin {
        file_name: "extract.tw",
        title: "Extract",
        description: "Extract emails, URLs, numbers and unique lines from the \
                      selection or document",
        source: include_str!("../plugin-catalog/extract.tw"),
    },
    CatalogPlugin {
        file_name: "number_base.tw",
        title: "Number base",
        description: "Convert integers between decimal, hexadecimal and binary, and \
                      increment or decrement values",
        source: include_str!("../plugin-catalog/number_base.tw"),
    },
    CatalogPlugin {
        file_name: "checklist.tw",
        title: "Checklist",
        description: "Toggle Markdown checkboxes, add or remove bullets, and indent \
                      or outdent selected lines",
        source: include_str!("../plugin-catalog/checklist.tw"),
    },
];

/// The catalog entry with this file name, if any.
pub fn by_file_name(file_name: &str) -> Option<&'static CatalogPlugin> {
    CATALOG.iter().find(|plugin| plugin.file_name == file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entries_are_well_formed() {
        for plugin in CATALOG {
            assert!(plugin.file_name.ends_with(".tw"), "{}", plugin.file_name);
            assert!(!plugin.title.is_empty(), "{}", plugin.file_name);
            assert!(!plugin.description.is_empty(), "{}", plugin.file_name);
            assert!(!plugin.source.trim().is_empty(), "{}", plugin.file_name);
            assert!(
                plugin.source.contains("command"),
                "{} registers no commands",
                plugin.file_name
            );
        }
    }

    #[test]
    fn file_names_are_unique() {
        let mut names: Vec<&str> = CATALOG.iter().map(|p| p.file_name).collect();
        names.sort();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn lookup_by_file_name() {
        assert!(by_file_name("themes.tw").is_some());
        assert!(by_file_name("nope.tw").is_none());
    }

    #[test]
    fn every_catalog_plugin_loads_and_registers_commands() {
        for plugin in CATALOG {
            let mut host = fast_text_textwrite::ScriptHost::new();
            host.add_script(plugin.file_name, plugin.source);
            assert!(
                host.errors.is_empty(),
                "{}: {:?}",
                plugin.file_name,
                host.errors
            );
            assert!(
                !host.command_titles().is_empty(),
                "{} registers no commands",
                plugin.file_name
            );
        }
    }

    #[test]
    fn catalog_plugins_load_together_without_name_clashes() {
        // The editor merges every script into one interpreter, so the whole
        // catalog must coexist: no duplicate function names, no bad syntax.
        let mut host = fast_text_textwrite::ScriptHost::new();
        for plugin in CATALOG {
            host.add_script(plugin.file_name, plugin.source);
        }
        assert!(host.errors.is_empty(), "{:?}", host.errors);
    }

    #[test]
    fn catalog_commands_run_without_runtime_errors() {
        // Loading a script only parses it; this runs every command that the
        // whole catalog registers, so a call to a missing builtin or a bad
        // argument shows up here rather than in front of a user. It runs twice
        // — once with an empty editor and once with a selection, so the
        // guarded branches in the plugins are exercised too.
        let mut host = fast_text_textwrite::ScriptHost::new();
        for plugin in CATALOG {
            host.add_script(plugin.file_name, plugin.source);
        }
        assert!(host.errors.is_empty(), "{:?}", host.errors);

        let selected = "the quick brown fox\njumps over\t2 lazy dogs\n\nthe quick brown fox";
        let contexts = [
            fast_text_plugin::EditorContext::default(),
            fast_text_plugin::EditorContext {
                has_selection: true,
                selected_text: selected.to_string(),
                selection_start: 0,
                selection_end: selected.len(),
                doc_text: format!("{selected}\ntail"),
                ..Default::default()
            },
        ];

        let titles = host.command_titles();
        assert!(!titles.is_empty());
        for context in &contexts {
            for title in &titles {
                host.run_command(title, context);
            }
        }
        assert!(
            host.errors.is_empty(),
            "commands raised runtime errors: {:?}",
            host.errors
        );
    }

    /// Runs one catalog command against a selection and returns the inserted text.
    fn run_with_selection(title: &str, text: &str) -> String {
        let mut host = fast_text_textwrite::ScriptHost::new();
        for plugin in CATALOG {
            host.add_script(plugin.file_name, plugin.source);
        }
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        let context = fast_text_plugin::EditorContext {
            has_selection: true,
            selected_text: text.to_string(),
            selection_start: 0,
            selection_end: text.len(),
            doc_text: text.to_string(),
            ..Default::default()
        };
        let actions = host.run_command(title, &context);
        assert!(host.errors.is_empty(), "{title}: {:?}", host.errors);
        actions
            .iter()
            .filter_map(|action| match action {
                fast_text_plugin::EditorAction::Insert(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_cipher_plugin_rotates_and_mirrors_letters() {
        assert_eq!(
            run_with_selection("Cipher: ROT13", "Hello, World! 123"),
            "Uryyb, Jbeyq! 123"
        );
        assert_eq!(run_with_selection("Cipher: Atbash", "Abc XYZ"), "Zyx CBA");
    }

    #[test]
    fn the_box_plugin_pads_to_the_widest_line() {
        let out = run_with_selection("Box: single line", "a\nbb\nccc");
        assert_eq!(out, "┌─────┐\n│ a   │\n│ bb  │\n│ ccc │\n└─────┘");
    }

    #[test]
    fn the_align_plugin_lines_up_the_delimiter() {
        let out = run_with_selection("Align: on =", "a = 1\nlong_name = 2\n# note");
        assert_eq!(out, "a         = 1\nlong_name = 2\n# note");
    }

    #[test]
    fn the_wrap_plugin_reflows_to_the_requested_width() {
        let text = "one two three four five six seven eight nine ten eleven twelve \
                    thirteen fourteen fifteen sixteen";
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let out = run_with_selection("Wrap: 80 columns", &text);
        assert!(out.lines().count() > 1, "{out}");
        assert!(out.lines().all(|line| line.chars().count() <= 80), "{out}");
        assert_eq!(out.split_whitespace().count(), 16);
    }

    #[test]
    fn the_todo_plugin_reports_line_numbers_by_marker() {
        let out = run_with_selection(
            "TODO: report at cursor",
            "fn main() {} // TODO tidy\n// FIXME and again\n// NOTE fine",
        );
        assert!(out.contains("TODO (1)"), "{out}");
        assert!(out.contains("FIXME (1)"), "{out}");
        assert!(out.contains("line 1: fn main() {} // TODO tidy"), "{out}");
    }

    /// Runs one catalog command with a fully custom context.
    fn actions_for(
        title: &str,
        context: &fast_text_plugin::EditorContext,
    ) -> Vec<fast_text_plugin::EditorAction> {
        let mut host = fast_text_textwrite::ScriptHost::new();
        for plugin in CATALOG {
            host.add_script(plugin.file_name, plugin.source);
        }
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        let actions = host.run_command(title, context);
        assert!(host.errors.is_empty(), "{title}: {:?}", host.errors);
        actions
    }

    fn inserted(title: &str, ctx: &fast_text_plugin::EditorContext) -> String {
        actions_for(title, ctx)
            .into_iter()
            .filter_map(|action| match action {
                fast_text_plugin::EditorAction::Insert(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn selection_context(text: &str) -> fast_text_plugin::EditorContext {
        fast_text_plugin::EditorContext {
            has_selection: true,
            selected_text: text.to_string(),
            selection_start: 0,
            selection_end: text.len(),
            doc_text: text.to_string(),
            file_name: "notes.md".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn the_bracket_plugin_wraps_the_selection_and_parks_the_caret() {
        let actions = actions_for("Brackets: wrap in ()", &selection_context("hi"));
        let inserted = actions
            .iter()
            .find_map(|action| match action {
                fast_text_plugin::EditorAction::Insert(text) => Some(text.as_str()),
                _ => None,
            })
            .unwrap();
        assert_eq!(inserted, "(hi)");
        assert!(
            actions.contains(&fast_text_plugin::EditorAction::SetCursor(1)),
            "the caret belongs between the pair: {actions:?}"
        );
    }

    #[test]
    fn the_brace_formatter_indents_and_compacts_json() {
        let out = run_with_selection("Format: indent braces", "{\"a\":{\"b\":1},\"c\":[2,3]}");
        assert!(out.contains("  \"a\": {"), "{out}");
        assert!(out.contains("    \"b\": 1"), "{out}");
        assert!(out.lines().count() >= 5, "{out}");
        let compact =
            run_with_selection("Format: compact JSON", "{ \"a\" : 1,\n \"b\" : [ 2, 3 ] }");
        assert_eq!(compact, "{\"a\":1,\"b\":[2,3]}");
    }

    #[test]
    fn the_sql_plugin_only_touches_keywords() {
        let out = run_with_selection(
            "SQL: upper-case keywords",
            "select name from t where name = 'select'",
        );
        assert_eq!(out, "SELECT name FROM t WHERE name = 'select'");
        let lower = run_with_selection(
            "SQL: lower-case keywords",
            "SELECT * FROM T WHERE x IS NULL",
        );
        assert_eq!(lower, "select * from T where x is null");
    }

    #[test]
    fn the_table_plugin_builds_aligned_markdown() {
        let out = run_with_selection("Table: from TSV", "a\tbb\nc\td");
        assert_eq!(out, "| a | bb |\n| - | -- |\n| c | d  |");
    }

    #[test]
    fn the_log_plugin_counts_severities_and_filters_errors() {
        let log =
            "2026-01-01 INFO boot\n2026-01-01 ERROR disk full\n2026-01-01 WARN slow\nERROR again";
        let summary = run_with_selection("Log: summary", log);
        assert!(summary.contains("Log summary"), "{summary}");
        assert!(summary.contains("ERROR: 2"), "{summary}");
        assert!(summary.contains("WARN: 1"), "{summary}");
        assert_eq!(
            run_with_selection("Log: errors only", log),
            "2026-01-01 ERROR disk full\nERROR again"
        );
    }

    #[test]
    fn the_toc_plugin_links_headings_and_skips_code() {
        let doc = "# Hello World\n\n```md\n# Not a heading\n```\n\n## Getting started\n";
        let ctx = selection_context(doc);
        let out = inserted("Outline: insert Markdown TOC", &ctx);
        assert_eq!(
            out,
            "- [Hello World](#hello-world)\n  - [Getting started](#getting-started)"
        );
    }

    #[test]
    fn the_preview_plugin_restores_the_original_source() {
        let mut host = fast_text_textwrite::ScriptHost::new();
        for plugin in CATALOG {
            host.add_script(plugin.file_name, plugin.source);
        }
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        let original = "# Hello\n\n- [x] done\n\n**bold**\n";
        let ctx = selection_context(original);
        let actions = host.run_command("Markdown view: toggle preview", &ctx);
        let preview = actions
            .iter()
            .find_map(|action| match action {
                fast_text_plugin::EditorAction::Insert(text) => Some(text.as_str()),
                _ => None,
            })
            .expect("the preview replaces the buffer");
        assert!(preview.contains("HELLO"), "{preview}");
        assert!(preview.contains("☑ done"), "{preview}");
        assert!(preview.contains("="), "{preview}");
        let preview_ctx = selection_context(preview);
        let actions = host.run_command("Markdown view: toggle preview", &preview_ctx);
        let restored = actions
            .iter()
            .find_map(|action| match action {
                fast_text_plugin::EditorAction::Insert(text) => Some(text.as_str()),
                _ => None,
            })
            .expect("the toggle restores the buffer");
        assert_eq!(restored, original);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
    }

    #[test]
    fn the_status_plugin_formats_the_status_line() {
        let ctx = fast_text_plugin::EditorContext {
            line: 3,
            col: 4,
            line_count: 20,
            language: "Rust".to_string(),
            file_name: "main.rs".to_string(),
            doc_text: "one two three four five".to_string(),
            ..Default::default()
        };
        let actions = actions_for("Status: compact", &ctx);
        assert!(
            actions.iter().any(|action| match action {
                fast_text_plugin::EditorAction::SetStatus(Some(text)) =>
                    text.contains("Ln 4, Col 5") && text.contains("Rust"),
                _ => false,
            }),
            "{actions:?}"
        );
        let actions = actions_for("Status: writing", &ctx);
        assert!(
            actions.iter().any(|action| match action {
                fast_text_plugin::EditorAction::SetStatus(Some(text)) =>
                    text.contains("5 document words"),
                _ => false,
            }),
            "{actions:?}"
        );
        assert!(actions_for("Status: restore default", &ctx)
            .contains(&fast_text_plugin::EditorAction::SetStatus(None)));
    }

    #[test]
    fn the_path_plugin_keeps_base_names_and_directories() {
        assert_eq!(
            run_with_selection("Paths: base names", "src/main.rs\n/usr/local/bin/"),
            "main.rs\nbin"
        );
        assert_eq!(
            run_with_selection("Paths: directories", "src/main.rs\n/usr/local/bin"),
            "src\n/usr/local"
        );
        assert_eq!(
            run_with_selection("Paths: forward slashes", r"C:\Users\kali\file.txt"),
            "C:/Users/kali/file.txt"
        );
    }
}
