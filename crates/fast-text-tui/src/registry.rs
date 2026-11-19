//! The plugin registry and the `ft plugin` / `ft install` commands.
//!
//! Fast Text ships a curated registry of TextWrite plugins. Their sources are
//! compiled into the binary (see [`crate::catalog`]), so `ft install boxes`
//! works with no network connection and no package manager. The same registry
//! is stored as JSON in `registry/index.json`, which makes it easy to generate
//! documentation and lets people add their own entries in
//! `~/.config/fast-text/registry.json`; entries without a compiled-in source
//! are downloaded from their `url`.
//!
//! The command-line side is deliberately boring: it prints plain lines, exits
//! non-zero on failure, and never starts the editor or the terminal UI.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::catalog;

/// The registry index compiled into the editor, so installing a first-party
/// plugin never needs the network.
const INDEX_JSON: &str = include_str!("../registry/index.json");

/// Where the canonical registry lives in the repository.
pub const REPO_INDEX_URL: &str = "https://raw.githubusercontent.com/Fast-Text-editor/Fast-text-editor/main/crates/fast-text-tui/registry/index.json";

/// One plugin available from the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPlugin {
    /// Stable, human-friendly name used on the command line, e.g. `boxes`.
    pub name: String,
    /// File written into the plugin folder, e.g. `boxes.tw`.
    pub file: String,
    /// Short title for listings.
    pub title: String,
    /// One-line summary.
    pub description: String,
    /// Search tags.
    pub tags: Vec<String>,
    /// Download location for entries that are not compiled in.
    pub url: Option<String>,
    /// The source, when this plugin ships with the editor.
    pub embedded: Option<&'static str>,
}

impl RegistryPlugin {
    /// True when this entry can be installed without a download.
    pub fn is_bundled(&self) -> bool {
        self.embedded.is_some()
    }

    /// The plugin runtime selected by the file extension.
    pub fn kind(&self) -> &'static str {
        if self.file.ends_with(".rhai") {
            "Rhai"
        } else {
            "TextWrite"
        }
    }
}

#[derive(Debug, Deserialize)]
struct IndexFile {
    schema: u32,
    #[serde(default)]
    plugins: Vec<RawPlugin>,
}

#[derive(Debug, Deserialize)]
struct RawPlugin {
    name: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    url: Option<String>,
}

/// Parses a registry index. Unknown JSON fields are ignored so the format can
/// grow without breaking older editors.
pub fn parse_index(text: &str) -> Result<Vec<RegistryPlugin>, String> {
    let index: IndexFile =
        serde_json::from_str(text).map_err(|error| format!("invalid registry JSON: {error}"))?;
    if index.schema != 1 {
        return Err(format!(
            "unsupported registry schema {} (this build understands schema 1)",
            index.schema
        ));
    }
    let mut plugins = Vec::with_capacity(index.plugins.len());
    for raw in index.plugins {
        let name = raw.name.trim().to_string();
        if name.is_empty() {
            return Err("a registry entry is missing its name".to_string());
        }
        let file = raw
            .file
            .unwrap_or_else(|| format!("{}.tw", name.replace([' ', '-'], "_")));
        validate_file_name(&file)?;
        let title = raw.title.unwrap_or_else(|| name.clone());
        plugins.push(RegistryPlugin {
            name,
            file,
            title,
            description: raw.description.unwrap_or_default(),
            tags: raw.tags,
            url: raw.url,
            embedded: None,
        });
    }
    Ok(plugins)
}

/// The registry compiled into the editor, enriched with catalog metadata.
pub fn bundled() -> Result<Vec<RegistryPlugin>, String> {
    let mut plugins = parse_index(INDEX_JSON)?;
    enrich(&mut plugins);
    Ok(plugins)
}

/// Adds titles, descriptions and sources from the compiled-in catalog.
fn enrich(plugins: &mut [RegistryPlugin]) {
    for plugin in plugins {
        if let Some(entry) = catalog::by_file_name(&plugin.file) {
            if plugin.description.is_empty() {
                plugin.description = entry.description.to_string();
            }
            if plugin.title == plugin.name {
                plugin.title = entry.title.to_string();
            }
            plugin.embedded = Some(entry.source);
        }
    }
}

/// Loads the bundled registry plus any entries in `user_index`.
///
/// A user entry with the same name replaces the bundled one, which makes it
/// easy to pin a fork or try a local plugin under a familiar name. A broken
/// user file is reported as a warning instead of hiding the bundled plugins.
pub fn load(user_index: Option<&Path>) -> (Vec<RegistryPlugin>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut plugins = match bundled() {
        Ok(plugins) => plugins,
        Err(error) => {
            warnings.push(format!("the built-in registry is broken: {error}"));
            Vec::new()
        }
    };
    if let Some(path) = user_index {
        match fs::read_to_string(path) {
            Ok(text) => match parse_index(&text) {
                Ok(mut extra) => {
                    enrich(&mut extra);
                    for plugin in extra {
                        if let Some(existing) = plugins.iter_mut().find(|p| p.name == plugin.name) {
                            *existing = plugin;
                        } else {
                            plugins.push(plugin);
                        }
                    }
                }
                Err(error) => warnings.push(format!("{}: {error}", path.display())),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => warnings.push(format!("{}: {error}", path.display())),
        }
    }
    (plugins, warnings)
}

/// Finds a plugin by name, file name or title, ignoring case and separators.
pub fn find<'a>(plugins: &'a [RegistryPlugin], name: &str) -> Option<&'a RegistryPlugin> {
    let wanted = normalize(name);
    plugins.iter().find(|plugin| {
        normalize(&plugin.name) == wanted
            || normalize(&plugin.file) == wanted
            || normalize(&plugin.title) == wanted
    })
}

/// Filter for `ft plugin search`: all words must match somewhere.
pub fn search<'a>(plugins: &'a [RegistryPlugin], query: &str) -> Vec<&'a RegistryPlugin> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|word| word.to_ascii_lowercase())
        .collect();
    plugins
        .iter()
        .filter(|plugin| {
            let haystack = format!(
                "{} {} {} {} {}",
                plugin.name,
                plugin.file,
                plugin.title,
                plugin.description,
                plugin.tags.join(" ")
            )
            .to_ascii_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect()
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Rejects file names that could escape the plugin directory.
pub fn validate_file_name(file: &str) -> Result<(), String> {
    if file.is_empty()
        || file.starts_with('.')
        || !file.ends_with(".tw") && !file.ends_with(".rhai")
        || !file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(format!(
            "'{file}' is not a safe plugin file name (expected name.tw or name.rhai)"
        ));
    }
    Ok(())
}

/// `$XDG_CONFIG_HOME/fast-text/plugins`, falling back to `~/.config/fast-text/plugins`.
pub fn plugin_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("fast-text").join("plugins"))
}

/// `~/.config/fast-text/registry.json`, beside the plugin folder.
pub fn user_index_path() -> Option<PathBuf> {
    plugin_dir().map(|dir| dir.with_file_name("registry.json"))
}

/// Installs `plugin` into `dir`, downloading it only when it is not bundled.
pub fn install(plugin: &RegistryPlugin, dir: &Path, force: bool) -> Result<PathBuf, String> {
    if let Some(source) = plugin.embedded {
        install_source(plugin, dir, force, source)
    } else {
        let url = plugin.url.as_deref().ok_or_else(|| {
            format!(
                "'{}' ships without a source and has no download URL in the registry",
                plugin.name
            )
        })?;
        let source = fast_text_update::fetch_text(url, 30)?;
        install_source(plugin, dir, force, &source)
    }
}

/// Writes already-loaded source into `dir`, refusing to clobber a file unless
/// `force` is set. The file is written next to its destination and renamed, so
/// an interrupted install cannot leave a half-written plugin behind.
pub fn install_source(
    plugin: &RegistryPlugin,
    dir: &Path,
    force: bool,
    source: &str,
) -> Result<PathBuf, String> {
    validate_file_name(&plugin.file)?;
    if source.trim().is_empty() {
        return Err(format!("the source for '{}' is empty", plugin.name));
    }
    if source.len() > 1024 * 1024 {
        return Err(format!(
            "the source for '{}' is larger than 1 MiB; refusing to install",
            plugin.name
        ));
    }
    fs::create_dir_all(dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    let path = dir.join(&plugin.file);
    if path.exists() && !force {
        return Err(format!(
            "{} is already installed (pass --force to overwrite it)",
            path.display()
        ));
    }
    let temp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("tw")
    ));
    fs::write(&temp, source)
        .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
    fs::rename(&temp, &path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("could not install {}: {error}", path.display())
    })?;
    Ok(path)
}

/// Removes an installed plugin by name.
pub fn remove(plugins: &[RegistryPlugin], dir: &Path, name: &str) -> Result<PathBuf, String> {
    let plugin =
        find(plugins, name).ok_or_else(|| format!("no plugin named '{name}' in the registry"))?;
    let path = dir.join(&plugin.file);
    fs::remove_file(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("{} is not installed", plugin.file)
        } else {
            format!("could not remove {}: {error}", path.display())
        }
    })?;
    Ok(path)
}

/// Lists the plugin files installed in `dir`, sorted by name.
pub fn installed(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("tw") | Some("rhai")
                )
        })
        .collect();
    files.sort();
    files
}

/// Runs `ft plugin …` and prints human-readable output.
pub fn run(command: &PluginCommand) -> std::io::Result<()> {
    let (plugins, warnings) = load(user_index_path().as_deref());
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
    let dir = plugin_dir().ok_or_else(|| {
        std::io::Error::other("could not find a home directory for the plugin folder")
    })?;

    match command {
        PluginCommand::List => {
            println!("Plugin folder: {}", dir.display());
            let files = installed(&dir);
            if files.is_empty() {
                println!("No plugins installed yet.");
            } else {
                println!("Installed ({}):", files.len());
                for path in files {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                    let marker = if find(&plugins, name).is_some() {
                        ""
                    } else {
                        "  (local)"
                    };
                    println!("  {name}{marker}");
                }
            }
            println!(
                "\n{} plugins available in the registry — try 'ft plugin search'.",
                plugins.len()
            );
        }
        PluginCommand::Search(query) => {
            let matches = search(&plugins, query);
            if matches.is_empty() {
                println!("No registry plugins match '{query}'.");
                return Ok(());
            }
            let installed_files: Vec<String> = installed(&dir)
                .iter()
                .filter_map(|path| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string)
                })
                .collect();
            println!("{:<22} {:<20} TAGS", "NAME", "TITLE");
            for plugin in matches {
                let installed = if installed_files.iter().any(|file| file == &plugin.file) {
                    "  [installed]"
                } else {
                    ""
                };
                println!(
                    "{:<22} {:<20} {}{}",
                    plugin.name,
                    plugin.title,
                    plugin.tags.join(", "),
                    installed
                );
            }
        }
        PluginCommand::Install { names, force } => {
            let mut failed = false;
            for name in names {
                let Some(plugin) = find(&plugins, name) else {
                    eprintln!("error: no plugin named '{name}' in the registry");
                    eprintln!("       run 'ft plugin search' to see what is available");
                    failed = true;
                    continue;
                };
                match install(plugin, &dir, *force) {
                    Ok(path) => println!(
                        "Installed {} ({}) → {}",
                        plugin.name,
                        plugin.kind(),
                        path.display()
                    ),
                    Err(error) => {
                        eprintln!("error: {error}");
                        failed = true;
                    }
                }
            }
            if failed {
                std::process::exit(1);
            }
        }
        PluginCommand::Remove { names } => {
            let mut failed = false;
            for name in names {
                match remove(&plugins, &dir, name) {
                    Ok(path) => println!("Removed {}", path.display()),
                    Err(error) => {
                        eprintln!("error: {error}");
                        failed = true;
                    }
                }
            }
            if failed {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

/// What `ft plugin` (or the `ft install` shorthand) was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginCommand {
    /// List installed plugins.
    List,
    /// Search the registry. An empty query lists everything.
    Search(String),
    /// Install one or more registry plugins.
    Install { names: Vec<String>, force: bool },
    /// Remove installed plugins.
    Remove { names: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fast-text-registry-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> RegistryPlugin {
        RegistryPlugin {
            name: "sample".to_string(),
            file: "sample.tw".to_string(),
            title: "Sample".to_string(),
            description: "A sample plugin".to_string(),
            tags: vec!["test".to_string()],
            url: None,
            embedded: None,
        }
    }

    #[test]
    fn the_bundled_index_is_valid_and_every_entry_is_installable() {
        let plugins = bundled().unwrap();
        assert_eq!(plugins.len(), catalog::CATALOG.len());
        for plugin in &plugins {
            assert!(plugin.is_bundled(), "{} has no source", plugin.name);
            assert!(plugin.embedded.unwrap().contains("command"));
            validate_file_name(&plugin.file).unwrap();
        }
    }

    #[test]
    fn bundled_entries_borrow_titles_and_descriptions_from_the_catalog() {
        let plugins = bundled().unwrap();
        let boxes = find(&plugins, "boxes").unwrap();
        assert_eq!(boxes.title, "Box drawing");
        assert!(boxes.description.contains("selection"));
    }

    #[test]
    fn names_ignore_case_separators_and_extensions() {
        let plugins = bundled().unwrap();
        for name in ["boxes", "Boxes", "boxes.tw", "BOX-DRAWING", "box_drawing"] {
            assert!(find(&plugins, name).is_some(), "{name} did not resolve");
        }
        assert!(find(&plugins, "no-such-plugin").is_none());
    }

    #[test]
    fn search_matches_every_word_across_fields() {
        let plugins = bundled().unwrap();
        assert!(search(&plugins, "markdown").len() >= 2);
        assert!(search(&plugins, "markdown table")
            .iter()
            .any(|p| p.name == "markdown-pro"));
        assert!(search(&plugins, "zzzz").is_empty());
    }

    #[test]
    fn a_user_index_can_add_and_replace_entries() {
        let dir = temp_dir("user-index");
        let path = dir.join("registry.json");
        fs::write(
            &path,
            r#"{"schema":1,"plugins":[
                {"name":"my-plugin","title":"Mine","description":"local","url":"https://example.com/my.tw"},
                {"name":"boxes","title":"Boxes (fork)"}
            ]}"#,
        )
        .unwrap();
        let (plugins, warnings) = load(Some(&path));
        assert!(warnings.is_empty());
        assert_eq!(find(&plugins, "my-plugin").unwrap().title, "Mine");
        assert_eq!(find(&plugins, "boxes").unwrap().title, "Boxes (fork)");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_user_index_warns_but_keeps_the_bundled_entries() {
        let dir = temp_dir("broken-index");
        let path = dir.join("registry.json");
        fs::write(&path, "{not json").unwrap();
        let (plugins, warnings) = load(Some(&path));
        assert_eq!(warnings.len(), 1);
        assert_eq!(plugins.len(), catalog::CATALOG.len());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_writes_the_plugin_and_refuses_to_overwrite() {
        let dir = temp_dir("install");
        let plugin = sample();
        let path = install_source(&plugin, &dir, false, "command(\"Hi\", \"hi\")\n").unwrap();
        assert_eq!(path, dir.join("sample.tw"));
        assert!(fs::read_to_string(&path).unwrap().contains("Hi"));
        let error = install_source(&plugin, &dir, false, "other").unwrap_err();
        assert!(error.contains("already installed"));
        install_source(&plugin, &dir, true, "other").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "other");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_never_escapes_the_plugin_folder() {
        let dir = temp_dir("escape");
        let mut plugin = sample();
        plugin.file = "../evil.tw".to_string();
        let error = install_source(&plugin, &dir, false, "x").unwrap_err();
        assert!(error.contains("not a safe plugin file name"));
        assert!(!dir.parent().unwrap().join("evil.tw").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_deletes_only_installed_plugins() {
        let dir = temp_dir("remove");
        let plugins = vec![sample()];
        install_source(&sample(), &dir, false, "x").unwrap();
        let path = remove(&plugins, &dir, "sample").unwrap();
        assert!(!path.exists());
        assert!(remove(&plugins, &dir, "sample")
            .unwrap_err()
            .contains("not installed"));
        assert!(remove(&plugins, &dir, "missing")
            .unwrap_err()
            .contains("no plugin"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn installed_lists_only_plugin_files() {
        let dir = temp_dir("installed");
        fs::write(dir.join("a.tw"), "").unwrap();
        fs::write(dir.join("b.rhai"), "").unwrap();
        fs::write(dir.join("notes.txt"), "").unwrap();
        let names: Vec<String> = installed(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.tw", "b.rhai"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_indexes_are_rejected_with_a_clear_message() {
        assert!(parse_index("[]")
            .unwrap_err()
            .contains("invalid registry JSON"));
        assert!(parse_index(r#"{"schema":2,"plugins":[]}"#)
            .unwrap_err()
            .contains("schema 2"));
        assert!(parse_index(r#"{"schema":1,"plugins":[{"name":""}]}"#)
            .unwrap_err()
            .contains("missing its name"));
        assert!(
            parse_index(r#"{"schema":1,"plugins":[{"name":"x","file":"../x.tw"}]}"#)
                .unwrap_err()
                .contains("not a safe plugin file name")
        );
    }
}
