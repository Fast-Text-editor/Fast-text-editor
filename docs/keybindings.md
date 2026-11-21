# Keybindings

Fast Text uses one shared keymap across the terminal, desktop, and web frontends.
`Cmd` on macOS is treated as `Ctrl`.

## Default keybindings

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `Ctrl+S` | Save | `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `F2` | Save as | `Ctrl+A` | Select all |
| `Ctrl+O` | Open file | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` | Cut / Copy / Paste |
| `Ctrl+N` | New buffer | `Ctrl+F` | Find |
| `Ctrl+W` | Close buffer | `F3` / `F4` | Find next / previous |
| `Ctrl+Q` | Quit | `Ctrl+H` | Replace all |
| `Ctrl+G` | Go to line | `Ctrl+T` | Toggle theme |
| `Ctrl+L` | Language picker | `Ctrl+P` | Fuzzy file finder |
| `Ctrl+Shift+P` | Command palette | `Alt+:` | `:` command bar |
| `Ctrl+Space` | Complete | `F5` | Hover info |
| `F12` | Go to definition | `Alt+H` | Help |
| `Esc` | Collapse selection | `Alt+M` | Plugin manager |
| `Ctrl+B` | Toggle explorer | `Ctrl+Shift+O` | Open folder |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab | `Alt+1` `Alt+2` `Alt+3` | Cursor bar / block / underline |
| `Ctrl+Shift+F` / `Alt+F` | Find in files | `Ctrl+D` | Select next occurrence |
| `Alt+O` | Document symbols | `Alt+N` | Rename symbol |
| `Ctrl+Shift+I` | Format document | `Ctrl+.` | Code actions |
| `Ctrl+/` | Toggle comment | `Ctrl+Shift+D` | Duplicate line |
| `Ctrl+Shift+K` | Delete line | `Alt+↑` / `Alt+↓` | Move line up / down |
| `Alt+Shift+↑` / `Alt+Shift+↓` | Add cursor above / below | `F6` / `F7` | Record / replay macro |
| `Alt+U` | Check for updates | `Alt+C` | Terminal panel |
| `Alt+V` | Toggle Vim layer | `Ctrl+←/↑` `Ctrl+→/↓` | Previous / next split pane |

A few chords are terminal-dependent (`Ctrl+Shift+<letter>`, `Ctrl+.`). Every action
also has a palette entry and a `[keys]` id, so you can rebind it.

## Editing behavior

- `Shift` + arrows extends the selection; `Alt` + left/right moves by word.
- With more than one pane open, `Ctrl` + arrows moves focus between panes.
- Mouse editing works in the text area: click to place the caret, drag or `Shift`-click to select, and scroll with the wheel.
- `Ctrl+D` adds the next occurrence of the word/selection as another caret.
- `Alt+Shift+↑/↓` add a cursor above/below; typing edits every caret as a single undo step.

## Rebinding keys

Settings live in `$XDG_CONFIG_HOME/fast-text/config.toml` (usually `~/.config/fast-text/config.toml`).
`FAST_TEXT_CONFIG` or `--config PATH` overrides that. Missing file means defaults.

```toml
[keys]
# A chord is modifier+modifier+key, joined with +. Modifiers: ctrl, alt, shift.
# The right side is a built-in command id, a palette command title, or a macro name.
"ctrl+shift+s" = "save_as"
"alt+w"         = "close"
"ctrl+b"        = "toggle_sidebar"
"alt+m"         = "plugin_manager"
"f6"            = "macro_record"
"alt+f7"        = "macro_tidy imports"
```

## Built-in command IDs

Common command IDs used in `[keys]`:

| ID | Action |
| --- | --- |
| `save`, `save_as`, `open`, `new`, `close`, `quit` | File/buffer actions |
| `undo`, `redo`, `copy`, `cut`, `paste`, `select_all` | Editing |
| `find`, `replace`, `goto_line`, `goto_file` | Navigation/search |
| `command_palette`, `language`, `theme_toggle` | Pickers/UI |
| `toggle_sidebar`, `open_folder` | Explorer |
| `plugin_manager`, `eval`, `automation` | Scripts/plugins |
| `complete`, `hover`, `definition`, `rename`, `format`, `code_action`, `symbols` | LSP |
| `comment_toggle`, `duplicate_line`, `delete_line` | Line editing |
| `move_line_up`, `move_line_down` | Move lines |
| `cursor_above`, `cursor_below`, `select_next_occurrence` | Multi-cursor |
| `macro_record`, `macro_replay` | Macros |
| `split_right`, `split_down`, `pane_next`, `pane_previous`, `pane_close` | Panes |
| `toggle_console` | Terminal panel |
| `check_updates` | Updates |

You can also bind to a command palette title or a saved macro name.
