//! Embeds the [TextWrite](https://example.com/fast-text) scripting language
//! into the Fast Text editor.
//!
//! TextWrite scripts run against the *same* editor API as Rhai plugins: they
//! never touch the buffer directly, they describe changes as
//! [`EditorAction`]s that the host applies in order. That keeps the scripting
//! layer decoupled from the editor internals and from the borrow checker.
//!
//! A script makes the editor globals (`selected_text`, `document_text`,
//! `has_selection`, `file_name`, `theme`, …) available and calls editor
//! functions to act:
//!
//! ```textwrite
//! fn wrap_in_quotes() {
//!     if has_selection {
//!         insert("\"" + selected_text + "\"")
//!     } else {
//!         message("Select some text first")
//!     }
//! }
//!
//! command_with_desc("Wrap in quotes", "Surround the selection with \"", "wrap_in_quotes")
//! bind("ctrl+shift+q", "Wrap in quotes")
//! ```
//!
//! Besides editing actions, a script can restyle the editor: `set_theme(name)`
//! switches to a built-in or plugin-defined theme, and `define_theme(name,
//! colors)` registers a new palette from a map of `0xRRGGBB` colors.
//!
//! [`ScriptHost`] loads scripts, remembers the commands, keybindings and themes
//! they register, and runs a named command against a snapshot of the editor
//! state.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use fast_text_plugin::{CommandInfo, CursorStyle, EditorAction, EditorContext, KeySpec};
use textwrite::host::Host;
use textwrite::Interpreter;
pub use textwrite::Value;

/// The editor automation package, importable as `"fast-text/automator"`.
const AUTOMATOR_SRC: &str = include_str!("automator.tw");

type Queue = Rc<RefCell<Vec<EditorAction>>>;
type Commands = Rc<RefCell<BTreeMap<String, CommandInfo>>>;
type Keybinds = Rc<RefCell<Vec<(KeySpec, String)>>>;
/// Themes a script defined with `define_theme`, as name → `(key, 0xRRGGBB)`.
type Themes = Rc<RefCell<BTreeMap<String, Vec<(String, u32)>>>>;

/// The native functions a TextWrite editor script may call.
static HOST_FUNCTIONS: &[&str] = &[
    "print",
    "write",
    "message",
    "insert",
    "insert_at",
    "delete",
    "delete_selection",
    "backspace",
    "delete_forward",
    "newline",
    "set_cursor",
    "select",
    "select_all",
    "move_caret",
    "undo",
    "redo",
    "save",
    "set_language",
    "set_cursor_style",
    "set_theme",
    "define_theme",
    "wait",
    "command",
    "command_with_desc",
    "bind",
];

/// The host object wired into the interpreter; it translates TextWrite calls
/// into queued [`EditorAction`]s and registration side effects.
struct EditorHost {
    queue: Queue,
    commands: Commands,
    keybindings: Keybinds,
    themes: Themes,
}

impl Host for EditorHost {
    fn functions(&self) -> &'static [&'static str] {
        HOST_FUNCTIONS
    }

    fn call(
        &mut self,
        name: &str,
        args: Vec<Value>,
        span: textwrite::Span,
    ) -> textwrite::Result<Value> {
        match name {
            "print" => {
                let text = args
                    .iter()
                    .map(Value::to_display)
                    .collect::<Vec<_>>()
                    .join(" ");
                self.push(EditorAction::Message(text));
            }
            "write" => {
                let text = args
                    .iter()
                    .map(Value::to_display)
                    .collect::<Vec<_>>()
                    .join("");
                self.push(EditorAction::Message(text));
            }
            "message" => {
                let text = args
                    .iter()
                    .map(Value::to_display)
                    .collect::<Vec<_>>()
                    .join(" ");
                self.push(EditorAction::Message(text));
            }
            "insert" => self.push(EditorAction::Insert(arg_str(&args, 0, name, span)?)),
            "insert_at" => {
                let at = arg_usize(&args, 0, name, span)?;
                let text = arg_str(&args, 1, name, span)?;
                self.push(EditorAction::InsertAt { at, text });
            }
            "delete" => {
                let start = arg_usize(&args, 0, name, span)?;
                let end = arg_usize(&args, 1, name, span)?;
                self.push(EditorAction::DeleteRange { start, end });
            }
            "delete_selection" => self.push(EditorAction::DeleteSelection),
            "backspace" => self.push(EditorAction::Backspace),
            "delete_forward" => self.push(EditorAction::DeleteForward),
            "newline" => self.push(EditorAction::InsertNewline),
            "set_cursor" => {
                let at = arg_usize(&args, 0, name, span)?;
                self.push(EditorAction::SetCursor(at));
            }
            "select" => {
                let anchor = arg_usize(&args, 0, name, span)?;
                let head = arg_usize(&args, 1, name, span)?;
                self.push(EditorAction::Select { anchor, head });
            }
            "select_all" => self.push(EditorAction::SelectAll),
            "move_caret" => {
                let dir = arg_str(&args, 0, name, span)?;
                let extend = args.get(1).map(Value::is_truthy).unwrap_or(false);
                match EditorAction::movement(&dir, extend) {
                    Some(action) => self.push(action),
                    None => self.push(EditorAction::Message(format!("unknown move: {dir}"))),
                }
            }
            "undo" => self.push(EditorAction::Undo),
            "redo" => self.push(EditorAction::Redo),
            "save" => self.push(EditorAction::Save),
            "set_language" => {
                let lang = arg_str(&args, 0, name, span)?;
                self.push(EditorAction::SetLanguage(lang));
            }
            "set_cursor_style" => {
                let style = arg_str(&args, 0, name, span)?;
                match CursorStyle::parse(&style) {
                    Some(style) => self.push(EditorAction::SetCursorStyle(style)),
                    None => self.push(EditorAction::Message(format!(
                        "unknown cursor style: {style}"
                    ))),
                }
            }
            "set_theme" => {
                let theme = arg_str(&args, 0, name, span)?;
                self.push(EditorAction::SetTheme(theme));
            }
            "define_theme" => {
                let theme = arg_str(&args, 0, name, span)?;
                let colors = match args.get(1) {
                    Some(Value::Map(map)) => {
                        let entries = map.borrow();
                        let mut colors = Vec::with_capacity(entries.len());
                        for (key, value) in entries.iter() {
                            match value.as_i64() {
                                Some(n) => colors.push((key.to_string(), (n as u32) & 0xFF_FF_FF)),
                                None => {
                                    return Err(textwrite::Error::runtime(
                                        format!(
                                            "`define_theme` colors must be ints like 0xRRGGBB; `{key}` is {}",
                                            value.type_name()
                                        ),
                                        span,
                                    ))
                                }
                            }
                        }
                        colors
                    }
                    Some(other) => {
                        return Err(textwrite::Error::runtime(
                            format!(
                                "`define_theme` expects a color map for argument 2, found {}",
                                other.type_name()
                            ),
                            span,
                        ))
                    }
                    None => {
                        return Err(textwrite::Error::runtime(
                            "`define_theme` expects 2 argument(s)".to_string(),
                            span,
                        ))
                    }
                };
                self.themes.borrow_mut().insert(theme, colors);
            }
            "wait" => {
                let ms = arg_usize(&args, 0, name, span)?;
                self.push(EditorAction::Wait(ms as u64));
            }
            "command" => {
                let title = arg_str(&args, 0, name, span)?;
                let fn_name = arg_str(&args, 1, name, span)?;
                self.register_command(title, String::new(), fn_name);
            }
            "command_with_desc" => {
                let title = arg_str(&args, 0, name, span)?;
                let description = arg_str(&args, 1, name, span)?;
                let fn_name = arg_str(&args, 2, name, span)?;
                self.register_command(title, description, fn_name);
            }
            "bind" => {
                let key = arg_str(&args, 0, name, span)?;
                let title = arg_str(&args, 1, name, span)?;
                if let Some(spec) = KeySpec::parse(&key) {
                    self.keybindings.borrow_mut().push((spec, title));
                } else {
                    self.push(EditorAction::Message(format!("unknown key: {key}")));
                }
            }
            _ => {
                return Err(textwrite::Error::runtime(
                    format!("unknown editor function `{name}`"),
                    span,
                ))
            }
        }
        Ok(Value::None)
    }
}

impl EditorHost {
    fn push(&mut self, action: EditorAction) {
        self.queue.borrow_mut().push(action);
    }

    fn register_command(&mut self, title: String, description: String, fn_name: String) {
        self.commands.borrow_mut().insert(
            title.clone(),
            CommandInfo {
                title,
                description,
                fn_name,
            },
        );
    }
}

/// Loads TextWrite editor scripts and runs their commands.
///
/// Mirrors [`fast_text_plugin::PluginHost`], but backed by the TextWrite
/// interpreter instead of Rhai.
pub struct ScriptHost {
    interp: textwrite::Interpreter,
    queue: Queue,
    commands: Commands,
    keybindings: Keybinds,
    themes: Themes,
    sources: Vec<(String, String)>,
    /// Directory that relative `import "./file.tw"` paths resolve against.
    base_dir: Option<PathBuf>,
    /// Errors encountered while loading or running scripts.
    pub errors: Vec<String>,
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptHost {
    /// Creates a host with the editor API installed but no scripts loaded.
    pub fn new() -> Self {
        let queue: Queue = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let commands: Commands =
            std::rc::Rc::new(std::cell::RefCell::new(std::collections::BTreeMap::new()));
        let keybindings: Keybinds = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let themes: Themes = std::rc::Rc::new(std::cell::RefCell::new(BTreeMap::new()));

        let interp = make_interpreter(&queue, &commands, &keybindings, &themes);
        Self {
            interp,
            queue,
            commands,
            keybindings,
            themes,
            sources: Vec::new(),
            base_dir: None,
            errors: Vec::new(),
        }
    }

    /// Sets the directory that relative imports in loaded scripts resolve
    /// against (normally the plugin directory).
    pub fn set_module_dir(&mut self, dir: impl Into<PathBuf>) {
        self.base_dir = Some(dir.into());
        if let Some(dir) = self.base_dir.clone() {
            self.interp.set_module_base(dir);
        }
    }

    /// Adds a script by name, re-running all loaded scripts.
    pub fn add_script(&mut self, name: &str, source: &str) {
        self.sources.retain(|(n, _)| n != name);
        self.sources.push((name.to_string(), source.to_string()));
        self.reload();
    }

    /// Loads every `.tw` file in a directory, sorted by name.
    pub fn load_dir(&mut self, dir: &std::path::Path) {
        self.base_dir = Some(dir.to_path_buf());
        let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("tw"))
                .collect(),
            Err(_) => return,
        };
        entries.sort();
        for path in entries {
            match std::fs::read_to_string(&path) {
                Ok(source) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    self.add_script(&name, &source);
                }
                Err(e) => self.errors.push(format!("{}: {e}", path.display())),
            }
        }
    }

    fn reload(&mut self) {
        self.errors.clear();
        self.commands.borrow_mut().clear();
        self.keybindings.borrow_mut().clear();
        self.themes.borrow_mut().clear();
        self.queue.borrow_mut().clear();
        // Start from a clean interpreter so removed scripts leave no functions
        // behind, then give top-level code a default snapshot so it can
        // reference editor globals while registering commands.
        self.interp =
            make_interpreter(&self.queue, &self.commands, &self.keybindings, &self.themes);
        if let Some(dir) = self.base_dir.clone() {
            self.interp.set_module_base(dir);
        }
        self.install_context(&EditorContext::default());

        let combined = self
            .sources
            .iter()
            .map(|(_, src)| src.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if let Err(e) = self.interp.run(&combined) {
            self.errors.push(format!("error loading scripts: {e}"));
        }
        self.queue.borrow_mut().clear();
    }

    /// Copies an editor snapshot into the interpreter's globals.
    fn install_context(&mut self, ctx: &EditorContext) {
        let set = |interp: &mut Interpreter, name: &str, v: Value| interp.set_global(name, v);
        set(&mut self.interp, "cursor", Value::Int(ctx.cursor as i64));
        set(
            &mut self.interp,
            "selection_start",
            Value::Int(ctx.selection_start as i64),
        );
        set(
            &mut self.interp,
            "selection_end",
            Value::Int(ctx.selection_end as i64),
        );
        set(
            &mut self.interp,
            "has_selection",
            Value::Bool(ctx.has_selection),
        );
        set(
            &mut self.interp,
            "selected_text",
            Value::str(ctx.selected_text.clone()),
        );
        set(&mut self.interp, "line", Value::Int(ctx.line as i64));
        set(&mut self.interp, "col", Value::Int(ctx.col as i64));
        set(&mut self.interp, "doc_len", Value::Int(ctx.doc_len as i64));
        set(
            &mut self.interp,
            "line_count",
            Value::Int(ctx.line_count as i64),
        );
        set(
            &mut self.interp,
            "file_name",
            Value::str(ctx.file_name.clone()),
        );
        set(
            &mut self.interp,
            "language",
            Value::str(ctx.language.clone()),
        );
        set(&mut self.interp, "theme", Value::str(ctx.theme.clone()));
        set(
            &mut self.interp,
            "document_text",
            Value::str(ctx.doc_text.clone()),
        );
    }

    /// Every registered command, sorted by title.
    pub fn commands(&self) -> Vec<CommandInfo> {
        self.commands.borrow().values().cloned().collect()
    }

    /// Registered command titles, sorted.
    pub fn command_titles(&self) -> Vec<String> {
        self.commands.borrow().keys().cloned().collect()
    }

    /// All registered keybindings.
    pub fn keybindings(&self) -> Vec<(KeySpec, String)> {
        self.keybindings.borrow().clone()
    }

    /// Every theme defined by a loaded script, as `(name, colors)`.
    pub fn themes(&self) -> Vec<(String, Vec<(String, u32)>)> {
        self.themes
            .borrow()
            .iter()
            .map(|(name, colors)| (name.clone(), colors.clone()))
            .collect()
    }

    /// The names of every theme defined by a loaded script.
    pub fn theme_names(&self) -> Vec<String> {
        self.themes.borrow().keys().cloned().collect()
    }

    /// Finds the command title bound to a key, if any.
    pub fn lookup_key(&self, key: &KeySpec) -> Option<String> {
        self.keybindings
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, title)| title.clone())
    }

    /// True when a command with this title is registered.
    pub fn has_command(&self, title: &str) -> bool {
        self.commands.borrow().contains_key(title)
    }

    /// Runs a command by title, returning the actions it requested.
    pub fn run_command(&mut self, title: &str, ctx: &EditorContext) -> Vec<EditorAction> {
        let Some(info) = self.commands.borrow().get(title).cloned() else {
            return Vec::new();
        };
        self.install_context(ctx);
        self.queue.borrow_mut().clear();
        if let Err(e) = self.interp.eval(&format!("{}()", info.fn_name)) {
            self.errors.push(format!("{title}: {e}"));
        }
        std::mem::take(&mut *self.queue.borrow_mut())
    }

    /// Evaluates a snippet of TextWrite against the current editor snapshot.
    ///
    /// Actions requested by the snippet are available afterwards via
    /// [`take_actions`](Self::take_actions).
    pub fn eval(&mut self, source: &str, ctx: &EditorContext) -> textwrite::Result<Value> {
        self.install_context(ctx);
        self.queue.borrow_mut().clear();
        self.interp.eval(source)
    }

    /// Takes the actions requested by the most recent [`eval`](Self::eval).
    pub fn take_actions(&self) -> Vec<EditorAction> {
        std::mem::take(&mut *self.queue.borrow_mut())
    }

    /// Compiles a `.tw` file: parses it and resolves (recursively) every module
    /// it imports, without running anything. The `.tw` extension is optional.
    pub fn compile_file(&mut self, path: &str) -> textwrite::Result<()> {
        let (source, dir) = read_script(path)?;
        self.interp.set_module_base(dir);
        self.interp.compile(&source)
    }

    /// Compiles and runs a `.tw` file against a snapshot of the editor state,
    /// returning the actions it queued. The `.tw` extension is optional.
    pub fn run_file(
        &mut self,
        path: &str,
        ctx: &EditorContext,
    ) -> textwrite::Result<Vec<EditorAction>> {
        let (source, dir) = read_script(path)?;
        self.interp.set_module_base(dir);
        self.install_context(ctx);
        self.queue.borrow_mut().clear();
        self.interp.run(&source)?;
        Ok(std::mem::take(&mut *self.queue.borrow_mut()))
    }
}

/// Reads a script from disk, defaulting the `.tw` extension and reporting the
/// directory it lives in (used as the base for its relative imports).
fn read_script(path: &str) -> textwrite::Result<(String, PathBuf)> {
    let trimmed = path.trim();
    let mut path = PathBuf::from(trimmed);
    if !path.exists() && path.extension().is_none() {
        path = path.with_extension("tw");
    }
    let source = std::fs::read_to_string(&path).map_err(|e| {
        textwrite::Error::runtime(
            format!("cannot read {}: {e}", path.display()),
            textwrite::Span::zero(),
        )
    })?;
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Ok((source, dir))
}

/// Builds a fresh interpreter wired to the shared editor host.
fn make_interpreter(
    queue: &Queue,
    commands: &Commands,
    keybindings: &Keybinds,
    themes: &Themes,
) -> Interpreter {
    let host = EditorHost {
        queue: queue.clone(),
        commands: commands.clone(),
        keybindings: keybindings.clone(),
        themes: themes.clone(),
    };
    let mut interp = Interpreter::with_output(Box::new(std::io::sink()));
    interp.set_host(Rc::new(RefCell::new(host)));
    interp.register_module("fast-text/automator", AUTOMATOR_SRC);
    interp.register_module("automator", AUTOMATOR_SRC);
    interp
}

fn arg_str(
    args: &[Value],
    i: usize,
    name: &str,
    span: textwrite::Span,
) -> textwrite::Result<String> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.to_string()),
        Some(other) => Err(textwrite::Error::runtime(
            format!(
                "`{name}` expects a string for argument {}, found {}",
                i + 1,
                other.type_name()
            ),
            span,
        )),
        None => Err(textwrite::Error::runtime(
            format!("`{name}` expects {} argument(s)", i + 1),
            span,
        )),
    }
}

fn arg_usize(
    args: &[Value],
    i: usize,
    name: &str,
    span: textwrite::Span,
) -> textwrite::Result<usize> {
    match args.get(i).and_then(Value::as_i64) {
        Some(n) if n >= 0 => Ok(n as usize),
        Some(n) => Err(textwrite::Error::runtime(
            format!("`{name}` expects a non-negative index, found {n}"),
            span,
        )),
        None => Err(textwrite::Error::runtime(
            format!("`{name}` expects {} argument(s)", i + 1),
            span,
        )),
    }
}
