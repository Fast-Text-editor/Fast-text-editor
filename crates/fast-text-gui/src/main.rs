//! Fast Text — native desktop frontend.
//!
//! Built on `eframe`/`egui`. The editing widget is a multiline `TextEdit`
//! painted by the same `fast-text-syntax` highlighter the terminal uses, and
//! files are loaded and saved through `fast-text-core`'s `Document`.

mod highlight;

use std::path::PathBuf;

use eframe::egui;
use fast_text_core::Document;
use fast_text_syntax::LanguageId;
use highlight::{highlight_job, Palette};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([480.0, 320.0])
            .with_title("Fast Text"),
        ..Default::default()
    };

    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();

    eframe::run_native(
        "Fast Text",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, files)))),
    )
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

struct App {
    buffers: Vec<Buffer>,
    active: usize,
    dark: bool,
    dialog: Dialog,
    find: String,
    status: String,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>) -> Self {
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

        let app = App {
            buffers,
            active: 0,
            dark: true,
            dialog: Dialog::None,
            find: String::new(),
            status: String::new(),
        };
        app.apply_theme(_cc);
        app
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
