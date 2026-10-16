// Example Fast Text plugin.
//
// Drop `.rhai` files in `plugins/` (next to the binary) or in
// `~/.config/fast-text/plugins/` and restart. `Ctrl+P` lists every command.
//
// Available globals: cursor, selection_start, selection_end, has_selection,
// selected_text, line, col, doc_len, line_count, file_name, language.

// --- Editing helpers -------------------------------------------------------

fn duplicate_line() {
    // Select the current line and re-insert it twice.
    move_caret("line_start", false);
    move_caret("line_end", true);
    let line_text = selected_text;
    move_caret("line_end", false);
    insert("\n" + line_text);
}
command_with_desc("Duplicate line", "Duplicate the current line", "duplicate_line");
bind("ctrl+shift+d", "Duplicate line");

fn trim_trailing_whitespace() {
    if !has_selection {
        message("Select text first");
        return;
    }
    let text = selected_text;
    let cleaned = "";
    for part in text.split("\n") {
        let t = part;
        while t.len() > 0 && t[t.len() - 1] == ' ' {
            t = t.sub_string(0, t.len() - 1);
        }
        if cleaned == "" {
            cleaned = t;
        } else {
            cleaned = cleaned + "\n" + t;
        }
    }
    delete_selection();
    insert(cleaned);
    message("Trimmed trailing whitespace");
}
command_with_desc("Trim trailing whitespace", "Remove trailing spaces from the selection", "trim_trailing_whitespace");

fn wrap_in_quotes() {
    if has_selection {
        insert("\"" + selected_text + "\"");
    } else {
        insert("\"\"");
        move_caret("left", false);
    }
}
command_with_desc("Wrap in quotes", "Surround the selection with double quotes", "wrap_in_quotes");
bind("ctrl+shift+q", "Wrap in quotes");

// --- Info ------------------------------------------------------------------

fn document_stats() {
    message(file_name + ": " + line_count + " lines, " + doc_len + " chars, " + language);
}
command_with_desc("Document statistics", "Show line/char counts for the current buffer", "document_stats");
