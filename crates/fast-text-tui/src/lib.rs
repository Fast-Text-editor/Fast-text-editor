//! Fast Text terminal user interface.
//!
//! A fast, keyboard-driven editor built on `fast-text-core` and
//! `fast-text-syntax`. The crate is exposed as a library so that both the
//! `fast-text` and the short `ft` binaries can share the same entry point.

pub mod app;
pub mod catalog;
pub mod config;
pub mod find_in_files;
pub mod lsp;
pub mod picker;
pub mod plugin_manager;
pub mod registry;
pub mod sidebar;
pub mod terminal;
pub mod theme;
pub mod ui;

use std::ffi::OsString;
use std::io::stdout;
use std::path::PathBuf;

use app::App;
use config::Config;
use fast_text_plugin::CursorStyle;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::SetCursorStyle;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;

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
    /// Open the files as split panes instead of tabs (`-O`).
    split: bool,
    /// Launch the native GUI frontend instead of the TUI.
    gui: bool,
    /// A `ft plugin …` / `ft install …` command instead of opening files.
    command: Option<registry::PluginCommand>,
}

/// The `--help` text.
const HELP: &str = "\
Fast Text — a fast, keyboard-driven text editor

USAGE:
    fast-text [OPTIONS] [FILE]...
    fast-text --gui [FILE]...
    fast-text plugin <COMMAND> [ARGS]
    fast-text install <NAME>... [--force]

OPTIONS:
    -c, --config <PATH>   Read settings from PATH instead of
                          $FAST_TEXT_CONFIG, else $XDG_CONFIG_HOME/fast-text/config.toml
    -O, --split           Open files as nested split panes instead of tabs
        --gui             Open the native GUI frontend (fast-text-gui)
    -h, --help            Print this help and exit
    -V, --version         Print the version and exit
    --                    Treat every later argument as a file name

PLUGIN COMMANDS:
    list                  Show installed plugins and the registry size
    search [QUERY...]     Search the registry by name, title, tag or description
    install <NAME>...     Install one or more registry plugins (--force overwrites)
    remove <NAME>...      Delete installed plugins
    help                  Show this list

ARGS:
    FILE...               Files to open; a missing file opens an empty buffer

KEYS:
    Alt+H                 In-editor help, listing every key
    Ctrl+P                Fuzzy file finder
    Ctrl+Shift+P          Command palette (including plugin commands)
    Alt+M                 Plugin manager
";

/// Parses the command line. `Ok(None)` means `--help`/`--version` already
/// printed and the process should exit successfully; `Err` is a usage error.
fn parse_args(raw: impl Iterator<Item = OsString>) -> Result<Option<Args>, String> {
    let mut files = Vec::new();
    let mut config = None;
    let mut split = false;
    let mut gui = false;
    let mut command = None;
    let mut literal = false;
    let mut args = raw;

    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy().to_string();
        if literal {
            files.push(PathBuf::from(arg));
            continue;
        }
        // Subcommands are only recognised before the first file, so a file
        // literally named `install` can still be opened with `-- install`.
        if files.is_empty() && command.is_none() {
            match text.as_str() {
                "install" => {
                    command = Some(parse_install(args)?);
                    break;
                }
                "plugin" => {
                    command = parse_plugin(args)?;
                    break;
                }
                _ => {}
            }
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
            "-O" | "--split" => split = true,
            "--gui" => gui = true,
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

    Ok(Some(Args {
        files,
        config,
        split,
        gui,
        command,
    }))
}

/// Parses `install NAME... [--force]` (also used by `plugin install`).
fn parse_install(
    mut args: impl Iterator<Item = OsString>,
) -> Result<registry::PluginCommand, String> {
    let mut names = Vec::new();
    let mut force = false;
    for arg in args.by_ref() {
        let text = arg.to_string_lossy().to_string();
        match text.as_str() {
            "-f" | "--force" => force = true,
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unrecognized option '{other}' for install"));
            }
            _ => names.push(text),
        }
    }
    if names.is_empty() {
        return Err("'install' needs at least one plugin name".to_string());
    }
    Ok(registry::PluginCommand::Install { names, force })
}

/// Parses the rest of a `plugin` command; `Ok(None)` means help was printed.
fn parse_plugin(
    mut args: impl Iterator<Item = OsString>,
) -> Result<Option<registry::PluginCommand>, String> {
    let Some(subcommand) = args.next() else {
        return Ok(Some(registry::PluginCommand::List));
    };
    let subcommand = subcommand.to_string_lossy().to_string();
    match subcommand.as_str() {
        "list" | "ls" | "installed" => Ok(Some(registry::PluginCommand::List)),
        "search" | "find" => {
            let query: Vec<String> = args.map(|arg| arg.to_string_lossy().into_owned()).collect();
            Ok(Some(registry::PluginCommand::Search(query.join(" "))))
        }
        "install" | "add" => parse_install(args).map(Some),
        "remove" | "rm" | "uninstall" | "delete" => {
            let names: Vec<String> = args.map(|arg| arg.to_string_lossy().into_owned()).collect();
            if names.is_empty() {
                Err("'plugin remove' needs at least one plugin name".to_string())
            } else {
                Ok(Some(registry::PluginCommand::Remove { names }))
            }
        }
        "help" | "--help" | "-h" => {
            print_plugin_help();
            Ok(None)
        }
        other => Err(format!("unknown plugin command '{other}'")),
    }
}

fn print_plugin_help() {
    println!(
        "Fast Text plugin registry\n\n\
         USAGE:\n    \
         fast-text plugin <COMMAND> [ARGS]\n    \
         fast-text install <NAME>... [--force]\n\n\
         COMMANDS:\n    \
         list                  Show installed plugins and the registry size\n    \
         search [QUERY...]     Search by name, title, tag or description\n    \
         install <NAME>...     Install registry plugins (--force overwrites)\n    \
         remove <NAME>...      Delete installed plugins\n\n\
         Plugins are written in TextWrite and live in\n    \
         ~/.config/fast-text/plugins\n\n\
         The registry itself is in crates/fast-text-tui/registry/index.json."
    );
}

/// Runs the editor, opening any files named on the command line.
pub fn run_cli() -> std::io::Result<()> {
    match parse_args(std::env::args_os().skip(1)) {
        Ok(Some(args)) => match args.command {
            Some(command) => registry::run(&command),
            None if args.gui => launch_gui(args.files),
            None => run_cli_with_layout(args.files, args.config, args.split),
        },
        Ok(None) => Ok(()), // --help / --version already printed
        Err(message) => {
            eprintln!("fast-text: {message}");
            eprintln!("Try 'fast-text --help' for usage.");
            std::process::exit(2);
        }
    }
}

/// Launches the native GUI frontend, forwarding file arguments.
fn launch_gui(files: Vec<PathBuf>) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let program = gui_program();
    let status = Command::new(&program)
        .args(&files)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "cannot launch GUI frontend '{}': {e}\nInstall fast-text-gui or run without --gui.",
                    program.display()
                ),
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// The `fast-text-gui` binary next to the current executable, or the bare name.
fn gui_program() -> PathBuf {
    let name = format!("fast-text-gui{}", std::env::consts::EXE_SUFFIX);
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(&name);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from(name)
}

/// Runs the editor with an explicit list of files to open.
pub fn run_cli_with(files: Vec<PathBuf>) -> std::io::Result<()> {
    run_cli_with_options(files, None)
}

/// Runs the editor with files and an optional config path.
pub fn run_cli_with_options(files: Vec<PathBuf>, config: Option<PathBuf>) -> std::io::Result<()> {
    run_cli_with_layout(files, config, false)
}

/// Runs the editor with files, a config path, and a request to open the files
/// as split panes instead of tabs.
pub fn run_cli_with_layout(
    files: Vec<PathBuf>,
    config: Option<PathBuf>,
    split: bool,
) -> std::io::Result<()> {
    // Set up the terminal.
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, files, config, split);

    // Restore the terminal even if the loop errored.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        SetCursorStyle::DefaultUserShape,
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    files: Vec<PathBuf>,
    config_path: Option<PathBuf>,
    split: bool,
) -> std::io::Result<()> {
    // `--config` wins; otherwise the usual XDG location.
    let config = Config::load_from(config_path.or_else(Config::path));
    let mut app = App::with_config_and_layout(files, config, split);
    if let Ok(size) = terminal.size() {
        app.set_terminal_size(size.width, size.height);
    }
    // Clear out the `.old` twins a previous self-update left behind, then ask
    // GitHub about newer releases in the background (throttled to once a day,
    // and off when `check_updates = false` / `FAST_TEXT_UPDATE_CHECK=0`).
    fast_text_update::cleanup_old_binaries();
    app.begin_update_check(false);
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
                Event::Paste(text) => {
                    app.paste_text(&text);
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                }
                Event::Resize(width, height) => app.set_terminal_size(width, height),
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
        assert!(!args.gui);
    }

    #[test]
    fn gui_flag_requests_the_desktop_frontend() {
        let args = parse(&["--gui", "a.rs", "b.rs"]).unwrap().unwrap();
        assert!(args.gui);
        assert_eq!(
            args.files,
            vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
        );
        assert!(!parse(&["a.rs"]).unwrap().unwrap().gui);
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
    fn split_flag_opens_files_as_panes() {
        let args = parse(&["-O", "a.rs", "b.rs"]).unwrap().unwrap();
        assert!(args.split);
        assert_eq!(
            args.files,
            vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
        );
        let args = parse(&["--split", "a.rs"]).unwrap().unwrap();
        assert!(args.split);
        assert!(!parse(&["a.rs"]).unwrap().unwrap().split);
        // After `--`, `-O` is a file name like any other.
        let args = parse(&["--", "-O"]).unwrap().unwrap();
        assert!(!args.split);
        assert_eq!(args.files, vec![PathBuf::from("-O")]);
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

    #[test]
    fn install_is_a_shorthand_for_plugin_install() {
        let args = parse(&["install", "boxes", "align", "--force"])
            .unwrap()
            .unwrap();
        assert!(args.files.is_empty());
        assert_eq!(
            args.command,
            Some(registry::PluginCommand::Install {
                names: vec!["boxes".into(), "align".into()],
                force: true,
            })
        );
    }

    #[test]
    fn plugin_commands_are_parsed() {
        assert_eq!(
            parse(&["plugin"]).unwrap().unwrap().command,
            Some(registry::PluginCommand::List)
        );
        assert_eq!(
            parse(&["plugin", "search", "markdown", "table"])
                .unwrap()
                .unwrap()
                .command,
            Some(registry::PluginCommand::Search("markdown table".into()))
        );
        assert_eq!(
            parse(&["plugin", "remove", "boxes"])
                .unwrap()
                .unwrap()
                .command,
            Some(registry::PluginCommand::Remove {
                names: vec!["boxes".into()]
            })
        );
    }

    #[test]
    fn plugin_commands_validate_their_arguments() {
        assert!(parse(&["install"]).is_err());
        assert!(parse(&["install", "--force"]).is_err());
        assert!(parse(&["install", "boxes", "--bogus"]).is_err());
        assert!(parse(&["plugin", "remove"]).is_err());
        assert!(parse(&["plugin", "bogus"]).is_err());
    }

    #[test]
    fn a_double_dash_still_opens_a_file_called_install() {
        let args = parse(&["--", "install"]).unwrap().unwrap();
        assert_eq!(args.files, vec![PathBuf::from("install")]);
        assert_eq!(args.command, None);
    }
}
