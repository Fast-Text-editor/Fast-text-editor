//! A Visual-Studio-style folder explorer for the left edge of the editor.
//!
//! The sidebar shows a single root folder as an expandable tree. Directories
//! come first, then files, each sorted case-insensitively. It is keyboard
//! driven: arrows move and expand, `Enter` opens a file, `Esc` hands focus back
//! to the editor.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

/// Largest number of rows the tree will ever hold (guards against huge trees).
const MAX_ENTRIES: usize = 5000;
/// Deepest nesting the tree will walk.
const MAX_DEPTH: usize = 64;
/// Default assumed height until the first draw reports the real one.
const DEFAULT_HEIGHT: usize = 20;

/// One visible row in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    /// Nesting level; `0` is a direct child of the root.
    pub depth: usize,
    pub is_dir: bool,
    /// For directories: whether its children are shown.
    pub expanded: bool,
}

/// What the sidebar wants the editor to do after a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarAction {
    /// The key was handled; nothing else to do.
    None,
    /// Open this file in the editor.
    Open(PathBuf),
    /// Move focus back to the editor, leaving the sidebar visible.
    Unfocus,
}

/// What the inline "new" prompt will create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateKind {
    File,
    Folder,
}

/// The inline new-file / new-folder prompt at the bottom of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CreatePrompt {
    kind: CreateKind,
    input: String,
    /// A rejected name or a failed creation, shown next to the input.
    error: Option<String>,
}

impl CreatePrompt {
    fn new(kind: CreateKind) -> Self {
        Self {
            kind,
            input: String::new(),
            error: None,
        }
    }

    fn label(&self) -> &'static str {
        match self.kind {
            CreateKind::File => "New file",
            CreateKind::Folder => "New folder",
        }
    }

    fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// The folder explorer state.
pub struct Sidebar {
    root: PathBuf,
    visible: bool,
    focused: bool,
    /// Whether dotfiles are listed.
    show_hidden: bool,
    /// Directories whose children are shown.
    expanded: HashSet<PathBuf>,
    /// The flattened, visible tree, top to bottom.
    entries: Vec<Entry>,
    selected: usize,
    scroll: usize,
    view_height: usize,
    error: Option<String>,
    /// The inline new-file / new-folder prompt, while it is open.
    create_prompt: Option<CreatePrompt>,
    /// Screen rectangle of the whole panel (set while drawing).
    outer_area: Rect,
    /// Screen rectangle of the list inside the border (set while drawing).
    list_area: Rect,
}

impl Sidebar {
    /// Creates a sidebar rooted at `root` (already refreshed).
    pub fn new(root: PathBuf) -> Self {
        let mut sidebar = Self {
            root,
            visible: false,
            focused: false,
            show_hidden: false,
            expanded: HashSet::new(),
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            view_height: DEFAULT_HEIGHT,
            error: None,
            create_prompt: None,
            outer_area: Rect::default(),
            list_area: Rect::default(),
        };
        sidebar.refresh();
        sidebar
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The name shown in the panel's title bar (just the folder's name).
    pub fn title(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    /// Shows or hides dot-files (used by `config.toml`).
    pub fn set_show_hidden(&mut self, show: bool) {
        if self.show_hidden != show {
            self.show_hidden = show;
            self.refresh();
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether the inline new-file / new-folder prompt is open.
    pub fn is_creating(&self) -> bool {
        self.create_prompt.is_some()
    }

    /// The prompt's label ("New file" / "New folder"), if it is open.
    pub fn create_label(&self) -> Option<&'static str> {
        self.create_prompt.as_ref().map(CreatePrompt::label)
    }

    /// What has been typed into the prompt so far.
    pub fn create_input(&self) -> &str {
        self.create_prompt
            .as_ref()
            .map(|p| p.input.as_str())
            .unwrap_or("")
    }

    /// A rejected name or failed creation, shown next to the input.
    pub fn create_error(&self) -> Option<&str> {
        self.create_prompt.as_ref().and_then(|p| p.error.as_deref())
    }

    /// Records how many rows are on screen (used to keep the selection visible).
    pub fn set_view_height(&mut self, height: usize) {
        self.view_height = height.max(1);
        self.clamp_scroll();
    }

    /// Records the panel's on-screen rectangles while it is being drawn.
    pub fn set_area(&mut self, outer: Rect, list: Rect) {
        self.outer_area = outer;
        self.list_area = list;
    }

    /// Whether a screen cell is inside the panel, border included.
    pub fn contains(&self, column: u16, row: u16) -> bool {
        rect_contains(self.outer_area, column, row)
    }

    /// The index of the tree row drawn at a screen cell, if any.
    pub fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        if !rect_contains(self.list_area, column, row) {
            return None;
        }
        let index = self.scroll + (row - self.list_area.y) as usize;
        (index < self.entries.len()).then_some(index)
    }

    /// Selects an entry by index (a mouse click).
    pub fn select_index(&mut self, index: usize) {
        self.select(index);
    }

    /// Selects an entry and performs its action, as a click does.
    pub fn activate_index(&mut self, index: usize) -> SidebarAction {
        self.select(index);
        self.activate()
    }

    /// Moves the selection by `delta` rows (the scroll wheel).
    pub fn scroll_by(&mut self, delta: isize) {
        self.move_selection(delta);
    }

    /// Points the sidebar at a new root, collapsing everything.
    pub fn set_root(&mut self, root: PathBuf) {
        self.root = root;
        self.expanded.clear();
        self.selected = 0;
        self.scroll = 0;
        self.refresh();
    }

    /// Makes the sidebar visible and keyboard-focused, refreshing the tree.
    pub fn show(&mut self) {
        self.visible = true;
        self.focused = true;
        self.refresh();
    }

    /// Hides the sidebar and returns focus to the editor.
    pub fn hide(&mut self) {
        self.visible = false;
        self.focused = false;
    }

    pub fn focus(&mut self) {
        self.focused = true;
        self.refresh();
    }

    pub fn unfocus(&mut self) {
        self.focused = false;
    }

    /// Rebuilds the flattened tree, keeping the selection on the same path.
    pub fn refresh(&mut self) {
        let keep = self.entries.get(self.selected).map(|e| e.path.clone());
        let mut entries = Vec::new();
        self.error = None;
        if !self.root.is_dir() {
            self.error = Some(format!("not a folder: {}", self.root.display()));
        } else {
            let mut budget = MAX_ENTRIES;
            self.collect(&self.root.clone(), 0, &mut entries, &mut budget);
        }
        self.entries = entries;
        // Keep the selection on the same path when possible.
        self.selected = match keep.and_then(|p| self.entries.iter().position(|e| e.path == p)) {
            Some(i) => i,
            None => self.selected.min(self.entries.len().saturating_sub(1)),
        };
        self.clamp_scroll();
    }

    fn collect(&self, dir: &Path, depth: usize, out: &mut Vec<Entry>, budget: &mut usize) {
        if depth > MAX_DEPTH || *budget == 0 {
            return;
        }
        let Ok(reader) = std::fs::read_dir(dir) else {
            return;
        };
        let mut items: Vec<(bool, String, PathBuf)> = Vec::new();
        for entry in reader.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            items.push((is_dir, name, entry.path()));
        }
        // Directories first, then case-insensitive by name.
        items.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });

        for (is_dir, name, path) in items {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            let expanded = is_dir && self.expanded.contains(&path);
            out.push(Entry {
                path: path.clone(),
                name,
                depth,
                is_dir,
                expanded,
            });
            if expanded {
                self.collect(&path, depth + 1, out, budget);
            }
        }
    }

    /// Handles one key press, returning what (if anything) the app should do.
    pub fn handle_key(&mut self, key: KeyEvent) -> SidebarAction {
        // While the new-file / new-folder prompt is open it owns the keyboard.
        if self.create_prompt.is_some() {
            return self.handle_create_key(key);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl {
            return SidebarAction::None;
        }
        let page = self.view_height.max(1);
        match key.code {
            KeyCode::Esc => return SidebarAction::Unfocus,
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Home => self.select(0),
            KeyCode::End => self.select(self.entries.len().saturating_sub(1)),
            KeyCode::PageUp => self.move_selection(-(page as isize)),
            KeyCode::PageDown => self.move_selection(page as isize),
            KeyCode::Left => self.collapse_or_parent(),
            KeyCode::Right => self.expand_or_child(),
            KeyCode::Enter => return self.activate(),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('.') => {
                self.show_hidden = !self.show_hidden;
                self.refresh();
            }
            // `n` / `N` create a file / folder next to the selection, like the
            // buttons at the top of the Visual Studio Code explorer.
            KeyCode::Char('n') => self.begin_create(CreateKind::File),
            KeyCode::Char('N') => self.begin_create(CreateKind::Folder),
            _ => {}
        }
        SidebarAction::None
    }

    /// Opens the inline prompt for a new file or folder.
    fn begin_create(&mut self, kind: CreateKind) {
        self.error = None;
        self.create_prompt = Some(CreatePrompt::new(kind));
    }

    /// Handles a key press while the create prompt is open.
    fn handle_create_key(&mut self, key: KeyEvent) -> SidebarAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if ctrl || alt {
            return SidebarAction::None;
        }
        match key.code {
            // Escape closes the prompt and keeps the explorer focused.
            KeyCode::Esc => self.create_prompt = None,
            KeyCode::Enter => return self.commit_create(),
            KeyCode::Backspace => {
                if let Some(prompt) = self.create_prompt.as_mut() {
                    prompt.input.pop();
                    prompt.error = None;
                }
            }
            KeyCode::Char(c) => {
                if let Some(prompt) = self.create_prompt.as_mut() {
                    prompt.input.push(c);
                    prompt.error = None;
                }
            }
            _ => {}
        }
        SidebarAction::None
    }

    /// Creates what the prompt describes and selects (or opens) the result.
    fn commit_create(&mut self) -> SidebarAction {
        let Some(mut prompt) = self.create_prompt.take() else {
            return SidebarAction::None;
        };
        let name = prompt.input.trim().to_string();
        // Names are single path components: no separators, no dot segments,
        // nothing that could climb out of the explorer's root.
        let valid = !name.is_empty()
            && !name.contains('/')
            && !name.contains('\\')
            && name != "."
            && name != "..";
        if !valid {
            self.create_prompt = Some(prompt.with_error("enter a plain name"));
            return SidebarAction::None;
        }
        let base = self.create_target();
        let path = base.join(&name);
        match prompt.kind {
            CreateKind::Folder => {
                // `create_dir` (not `create_dir_all`) so an existing name is
                // an error rather than a silent success.
                if let Err(e) = std::fs::create_dir(&path) {
                    prompt.error = Some(e.to_string());
                    self.create_prompt = Some(prompt);
                    return SidebarAction::None;
                }
                // Reveal the new folder: expand its parents, then select it.
                self.expanded.insert(base.clone());
                self.refresh();
                self.select_path(&path);
                SidebarAction::None
            }
            CreateKind::File => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(_) => {
                        self.expanded.insert(base);
                        self.refresh();
                        self.select_path(&path);
                        // Files open in the editor, like activating the row.
                        SidebarAction::Open(path)
                    }
                    Err(e) => {
                        prompt.error = Some(e.to_string());
                        self.create_prompt = Some(prompt);
                        SidebarAction::None
                    }
                }
            }
        }
    }

    /// The folder new entries are created in: the highlighted folder, the
    /// highlighted file's folder, or the explorer's root.
    fn create_target(&self) -> PathBuf {
        match self.entries.get(self.selected) {
            Some(e) if e.is_dir => e.path.clone(),
            Some(e) => e
                .path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.root.clone()),
            None => self.root.clone(),
        }
    }

    /// Moves the selection to the row for `path`, if it is visible.
    fn select_path(&mut self, path: &Path) {
        if let Some(index) = self.entries.iter().position(|e| e.path == path) {
            self.select(index);
        }
    }

    /// Opens the selected file, or toggles the selected directory.
    fn activate(&mut self) -> SidebarAction {
        match self.entries.get(self.selected) {
            Some(e) if e.is_dir => {
                self.toggle_expand();
                SidebarAction::None
            }
            Some(e) => SidebarAction::Open(e.path.clone()),
            None => SidebarAction::None,
        }
    }

    fn select(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = index.min(self.entries.len() - 1);
        self.clamp_scroll();
    }

    /// Adds `delta` to the selection, clamped to the tree.
    fn move_selection(&mut self, delta: isize) {
        let len = self.entries.len();
        if len == 0 {
            return;
        }
        let next = (self.selected as isize + delta).clamp(0, len as isize - 1) as usize;
        self.selected = next;
        self.clamp_scroll();
    }

    /// Keeps the selection within the viewport and the viewport within the tree.
    fn clamp_scroll(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + self.view_height {
            self.scroll = self.selected + 1 - self.view_height.max(1);
        }
        let max = self.entries.len().saturating_sub(self.view_height);
        self.scroll = self.scroll.min(max);
    }

    /// `Left`: collapse an expanded directory, else jump to the parent.
    fn collapse_or_parent(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if entry.is_dir && entry.expanded {
            self.expanded.remove(&entry.path);
            self.refresh();
            return;
        }
        if entry.depth == 0 {
            return;
        }
        if let Some(parent) = (0..self.selected)
            .rev()
            .find(|&i| self.entries[i].depth < entry.depth)
        {
            self.selected = parent;
            self.clamp_scroll();
        }
    }

    /// `Right`: expand a collapsed directory, else step into the first child.
    fn expand_or_child(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if !entry.is_dir {
            return;
        }
        if !entry.expanded {
            self.expanded.insert(entry.path);
            self.refresh();
            return;
        }
        if let Some(child) = self.entries.get(self.selected + 1) {
            if child.depth > entry.depth {
                self.selected += 1;
                self.clamp_scroll();
            }
        }
    }

    /// Collapses an expanded directory or expands a collapsed one.
    fn toggle_expand(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if !entry.is_dir {
            return;
        }
        if entry.expanded {
            self.expanded.remove(&entry.path);
        } else {
            self.expanded.insert(entry.path);
        }
        self.refresh();
    }
}

/// Whether a cell lies inside a rectangle (empty rectangles contain nothing).
fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && column >= rect.x
        && column < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A throwaway folder tree, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ft-sidebar-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn file(&self, rel: &str) {
            let path = self.0.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, "").unwrap();
        }

        fn dir(&self, rel: &str) {
            std::fs::create_dir_all(self.0.join(rel)).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Populates a tree with two folders, some files, and a dotfile.
    fn build_tree(tree: &TempDir) {
        tree.dir("src");
        tree.file("src/main.rs");
        tree.file("README.md");
        tree.file("zeta.txt");
        tree.file(".hidden");
    }

    fn names(sidebar: &Sidebar) -> Vec<String> {
        sidebar.entries().iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn dirs_sort_before_files_case_insensitively() {
        let tree = TempDir::new();
        build_tree(&tree);
        let sidebar = Sidebar::new(tree.path().to_path_buf());
        assert_eq!(names(&sidebar), vec!["src", "README.md", "zeta.txt"]);
    }

    #[test]
    fn expanding_and_collapsing_a_directory() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());

        // Right expands the highlighted directory.
        sidebar.handle_key(key(KeyCode::Right));
        assert_eq!(
            names(&sidebar),
            vec!["src", "main.rs", "README.md", "zeta.txt"]
        );
        // Right again steps into its first child.
        sidebar.handle_key(key(KeyCode::Right));
        assert_eq!(sidebar.entries()[sidebar.selected()].name, "main.rs");
        // Left jumps back to the parent...
        sidebar.handle_key(key(KeyCode::Left));
        assert_eq!(sidebar.entries()[sidebar.selected()].name, "src");
        // ...and Left again collapses it.
        sidebar.handle_key(key(KeyCode::Left));
        assert_eq!(names(&sidebar), vec!["src", "README.md", "zeta.txt"]);
    }

    #[test]
    fn enter_opens_the_selected_file() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.handle_key(key(KeyCode::Down)); // src -> README.md
        match sidebar.handle_key(key(KeyCode::Enter)) {
            SidebarAction::Open(path) => assert_eq!(path, tree.path().join("README.md")),
            other => panic!("expected Open, got {other:?}"),
        }
    }

    #[test]
    fn enter_on_a_directory_expands_it() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        assert_eq!(sidebar.entries().len(), 3);
        sidebar.handle_key(key(KeyCode::Enter));
        assert_eq!(sidebar.entries().len(), 4);
    }

    #[test]
    fn hidden_files_are_opt_in() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        assert!(!sidebar.entries().iter().any(|e| e.name == ".hidden"));
        sidebar.handle_key(key(KeyCode::Char('.')));
        assert!(sidebar.show_hidden());
        assert!(sidebar.entries().iter().any(|e| e.name == ".hidden"));
    }

    #[test]
    fn escape_requests_unfocus() {
        let tree = TempDir::new();
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.show();
        assert!(sidebar.focused());
        assert_eq!(
            sidebar.handle_key(key(KeyCode::Esc)),
            SidebarAction::Unfocus
        );
    }

    #[test]
    fn missing_root_is_reported() {
        let tree = TempDir::new();
        let missing = tree.path().join("nope");
        let sidebar = Sidebar::new(missing);
        assert!(sidebar.entries().is_empty());
        assert!(sidebar.error().is_some());
    }

    #[test]
    fn row_at_maps_screen_cells_to_entries() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.set_area(Rect::new(0, 0, 20, 10), Rect::new(1, 1, 18, 8));
        sidebar.set_view_height(8);

        assert!(sidebar.contains(0, 0));
        assert_eq!(sidebar.row_at(2, 1), Some(0)); // src
        assert_eq!(sidebar.row_at(2, 2), Some(1)); // README.md
        assert_eq!(sidebar.row_at(2, 3), Some(2)); // zeta.txt
        assert_eq!(sidebar.row_at(2, 4), None, "below the last entry");
        assert_eq!(sidebar.row_at(0, 1), None, "the border column");
        assert_eq!(sidebar.row_at(2, 0), None, "the title row");
    }

    #[test]
    fn clicking_a_file_row_opens_it() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.set_area(Rect::new(0, 0, 20, 10), Rect::new(1, 1, 18, 8));
        sidebar.set_view_height(8);

        let index = sidebar.row_at(2, 2).unwrap();
        match sidebar.activate_index(index) {
            SidebarAction::Open(path) => assert_eq!(path, tree.path().join("README.md")),
            other => panic!("expected Open, got {other:?}"),
        }
    }

    #[test]
    fn clicking_a_directory_row_toggles_it() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.set_area(Rect::new(0, 0, 20, 10), Rect::new(1, 1, 18, 8));
        sidebar.set_view_height(8);

        let index = sidebar.row_at(2, 1).unwrap();
        assert_eq!(sidebar.activate_index(index), SidebarAction::None);
        assert_eq!(sidebar.entries().len(), 4, "src should have expanded");
    }

    // ------------------------------------------------------------------
    // The inline new-file / new-folder prompt
    // ------------------------------------------------------------------

    /// Types `text` into the open create prompt, key by key.
    fn type_name(sidebar: &mut Sidebar, text: &str) {
        for c in text.chars() {
            sidebar.handle_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn n_opens_the_new_file_prompt_and_esc_closes_it() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());

        sidebar.handle_key(key(KeyCode::Char('n')));
        assert!(sidebar.is_creating());
        assert_eq!(sidebar.create_label(), Some("New file"));
        assert_eq!(sidebar.create_input(), "");

        type_name(&mut sidebar, "notes.txt");
        assert_eq!(sidebar.create_input(), "notes.txt");
        // Typing in the prompt must not reach the explorer's rows.
        assert_eq!(names(&sidebar), vec!["src", "README.md", "zeta.txt"]);

        sidebar.handle_key(key(KeyCode::Esc));
        assert!(!sidebar.is_creating());
        assert!(
            !tree.path().join("notes.txt").exists(),
            "Escape must abandon the pending file"
        );
    }

    #[test]
    fn capital_n_opens_the_new_folder_prompt() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());

        sidebar.handle_key(key(KeyCode::Char('N')));
        assert!(sidebar.is_creating());
        assert_eq!(sidebar.create_label(), Some("New folder"));
    }

    #[test]
    fn creating_a_file_creates_it_opens_it_and_selects_it() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.handle_key(key(KeyCode::Down)); // -> README.md, so the root

        sidebar.handle_key(key(KeyCode::Char('n')));
        type_name(&mut sidebar, "notes.txt");
        match sidebar.handle_key(key(KeyCode::Enter)) {
            SidebarAction::Open(path) => assert_eq!(path, tree.path().join("notes.txt")),
            other => panic!("expected Open, got {other:?}"),
        }
        assert!(tree.path().join("notes.txt").is_file());
        assert!(!sidebar.is_creating(), "the prompt closes on success");
        assert_eq!(sidebar.entries()[sidebar.selected()].name, "notes.txt");
    }

    #[test]
    fn creating_a_file_inside_the_highlighted_folder() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());

        // The highlighted row is the `src` directory: the file lands in it.
        sidebar.handle_key(key(KeyCode::Char('n')));
        type_name(&mut sidebar, "lib.rs");
        match sidebar.handle_key(key(KeyCode::Enter)) {
            SidebarAction::Open(path) => assert_eq!(path, tree.path().join("src/lib.rs")),
            other => panic!("expected Open, got {other:?}"),
        }
        assert!(tree.path().join("src/lib.rs").is_file());
        assert_eq!(sidebar.entries()[sidebar.selected()].name, "lib.rs");
    }

    #[test]
    fn creating_a_file_next_to_a_highlighted_file() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());

        // Highlight src/main.rs: the new file joins it in src/.
        sidebar.handle_key(key(KeyCode::Right)); // expand src
        sidebar.handle_key(key(KeyCode::Down)); // -> main.rs
        assert_eq!(sidebar.entries()[sidebar.selected()].name, "main.rs");
        sidebar.handle_key(key(KeyCode::Char('n')));
        type_name(&mut sidebar, "helper.rs");
        match sidebar.handle_key(key(KeyCode::Enter)) {
            SidebarAction::Open(path) => assert_eq!(path, tree.path().join("src/helper.rs")),
            other => panic!("expected Open, got {other:?}"),
        }
        assert!(tree.path().join("src/helper.rs").is_file());
    }

    #[test]
    fn creating_a_folder_selects_it_without_opening() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.handle_key(key(KeyCode::Down)); // -> README.md, so the root

        sidebar.handle_key(key(KeyCode::Char('N')));
        type_name(&mut sidebar, "docs");
        assert_eq!(
            sidebar.handle_key(key(KeyCode::Enter)),
            SidebarAction::None,
            "folders are selected, not opened"
        );
        assert!(tree.path().join("docs").is_dir());
        assert!(!sidebar.is_creating());
        assert_eq!(sidebar.entries()[sidebar.selected()].name, "docs");
    }

    #[test]
    fn invalid_names_are_rejected_and_keep_the_prompt_open() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());

        for bad in ["", "a/b", ".", ".."] {
            sidebar.handle_key(key(KeyCode::Char('n')));
            type_name(&mut sidebar, bad);
            assert_eq!(sidebar.handle_key(key(KeyCode::Enter)), SidebarAction::None);
            assert!(sidebar.is_creating(), "{bad:?} must keep the prompt open");
            assert!(sidebar.create_error().is_some(), "{bad:?} must be reported");
            assert_eq!(
                names(&sidebar),
                vec!["src", "README.md", "zeta.txt"],
                "{bad:?} must not have been created"
            );
            sidebar.handle_key(key(KeyCode::Esc));
        }
    }

    #[test]
    fn backspace_edits_the_input_and_clears_the_error() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());

        sidebar.handle_key(key(KeyCode::Char('n')));
        type_name(&mut sidebar, "a/b");
        sidebar.handle_key(key(KeyCode::Enter));
        assert!(sidebar.create_error().is_some());

        // One Backspace removes one character and clears the error.
        sidebar.handle_key(key(KeyCode::Backspace));
        assert_eq!(sidebar.create_input(), "a/");
        assert_eq!(sidebar.create_error(), None);
        sidebar.handle_key(key(KeyCode::Backspace));
        assert_eq!(sidebar.create_input(), "a");
    }

    #[test]
    fn an_existing_name_is_an_error_and_creates_nothing() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.handle_key(key(KeyCode::Down)); // -> README.md, so the root

        // A file that already exists.
        sidebar.handle_key(key(KeyCode::Char('n')));
        type_name(&mut sidebar, "README.md");
        assert_eq!(sidebar.handle_key(key(KeyCode::Enter)), SidebarAction::None);
        assert!(sidebar.is_creating());
        assert!(sidebar.create_error().is_some());
        sidebar.handle_key(key(KeyCode::Esc));

        // A folder that already exists.
        sidebar.handle_key(key(KeyCode::Char('N')));
        type_name(&mut sidebar, "src");
        assert_eq!(sidebar.handle_key(key(KeyCode::Enter)), SidebarAction::None);
        assert!(sidebar.is_creating());
        assert!(sidebar.create_error().is_some());
    }

    #[test]
    fn prompt_keys_do_not_leak_into_the_explorer() {
        let tree = TempDir::new();
        build_tree(&tree);
        let mut sidebar = Sidebar::new(tree.path().to_path_buf());
        sidebar.handle_key(key(KeyCode::Down)); // highlight README.md

        sidebar.handle_key(key(KeyCode::Char('n')));
        // Navigation keys are ignored while the prompt is open.
        sidebar.handle_key(key(KeyCode::Down));
        sidebar.handle_key(key(KeyCode::Up));
        sidebar.handle_key(key(KeyCode::Enter)); // rejected: empty name
        assert_eq!(sidebar.entries()[sidebar.selected()].name, "README.md");
        assert!(sidebar.is_creating());

        // Modifier chords never type into the prompt either.
        sidebar.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert_eq!(sidebar.create_input(), "");
    }
}
