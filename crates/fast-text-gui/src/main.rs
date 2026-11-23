//! Fast Text — native desktop frontend.
//!
//! Built on `eframe`/`egui`. The editing widget is a multiline `TextEdit`
//! painted by the same `fast-text-syntax` highlighter the terminal uses, and
//! files are loaded and saved through `fast-text-core`'s `Document`.

mod highlight;

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui;
use fast_text_core::fuzzy::{fuzzy_match, FileFinder, FileHit};
use fast_text_core::Document;
use fast_text_plugin::{command_for, command_title, EditorCommand, KeyName, KeySpec};
use fast_text_syntax::LanguageId;
use fast_text_theme::{Rgb, Theme};
use fast_text_update::Release;
use highlight::{highlight_job, Palette};
use std::sync::Arc;

/// The version this build was compiled at, shown in the title and status bars.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The first-party logo, packaged with the crate so it builds from crates.io.
const LOGO_SVG: &[u8] = include_bytes!("../assets/fast-text-logo.svg");

/// Rasterizes `LOGO_SVG` into the RGBA icon egui expects.
fn app_icon() -> Option<egui::IconData> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(LOGO_SVG, &options).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Some(egui::IconData {
        rgba: pixmap.take_demultiplied(),
        width: size.width(),
        height: size.height(),
    })
}

/// Id of the toolbar's Find field, so `Ctrl+F` can focus it.
fn find_input() -> egui::Id {
    egui::Id::new("fast-text-find")
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1000.0, 700.0])
        .with_min_inner_size([480.0, 320.0])
        .with_title(format!("Fast Text v{VERSION}"));
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let auto_check = update_checks_enabled();
    // Clear out the `.old` twins a previous self-update left behind.
    fast_text_update::cleanup_old_binaries();

    eframe::run_native(
        "Fast Text",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, files, auto_check)))),
    )
}

/// `FAST_TEXT_UPDATE_CHECK=0` (or off/false/no) turns the startup check off.
fn update_checks_enabled() -> bool {
    match std::env::var("FAST_TEXT_UPDATE_CHECK") {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        ),
        Err(_) => true,
    }
}

/// One open document.
///
/// The visible text is a `String` because egui's `TextEdit` edits one
/// directly, while `doc` owns the same text as a rope with the undo history.
/// [`Buffer::sync`] mirrors each frame's edits into the rope as a single
/// transaction, which is what makes undo persistent and identical to the
/// terminal's.
struct Buffer {
    text: String,
    doc: Document,
    language: LanguageId,
    path: Option<PathBuf>,
    dirty: bool,
}

impl Buffer {
    fn untitled() -> Self {
        let doc = Document::from_str(
            "// Welcome to Fast Text\n//\n// A native egui frontend over fast-text-core.\n\nfn main() {\n    println!(\"Hello, world!\");\n}\n",
        );
        Buffer {
            text: doc.text(),
            doc,
            language: LanguageId::Rust,
            path: None,
            dirty: false,
        }
    }

    fn from_path(path: PathBuf) -> std::io::Result<Self> {
        let doc = Document::from_file(&path).map_err(|e| std::io::Error::other(e.to_string()))?;
        // Restore the undo stack saved when this buffer was last closed, so a
        // crash or a closed window does not take the history with it.
        let doc = match fast_text_core::undo::load(&path) {
            Some(mut restored) => {
                if restored.text() != doc.text() {
                    restored.mark_dirty();
                }
                restored
            }
            None => doc,
        };
        let language = LanguageId::from_path(&path.to_string_lossy());
        let mut buffer = Buffer {
            text: doc.text(),
            doc,
            language,
            path: Some(path),
            dirty: false,
        };
        buffer.dirty = buffer.doc.is_dirty();
        Ok(buffer)
    }

    fn title(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned()),
            None => "untitled".to_string(),
        }
    }

    /// Applies the visible text edits to the rope as one transaction.
    ///
    /// Returns true when the text changed. A single [`Document::apply_edits`]
    /// call keeps a coalesced typing burst undoable in one step, exactly like
    /// the terminal frontend.
    fn sync(&mut self) -> bool {
        let old = self.doc.text();
        if old == self.text {
            return false;
        }
        let before: Vec<char> = old.chars().collect();
        let after: Vec<char> = self.text.chars().collect();
        let mut start = 0;
        while start < before.len() && start < after.len() && before[start] == after[start] {
            start += 1;
        }
        let mut old_end = before.len();
        let mut new_end = after.len();
        while old_end > start && new_end > start && before[old_end - 1] == after[new_end - 1] {
            old_end -= 1;
            new_end -= 1;
        }
        let inserted: String = after[start..new_end].iter().collect();
        self.doc.apply_edits(&[(start, old_end, inserted)]);
        self.dirty = true;
        true
    }

    fn undo(&mut self) -> bool {
        self.sync();
        if !self.doc.can_undo() {
            return false;
        }
        self.doc.undo();
        self.text = self.doc.text();
        self.dirty = true;
        true
    }

    fn redo(&mut self) -> bool {
        self.sync();
        if !self.doc.can_redo() {
            return false;
        }
        self.doc.redo();
        self.text = self.doc.text();
        self.dirty = true;
        true
    }

    fn save(&mut self) -> Result<(), String> {
        let Some(path) = self.path.clone() else {
            return Err("no path (use Save As)".into());
        };
        self.sync();
        self.doc.save_as(&path).map_err(|e| e.to_string())?;
        self.dirty = false;
        let _ = fast_text_core::undo::save(&self.doc);
        Ok(())
    }

    fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        self.sync();
        self.doc.save_as(&path).map_err(|e| e.to_string())?;
        self.language = LanguageId::from_path(&path.to_string_lossy());
        self.path = Some(path);
        self.dirty = false;
        let _ = fast_text_core::undo::save(&self.doc);
        Ok(())
    }

    /// Writes the undo history if the buffer has a path.
    fn persist_undo(&self) {
        if self.path.is_some() {
            let _ = fast_text_core::undo::save(&self.doc);
        }
    }
}

/// An in-app file prompt (avoids native-dialog dependencies).
enum Dialog {
    None,
    Open(String),
    SaveAs(String),
}

/// What the update window is currently showing.
enum UpdateState {
    /// Nothing to show.
    Idle,
    /// A release check is running in the background.
    Checking,
    /// A newer release exists, by tag.
    Available(String),
    /// The archive is being downloaded and unpacked.
    Installing {
        /// The release tag being installed.
        tag: String,
        /// Short progress text.
        message: String,
        /// Completion fraction for the progress bar.
        fraction: Option<f32>,
    },
    /// Installed; a restart is needed.
    ReadyToRestart(String),
    /// The last check or install failed.
    Failed(String),
}

/// A button the update window asked us to act on.
enum UpdateAction {
    /// Download and install this tag.
    Install(String),
    /// Start the fresh executable and close this window.
    Restart,
    /// Close the window.
    Dismiss,
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
    Progress {
        message: String,
        fraction: Option<f32>,
    },
    /// The install finished (or did not).
    Installed {
        /// The release tag that was being installed.
        tag: String,
        /// The executables replaced, or the reason there are none.
        result: Result<Vec<String>, String>,
    },
}

struct App {
    buffers: Vec<Buffer>,
    active: usize,
    /// The active palette, loaded from the shared theme schema.
    theme: Theme,
    theme_name: String,
    /// User themes found in the per-user theme directory.
    user_themes: Vec<Theme>,
    dialog: Dialog,
    find: String,
    status: String,
    /// Cached workspace index for the fuzzy finder.
    finder: Option<FileFinder>,
    finder_query: String,
    finder_selected: usize,
    show_finder: bool,
    palette_query: String,
    palette_selected: usize,
    show_palette: bool,
    /// When undo snapshots were last written, for the periodic autosave.
    last_persist: Instant,
    update: UpdateState,
    update_rx: Option<mpsc::Receiver<UpdateEvent>>,
    update_tx: Option<mpsc::Sender<UpdateEvent>>,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>, auto_check: bool) -> Self {
        let mut buffers = Vec::new();
        for path in files {
            match Buffer::from_path(path) {
                Ok(b) => buffers.push(b),
                Err(e) => eprintln!("fast-text-gui: {e}"),
            }
        }
        if buffers.is_empty() {
            buffers.push(Buffer::untitled());
        }

        let user_themes = Theme::user_dir()
            .map(|dir| Theme::load_dir(dir).0)
            .unwrap_or_default();
        let (update_tx, update_rx) = mpsc::channel();
        let mut app = App {
            buffers,
            active: 0,
            theme: Theme::dark(),
            theme_name: "dark".to_string(),
            user_themes,
            dialog: Dialog::None,
            find: String::new(),
            status: String::new(),
            finder: None,
            finder_query: String::new(),
            finder_selected: 0,
            show_finder: false,
            palette_query: String::new(),
            palette_selected: 0,
            show_palette: false,
            last_persist: Instant::now(),
            update: UpdateState::Idle,
            update_rx: Some(update_rx),
            update_tx: Some(update_tx),
        };
        if auto_check {
            app.begin_update_check(false);
        }
        app.apply_theme(&_cc.egui_ctx);
        app
    }

    /// Every selectable theme: the built-ins then user themes.
    fn themes(&self) -> Vec<(String, Theme)> {
        let mut themes: Vec<(String, Theme)> = Theme::names()
            .iter()
            .filter_map(|name| Theme::builtin(name).map(|theme| ((*name).to_string(), theme)))
            .collect();
        for theme in &self.user_themes {
            themes.push((theme.name.to_ascii_lowercase(), theme.clone()));
        }
        themes
    }

    /// Switches to a theme by name, returning false when it is unknown.
    fn set_theme(&mut self, ctx: &egui::Context, name: &str) -> bool {
        let found = Theme::builtin(name)
            .or_else(|| {
                self.user_themes
                    .iter()
                    .find(|t| t.name.eq_ignore_ascii_case(name))
                    .cloned()
            })
            .map(|theme| (name.to_ascii_lowercase(), theme));
        match found {
            Some((name, theme)) => {
                self.theme = theme;
                self.theme_name = name;
                self.apply_theme(ctx);
                true
            }
            None => false,
        }
    }

    /// Cycles to the next theme (`Alt+T`).
    fn cycle_theme(&mut self, ctx: &egui::Context) {
        let themes = self.themes();
        if themes.is_empty() {
            return;
        }
        let next = themes
            .iter()
            .position(|(name, _)| *name == self.theme_name)
            .map(|i| (i + 1) % themes.len())
            .unwrap_or(0);
        let (name, theme) = themes[next].clone();
        self.theme = theme;
        self.theme_name = name.clone();
        self.apply_theme(ctx);
        self.status = format!("Theme: {name}");
    }

    /// Starts a release check in the background.
    fn begin_update_check(&mut self, manual: bool) {
        if !update_checks_enabled() {
            if manual {
                self.status = "Update checks are off (FAST_TEXT_UPDATE_CHECK=0)".into();
            }
            return;
        }
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Installing { .. }
        ) {
            return;
        }
        self.update = UpdateState::Checking;
        if manual {
            self.status = "Checking for updates…".into();
        }
        if let Some(tx) = self.update_tx.clone() {
            std::thread::spawn(move || {
                let result = fast_text_update::latest_release(10);
                let _ = tx.send(UpdateEvent::Checked { manual, result });
            });
        }
    }

    /// Downloads the release in the background and swaps the binaries in place.
    fn begin_update_install(&mut self, tag: String) {
        self.update = UpdateState::Installing {
            tag: tag.clone(),
            message: "Starting…".into(),
            fraction: Some(0.02),
        };
        if let Some(tx) = self.update_tx.clone() {
            let progress = tx.clone();
            std::thread::spawn(move || {
                let result = fast_text_update::download_and_install(&tag, &mut |p| {
                    let _ = progress.send(UpdateEvent::Progress {
                        message: p.message,
                        fraction: p.fraction,
                    });
                });
                let _ = tx.send(UpdateEvent::Installed { tag, result });
            });
        }
    }

    /// Starts the freshly updated executable and closes this window.
    fn restart_now(&mut self, ctx: &egui::Context) {
        match fast_text_update::restart_command().map(|mut cmd| cmd.spawn()) {
            Ok(Ok(_)) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Ok(Err(e)) | Err(e) => {
                self.status = format!("Could not restart: {e}");
                self.update = UpdateState::Idle;
            }
        }
    }

    /// Drains messages from the update worker.
    fn poll_updates(&mut self) {
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

    /// Folds one worker message into the window state.
    fn apply_update_event(&mut self, event: UpdateEvent) {
        match event {
            UpdateEvent::Checked { manual, result } => match result {
                Ok(Some(release)) => {
                    if fast_text_update::is_newer(&release.version, VERSION) {
                        self.status = format!("Fast Text {} is available", release.tag);
                        self.update = UpdateState::Available(release.tag);
                    } else {
                        if manual {
                            self.status = format!("Fast Text v{VERSION} is up to date");
                        }
                        self.update = UpdateState::Idle;
                    }
                }
                Ok(None) => {
                    if manual {
                        self.status = format!("Fast Text v{VERSION} is up to date");
                    }
                    self.update = UpdateState::Idle;
                }
                Err(error) => {
                    if manual {
                        self.status = format!("Update check failed: {error}");
                    }
                    // An offline machine at startup is not an error worth a popup.
                    self.update = UpdateState::Idle;
                }
            },
            UpdateEvent::Progress { message, fraction } => {
                if let UpdateState::Installing { tag, .. } = &self.update {
                    self.update = UpdateState::Installing {
                        tag: tag.clone(),
                        message,
                        fraction,
                    };
                }
            }
            UpdateEvent::Installed { tag, result } => match result {
                Ok(_replaced) => {
                    self.status = format!("{tag} installed — restart to finish");
                    self.update = UpdateState::ReadyToRestart(tag);
                }
                Err(error) => {
                    self.status = format!("Update failed: {error}");
                    self.update = UpdateState::Failed(error);
                }
            },
        }
    }

    /// The update modal: offers a release, shows progress, offers a restart.
    fn update_window(&mut self, ctx: &egui::Context) {
        let mut action = None;
        match &self.update {
            UpdateState::Idle => return,
            UpdateState::Checking => {
                egui::Window::new("Update check")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(format!("Checking for a newer Fast Text (v{VERSION})…"));
                        ui.label("You can keep editing while this runs.");
                    });
            }
            UpdateState::Available(tag) => {
                egui::Window::new("Update available")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(format!("Fast Text {tag} has been released."));
                        ui.label(format!("You are running v{VERSION}."));
                        ui.horizontal(|ui| {
                            if ui.button("Yes, update").clicked() {
                                action = Some(UpdateAction::Install(tag.clone()));
                            }
                            if ui.button("Not now").clicked() {
                                action = Some(UpdateAction::Dismiss);
                            }
                        });
                    });
            }
            UpdateState::Installing {
                tag,
                message,
                fraction,
            } => {
                egui::Window::new(format!("Installing {tag}"))
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(message);
                        let progress = fraction.unwrap_or(0.0).clamp(0.0, 1.0);
                        let text = if let Some(f) = fraction {
                            format!("{}%", (f * 100.0).round() as u16)
                        } else {
                            "Working…".to_string()
                        };
                        let bar = egui::ProgressBar::new(progress)
                            .text(text)
                            .animate(fraction.is_none());
                        ui.add(bar);
                        ui.label("Keep this window open until it finishes.");
                    });
            }
            UpdateState::ReadyToRestart(tag) => {
                egui::Window::new("Update installed")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(format!("Fast Text {tag} is installed."));
                        ui.label("Restart now to start using it.");
                        ui.horizontal(|ui| {
                            if ui.button("Restart now").clicked() {
                                action = Some(UpdateAction::Restart);
                            }
                            if ui.button("Later").clicked() {
                                action = Some(UpdateAction::Dismiss);
                            }
                        });
                    });
            }
            UpdateState::Failed(error) => {
                egui::Window::new("Update failed")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(error);
                        if ui.button("Close").clicked() {
                            action = Some(UpdateAction::Dismiss);
                        }
                    });
            }
        }
        match action {
            Some(UpdateAction::Install(tag)) => self.begin_update_install(tag),
            Some(UpdateAction::Restart) => self.restart_now(ctx),
            Some(UpdateAction::Dismiss) => self.update = UpdateState::Idle,
            None => {}
        }
    }

    /// Applies the shared theme to egui's native widgets.
    fn apply_theme(&self, ctx: &egui::Context) {
        let ui = self.theme.ui;
        let mut visuals = if self.theme.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = color(ui.background);
        visuals.window_fill = color(ui.status_bg);
        visuals.extreme_bg_color = color(ui.prompt_bg);
        visuals.faint_bg_color = color(ui.cursor_line);
        visuals.override_text_color = Some(color(ui.foreground));
        visuals.selection.bg_fill = color(ui.selection);
        visuals.selection.stroke.color = color(ui.foreground);
        visuals.hyperlink_color = color(ui.status_accent);
        visuals.warn_fg_color = color(ui.warning);
        visuals.error_fg_color = color(ui.error);
        ctx.set_visuals(visuals);
    }

    fn palette(&self) -> Palette {
        Palette::from_theme(&self.theme)
    }

    fn buffer(&self) -> &Buffer {
        &self.buffers[self.active]
    }

    fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.active]
    }

    fn editor_id(index: usize) -> egui::Id {
        egui::Id::new(("fast-text-editor", index))
    }

    fn save_active(&mut self) {
        match self.buffer_mut().save() {
            Ok(()) => self.status = "Saved".into(),
            Err(e) => {
                if e.contains("Save As") {
                    self.ask_save_as();
                } else {
                    self.status = format!("Save failed: {e}");
                }
            }
        }
    }

    /// Selects the next (or previous) occurrence of `self.find`.
    fn find_step(&mut self, ctx: &egui::Context, backward: bool) {
        if self.find.is_empty() {
            self.status = "Type something to find".into();
            return;
        }
        let id = Self::editor_id(self.active);
        let current = TextCursor::from_state(ctx, id)
            .map(|c| c.index)
            .unwrap_or(0);
        let text = &self.buffer().text;

        let start_char = current.min(text.chars().count());
        let (byte_start, byte_end) = if backward {
            let end = char_to_byte(text, start_char.saturating_sub(1));
            (0, if start_char == 0 { text.len() } else { end })
        } else {
            let start = char_to_byte(text, start_char);
            (start, text.len())
        };
        let found = if backward {
            text.get(..byte_end)
                .and_then(|head| head.rfind(&self.find))
                .or_else(|| text.rfind(&self.find))
        } else {
            text.get(byte_start..)
                .and_then(|rest| rest.find(&self.find).map(|off| byte_start + off))
                .or_else(|| text.find(&self.find)) // wrap
        };

        match found {
            Some(byte_pos) => {
                let char_pos = text[..byte_pos].chars().count();
                let char_end = char_pos + self.find.chars().count();
                set_selection(ctx, id, char_pos, char_end);
                self.status = format!("Found \"{}\"", self.find);
            }
            None => self.status = format!("No match for \"{}\"", self.find),
        }
    }

    /// Opens the fuzzy file finder rooted at the current directory.
    fn open_finder(&mut self) {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        match &mut self.finder {
            Some(finder) if finder.root() == root => finder.refresh(),
            _ => self.finder = Some(FileFinder::new(root)),
        }
        self.finder_query.clear();
        self.finder_selected = 0;
        self.show_finder = true;
        self.show_palette = false;
    }

    /// Opens (or focuses) the file a hit points at.
    fn open_hit(&mut self, hit: &FileHit) {
        if let Some(index) = self
            .buffers
            .iter()
            .position(|b| b.path.as_deref() == Some(hit.path.as_path()))
        {
            self.active = index;
        } else {
            match Buffer::from_path(hit.path.clone()) {
                Ok(buffer) => {
                    self.buffers.push(buffer);
                    self.active = self.buffers.len() - 1;
                }
                Err(e) => {
                    self.status = format!("Open failed: {e}");
                    return;
                }
            }
        }
        self.status = format!("Opened {}", hit.relative);
        self.show_finder = false;
    }

    /// The commands this frontend can actually run, in palette order.
    fn palette_entries(&self) -> Vec<(EditorCommand, &'static str)> {
        use EditorCommand::*;
        [
            Save,
            SaveAs,
            Open,
            New,
            Close,
            Quit,
            Undo,
            Redo,
            SelectAll,
            Find,
            FindNext,
            FindPrevious,
            FileFinder,
            ToggleTheme,
            NextBuffer,
            PrevBuffer,
        ]
        .into_iter()
        .map(|command| (command, command_title(command)))
        .collect()
    }

    fn open_command_palette(&mut self) {
        self.palette_query.clear();
        self.palette_selected = 0;
        self.show_palette = true;
        self.show_finder = false;
    }

    /// Runs a command from a shared keymap binding or the palette.
    fn run_command(&mut self, ctx: &egui::Context, command: EditorCommand) {
        use EditorCommand::*;
        match command {
            Save => self.save_active(),
            SaveAs => self.ask_save_as(),
            Open => self.dialog = Dialog::Open(String::new()),
            New => {
                self.buffers.push(Buffer::untitled());
                self.active = self.buffers.len() - 1;
                self.status = "New buffer".into();
            }
            Close => self.close_active(),
            Quit => {
                self.persist_all();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Undo => {
                self.status = if self.buffer_mut().undo() {
                    "Undo".into()
                } else {
                    "Nothing to undo".into()
                };
            }
            Redo => {
                self.status = if self.buffer_mut().redo() {
                    "Redo".into()
                } else {
                    "Nothing to redo".into()
                };
            }
            SelectAll => {
                let end = self.buffer().text.chars().count();
                set_selection(ctx, Self::editor_id(self.active), 0, end);
            }
            Find => {
                self.status = "Type in the Find box".into();
                ctx.memory_mut(|m| m.request_focus(find_input()));
            }
            FindNext => self.find_step(ctx, false),
            FindPrevious => self.find_step(ctx, true),
            FileFinder => self.open_finder(),
            CommandPalette => self.open_command_palette(),
            ToggleTheme => self.cycle_theme(ctx),
            NextBuffer => self.cycle_buffer(1),
            PrevBuffer => self.cycle_buffer(-1),
            other => {
                self.status = format!("{} is not available in this frontend", command_title(other));
            }
        }
    }

    fn ask_save_as(&mut self) {
        let path = self
            .buffer()
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.dialog = Dialog::SaveAs(path);
    }

    fn close_active(&mut self) {
        let removed = if self.buffers.len() == 1 {
            let mut buffer = Buffer::untitled();
            std::mem::swap(&mut buffer, self.buffer_mut());
            Some(buffer)
        } else {
            let removed = self.buffers.remove(self.active);
            if self.active >= self.buffers.len() {
                self.active = self.buffers.len().saturating_sub(1);
            }
            Some(removed)
        };
        if let Some(buffer) = removed {
            if buffer.dirty {
                buffer.persist_undo();
                self.status = format!("Closed {} (unsaved changes discarded)", buffer.title());
            } else {
                if let Some(path) = &buffer.path {
                    let _ = fast_text_core::undo::clear(path);
                }
                self.status = format!("Closed {}", buffer.title());
            }
        }
    }

    fn cycle_buffer(&mut self, delta: isize) {
        if self.buffers.len() < 2 {
            return;
        }
        let len = self.buffers.len() as isize;
        self.active = (self.active as isize + delta).rem_euclid(len) as usize;
        self.status = self.buffer().title();
    }

    /// Persists the undo history of every modification.
    fn persist_all(&mut self) {
        for buffer in &self.buffers {
            buffer.persist_undo();
        }
        self.last_persist = Instant::now();
    }

    fn autosave_undo(&mut self) {
        if self.last_persist.elapsed() >= Duration::from_secs(5)
            && self.buffers.iter().any(|b| b.dirty)
        {
            self.persist_all();
        }
    }

    /// Resolves this frame's key presses through the shared keymap.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut hits: Vec<(egui::Key, EditorCommand)> = Vec::new();
        ctx.input(|i| {
            for event in &i.events {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                else {
                    continue;
                };
                let Some(name) = key_name(*key) else {
                    continue;
                };
                // `Cmd` on macOS is the conventional shortcut modifier, so it
                // maps to Ctrl exactly like the desktop decision document says.
                let ctrl = modifiers.ctrl || modifiers.command;
                let spec = KeySpec::new(name, ctrl, modifiers.alt, modifiers.shift);
                if let Some(command) = command_for(spec) {
                    if is_supported(command) {
                        hits.push((*key, command));
                    }
                }
            }
        });
        if let Some((key, command)) = hits.into_iter().next() {
            consume_key(ctx, key);
            self.run_command(ctx, command);
        }
    }

    /// The fuzzy finder overlay (`Ctrl+P`).
    fn finder_window(&mut self, ctx: &egui::Context) {
        if !self.show_finder {
            return;
        }
        let mut query = std::mem::take(&mut self.finder_query);
        let mut selected = self.finder_selected;
        let mut open: Option<usize> = None;
        let mut close = false;
        let finder = self.finder.as_ref();

        egui::Window::new("Go to file")
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("fuzzy file name")
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();

                let hits = finder
                    .map(|finder| finder.search(&query, 10))
                    .unwrap_or_default();
                if hits.is_empty() {
                    ui.weak("No matching files");
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    selected = (selected + 1).min(hits.len().saturating_sub(1));
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    selected = selected.saturating_sub(1);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    open = Some(selected);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
                selected = selected.min(hits.len().saturating_sub(1));
                ui.separator();
                for (index, hit) in hits.iter().enumerate() {
                    if ui
                        .selectable_label(index == selected, &hit.relative)
                        .clicked()
                    {
                        selected = index;
                        open = Some(index);
                    }
                }
            });

        self.finder_query = query;
        self.finder_selected = selected;
        if close {
            self.show_finder = false;
        }
        if let Some(index) = open {
            if let Some(hit) = self
                .finder
                .as_ref()
                .map(|finder| finder.search(&self.finder_query, 10))
                .and_then(|hits| hits.get(index).cloned())
            {
                self.open_hit(&hit);
            }
        }
    }

    /// The command palette overlay (`Ctrl+Shift+C`).
    fn command_palette_window(&mut self, ctx: &egui::Context) {
        if !self.show_palette {
            return;
        }
        let mut query = std::mem::take(&mut self.palette_query);
        let mut selected = self.palette_selected;
        let mut run: Option<EditorCommand> = None;
        let mut close = false;
        let entries = self.palette_entries();

        egui::Window::new("Command palette")
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("command name")
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();
                let matches = filter_palette(&entries, &query);
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    selected = (selected + 1).min(matches.len().saturating_sub(1));
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    selected = selected.saturating_sub(1);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    run = matches.get(selected).map(|(command, _)| *command);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
                selected = selected.min(matches.len().saturating_sub(1));
                ui.separator();
                for (index, (command, title)) in matches.iter().enumerate() {
                    if ui.selectable_label(index == selected, *title).clicked() {
                        run = Some(*command);
                    }
                }
            });

        self.palette_query = query;
        self.palette_selected = selected;
        if close {
            self.show_palette = false;
        }
        if let Some(command) = run {
            self.show_palette = false;
            self.run_command(ctx, command);
        }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.buffers.push(Buffer::untitled());
                        self.active = self.buffers.len() - 1;
                        ui.close_menu();
                    }
                    if ui.button("Open…").clicked() {
                        self.dialog = Dialog::Open(String::new());
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        self.save_active();
                        ui.close_menu();
                    }
                    if ui.button("Save As…").clicked() {
                        let path = self
                            .buffer()
                            .path
                            .as_ref()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.dialog = Dialog::SaveAs(path);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("View", |ui| {
                    let mut chosen = None;
                    ui.menu_button("Theme", |ui| {
                        for (name, _) in self.themes() {
                            let selected = name == self.theme_name;
                            if ui.radio(selected, &name).clicked() {
                                chosen = Some(name);
                                ui.close_menu();
                            }
                        }
                    });
                    if let Some(name) = chosen {
                        self.set_theme(ctx, &name);
                        ui.close_menu();
                    }
                    if ui.button("Next theme (Alt+T)").clicked() {
                        self.cycle_theme(ctx);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Go to File… (Ctrl+P)").clicked() {
                        self.open_finder();
                        ui.close_menu();
                    }
                    if ui.button("Command Palette… (Ctrl+Shift+C)").clicked() {
                        self.open_command_palette();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Check for updates").clicked() {
                        self.begin_update_check(true);
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(format!("Fast Text v{VERSION}"));
                    ui.label("Ctrl+P  go to file");
                    ui.label("Ctrl+Shift+C  command palette");
                    ui.label("Alt+T  next theme");
                });

                ui.separator();
                ui.label("Find:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.find)
                        .id(find_input())
                        .desired_width(150.0)
                        .hint_text("search"),
                );
                if ui.button("Next").clicked() {
                    self.find_step(ctx, false);
                }
                if ui.button("Prev").clicked() {
                    self.find_step(ctx, true);
                }

                // Buffer tabs on the right.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for i in (0..self.buffers.len()).rev() {
                        let label = format!(
                            "{}{}",
                            self.buffers[i].title(),
                            if self.buffers[i].dirty { " •" } else { "" }
                        );
                        if ui.selectable_label(i == self.active, label).clicked() {
                            self.active = i;
                        }
                    }
                });
            });
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let id = Self::editor_id(self.active);
                let (line, col) = match TextCursor::from_state(ctx, id) {
                    Some(c) => line_col(&self.buffer().text, c.index),
                    None => (1, 1),
                };
                let buf = self.buffer();
                ui.label(buf.title());
                if buf.dirty {
                    ui.colored_label(egui::Color32::from_rgb(0xE5, 0xC0, 0x7B), "•");
                }
                ui.separator();
                ui.label(buf.language.name());
                ui.separator();
                ui.label(format!("Ln {line}, Col {col}"));
                ui.separator();
                ui.label(format!("{} chars", buf.text.chars().count()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.status.is_empty() {
                        ui.label(self.status.clone());
                    }
                    ui.label(format!("v{VERSION}"));
                });
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let mut close = false;
        match &mut self.dialog {
            Dialog::None => {}
            Dialog::Open(path) => {
                egui::Window::new("Open file")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label("Path:");
                        let resp = ui.text_edit_singleline(path);
                        resp.request_focus();
                        ui.horizontal(|ui| {
                            if ui.button("Open").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                if close {
                    let p = path.clone();
                    self.dialog = Dialog::None;
                    if !p.trim().is_empty() {
                        match Buffer::from_path(PathBuf::from(p.trim())) {
                            Ok(b) => {
                                self.buffers.push(b);
                                self.active = self.buffers.len() - 1;
                                self.status = "Opened".into();
                            }
                            Err(e) => self.status = format!("Open failed: {e}"),
                        }
                    }
                }
            }
            Dialog::SaveAs(path) => {
                egui::Window::new("Save as")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label("Path:");
                        let resp = ui.text_edit_singleline(path);
                        resp.request_focus();
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                if close {
                    let p = path.clone();
                    self.dialog = Dialog::None;
                    if !p.trim().is_empty() {
                        match self.buffer_mut().save_as(PathBuf::from(p.trim())) {
                            Ok(()) => self.status = "Saved".into(),
                            Err(e) => self.status = format!("Save failed: {e}"),
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_updates();
        self.autosave_undo();
        // Shortcuts run before the text widget so Ctrl+Z is handled by the
        // shared document history instead of egui's private undo stack.
        self.handle_shortcuts(ctx);
        self.menu_bar(ctx);
        self.status_bar(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            let index = self.active;
            let language = self.buffer().language;
            let palette = self.palette();
            let id = Self::editor_id(index);

            let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
                let font = egui::FontId::monospace(14.0);
                let mut job = highlight_job(string, language, font, palette);
                job.wrap.max_width = wrap_width;
                ui.fonts(|f| f.layout_job(job))
            };

            let buffer = &mut self.buffers[index];
            let before = buffer.text.clone();
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut buffer.text)
                            .id(id)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(30)
                            .lock_focus(true)
                            .code_editor()
                            .layouter(&mut layouter),
                    );
                });
            let changed = buffer.text != before;
            if changed {
                buffer.dirty = true;
            }
            // Mirror the edit into the rope (one undo transaction) every frame.
            buffer.sync();
        });

        self.finder_window(ctx);
        self.command_palette_window(ctx);
        self.dialogs(ctx);
        self.update_window(ctx);
    }

    /// Persists every buffer's undo history when the window closes.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_all();
    }
}

#[cfg(test)]
impl App {
    /// A headless app for unit tests (no window, no worker channels).
    fn new_for_tests() -> Self {
        App {
            buffers: vec![Buffer::untitled()],
            active: 0,
            theme: Theme::dark(),
            theme_name: "dark".into(),
            user_themes: Vec::new(),
            dialog: Dialog::None,
            find: String::new(),
            status: String::new(),
            finder: None,
            finder_query: String::new(),
            finder_selected: 0,
            show_finder: false,
            palette_query: String::new(),
            palette_selected: 0,
            show_palette: false,
            last_persist: Instant::now(),
            update: UpdateState::Idle,
            update_rx: None,
            update_tx: None,
        }
    }
}

/// Filters palette entries with the shared fuzzy scorer.
fn filter_palette(
    entries: &[(EditorCommand, &'static str)],
    query: &str,
) -> Vec<(EditorCommand, &'static str)> {
    let query = query.trim();
    if query.is_empty() {
        return entries.to_vec();
    }
    let mut scored: Vec<(i64, EditorCommand, &'static str)> = entries
        .iter()
        .filter_map(|(command, title)| {
            fuzzy_match(query, title).map(|score| (score, *command, *title))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(b.2)));
    scored
        .into_iter()
        .map(|(_, command, title)| (command, title))
        .collect()
}

/// True when the desktop frontend implements a command.
fn is_supported(command: EditorCommand) -> bool {
    use EditorCommand::*;
    matches!(
        command,
        Save | SaveAs
            | Open
            | New
            | Close
            | Quit
            | Undo
            | Redo
            | SelectAll
            | Find
            | FindNext
            | FindPrevious
            | FileFinder
            | CommandPalette
            | ToggleTheme
            | NextBuffer
            | PrevBuffer
    )
}

/// Maps an egui key to the frontend-independent [`KeyName`].
fn key_name(key: egui::Key) -> Option<KeyName> {
    use egui::Key;
    Some(match key {
        Key::ArrowLeft => KeyName::Left,
        Key::ArrowRight => KeyName::Right,
        Key::ArrowUp => KeyName::Up,
        Key::ArrowDown => KeyName::Down,
        Key::Home => KeyName::Home,
        Key::End => KeyName::End,
        Key::PageUp => KeyName::PageUp,
        Key::PageDown => KeyName::PageDown,
        Key::Enter => KeyName::Enter,
        Key::Escape => KeyName::Esc,
        Key::Tab => KeyName::Tab,
        Key::Space => KeyName::Space,
        Key::Backspace => KeyName::Backspace,
        Key::Delete => KeyName::Delete,
        Key::Slash => KeyName::Char('/'),
        Key::Period => KeyName::Char('.'),
        Key::Comma => KeyName::Char(','),
        Key::Minus => KeyName::Char('-'),
        Key::Plus | Key::Equals => KeyName::Char('+'),
        Key::Semicolon => KeyName::Char(';'),
        Key::Colon => KeyName::Char(':'),
        Key::Quote => KeyName::Char('\''),
        Key::Backslash => KeyName::Char('\\'),
        Key::OpenBracket => KeyName::Char('['),
        Key::CloseBracket => KeyName::Char(']'),
        Key::Backtick => KeyName::Char('`'),
        Key::Num0 => KeyName::Char('0'),
        Key::Num1 => KeyName::Char('1'),
        Key::Num2 => KeyName::Char('2'),
        Key::Num3 => KeyName::Char('3'),
        Key::Num4 => KeyName::Char('4'),
        Key::Num5 => KeyName::Char('5'),
        Key::Num6 => KeyName::Char('6'),
        Key::Num7 => KeyName::Char('7'),
        Key::Num8 => KeyName::Char('8'),
        Key::Num9 => KeyName::Char('9'),
        Key::F1 => KeyName::F(1),
        Key::F2 => KeyName::F(2),
        Key::F3 => KeyName::F(3),
        Key::F4 => KeyName::F(4),
        Key::F5 => KeyName::F(5),
        Key::F6 => KeyName::F(6),
        Key::F7 => KeyName::F(7),
        Key::F8 => KeyName::F(8),
        Key::F9 => KeyName::F(9),
        Key::F10 => KeyName::F(10),
        Key::F11 => KeyName::F(11),
        Key::F12 => KeyName::F(12),
        other => {
            // Letters are the only remaining variants we care about; their
            // debug spelling is exactly `A`..`Z`.
            let name = format!("{other:?}");
            let mut chars = name.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            if !c.is_ascii_alphabetic() {
                return None;
            }
            KeyName::Char(c.to_ascii_lowercase())
        }
    })
}

/// Removes a key press from this frame's events so the text widget cannot also
/// act on it (which would, for example, undo twice).
fn consume_key(ctx: &egui::Context, key: egui::Key) {
    ctx.input_mut(|i| {
        i.events.retain(
            |event| !matches!(event, egui::Event::Key { key: pressed, .. } if *pressed == key),
        );
    });
}

fn color(c: Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c.r, c.g, c.b)
}

/// A resolved cursor position within a `TextEdit` (character index).
struct TextCursor {
    index: usize,
}

impl TextCursor {
    fn from_state(ctx: &egui::Context, id: egui::Id) -> Option<TextCursor> {
        let state = egui::text_edit::TextEditState::load(ctx, id)?;
        let range = state.cursor.char_range()?;
        Some(TextCursor {
            index: range.primary.index,
        })
    }
}

/// Sets the selection in a `TextEdit` by character range.
fn set_selection(ctx: &egui::Context, id: egui::Id, start: usize, end: usize) {
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, id) {
        let a = egui::text::CCursor::new(start);
        let b = egui::text::CCursor::new(end);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(a, b)));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
    }
}

/// Converts a character index into a byte index.
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

/// One-based line/column for a character index.
fn line_col(text: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in text.chars().enumerate() {
        if i >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_counts_from_one() {
        let text = "ab\ncde\nf";
        assert_eq!(line_col(text, 0), (1, 1));
        assert_eq!(line_col(text, 2), (1, 3));
        assert_eq!(line_col(text, 3), (2, 1));
        assert_eq!(line_col(text, 6), (2, 4));
        assert_eq!(line_col(text, 7), (3, 1));
    }

    #[test]
    fn char_to_byte_handles_unicode() {
        let text = "aé😀b";
        assert_eq!(char_to_byte(text, 0), 0);
        assert_eq!(char_to_byte(text, 1), 1);
        assert_eq!(char_to_byte(text, 2), 3); // é is 2 bytes
        assert_eq!(char_to_byte(text, 3), 7); // 😀 is 4 bytes
        assert_eq!(char_to_byte(text, 4), 8);
        assert_eq!(char_to_byte(text, 99), text.len());
    }

    #[test]
    fn untitled_buffer_has_language_and_title() {
        let b = Buffer::untitled();
        assert_eq!(b.title(), "untitled");
        assert!(!b.dirty);
    }

    #[test]
    fn save_as_writes_the_file() {
        let dir = std::env::temp_dir().join("fast-text-gui-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("FAST_TEXT_CACHE_DIR", dir.join("cache"));
        let path = dir.join("out.txt");
        let mut b = Buffer::untitled();
        b.text = "hello gui".to_string();
        b.save_as(path.clone()).unwrap();
        assert!(!b.dirty);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello gui");
        assert_eq!(b.title(), "out.txt");
    }

    #[test]
    fn text_edits_sync_into_a_single_undo_step() {
        let mut b = Buffer::untitled();
        let original = b.text.clone();
        b.text = "replaced".to_string();
        assert!(b.sync());
        assert_eq!(b.doc.text(), "replaced");
        assert!(b.undo());
        assert_eq!(b.text, original);
        assert!(b.redo());
        assert_eq!(b.text, "replaced");
        // Undoing past the beginning is a no-op, not a panic.
        while b.undo() {}
        assert_eq!(b.text, original);
    }

    #[test]
    fn palette_filters_use_the_shared_fuzzy_scorer() {
        let app = App::new_for_tests();
        let entries = app.palette_entries();
        let matches = filter_palette(&entries, "save");
        assert!(matches
            .iter()
            .any(|(command, _)| *command == EditorCommand::Save));
        assert!(filter_palette(&entries, "zzzzz").is_empty());
        assert_eq!(filter_palette(&entries, "").len(), entries.len());
    }

    #[test]
    fn egui_keys_map_to_shared_key_names() {
        assert_eq!(key_name(egui::Key::P), Some(KeyName::Char('p')));
        assert_eq!(key_name(egui::Key::F3), Some(KeyName::F(3)));
        assert_eq!(key_name(egui::Key::ArrowUp), Some(KeyName::Up));
        assert_eq!(key_name(egui::Key::Slash), Some(KeyName::Char('/')));
        assert_eq!(key_name(egui::Key::Num7), Some(KeyName::Char('7')));
    }
}
