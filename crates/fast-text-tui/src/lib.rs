//! Fast Text terminal user interface.
//!
//! A fast, keyboard-driven editor built on `fast-text-core` and
//! `fast-text-syntax`. The crate is exposed as a library so that both the
//! `fast-text` and the short `ft` binaries can share the same entry point.

pub mod app;
pub mod config;
pub mod lsp;
pub mod plugin_manager;
pub mod sidebar;
pub mod theme;
pub mod ui;

use std::ffi::OsString;
use std::io::stdout;
use std::path::PathBuf;

use config::Config;
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

/// What the command line asked for.
struct Args {
    files: Vec<PathBuf>,
    config: Option<PathBuf>,
}

/// The `--help` text.
const HELP: &str = "\
Fast Text — a fast, keyboard-driven text editor

USAGE:
    fast-text [OPTIONS] [FILE]...

OPTIONS:
    -c, --config <PATH>   Read settings from PATH instead of
                          $FAST_TEXT_CONFIG, else $XDG_CONFIG_HOME/fast-text/config.toml
    -h, --help            Print this help and exit
    -V, --version         Print the version and exit
    --                    Treat every later argument as a file name

ARGS:
    FILE...               Files to open; a missing file opens an empty buffer

KEYS:
    Alt+H                 In-editor help, listing every key
    Ctrl+P                Command palette (including plugin commands)
    Alt+M                 Plugin manager
";

/// Parses the command line. `Ok(None)` means `--help`/`--version` already
/// printed and the process should exit successfully; `Err` is a usage error.
fn parse_args(raw: impl Iterator<Item = OsString>) -> Result<Option<Args>, String> {
    let mut files = Vec::new();
    let mut config = None;
    let mut literal = false;
    let mut args = raw;

    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy().to_string();
        if literal {
            files.push(PathBuf::from(arg));
            continue;
        }
        match text.as_str() {
            "--" => literal = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("fast-text {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-c" | "--config" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("option '{text}' needs a path"))?;
                config = Some(PathBuf::from(value));
            }
            other if other.starts_with("--config=") => {
                config = Some(PathBuf::from(&other["--config=".len()..]));
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unrecognized option '{other}'"));
            }
            _ => files.push(PathBuf::from(arg)),
        }
    }

    Ok(Some(Args { files, config }))
}

/// Runs the editor, opening any files named on the command line.
pub fn run_cli() -> std::io::Result<()> {
    match parse_args(std::env::args_os().skip(1)) {
        Ok(Some(args)) => run_cli_with_options(args.files, args.config),
        Ok(None) => Ok(()), // --help / --version already printed
        Err(message) => {
            eprintln!("fast-text: {message}");
            eprintln!("Try 'fast-text --help' for usage.");
            std::process::exit(2);
        }
    }
}

/// Runs the editor with an explicit list of files to open.
pub fn run_cli_with(files: Vec<PathBuf>) -> std::io::Result<()> {
    run_cli_with_options(files, None)
}

/// Runs the editor with files and an optional config path.
pub fn run_cli_with_options(files: Vec<PathBuf>, config: Option<PathBuf>) -> std::io::Result<()> {
    // Set up the terminal.
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, files, config);

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
    config_path: Option<PathBuf>,
) -> std::io::Result<()> {
    // `--config` wins; otherwise the usual XDG location.
    let config = Config::load_from(config_path.or_else(Config::path));
    let mut app = App::with_config(files, config);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Args>, String> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn a_bare_argument_is_a_file() {
        let args = parse(&["notes.md"]).unwrap().unwrap();
        assert_eq!(args.files, vec![PathBuf::from("notes.md")]);
        assert_eq!(args.config, None);
    }

    #[test]
    fn files_and_the_config_flag_mix() {
        let args = parse(&["a.txt", "--config", "/tmp/x.toml", "b.txt"])
            .unwrap()
            .unwrap();
        assert_eq!(
            args.files,
            vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")]
        );
        assert_eq!(args.config, Some(PathBuf::from("/tmp/x.toml")));
    }

    #[test]
    fn config_accepts_the_equals_form() {
        let args = parse(&["--config=/tmp/y.toml"]).unwrap().unwrap();
        assert!(args.files.is_empty());
        assert_eq!(args.config, Some(PathBuf::from("/tmp/y.toml")));
    }

    #[test]
    fn a_lone_dash_is_a_file_but_unknown_flags_are_errors() {
        let args = parse(&["-"]).unwrap().unwrap();
        assert_eq!(args.files, vec![PathBuf::from("-")]);
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["-x"]).is_err());
    }

    #[test]
    fn double_dash_protects_odd_file_names() {
        let args = parse(&["--", "--config", "-x"]).unwrap().unwrap();
        assert_eq!(
            args.files,
            vec![PathBuf::from("--config"), PathBuf::from("-x")]
        );
        assert_eq!(args.config, None);
    }

    #[test]
    fn config_without_a_path_is_an_error() {
        assert!(parse(&["--config"]).is_err());
    }
}
