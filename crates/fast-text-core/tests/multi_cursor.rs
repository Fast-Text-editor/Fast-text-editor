use fast_text_core::Document;

#[test]
fn ctrl_d_selects_the_word_then_each_next_occurrence() {
    let mut doc = Document::from_str("let alpha = 1;\nlet alpha = 2;\n");
    doc.set_cursor(5); // inside the first `alpha`
    assert!(doc.select_next_occurrence(true));
    assert_eq!(doc.buffer().slice(4, 9), "alpha");
    assert!(!doc.has_multi_cursor());

    // A second press adds the next occurrence as a secondary selection.
    assert!(doc.select_next_occurrence(true));
    assert!(doc.has_multi_cursor());
    assert_eq!(doc.extra_selections().len(), 1);

    // A third press finds nothing more.
    assert!(!doc.select_next_occurrence(true));
}

#[test]
fn typing_replaces_every_selection_at_once() {
    let mut doc = Document::from_str("foo bar foo baz foo");
    doc.set_cursor(0);
    doc.select_next_occurrence(true); // select first "foo"
    doc.select_next_occurrence(true); // add second
    doc.select_next_occurrence(true); // add third
    doc.insert_str("qux");
    assert_eq!(doc.text(), "qux bar qux baz qux");
}

#[test]
fn multi_cursor_edits_are_a_single_undo_step() {
    let mut doc = Document::from_str("x x x");
    doc.set_cursor(0);
    doc.select_next_occurrence(true);
    doc.select_next_occurrence(true);
    doc.select_next_occurrence(true);
    doc.insert_str("yy");
    assert_eq!(doc.text(), "yy yy yy");
    doc.undo();
    assert_eq!(doc.text(), "x x x");
}

#[test]
fn adding_carets_below_places_them_on_following_lines() {
    let mut doc = Document::from_str("aaa\nbbb\nccc\n");
    doc.set_cursor(1);
    doc.add_caret_below();
    assert_eq!(doc.extra_selections().len(), 1);
    assert_eq!(doc.extra_selections()[0].head, 5); // start of "bbb"

    doc.insert_char('Z');
    assert_eq!(doc.text(), "aZaa\nbZbb\nccc\n");
}

#[test]
fn backspace_deletes_at_every_caret() {
    let mut doc = Document::from_str("aaa\nbbb\n");
    doc.set_cursor(1);
    doc.add_caret_below(); // a second caret at the start of "bbb"
    doc.backspace();
    assert_eq!(doc.text(), "aa\nbb\n");
}

#[test]
fn navigation_collapses_the_multi_cursor() {
    let mut doc = Document::from_str("one two one");
    doc.set_cursor(0);
    doc.select_next_occurrence(true);
    doc.select_next_occurrence(true);
    assert!(doc.has_multi_cursor());
    doc.move_down(false);
    assert!(!doc.has_multi_cursor());
}

#[test]
fn apply_edits_is_one_undoable_step_in_any_order() {
    let mut doc = Document::from_str("one two three");
    doc.apply_edits(&[(8, 13, "THREE".to_string()), (0, 3, "ONE".to_string())]);
    assert_eq!(doc.text(), "ONE two THREE");
    doc.undo();
    assert_eq!(doc.text(), "one two three");
}

#[test]
fn word_under_cursor_handles_punctuation() {
    let mut doc = Document::from_str("let my_var = other(x);");
    doc.set_cursor(6);
    assert_eq!(doc.word_under_cursor().as_deref(), Some("my_var"));
    doc.set_cursor(10);
    assert_eq!(doc.word_under_cursor().as_deref(), Some("my_var"));
    doc.set_cursor(12);
    assert_eq!(doc.word_under_cursor(), None);
}
