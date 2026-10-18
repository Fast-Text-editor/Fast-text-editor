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
}
