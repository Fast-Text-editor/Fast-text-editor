//! End-to-end tests for running TextWrite editor scripts.

use fast_text_plugin::{CursorStyle, EditorAction, EditorContext, KeySpec};
use fast_text_textwrite::ScriptHost;

fn host_with(name: &str, source: &str) -> ScriptHost {
    let mut host = ScriptHost::new();
    host.add_script(name, source);
    assert!(host.errors.is_empty(), "load errors: {:?}", host.errors);
    host
}

#[test]
fn registers_and_runs_a_command() {
    let mut host = host_with(
        "test",
        r#"
            fn shout() {
                insert("HELLO")
            }
            command("Shout", "shout")
        "#,
    );
    assert!(host.has_command("Shout"));
    assert_eq!(host.command_titles(), vec!["Shout".to_string()]);

    let actions = host.run_command("Shout", &EditorContext::default());
    assert_eq!(actions, vec![EditorAction::Insert("HELLO".to_string())]);
}

#[test]
fn exposes_editor_context_globals() {
    let mut host = host_with(
        "test",
        r#"
            fn wrap() {
                insert("<" + selected_text + ">")
            }
            command("Wrap", "wrap")
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
fn commands_can_branch_on_editor_state() {
    let mut host = host_with(
        "test",
        r#"
            fn delete_or_note() {
                if has_selection {
                    delete_selection()
                } else {
                    message("Select something first")
                }
            }
            command("Delete or note", "delete_or_note")
        "#,
    );
    let with_sel = host.run_command(
        "Delete or note",
        &EditorContext {
            has_selection: true,
            ..Default::default()
        },
    );
    assert_eq!(with_sel, vec![EditorAction::DeleteSelection]);

    let without = host.run_command("Delete or note", &EditorContext::default());
    assert_eq!(
        without,
        vec![EditorAction::Message("Select something first".to_string())]
    );
}

#[test]
fn move_caret_maps_directions() {
    let mut host = host_with(
        "test",
        r#"
            fn jump() {
                move_caret("word_right", true)
                move_caret("bad_direction", false)
            }
            command("Jump", "jump")
        "#,
    );
    let actions = host.run_command("Jump", &EditorContext::default());
    assert_eq!(
        actions,
        vec![
            EditorAction::MoveWordForward { extend: true },
            EditorAction::Message("unknown move: bad_direction".to_string()),
        ]
    );
}

#[test]
fn changes_the_cursor_style() {
    let mut host = host_with(
        "test",
        r#"
            fn bar() { set_cursor_style("bar") }
            fn underline() { set_cursor_style("underline") }
            fn bogus() { set_cursor_style("sideways") }
            command("Bar", "bar")
            command("Underline", "underline")
            command("Bogus", "bogus")
        "#,
    );
    let ctx = EditorContext::default();
    assert_eq!(
        host.run_command("Bar", &ctx),
        vec![EditorAction::SetCursorStyle(CursorStyle::Bar)]
    );
    assert_eq!(
        host.run_command("Underline", &ctx),
        vec![EditorAction::SetCursorStyle(CursorStyle::Underline)]
    );
    assert_eq!(
        host.run_command("Bogus", &ctx),
        vec![EditorAction::Message(
            "unknown cursor style: sideways".to_string()
        )]
    );
}

#[test]
fn keybindings_are_registered() {
    let host = host_with(
        "test",
        r#"
            fn noop() { message("hi") }
            command("Noop", "noop")
            bind("ctrl+shift+t", "Noop")
        "#,
    );
    let spec = KeySpec::parse("ctrl+shift+t").expect("valid key spec");
    assert_eq!(host.lookup_key(&spec), Some("Noop".to_string()));
    assert_eq!(host.keybindings().len(), 1);
}

#[test]
fn closures_and_collections_work_in_scripts() {
    let mut host = host_with(
        "test",
        r#"
            fn doubled(nums) {
                return map(nums, fn(n) { return n * 2 })
            }
            fn report() {
                let nums = [1, 2, 3]
                message(join(doubled(nums), ","))
            }
            command("Report", "report")
        "#,
    );
    let actions = host.run_command("Report", &EditorContext::default());
    assert_eq!(actions, vec![EditorAction::Message("2,4,6".to_string())]);
}

#[test]
fn eval_runs_snippets_and_collects_actions() {
    let mut host = ScriptHost::new();
    let ctx = EditorContext::default();
    let value = host.eval("2 + 3 * 4", &ctx).expect("eval");
    assert_eq!(value.to_display(), "14");

    host.eval("insert(\"x\")\nmessage(\"done\")", &ctx)
        .expect("eval statements");
    assert_eq!(
        host.take_actions(),
        vec![
            EditorAction::Insert("x".to_string()),
            EditorAction::Message("done".to_string()),
        ]
    );
}

#[test]
fn re_adding_a_script_replaces_its_registrations() {
    let mut host = ScriptHost::new();
    host.add_script(
        "a.tw",
        r#"
            fn old_cmd() { message("old") }
            command("Old", "old_cmd")
        "#,
    );
    assert!(host.has_command("Old"));

    host.add_script(
        "a.tw",
        r#"
            fn new_cmd() { message("new") }
            command("New", "new_cmd")
        "#,
    );
    assert!(!host.has_command("Old"), "stale command should be dropped");
    assert!(host.has_command("New"));
}

#[test]
fn load_errors_are_recorded() {
    let mut host = ScriptHost::new();
    host.add_script("broken", "fn oops( {");
    assert!(!host.errors.is_empty());
}

#[test]
fn shipped_example_script_loads_and_registers_commands() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins/example.tw");
    let source = std::fs::read_to_string(path).expect("example.tw should exist");
    let mut host = ScriptHost::new();
    host.add_script("example.tw", &source);
    assert!(host.errors.is_empty(), "load errors: {:?}", host.errors);
    for title in [
        "TextWrite: Duplicate line",
        "TextWrite: Wrap in quotes",
        "TextWrite: Uppercase selection",
        "TextWrite: Document stats",
    ] {
        assert!(host.has_command(title), "missing command: {title}");
    }
    assert!(!host.keybindings().is_empty());
}

#[test]
fn every_shipped_plugin_runs_without_runtime_errors() {
    // Loading only parses a script; running every command it registers catches
    // calls to missing builtins (this is how `words` was caught) and bad
    // arguments. Commands run both with and without a selection.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins");
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("plugins/ should exist")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("tw"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no .tw plugins found");

    for path in entries {
        let source = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let mut host = ScriptHost::new();
        host.add_script(&name, &source);
        assert!(
            host.errors.is_empty(),
            "{name} load errors: {:?}",
            host.errors
        );

        let selected = "the quick brown fox\njumps over\t2 lazy dogs";
        let contexts = [
            EditorContext::default(),
            EditorContext {
                has_selection: true,
                selected_text: selected.to_string(),
                selection_start: 0,
                selection_end: selected.len(),
                doc_text: selected.to_string(),
                ..Default::default()
            },
        ];
        let titles = host.command_titles();
        for context in &contexts {
            for title in &titles {
                host.run_command(title, context);
            }
        }
        assert!(
            host.errors.is_empty(),
            "{name} runtime errors: {:?}",
            host.errors
        );
    }
}

#[test]
fn shipped_cursor_script_loads_and_changes_the_cursor() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins/cursor.tw");
    let source = std::fs::read_to_string(path).expect("cursor.tw should exist");
    let mut host = ScriptHost::new();
    host.add_script("cursor.tw", &source);
    assert!(host.errors.is_empty(), "load errors: {:?}", host.errors);

    let ctx = EditorContext::default();
    for (title, style) in [
        ("TextWrite: Cursor bar", CursorStyle::Bar),
        ("TextWrite: Cursor block", CursorStyle::Block),
        ("TextWrite: Cursor underline", CursorStyle::Underline),
    ] {
        assert!(host.has_command(title), "missing command: {title}");
        assert_eq!(
            host.run_command(title, &ctx),
            vec![EditorAction::SetCursorStyle(style)]
        );
    }

    let keys: Vec<String> = host
        .keybindings()
        .iter()
        .map(|(k, _)| k.display())
        .collect();
    assert_eq!(keys, vec!["Alt+1", "Alt+2", "Alt+3"]);
}

#[test]
fn set_theme_queues_a_theme_switch() {
    let mut host = host_with(
        "test",
        r#"
            fn mono() { set_theme("monokai") }
            command("Monokai", "mono")
        "#,
    );
    assert_eq!(
        host.run_command("Monokai", &EditorContext::default()),
        vec![EditorAction::SetTheme("monokai".to_string())]
    );
}

#[test]
fn define_theme_registers_a_usable_palette() {
    let host = host_with(
        "test",
        r#"
            define_theme("Midnight", { background: 0x0B0E14, keyword: 0xBB9AF7 })
            fn mono() { set_theme("Midnight") }
            command("Midnight", "mono")
        "#,
    );
    let themes = host.themes();
    assert_eq!(themes.len(), 1);
    assert_eq!(themes[0].0, "Midnight");
    assert!(themes[0]
        .1
        .iter()
        .any(|(key, value)| key == "background" && *value == 0x0B0E14));
    assert_eq!(host.theme_names(), vec!["Midnight".to_string()]);
}

#[test]
fn define_theme_rejects_non_int_colors() {
    let mut host = ScriptHost::new();
    host.add_script("bad", "define_theme(\"x\", { background: \"nope\" })\n");
    assert!(!host.errors.is_empty(), "a string color is an error");
}

#[test]
fn exposes_the_active_theme_global() {
    let mut host = host_with(
        "test",
        r#"
            fn show() { message(theme) }
            command("Theme", "show")
        "#,
    );
    let ctx = EditorContext {
        theme: "monokai".to_string(),
        ..Default::default()
    };
    assert_eq!(
        host.run_command("Theme", &ctx),
        vec![EditorAction::Message("monokai".to_string())]
    );
}

#[test]
fn wait_queues_a_timed_step() {
    let mut host = host_with(
        "test",
        r#"
            fn animation() {
                set_cursor_style("bar")
                wait(150)
                set_cursor_style("block")
            }
            command("Animate", "animation")
        "#,
    );
    assert_eq!(
        host.run_command("Animate", &EditorContext::default()),
        vec![
            EditorAction::SetCursorStyle(CursorStyle::Bar),
            EditorAction::Wait(150),
            EditorAction::SetCursorStyle(CursorStyle::Block),
        ]
    );
}

#[test]
fn automator_package_is_importable() {
    let mut host = host_with(
        "test",
        r#"
            import "fast-text/automator"
            fn demo() {
                cursor_demo(100)
            }
            command("Demo", "demo")
        "#,
    );
    assert_eq!(
        host.run_command("Demo", &EditorContext::default()),
        vec![
            EditorAction::SetCursorStyle(CursorStyle::Bar),
            EditorAction::Wait(100),
            EditorAction::SetCursorStyle(CursorStyle::Block),
            EditorAction::Wait(100),
            EditorAction::SetCursorStyle(CursorStyle::Underline),
            EditorAction::Wait(100),
        ]
    );
}

#[test]
fn automator_package_supports_a_namespaced_import() {
    let mut host = host_with(
        "test",
        r#"
            import "fast-text/automator" as auto
            fn go() {
                auto.cursor("underline")
                auto.type_text("x")
            }
            command("Go", "go")
        "#,
    );
    assert_eq!(
        host.run_command("Go", &EditorContext::default()),
        vec![
            EditorAction::SetCursorStyle(CursorStyle::Underline),
            EditorAction::Insert("x".to_string()),
        ]
    );
}

#[test]
fn compiles_and_runs_automation_files() {
    let dir = std::env::temp_dir().join(format!("ft-auto-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("demo.tw");
    std::fs::write(
        &path,
        "import \"fast-text/automator\"\ncursor(\"block\")\nwait(50)\ntype_text(\"hi\")\n",
    )
    .unwrap();

    let mut host = ScriptHost::new();
    let file = path.to_string_lossy().into_owned();
    host.compile_file(&file).expect("compiles");
    assert_eq!(
        host.run_file(&file, &EditorContext::default())
            .expect("runs"),
        vec![
            EditorAction::SetCursorStyle(CursorStyle::Block),
            EditorAction::Wait(50),
            EditorAction::Insert("hi".to_string()),
        ]
    );

    // The `.tw` extension is optional.
    let bare = dir.join("demo").to_string_lossy().into_owned();
    host.compile_file(&bare)
        .expect("compiles without extension");

    // Compiling reports missing imports without running anything.
    let bad = dir.join("bad.tw");
    std::fs::write(&bad, "import \"nope/missing\"\n").unwrap();
    let err = host.compile_file(&bad.to_string_lossy()).unwrap_err();
    assert!(
        format!("{err}").contains("cannot find module"),
        "unexpected error: {err}"
    );

    // A missing file is an error, not a panic.
    assert!(host
        .compile_file(&dir.join("absent.tw").to_string_lossy())
        .is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shipped_automations_compile_and_run() {
    for rel in [
        "/../../automations/cursor-demo.tw",
        "/../../automations/type-demo.tw",
    ] {
        let path = format!("{}{}", env!("CARGO_MANIFEST_DIR"), rel);
        let mut host = ScriptHost::new();
        host.compile_file(&path)
            .unwrap_or_else(|e| panic!("compile {path} failed: {e}"));
        let actions = host
            .run_file(&path, &EditorContext::default())
            .unwrap_or_else(|e| panic!("run {path} failed: {e}"));
        assert!(!actions.is_empty(), "{path} produced no actions");
        assert!(
            actions.iter().any(|a| matches!(a, EditorAction::Wait(_))),
            "{path} should animate, but scheduled no waits"
        );
    }
}
