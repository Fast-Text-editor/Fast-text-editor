use fast_text_core::{Document, Position, SearchDirection};

#[test]
fn insert_char_appends_and_moves_caret() {
    let mut doc = Document::new();
    for c in "hello".chars() {
        doc.insert_char(c);
    }
    assert_eq!(doc.text(), "hello");
    assert_eq!(doc.cursor(), 5);
}

#[test]
fn undo_groups_typing_burst() {
    let mut doc = Document::new();
    for c in "hello".chars() {
        doc.insert_char(c);
    }
    assert_eq!(doc.text(), "hello");
    doc.undo();
    // A burst of single-char inserts undoes as one unit.
    assert_eq!(doc.text(), "");
    doc.redo();
    assert_eq!(doc.text(), "hello");
}

#[test]
fn backspace_removes_chars_and_undoes() {
    let mut doc = Document::from_str("abc");
    doc.set_cursor(3);
    doc.backspace();
    doc.backspace();
    assert_eq!(doc.text(), "a");
    assert_eq!(doc.cursor(), 1);
    doc.undo();
    // Backspaces coalesce into one undo unit too.
    assert_eq!(doc.text(), "abc");
}

#[test]
fn insert_replaces_selection() {
    let mut doc = Document::from_str("hello world");
    doc.select(0, 5);
    doc.insert_str("goodbye");
    assert_eq!(doc.text(), "goodbye world");
    doc.undo();
    assert_eq!(doc.text(), "hello world");
}

#[test]
fn cursor_movement_up_down_preserves_goal_column() {
    let mut doc = Document::from_str("long line here\nx\nanother long line");
    doc.set_cursor(7); // col 7 of line 0
    doc.move_down(false);
    assert_eq!(doc.cursor_position(), Position::new(1, 1)); // clamped to short line
    doc.move_down(false);
    // Goal column restored on the longer line.
    assert_eq!(doc.cursor_position(), Position::new(2, 7));
}

#[test]
fn word_movement_skips_punctuation() {
    let mut doc = Document::from_str("foo bar_baz, qux");
    doc.set_cursor(0);
    doc.move_word_forward(false);
    assert_eq!(doc.cursor(), 4); // start of bar_baz
    doc.move_word_forward(false);
    assert_eq!(doc.cursor(), 13); // start of qux
    doc.move_word_backward(false);
    assert_eq!(doc.cursor(), 4);
}

#[test]
fn find_forward_wraps() {
    let mut doc = Document::from_str("alpha beta alpha gamma");
    doc.set_cursor(0);
    let m = doc
        .find("alpha", SearchDirection::Forward, true, true)
        .unwrap();
    assert_eq!((m.start, m.end), (0, 5));
    let m = doc
        .find("alpha", SearchDirection::Forward, true, true)
        .unwrap();
    assert_eq!((m.start, m.end), (11, 16));
    // Wraps back to the first match.
    let m = doc
        .find("alpha", SearchDirection::Forward, true, true)
        .unwrap();
    assert_eq!((m.start, m.end), (0, 5));
}

#[test]
fn replace_all_replaces_every_occurrence() {
    let mut doc = Document::from_str("a a a");
    let n = doc.replace_all("a", "b", true);
    assert_eq!(n, 3);
    assert_eq!(doc.text(), "b b b");
    doc.undo();
    assert_eq!(doc.text(), "a a a");
}

#[test]
fn delete_range_is_one_undo_step() {
    let mut doc = Document::from_str("0123456789");
    doc.delete_range(2, 5);
    assert_eq!(doc.text(), "0156789");
    doc.undo();
    assert_eq!(doc.text(), "0123456789");
}

#[test]
fn save_and_reload_round_trip() {
    let dir = std::env::temp_dir().join("fast-text-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("round_trip.txt");

    let mut doc = Document::from_str("persisted\ncontent\n");
    doc.save_as(&path).unwrap();
    assert!(!doc.is_dirty());

    let reloaded = Document::from_file(&path).unwrap();
    assert_eq!(reloaded.text(), "persisted\ncontent\n");
    assert_eq!(reloaded.buffer().len_lines(), 3);
}
