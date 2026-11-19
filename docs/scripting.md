# TextWrite editor scripting

Fast Text scripts are plain [TextWrite](https://github.com/Fast-Text-editor/Fast-text-editor/tree/main/crates/textwrite)
programs. They are not a second, editor-specific language: the same parser,
standard library and REPL power both the standalone language and the editor
bridge described here.

A script never mutates a buffer directly. It receives a **snapshot** of editor
state in a handful of globals, queues **actions** such as `insert(text)`, and
the editor applies those actions in order once the script returns. That one
design decision explains almost every surprising detail below, so it is worth
keeping in mind:

> **Globals are a photograph, not a live view.** `selected_text`, `cursor` and
> `document_text` describe the editor *before* your script ran. Calling
> `insert()` does not change them mid-script.

For the language itself (operators, loops, collections, builtins), see the
[TextWrite README](../crates/textwrite/README.md). This page is about the
`ScriptHost` bridge: what the editor publishes to scripts, what scripts can ask
it to do, and how failures are reported.

## Contents

- [A first script](#a-first-script)
- [Where scripts live](#where-scripts-live)
- [The editor context](#the-editor-context)
- [The action queue](#the-action-queue)
- [Selections and replacement](#selections-and-replacement)
- [Host functions](#host-functions)
- [Commands, keys and hooks](#commands-keys-and-hooks)
- [Themes from a script](#themes-from-a-script)
- [The status line](#the-status-line)
- [Errors and feedback](#errors-and-feedback)
- [Imports and name clashes](#imports-and-name-clashes)
- [Automations](#automations)
- [Language gotchas](#language-gotchas)
- [Testing a script](#testing-a-script)
- [Further reading](#further-reading)

## A first script

Save this as `~/.config/fast-text/plugins/hello.tw`. The editor picks it up
on the next reload (`Alt+M`, then `R`) or at the next start.

```textwrite
# Greet the selection for Fast Text — a TextWrite plugin.

fn hello_selection() {
    if not has_selection {
        message('Select some text first')
        return
    }
    message('Hello, ' + selected_text + '!')
}
command_with_desc('Hello: selection', 'Greet the selected text', 'hello_selection')
```

Press `Ctrl+Shift+P`, type *Hello* and hit `Enter`. The greeting appears in the
status bar. Nothing was inserted into the buffer, so this script could not be
simpler — and it already shows the three moving parts:

1. **Globals** (`has_selection`, `selected_text`) carry editor state in.
2. **Host functions** (`message`) request an editor effect.
3. **Registration** (`command_with_desc`) makes the function reachable.

## Where scripts live

| Location | Purpose |
| --- | --- |
| `~/.config/fast-text/plugins/` | Your scripts and installed registry plugins |
| `./plugins/` | Project-local scripts, loaded alongside the user directory |
| `~/.config/fast-text/plugins.disabled` | One file name per line to disable a plugin without deleting it |
| `~/.config/fast-text/themes/*.toml` | Shared-schema themes, loaded by both the TUI and the GUI |

Only `.tw` (TextWrite) and `.rhai` files are loaded. If both directories
contain the same file name, the user directory wins. Open the manager with
`Alt+M` to see what was found, toggle it, edit it or install a first-party
plugin from the **Available** list.

The registry is a separate, optional layer. `ft plugin search markdown`,
`ft install smart-brackets` and `ft plugin list` work entirely offline for all
26 bundled plugins; only entries that are not bundled need a network
connection.

## The editor context

Before each command or hook runs, `ScriptHost` copies the current editor state
into these globals. All indices are **character** offsets, not byte offsets.

| Global | Type | Meaning |
| --- | --- | --- |
| `cursor` | int | Caret position in the buffer |
| `selection_start` | int | Anchor of the current selection |
| `selection_end` | int | Head of the current selection |
| `has_selection` | bool | Whether a non-empty selection is active |
| `selected_text` | str | Text inside the selection, or `''` |
| `line` | int | Zero-based caret line |
| `col` | int | Zero-based caret column |
| `doc_len` | int | Length of the whole document in characters |
| `line_count` | int | Number of lines in the document |
| `file_name` | str | Display name of the buffer, e.g. `notes.md` |
| `language` | str | Language name, e.g. `Rust` or `Markdown` |
| `theme` | str | Name of the active theme |
| `document_text` | str | The entire buffer |

A minimal Alice-in-Wonderland report shows how they fit together:

```textwrite
fn where_am_i() {
    let scope = 'document'
    let text = document_text
    if has_selection {
        scope = 'selection'
        text = selected_text
    }
    message("{scope}: {len(words(text))} words at Ln {line + 1}, Col {col + 1}")
}
command_with_desc('Where am I?', 'Report the caret and word count', 'where_am_i')
```

## The action queue

`ScriptHost` does not hold a mutable document. Every editor effect is an
`EditorAction` pushed onto a queue; `App` drains the queue after the script
finishes. Actions run in the order they were queued, which means a script can
describe a multi-step edit without any callbacks:

```textwrite
fn shout() {
    if not has_selection {
        message('Select the text to shout')
        return
    }
    delete_selection()
    insert(upper(selected_text))
    message('Shouted')
}
command_with_desc('Shout', 'Upper-case the selected text', 'shout')
```

The sequence is:

```text
selected text ──► EditorContext ──► globals ──► your fn
                                                  │
                                                  ▼
                                      queue: DeleteSelection
                                             Insert("HELLO")
                                             Message("Shouted")
                                                  │
                                                  ▼
                                     App::apply_actions ──► document
```

Two consequences matter in practice:

- **Read everything you need before queueing edits.** In the example,
  `selected_text` is captured inside the argument to `insert()` while the
  global still describes the original buffer. You may also store it in a
  local variable first; both forms work.
- **There is no read-back.** A script cannot inspect the text it just
  inserted. If you need a later transformation, write a second command and
  let the user run it after the first has been applied.

Actions that affect the UI rather than the buffer (`set_theme`, `set_status`,
`message`, `save`, `format`) are queued in exactly the same way and are applied
in the same order. `wait(ms)` is special: during an automation it schedules
everything after it to run `ms` milliseconds later.

## Selections and replacement

There is deliberately no single "replace selection" host function. Instead,
queue `delete_selection()` and `insert(text)`; the editor applies them as one
undoable sequence:

```textwrite
fn wrap_in_quotes() {
    if not has_selection {
        message('Select the text to wrap')
        return
    }
    delete_selection()
    insert('"' + selected_text + '"')
    set_cursor(selection_start + 1)
}
command_with_desc('Wrap in quotes', 'Surround the selection with quotes', 'wrap_in_quotes')
```

`set_cursor` moves the caret *after* the insertion has been applied, so using
`selection_start + 1` places it just inside the opening quote. The bundled
[`brackets.tw`](../crates/fast-text-tui/plugin-catalog/brackets.tw) plugin uses
this pattern for all five bracket pairs and steps over an existing closing
character instead of doubling it.

Useful variants:

- `select(anchor, head)` selects an arbitrary range.
- `select_all()` selects the whole document.
- `delete(start, end)` removes a known range; `insert_at(at, text)` inserts at
  a known index.
- `move_caret(direction, extend)` accepts `left`, `right`, `up`, `down`,
  `word_left`, `word_right`, `line_start`, `line_end`, `doc_start`, `doc_end`.
  Pass `true` as the second argument to extend the selection.

## Host functions

These names are provided by the editor and take precedence over builtins or
user functions of the same name. Calls in method position work too:
`insert('x')` and `'x'.insert()` queue the same action.

### Output

| Function | Effect |
| --- | --- |
| `print(...)` | Joins arguments with a space and shows the result as a status message |
| `write(...)` | Joins arguments with **no** separator and shows the result as a status message |
| `message(...)` | Joins arguments with a space and shows the result as a status message |

The builtin `print`/`write` output is discarded inside the editor; these host
versions replace them so script feedback is always visible.

### Editing

| Function | Effect |
| --- | --- |
| `insert(text)` | Insert at the caret, replacing any selection |
| `insert_at(at, text)` | Insert at a character index (clamped to the buffer) |
| `delete(start, end)` | Delete a character range |
| `delete_selection()` | Delete the active selection |
| `backspace()`, `delete_forward()`, `newline()` | Single-step edits |
| `set_cursor(at)` | Move the caret to a character index |
| `select(anchor, head)`, `select_all()` | Set the selection |
| `move_caret(dir, extend)` | Move or extend in one of the ten directions listed above |
| `undo()`, `redo()` | Undo/redo one step |
| `save()`, `format()` | Save the buffer, or format it through its language server |

### Editor state

| Function | Effect |
| --- | --- |
| `set_language(name)` | Change the buffer's syntax language |
| `set_cursor_style(style)` | `block`, `bar` or `underline` (with aliases) |
| `set_theme(name)` | Switch to a built-in, user or script-defined theme |
| `define_theme(name, colors)` | Define a theme from a map of `0xRRGGBB` colors |
| `set_status(text)` | Replace the right-hand status line until cleared |
| `clear_status()` | Restore the built-in status line |
| `wait(ms)` | Delay the following actions during an automation |

### Registration

| Function | Effect |
| --- | --- |
| `command(title, fn_name)` | Register a zero-argument function in `Ctrl+Shift+P` |
| `command_with_desc(title, description, fn_name)` | The same, with palette help text |
| `bind(key, title)` | Bind a `KeySpec` string such as `'alt+p'` to a registered command |

## Commands, keys and hooks

Put registration at **top level**; put editing inside functions. Top-level
statements run once when the editor reloads scripts, with a default context,
and any actions they queue are thrown away.

```textwrite
fn tidy() { /* ... */ }
command_with_desc('Tidy selection', 'Trim and reflow the selection', 'tidy')
bind('alt+g', 'Tidy selection')
```

Keys use the shared `KeySpec` spelling: `ctrl+s`, `shift+f3`, `alt+p`,
`ctrl+shift+i`, `left`, `f5`. `cmd`/`super` are accepted but treated as `ctrl`
so macOS gets the conventional shortcuts. A binding that cannot be parsed
reports `unknown key: …` as a message instead of failing the load.

Two lifecycle hooks are dispatched, both optional and both recursion-guarded:

| Hook | When it runs |
| --- | --- |
| `on_buffer_open()` | At startup and whenever a file is opened |
| `on_buffer_save()` | After a successful save |

There is no `on_load`. Registration is top-level code; editing belongs in a
hook or command. A hook that calls `save()` does not re-enter itself.

## Themes from a script

`define_theme` takes a name and a map of color keys to `0xRRGGBB` integers.
Keys you omit fall back to the dark theme, so a small palette is enough:

```textwrite
define_theme('Ink', {
    background: 0x11151C,
    foreground: 0xDDE1E8,
    status_accent: 0x82AAFF,
    keyword: 0xC792EA,
    string: 0xC3E88D,
})

fn use_ink() {
    set_theme('Ink')
}
command_with_desc('Theme: Ink', 'Switch to the Ink palette', 'use_ink')
```

Available keys are `background`, `foreground`, `gutter`, `gutter_active`,
`cursor_line`, `selection`, `status_bg`, `status_fg`, `status_accent`,
`prompt_bg`, `tab_active`, `tab_inactive`, `error`, `warning`, and the syntax
keys `plain`, `keyword`, `type`, `function`, `string`, `number`, `comment`,
`operator`, `punctuation`, `constant`, `attribute`, `variable`.

Values are masked to 24 bits; a value such as `0xFF82AAFF` is accepted and
becomes `0x82AAFF`. Colors that are not integers produce a runtime error,
recorded with the script's name. The TUI reads the same shared TOML schema as
the GUI for file-based themes; `define_theme` is the in-script alternative.

## The status line

`set_status` hands the right-hand side of the status bar to a script. The
string is shown verbatim, in the theme's accent color, until `clear_status`
runs. Transient messages (from `message`, `print` or `write`) take precedence
so feedback is never hidden.

```textwrite
fn status_writing() {
    let count = len(words(document_text))
    let minutes = (count + 199) / 200
    set_status("Writing: {count} words  ·  ~{minutes} min read")
}
command_with_desc('Status: writing', 'Show word count and reading time', 'status_writing')
bind('alt+s', 'Status: writing')
```

[`status_line.tw`](../crates/fast-text-tui/plugin-catalog/status_line.tw)
ships four such formats, refreshes them from `on_buffer_open` and
`on_buffer_save`, and offers *Restore default status line* in the palette.

## Errors and feedback

There are three distinct failure channels, and knowing which one you are
looking at saves a lot of guesswork.

1. **Bad host arguments are runtime errors.** `insert()` without a string, or
   `set_cursor(-1)`, fails before queueing anything. The error is recorded in
   `ScriptHost.errors` as `"<command title>: <message>"` and the status bar
   shows it. Examples:
   - `` `insert` expects 1 argument(s) ``
   - `` `insert` expects a string for argument 1, found int ``
   - `` `set_cursor` expects a non-negative index, found -1 ``
2. **Invalid options become messages, not errors.** An unknown
   `move_caret` direction, cursor style or keybinding queues a `Message`
   describing the problem; the script continues.
3. **Unknown names fail silently at lookup time.** `run_command` with a title
   that was never registered returns no actions. `has_hook`/`run_hook` and
   `has_command` let you check before calling. A missing hook is a no-op; a
   hook that *does* fail records `hook <name>: <error>` and also shows
   `<name> failed: <error>` in the status bar.

Script load errors (a syntax error in a `.tw` file, for example) are listed
in the plugin manager next to the offending plugin, and compilation errors
include a line and column. The REPL (`Ctrl+E`) returns errors to its own
output rather than the persistent error log, which makes it a convenient
scratchpad for testing expressions.

## Imports and name clashes

All loaded scripts share **one** interpreter and one top-level namespace. Two
files that both define `fn format()` or `let cache = {}` will overwrite one
another. Give every helper a short file-specific prefix:

```textwrite
# In status_line.tw
let st_template = 'compact'
fn st_scope() { /* ... */ }

# In log_summary.tw
let lg_levels = ['ERROR', 'WARN', 'INFO']
fn lg_level_of(line) { /* ... */ }
```

The catalog uses `ws_`, `tt_`, `cmt_`, `csv_`, `mdx_`, `dt_`, `iden_`,
`freq_`, `box_`, `aln_`, `ciph_`, `todo_`, `wrp_`, `path_`, `br_`, `mdp_`,
`st_`, `ol_`, `js_`, `sq_`, `tb_` and `lg_`. The catalog test loads every
script together specifically to catch collisions.

`import './helper.tw'` resolves relative to the importing script's folder;
`import './helper.tw' as h` binds a namespace instead of merging names.
Modules are run once and cached. Registered modules such as
`import 'fast-text/automator'` need no path.

## Automations

Scripts in `~/.config/fast-text/plugins/` are editor plugins. For a one-shot
typing or cursor animation, put a `.tw` file anywhere and run it with `Alt+R`
(the *Automation* prompt). These scripts run with an editor context too, but
`wait(ms)` schedules later actions on a timeline so a sequence can be watched.

```textwrite
import 'automator'

fn demo() {
    type_text('Hello from TextWrite')
    pause(200)
    move('word_left', false)
    cursor('block')
}
demo()
```

Top-level execution is intentional here: an automation is a program, not a
command registry. The bundled `automations/` folder has two complete examples.

## Language gotchas

These are the details that most often trip up editor scripts:

- **Hex only for bases other than ten.** Write `0x1F`, not `0b11111`.
- **Floats need digits on both sides.** `0.5` is valid; `.5` and `1.` are not.
- **Double quotes interpolate `{...}`.** A literal brace needs single quotes:
  use `'{'` and `'}'`, or escape it. This matters for `{}`, Markdown links and
  JSON templates. Single-quoted strings never interpolate.
- **Trailing commas are allowed in list and map literals**, not in call
  arguments, parameter lists or command arguments.
- **`if` is not an expression.** Assign in each branch:
  `let x = ''` then `if ok { x = 'yes' } else { x = 'no' }`.
- **`print` joins with a space; `write` joins with nothing** — and in the
  editor both become status messages.
- **`repeat 3 times { … }`** is the counted loop; `while` and `for` are also
  available. `break` and `continue` only work inside loops.
- **No `ord` or `chr` builtin.** Transform characters with a map or with
  `chars()`, as [`cipher.tw`](../crates/fast-text-tui/plugin-catalog/cipher.tw)
  does.
- **Closures capture their environment by reference**, so a function defined
  inside another function can still see the outer variables when it is called
  later.
- **Assignment to an undeclared name is an error.** Declare the variable with
  `let` first. Missing map keys error too; use `get(map, key, fallback)`.

## Testing a script

The first-party scripts are compiled into the editor with `include_str!` and
exercised by `crates/fast-text-tui/src/catalog.rs`. Its test harness loads the
whole catalog into one `ScriptHost`, runs every registered command against two
contexts, and asserts that the error log stays empty. Behavior tests use a
small helper:

```rust
let mut host = ScriptHost::new();
host.add_script("mine.tw", include_str!("../plugin-catalog/mine.tw"));
let context = EditorContext {
    has_selection: true,
    selected_text: "hello".into(),
    ..Default::default()
};
let actions = host.run_command("Mine: shout", &context);
assert!(actions.contains(&EditorAction::Insert("HELLO".into())));
```

Tests instantiate `ScriptHost` directly, so they never touch the network and
never spawn an editor or a package manager. Add your script to `CATALOG` and
`registry/index.json`, then run:

```sh
cargo test -p fast-text-tui --lib catalog::tests
```

## Further reading

- [TextWrite language reference](../crates/textwrite/README.md) — syntax,
  operators and the full standard library
- [Plugin catalog](../crates/fast-text-tui/plugin-catalog/README.md) — every
  bundled script, with the one-line summary shown in the manager
- [Fast Text README](../README.md) — keybindings, configuration, themes and
  the registry command line
