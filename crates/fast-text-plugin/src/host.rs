use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rhai::{Dynamic, Engine, ImmutableString, Scope, AST};

use crate::action::{CursorStyle, EditorAction};
use crate::key::KeySpec;

/// Metadata about a registered command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInfo {
    pub title: String,
    pub description: String,
    pub fn_name: String,
}

/// A snapshot of the editor, exposed to scripts as global constants.
#[derive(Debug, Clone, Default)]
pub struct EditorContext {
    pub cursor: usize,
    pub selection_start: usize,
    pub selection_end: usize,
    pub has_selection: bool,
    pub selected_text: String,
    pub line: usize,
    pub col: usize,
    pub doc_len: usize,
    pub line_count: usize,
    pub file_name: String,
    pub language: String,
    /// The name of the active color theme, e.g. `dark` or `monokai`.
    pub theme: String,
    /// The whole buffer as text (not just the selection).
    pub doc_text: String,
}

type Queue = Arc<Mutex<Vec<EditorAction>>>;
type Commands = Arc<Mutex<BTreeMap<String, CommandInfo>>>;
type Keybinds = Arc<Mutex<Vec<(KeySpec, String)>>>;

/// Loads Rhai plugins and runs their commands against the editor.
pub struct PluginHost {
    engine: Engine,
    ast: AST,
    sources: Vec<(String, String)>,
    commands: Commands,
    keybindings: Keybinds,
    queue: Queue,
    /// Errors encountered while loading or running plugins.
    pub errors: Vec<String>,
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    /// Creates a host with the editor API installed but no plugins loaded.
    pub fn new() -> Self {
        let queue: Queue = Arc::new(Mutex::new(Vec::new()));
        let commands: Commands = Arc::new(Mutex::new(BTreeMap::new()));
        let keybindings: Keybinds = Arc::new(Mutex::new(Vec::new()));
        let engine = build_engine(queue.clone(), commands.clone(), keybindings.clone());
        Self {
            engine,
            ast: AST::empty(),
            sources: Vec::new(),
            commands,
            keybindings,
            queue,
            errors: Vec::new(),
        }
    }

    /// Adds (or replaces) a named plugin from source and rebuilds.
    pub fn add_plugin(&mut self, name: &str, source: &str) {
        self.sources.retain(|(n, _)| n != name);
        self.sources.push((name.to_string(), source.to_string()));
        self.rebuild();
    }

    /// Loads every `.rhai` file in a directory.
    pub fn load_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "rhai"))
            .collect();
        paths.sort();
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(source) => {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    self.add_plugin(&name, &source);
                }
                Err(e) => self.errors.push(format!("{}: {e}", path.display())),
            }
        }
    }

    fn rebuild(&mut self) {
        self.errors.clear();
        self.commands.lock().unwrap().clear();
        self.keybindings.lock().unwrap().clear();

        let combined = self
            .sources
            .iter()
            .map(|(_, src)| src.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        match self.engine.compile(&combined) {
            Ok(ast) => {
                // Execute top-level statements to collect command/binding registrations.
                let mut scope = Scope::new();
                if let Err(e) = self.engine.run_ast_with_scope(&mut scope, &ast) {
                    self.errors.push(format!("error loading plugins: {e}"));
                }
                self.ast = ast;
            }
            Err(e) => self.errors.push(format!("compile error: {e}")),
        }
    }

    /// All registered commands, sorted by title.
    pub fn commands(&self) -> Vec<CommandInfo> {
        self.commands.lock().unwrap().values().cloned().collect()
    }

    pub fn command_titles(&self) -> Vec<String> {
        self.commands.lock().unwrap().keys().cloned().collect()
    }

    /// All registered keybindings.
    pub fn keybindings(&self) -> Vec<(KeySpec, String)> {
        self.keybindings.lock().unwrap().clone()
    }

    /// Finds the command title bound to a key, if any.
    pub fn lookup_key(&self, key: &KeySpec) -> Option<String> {
        self.keybindings
            .lock()
            .unwrap()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, title)| title.clone())
    }

    /// Runs a command by title, returning the actions it requested.
    pub fn run_command(&mut self, title: &str, ctx: &EditorContext) -> Vec<EditorAction> {
        self.queue.lock().unwrap().clear();

        let fn_name = {
            let commands = self.commands.lock().unwrap();
            match commands.get(title) {
                Some(c) => c.fn_name.clone(),
                None => {
                    return vec![EditorAction::Message(format!("No such command: {title}"))];
                }
            }
        };

        let mut scope = Scope::new();
        push_context(&mut scope, ctx);

        if let Err(e) = self
            .engine
            .call_fn::<Dynamic>(&mut scope, &self.ast, &fn_name, ())
        {
            return vec![EditorAction::Message(format!(
                "Plugin error in '{title}': {e}"
            ))];
        }

        std::mem::take(&mut *self.queue.lock().unwrap())
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

fn push_context(scope: &mut Scope<'static>, ctx: &EditorContext) {
    scope.push_constant("cursor", ctx.cursor as i64);
    scope.push_constant("selection_start", ctx.selection_start as i64);
    scope.push_constant("selection_end", ctx.selection_end as i64);
    scope.push_constant("has_selection", ctx.has_selection);
    scope.push_constant("selected_text", ctx.selected_text.clone());
    scope.push_constant("line", ctx.line as i64);
    scope.push_constant("col", ctx.col as i64);
    scope.push_constant("doc_len", ctx.doc_len as i64);
    scope.push_constant("line_count", ctx.line_count as i64);
    scope.push_constant("file_name", ctx.file_name.clone());
    scope.push_constant("language", ctx.language.clone());
    scope.push_constant("theme", ctx.theme.clone());
    scope.push_constant("document_text", ctx.doc_text.clone());
}

fn build_engine(queue: Queue, commands: Commands, keybindings: Keybinds) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(2_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(64, 32);

    // --- Editing actions ---------------------------------------------------
    {
        let q = queue.clone();
        engine.register_fn("insert", move |text: ImmutableString| {
            q.lock()
                .unwrap()
                .push(EditorAction::Insert(text.to_string()));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("insert_at", move |at: i64, text: ImmutableString| {
            q.lock().unwrap().push(EditorAction::InsertAt {
                at: at.max(0) as usize,
                text: text.to_string(),
            });
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("delete", move |start: i64, end: i64| {
            q.lock().unwrap().push(EditorAction::DeleteRange {
                start: start.max(0) as usize,
                end: end.max(0) as usize,
            });
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("delete_selection", move || {
            q.lock().unwrap().push(EditorAction::DeleteSelection);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("backspace", move || {
            q.lock().unwrap().push(EditorAction::Backspace);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("delete_forward", move || {
            q.lock().unwrap().push(EditorAction::DeleteForward);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("newline", move || {
            q.lock().unwrap().push(EditorAction::InsertNewline);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("set_cursor", move |at: i64| {
            q.lock()
                .unwrap()
                .push(EditorAction::SetCursor(at.max(0) as usize));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("select", move |anchor: i64, head: i64| {
            q.lock().unwrap().push(EditorAction::Select {
                anchor: anchor.max(0) as usize,
                head: head.max(0) as usize,
            });
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("select_all", move || {
            q.lock().unwrap().push(EditorAction::SelectAll);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("move_caret", move |dir: ImmutableString, extend: bool| {
            let mut q = q.lock().unwrap();
            match EditorAction::movement(dir.as_str(), extend) {
                Some(a) => q.push(a),
                None => q.push(EditorAction::Message(format!("unknown move: {dir}"))),
            }
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("undo", move || {
            q.lock().unwrap().push(EditorAction::Undo);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("redo", move || {
            q.lock().unwrap().push(EditorAction::Redo);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("save", move || {
            q.lock().unwrap().push(EditorAction::Save);
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("set_language", move |name: ImmutableString| {
            q.lock()
                .unwrap()
                .push(EditorAction::SetLanguage(name.to_string()));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("set_cursor_style", move |name: ImmutableString| {
            let mut q = q.lock().unwrap();
            match CursorStyle::parse(&name) {
                Some(style) => q.push(EditorAction::SetCursorStyle(style)),
                None => q.push(EditorAction::Message(format!(
                    "unknown cursor style: {name}"
                ))),
            }
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("set_theme", move |name: ImmutableString| {
            q.lock()
                .unwrap()
                .push(EditorAction::SetTheme(name.to_string()));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("message", move |text: ImmutableString| {
            q.lock()
                .unwrap()
                .push(EditorAction::Message(text.to_string()));
        });
    }

    // --- Registration API --------------------------------------------------
    {
        let cmds = commands.clone();
        engine.register_fn(
            "command",
            move |title: ImmutableString, fn_name: ImmutableString| {
                cmds.lock().unwrap().insert(
                    title.to_string(),
                    CommandInfo {
                        title: title.to_string(),
                        description: String::new(),
                        fn_name: fn_name.to_string(),
                    },
                );
            },
        );
    }
    {
        let cmds = commands.clone();
        engine.register_fn(
            "command_with_desc",
            move |title: ImmutableString,
                  description: ImmutableString,
                  fn_name: ImmutableString| {
                cmds.lock().unwrap().insert(
                    title.to_string(),
                    CommandInfo {
                        title: title.to_string(),
                        description: description.to_string(),
                        fn_name: fn_name.to_string(),
                    },
                );
            },
        );
    }
    {
        let binds = keybindings.clone();
        engine.register_fn(
            "bind",
            move |key: ImmutableString, title: ImmutableString| {
                if let Some(spec) = KeySpec::parse(&key) {
                    binds.lock().unwrap().push((spec, title.to_string()));
                }
            },
        );
    }

    engine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_a_simple_command() {
        let mut host = PluginHost::new();
        host.add_plugin(
            "test",
            r#"
                fn shout() {
                    insert("HELLO");
                }
                command("Shout", "shout");
            "#,
        );
        assert!(host.errors.is_empty(), "{:?}", host.errors);

        let ctx = EditorContext::default();
        let actions = host.run_command("Shout", &ctx);
        assert_eq!(actions, vec![EditorAction::Insert("HELLO".to_string())]);
    }

    #[test]
    fn sets_cursor_style() {
        let mut host = PluginHost::new();
        host.add_plugin(
            "test",
            r#"
                fn bar() { set_cursor_style("bar"); }
                command("Bar", "bar");
            "#,
        );
        let actions = host.run_command("Bar", &EditorContext::default());
        assert_eq!(
            actions,
            vec![EditorAction::SetCursorStyle(crate::CursorStyle::Bar)]
        );
    }

    #[test]
    fn sets_the_theme() {
        let mut host = PluginHost::new();
        host.add_plugin(
            "test",
            r#"
                fn mono() { set_theme("monokai"); }
                command("Monokai", "mono");
            "#,
        );
        let actions = host.run_command("Monokai", &EditorContext::default());
        assert_eq!(actions, vec![EditorAction::SetTheme("monokai".to_string())]);
    }

    #[test]
    fn exposes_editor_context_constants() {
        let mut host = PluginHost::new();
        host.add_plugin(
            "test",
            r#"
                fn wrap() {
                    insert("<" + selected_text + ">");
                }
                command("Wrap", "wrap");
            "#,
        );
        let ctx = EditorContext {
            selected_text: "hi".to_string(),
            has_selection: true,
            ..Default::default()
        };
        let actions = host.run_command("Wrap", &ctx);
        assert_eq!(actions, vec![EditorAction::Insert("<hi>".to_string())]);
    }

    #[test]
    fn registers_keybindings() {
        let mut host = PluginHost::new();
        host.add_plugin(
            "test",
            r#"
                fn upper() { insert("x"); }
                command("Upper", "upper");
                bind("ctrl+shift+u", "Upper");
            "#,
        );
        let key = KeySpec::parse("ctrl+shift+u").unwrap();
        assert_eq!(host.lookup_key(&key).as_deref(), Some("Upper"));
        let titles = host.command_titles();
        assert!(titles.contains(&"Upper".to_string()));
    }

    #[test]
    fn movement_and_multi_actions() {
        let mut host = PluginHost::new();
        host.add_plugin(
            "test",
            r#"
                fn jump() {
                    set_cursor(0);
                    move_caret("word_right", false);
                }
                command("Jump", "jump");
            "#,
        );
        let actions = host.run_command("Jump", &EditorContext::default());
        assert_eq!(
            actions,
            vec![
                EditorAction::SetCursor(0),
                EditorAction::MoveWordForward { extend: false },
            ]
        );
    }

    #[test]
    fn compile_errors_are_reported() {
        let mut host = PluginHost::new();
        host.add_plugin("bad", "fn broken( {");
        assert!(!host.errors.is_empty());
    }
}
