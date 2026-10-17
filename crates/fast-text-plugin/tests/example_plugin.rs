//! Integration test: the bundled example plugin loads and produces actions.

use fast_text_plugin::{EditorAction, EditorContext, PluginHost};

const EXAMPLE: &str = include_str!("../../../plugins/example.rhai");

#[test]
fn example_plugin_loads_without_errors() {
    let mut host = PluginHost::new();
    host.add_plugin("example.rhai", EXAMPLE);
    assert!(host.errors.is_empty(), "errors: {:?}", host.errors);

    let titles = host.command_titles();
    for expected in [
        "Duplicate line",
        "Trim trailing whitespace",
        "Wrap in quotes",
        "Document statistics",
    ] {
        assert!(titles.contains(&expected.to_string()), "missing {expected}");
    }
}

#[test]
fn wrap_in_quotes_wraps_the_selection() {
    let mut host = PluginHost::new();
    host.add_plugin("example.rhai", EXAMPLE);

    let ctx = EditorContext {
        has_selection: true,
        selected_text: "hello".to_string(),
        ..Default::default()
    };
    let actions = host.run_command("Wrap in quotes", &ctx);
    assert_eq!(actions, vec![EditorAction::Insert("\"hello\"".to_string())]);
}

#[test]
fn document_statistics_reports_a_message() {
    let mut host = PluginHost::new();
    host.add_plugin("example.rhai", EXAMPLE);

    let ctx = EditorContext {
        file_name: "demo.rs".to_string(),
        line_count: 3,
        doc_len: 42,
        language: "Rust".to_string(),
        ..Default::default()
    };
    let actions = host.run_command("Document statistics", &ctx);
    match actions.as_slice() {
        [EditorAction::Message(msg)] => {
            assert!(msg.contains("demo.rs"));
            assert!(msg.contains("3 lines"));
        }
        other => panic!("unexpected actions: {other:?}"),
    }
}

#[test]
fn duplicate_line_edits_the_document() {
    let mut host = PluginHost::new();
    host.add_plugin("example.rhai", EXAMPLE);
    let actions = host.run_command("Duplicate line", &EditorContext::default());
    // Should move to line start/end, capture selection, then insert a copy.
    assert!(actions.iter().any(|a| matches!(a, EditorAction::Insert(_))));
}

#[test]
fn keybindings_from_example_are_registered() {
    let mut host = PluginHost::new();
    host.add_plugin("example.rhai", EXAMPLE);
    let key = fast_text_plugin::KeySpec::parse("ctrl+shift+q").unwrap();
    assert_eq!(host.lookup_key(&key).as_deref(), Some("Wrap in quotes"));
}
