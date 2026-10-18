//! Fast Text — native desktop frontend.
//!
//! Built on `eframe`/`egui`. The editing widget is a multiline `TextEdit`
//! painted by the same `fast-text-syntax` highlighter the terminal uses, and
//! files are loaded and saved through `fast-text-core`'s `Document`.

mod highlight;

use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use fast_text_core::Document;
use fast_text_syntax::LanguageId;
use fast_text_update::Release;
use highlight::{highlight_job, Palette};

/// The version this build was compiled at, shown in the title and status bars.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([480.0, 320.0])
            .with_title(format!("Fast Text v{VERSION}")),
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
struct Buffer {
    text: String,
    language: LanguageId,
    path: Option<PathBuf>,
    dirty: bool,
}

impl Buffer {
    fn untitled() -> Self {
        Buffer {
            text: "// Welcome to Fast Text\n//\n// A native egui frontend over fast-text-core.\n\nfn main() {\n    println!(\"Hello, world!\");\n}\n"
                .to_string(),
            language: LanguageId::Rust,
            path: None,
            dirty: false,
        }
    }

    fn from_path(path: PathBuf) -> std::io::Result<Self> {
        let doc = Document::from_file(&path).map_err(|e| std::io::Error::other(e.to_string()))?;
        let language = LanguageId::from_path(&path.to_string_lossy());
        Ok(Buffer {
            text: doc.text(),
            language,
            path: Some(path),
            dirty: false,
        })
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

    fn save(&mut self) -> Result<(), String> {
        let Some(path) = self.path.clone() else {
            return Err("no path (use Save As)".into());
        };
        let mut doc = Document::from_str(&self.text);
        doc.save_as(&path).map_err(|e| e.to_string())?;
        self.dirty = false;
        Ok(())
    }

    fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        let mut doc = Document::from_str(&self.text);
        doc.save_as(&path).map_err(|e| e.to_string())?;
        self.language = LanguageId::from_path(&path.to_string_lossy());
        self.path = Some(path);
        self.dirty = false;
        Ok(())
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
        progress: String,
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
    Progress(String),
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
    dark: bool,
    dialog: Dialog,
    find: String,
    status: String,
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

        let (update_tx, update_rx) = mpsc::channel();
        let mut app = App {
            buffers,
            active: 0,
            dark: true,
            dialog: Dialog::None,
            find: String::new(),
            status: String::new(),
            update: UpdateState::Idle,
            update_rx: Some(update_rx),
            update_tx: Some(update_tx),
        };
        if auto_check {
            app.begin_update_check(false);
        }
        app.apply_theme(_cc);
        app
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
            progress: "Starting…".into(),
        };
        if let Some(tx) = self.update_tx.clone() {
            let progress = tx.clone();
            std::thread::spawn(move || {
                let result = fast_text_update::download_and_install(&tag, &mut |message| {
                    let _ = progress.send(UpdateEvent::Progress(message.to_string()));
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
            UpdateState::Installing { tag, progress } => {
                egui::Window::new(format!("Installing {tag}"))
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(progress);
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

    fn apply_theme(&self, cc: &eframe::CreationContext<'_>) {
        cc.egui_ctx.set_visuals(if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
    }

    fn palette(&self) -> Palette {
        if self.dark {
            Palette::DARK
        } else {
            Palette::LIGHT
        }
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
                    let path = self
                        .buffer()
                        .path
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.dialog = Dialog::SaveAs(path);
                } else {
                    self.status = format!("Save failed: {e}");
                }
            }
        }
    }

    /// Selects the next occurrence of `self.find` at or after the cursor.
    fn find_next(&mut self, ctx: &egui::Context) {
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
        let byte_start = char_to_byte(text, start_char);
        let found = text
            .get(byte_start..)
            .and_then(|rest| rest.find(&self.find).map(|off| byte_start + off))
            .or_else(|| text.find(&self.find)); // wrap

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
                    if ui
                        .button(if self.dark {
                            "Light theme"
                        } else {
                            "Dark theme"
                        })
                        .clicked()
                    {
                        self.dark = !self.dark;
                        ctx.set_visuals(if self.dark {
                            egui::Visuals::dark()
                        } else {
                            egui::Visuals::light()
                        });
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
                });

                ui.separator();
                ui.label("Find:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.find)
                        .desired_width(150.0)
                        .hint_text("search"),
                );
                if ui.button("Next").clicked() {
                    self.find_next(ctx);
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
        });

        // Global shortcuts.
        let save = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S));
        if save {
            self.save_active();
        }
        let new = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::N));
        if new {
            self.buffers.push(Buffer::untitled());
            self.active = self.buffers.len() - 1;
        }

        self.dialogs(ctx);
        self.update_window(ctx);
    }
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
        let path = dir.join("out.txt");
        let mut b = Buffer::untitled();
        b.text = "hello gui".to_string();
        b.save_as(path.clone()).unwrap();
        assert!(!b.dirty);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello gui");
        assert_eq!(b.title(), "out.txt");
    }
}
