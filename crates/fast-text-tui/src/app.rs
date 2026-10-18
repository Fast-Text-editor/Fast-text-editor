use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use fast_text_core::{Document, Position, SearchDirection};
use fast_text_lsp::{Diagnostic, LspRange, Severity, TextEdit, WorkspaceEdit};
use fast_text_plugin::{CursorStyle, EditorAction, EditorContext, KeyName, KeySpec, PluginHost};
use fast_text_syntax::{highlight_line, HighlightSpan, LanguageId, LineState};
use fast_text_textwrite::ScriptHost;
use fast_text_update::Release;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::config::Config;
use crate::find_in_files;
use crate::lsp::LspManager;
use crate::picker::{Picker, PickerItem, PickerOutcome};
use crate::plugin_manager::{ManagerAction, PluginManager};
use crate::sidebar::{Sidebar, SidebarAction};
use crate::theme::Theme;

/// A single open buffer plus its view state (scroll, highlight cache).
pub struct Editor {
    pub doc: Document,
    pub language: LanguageId,
    /// First visible line (0-based).
    pub scroll_line: usize,
    /// First visible column (in terminal cells).
    pub scroll_col: usize,
    /// `states[i]` is the highlight state at the start of line `i`.
    states: Vec<LineState>,
    /// Buffer revision the cache was built for.
    cached_rev: u64,
}

impl Editor {
    pub fn new(doc: Document, language: LanguageId) -> Self {
        Self {
            doc,
            language,
            scroll_line: 0,
            scroll_col: 0,
            states: vec![LineState::default()],
            cached_rev: u64::MAX,
        }
    }

    pub fn from_text(text: &str, language: LanguageId) -> Self {
        Self::new(Document::from_str(text), language)
    }

    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        let doc = Document::from_file(&path).map_err(|e| std::io::Error::other(e.to_string()))?;
        let language = LanguageId::from_path(&path.to_string_lossy());
        Ok(Self::new(doc, language))
    }

    /// Opens `path`, or — when the file does not exist yet — creates an empty
    /// buffer bound to it with the language inferred from its name. This is how
    /// `ft newfile.py` behaves (and `Ctrl+O` on a not-yet-created file), rather
    /// than falling back to the Rust welcome buffer.
    pub fn open_or_new(path: PathBuf) -> std::io::Result<Self> {
        if path.exists() {
            return Self::open(path);
        }
        let mut doc = Document::new();
        doc.set_path(path.clone());
        let language = LanguageId::from_path(&path.to_string_lossy());
        Ok(Self::new(doc, language))
    }

    /// Invalidate the highlight cache if the buffer changed.
    fn sync_cache(&mut self) {
        let rev = self.doc.buffer().revision();
        if rev != self.cached_rev {
            self.states.truncate(1);
            self.cached_rev = rev;
        }
    }

    /// Ensures highlight state is computed up to (and including) `line`.
    fn ensure_states(&mut self, line: usize) {
        self.sync_cache();
        let lang = self.language.language();
        let n_lines = self.doc.buffer().len_lines();
        let target = line.min(n_lines.saturating_sub(1));
        while self.states.len() <= target {
            let i = self.states.len() - 1;
            let mut state = self.states[i].clone();
            let text = self.doc.buffer().line_str(i);
            highlight_line(&lang, &text, &mut state);
            self.states.push(state);
        }
    }

    /// Returns highlight spans for a single line.
    pub fn highlights(&mut self, line: usize) -> Vec<HighlightSpan> {
        self.ensure_states(line);
        let mut state = self.states[line.min(self.states.len() - 1)].clone();
        let lang = self.language.language();
        let text = self.doc.buffer().line_str(line);
        highlight_line(&lang, &text, &mut state)
    }

    /// Switch languages, invalidating the highlight cache.
    pub fn set_language(&mut self, language: LanguageId) {
        if self.language != language {
            self.language = language;
            self.states.truncate(1);
            self.cached_rev = u64::MAX;
        }
    }

    pub fn title(&self) -> String {
        match self.doc.path() {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned()),
            None => "untitled".to_string(),
        }
    }

    pub fn is_untitled(&self) -> bool {
        self.doc.path().is_none()
    }
}

/// The active minibuffer prompt, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    None,
    OpenFile {
        input: String,
    },
    SaveAs {
        input: String,
    },
    Find {
        input: String,
    },
    Replace {
        find: String,
        replace: String,
        on_replace: bool,
    },
    GotoLine {
        input: String,
    },
    Language {
        input: String,
        selected: usize,
    },
    Command {
        input: String,
    },
    /// A `TextWrite: ` prompt that runs an automation file.
    Automation {
        input: String,
    },
    /// A project-wide literal search.
    FindInFiles {
        input: String,
    },
    /// New name for the symbol under the cursor (LSP rename).
    Rename {
        input: String,
    },
    /// Name to store the last recorded macro under.
    SaveMacro {
        input: String,
    },
    /// Evaluates a TextWrite snippet against the active buffer.
    Eval {
        input: String,
    },
    /// Chooses the folder shown in the explorer sidebar.
    OpenFolder {
        input: String,
    },
    /// Copies a plugin file into the user plugin directory.
    InstallPlugin {
        input: String,
    },
}

impl Prompt {
    pub fn is_active(&self) -> bool {
        !matches!(self, Prompt::None)
    }

    /// The label and current input shown in the minibuffer.
    pub fn render(&self) -> Option<(String, String)> {
        match self {
            Prompt::None => None,
            Prompt::OpenFile { input } => Some(("Open file: ".into(), input.clone())),
            Prompt::SaveAs { input } => Some(("Save as: ".into(), input.clone())),
            Prompt::Find { input } => Some(("Find: ".into(), input.clone())),
            Prompt::Replace {
                find,
                replace,
                on_replace,
            } => Some((
                if *on_replace {
                    "Replace with: ".into()
                } else {
                    "Replace: ".into()
                },
                if *on_replace {
                    replace.clone()
                } else {
                    find.clone()
                },
            )),
            Prompt::GotoLine { input } => Some(("Go to line: ".into(), input.clone())),
            Prompt::Language { input, .. } => Some(("Language: ".into(), input.clone())),
            Prompt::Command { input } => Some(("Command: ".into(), input.clone())),
            Prompt::Automation { input } => Some(("Run automation: ".into(), input.clone())),
            Prompt::Eval { input } => Some(("TextWrite: ".into(), input.clone())),
            Prompt::OpenFolder { input } => Some(("Open folder: ".into(), input.clone())),
            Prompt::FindInFiles { input } => Some(("Find in files: ".into(), input.clone())),
            Prompt::Rename { input } => Some(("Rename symbol to: ".into(), input.clone())),
            Prompt::SaveMacro { input } => Some(("Save macro as: ".into(), input.clone())),
            Prompt::InstallPlugin { input } => {
                Some(("Install plugin (path): ".into(), input.clone()))
            }
        }
    }
}

/// An open completion popup.
pub struct CompletionState {
    pub items: Vec<fast_text_lsp::CompletionItem>,
    pub selected: usize,
    /// Character index where the identifier under the cursor begins.
    pub prefix_start: usize,
}

/// Command palette entries handled directly by the editor, with no plugin.
const BUILTIN_COMMANDS: &[&str] = &[
    "Toggle sidebar",
    "Open folder…",
    "Manage plugins",
    "Install plugin…",
    "Cursor: bar",
    "Cursor: block",
    "Cursor: underline",
    "Reload config",
    "Open config file",
    "Find in files…",
    "Document symbols",
    "Rename symbol…",
    "Format document",
    "Code actions",
    "Toggle comment",
    "Duplicate line",
    "Delete line",
    "Move line up",
    "Move line down",
    "Record macro",
    "Stop recording",
    "Replay macro",
    "Save macro…",
    "Check for updates",
];

/// A place a picker row can jump to.
#[derive(Debug, Clone)]
struct JumpTarget {
    path: PathBuf,
    line: usize,
    /// Character column within the line.
    col: usize,
}

/// What the open picker's rows mean.
enum PickerAction {
    /// Nothing to do on accept (or an overlay without an action).
    None,
    /// Each row is a location to jump to.
    Locations(Vec<JumpTarget>),
    /// Each row is a workspace edit to apply.
    Edits(Vec<WorkspaceEdit>),
}

/// Which flavour of picker is open, when the distinction matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerKind {
    Generic,
    FindInFiles,
}

/// What the self-update dialog is currently showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    /// Nothing to show.
    Idle,
    /// A release check is running in the background.
    Checking,
    /// A newer release exists; the dialog asks whether to install it.
    Available {
        /// The release tag, e.g. `v0.2.0`.
        tag: String,
    },
    /// The archive is being downloaded and unpacked.
    Installing {
        /// The release tag being installed.
        tag: String,
        /// Short progress text for the dialog.
        progress: String,
    },
    /// Installed; the dialog offers a restart.
    ReadyToRestart {
        /// The release tag that is now on disk.
        tag: String,
    },
    /// The last check or install failed; the dialog shows why.
    Failed {
        /// The release involved, when there was one.
        tag: Option<String>,
        /// What went wrong.
        error: String,
    },
}

impl UpdateState {
    /// True while the dialog should capture keys and clicks.
    pub fn is_dialog_open(&self) -> bool {
        !matches!(self, UpdateState::Idle)
    }
}

/// A clickable button inside the update dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateButton {
    /// Install the offered release now.
    Yes,
    /// Install the offered release later.
    No,
    /// Restart into the freshly installed release.
    Restart,
    /// Keep running this session; restart by hand.
    Later,
    /// Dismiss a failure notice.
    Close,
}

/// Messages sent back by the update worker threads.
enum UpdateEvent {
    /// The result of a release check.
    Checked {
        /// True when the user asked for this check by hand.
        manual: bool,
        /// The newest release, or the reason there is none.
        result: Result<Option<Release>, String>,
    },
    /// A short progress note while installing.
    Progress(String),
    /// The install finished (or did not).
    Installed {
        /// The release tag that was being installed.
        tag: String,
        /// The executables replaced, or the reason there are none.
        result: Result<Vec<String>, String>,
    },
}

/// A clickable region of the tab bar, recorded while it is drawn.
#[derive(Debug, Clone, Copy)]
pub struct TabHit {
    /// Index of the editor the tab represents.
    pub index: usize,
    /// The tab's label area.
    pub label: Rect,
    /// The `✕` close box.
    pub close: Rect,
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

/// Picks the explorer's starting folder: the directory of the first opened
/// file, or the process working directory when no file was named.
fn default_root(editors: &[Editor]) -> PathBuf {
    for editor in editors {
        if let Some(path) = editor.doc.path() {
            let dir = path
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            return canonical(dir);
        }
    }
    canonical(Path::new("."))
}

/// Resolves `path` to an absolute form when possible, otherwise keeps it as-is.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The application: a set of editors, UI state, and input handling.
pub struct App {
    pub editors: Vec<Editor>,
    pub active: usize,
    pub should_quit: bool,
    pub prompt: Prompt,
    pub message: Option<String>,
    pub is_error: bool,
    pub theme: Theme,
    pub dark_theme: bool,
    pub show_help: bool,
    pub clipboard: String,
    pub last_search: Option<String>,
    pub last_search_case: bool,
    /// True once a quit has been attempted with unsaved changes.
    pub force_quit_armed: bool,
    /// The shape drawn for the terminal cursor.
    pub cursor_style: CursorStyle,
    /// Last known content viewport (rows, cols), updated during draw.
    pub viewport_rows: u16,
    pub viewport_cols: u16,
    /// Loaded Rhai plugins.
    pub plugins: PluginHost,
    /// Loaded TextWrite editor scripts.
    pub scripts: ScriptHost,
    /// Discovers plugins on disk and tracks which are enabled.
    pub plugin_manager: PluginManager,
    /// Language server sessions.
    pub lsp: LspManager,
    /// Active completion popup, if any.
    pub completion: Option<CompletionState>,
    /// Actions scheduled for future frames by a timed automation.
    pending: VecDeque<(Instant, EditorAction)>,
    /// Sorted names of every selectable language (curated + imported).
    language_names: Vec<&'static str>,
    /// The folder explorer shown along the left edge.
    pub sidebar: Sidebar,
    /// Clickable tab-bar regions, refreshed on every draw.
    pub tab_hits: Vec<TabHit>,
    /// `[keys]` from `config.toml`: user bindings, the highest precedence.
    pub keybindings: HashMap<KeySpec, String>,
    /// Where `config.toml` was read from (or would be written to).
    pub config_path: Option<PathBuf>,
    /// The generic list overlay (symbols, code actions, find in files).
    pub picker: Picker,
    /// What a chosen picker row means.
    picker_action: PickerAction,
    /// Which picker flavour is open.
    picker_kind: PickerKind,
    /// The live find-in-files query.
    pub find_query: String,
    /// Whether find-in-files matches case.
    pub find_case_sensitive: bool,
    /// The most recent find-in-files results.
    find_results: Vec<find_in_files::FileMatch>,
    /// Keys recorded so far, while a macro is being recorded.
    recording: Option<Vec<KeyEvent>>,
    /// The last macro recorded or loaded, ready to replay.
    last_macro: Option<Vec<KeyEvent>>,
    /// Named macros, shown in the command palette as `Macro: <name>`.
    macros: HashMap<String, Vec<KeyEvent>>,
    /// True while a macro is replaying (so replays are not re-recorded).
    replaying: bool,
    /// The self-update dialog's state.
    pub update: UpdateState,
    /// Button rectangles of the update dialog, refreshed on every draw.
    pub update_hits: Vec<(Rect, UpdateButton)>,
    /// Whether a startup check is allowed (`check_updates`, env override).
    pub update_checks_enabled: bool,
    /// Where the record of past checks lives (next to `config.toml`).
    pub update_record_path: Option<PathBuf>,
    /// Receiver for the update worker threads.
    update_rx: Option<Receiver<UpdateEvent>>,
    /// Cloneable sender so every worker can report back.
    update_tx: Option<Sender<UpdateEvent>>,
}

impl App {
    /// Builds an editor with the built-in defaults. `App::with_config` is the
    /// same thing with a user's `config.toml` applied on top.
    pub fn new(files: Vec<PathBuf>) -> Self {
        Self::with_config(files, Config::default())
    }

    /// Builds an editor, applying the settings and keybindings from `config`.
    pub fn with_config(files: Vec<PathBuf>, config: Config) -> Self {
        // `FAST_TEXT_LSP=0` still wins, so a stray config can't force LSP on.
        let lsp = config.lsp && lsp_enabled();
        let mut app = Self::base(files, lsp);
        let warnings = app.apply_config(config);
        if !warnings.is_empty() {
            app.set_error(format!("fast-text: {}", warnings.join("; ")));
        }
        app
    }

    fn base(files: Vec<PathBuf>, lsp: bool) -> Self {
        let mut editors = Vec::new();
        for path in files {
            match Editor::open_or_new(path.clone()) {
                Ok(e) => editors.push(e),
                Err(err) => eprintln!("fast-text: cannot open {}: {err}", path.display()),
            }
        }
        if editors.is_empty() {
            editors.push(Editor::from_text(
                "// Welcome to Fast Text\n//\n// Alt+H help  ·  Ctrl+S save  ·  Ctrl+O open  ·  Ctrl+L language  ·  Ctrl+Q quit\n//\n// Ctrl+P runs a plugin command  ·  Alt+R runs a TextWrite automation.\n//\n// Ctrl+B toggles the folder explorer  ·  Ctrl+Shift+O picks its folder.\n// Click a file in the explorer, a tab, or its ✕ to open, switch, or close.\n//\n// Rebind keys in ~/.config/fast-text/config.toml  (Ctrl+P → Open config file).\n\nfn main() {\n    println!(\"Hello, world!\");\n}\n",
                LanguageId::Rust,
            ));
        }
        let sidebar = Sidebar::new(default_root(&editors));
        let plugin_manager = PluginManager::discover(plugin_dirs(), plugins_state_path());
        let (plugins, scripts) = plugin_manager.build_hosts();
        Self {
            editors,
            active: 0,
            should_quit: false,
            prompt: Prompt::None,
            message: None,
            is_error: false,
            theme: Theme::dark(),
            dark_theme: true,
            show_help: false,
            clipboard: String::new(),
            last_search: None,
            last_search_case: false,
            force_quit_armed: false,
            cursor_style: CursorStyle::Bar,
            viewport_rows: 20,
            viewport_cols: 80,
            plugins,
            scripts,
            plugin_manager,
            lsp: LspManager::new(lsp),
            completion: None,
            language_names: all_language_names(),
            pending: VecDeque::new(),
            sidebar,
            tab_hits: Vec::new(),
            keybindings: HashMap::new(),
            config_path: None,
            picker: Picker::new(),
            picker_action: PickerAction::None,
            picker_kind: PickerKind::Generic,
            find_query: String::new(),
            find_case_sensitive: false,
            find_results: Vec::new(),
            recording: None,
            last_macro: None,
            macros: HashMap::new(),
            replaying: false,
            update: UpdateState::Idle,
            update_hits: Vec::new(),
            update_checks_enabled: true,
            update_record_path: Config::path().map(|p| p.with_file_name("update-record")),
            update_rx: None,
            update_tx: None,
        }
    }

    /// Applies a loaded config, returning the warnings it produced.
    fn apply_config(&mut self, config: Config) -> Vec<String> {
        self.cursor_style = config.cursor_style;
        self.dark_theme = !config.light_theme;
        self.theme = if config.light_theme {
            Theme::light()
        } else {
            Theme::dark()
        };
        if config.sidebar {
            self.sidebar.show();
        } else {
            self.sidebar.hide();
        }
        self.sidebar.set_show_hidden(config.show_hidden);
        self.keybindings = config.key_map();
        self.config_path = config.path.clone();
        self.update_checks_enabled = config.check_updates && env_update_checks_enabled();
        config.warnings
    }

    pub fn editor(&self) -> &Editor {
        &self.editors[self.active]
    }

    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editors[self.active]
    }

    // --- Plugins -----------------------------------------------------------

    /// Builds the context snapshot handed to plugin scripts.
    fn plugin_context(&self) -> EditorContext {
        let ed = self.editor();
        let (start, end) = ed.doc.selection_range();
        let pos = ed.doc.cursor_position();
        EditorContext {
            cursor: ed.doc.cursor(),
            selection_start: start,
            selection_end: end,
            has_selection: ed.doc.has_selection(),
            selected_text: ed.doc.selected_text(),
            line: pos.line,
            col: pos.col,
            doc_len: ed.doc.buffer().max_char_idx(),
            line_count: ed.doc.buffer().len_lines(),
            file_name: ed.title(),
            language: ed.language.name().to_string(),
        }
    }

    /// Applies a list of plugin actions to the active document.
    ///
    /// Actions with no preceding [`EditorAction::Wait`] run immediately; waits
    /// schedule the actions that follow, so an automation can play out over
    /// time. Pending actions are drained by [`App::tick`].
    fn apply_actions(&mut self, actions: Vec<EditorAction>) {
        self.schedule(actions);
        self.run_pending();
    }

    /// Turns a flat action list into a deadline for each action, honouring
    /// `Wait` as a running delay.
    fn schedule(&mut self, actions: Vec<EditorAction>) {
        let now = Instant::now();
        let mut delay = Duration::ZERO;
        for action in actions {
            match action {
                EditorAction::Wait(ms) => delay += Duration::from_millis(ms),
                other => self.pending.push_back((now + delay, other)),
            }
        }
    }

    /// Applies every pending action whose deadline has passed.
    fn run_pending(&mut self) {
        self.run_pending_at(Instant::now());
    }

    /// Like `run_pending`, but with an explicit clock (used by tests).
    fn run_pending_at(&mut self, now: Instant) {
        while let Some((at, _)) = self.pending.front() {
            if *at > now {
                break;
            }
            let (_, action) = self.pending.pop_front().expect("front checked");
            self.apply_action(action);
        }
    }

    /// True while scheduled actions are still waiting to run.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// How long the event loop may block before ticking again: short while an
    /// automation is playing, otherwise the idle poll interval.
    pub fn poll_timeout(&self) -> Duration {
        match self.pending.front() {
            Some((at, _)) => {
                let remaining = at.saturating_duration_since(Instant::now());
                remaining.clamp(Duration::from_millis(5), Duration::from_millis(30))
            }
            None => Duration::from_millis(250),
        }
    }

    /// Applies a single action to the active document.
    fn apply_action(&mut self, action: EditorAction) {
        match action {
            EditorAction::Insert(text) => self.editor_mut().doc.insert_str(&text),
            EditorAction::InsertAt { at, text } => self.editor_mut().doc.insert_at(at, &text),
            EditorAction::DeleteRange { start, end } => {
                self.editor_mut().doc.delete_range(start, end)
            }
            EditorAction::DeleteSelection => self.editor_mut().doc.delete_selection(),
            EditorAction::Backspace => self.editor_mut().doc.backspace(),
            EditorAction::DeleteForward => self.editor_mut().doc.delete_forward(),
            EditorAction::InsertNewline => self.editor_mut().doc.insert_newline(),
            EditorAction::SetCursor(at) => self.editor_mut().doc.set_cursor(at),
            EditorAction::Select { anchor, head } => self.editor_mut().doc.select(anchor, head),
            EditorAction::SelectAll => self.editor_mut().doc.select_all(),
            EditorAction::MoveLeft { extend } => self.editor_mut().doc.move_left(extend),
            EditorAction::MoveRight { extend } => self.editor_mut().doc.move_right(extend),
            EditorAction::MoveUp { extend } => self.editor_mut().doc.move_up(extend),
            EditorAction::MoveDown { extend } => self.editor_mut().doc.move_down(extend),
            EditorAction::MoveWordForward { extend } => {
                self.editor_mut().doc.move_word_forward(extend)
            }
            EditorAction::MoveWordBackward { extend } => {
                self.editor_mut().doc.move_word_backward(extend)
            }
            EditorAction::MoveLineStart { extend } => self.editor_mut().doc.move_line_start(extend),
            EditorAction::MoveLineEnd { extend } => self.editor_mut().doc.move_line_end(extend),
            EditorAction::MoveDocStart { extend } => self.editor_mut().doc.move_doc_start(extend),
            EditorAction::MoveDocEnd { extend } => self.editor_mut().doc.move_doc_end(extend),
            EditorAction::Undo => self.editor_mut().doc.undo(),
            EditorAction::Redo => self.editor_mut().doc.redo(),
            EditorAction::Save => self.save(),
            EditorAction::SetLanguage(name) => {
                let lang = LanguageId::from_name(&name);
                self.editor_mut().set_language(lang);
            }
            EditorAction::SetCursorStyle(style) => {
                self.cursor_style = style;
                self.set_message(format!("Cursor style: {}", style.name()));
            }
            // Consumed by `schedule`; a stray one is a no-op.
            EditorAction::Wait(_) => {}
            EditorAction::Message(msg) => self.set_message(msg),
        }
    }

    /// Runs a Rhai plugin or TextWrite script command by title.
    pub fn run_plugin_command(&mut self, title: &str) {
        if let Some(name) = title.strip_prefix("Macro: ") {
            self.run_named_macro(name);
            return;
        }
        if self.run_builtin_command(title) {
            return;
        }
        let ctx = self.plugin_context();
        let actions = if self.plugins.command_titles().iter().any(|t| t == title) {
            self.plugins.run_command(title, &ctx)
        } else if self.scripts.has_command(title) {
            self.scripts.run_command(title, &ctx)
        } else {
            self.set_error(format!("No command named '{title}'"));
            return;
        };
        self.set_message(format!("Ran {title}"));
        self.apply_actions(actions);
    }

    /// Runs a built-in command by title, returning true when one matched.
    fn run_builtin_command(&mut self, title: &str) -> bool {
        match title {
            "Toggle sidebar" => {
                self.toggle_sidebar();
                true
            }
            "Open folder…" => {
                self.prompt = Prompt::OpenFolder {
                    input: self.sidebar.root().to_string_lossy().into_owned(),
                };
                true
            }
            "Manage plugins" => {
                self.open_plugin_manager();
                true
            }
            "Install plugin…" => {
                self.ask_install_plugin();
                true
            }
            "Cursor: bar" => {
                self.set_cursor_style_builtin(CursorStyle::Bar);
                true
            }
            "Cursor: block" => {
                self.set_cursor_style_builtin(CursorStyle::Block);
                true
            }
            "Cursor: underline" => {
                self.set_cursor_style_builtin(CursorStyle::Underline);
                true
            }
            "Reload config" => {
                self.reload_config();
                true
            }
            "Open config file" => {
                self.open_config_file();
                true
            }
            "Find in files…" => {
                self.prompt = Prompt::FindInFiles {
                    input: self.last_search.clone().unwrap_or_default(),
                };
                true
            }
            "Document symbols" => {
                self.show_document_symbols();
                true
            }
            "Rename symbol…" => {
                self.open_rename_prompt();
                true
            }
            "Format document" => {
                self.format_document();
                true
            }
            "Code actions" => {
                self.show_code_actions();
                true
            }
            "Toggle comment" => {
                self.toggle_comment();
                true
            }
            "Duplicate line" => {
                self.duplicate_line();
                true
            }
            "Delete line" => {
                self.delete_line();
                true
            }
            "Move line up" => {
                self.move_line_up();
                true
            }
            "Move line down" => {
                self.move_line_down();
                true
            }
            "Record macro" => {
                if self.recording.is_none() {
                    self.toggle_macro_recording();
                }
                true
            }
            "Stop recording" => {
                if self.recording.is_some() {
                    self.toggle_macro_recording();
                }
                true
            }
            "Replay macro" => {
                self.replay_last_macro();
                true
            }
            "Save macro…" => {
                self.prompt = Prompt::SaveMacro {
                    input: String::new(),
                };
                true
            }
            "Check for updates" => {
                self.begin_update_check(true);
                true
            }
            _ => false,
        }
    }

    /// Runs a named command: a built-in id (see `Config::TEMPLATE`) or any
    /// command palette title, including plugin commands. Returns false when
    /// nothing matches, so callers can report it.
    pub fn run_command(&mut self, name: &str) -> bool {
        let id = name.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        if self.run_command_id(&id) {
            return true;
        }
        if let Some(title) = self
            .command_titles()
            .into_iter()
            .find(|t| t.eq_ignore_ascii_case(name.trim()))
        {
            self.run_plugin_command(&title);
            return true;
        }
        false
    }

    /// Dispatches the built-in command ids used by `[keys]`.
    fn run_command_id(&mut self, id: &str) -> bool {
        match id {
            "save" => self.save(),
            "save_as" | "saveas" => {
                self.prompt = Prompt::SaveAs {
                    input: self.current_path(),
                }
            }
            "open" | "open_file" => {
                self.prompt = Prompt::OpenFile {
                    input: String::new(),
                }
            }
            "new" | "new_buffer" => self.new_buffer(),
            "close" | "close_buffer" | "close_tab" => self.close_buffer(),
            "quit" | "quit_force" => self.quit(),
            "undo" => self.editor_mut().doc.undo(),
            "redo" => self.editor_mut().doc.redo(),
            "select_all" => self.editor_mut().doc.select_all(),
            "clear_selection" => {
                self.editor_mut().doc.collapse_selection();
                self.message = None;
            }
            "copy" => self.copy(),
            "cut" => self.cut(),
            "paste" => self.paste(),
            "find" => {
                self.prompt = Prompt::Find {
                    input: self.last_search.clone().unwrap_or_default(),
                }
            }
            "find_next" => self.find_next(SearchDirection::Forward),
            "find_prev" | "find_previous" => self.find_next(SearchDirection::Backward),
            "replace" => {
                self.prompt = Prompt::Replace {
                    find: self.last_search.clone().unwrap_or_default(),
                    replace: String::new(),
                    on_replace: false,
                }
            }
            "goto_line" => {
                self.prompt = Prompt::GotoLine {
                    input: String::new(),
                }
            }
            "language" | "set_language" => {
                self.prompt = Prompt::Language {
                    input: String::new(),
                    selected: 0,
                }
            }
            "command_palette" | "palette" => {
                self.prompt = Prompt::Command {
                    input: String::new(),
                }
            }
            "complete" => self.trigger_completion(),
            "hover" => self.show_hover(),
            "definition" | "goto_definition" => self.goto_definition(),
            "eval" | "eval_textwrite" => {
                self.prompt = Prompt::Eval {
                    input: String::new(),
                }
            }
            "run_automation" | "automation" => {
                self.prompt = Prompt::Automation {
                    input: String::new(),
                }
            }
            "toggle_theme" => self.toggle_theme(),
            "toggle_sidebar" => self.toggle_sidebar(),
            "open_folder" => {
                self.prompt = Prompt::OpenFolder {
                    input: self.sidebar.root().to_string_lossy().into_owned(),
                }
            }
            "manage_plugins" => self.open_plugin_manager(),
            "install_plugin" => self.ask_install_plugin(),
            "cursor_bar" => self.set_cursor_style_builtin(CursorStyle::Bar),
            "cursor_block" => self.set_cursor_style_builtin(CursorStyle::Block),
            "cursor_underline" => self.set_cursor_style_builtin(CursorStyle::Underline),
            "help" => self.show_help = true,
            "next_tab" | "next_buffer" => self.next_tab(),
            "prev_tab" | "previous_tab" => self.prev_tab(),
            "reload_config" => self.reload_config(),
            "open_config" | "open_config_file" => self.open_config_file(),
            "find_in_files" | "search_in_files" | "grep" => {
                self.prompt = Prompt::FindInFiles {
                    input: self.last_search.clone().unwrap_or_default(),
                }
            }
            "document_symbols" | "symbols" | "outline" => self.show_document_symbols(),
            "rename_symbol" | "rename" => self.open_rename_prompt(),
            "format_document" | "format" => self.format_document(),
            "code_actions" | "code_action" => self.show_code_actions(),
            "toggle_comment" | "comment" => self.toggle_comment(),
            "duplicate_line" => self.duplicate_line(),
            "delete_line" => self.delete_line(),
            "move_line_up" => self.move_line_up(),
            "move_line_down" => self.move_line_down(),
            "select_next_occurrence" | "add_cursor_to_next" => self.select_next_occurrence(),
            "record_macro" => self.toggle_macro_recording(),
            "replay_macro" => self.replay_last_macro(),
            "save_macro" => {
                self.prompt = Prompt::SaveMacro {
                    input: String::new(),
                }
            }
            "check_updates" | "check_for_updates" | "update_check" => self.begin_update_check(true),
            _ => {
                if let Some(name) = id.strip_prefix("macro_") {
                    self.run_named_macro(name);
                } else {
                    return false;
                }
            }
        }
        true
    }

    /// Prompts for a new symbol name (LSP rename), prefilled with the word
    /// under the cursor.
    fn open_rename_prompt(&mut self) {
        let input = self.editor().doc.word_under_cursor().unwrap_or_default();
        self.prompt = Prompt::Rename { input };
    }

    /// The active buffer's path, or an empty string when it has none.
    fn current_path(&self) -> String {
        self.editor()
            .doc
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Re-reads `config.toml`, applying settings and keybindings immediately.
    pub fn reload_config(&mut self) {
        let config = Config::load_from(self.config_path.clone().or_else(Config::path));
        let path = config.path.clone();
        let loaded = config.loaded;
        let warnings = self.apply_config(config);
        if !warnings.is_empty() {
            self.set_error(format!("fast-text: {}", warnings.join("; ")));
        } else {
            match path {
                Some(p) if loaded => self.set_message(format!("Reloaded {}", p.display())),
                Some(p) => self.set_message(format!("No config at {}", p.display())),
                None => self.set_message("Reloaded defaults"),
            }
        }
    }

    /// Opens `config.toml`, writing the commented template when it is missing.
    pub fn open_config_file(&mut self) {
        let Some(path) = self.config_path.clone().or_else(Config::path) else {
            self.set_error("fast-text: cannot locate a config path (HOME is unset)");
            return;
        };
        if !path.exists() {
            if let Err(e) = Config::write_template(&path) {
                self.set_error(format!("Cannot create {}: {e}", path.display()));
                return;
            }
            self.set_message(format!("Created {}", path.display()));
        }
        self.config_path = Some(path.clone());
        self.open_path(path);
    }

    // --- Self-update -------------------------------------------------------

    /// Starts a release check. Automatic checks are throttled by the on-disk
    /// record; manual ones (Alt+U, the palette) always run.
    pub fn begin_update_check(&mut self, manual: bool) {
        if !self.update_checks_enabled {
            if manual {
                self.set_error("Update checks are off (set check_updates = true in config.toml)");
            }
            return;
        }
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Installing { .. }
        ) {
            return;
        }
        if !manual {
            if let Some(record) = self.update_record() {
                if let Some(last) = record.last_check {
                    if fast_text_update::now_secs().saturating_sub(last)
                        < fast_text_update::CHECK_INTERVAL_SECS
                    {
                        return;
                    }
                }
                // A release the user declined stays quiet until a newer one ships.
                if record.latest.is_some() && record.latest == record.declined {
                    return;
                }
            }
        }
        self.update = UpdateState::Checking;
        if manual {
            self.set_message("Checking for updates…");
        }
        let Some(tx) = self.update_sender() else {
            self.update = UpdateState::Idle;
            return;
        };
        spawn_worker(move || {
            let result = fast_text_update::latest_release(10);
            let _ = tx.send(UpdateEvent::Checked { manual, result });
        });
    }

    /// Handles a key while the update dialog is open.
    fn handle_update_key(&mut self, key: KeyEvent) -> bool {
        let yes = matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter);
        let no = matches!(key.code, KeyCode::Char('n' | 'N') | KeyCode::Esc);
        match self.update.clone() {
            UpdateState::Available { tag } => {
                if yes {
                    self.begin_update_install(tag);
                } else if no {
                    self.record_declined_update();
                    self.update = UpdateState::Idle;
                    self.set_message("Update postponed — Alt+U checks again");
                }
            }
            UpdateState::ReadyToRestart { tag } => {
                if yes {
                    self.restart_after_update(&tag);
                } else if no {
                    self.update = UpdateState::Idle;
                    self.set_message(format!("{tag} is installed — restart Fast Text to use it"));
                }
            }
            // A failed update closes on any key; an install runs to the end.
            UpdateState::Failed { .. } => self.update = UpdateState::Idle,
            UpdateState::Installing { .. } | UpdateState::Checking | UpdateState::Idle => {}
        }
        true
    }

    /// Runs the action of a clicked update-dialog button.
    fn run_update_button(&mut self, button: UpdateButton) {
        match (button, self.update.clone()) {
            (UpdateButton::Yes, UpdateState::Available { tag }) => self.begin_update_install(tag),
            (UpdateButton::No, UpdateState::Available { .. }) => {
                self.record_declined_update();
                self.update = UpdateState::Idle;
                self.set_message("Update postponed — Alt+U checks again");
            }
            (UpdateButton::Restart, UpdateState::ReadyToRestart { tag }) => {
                self.restart_after_update(&tag)
            }
            (UpdateButton::Later | UpdateButton::No, UpdateState::ReadyToRestart { .. }) => {
                self.update = UpdateState::Idle;
                self.set_message("Restart Fast Text later to finish the update");
            }
            (UpdateButton::Close | UpdateButton::No | UpdateButton::Later, _) => {
                self.update = UpdateState::Idle
            }
            _ => {}
        }
    }

    /// Downloads the release in the background and swaps the binaries in place.
    fn begin_update_install(&mut self, tag: String) {
        self.update = UpdateState::Installing {
            tag: tag.clone(),
            progress: "Starting…".into(),
        };
        let Some(tx) = self.update_sender() else {
            self.update = UpdateState::Failed {
                tag: Some(tag),
                error: "no update worker available".to_string(),
            };
            return;
        };
        spawn_worker(move || {
            let progress_tx = tx.clone();
            let result = fast_text_update::download_and_install(&tag, &mut |message| {
                let _ = progress_tx.send(UpdateEvent::Progress(message.to_string()));
            });
            let _ = tx.send(UpdateEvent::Installed { tag, result });
        });
    }

    /// Spawns the freshly updated editor and quits this one.
    fn restart_after_update(&mut self, tag: &str) {
        match spawn_restarted_editor() {
            Ok(()) => self.should_quit = true,
            Err(e) => {
                let error = format!("Could not restart automatically: {e}");
                self.update = UpdateState::Failed {
                    tag: Some(tag.to_string()),
                    error: error.clone(),
                };
                self.set_error(error);
            }
        }
    }

    /// Drains messages from the update workers (called on every tick).
    fn poll_update_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.update_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            self.apply_update_event(event);
        }
    }

    /// Folds one worker message into the dialog state.
    fn apply_update_event(&mut self, event: UpdateEvent) {
        match event {
            UpdateEvent::Checked { manual, result } => match result {
                Ok(Some(release)) => {
                    let declined = self.note_update_check(Some(&release.tag));
                    // GitHub names its newest release whether or not it is
                    // newer than this build; only offer a real upgrade.
                    if !fast_text_update::is_newer(&release.version, env!("CARGO_PKG_VERSION")) {
                        if manual {
                            self.set_message(format!(
                                "Fast Text v{} is up to date",
                                env!("CARGO_PKG_VERSION")
                            ));
                        }
                        self.update = UpdateState::Idle;
                        return;
                    }
                    // Never nag about a release the user already declined.
                    if !manual && declined.as_deref() == Some(release.tag.as_str()) {
                        self.update = UpdateState::Idle;
                        return;
                    }
                    self.update = UpdateState::Available {
                        tag: release.tag.clone(),
                    };
                    self.set_message(format!(
                        "Fast Text {} is available (you have v{})",
                        release.tag,
                        env!("CARGO_PKG_VERSION")
                    ));
                }
                Ok(None) => {
                    self.note_update_check(None);
                    if manual {
                        self.set_message(format!(
                            "Fast Text v{} is up to date",
                            env!("CARGO_PKG_VERSION")
                        ));
                    }
                    self.update = UpdateState::Idle;
                }
                Err(error) => {
                    self.note_update_check(None);
                    if manual {
                        self.set_error(format!("Update check failed: {error}"));
                    }
                    // Offline at startup is normal life, not an error.
                    self.update = UpdateState::Idle;
                }
            },
            UpdateEvent::Progress(message) => {
                if let UpdateState::Installing { tag, .. } = &self.update {
                    self.update = UpdateState::Installing {
                        tag: tag.clone(),
                        progress: message,
                    };
                }
            }
            UpdateEvent::Installed { tag, result } => match result {
                Ok(_replaced) => {
                    self.update = UpdateState::ReadyToRestart { tag: tag.clone() };
                    self.set_message(format!("{tag} installed — restart Fast Text to finish"));
                }
                Err(error) => {
                    self.update = UpdateState::Failed {
                        tag: Some(tag),
                        error: error.clone(),
                    };
                    self.set_error(format!("Update failed: {error}"));
                }
            },
        }
    }

    /// Records that a check ran (and what it found), returning the declined
    /// tag so callers can decide whether to nag.
    fn note_update_check(&mut self, latest: Option<&str>) -> Option<String> {
        let path = self.update_record_path.clone()?;
        let mut record = fast_text_update::load_record(&path);
        record.last_check = Some(fast_text_update::now_secs());
        if let Some(latest) = latest {
            record.latest = Some(latest.to_string());
        }
        let declined = record.declined.clone();
        let _ = fast_text_update::save_record(&path, &record);
        declined
    }

    /// Remembers that the user said no to the offered release.
    fn record_declined_update(&mut self) {
        if let UpdateState::Available { tag } = &self.update {
            let tag = tag.clone();
            if let Some(path) = self.update_record_path.clone() {
                let mut record = fast_text_update::load_record(&path);
                record.declined = Some(tag);
                let _ = fast_text_update::save_record(&path, &record);
            }
        }
    }

    /// The last check record, if this machine has somewhere to keep one.
    fn update_record(&self) -> Option<fast_text_update::CheckRecord> {
        self.update_record_path
            .as_ref()
            .map(|path| fast_text_update::load_record(path))
    }

    /// Lazily creates the channel the worker threads report back through.
    fn update_sender(&mut self) -> Option<Sender<UpdateEvent>> {
        if self.update_tx.is_none() {
            let (tx, rx) = mpsc::channel();
            self.update_tx = Some(tx.clone());
            self.update_rx = Some(rx);
        }
        self.update_tx.clone()
    }

    /// Every registered command title: built-ins, Rhai plugins, TextWrite scripts.
    pub fn command_titles(&self) -> Vec<String> {
        let mut titles: Vec<String> = BUILTIN_COMMANDS.iter().map(|s| s.to_string()).collect();
        titles.extend(self.plugins.command_titles());
        titles.extend(self.scripts.command_titles());
        titles.extend(self.macro_command_titles());
        titles.sort();
        titles.dedup();
        titles
    }

    /// Command titles matching a substring query.
    pub fn command_matches(&self, query: &str) -> Vec<String> {
        let q = query.trim().to_ascii_lowercase();
        let mut titles = self.command_titles();
        if !q.is_empty() {
            titles.retain(|t| t.to_ascii_lowercase().contains(&q));
        }
        titles
    }

    /// Evaluates a TextWrite snippet against the active buffer, applying any
    /// editor actions it requests and showing the resulting value.
    pub fn eval_script(&mut self, source: &str) {
        if source.trim().is_empty() {
            return;
        }
        let ctx = self.plugin_context();
        match self.scripts.eval(source, &ctx) {
            Ok(value) => {
                let actions = self.scripts.take_actions();
                let messaged = actions
                    .iter()
                    .any(|a| matches!(a, EditorAction::Message(_)));
                self.apply_actions(actions);
                if !messaged && !matches!(value, fast_text_textwrite::Value::None) {
                    self.set_message(value.to_display());
                }
            }
            Err(e) => self.set_error(format!("{e}")),
        }
    }

    /// Compiles and runs a TextWrite automation file against the active buffer.
    ///
    /// The file is compiled first (imports resolved) so mistakes surface before
    /// anything runs; its queued actions are then scheduled, honouring `wait`.
    pub fn run_automation(&mut self, path: &str) {
        let path = path.trim();
        if path.is_empty() {
            return;
        }
        if let Err(e) = self.scripts.compile_file(path) {
            self.set_error(format!("{e}"));
            return;
        }
        let ctx = self.plugin_context();
        match self.scripts.run_file(path, &ctx) {
            Ok(actions) => {
                self.set_message(format!("Running automation: {path}"));
                self.apply_actions(actions);
            }
            Err(e) => self.set_error(format!("{e}")),
        }
    }

    // --- Languages ---------------------------------------------------------

    /// Language names matching a substring query, prefix matches first.
    pub fn language_matches(&self, query: &str) -> Vec<&'static str> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return self.language_names.clone();
        }
        let mut hits: Vec<&'static str> = self
            .language_names
            .iter()
            .copied()
            .filter(|n| n.to_ascii_lowercase().contains(&q))
            .collect();
        hits.sort_by_key(|n| !n.to_ascii_lowercase().starts_with(&q));
        hits
    }

    /// Switches the active buffer to the named language.
    pub fn set_language_by_name(&mut self, name: &str) {
        let lang = LanguageId::from_name(name);
        self.editor_mut().set_language(lang);
        self.set_message(format!("Language set to {}", lang.name()));
    }

    // --- Language servers --------------------------------------------------

    /// Called once per event-loop iteration: plays back any scheduled
    /// automation actions, then keeps servers in sync with edits and picks up
    /// freshly published diagnostics.
    pub fn tick(&mut self) {
        self.poll_update_events();
        self.run_pending();
        if !self.lsp.is_enabled() {
            return;
        }
        // Sync every open editor so background buffers stay current too.
        for ed in &self.editors {
            self.lsp.sync(ed);
        }
    }

    /// Diagnostics for the active buffer.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.lsp.diagnostics(self.editor())
    }

    /// The most severe diagnostic covering a given line, if any.
    pub fn diagnostic_for_line(&self, line: usize) -> Option<Diagnostic> {
        self.diagnostics()
            .into_iter()
            .filter(|d| d.range.start.line as usize <= line && line <= d.range.end.line as usize)
            .min_by_key(|d| match d.severity() {
                Severity::Error => 0,
                Severity::Warning => 1,
                Severity::Information => 2,
                Severity::Hint => 3,
            })
    }

    /// A compact diagnostics summary like `2 errors, 1 warning`.
    pub fn diagnostics_summary(&self, diags: &[Diagnostic]) -> Option<String> {
        if diags.is_empty() {
            return None;
        }
        let (mut e, mut w, mut i) = (0, 0, 0);
        for d in diags {
            match d.severity() {
                Severity::Error => e += 1,
                Severity::Warning => w += 1,
                Severity::Information | Severity::Hint => i += 1,
            }
        }
        let mut parts = Vec::new();
        if e > 0 {
            parts.push(format!("{e} error{}", plural(e)));
        }
        if w > 0 {
            parts.push(format!("{w} warning{}", plural(w)));
        }
        if i > 0 {
            parts.push(format!("{i} info"));
        }
        Some(parts.join(", "))
    }

    /// Requests completions at the cursor and opens the popup.
    pub fn trigger_completion(&mut self) {
        let start = self.identifier_start();
        match self.lsp.completion(self.editor()) {
            Ok(items) if !items.is_empty() => {
                self.completion = Some(CompletionState {
                    items,
                    selected: 0,
                    prefix_start: start,
                });
            }
            Ok(_) => self.set_message("No completions"),
            Err(e) => self.set_error(e),
        }
    }

    /// Character index where the identifier before the cursor begins.
    fn identifier_start(&self) -> usize {
        let ed = self.editor();
        let cursor = ed.doc.cursor();
        let chars: Vec<char> = ed.doc.text().chars().collect();
        let mut start = cursor.min(chars.len());
        while start > 0 {
            let c = chars[start - 1];
            if c.is_alphanumeric() || c == '_' {
                start -= 1;
            } else {
                break;
            }
        }
        start
    }

    /// Accepts the selected completion.
    fn accept_completion(&mut self) {
        let Some(state) = self.completion.take() else {
            return;
        };
        let Some(item) = state.items.get(state.selected) else {
            return;
        };
        let start = state.prefix_start;
        let end = self.editor().doc.cursor();
        if start <= end {
            let insertion = item.insertion().to_string();
            let ed = self.editor_mut();
            if start < end {
                ed.doc.delete_range(start, end);
            }
            ed.doc.insert_str(&insertion);
        }
    }

    fn move_completion(&mut self, delta: isize) {
        if let Some(state) = &mut self.completion {
            let len = state.items.len();
            if len == 0 {
                return;
            }
            let next = (state.selected as isize + delta).rem_euclid(len as isize);
            state.selected = next as usize;
        }
    }

    /// Shows hover information at the cursor.
    pub fn show_hover(&mut self) {
        match self.lsp.hover(self.editor()) {
            Ok(Some(text)) => self.set_message(text.replace('\n', "  ")),
            Ok(None) => self.set_message("No hover information"),
            Err(e) => self.set_error(e),
        }
    }

    /// Jumps to the definition of the symbol under the cursor.
    pub fn goto_definition(&mut self) {
        match self.lsp.definition(self.editor()) {
            Ok(Some(loc)) => {
                let Some(path) = fast_text_lsp::uri_to_path(&loc.uri) else {
                    self.set_error("Definition location has an unsupported URI");
                    return;
                };
                let target = self
                    .editors
                    .iter()
                    .position(|e| e.doc.path().is_some_and(|p| *p == path));
                let idx = match target {
                    Some(i) => i,
                    None => match Editor::open(path.clone()) {
                        Ok(e) => {
                            self.editors.push(e);
                            self.editors.len() - 1
                        }
                        Err(e) => {
                            self.set_error(format!("Cannot open {}: {e}", path.display()));
                            return;
                        }
                    },
                };
                self.active = idx;
                let line = loc.range.start.line as usize;
                let col = loc.range.start.character as usize;
                let ed = &mut self.editors[self.active];
                let index = ed.doc.index_of(fast_text_core::Position::new(line, col));
                ed.doc.set_cursor(index);
                self.set_message("Jumped to definition");
            }
            Ok(None) => self.set_message("No definition found"),
            Err(e) => self.set_error(e),
        }
    }

    /// Renames the symbol under the cursor via the language server.
    fn rename_symbol(&mut self, new_name: &str) {
        if new_name.is_empty() {
            self.set_error("Rename needs a new name");
            return;
        }
        match self.lsp.rename(self.editor(), new_name) {
            Ok(edit) if edit.is_empty() => self.set_message("Rename produced no changes"),
            Ok(edit) => self.apply_workspace_edit(&edit),
            Err(e) => self.set_error(format!("Rename: {e}")),
        }
    }

    /// Formats the active document via the language server.
    fn format_document(&mut self) {
        let edits = match self.lsp.format(self.editor()) {
            Ok(edits) => edits,
            Err(e) => {
                self.set_error(format!("Format: {e}"));
                return;
            }
        };
        if edits.is_empty() {
            self.set_message("Already formatted");
            return;
        }
        let ranges: Vec<(usize, usize, String)> = {
            let doc = &self.editor().doc;
            edits
                .iter()
                .map(|te| {
                    let (s, e) = lsp_range_to_chars(doc, &te.range);
                    (s, e, te.new_text.clone())
                })
                .collect()
        };
        self.editor_mut().doc.apply_edits(&ranges);
        self.set_message(format!("Formatted ({} edit(s))", edits.len()));
    }

    /// Offers the server's code actions for the cursor in the picker.
    fn show_code_actions(&mut self) {
        let actions = match self.lsp.code_actions(self.editor()) {
            Ok(actions) => actions,
            Err(e) => {
                self.set_error(format!("Code actions: {e}"));
                return;
            }
        };
        let mut items = Vec::new();
        let mut edits = Vec::new();
        for action in actions {
            let Some(edit) = action.edit else {
                // Command-only actions need an `executeCommand` round-trip.
                continue;
            };
            if edit.is_empty() {
                continue;
            }
            let badge = action
                .kind
                .as_deref()
                .and_then(|k| k.split('.').next())
                .unwrap_or("");
            items.push(PickerItem::new(action.title).with_badge(badge));
            edits.push(edit);
        }
        if items.is_empty() {
            self.set_message("No applicable code actions");
            return;
        }
        self.picker_kind = PickerKind::Generic;
        self.picker_action = PickerAction::Edits(edits);
        self.picker.open("Code actions", items);
    }

    /// Shows the document outline in the picker.
    fn show_document_symbols(&mut self) {
        let symbols = match self.lsp.document_symbols(self.editor()) {
            Ok(symbols) => symbols,
            Err(e) => {
                self.set_error(format!("Symbols: {e}"));
                return;
            }
        };
        if symbols.is_empty() {
            self.set_message("No symbols");
            return;
        }
        let current = self.editor().doc.path().map(Path::to_path_buf);
        let mut items = Vec::new();
        let mut targets = Vec::new();
        for symbol in symbols {
            let path = symbol
                .uri
                .as_deref()
                .and_then(fast_text_lsp::uri_to_path)
                .or_else(|| current.clone());
            let Some(path) = path else {
                continue;
            };
            let line = symbol.range.start.line as usize;
            let col = self.utf16_col_for(&path, line, symbol.range.start.character as usize);
            let badge = symbol.kind_badge();
            let mut item = PickerItem::new(symbol.name).with_badge(badge);
            if let Some(detail) = symbol.detail {
                item = item.with_detail(detail);
            }
            items.push(item);
            targets.push(JumpTarget { path, line, col });
        }
        if items.is_empty() {
            self.set_message("No symbols");
            return;
        }
        self.picker_kind = PickerKind::Generic;
        self.picker_action = PickerAction::Locations(targets);
        self.picker.open("Symbols", items);
    }

    /// Converts an LSP UTF-16 column to a character column for `path`.
    fn utf16_col_for(&self, path: &Path, line: usize, utf16: usize) -> usize {
        if let Some(ed) = self
            .editors
            .iter()
            .find(|e| e.doc.path().is_some_and(|p| p == path))
        {
            if line < ed.doc.buffer().len_lines() {
                return utf16_to_char_col(&ed.doc.buffer().line_str(line), utf16);
            }
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Some(text_line) = text.lines().nth(line) {
                return utf16_to_char_col(text_line, utf16);
            }
        }
        utf16
    }

    /// Applies a server workspace edit to every file it touches.
    fn apply_workspace_edit(&mut self, edit: &WorkspaceEdit) {
        let changes: Vec<(PathBuf, Vec<TextEdit>)> = edit
            .changes
            .iter()
            .filter_map(|(uri, edits)| fast_text_lsp::uri_to_path(uri).map(|p| (p, edits.clone())))
            .collect();
        let mut edits_applied = 0;
        let mut files = 0;
        for (path, edits) in changes {
            let target = self
                .editors
                .iter()
                .position(|e| e.doc.path().is_some_and(|p| p == path));
            let idx = match target {
                Some(i) => i,
                None => match Editor::open_or_new(path.clone()) {
                    Ok(e) => {
                        self.editors.push(e);
                        self.editors.len() - 1
                    }
                    Err(e) => {
                        self.set_error(format!("Cannot open {}: {e}", path.display()));
                        continue;
                    }
                },
            };
            let doc = &mut self.editors[idx].doc;
            let ranges: Vec<(usize, usize, String)> = edits
                .iter()
                .map(|te| {
                    let (s, e) = lsp_range_to_chars(doc, &te.range);
                    (s, e, te.new_text.clone())
                })
                .collect();
            edits_applied += ranges.len();
            files += 1;
            doc.apply_edits(&ranges);
        }
        if files == 0 {
            self.set_message("Nothing to apply");
        } else {
            self.set_message(format!(
                "Applied {edits_applied} edit(s) in {files} file(s)"
            ));
        }
    }

    // --- Find in files -----------------------------------------------------

    /// Opens the find-in-files picker for `query`.
    fn open_find_in_files(&mut self, query: &str) {
        self.find_query = query.to_string();
        self.picker_kind = PickerKind::FindInFiles;
        self.picker
            .open(format!("Find in files: {query}"), Vec::new());
        self.refresh_find_in_files();
    }

    /// Re-runs the search and refreshes the open picker.
    fn refresh_find_in_files(&mut self) {
        let root = self.sidebar.root().to_path_buf();
        self.find_results = find_in_files::find(&root, &self.find_query, self.find_case_sensitive);
        self.apply_find_items();
    }

    /// Rebuilds the picker rows from the cached results.
    fn apply_find_items(&mut self) {
        let root = self.sidebar.root().to_path_buf();
        let items: Vec<PickerItem> = self
            .find_results
            .iter()
            .map(|m| {
                let rel = m.path.strip_prefix(&root).unwrap_or(&m.path);
                PickerItem::new(m.text.trim_start().to_string()).with_detail(format!(
                    "{}:{}",
                    rel.display(),
                    m.line + 1
                ))
            })
            .collect();
        let targets: Vec<JumpTarget> = self
            .find_results
            .iter()
            .map(|m| JumpTarget {
                path: m.path.clone(),
                line: m.line,
                col: m.col,
            })
            .collect();
        let count = targets.len();
        self.picker
            .set_title(format!("Find in files: {}", self.find_query));
        self.picker.set_items(items);
        let case = if self.find_case_sensitive {
            "case-sensitive"
        } else {
            "case-insensitive"
        };
        self.picker
            .set_footer(format!("{count} match(es) · {case} · Tab toggles case"));
        self.picker_action = PickerAction::Locations(targets);
    }

    // --- Picker ------------------------------------------------------------

    /// Routes a key to the open picker.
    fn handle_picker_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if self.picker_kind == PickerKind::FindInFiles {
            match key.code {
                KeyCode::Char(c) if !ctrl && !alt => {
                    self.find_query.push(c);
                    self.refresh_find_in_files();
                    return true;
                }
                KeyCode::Backspace => {
                    self.find_query.pop();
                    self.refresh_find_in_files();
                    return true;
                }
                KeyCode::Tab => {
                    self.find_case_sensitive = !self.find_case_sensitive;
                    self.refresh_find_in_files();
                    return true;
                }
                _ => {}
            }
        }
        match self.picker.handle_key(key) {
            PickerOutcome::Close => self.close_picker(),
            PickerOutcome::Handled => {}
            PickerOutcome::Chosen(index) => {
                // Keep the action alive until the row has been chosen.
                self.picker.close();
                self.picker_kind = PickerKind::Generic;
                self.choose_picker_row(index);
                self.picker_action = PickerAction::None;
            }
        }
        true
    }

    fn close_picker(&mut self) {
        self.picker.close();
        self.picker_action = PickerAction::None;
        self.picker_kind = PickerKind::Generic;
    }

    /// Acts on the chosen picker row.
    fn choose_picker_row(&mut self, index: usize) {
        match &self.picker_action {
            PickerAction::None => {}
            PickerAction::Locations(targets) => {
                if let Some(target) = targets.get(index).cloned() {
                    self.open_and_jump(&target.path, target.line, target.col);
                }
            }
            PickerAction::Edits(edits) => {
                if let Some(edit) = edits.get(index).cloned() {
                    self.apply_workspace_edit(&edit);
                }
            }
        }
    }

    /// Opens `path` (if needed) and places the caret at `(line, col)`.
    pub fn open_and_jump(&mut self, path: &Path, line: usize, col: usize) {
        let target = self
            .editors
            .iter()
            .position(|e| e.doc.path().is_some_and(|p| p == path));
        let idx = match target {
            Some(i) => i,
            None => match Editor::open_or_new(path.to_path_buf()) {
                Ok(e) => {
                    self.editors.push(e);
                    self.editors.len() - 1
                }
                Err(e) => {
                    self.set_error(format!("Cannot open {}: {e}", path.display()));
                    return;
                }
            },
        };
        self.active = idx;
        let ed = &mut self.editors[idx];
        let line = line.min(ed.doc.buffer().len_lines().saturating_sub(1));
        let index = ed.doc.index_of(Position::new(line, col));
        ed.doc.set_cursor(index);
    }

    fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.is_error = false;
    }

    fn set_error(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.is_error = true;
    }

    pub fn toggle_theme(&mut self) {
        self.dark_theme = !self.dark_theme;
        self.theme = if self.dark_theme {
            Theme::dark()
        } else {
            Theme::light()
        };
    }

    // --- Buffer management -------------------------------------------------

    fn new_buffer(&mut self) {
        self.editors.push(Editor::from_text("", LanguageId::Plain));
        self.active = self.editors.len() - 1;
        self.set_message("New buffer");
    }

    fn close_buffer(&mut self) {
        if self.editors.len() == 1 {
            self.editor_mut().doc = Document::new();
            self.editor_mut().language = LanguageId::Plain;
            self.editor_mut().scroll_line = 0;
            self.editor_mut().scroll_col = 0;
            self.set_message("Cleared buffer");
            return;
        }
        let removed = self.editors.remove(self.active);
        if removed.doc.is_dirty() {
            self.set_message(format!(
                "Closed {} (unsaved changes discarded)",
                removed.title()
            ));
        } else {
            self.set_message(format!("Closed {}", removed.title()));
        }
        if self.active >= self.editors.len() {
            self.active = self.editors.len() - 1;
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        // If already open, switch to it.
        if let Some(idx) = self
            .editors
            .iter()
            .position(|e| e.doc.path() == Some(path.as_path()))
        {
            self.active = idx;
            return;
        }
        match Editor::open_or_new(path.clone()) {
            Ok(e) => {
                self.editors.push(e);
                self.active = self.editors.len() - 1;
                if path.exists() {
                    self.set_message(format!("Opened {}", path.display()));
                } else {
                    self.set_message(format!("New file {}", path.display()));
                }
            }
            Err(err) => self.set_error(format!("Cannot open {}: {err}", path.display())),
        }
    }

    // --- Explorer sidebar --------------------------------------------------

    /// Handles `Ctrl+B`: show and focus, focus, or hide the explorer.
    fn toggle_sidebar(&mut self) {
        if !self.sidebar.visible() {
            self.sidebar.show();
            self.set_message(format!("Explorer: {}", self.sidebar.title()));
        } else if self.sidebar.focused() {
            self.sidebar.hide();
            self.message = None;
        } else {
            self.sidebar.focus();
            self.message = None;
        }
    }

    /// Points the explorer at `input`: a folder, or a file inside one.
    fn open_folder(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        let path = PathBuf::from(trimmed);
        let dir = if path.is_dir() {
            canonical(&path)
        } else if path.is_file() {
            let parent = path
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            canonical(parent)
        } else {
            self.set_error(format!("No such folder: {trimmed}"));
            return;
        };
        self.sidebar.set_root(dir);
        self.sidebar.show();
        let title = self.sidebar.title();
        self.set_message(format!("Explorer: {title}"));
    }

    /// Opens the prompt that installs a plugin file.
    fn ask_install_plugin(&mut self) {
        self.plugin_manager.close();
        self.prompt = Prompt::InstallPlugin {
            input: String::new(),
        };
    }

    /// Copies a plugin file into the user plugin directory and reloads.
    fn install_plugin(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        let source = PathBuf::from(input);
        if !source.is_file() {
            self.set_error(format!("Not a file: {input}"));
            return;
        }
        let Some(dir) = self.plugin_manager.primary_dir().map(Path::to_path_buf) else {
            self.set_error("No plugin directory configured".to_string());
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.set_error(format!("Cannot create {}: {e}", dir.display()));
            return;
        }
        let Some(name) = source.file_name() else {
            self.set_error(format!("Bad path: {input}"));
            return;
        };
        let dest = dir.join(name);
        match std::fs::copy(&source, &dest) {
            Ok(_) => {
                self.plugin_manager.rescan();
                self.reload_plugins();
                self.set_message(format!("Installed {}", name.to_string_lossy()));
            }
            Err(e) => self.set_error(format!("Install failed: {e}")),
        }
    }

    /// Routes a key press to the explorer while it holds focus.
    fn handle_sidebar_key(&mut self, key: KeyEvent) -> bool {
        match self.sidebar.handle_key(key) {
            SidebarAction::None => {}
            SidebarAction::Open(path) => {
                self.open_path(path);
                self.sidebar.unfocus();
            }
            SidebarAction::Unfocus => self.sidebar.unfocus(),
        }
        true
    }

    // --- Plugin manager ----------------------------------------------------

    /// Opens the plugin manager overlay.
    fn open_plugin_manager(&mut self) {
        self.plugin_manager.open();
    }

    /// Routes a key to the plugin manager while it is open.
    fn handle_plugin_manager_key(&mut self, key: KeyEvent) -> bool {
        match self.plugin_manager.handle_key(key) {
            ManagerAction::Reload => self.reload_plugins(),
            ManagerAction::Install => self.ask_install_plugin(),
            ManagerAction::Close => self.plugin_manager.close(),
            ManagerAction::None => {}
        }
        true
    }

    /// Rebuilds the plugin hosts from the manager's enabled set.
    fn reload_plugins(&mut self) {
        let (plugins, scripts) = self.plugin_manager.build_hosts();
        self.plugins = plugins;
        self.scripts = scripts;
        let on = self.plugin_manager.enabled_count();
        let total = self.plugin_manager.entries.len();
        self.set_message(format!("Plugins: {on}/{total} enabled"));
    }

    /// Sets the cursor shape from a built-in command or keybinding.
    fn set_cursor_style_builtin(&mut self, style: CursorStyle) {
        self.cursor_style = style;
        self.set_message(format!("Cursor style: {}", style.name()));
    }

    // --- Tabs --------------------------------------------------------------

    /// Switches to the next open tab.
    fn next_tab(&mut self) {
        if self.editors.len() > 1 {
            self.active = (self.active + 1) % self.editors.len();
        }
    }

    /// Switches to the previous open tab.
    fn prev_tab(&mut self) {
        let count = self.editors.len();
        if count > 1 {
            self.active = (self.active + count - 1) % count;
        }
    }

    /// Switches to the tab at `index`, if it exists.
    fn activate_tab(&mut self, index: usize) {
        if index < self.editors.len() {
            self.active = index;
        }
    }

    /// Closes the tab at `index`, if it exists.
    fn close_buffer_at(&mut self, index: usize) {
        if index < self.editors.len() {
            self.active = index;
            self.close_buffer();
        }
    }

    // --- Mouse -------------------------------------------------------------

    /// Handles one mouse event, returning true when the UI should redraw.
    pub fn handle_mouse(&mut self, event: MouseEvent) -> bool {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click(event.column, event.row),
            MouseEventKind::ScrollUp if self.picker.is_open() => {
                self.picker
                    .handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                true
            }
            MouseEventKind::ScrollDown if self.picker.is_open() => {
                self.picker
                    .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                true
            }
            MouseEventKind::ScrollUp if self.plugin_manager.is_open() => {
                self.plugin_manager.scroll_by(-3);
                true
            }
            MouseEventKind::ScrollDown if self.plugin_manager.is_open() => {
                self.plugin_manager.scroll_by(3);
                true
            }
            MouseEventKind::ScrollUp
                if self.sidebar.visible() && self.sidebar.contains(event.column, event.row) =>
            {
                self.sidebar.scroll_by(-3);
                true
            }
            MouseEventKind::ScrollDown
                if self.sidebar.visible() && self.sidebar.contains(event.column, event.row) =>
            {
                self.sidebar.scroll_by(3);
                true
            }
            _ => false,
        }
    }

    /// Handles a left click at a screen cell.
    fn click(&mut self, column: u16, row: u16) -> bool {
        // The update dialog is modal: a click hits one of its buttons.
        if self.update.is_dialog_open() {
            if let Some(button) = self
                .update_hits
                .iter()
                .find(|(rect, _)| rect_contains(*rect, column, row))
                .map(|(_, button)| *button)
            {
                self.run_update_button(button);
            }
            return true;
        }
        // The picker is modal: a click selects, a second click activates.
        if self.picker.is_open() {
            if let Some(index) = self.picker.row_at(column, row) {
                if self.picker.selected() == index {
                    self.picker.close();
                    self.picker_kind = PickerKind::Generic;
                    self.choose_picker_row(index);
                    self.picker_action = PickerAction::None;
                } else {
                    self.picker.select_index(index);
                }
            }
            return true;
        }
        self.click_editor(column, row)
    }

    fn click_editor(&mut self, column: u16, row: u16) -> bool {
        // The plugin manager is modal: it takes every click while open.
        if self.plugin_manager.is_open() {
            if let Some(index) = self.plugin_manager.row_at(column, row) {
                if self.plugin_manager.selected == index {
                    // A second click on the same row toggles it.
                    self.plugin_manager.toggle(index);
                    self.reload_plugins();
                } else {
                    self.plugin_manager.select(index);
                }
            }
            return true;
        }

        // Tab bar: a close box first, then the tab itself.
        if let Some(index) = self
            .tab_hits
            .iter()
            .find(|h| rect_contains(h.close, column, row))
            .map(|h| h.index)
        {
            self.close_buffer_at(index);
            return true;
        }
        if let Some(index) = self
            .tab_hits
            .iter()
            .find(|h| rect_contains(h.label, column, row))
            .map(|h| h.index)
        {
            self.activate_tab(index);
            return true;
        }

        // Explorer: click a row to select or open, or anywhere to focus it.
        if self.sidebar.visible() && self.sidebar.contains(column, row) {
            self.sidebar.focus();
            if let Some(index) = self.sidebar.row_at(column, row) {
                if let SidebarAction::Open(path) = self.sidebar.activate_index(index) {
                    self.open_path(path);
                }
            }
            return true;
        }
        // A click elsewhere hands focus back to the editor.
        if self.sidebar.focused() {
            self.sidebar.unfocus();
        }
        false
    }

    // --- Saving ------------------------------------------------------------

    fn save(&mut self) {
        if self.editors[self.active].is_untitled() {
            self.prompt = Prompt::SaveAs {
                input: String::new(),
            };
            return;
        }
        match self.editors[self.active].doc.save() {
            Ok(()) => {
                let title = self.editors[self.active].title();
                self.set_message(format!("Saved {title}"));
            }
            Err(e) => self.set_error(format!("Save failed: {e}")),
        }
    }

    fn do_save_as(&mut self, path: String) {
        if path.trim().is_empty() {
            return;
        }
        let path = PathBuf::from(path);
        let ed = &mut self.editors[self.active];
        match ed.doc.save_as(path.clone()) {
            Ok(()) => {
                ed.language = LanguageId::from_path(&path.to_string_lossy());
                self.set_message(format!("Saved {}", path.display()));
            }
            Err(e) => self.set_error(format!("Save failed: {e}")),
        }
    }

    // --- Search ------------------------------------------------------------

    fn search(&mut self, query: &str, direction: SearchDirection) {
        if query.is_empty() {
            return;
        }
        let case = self.last_search_case;
        let ed = &mut self.editors[self.active];
        match ed.doc.find(query, direction, case, true) {
            Some(_) => {
                self.message = None;
            }
            None => self.set_error(format!("Not found: {query}")),
        }
    }

    fn find_next(&mut self, direction: SearchDirection) {
        if let Some(q) = self.last_search.clone() {
            self.search(&q, direction);
        } else {
            self.prompt = Prompt::Find {
                input: String::new(),
            };
        }
    }

    // --- Clipboard ---------------------------------------------------------

    fn copy(&mut self) {
        let text = self.editor().doc.selected_text();
        if !text.is_empty() {
            self.clipboard = text;
            self.set_message("Copied");
        }
    }

    fn cut(&mut self) {
        let text = self.editor().doc.selected_text();
        if !text.is_empty() {
            self.clipboard = text;
            self.editor_mut().doc.delete_selection();
            self.set_message("Cut");
        }
    }

    fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let clip = self.clipboard.clone();
        self.editor_mut().doc.insert_str(&clip);
    }

    // --- Editing ergonomics ------------------------------------------------

    /// Inserts a typed character, honouring auto-pairs.
    fn typed_char(&mut self, c: char) {
        let ed = self.editor_mut();
        let doc = &mut ed.doc;
        let opener = matching_open(c);
        let closer = matching_close(c);

        // Quotation characters are their own pair.
        if closer == Some(c) {
            if doc.has_selection() {
                let text = doc.selected_text();
                doc.insert_str(&format!("{c}{text}{c}"));
                return;
            }
            let head = doc.cursor();
            if doc.buffer().char_at(head) == Some(c) {
                doc.move_right(false);
                return;
            }
            doc.insert_str(&format!("{c}{c}"));
            doc.move_left(false);
            return;
        }

        // An opening bracket inserts its matching close.
        if let Some(close) = closer {
            if doc.has_selection() {
                let text = doc.selected_text();
                doc.insert_str(&format!("{c}{text}{close}"));
            } else {
                doc.insert_str(&format!("{c}{close}"));
                doc.move_left(false);
            }
            return;
        }

        // A closing bracket steps over an auto-inserted partner.
        if opener.is_some() {
            let head = doc.cursor();
            if doc.selection().is_empty() && doc.buffer().char_at(head) == Some(c) {
                doc.move_right(false);
                return;
            }
        }

        doc.insert_char(c);
    }

    /// Inserts a newline, copying the current indentation and opening a block.
    fn newline_auto_indent(&mut self) {
        let ed = self.editor_mut();
        let doc = &mut ed.doc;
        let pos = doc.cursor_position();
        let line = doc.buffer().line_str(pos.line);
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let head = doc.selection().head;
        let before = head.checked_sub(1).and_then(|i| doc.buffer().char_at(i));
        let after = doc.buffer().char_at(head);
        let opens = matches!(before, Some('{') | Some('[') | Some('('));
        let closes = matches!(after, Some('}') | Some(']') | Some(')'));
        doc.insert_newline();
        doc.insert_str(&indent);
        if opens && closes {
            // Put the closer on its own line and step the caret onto the
            // freshly indented inner line.
            doc.insert_str("    ");
            doc.insert_newline();
            doc.insert_str(&indent);
            doc.move_up(false);
            doc.move_line_end(false);
        }
    }

    /// Adds the next occurrence of the word/selection as a caret.
    pub fn select_next_occurrence(&mut self) {
        let case = self.last_search_case;
        if !self.editor_mut().doc.select_next_occurrence(case) {
            self.set_error("No further occurrence");
        }
    }

    /// Toggles a `//`-style comment on the selected or current lines.
    pub fn toggle_comment(&mut self) {
        let marker = self
            .editor()
            .language
            .def()
            .line_comment
            .first()
            .copied()
            .unwrap_or("//");
        let (start_line, end_line) = {
            let doc = &self.editor().doc;
            let (s, e) = doc.selection().range();
            (doc.position_of(s).line, doc.position_of(e).line)
        };
        let all_commented = (start_line..=end_line).all(|line| {
            let text = self.editor().doc.buffer().line_str(line);
            let trimmed = text.trim_start();
            trimmed.is_empty() || trimmed.starts_with(marker)
        });
        let mut edits = Vec::new();
        for line in start_line..=end_line {
            let line_start = self.editor().doc.index_of(Position::new(line, 0));
            let text = self.editor().doc.buffer().line_str(line);
            if text.trim().is_empty() {
                continue;
            }
            if all_commented {
                let stripped = text.strip_prefix(marker).unwrap_or(&text);
                let stripped = stripped.strip_prefix(' ').unwrap_or(stripped);
                let removed = text.chars().count() - stripped.chars().count();
                edits.push((line_start, line_start + removed, String::new()));
            } else {
                let prefix: String = text
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let at = line_start + prefix.chars().count();
                edits.push((at, at, format!("{marker} ")));
            }
        }
        if edits.is_empty() {
            return;
        }
        self.editor_mut().doc.apply_edits(&edits);
    }

    /// Duplicates the current line (or every selected line).
    pub fn duplicate_line(&mut self) {
        let ed = self.editor_mut();
        let doc = &mut ed.doc;
        let (s, e) = doc.selection().range();
        let start_line = doc.position_of(s).line;
        let end_line = doc.position_of(e).line;
        let line_start = doc.index_of(Position::new(start_line, 0));
        let line_end = doc.index_of(Position::new(end_line, usize::MAX));
        let text = doc.buffer().slice(line_start, line_end);
        let ending = doc.buffer().line_ending().as_str().to_string();
        doc.apply_edits(&[(line_end, line_end, format!("{ending}{text}"))]);
        let caret = line_end + ending.chars().count() + text.chars().count();
        doc.set_cursor(caret.min(doc.buffer().max_char_idx()));
    }

    /// Deletes the current line (or every selected line).
    pub fn delete_line(&mut self) {
        let ed = self.editor_mut();
        let doc = &mut ed.doc;
        let (s, e) = doc.selection().range();
        let start_line = doc.position_of(s).line;
        let end_line = doc.position_of(e).line;
        let start = doc.index_of(Position::new(start_line, 0));
        let last = doc.buffer().len_lines().saturating_sub(1);
        let (from, to) = if end_line < last {
            (start, doc.index_of(Position::new(end_line + 1, 0)))
        } else if start > 0 {
            // Last line: drop the newline that precedes it instead.
            (start - 1, doc.index_of(Position::new(end_line, usize::MAX)))
        } else {
            (start, doc.index_of(Position::new(end_line, usize::MAX)))
        };
        doc.apply_edits(&[(from, to, String::new())]);
        doc.set_cursor(from.min(doc.buffer().max_char_idx()));
    }

    /// Moves the current line (or selection) up or down by one line.
    fn shift_lines(&mut self, delta: i64) {
        let ed = self.editor_mut();
        let doc = &mut ed.doc;
        let pos = doc.cursor_position();
        let line = pos.line as i64;
        let target = line + delta;
        let last = doc.buffer().len_lines().saturating_sub(1) as i64;
        if line < 0 || target < 0 || target > last {
            return;
        }
        let upper = line.min(target) as usize;
        let lower = line.max(target) as usize;
        let upper_start = doc.index_of(Position::new(upper, 0));
        let lower_end = doc.index_of(Position::new(lower, usize::MAX));
        let upper_text = doc
            .buffer()
            .slice(upper_start, doc.index_of(Position::new(upper, usize::MAX)));
        let lower_text = doc
            .buffer()
            .slice(doc.index_of(Position::new(lower, 0)), lower_end);
        let ending = doc.buffer().line_ending().as_str().to_string();
        doc.apply_edits(&[(
            upper_start,
            lower_end,
            format!("{lower_text}{ending}{upper_text}"),
        )]);
        let new_line = (line + delta) as usize;
        let col = pos.col.min(doc.buffer().line_len_chars(new_line));
        doc.set_cursor(doc.index_of(Position::new(new_line, col)));
    }

    pub fn move_line_up(&mut self) {
        self.shift_lines(-1);
    }

    pub fn move_line_down(&mut self) {
        self.shift_lines(1);
    }

    // --- Macros ------------------------------------------------------------

    /// Starts or stops recording a macro (F6).
    fn toggle_macro_recording(&mut self) {
        if self.replaying {
            return;
        }
        if let Some(recorded) = self.recording.take() {
            let count = recorded.len();
            self.last_macro = Some(recorded);
            self.set_message(format!("Recorded macro ({count} keys) — F7 to replay"));
        } else {
            self.recording = Some(Vec::new());
            self.set_message("Recording macro — F6 to stop");
        }
    }

    /// Replays the last recorded macro (F7).
    fn replay_last_macro(&mut self) {
        if self.replaying {
            return;
        }
        let Some(keys) = self.last_macro.clone() else {
            self.set_error("No macro recorded");
            return;
        };
        if keys.is_empty() {
            self.set_message("Macro is empty");
            return;
        }
        self.replaying = true;
        for key in keys {
            self.dispatch_key(key);
        }
        self.replaying = false;
        self.set_message("Replayed macro");
    }

    /// Runs a named macro from the palette.
    fn run_named_macro(&mut self, name: &str) {
        if self.replaying {
            return;
        }
        let Some(keys) = self.macros.get(name).cloned() else {
            self.set_error(format!("No macro named '{name}'"));
            return;
        };
        self.replaying = true;
        for key in keys {
            self.dispatch_key(key);
        }
        self.replaying = false;
        self.set_message(format!("Ran macro '{name}'"));
    }

    /// Stores the last recorded macro under a name.
    fn save_macro(&mut self, name: &str) {
        if name.is_empty() {
            self.set_error("Macro needs a name");
            return;
        }
        let Some(keys) = self.last_macro.clone() else {
            self.set_error("Record a macro first (F6)");
            return;
        };
        self.macros.insert(name.to_string(), keys);
        self.set_message(format!("Saved macro '{name}'"));
    }

    /// Named macros, offered in the command palette.
    fn macro_command_titles(&self) -> Vec<String> {
        let mut names: Vec<&String> = self.macros.keys().collect();
        names.sort();
        names.iter().map(|n| format!("Macro: {n}")).collect()
    }

    // --- Input -------------------------------------------------------------

    /// Handles one key event. Returns true if the app should redraw.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // A replaying macro must not be re-recorded, and cannot nest.
        if self.replaying {
            return self.dispatch_key(key);
        }
        if let Some(recorded) = self.recording.as_mut() {
            // The stop key ends the recording without being part of it.
            if !is_macro_toggle(&key) {
                recorded.push(key);
            }
        }
        self.dispatch_key(key)
    }

    fn dispatch_key(&mut self, key: KeyEvent) -> bool {
        // The self-update dialog is modal and outranks everything else.
        if self.update.is_dialog_open() {
            return self.handle_update_key(key);
        }
        if self.show_help {
            self.show_help = false;
            return true;
        }
        if self.plugin_manager.is_open() {
            return self.handle_plugin_manager_key(key);
        }
        if self.picker.is_open() {
            return self.handle_picker_key(key);
        }
        if self.prompt.is_active() {
            self.handle_prompt_key(key);
            return true;
        }
        self.handle_editor_key(key)
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let mut moved_selection = false;
        match key.code {
            KeyCode::Esc => {
                self.prompt = Prompt::None;
            }
            KeyCode::Enter => self.commit_prompt(),
            KeyCode::Up => {
                if let Prompt::Language { selected, .. } = &mut self.prompt {
                    *selected = selected.saturating_sub(1);
                    moved_selection = true;
                }
            }
            KeyCode::Down => {
                let current = match &self.prompt {
                    Prompt::Language { input, selected } => Some((input.clone(), *selected)),
                    _ => None,
                };
                if let Some((input, selected)) = current {
                    let max = self.language_matches(&input).len().saturating_sub(1);
                    if let Prompt::Language { selected: s, .. } = &mut self.prompt {
                        *s = (selected + 1).min(max);
                    }
                    moved_selection = true;
                }
            }
            KeyCode::Backspace => {
                if let Some(input) = prompt_input_mut(&mut self.prompt) {
                    input.pop();
                }
            }
            KeyCode::Char(c) if !ctrl => {
                if let Some(input) = prompt_input_mut(&mut self.prompt) {
                    input.push(c);
                }
            }
            _ => {}
        }
        // Editing the query resets the picker highlight to the best match.
        if !moved_selection {
            if let Prompt::Language { selected, .. } = &mut self.prompt {
                *selected = 0;
            }
        }
    }

    fn commit_prompt(&mut self) {
        let prompt = std::mem::replace(&mut self.prompt, Prompt::None);
        match prompt {
            Prompt::None => {}
            Prompt::OpenFile { input } => {
                let path = PathBuf::from(input.trim());
                self.open_path(path);
            }
            Prompt::SaveAs { input } => self.do_save_as(input),
            Prompt::Find { input } => {
                self.last_search = Some(input.clone());
                self.search(&input, SearchDirection::Forward);
            }
            Prompt::Replace {
                find,
                replace,
                on_replace,
            } => {
                if !on_replace {
                    // First half: capture the find term, then ask for replacement.
                    self.prompt = Prompt::Replace {
                        find,
                        replace,
                        on_replace: true,
                    };
                } else {
                    let n = self.editors[self.active].doc.replace_all(
                        &find,
                        &replace,
                        self.last_search_case,
                    );
                    self.set_message(format!("Replaced {n} occurrence(s)"));
                }
            }
            Prompt::GotoLine { input } => {
                if let Ok(n) = input.trim().parse::<usize>() {
                    let line = n.saturating_sub(1);
                    let ed = &mut self.editors[self.active];
                    let idx = ed.doc.index_of(fast_text_core::Position::new(line, 0));
                    ed.doc.set_cursor(idx);
                }
            }
            Prompt::Language { input, selected } => {
                let matches = self.language_matches(&input);
                let idx = selected.min(matches.len().saturating_sub(1));
                if let Some(name) = matches.get(idx).copied() {
                    self.set_language_by_name(name);
                } else {
                    self.set_error(format!("No language matches '{}'", input.trim()));
                }
            }
            Prompt::Command { input } => {
                let matches = self.command_matches(&input);
                if let Some(title) = matches.first().cloned() {
                    self.run_plugin_command(&title);
                } else if input.trim().is_empty() {
                    self.set_message("No commands registered");
                } else {
                    self.set_error(format!("No command matches '{}'", input.trim()));
                }
            }
            Prompt::Automation { input } => self.run_automation(&input),
            Prompt::Eval { input } => self.eval_script(&input),
            Prompt::OpenFolder { input } => self.open_folder(input.trim()),
            Prompt::InstallPlugin { input } => self.install_plugin(input.trim()),
            Prompt::FindInFiles { input } => self.open_find_in_files(input.trim()),
            Prompt::Rename { input } => self.rename_symbol(input.trim()),
            Prompt::SaveMacro { input } => self.save_macro(input.trim()),
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) -> bool {
        self.message = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        // The completion popup captures navigation keys while it is open.
        if self.completion.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.completion = None;
                    return true;
                }
                KeyCode::Up => {
                    self.move_completion(-1);
                    return true;
                }
                KeyCode::Down => {
                    self.move_completion(1);
                    return true;
                }
                KeyCode::Enter | KeyCode::Tab => {
                    self.accept_completion();
                    return true;
                }
                _ => self.completion = None,
            }
        }

        // User keybindings from config.toml win over every built-in binding.
        if let Some(spec) = key_event_to_spec(&key, ctrl, alt, shift) {
            if let Some(name) = self.keybindings.get(&spec).cloned() {
                if !self.run_command(&name) {
                    self.set_error(format!("fast-text: no command named '{name}'"));
                }
                return true;
            }
        }

        // The folder explorer: Ctrl+B focuses it (or hides it when focused).
        if ctrl && matches!(key.code, KeyCode::Char('b')) {
            self.toggle_sidebar();
            return true;
        }
        if ctrl && shift && matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O')) {
            self.prompt = Prompt::OpenFolder {
                input: self.sidebar.root().to_string_lossy().into_owned(),
            };
            return true;
        }
        if ctrl && matches!(key.code, KeyCode::Char('m') | KeyCode::Char('M')) {
            self.open_plugin_manager();
            return true;
        }
        if ctrl && shift && matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F')) {
            self.prompt = Prompt::FindInFiles {
                input: self.last_search.clone().unwrap_or_default(),
            };
            return true;
        }
        if ctrl && shift && matches!(key.code, KeyCode::Char('i') | KeyCode::Char('I')) {
            self.format_document();
            return true;
        }
        if ctrl && shift && matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D')) {
            self.duplicate_line();
            return true;
        }

        // Control / function commands first.
        if ctrl {
            match key.code {
                KeyCode::Char('q') => {
                    self.quit();
                    return true;
                }
                KeyCode::Char('s') => {
                    self.save();
                    return true;
                }
                KeyCode::Char('o') => {
                    self.prompt = Prompt::OpenFile {
                        input: String::new(),
                    };
                    return true;
                }
                KeyCode::Char('n') => {
                    self.new_buffer();
                    return true;
                }
                KeyCode::Char('w') => {
                    self.close_buffer();
                    return true;
                }
                KeyCode::Char('z') => {
                    if shift {
                        self.editor_mut().doc.redo();
                    } else {
                        self.editor_mut().doc.undo();
                    }
                    return true;
                }
                KeyCode::Char('y') => {
                    self.editor_mut().doc.redo();
                    return true;
                }
                KeyCode::Char('a') => {
                    self.editor_mut().doc.select_all();
                    return true;
                }
                KeyCode::Char('p') => {
                    self.prompt = Prompt::Command {
                        input: String::new(),
                    };
                    return true;
                }
                KeyCode::Char('e') => {
                    self.prompt = Prompt::Eval {
                        input: String::new(),
                    };
                    return true;
                }
                KeyCode::Char(' ') => {
                    self.trigger_completion();
                    return true;
                }
                KeyCode::Char('f') => {
                    self.prompt = Prompt::Find {
                        input: self.last_search.clone().unwrap_or_default(),
                    };
                    return true;
                }
                KeyCode::Char('h') => {
                    self.prompt = Prompt::Replace {
                        find: self.last_search.clone().unwrap_or_default(),
                        replace: String::new(),
                        on_replace: false,
                    };
                    return true;
                }
                KeyCode::Char('g') => {
                    self.prompt = Prompt::GotoLine {
                        input: String::new(),
                    };
                    return true;
                }
                KeyCode::Char('l') => {
                    self.prompt = Prompt::Language {
                        input: String::new(),
                        selected: 0,
                    };
                    return true;
                }
                KeyCode::Char('c') => {
                    self.copy();
                    return true;
                }
                KeyCode::Char('x') => {
                    self.cut();
                    return true;
                }
                KeyCode::Char('v') => {
                    self.paste();
                    return true;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.select_next_occurrence();
                    return true;
                }
                KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.delete_line();
                    return true;
                }
                KeyCode::Char('/') => {
                    self.toggle_comment();
                    return true;
                }
                KeyCode::Char('.') => {
                    self.show_code_actions();
                    return true;
                }
                KeyCode::Char('t') => {
                    self.toggle_theme();
                    return true;
                }
                KeyCode::Left => {
                    self.editor_mut().doc.move_word_backward(shift);
                    return true;
                }
                KeyCode::Right => {
                    self.editor_mut().doc.move_word_forward(shift);
                    return true;
                }
                KeyCode::Home => {
                    self.editor_mut().doc.move_doc_start(shift);
                    return true;
                }
                KeyCode::End => {
                    self.editor_mut().doc.move_doc_end(shift);
                    return true;
                }
                KeyCode::Tab => {
                    self.next_tab();
                    return true;
                }
                KeyCode::BackTab => {
                    self.prev_tab();
                    return true;
                }
                KeyCode::PageDown => {
                    self.next_tab();
                    return true;
                }
                KeyCode::PageUp => {
                    self.prev_tab();
                    return true;
                }
                _ => {}
            }
        }

        // While the explorer holds keyboard focus, its own keys win.
        if self.sidebar.focused() {
            return self.handle_sidebar_key(key);
        }

        // Plugin keybindings (checked before built-in defaults, after the
        // built-in Ctrl commands so plugins can't lock the user out of saving).
        if let Some(spec) = key_event_to_spec(&key, ctrl, alt, shift) {
            let title = self
                .plugins
                .lookup_key(&spec)
                .or_else(|| self.scripts.lookup_key(&spec));
            if let Some(title) = title {
                self.run_plugin_command(&title);
                return true;
            }
        }

        match key.code {
            KeyCode::Char('h') if alt => self.show_help = true,
            KeyCode::Char('r') if alt => {
                self.prompt = Prompt::Automation {
                    input: String::new(),
                }
            }
            // Cursor shapes are built in, so they work with no plugins loaded.
            // An enabled plugin that binds the same key still wins (plugins are
            // consulted just above).
            KeyCode::Char('1') if alt => self.set_cursor_style_builtin(CursorStyle::Bar),
            KeyCode::Char('2') if alt => self.set_cursor_style_builtin(CursorStyle::Block),
            KeyCode::Char('3') if alt => self.set_cursor_style_builtin(CursorStyle::Underline),
            // Alt+M is the terminal-independent fallback for the plugin manager:
            // a legacy terminal reports Ctrl+M as plain Enter, so Alt+M (which
            // arrives as an ESC-prefixed byte) always reaches us.
            KeyCode::Char('m') if alt => self.open_plugin_manager(),
            // Alt+U checks GitHub for a newer release (terminal-independent).
            KeyCode::Char('u') if alt => self.begin_update_check(true),
            KeyCode::F(2) => {
                self.prompt = Prompt::SaveAs {
                    input: self
                        .editor()
                        .doc
                        .path()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                }
            }
            KeyCode::F(3) => self.find_next(SearchDirection::Forward),
            KeyCode::F(4) => self.find_next(SearchDirection::Backward),
            KeyCode::F(5) => self.show_hover(),
            KeyCode::F(6) => self.toggle_macro_recording(),
            KeyCode::F(7) => self.replay_last_macro(),
            KeyCode::F(12) => self.goto_definition(),
            KeyCode::Esc => {
                self.editor_mut().doc.collapse_selection();
                self.message = None;
            }
            KeyCode::Char('f') if alt => {
                self.prompt = Prompt::FindInFiles {
                    input: self.last_search.clone().unwrap_or_default(),
                }
            }
            KeyCode::Char('o') if alt => self.show_document_symbols(),
            KeyCode::Char('n') if alt => self.open_rename_prompt(),
            KeyCode::Char('c') if alt => self.toggle_comment(),
            KeyCode::Up if alt && shift => self.editor_mut().doc.add_caret_above(),
            KeyCode::Down if alt && shift => self.editor_mut().doc.add_caret_below(),
            KeyCode::Up if alt => self.move_line_up(),
            KeyCode::Down if alt => self.move_line_down(),
            KeyCode::Left if alt => self.editor_mut().doc.move_word_backward(shift),
            KeyCode::Right if alt => self.editor_mut().doc.move_word_forward(shift),
            KeyCode::Left => self.editor_mut().doc.move_left(shift),
            KeyCode::Right => self.editor_mut().doc.move_right(shift),
            KeyCode::Up => self.editor_mut().doc.move_up(shift),
            KeyCode::Down => self.editor_mut().doc.move_down(shift),
            KeyCode::Home => self.editor_mut().doc.move_line_start(shift),
            KeyCode::End => self.editor_mut().doc.move_line_end(shift),
            KeyCode::PageUp => {
                let rows = self.viewport_rows.max(1) as usize;
                for _ in 0..rows {
                    self.editor_mut().doc.move_up(shift);
                }
            }
            KeyCode::PageDown => {
                let rows = self.viewport_rows.max(1) as usize;
                for _ in 0..rows {
                    self.editor_mut().doc.move_down(shift);
                }
            }
            KeyCode::Enter => self.newline_auto_indent(),
            KeyCode::Tab => self.editor_mut().doc.insert_char('\t'),
            KeyCode::Backspace => self.editor_mut().doc.backspace(),
            KeyCode::Delete => self.editor_mut().doc.delete_forward(),
            KeyCode::Char(c) if !ctrl && !alt => self.typed_char(c),
            _ => {}
        }
        true
    }

    fn quit(&mut self) {
        let dirty: Vec<String> = self
            .editors
            .iter()
            .filter(|e| e.doc.is_dirty())
            .map(|e| e.title())
            .collect();
        if dirty.is_empty() {
            self.should_quit = true;
        } else {
            self.set_error(format!(
                "Unsaved changes in {} — press Ctrl+Q again to force quit, or Ctrl+S to save",
                dirty.join(", ")
            ));
            // Second Ctrl+Q within the same message forces quit.
            if self.force_quit_armed {
                self.should_quit = true;
            } else {
                self.force_quit_armed = true;
            }
        }
    }
}

/// Every selectable language name: the hand-curated set plus everything
/// imported from Helix, sorted and de-duplicated.
fn all_language_names() -> Vec<&'static str> {
    fast_text_syntax::all_names()
}

/// `""` for one, `"s"` otherwise.
fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Mutable access to the text input of a prompt, if it has one.
fn prompt_input_mut(prompt: &mut Prompt) -> Option<&mut String> {
    match prompt {
        Prompt::OpenFile { input }
        | Prompt::SaveAs { input }
        | Prompt::Find { input }
        | Prompt::GotoLine { input }
        | Prompt::Language { input, .. }
        | Prompt::Command { input }
        | Prompt::Automation { input }
        | Prompt::Eval { input }
        | Prompt::FindInFiles { input }
        | Prompt::Rename { input }
        | Prompt::SaveMacro { input }
        | Prompt::OpenFolder { input }
        | Prompt::InstallPlugin { input } => Some(input),
        Prompt::Replace {
            find,
            replace,
            on_replace,
        } => {
            if *on_replace {
                Some(replace)
            } else {
                Some(find)
            }
        }
        Prompt::None => None,
    }
}

/// The closing character that pairs with an opening one.
fn matching_close(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

/// The opening character that a closing one pairs with.
fn matching_open(close: char) -> Option<char> {
    match close {
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        '"' | '\'' | '`' => Some(close),
        _ => None,
    }
}

/// Converts an LSP UTF-16 column into a character column within `line`.
fn utf16_to_char_col(line: &str, utf16: usize) -> usize {
    let mut units = 0;
    let mut col = 0;
    for c in line.chars() {
        if units >= utf16 {
            break;
        }
        units += c.len_utf16();
        col += 1;
    }
    col
}

/// Converts an LSP (line, UTF-16 column) position into a character index.
fn lsp_pos_to_char(doc: &Document, line: u32, character: u32) -> usize {
    let line = line as usize;
    let text = if line < doc.buffer().len_lines() {
        doc.buffer().line_str(line)
    } else {
        String::new()
    };
    let col = utf16_to_char_col(&text, character as usize);
    doc.index_of(Position::new(line, col))
}

/// Converts an LSP range into a pair of character indices.
fn lsp_range_to_chars(doc: &Document, range: &LspRange) -> (usize, usize) {
    (
        lsp_pos_to_char(doc, range.start.line, range.start.character),
        lsp_pos_to_char(doc, range.end.line, range.end.character),
    )
}

/// Whether a key stops macro recording (it is not part of the macro).
fn is_macro_toggle(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::F(6))
}

/// The plugin directories, in load order: the user config first, then the
/// `plugins/` folder next to the working directory.
fn plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = user_plugin_dir() {
        dirs.push(dir);
    }
    dirs.push(PathBuf::from("plugins"));
    dirs
}

/// Where the plugin manager records the plugins that are turned off.
fn plugins_state_path() -> Option<PathBuf> {
    // `~/.config/fast-text/plugins.disabled`, beside the plugin folder.
    user_plugin_dir().map(|dir| dir.with_file_name("plugins.disabled"))
}

/// Whether language-server integration is enabled.
///
/// On by default; set `FAST_TEXT_LSP=0` (or `off`/`false`/`no`) to disable.
fn lsp_enabled() -> bool {
    match std::env::var("FAST_TEXT_LSP") {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        ),
        Err(_) => true,
    }
}

/// `FAST_TEXT_UPDATE_CHECK=0` (or off/false/no) forces update checks off,
/// whatever `config.toml` says.
fn env_update_checks_enabled() -> bool {
    match std::env::var("FAST_TEXT_UPDATE_CHECK") {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        ),
        Err(_) => true,
    }
}

/// Runs `f` on a background thread. A no-op under `cfg(test)`, so unit tests
/// never reach for the network or spawn installer processes.
fn spawn_worker(f: impl FnOnce() + Send + 'static) {
    #[cfg(not(test))]
    std::thread::spawn(f);
    #[cfg(test)]
    let _ = f;
}

/// Starts a new editor process from the current executable and its arguments.
fn spawn_restarted_editor() -> std::io::Result<()> {
    #[cfg(test)]
    return Ok(());
    #[cfg(not(test))]
    {
        let mut command = fast_text_update::restart_command()?;
        command.spawn()?;
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/fast-text/plugins`, falling back to `~/.config/fast-text/plugins`.
fn user_plugin_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("fast-text").join("plugins"))
}

/// Converts a backend key event into a backend-independent [`KeySpec`].
fn key_event_to_spec(key: &KeyEvent, ctrl: bool, alt: bool, shift: bool) -> Option<KeySpec> {
    let name = match key.code {
        KeyCode::Char(' ') => KeyName::Space,
        KeyCode::Char(c) => KeyName::Char(c.to_ascii_lowercase()),
        KeyCode::Enter => KeyName::Enter,
        KeyCode::Esc => KeyName::Esc,
        KeyCode::Tab => KeyName::Tab,
        KeyCode::Backspace => KeyName::Backspace,
        KeyCode::Delete => KeyName::Delete,
        KeyCode::Left => KeyName::Left,
        KeyCode::Right => KeyName::Right,
        KeyCode::Up => KeyName::Up,
        KeyCode::Down => KeyName::Down,
        KeyCode::Home => KeyName::Home,
        KeyCode::End => KeyName::End,
        KeyCode::PageUp => KeyName::PageUp,
        KeyCode::PageDown => KeyName::PageDown,
        KeyCode::F(n) => KeyName::F(n),
        _ => return None,
    };
    Some(KeySpec::new(name, ctrl, alt, shift))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn empty_app() -> App {
        App::new(vec![])
    }

    fn clear_and_type(app: &mut App, text: &str) {
        app.handle_key(ctrl('a'));
        app.handle_key(key(KeyCode::Backspace));
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    /// Replaces the active buffer's text with `text`, leaving the caret at 0.
    fn set_text(app: &mut App, text: &str) {
        let doc = &mut app.editor_mut().doc;
        doc.select_all();
        if text.is_empty() {
            doc.delete_selection();
        } else {
            doc.insert_str(text);
        }
        doc.set_cursor(0);
    }

    fn add_demo_plugin(app: &mut App) {
        app.plugins.add_plugin(
            "demo",
            r#"
                fn shout() { insert("SHOUT"); }
                command("Shout", "shout");
                bind("ctrl+shift+s", "Shout");
            "#,
        );
    }

    fn add_demo_script(app: &mut App) {
        app.scripts.add_script(
            "demo.tw",
            r#"
                fn tick() {
                    insert("TW")
                }
                command("TextWrite tick", "tick")
                bind("ctrl+shift+t", "TextWrite tick")
            "#,
        );
        assert!(app.scripts.errors.is_empty(), "{:?}", app.scripts.errors);
    }

    #[test]
    fn plugin_command_runs_and_edits() {
        let mut app = empty_app();
        add_demo_plugin(&mut app);
        clear_and_type(&mut app, "hi");
        app.run_plugin_command("Shout");
        assert_eq!(app.editor().doc.text(), "hiSHOUT");
    }

    #[test]
    fn plugin_keybinding_is_dispatched() {
        let mut app = empty_app();
        add_demo_plugin(&mut app);
        clear_and_type(&mut app, "");
        // Ctrl+Shift+S should trigger the plugin's "Shout".
        let ev = KeyEvent::new(
            KeyCode::Char('S'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        app.handle_key(ev);
        assert_eq!(app.editor().doc.text(), "SHOUT");
    }

    #[test]
    fn command_palette_filters_and_runs() {
        let mut app = empty_app();
        add_demo_plugin(&mut app);
        clear_and_type(&mut app, "");
        app.handle_key(ctrl('p'));
        assert!(matches!(app.prompt, Prompt::Command { .. }));
        for c in "sho".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.command_matches("sho"), vec!["Shout".to_string()]);
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.editor().doc.text(), "SHOUT");
    }

    #[test]
    fn plugin_set_language_action() {
        let mut app = empty_app();
        app.plugins.add_plugin(
            "lang",
            r#"
                fn make_python() { set_language("python"); }
                command("Make Python", "make_python");
            "#,
        );
        app.run_plugin_command("Make Python");
        assert_eq!(app.editor().language, fast_text_syntax::LanguageId::Python);
    }

    #[test]
    fn textwrite_command_runs_and_edits() {
        let mut app = empty_app();
        add_demo_script(&mut app);
        clear_and_type(&mut app, "hi");
        app.run_plugin_command("TextWrite tick");
        assert_eq!(app.editor().doc.text(), "hiTW");
    }

    #[test]
    fn textwrite_keybinding_is_dispatched() {
        let mut app = empty_app();
        add_demo_script(&mut app);
        clear_and_type(&mut app, "");
        let ev = KeyEvent::new(
            KeyCode::Char('T'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        app.handle_key(ev);
        assert_eq!(app.editor().doc.text(), "TW");
    }

    #[test]
    fn textwrite_command_changes_cursor_style() {
        let mut app = empty_app();
        app.scripts.add_script(
            "cursor.tw",
            r#"
                fn block() { set_cursor_style("block") }
                command("Cursor block", "block")
            "#,
        );
        assert!(app.scripts.errors.is_empty(), "{:?}", app.scripts.errors);
        app.run_plugin_command("Cursor block");
        assert_eq!(app.cursor_style, CursorStyle::Block);
        assert_eq!(app.message.as_deref(), Some("Cursor style: block"));
    }

    #[test]
    fn command_palette_includes_textwrite_commands() {
        let mut app = empty_app();
        add_demo_script(&mut app);
        assert!(app.command_titles().contains(&"TextWrite tick".to_string()));
        assert_eq!(
            app.command_matches("tick"),
            vec!["TextWrite tick".to_string()]
        );
    }

    #[test]
    fn ctrl_e_evaluates_a_textwrite_expression() {
        let mut app = empty_app();
        clear_and_type(&mut app, "");
        app.handle_key(ctrl('e'));
        assert!(matches!(app.prompt, Prompt::Eval { .. }));
        for c in "1 + 2 * 3".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.prompt, Prompt::None));
        assert_eq!(app.message.as_deref(), Some("7"));
        assert!(!app.is_error);
    }

    #[test]
    fn ctrl_e_applies_editor_actions() {
        let mut app = empty_app();
        clear_and_type(&mut app, "");
        app.handle_key(ctrl('e'));
        for c in "insert(\"abc\")".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.editor().doc.text(), "abc");
    }

    #[test]
    fn ctrl_e_reports_script_errors() {
        let mut app = empty_app();
        app.handle_key(ctrl('e'));
        for c in "1 +".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(app.is_error, "a syntax error should surface as an error");
    }

    #[test]
    fn typing_then_undo_redo() {
        let mut app = empty_app();
        clear_and_type(&mut app, "hello");
        assert_eq!(app.editor().doc.text(), "hello");
        app.handle_key(ctrl('z'));
        assert_eq!(app.editor().doc.text(), "");
        app.handle_key(ctrl('y'));
        assert_eq!(app.editor().doc.text(), "hello");
    }

    #[test]
    fn cut_and_paste_within_editor() {
        let mut app = empty_app();
        clear_and_type(&mut app, "abcdef");
        // Select "bcd" (indices 1..4) then cut.
        app.editor_mut().doc.select(1, 4);
        app.handle_key(ctrl('x'));
        assert_eq!(app.editor().doc.text(), "aef");
        assert_eq!(app.clipboard, "bcd");
        // Paste at end.
        app.editor_mut().doc.set_cursor(3);
        app.handle_key(ctrl('v'));
        assert_eq!(app.editor().doc.text(), "aefbcd");
    }

    #[test]
    fn open_prompt_creates_buffer_for_missing_file() {
        let mut app = empty_app();
        app.handle_key(ctrl('o'));
        assert!(matches!(app.prompt, Prompt::OpenFile { .. }));
        for c in "/tmp/opencode/fast-text-nonexistent-xyz.py".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.prompt, Prompt::None));
        assert!(
            !app.is_error,
            "a missing path should open a new buffer, not error"
        );
        assert_eq!(app.editor().language.name(), "Python");
        assert_eq!(app.editor().title(), "fast-text-nonexistent-xyz.py");
        assert!(!app.editor().is_untitled());
    }

    #[test]
    fn open_prompt_errors_on_unreadable_path() {
        let mut app = empty_app();
        app.handle_key(ctrl('o'));
        // An existing directory exists but cannot be read as text.
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        for c in dir.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.prompt, Prompt::None));
        assert!(app.is_error);
    }

    #[test]
    fn goto_line_moves_cursor() {
        let mut app = empty_app();
        clear_and_type(&mut app, "one\ntwo\nthree");
        app.handle_key(ctrl('g'));
        for c in "3".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.editor().doc.cursor_position().line, 2);
    }

    #[test]
    fn quit_blocked_when_dirty_then_forced() {
        let mut app = empty_app();
        clear_and_type(&mut app, "x");
        app.handle_key(ctrl('q'));
        assert!(!app.should_quit);
        assert!(app.is_error);
        app.handle_key(ctrl('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn quit_immediately_when_clean() {
        let mut app = empty_app();
        app.handle_key(ctrl('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn language_picker_lists_curated_and_imported() {
        let app = empty_app();
        let all = app.language_matches("");
        assert!(all.contains(&"Rust"), "missing curated Rust");
        assert!(all.contains(&"Plain Text"));
        assert!(all.contains(&"Zig"), "missing imported Zig");
        assert!(all.contains(&"Kotlin"));
        assert!(all.contains(&"Haskell"));
        assert!(all.contains(&"YAML"));
        // Sorted and unique.
        let mut sorted = all.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(all, sorted, "language list should be sorted and unique");
    }

    #[test]
    fn language_picker_filters_by_substring_with_prefix_first() {
        let app = empty_app();
        let hits = app.language_matches("ru");
        assert!(hits.contains(&"Ruby"));
        assert!(hits.contains(&"Rust"));
        // Prefix matches come before substring-only matches.
        let ruby = hits.iter().position(|n| *n == "Ruby").unwrap();
        let perl = hits.iter().position(|n| *n == "Perl");
        if let Some(perl) = perl {
            assert!(ruby < perl, "prefix match should sort first");
        }
    }

    #[test]
    fn ctrl_l_opens_language_picker_and_sets_language() {
        let mut app = empty_app();
        app.handle_key(ctrl('l'));
        assert!(matches!(app.prompt, Prompt::Language { .. }));
        for c in "zig".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        let first = app.language_matches("zig")[0];
        assert_eq!(first, "Zig");
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.prompt, Prompt::None));
        assert_eq!(app.editor().language.name(), "Zig");
    }

    #[test]
    fn language_picker_arrow_keys_move_selection() {
        let mut app = empty_app();
        app.handle_key(ctrl('l'));
        for c in "script".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        let count = app.language_matches("script").len();
        assert!(count >= 2, "expected several matches for 'script'");
        // Down then Enter picks the second match, not the first.
        app.handle_key(key(KeyCode::Down));
        let second = app.language_matches("script")[1];
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.editor().language.name(), second);
    }

    #[test]
    fn alt_h_opens_help_and_any_key_closes_it() {
        let mut app = empty_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT));
        assert!(app.show_help, "Alt+H should open help");
        // The help overlay closes on the next key press.
        app.handle_key(key(KeyCode::Char('x')));
        assert!(!app.show_help, "any key should close help");
        // F1 no longer opens help.
        app.handle_key(key(KeyCode::F(1)));
        assert!(!app.show_help, "F1 should no longer open help");
    }

    #[test]
    fn missing_command_line_file_opens_with_detected_language() {
        let dir = std::env::temp_dir().join(format!("ft-cmdline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("brand_new.py");
        let _ = std::fs::remove_file(&path);

        let app = App::new(vec![path.clone()]);
        assert_eq!(app.editors.len(), 1);
        assert_eq!(
            app.editor().language.name(),
            "Python",
            "a new .py file should be highlighted as Python, not the welcome buffer"
        );
        assert_eq!(app.editor().title(), "brand_new.py");
        assert!(!app.editor().is_untitled());
        assert!(app.editor().doc.text().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_files_still_shows_rust_welcome_buffer() {
        let app = empty_app();
        assert_eq!(app.editor().language.name(), "Rust");
        assert!(app.editor().doc.text().contains("Welcome to Fast Text"));
    }

    #[test]
    fn open_or_new_reports_errors_for_directories() {
        assert!(Editor::open_or_new(std::env::temp_dir()).is_err());
    }

    fn add_automation_script(app: &mut App, source: &str) {
        app.scripts.add_script("auto.tw", source);
        assert!(app.scripts.errors.is_empty(), "{:?}", app.scripts.errors);
    }

    #[test]
    fn immediate_actions_apply_without_waiting() {
        let mut app = empty_app();
        clear_and_type(&mut app, "");
        add_automation_script(
            &mut app,
            r#"
                fn go() {
                    insert("x")
                    set_cursor_style("block")
                }
                command("Go", "go")
            "#,
        );
        app.run_plugin_command("Go");
        assert!(!app.has_pending());
        assert_eq!(app.editor().doc.text(), "x");
        assert_eq!(app.cursor_style, CursorStyle::Block);
    }

    #[test]
    fn waits_schedule_actions_for_later_frames() {
        let mut app = empty_app();
        clear_and_type(&mut app, "");
        add_automation_script(
            &mut app,
            r#"
                fn animate() {
                    insert("a")
                    wait(1000)
                    insert("b")
                }
                command("Animate", "animate")
            "#,
        );
        app.run_plugin_command("Animate");
        // The step before the wait runs at once; the rest is queued.
        assert_eq!(app.editor().doc.text(), "a");
        assert!(app.has_pending());
        // Draining with a future clock applies the remainder.
        app.run_pending_at(Instant::now() + Duration::from_secs(5));
        assert_eq!(app.editor().doc.text(), "ab");
        assert!(!app.has_pending());
    }

    #[test]
    fn alt_r_opens_the_automation_prompt() {
        let mut app = empty_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT));
        assert!(matches!(app.prompt, Prompt::Automation { .. }));
    }

    #[test]
    fn run_automation_compiles_and_plays_a_file() {
        let dir = std::env::temp_dir().join(format!("ft-tui-auto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("demo.tw");
        std::fs::write(
            &path,
            r#"
                import "fast-text/automator"
                cursor("underline")
                pause(400)
                type_text("hi")
            "#,
        )
        .unwrap();

        let mut app = empty_app();
        clear_and_type(&mut app, "");
        app.run_automation(&path.to_string_lossy());
        assert!(!app.is_error, "unexpected error: {:?}", app.message);
        assert_eq!(app.cursor_style, CursorStyle::Underline);
        assert!(app.has_pending(), "the pause should defer the typing");
        app.run_pending_at(Instant::now() + Duration::from_secs(5));
        assert_eq!(app.editor().doc.text(), "hi");
        assert!(!app.has_pending());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_automation_reports_errors_without_running() {
        let dir = std::env::temp_dir().join(format!("ft-tui-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.tw");
        std::fs::write(&path, "import \"nope/definitely-missing\"\n").unwrap();

        let mut app = empty_app();
        app.run_automation(&path.to_string_lossy());
        assert!(app.is_error, "a missing import should surface as an error");
        assert!(!app.has_pending(), "nothing should run when compile fails");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn automation_prompt_runs_the_named_file() {
        let dir = std::env::temp_dir().join(format!("ft-tui-prompt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("greet.tw");
        std::fs::write(&path, "insert(\"hi\")\n").unwrap();

        let mut app = empty_app();
        clear_and_type(&mut app, "");
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT));
        for c in path.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.prompt, Prompt::None));
        assert!(!app.is_error, "unexpected error: {:?}", app.message);
        assert_eq!(app.editor().doc.text(), "hi");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ctrl_b_toggles_and_focuses_the_sidebar() {
        let mut app = empty_app();
        assert!(!app.sidebar.visible());
        app.handle_key(ctrl('b'));
        assert!(app.sidebar.visible());
        assert!(app.sidebar.focused());
        // Pressing it again while focused hides the explorer.
        app.handle_key(ctrl('b'));
        assert!(!app.sidebar.visible());
        assert!(!app.sidebar.focused());
    }

    #[test]
    fn escape_returns_focus_to_the_editor_without_hiding() {
        let mut app = empty_app();
        app.handle_key(ctrl('b'));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.sidebar.focused());
        assert!(app.sidebar.visible());
    }

    #[test]
    fn sidebar_enter_opens_the_selected_file() {
        let dir = std::env::temp_dir().join(format!("ft-tui-sidebar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alpha.txt"), "hello").unwrap();
        std::fs::write(dir.join("beta.txt"), "bye").unwrap();

        let mut app = empty_app();
        app.sidebar.set_root(dir.clone());
        app.sidebar.show();
        // Second entry is beta.txt; open it with Enter.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.editor().title(), "beta.txt");
        assert_eq!(app.editor().doc.text(), "bye");
        assert!(!app.sidebar.focused(), "focus should return to the editor");

        // While the explorer is unfocused, keys edit the buffer again.
        app.handle_key(key(KeyCode::Char('X')));
        assert_eq!(app.editor().doc.text(), "Xbye");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_folder_command_points_the_explorer() {
        let dir = std::env::temp_dir().join(format!("ft-tui-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = empty_app();
        app.open_folder(&dir.to_string_lossy());
        assert!(app.sidebar.visible());
        assert_eq!(app.sidebar.root(), canonical(&dir).as_path());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ctrl_shift_o_opens_the_folder_prompt() {
        let mut app = empty_app();
        let ev = KeyEvent::new(
            KeyCode::Char('O'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        app.handle_key(ev);
        assert!(matches!(app.prompt, Prompt::OpenFolder { .. }));
    }

    #[test]
    fn toggle_sidebar_is_a_builtin_command() {
        let mut app = empty_app();
        assert!(app.command_titles().contains(&"Toggle sidebar".to_string()));
        app.run_plugin_command("Toggle sidebar");
        assert!(app.sidebar.visible());
    }

    #[test]
    fn cursor_styles_are_builtin_commands() {
        let mut app = empty_app();
        app.run_plugin_command("Cursor: block");
        assert_eq!(app.cursor_style, CursorStyle::Block);
        app.run_plugin_command("Cursor: underline");
        assert_eq!(app.cursor_style, CursorStyle::Underline);
        app.run_plugin_command("Cursor: bar");
        assert_eq!(app.cursor_style, CursorStyle::Bar);
    }

    #[test]
    fn manage_plugins_is_a_builtin_command() {
        let mut app = empty_app();
        app.run_plugin_command("Manage plugins");
        assert!(app.plugin_manager.is_open());
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.plugin_manager.is_open());
    }

    #[test]
    fn ctrl_m_opens_the_plugin_manager() {
        let mut app = empty_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL));
        assert!(app.plugin_manager.is_open());
    }

    #[test]
    fn alt_m_opens_the_plugin_manager() {
        // Alt+M works in every terminal, unlike Ctrl+M which collapses to
        // Enter on terminals without the kitty keyboard protocol.
        let mut app = empty_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::ALT));
        assert!(app.plugin_manager.is_open());
    }

    #[test]
    fn plugin_manager_disables_a_plugin() {
        let dir = std::env::temp_dir().join(format!("ft-tui-manager-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("demo.tw"),
            "# Demo plugin.\nfn tick() { insert(\"TW\") }\ncommand_with_desc(\"TextWrite tick\", \"tick\", \"tick\")\n",
        )
        .unwrap();

        let mut app = empty_app();
        app.plugin_manager =
            crate::plugin_manager::PluginManager::discover(vec![dir.clone()], None);
        app.reload_plugins();
        assert!(app.scripts.has_command("TextWrite tick"));

        app.plugin_manager.open();
        assert_eq!(app.plugin_manager.selected, 0);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(
            !app.scripts.has_command("TextWrite tick"),
            "disabling a plugin removes its commands"
        );
        assert_eq!(app.plugin_manager.enabled_count(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_plugin_copies_into_the_plugin_dir() {
        let dir = std::env::temp_dir().join(format!("ft-tui-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.tw");
        std::fs::write(&source, "fn a() {}\ncommand(\"A\", \"a\")\n").unwrap();
        let plugins = dir.join("plugins");

        let mut app = empty_app();
        app.plugin_manager =
            crate::plugin_manager::PluginManager::discover(vec![plugins.clone()], None);
        app.reload_plugins();
        assert!(!app.scripts.has_command("A"));

        app.install_plugin(source.to_str().unwrap());
        assert!(
            plugins.join("source.tw").exists(),
            "plugin copied into place"
        );
        assert!(app.scripts.has_command("A"), "installed plugin is loaded");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_prompt_is_committed() {
        let dir = std::env::temp_dir().join(format!("ft-tui-prompt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("prompted.tw");
        std::fs::write(&source, "fn b() {}\ncommand(\"B\", \"b\")\n").unwrap();
        let plugins = dir.join("plugins");

        let mut app = empty_app();
        app.plugin_manager =
            crate::plugin_manager::PluginManager::discover(vec![plugins.clone()], None);
        app.reload_plugins();

        app.prompt = Prompt::InstallPlugin {
            input: source.to_string_lossy().into_owned(),
        };
        assert!(app.prompt.is_active());
        // Enter commits the prompt and installs the file.
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.prompt.is_active());
        assert!(plugins.join("prompted.tw").exists());
        assert!(app.scripts.has_command("B"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Tabs and mouse ----------------------------------------------------

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn click(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Down(MouseButton::Left), column, row)
    }

    /// A fake tab bar: each tab is 8 cells wide with a 1-cell close box.
    fn fake_tabs(app: &mut App) {
        app.tab_hits = (0..app.editors.len())
            .map(|i| {
                let x = (i as u16) * 10;
                TabHit {
                    index: i,
                    label: Rect::new(x, 0, 8, 1),
                    close: Rect::new(x + 8, 0, 1, 1),
                }
            })
            .collect();
    }

    #[test]
    fn ctrl_tab_cycles_between_tabs() {
        let mut app = empty_app();
        app.new_buffer();
        app.new_buffer();
        assert_eq!(app.active, 2);

        let next = KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL);
        app.handle_key(next);
        assert_eq!(app.active, 0, "Ctrl+Tab wraps forward");
        let prev = KeyEvent::new(KeyCode::BackTab, KeyModifiers::CONTROL);
        app.handle_key(prev);
        assert_eq!(app.active, 2, "Ctrl+Shift+Tab wraps back");
    }

    #[test]
    fn alt_digits_set_the_cursor_style() {
        let mut app = empty_app();
        assert_eq!(app.cursor_style, CursorStyle::Bar);
        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::ALT));
        assert_eq!(app.cursor_style, CursorStyle::Block);
        app.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::ALT));
        assert_eq!(app.cursor_style, CursorStyle::Underline);
        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::ALT));
        assert_eq!(app.cursor_style, CursorStyle::Bar);
    }

    #[test]
    fn ctrl_w_closes_the_active_tab() {
        let mut app = empty_app();
        app.new_buffer();
        assert_eq!(app.editors.len(), 2);
        assert_eq!(app.active, 1);
        app.handle_key(ctrl('w'));
        assert_eq!(app.editors.len(), 1);
        assert_eq!(app.active, 0);
    }

    #[test]
    fn clicking_a_tab_switches_to_it() {
        let mut app = empty_app();
        app.new_buffer();
        app.new_buffer();
        app.active = 0;
        fake_tabs(&mut app);

        app.handle_mouse(click(12, 0)); // inside the second tab
        assert_eq!(app.active, 1);
    }

    #[test]
    fn clicking_a_tab_close_box_closes_it() {
        let mut app = empty_app();
        app.new_buffer();
        app.new_buffer();
        assert_eq!(app.editors.len(), 3);
        fake_tabs(&mut app);

        app.handle_mouse(click(18, 0)); // the close box of the second tab
        assert_eq!(app.editors.len(), 2);
    }

    #[test]
    fn clicking_a_file_in_the_explorer_opens_it() {
        let dir = std::env::temp_dir().join(format!("ft-tui-click-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alpha.txt"), "hello").unwrap();

        let mut app = empty_app();
        app.sidebar.set_root(dir.clone());
        app.sidebar.show();
        app.sidebar
            .set_area(Rect::new(0, 0, 20, 10), Rect::new(1, 1, 18, 8));
        app.sidebar.set_view_height(8);

        app.handle_mouse(click(2, 1));
        assert_eq!(app.editor().title(), "alpha.txt");
        assert_eq!(app.editor().doc.text(), "hello");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scrolling_over_the_explorer_moves_the_selection() {
        let dir = std::env::temp_dir().join(format!("ft-tui-wheel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
            std::fs::write(dir.join(name), "x").unwrap();
        }

        let mut app = empty_app();
        app.sidebar.set_root(dir.clone());
        app.sidebar.show();
        app.sidebar
            .set_area(Rect::new(0, 0, 20, 10), Rect::new(1, 1, 18, 8));
        app.sidebar.set_view_height(8);
        assert_eq!(app.sidebar.selected(), 0);

        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 2, 1));
        assert!(app.sidebar.selected() > 0);
        app.handle_mouse(mouse(MouseEventKind::ScrollUp, 2, 1));
        assert_eq!(app.sidebar.selected(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn app_with(config: &str) -> App {
        App::with_config(Vec::new(), Config::parse(config))
    }

    #[test]
    fn config_settings_are_applied_at_startup() {
        let app = app_with(
            "[editor]\ncursor_style = \"underline\"\ntheme = \"light\"\nsidebar = true\n[sidebar]\nhidden = true\n",
        );
        assert_eq!(app.cursor_style, CursorStyle::Underline);
        assert!(!app.dark_theme);
        assert!(app.sidebar.visible());
        assert!(app.sidebar.show_hidden());
    }

    #[test]
    fn a_config_warning_is_shown_as_an_error() {
        let app = app_with("[editor]\ncursor_style = \"squiggly\"\n");
        assert!(app.is_error);
        assert!(app.message.unwrap().contains("cursor_style"));
    }

    #[test]
    fn a_user_binding_beats_the_built_in_one() {
        // Ctrl+P normally opens the palette; the config steals it.
        let mut app = app_with("[keys]\n\"ctrl+p\" = \"toggle_sidebar\"\n");
        assert!(!app.sidebar.visible());
        app.handle_key(ctrl('p'));
        assert!(app.sidebar.visible());
        assert!(!app.prompt.is_active(), "the palette must not open");
    }

    #[test]
    fn a_user_binding_can_reach_a_movement_free_command() {
        let mut app = app_with("[keys]\n\"f1\" = \"help\"\n");
        assert!(!app.show_help);
        app.handle_key(key(KeyCode::F(1)));
        assert!(app.show_help);
    }

    #[test]
    fn a_user_binding_may_name_any_command_title() {
        let mut app = app_with("[keys]\n\"alt+g\" = \"Manage plugins\"\n");
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::ALT));
        assert!(app.plugin_manager.is_open());
    }

    #[test]
    fn an_unknown_command_name_reports_an_error() {
        let mut app = app_with("[keys]\n\"alt+z\" = \"does-not-exist\"\n");
        app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::ALT));
        assert!(app.is_error);
        assert!(app.message.unwrap().contains("no command named"));
    }

    #[test]
    fn run_command_accepts_ids_titles_and_plugin_commands() {
        let mut app = empty_app();
        assert!(app.run_command("cursor_block"));
        assert_eq!(app.cursor_style, CursorStyle::Block);
        assert!(app.run_command("Toggle sidebar"));
        assert!(app.sidebar.visible());
        assert!(!app.run_command("no such command"));
    }

    #[test]
    fn the_welcome_screen_mentions_the_config_file() {
        let app = empty_app();
        assert!(app.editor().doc.text().contains("config.toml"));
    }

    // --- Editing ergonomics ------------------------------------------------

    #[test]
    fn pairs_auto_close_and_skip_over() {
        let mut app = empty_app();
        set_text(&mut app, "");
        app.handle_key(key(KeyCode::Char('(')));
        assert_eq!(app.editor().doc.text(), "()");
        assert_eq!(app.editor().doc.cursor(), 1);
        // Typing the closer steps over the auto-inserted one.
        app.handle_key(key(KeyCode::Char(')')));
        assert_eq!(app.editor().doc.text(), "()");
        assert_eq!(app.editor().doc.cursor(), 2);
    }

    #[test]
    fn typing_a_quote_wraps_the_selection() {
        let mut app = empty_app();
        set_text(&mut app, "word");
        app.editor_mut().doc.select_all();
        app.handle_key(key(KeyCode::Char('"')));
        assert_eq!(app.editor().doc.text(), "\"word\"");
    }

    #[test]
    fn newline_between_braces_indents_and_opens_a_block() {
        let mut app = empty_app();
        set_text(&mut app, "");
        app.handle_key(key(KeyCode::Char('{')));
        assert_eq!(app.editor().doc.text(), "{}");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.editor().doc.text(), "{\n    \n}");
        // The caret sits at the end of the freshly indented inner line.
        let pos = app.editor().doc.cursor_position();
        assert_eq!((pos.line, pos.col), (1, 4));
    }

    #[test]
    fn duplicate_line_copies_the_current_line() {
        let mut app = empty_app();
        set_text(&mut app, "one\ntwo");
        // Caret at the end of the second line.
        app.editor_mut().doc.move_doc_end(false);
        app.duplicate_line();
        assert_eq!(app.editor().doc.text(), "one\ntwo\ntwo");
        assert_eq!(app.editor().doc.cursor_position().line, 2);
    }

    #[test]
    fn delete_line_removes_the_current_line() {
        let mut app = empty_app();
        set_text(&mut app, "one\ntwo\nthree");
        app.editor_mut().doc.move_doc_end(false);
        app.delete_line();
        assert_eq!(app.editor().doc.text(), "one\ntwo");
        assert_eq!(app.editor().doc.cursor_position().line, 1);
    }

    #[test]
    fn move_line_down_swaps_with_the_next() {
        let mut app = empty_app();
        set_text(&mut app, "one\ntwo");
        app.editor_mut().doc.move_doc_end(false); // on "two"
        app.move_line_up();
        assert_eq!(app.editor().doc.text(), "two\none");
        assert_eq!(app.editor().doc.cursor_position().line, 0);
    }

    #[test]
    fn toggle_comment_adds_and_removes_a_marker() {
        let mut app = empty_app();
        set_text(&mut app, "let x = 1;");
        app.toggle_comment();
        assert_eq!(app.editor().doc.text(), "// let x = 1;");
        app.toggle_comment();
        assert_eq!(app.editor().doc.text(), "let x = 1;");
    }

    #[test]
    fn toggle_comment_uses_the_language_marker() {
        let mut app = empty_app();
        app.editor_mut().language = fast_text_syntax::LanguageId::Python;
        set_text(&mut app, "x = 1");
        app.toggle_comment();
        assert_eq!(app.editor().doc.text(), "# x = 1");
    }

    #[test]
    fn bracket_match_is_used_when_pairs_are_typed() {
        let mut app = empty_app();
        set_text(&mut app, "");
        app.handle_key(key(KeyCode::Char('[')));
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.editor().doc.text(), "[]");
        assert_eq!(app.editor().doc.cursor(), 2);
    }

    // --- Macros ------------------------------------------------------------

    #[test]
    fn macro_records_and_replays() {
        let mut app = empty_app();
        set_text(&mut app, "");
        app.handle_key(key(KeyCode::F(6)));
        assert!(app.recording.is_some());
        app.handle_key(key(KeyCode::Char('h')));
        app.handle_key(key(KeyCode::Char('i')));
        app.handle_key(key(KeyCode::F(6)));
        assert!(app.recording.is_none());
        assert_eq!(app.editor().doc.text(), "hi");
        app.handle_key(key(KeyCode::F(7)));
        assert_eq!(app.editor().doc.text(), "hihi");
    }

    #[test]
    fn named_macro_is_saved_and_run_from_the_palette() {
        let mut app = empty_app();
        set_text(&mut app, "");
        app.handle_key(key(KeyCode::F(6)));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::F(6)));
        app.save_macro("tick");
        assert!(app.command_titles().contains(&"Macro: tick".to_string()));
        app.run_plugin_command("Macro: tick");
        assert_eq!(app.editor().doc.text(), "xx");
    }

    // --- LSP conversions ---------------------------------------------------

    #[test]
    fn utf16_columns_account_for_surrogate_pairs() {
        // One emoji is two UTF-16 units but a single character.
        assert_eq!(utf16_to_char_col("a😀b", 0), 0);
        assert_eq!(utf16_to_char_col("a😀b", 1), 1);
        assert_eq!(utf16_to_char_col("a😀b", 3), 2);
        assert_eq!(utf16_to_char_col("a😀b", 4), 3);
    }

    #[test]
    fn lsp_ranges_convert_to_character_indices() {
        let mut doc = Document::new();
        doc.insert_str("one\ntw😀o\nthree");
        let range = LspRange {
            start: fast_text_lsp::LspPosition {
                line: 1,
                character: 0,
            },
            end: fast_text_lsp::LspPosition {
                line: 1,
                character: 5,
            },
        };
        let (start, end) = lsp_range_to_chars(&doc, &range);
        assert_eq!(start, 4);
        assert_eq!(end, 8);
        assert_eq!(doc.buffer().slice(start, end), "tw😀o");
    }

    // --- Find in files -----------------------------------------------------

    #[test]
    fn find_in_files_fills_the_picker() {
        let dir = std::env::temp_dir().join(format!("ft-tui-fif-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "alpha\nneedle here\n").unwrap();
        std::fs::write(dir.join("b.txt"), "nothing to see\n").unwrap();

        let mut app = empty_app();
        app.sidebar.set_root(canonical(&dir));
        app.open_find_in_files("needle");
        assert!(app.picker.is_open());
        assert_eq!(app.picker_kind, PickerKind::FindInFiles);
        assert_eq!(app.picker.items().len(), 1);
        // Enter jumps to the hit.
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.picker.is_open());
        assert_eq!(app.editor().title(), "a.txt");
        assert_eq!(app.editor().doc.cursor_position().line, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_in_files_query_typing_filters() {
        let dir = std::env::temp_dir().join(format!("ft-tui-fifq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "alpha\nbeta\n").unwrap();

        let mut app = empty_app();
        app.sidebar.set_root(canonical(&dir));
        app.run_plugin_command("Find in files…");
        assert!(matches!(app.prompt, Prompt::FindInFiles { .. }));
        for c in "alpha".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(app.picker.is_open());
        assert_eq!(app.find_query, "alpha");
        assert_eq!(app.picker.items().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picker_escape_closes_without_jumping() {
        let mut app = empty_app();
        app.picker.open("Test", vec![PickerItem::new("row")]);
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.picker.is_open());
        assert!(matches!(app.picker_action, PickerAction::None));
    }

    #[test]
    fn picker_rows_activate_on_enter() {
        let mut app = empty_app();
        app.picker_action = PickerAction::Locations(vec![JumpTarget {
            path: PathBuf::from("missing.txt"),
            line: 0,
            col: 0,
        }]);
        app.picker
            .open("Locations", vec![PickerItem::new("missing.txt")]);
        app.handle_key(key(KeyCode::Enter));
        // The picker closes and tries to open the target (which fails).
        assert!(!app.picker.is_open());
    }

    #[test]
    fn commands_are_exposed_in_the_palette() {
        let app = empty_app();
        let titles = app.command_titles();
        for expected in [
            "Find in files…",
            "Document symbols",
            "Rename symbol…",
            "Format document",
            "Code actions",
            "Toggle comment",
            "Duplicate line",
            "Delete line",
            "Move line up",
            "Move line down",
            "Record macro",
            "Stop recording",
            "Replay macro",
            "Save macro…",
            "Check for updates",
        ] {
            assert!(
                titles.contains(&expected.to_string()),
                "missing command: {expected}"
            );
        }
    }

    /// An app whose update record is pointed at a throwaway directory, so
    /// tests never touch the real `~/.config/fast-text`. Every call gets its
    /// own directory: tests run in parallel threads inside one process, so a
    /// shared file would be clobbered by whichever test wrote to it last.
    fn update_test_app() -> (App, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fast-text-tui-update-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("update-record");
        let mut app = empty_app();
        app.update_checks_enabled = true;
        app.update_record_path = Some(record.clone());
        (app, record)
    }

    #[test]
    fn alt_u_checks_and_offers_a_newer_release() {
        let (mut app, record) = update_test_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::ALT));
        assert_eq!(app.update, UpdateState::Checking);

        // The worker's answer arrives on the next tick.
        app.apply_update_event(UpdateEvent::Checked {
            manual: true,
            result: Ok(Some(Release::from_tag("v9.9.9"))),
        });
        assert_eq!(
            app.update,
            UpdateState::Available {
                tag: "v9.9.9".to_string()
            }
        );

        // "No" closes the dialog and remembers the declined tag.
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.update, UpdateState::Idle);
        let saved = fast_text_update::load_record(&record);
        assert_eq!(saved.declined.as_deref(), Some("v9.9.9"));
    }

    #[test]
    fn accepting_an_update_installs_then_offers_a_restart() {
        let (mut app, _record) = update_test_app();
        let alt_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::ALT);
        app.handle_key(alt_u); // manual check
        app.apply_update_event(UpdateEvent::Checked {
            manual: true,
            result: Ok(Some(Release::from_tag("v9.9.9"))),
        });
        app.handle_key(key(KeyCode::Char('y')));
        assert!(matches!(app.update, UpdateState::Installing { .. }));

        app.apply_update_event(UpdateEvent::Progress("Unpacking…".to_string()));
        match &app.update {
            UpdateState::Installing { progress, .. } => assert_eq!(progress, "Unpacking…"),
            other => panic!("unexpected state: {other:?}"),
        }

        app.apply_update_event(UpdateEvent::Installed {
            tag: "v9.9.9".to_string(),
            result: Ok(vec!["fast-text".to_string()]),
        });
        assert_eq!(
            app.update,
            UpdateState::ReadyToRestart {
                tag: "v9.9.9".to_string()
            }
        );

        // Restarting quits this instance (no process is spawned under test).
        app.handle_key(key(KeyCode::Enter));
        assert!(app.should_quit);
    }

    #[test]
    fn a_failed_install_shows_the_error_and_closes_on_a_key() {
        let (mut app, _record) = update_test_app();
        app.apply_update_event(UpdateEvent::Installed {
            tag: "v9.9.9".to_string(),
            result: Err("the download failed".to_string()),
        });
        match &app.update {
            UpdateState::Failed { error, .. } => assert!(error.contains("download failed")),
            other => panic!("unexpected state: {other:?}"),
        }
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.update, UpdateState::Idle);
    }

    #[test]
    fn an_up_to_date_manual_check_reports_so_quietly() {
        let (mut app, _record) = update_test_app();
        app.apply_update_event(UpdateEvent::Checked {
            manual: true,
            result: Ok(None),
        });
        assert_eq!(app.update, UpdateState::Idle);
        assert!(app.message.is_some(), "a manual check should say something");
    }

    #[test]
    fn a_release_equal_to_this_build_is_not_offered() {
        let (mut app, _record) = update_test_app();
        app.apply_update_event(UpdateEvent::Checked {
            manual: true,
            result: Ok(Some(Release::from_tag(env!("CARGO_PKG_VERSION")))),
        });
        assert_eq!(
            app.update,
            UpdateState::Idle,
            "the release already installed must not be offered"
        );
        assert!(
            app.message
                .as_deref()
                .is_some_and(|m| m.contains("up to date")),
            "a manual check should say it is up to date"
        );
    }

    #[test]
    fn a_recent_automatic_check_is_throttled_but_manual_ones_are_not() {
        let (mut app, record) = update_test_app();
        fast_text_update::save_record(
            &record,
            &fast_text_update::CheckRecord {
                last_check: Some(fast_text_update::now_secs()),
                latest: Some("v9.9.9".to_string()),
                declined: Some("v9.9.9".to_string()),
            },
        )
        .unwrap();

        app.begin_update_check(false);
        assert_eq!(
            app.update,
            UpdateState::Idle,
            "a fresh record must throttle"
        );

        app.begin_update_check(true);
        assert_eq!(
            app.update,
            UpdateState::Checking,
            "manual checks always run"
        );

        // A release the user said no to stays quiet on the next day's check.
        fast_text_update::save_record(
            &record,
            &fast_text_update::CheckRecord {
                last_check: Some(fast_text_update::now_secs() - 2 * 24 * 60 * 60),
                latest: Some("v9.9.9".to_string()),
                declined: Some("v9.9.9".to_string()),
            },
        )
        .unwrap();
        app.update = UpdateState::Idle;
        app.begin_update_check(false);
        assert_eq!(
            app.update,
            UpdateState::Idle,
            "a declined release must not be offered again"
        );
    }

    #[test]
    fn update_checks_can_be_switched_off() {
        let (mut app, _record) = update_test_app();
        app.update_checks_enabled = false;
        app.begin_update_check(true);
        assert_eq!(app.update, UpdateState::Idle);
        assert!(
            app.is_error,
            "a manual check with checks off should explain"
        );
    }

    #[test]
    fn the_check_updates_command_is_registered() {
        let (mut app, _record) = update_test_app();
        assert!(app.run_command_id("check_updates"));
        app.update = UpdateState::Idle;
        assert!(app.run_builtin_command("Check for updates"));
    }

    #[test]
    fn ctrl_d_selects_the_next_occurrence() {
        let mut app = empty_app();
        clear_and_type(&mut app, "foo bar foo");
        app.editor_mut().doc.set_cursor(1); // inside the first "foo"
        app.select_next_occurrence(); // selects the word
        assert!(app.editor().doc.has_selection());
        app.select_next_occurrence(); // adds the following occurrence
        assert!(app.editor().doc.has_multi_cursor());
    }

    #[test]
    fn alt_up_moves_the_current_line() {
        let mut app = empty_app();
        set_text(&mut app, "one\ntwo");
        app.editor_mut().doc.move_doc_end(false); // on "two"
        let ev = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        app.handle_key(ev);
        assert_eq!(app.editor().doc.text(), "two\none");
    }

    #[test]
    fn alt_shift_down_adds_a_secondary_caret() {
        let mut app = empty_app();
        set_text(&mut app, "one\ntwo");
        assert!(!app.editor().doc.has_multi_cursor());
        let ev = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT | KeyModifiers::SHIFT);
        app.handle_key(ev);
        assert!(app.editor().doc.has_multi_cursor());
    }

    #[test]
    fn bundled_plugins_load_without_errors() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let manager = crate::plugin_manager::PluginManager::discover(vec![dir], None);
        assert!(!manager.entries.is_empty(), "expected bundled plugins");
        for entry in &manager.entries {
            assert!(
                entry.error.is_none(),
                "{} failed to load: {:?}",
                entry.name,
                entry.error
            );
        }
    }
}
