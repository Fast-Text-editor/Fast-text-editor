//! A Visual-Studio-style plugin manager.
//!
//! The editor loads two kinds of plugins from disk — Rhai scripts (`.rhai`)
//! and TextWrite scripts (`.tw`). This module discovers those files, works out
//! what each one registers (commands, keybindings, and any load error), and
//! remembers which ones are switched off.
//!
//! The overlay has two lists. **Installed** shows the plugins found on disk;
//! **Available** shows the first-party catalog (see [`crate::catalog`]) —
//! TextWrite plugins compiled into the editor that can be installed into the
//! user's plugin folder with one key press.
//!
//! Discovery loads every plugin in isolation so the manager can show per-file
//! details even though the running editor merges them into a single
//! command/keybinding table. Turning a plugin on or off rewrites a small
//! `plugins.disabled` file next to the user's plugin folder and asks the app
//! to rebuild its hosts.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use fast_text_plugin::{CommandInfo, KeySpec, PluginHost};
use fast_text_textwrite::ScriptHost;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;

use crate::catalog::{CatalogPlugin, CATALOG};

/// Assumed list height until the first draw reports the real one.
const DEFAULT_VIEW_HEIGHT: usize = 20;

/// Which runtime a plugin file targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKind {
    Rhai,
    TextWrite,
}

impl PluginKind {
    /// Human name for the details pane.
    pub fn label(self) -> &'static str {
        match self {
            PluginKind::Rhai => "Rhai",
            PluginKind::TextWrite => "TextWrite",
        }
    }

    /// One-letter badge for the list.
    pub fn badge(self) -> &'static str {
        match self {
            PluginKind::Rhai => "R",
            PluginKind::TextWrite => "T",
        }
    }

    /// The runtime selected by a file extension, if the file is a plugin.
    fn from_path(path: &Path) -> Option<PluginKind> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rhai") => Some(PluginKind::Rhai),
            Some("tw") => Some(PluginKind::TextWrite),
            _ => None,
        }
    }
}

/// A plugin discovered on disk.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    /// File name, e.g. `cursor.tw`.
    pub name: String,
    pub path: PathBuf,
    pub kind: PluginKind,
    pub enabled: bool,
    /// First comment line in the file, if any.
    pub description: String,
    pub commands: Vec<CommandInfo>,
    pub keybindings: Vec<(KeySpec, String)>,
    /// Load error, if the plugin failed to run in isolation.
    pub error: Option<String>,
}

/// What the overlay wants the app to do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerAction {
    /// Nothing changed.
    None,
    /// A plugin was toggled or installed; the app should rebuild its hosts.
    Reload,
    /// The user asked to install a plugin from a path.
    Install,
    /// The user asked to close the manager.
    Close,
}

/// Which of the two lists the overlay is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerTab {
    /// Plugins found on disk.
    Installed,
    /// Catalog plugins that are not installed yet.
    Available,
}

impl ManagerTab {
    /// Title for the tab strip.
    pub fn title(self) -> &'static str {
        match self {
            ManagerTab::Installed => "Installed",
            ManagerTab::Available => "Available",
        }
    }

    /// The other tab.
    fn other(self) -> ManagerTab {
        match self {
            ManagerTab::Installed => ManagerTab::Available,
            ManagerTab::Available => ManagerTab::Installed,
        }
    }
}

/// Discovers plugins and tracks which ones are enabled.
pub struct PluginManager {
    pub entries: Vec<PluginEntry>,
    pub selected: usize,
    pub scroll: usize,
    view_height: usize,
    /// Whether the overlay is open (and capturing keys).
    open: bool,
    /// Index of a plugin awaiting a delete confirmation.
    pending_delete: Option<usize>,
    outer_area: Rect,
    list_area: Rect,
    /// Directories scanned for plugins; earlier directories win on name clashes.
    dirs: Vec<PathBuf>,
    /// Turned-off plugin file names.
    disabled: BTreeSet<String>,
    /// Where the disabled set is persisted, if anywhere.
    config_path: Option<PathBuf>,
    /// Which list the overlay shows.
    tab: ManagerTab,
    /// Catalog plugins that are not installed yet.
    available: Vec<&'static CatalogPlugin>,
    /// Selection within the Available list.
    avail_selected: usize,
    /// Scroll within the Available list.
    avail_scroll: usize,
    /// The last error from an install, shown in the details pane.
    install_error: Option<String>,
    /// A one-shot message about the last install, for the app to show.
    notice: Option<String>,
}

impl PluginManager {
    /// Scans `dirs` for plugins and reads the disabled set from `config_path`.
    pub fn discover(dirs: Vec<PathBuf>, config_path: Option<PathBuf>) -> Self {
        let disabled = config_path
            .as_deref()
            .map(read_disabled)
            .unwrap_or_default();
        let mut manager = Self {
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            view_height: DEFAULT_VIEW_HEIGHT,
            open: false,
            pending_delete: None,
            outer_area: Rect::default(),
            list_area: Rect::default(),
            dirs,
            disabled,
            config_path,
            tab: ManagerTab::Installed,
            available: Vec::new(),
            avail_selected: 0,
            avail_scroll: 0,
            install_error: None,
            notice: None,
        };
        manager.rescan();
        manager
    }

    /// Opens the overlay, rescanning first so new files show up.
    pub fn open(&mut self) {
        self.rescan();
        self.open = true;
    }

    /// Closes the overlay.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the overlay is currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The directory new plugins are installed into (the first one scanned).
    pub fn primary_dir(&self) -> Option<&Path> {
        self.dirs.first().map(PathBuf::as_path)
    }

    /// Number of enabled plugins.
    pub fn enabled_count(&self) -> usize {
        self.entries.iter().filter(|e| e.enabled).count()
    }

    /// The entry under the selection, if the list is non-empty.
    pub fn selected_entry(&self) -> Option<&PluginEntry> {
        self.entries.get(self.selected)
    }

    /// Rescans the plugin directories, keeping the selection by file name.
    pub fn rescan(&mut self) {
        let selected_name = self.entries.get(self.selected).map(|e| e.name.clone());
        self.pending_delete = None;
        let mut entries = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for dir in &self.dirs {
            let Ok(read) = fs::read_dir(dir) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = read
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| PluginKind::from_path(p).is_some())
                .collect();
            paths.sort();
            for path in paths {
                let Some(kind) = PluginKind::from_path(&path) else {
                    continue;
                };
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if !seen.insert(name.clone()) {
                    continue;
                }
                let mut entry = inspect(path, kind, name);
                entry.enabled = !self.disabled.contains(&entry.name);
                entries.push(entry);
            }
        }
        // A plugin that vanished from disk should not stay in the disabled set.
        self.disabled.retain(|name| seen.contains(name));
        self.entries = entries;
        self.selected = selected_name
            .and_then(|name| self.entries.iter().position(|e| e.name == name))
            .unwrap_or_else(|| self.selected.min(self.entries.len().saturating_sub(1)));
        // Catalog plugins whose file is not on disk yet can be installed.
        self.available = CATALOG
            .iter()
            .filter(|plugin| !seen.contains(plugin.file_name))
            .collect();
        self.avail_selected = self
            .avail_selected
            .min(self.available.len().saturating_sub(1));
        self.clamp_scroll();
    }

    // --- Catalog -----------------------------------------------------------

    /// Which list the overlay is showing.
    pub fn tab(&self) -> ManagerTab {
        self.tab
    }

    /// Switches between the Installed and Available lists.
    pub fn switch_tab(&mut self) {
        self.tab = self.tab.other();
        self.pending_delete = None;
        self.install_error = None;
        self.clamp_scroll();
    }

    /// The catalog plugins that are not installed yet.
    pub fn available(&self) -> &[&'static CatalogPlugin] {
        &self.available
    }

    /// The installed plugins.
    pub fn installed(&self) -> &[PluginEntry] {
        &self.entries
    }

    /// The number of rows in the active list.
    pub fn list_len(&self) -> usize {
        match self.tab {
            ManagerTab::Installed => self.entries.len(),
            ManagerTab::Available => self.available.len(),
        }
    }

    /// The selection within the active list.
    pub fn list_selected(&self) -> usize {
        match self.tab {
            ManagerTab::Installed => self.selected,
            ManagerTab::Available => self.avail_selected,
        }
    }

    /// The scroll offset of the active list.
    pub fn list_scroll(&self) -> usize {
        match self.tab {
            ManagerTab::Installed => self.scroll,
            ManagerTab::Available => self.avail_scroll,
        }
    }

    /// The catalog plugin under the selection, when the Available tab is shown.
    pub fn selected_catalog(&self) -> Option<&'static CatalogPlugin> {
        self.available.get(self.avail_selected).copied()
    }

    /// The last install error, if one is being shown.
    pub fn install_error(&self) -> Option<&str> {
        self.install_error.as_deref()
    }

    /// Takes the pending install notice, if one is waiting.
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Installs a catalog plugin into the primary plugin directory.
    ///
    /// Returns the file name that was written.
    pub fn install_plugin(&mut self, plugin: &CatalogPlugin) -> Result<String, String> {
        let dir = self
            .primary_dir()
            .ok_or_else(|| "no plugin directory configured".to_string())?
            .to_path_buf();
        fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let dest = dir.join(plugin.file_name);
        fs::write(&dest, plugin.source)
            .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
        // A leftover "off" entry would hide the freshly installed plugin.
        if self.disabled.remove(plugin.file_name) {
            if let Some(state) = self.config_path.clone() {
                write_disabled(&state, &self.disabled);
            }
        }
        self.install_error = None;
        self.notice = Some(format!(
            "Installed {} — {}",
            plugin.title, plugin.description
        ));
        self.rescan();
        Ok(plugin.file_name.to_string())
    }

    /// Installs the catalog plugin at `index` in the Available list.
    pub fn install(&mut self, index: usize) -> Result<String, String> {
        let Some(plugin) = self.available.get(index).copied() else {
            return Err("no such plugin".to_string());
        };
        let result = self.install_plugin(plugin);
        if let Err(error) = &result {
            self.install_error = Some(error.clone());
        }
        result
    }

    /// Installs the selected catalog plugin.
    fn install_selected(&mut self) -> bool {
        let index = self.avail_selected;
        self.install(index).is_ok()
    }

    /// Turns the plugin at `index` on or off, persisting the change.
    pub fn toggle(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        entry.enabled = !entry.enabled;
        let (name, enabled) = (entry.name.clone(), entry.enabled);
        if enabled {
            self.disabled.remove(&name);
        } else {
            self.disabled.insert(name);
        }
        if let Some(path) = self.config_path.clone() {
            write_disabled(&path, &self.disabled);
        }
        true
    }

    /// Toggles the selected plugin.
    pub fn toggle_selected(&mut self) -> bool {
        self.toggle(self.selected)
    }

    /// The plugin awaiting a delete confirmation, if any.
    pub fn pending_delete_name(&self) -> Option<&str> {
        self.pending_delete
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.name.as_str())
    }

    /// Asks for confirmation before deleting the selected plugin.
    fn request_delete(&mut self) {
        if self.tab == ManagerTab::Installed && !self.entries.is_empty() {
            self.pending_delete = Some(self.selected);
        }
    }

    /// Deletes the plugin file at `index` and rescans.
    fn delete(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };
        let (path, name) = (entry.path.clone(), entry.name.clone());
        if fs::remove_file(&path).is_err() {
            return false;
        }
        self.disabled.remove(&name);
        if let Some(state) = self.config_path.clone() {
            write_disabled(&state, &self.disabled);
        }
        self.rescan();
        true
    }

    /// Builds fresh plugin hosts from the enabled entries.
    pub fn build_hosts(&self) -> (PluginHost, ScriptHost) {
        let mut plugins = PluginHost::new();
        let mut scripts = ScriptHost::new();
        for entry in self.entries.iter().filter(|e| e.enabled) {
            let Ok(source) = fs::read_to_string(&entry.path) else {
                continue;
            };
            match entry.kind {
                PluginKind::Rhai => plugins.add_plugin(&entry.name, &source),
                PluginKind::TextWrite => {
                    if let Some(dir) = entry.path.parent() {
                        scripts.set_module_dir(dir);
                    }
                    scripts.add_script(&entry.name, &source);
                }
            }
        }
        (plugins, scripts)
    }

    // --- Navigation --------------------------------------------------------

    /// Selects a row of the active list, clamping to it and keeping it on screen.
    pub fn select(&mut self, index: usize) {
        let len = self.list_len();
        let clamped = if len == 0 { 0 } else { index.min(len - 1) };
        match self.tab {
            ManagerTab::Installed => self.selected = clamped,
            ManagerTab::Available => self.avail_selected = clamped,
        }
        self.clamp_scroll();
    }

    /// Moves the selection of the active list by `delta` rows.
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let selected = self.list_selected() as isize + delta;
        let clamped = selected.clamp(0, len as isize - 1) as usize;
        match self.tab {
            ManagerTab::Installed => self.selected = clamped,
            ManagerTab::Available => self.avail_selected = clamped,
        }
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let height = self.view_height.max(1);
        let (selected, scroll) = match self.tab {
            ManagerTab::Installed => (&mut self.selected, &mut self.scroll),
            ManagerTab::Available => (&mut self.avail_selected, &mut self.avail_scroll),
        };
        let len = match self.tab {
            ManagerTab::Installed => self.entries.len(),
            ManagerTab::Available => self.available.len(),
        };
        if *selected < *scroll {
            *scroll = *selected;
        } else if *selected >= *scroll + height {
            *scroll = *selected + 1 - height;
        }
        let max = len.saturating_sub(height);
        *scroll = (*scroll).min(max);
    }

    /// Toggles or installs the selected row of the active list. Returns true
    /// when the hosts should be rebuilt.
    fn activate_selected(&mut self) -> bool {
        match self.tab {
            ManagerTab::Installed => self.toggle_selected(),
            ManagerTab::Available => self.install_selected(),
        }
    }

    /// Toggles or installs the row at `index` of the active list. Returns true
    /// when the hosts should be rebuilt.
    pub fn activate(&mut self, index: usize) -> bool {
        self.select(index);
        self.activate_selected()
    }

    /// Handles one key press while the overlay is open.
    pub fn handle_key(&mut self, key: KeyEvent) -> ManagerAction {
        // A pending delete is confirmed or cancelled before anything else.
        if let Some(index) = self.pending_delete.take() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) && self.delete(index) {
                return ManagerAction::Reload;
            }
            return ManagerAction::None;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return ManagerAction::Close,
            KeyCode::Tab => self.switch_tab(),
            KeyCode::BackTab => self.switch_tab(),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-(self.view_height as isize)),
            KeyCode::PageDown => self.move_selection(self.view_height as isize),
            KeyCode::Home => self.select(0),
            KeyCode::End => self.select(self.list_len().saturating_sub(1)),
            KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('x') => {
                if self.activate_selected() {
                    return ManagerAction::Reload;
                }
            }
            KeyCode::Char('i') => {
                // On the Available tab `i` installs the highlighted plugin;
                // elsewhere it opens the "install from a path" prompt.
                if self.tab == ManagerTab::Available {
                    if self.install_selected() {
                        return ManagerAction::Reload;
                    }
                } else {
                    return ManagerAction::Install;
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => self.request_delete(),
            KeyCode::Char('r') => self.rescan(),
            _ => {}
        }
        ManagerAction::None
    }

    // --- Mouse geometry ----------------------------------------------------

    /// Records the overlay's on-screen rectangles while it is drawn.
    pub fn set_area(&mut self, outer: Rect, list: Rect) {
        self.outer_area = outer;
        self.list_area = list;
    }

    /// Records how many rows the list shows.
    pub fn set_view_height(&mut self, height: usize) {
        self.view_height = height.max(1);
        self.clamp_scroll();
    }

    /// Whether a screen cell is inside the overlay (border included).
    pub fn contains(&self, column: u16, row: u16) -> bool {
        rect_contains(self.outer_area, column, row)
    }

    /// The index of the list row drawn at a screen cell, if any.
    pub fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        if !rect_contains(self.list_area, column, row) {
            return None;
        }
        let index = self.list_scroll() + (row - self.list_area.y) as usize;
        (index < self.list_len()).then_some(index)
    }

    /// Moves the active list by `delta` rows without moving the selection.
    pub fn scroll_by(&mut self, delta: isize) {
        let height = self.view_height.max(1);
        let len = self.list_len();
        let max = len.saturating_sub(height) as isize;
        match self.tab {
            ManagerTab::Installed => {
                self.scroll = (self.scroll as isize + delta).clamp(0, max) as usize
            }
            ManagerTab::Available => {
                self.avail_scroll = (self.avail_scroll as isize + delta).clamp(0, max) as usize
            }
        }
    }
}

/// Whether a cell lies inside a rectangle.
fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && column >= rect.x
        && column < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}

/// Loads one plugin file in isolation to learn what it registers.
fn inspect(path: PathBuf, kind: PluginKind, name: String) -> PluginEntry {
    let mut entry = PluginEntry {
        name,
        path,
        kind,
        enabled: true,
        description: String::new(),
        commands: Vec::new(),
        keybindings: Vec::new(),
        error: None,
    };
    let Ok(source) = fs::read_to_string(&entry.path) else {
        entry.error = Some("cannot read file".to_string());
        return entry;
    };
    entry.description = describe(&source);
    match kind {
        PluginKind::Rhai => {
            let mut host = PluginHost::new();
            host.add_plugin(&entry.name, &source);
            entry.commands = host.commands();
            entry.keybindings = host.keybindings();
            entry.error = host.errors.into_iter().next();
        }
        PluginKind::TextWrite => {
            let mut host = ScriptHost::new();
            if let Some(dir) = entry.path.parent() {
                host.set_module_dir(dir);
            }
            host.add_script(&entry.name, &source);
            entry.commands = host.commands();
            entry.keybindings = host.keybindings();
            entry.error = host.errors.into_iter().next();
        }
    }
    entry
}

/// The first comment line of a script, used as its description.
fn describe(source: &str) -> String {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("//")
            .or_else(|| trimmed.strip_prefix('#'))
        {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        } else {
            break;
        }
    }
    String::new()
}

/// Reads the disabled-plugin set from disk.
fn read_disabled(path: &Path) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    let Ok(text) = fs::read_to_string(path) else {
        return set;
    };
    for line in text.lines() {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('#') {
            set.insert(line.to_string());
        }
    }
    set
}

/// Writes the disabled-plugin set to disk, creating the parent directory.
fn write_disabled(path: &Path, set: &BTreeSet<String>) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut text =
        String::from("# Fast Text plugins that are turned off, one file name per line.\n");
    for name in set {
        text.push_str(name);
        text.push('\n');
    }
    let _ = fs::write(path, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A throwaway directory that removes itself when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("ft-plugins-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    const CURSOR_TW: &str = "# Cursor styles for Fast Text.\nfn bar() { set_cursor_style(\"bar\") }\nfn block() { set_cursor_style(\"block\") }\ncommand_with_desc(\"TextWrite: Cursor bar\", \"bar\", \"bar\")\ncommand_with_desc(\"TextWrite: Cursor block\", \"block\", \"block\")\nbind(\"alt+1\", \"TextWrite: Cursor bar\")\n";

    #[test]
    fn discovers_plugins_and_their_details() {
        let dir = TempDir::new();
        write(dir.path(), "cursor.tw", CURSOR_TW);
        write(dir.path(), "notes.txt", "ignored");

        let manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        assert_eq!(manager.entries.len(), 1, "only .tw/.rhai files count");
        let entry = &manager.entries[0];
        assert_eq!(entry.name, "cursor.tw");
        assert_eq!(entry.kind, PluginKind::TextWrite);
        assert_eq!(entry.description, "Cursor styles for Fast Text.");
        assert_eq!(entry.commands.len(), 2);
        assert_eq!(entry.keybindings.len(), 1);
        assert_eq!(entry.keybindings[0].0, KeySpec::parse("alt+1").unwrap());
        assert!(entry.enabled);
        assert_eq!(manager.enabled_count(), 1);
    }

    #[test]
    fn toggling_persists_to_the_disabled_file() {
        let dir = TempDir::new();
        let state = dir.path().join("state");
        let config = state.join("plugins.disabled");
        write(dir.path(), "cursor.tw", CURSOR_TW);

        let mut manager =
            PluginManager::discover(vec![dir.path().to_path_buf()], Some(config.clone()));
        assert!(manager.toggle_selected());
        assert!(!manager.entries[0].enabled);
        assert_eq!(manager.enabled_count(), 0);

        // Reloading from the same state file remembers the choice.
        let reloaded = PluginManager::discover(vec![dir.path().to_path_buf()], Some(config));
        assert!(!reloaded.entries[0].enabled);

        // And turning it back on clears the entry.
        manager.toggle_selected();
        assert!(manager.entries[0].enabled);
        let text = fs::read_to_string(dir.path().join("state/plugins.disabled")).unwrap();
        assert!(!text.contains("cursor.tw"));
    }

    #[test]
    fn build_hosts_only_loads_enabled_plugins() {
        let dir = TempDir::new();
        write(dir.path(), "cursor.tw", CURSOR_TW);
        write(
            dir.path(),
            "extra.rhai",
            "fn hello() { message(\"hi\"); }\ncommand(\"Hello\", \"hello\");\n",
        );

        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        let (_, scripts) = manager.build_hosts();
        assert!(scripts.has_command("TextWrite: Cursor bar"));

        // Disabling the TextWrite plugin removes its commands from the hosts.
        let index = manager
            .entries
            .iter()
            .position(|e| e.name == "cursor.tw")
            .unwrap();
        manager.toggle(index);
        let (plugins, scripts) = manager.build_hosts();
        assert!(!scripts.has_command("TextWrite: Cursor bar"));
        assert!(plugins.command_titles().iter().any(|t| t == "Hello"));
    }

    #[test]
    fn reports_a_broken_plugin() {
        let dir = TempDir::new();
        write(dir.path(), "broken.tw", "fn oops( { this is not valid\n");
        let manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        assert!(manager.entries[0].error.is_some());
    }

    #[test]
    fn catalog_starts_fully_available() {
        let dir = TempDir::new();
        let manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        assert!(manager.entries.is_empty());
        assert_eq!(manager.available().len(), CATALOG.len());
        assert_eq!(manager.tab(), ManagerTab::Installed);
    }

    #[test]
    fn installing_writes_the_file_and_removes_it_from_the_catalog() {
        let dir = TempDir::new();
        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);

        // themes.tw is first in the catalog.
        let written = manager.install(0).unwrap();
        assert_eq!(written, "themes.tw");
        let path = dir.path().join("themes.tw");
        assert!(path.is_file(), "the plugin was written to disk");

        // It is now installed and no longer offered.
        assert!(manager.entries.iter().any(|e| e.name == "themes.tw"));
        assert!(!manager
            .available()
            .iter()
            .any(|p| p.file_name == "themes.tw"));
        assert_eq!(manager.available().len(), CATALOG.len() - 1);

        // And its commands load through the real host.
        let (_, scripts) = manager.build_hosts();
        assert!(scripts.has_command("Theme: Monokai"));
        assert!(scripts.errors.is_empty(), "{:?}", scripts.errors);
    }

    #[test]
    fn tab_switches_lists_and_enter_installs() {
        let dir = TempDir::new();
        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        let action = manager.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(action, ManagerAction::None);
        assert_eq!(manager.tab(), ManagerTab::Available);

        // Enter installs the highlighted catalog plugin.
        let action = manager.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(action, ManagerAction::Reload);
        assert_eq!(manager.entries.len(), 1);
        let notice = manager.take_notice().unwrap();
        assert!(notice.starts_with("Installed "), "{notice}");
        assert!(manager.take_notice().is_none(), "notice is one-shot");
    }

    #[test]
    fn deleting_is_disabled_on_the_available_tab() {
        let dir = TempDir::new();
        write(dir.path(), "cursor.tw", CURSOR_TW);
        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        manager.switch_tab();
        assert_eq!(manager.tab(), ManagerTab::Available);
        // `d` must not delete an installed plugin from the other tab.
        assert_eq!(
            manager.handle_key(KeyEvent::from(KeyCode::Char('d'))),
            ManagerAction::None
        );
        assert!(manager.entries[0].path.is_file());

        // On the Installed tab it asks for confirmation first.
        manager.switch_tab();
        assert_eq!(manager.tab(), ManagerTab::Installed);
        assert_eq!(
            manager.handle_key(KeyEvent::from(KeyCode::Char('d'))),
            ManagerAction::None
        );
        assert!(manager.pending_delete_name().is_some());
        let action = manager.handle_key(KeyEvent::from(KeyCode::Char('y')));
        assert_eq!(action, ManagerAction::Reload);
        assert!(manager.entries.is_empty());
    }

    #[test]
    fn install_error_is_reported_for_an_unwritable_target() {
        let dir = TempDir::new();
        // A plugin directory that is really a file cannot be written into.
        let file = dir.path().join("not-a-dir");
        fs::write(&file, "x").unwrap();
        let mut manager = PluginManager::discover(vec![file], None);
        let err = manager.install(0).unwrap_err();
        assert!(err.contains("cannot create"), "{err}");
        assert!(manager.install_error().is_some());
    }

    #[test]
    fn key_navigation_and_toggle() {
        let dir = TempDir::new();
        write(dir.path(), "a.tw", "command(\"A\", \"a\")\nfn a() {}\n");
        write(dir.path(), "b.tw", "command(\"B\", \"b\")\nfn b() {}\n");
        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        assert_eq!(manager.selected, 0);

        manager.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(manager.selected, 1);
        let action = manager.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        assert_eq!(action, ManagerAction::Reload);
        assert!(!manager.entries[1].enabled);
        assert_eq!(
            manager.handle_key(KeyEvent::from(KeyCode::Esc)),
            ManagerAction::Close
        );
    }

    #[test]
    fn delete_asks_for_confirmation() {
        let dir = TempDir::new();
        write(dir.path(), "a.tw", "fn a() {}\ncommand(\"A\", \"a\")\n");
        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);

        manager.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert_eq!(manager.pending_delete_name(), Some("a.tw"));
        // Any key but `y` cancels, leaving the file alone.
        manager.handle_key(KeyEvent::from(KeyCode::Char('n')));
        assert_eq!(manager.pending_delete_name(), None);
        assert!(dir.path().join("a.tw").exists());

        manager.handle_key(KeyEvent::from(KeyCode::Char('d')));
        let action = manager.handle_key(KeyEvent::from(KeyCode::Char('y')));
        assert_eq!(action, ManagerAction::Reload);
        assert!(!dir.path().join("a.tw").exists());
        assert!(manager.entries.is_empty());
    }

    #[test]
    fn install_key_asks_the_app() {
        let dir = TempDir::new();
        write(dir.path(), "a.tw", "fn a() {}\ncommand(\"A\", \"a\")\n");
        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        assert_eq!(
            manager.handle_key(KeyEvent::from(KeyCode::Char('i'))),
            ManagerAction::Install
        );
        // The first scanned directory is where new plugins go.
        assert_eq!(manager.primary_dir(), Some(dir.path()));
    }

    #[test]
    fn row_at_maps_screen_cells() {
        let dir = TempDir::new();
        write(dir.path(), "a.tw", "command(\"A\", \"a\")\nfn a() {}\n");
        write(dir.path(), "b.tw", "command(\"B\", \"b\")\nfn b() {}\n");
        let mut manager = PluginManager::discover(vec![dir.path().to_path_buf()], None);
        manager.set_area(Rect::new(0, 0, 40, 12), Rect::new(1, 2, 20, 8));
        manager.set_view_height(8);
        assert_eq!(manager.row_at(2, 2), Some(0));
        assert_eq!(manager.row_at(2, 3), Some(1));
        assert_eq!(manager.row_at(2, 4), None);
        assert!(manager.contains(0, 0));
        assert!(!manager.contains(0, 12));
    }
}
