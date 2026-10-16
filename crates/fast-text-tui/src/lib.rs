//! Fast Text terminal user interface.
//!
//! A fast, keyboard-driven editor built on `fast-text-core` and
//! `fast-text-syntax`. The crate is exposed as a library so that both the
//! `fast-text` and the short `ft` binaries can share the same entry point.

pub mod app;
pub mod lsp;
pub mod plugin_manager;
pub mod sidebar;
pub mod theme;
pub mod ui;

use std::io::stdout;
use std::path::PathBuf;

use fast_text_plugin::CursorStyle;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::SetCursorStyle;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;

use app::App;

/// Maps an editor cursor style to the terminal escape command for it.
fn terminal_cursor(style: CursorStyle) -> SetCursorStyle {
    match style {
        CursorStyle::Block => SetCursorStyle::SteadyBlock,
        CursorStyle::Bar => SetCursorStyle::SteadyBar,
        CursorStyle::Underline => SetCursorStyle::SteadyUnderScore,
    }
}

/// Runs the editor, opening any files named on the command line.
pub fn run_cli() -> std::io::Result<()> {
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    run_cli_with(files)
}

/// Runs the editor with an explicit list of files to open.
pub fn run_cli_with(files: Vec<PathBuf>) -> std::io::Result<()> {
    // Set up the terminal.
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, files);

    // Restore the terminal even if the loop errored.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        SetCursorStyle::DefaultUserShape,
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    files: Vec<PathBuf>,
) -> std::io::Result<()> {
    let mut app = App::new(files);
    let mut applied_cursor: Option<CursorStyle> = None;

    loop {
        app.tick();
        terminal.draw(|frame| ui::draw(&mut app, frame))?;
        if applied_cursor != Some(app.cursor_style) {
            execute!(terminal.backend_mut(), terminal_cursor(app.cursor_style))?;
            applied_cursor = Some(app.cursor_style);
        }

        if app.should_quit {
            break;
        }

        if event::poll(app.poll_timeout())? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                        app.handle_key(key);
                    }
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}
