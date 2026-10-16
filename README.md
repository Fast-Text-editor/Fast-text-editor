# Fast Text

A fast, modern text editor written in Rust. Fast Text is built as a **Cargo
workspace** with a single powerful engine and thin frontends layered on top, so
the editor behaves identically everywhere.

```
fast-text/
├── crates/
│   ├── fast-text-core/    # the engine: rope buffer, edits, undo/redo, find
│   ├── fast-text-syntax/  # stateful, dependency-free syntax highlighting
│   ├── fast-text-plugin/  # Rhai-scripted plugins: commands + keybindings
│   ├── fast-text-lsp/     # a minimal Language Server Protocol client
│   ├── fast-text-wasm/    # WebAssembly bindings for the engine
│   ├── fast-text-gui/     # native desktop frontend (egui/eframe)
│   ├── fast-text-tui/     # terminal frontend (ratatui) — the `fast-text` / `ft` binaries
│   ├── fast-text-textwrite/ # TextWrite editor scripts: the `ScriptHost` bridge
│   └── textwrite/         # TextWrite: the Fast Text scripting language — `textwrite` / `tw`
├── tools/gen_langs.py     # generates the language registry from Helix
├── plugins/               # example Rhai plugins (loaded at startup)
├── web/                   # browser frontend (see web/README.md)
```

## Why this shape

Every frontend is a *thin shell* over `fast-text-core`. The engine knows nothing
about terminals, windows, or browsers; it just tracks text, cursors, and
history. That means:

- the same editing semantics (undo grouping, word motion, goal columns, search)
  work in every frontend;
- the core is trivially testable without a UI;
- new frontends (native GUI, web/WASM) reuse 100% of the editing logic.

## Building and running

```sh
cargo run -p fast-text-tui              # terminal editor: welcome buffer
cargo run -p fast-text-tui -- file.rs   # terminal editor: one or more files
cargo run -p fast-text-tui --bin ft     # same editor, short alias
cargo run -p fast-text-gui -- file.rs   # native desktop window (needs a display)
cargo run -p textwrite -- file.tw       # run a TextWrite script
cargo run -p textwrite                  # TextWrite REPL
cargo test                              # run the whole test suite
./web/serve.sh                          # build + serve the web frontend
```

The terminal editor builds two identical binaries: **`fast-text`** and the short
alias **`ft`**. After `cargo install --path crates/fast-text-tui` you can open a
file with `ft file.py` or `fast-text file.py`.

The native GUI needs a windowing system (X11 or Wayland). Running it over a
plain SSH session without a display fails with winit's
`neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set` — that is
expected, not a build problem.

Opening a path that does not exist yet (for example `ft notes.py`) starts a new
empty buffer named `notes.py`, highlighted according to the file name, ready for
`Ctrl+S` to write it. Run `ft` with no arguments to see the Rust welcome buffer
instead.

## Keys

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `Ctrl+S` | Save | `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `F2` | Save as | `Ctrl+A` | Select all |
| `Ctrl+O` | Open file | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` | Cut / Copy / Paste |
| `Ctrl+N` | New buffer | `Ctrl+F` | Find |
| `Ctrl+W` | Close buffer | `F3` / `F4` | Find next / previous |
| `Ctrl+Q` | Quit | `Ctrl+H` | Replace all |
| `Ctrl+G` | Go to line | `Ctrl+T` | Toggle theme |
| `Ctrl+L` | Language picker | `Ctrl+P` | Command palette |
| `Ctrl+Space` | Complete | `F5` | Hover info |
| `F12` | Go to definition | `Alt+H` | Help |
| `Esc` | Clear selection | | |
| `Ctrl+B` | Toggle explorer | `Ctrl+Shift+O` | Open folder |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab | `Ctrl+M` | Manage plugins |
| `Alt+1` / `Alt+2` / `Alt+3` | Cursor bar / block / underline | `Alt+H` | Help |

`Shift` + arrows extends the selection; `Ctrl` + left/right moves by word.

`Ctrl+L` opens a fuzzy-filterable picker listing **every supported language**
(300+). Type to narrow, `↑`/`↓` to move, `Enter` to apply.

`Ctrl+B` shows a Visual-Studio-style **folder explorer** along the left edge,
rooted at the folder you launched from (or `Ctrl+Shift+O` to pick another).
`↑`/`↓` move, `→` expands or steps in, `←` collapses or steps out, `Enter`
opens a file, `r` reloads, and `.` toggles dotfiles. With the mouse you can
click a file to open it, click a tab to switch, and click a tab's `✕` to close
it.

## Languages

Fast Text highlights **340+ languages**. A hand-curated core (below) ships with
tailored keyword/type/constant lists, comments, and string rules; everything
else is generated from Helix's configuration:

| Curated | Generated from Helix |
| --- | --- |
| Rust, JavaScript, TypeScript, Python, Ruby, Java, C, C++, C#, Go, HTML, CSS, JSON, TOML, Markdown, Shell, TextWrite, plain text | Zig, Kotlin, Haskell, Scala, Clojure, Elixir, Erlang, Dart, Lua, Perl, R, Julia, SQL, YAML, XML, Vue, Svelte, Dockerfile, Make, CMake, Nix, HCL/Terraform, GraphQL, Protobuf, and 300+ more |

The registry in `crates/fast-text-syntax/src/generated.rs` is produced by
`tools/gen_langs.py` from a [Helix](https://helix-editor.com) checkout. It reads
`languages.toml` (file types, comment tokens, block comments, language servers)
and the tree-sitter highlight queries under `runtime/queries/`, extracting the
keyword/type/constant word lists from captures like
`[ "if" "else" ] @keyword.control`. Regenerate with:

```sh
python3 tools/gen_langs.py /path/to/helix
```

C and C++ headers are recognized across the common spellings — `.c`/`.h` are C;
`.cpp`/`.cc`/`.cxx`/`.c++` and `.hpp`/`.hh`/`.hxx`/`.h++`/`.ipp`/`.tpp`/`.tcc`
are C++.

## Language servers

When a supported language server is on `PATH`, Fast Text starts it
automatically and shows:

- **diagnostics** — markers in the gutter (`E`/`W`/`i`) plus a summary and the
  current line's message in the status bar;
- **completion** (`Ctrl+Space`) in a popup — `↑`/`↓` to choose, `Enter`/`Tab`
  to accept, `Esc` to dismiss;
- **hover** (`F5`) and **go to definition** (`F12`).

Server names and launch commands come from the same Helix-derived table, so the
supported set is large: `rust-analyzer`, `pylsp`/`jedi`, `gopls`, `clangd`,
`typescript-language-server`, `jdtls`, `ruby-lsp`/`solargraph`,
`csharp-ls`/`omnisharp`, `bash-language-server`, `zls`, `haskell-language-server`,
`taplo`, `marksman`, `vscode-json-language-server`, and many more. For each
language Fast Text tries its preferred servers in order and uses the first one
installed. Servers start on a background thread, so the editor never blocks —
and with none installed it behaves exactly as before. Disable the integration
with `FAST_TEXT_LSP=0`.

## Plugins and editor scripts

Drop `.rhai` **plugins** or `.tw` **TextWrite scripts** into `plugins/` (next to
the binary) or `~/.config/fast-text/plugins/`. Each script registers commands and
keybindings; `Ctrl+P` lists them all. Scripts describe edits as *actions*
(`insert`, `delete_selection`, `move_caret`, `set_language`, …) and read editor
state from globals (`selected_text`, `has_selection`, `cursor`, `line`,
`file_name`, `language`, …):

```rhai
// plugins/example.rhai
fn wrap_in_quotes() {
    if has_selection { insert("\"" + selected_text + "\""); }
}
command_with_desc("Wrap in quotes", "Surround the selection with \"", "wrap_in_quotes");
bind("ctrl+shift+q", "Wrap in quotes");
```

```textwrite
# plugins/example.tw
fn wrap_in_quotes() {
    if has_selection {
        insert("\"" + selected_text + "\"")
    }
}
command_with_desc("Wrap in quotes", "Surround the selection with \"", "wrap_in_quotes")
bind("ctrl+shift+q", "Wrap in quotes")
```

Both languages run against the same editor API (the `EditorAction` queue), so a
script can be written in either. See `plugins/example.rhai` and
`plugins/example.tw`. Press **`Ctrl+E`** to evaluate a TextWrite snippet against
the current buffer and see the result in the status bar.

Scripts can also change the terminal cursor shape with `set_cursor_style`, which
### The plugin manager

Press **`Ctrl+M`** (or pick *Manage plugins* from the `Ctrl+P` palette) to
open a Visual-Studio-style **plugin manager**. It lists every discovered
plugin with its runtime, commands, keybindings, and any load error, and lets you
turn plugins on and off without deleting anything:

| Key | Action |
| --- | --- |
| `↑` / `↓`, `PageUp` / `PageDown`, `Home` / `End` | Move the selection |
| `Space` / `Enter` / `x` | Enable or disable the selected plugin |
| `i` | Install a plugin: copies a file into the user plugin directory |
| `d` | Uninstall: delete the plugin file (asks for `y` confirmation) |
| `r` | Rescan the plugin directories |
| `Esc` / `q` | Close the manager |

Toggling a plugin takes effect immediately and is remembered in
`~/.config/fast-text/plugins.disabled` (one file name per line). Pressing `i`
(or the palette's *Install plugin…*) asks for a path and copies the file into
`~/.config/fast-text/plugins/`, where it is picked up right away. The manager is
also reachable with the mouse: click a row to select it, then click it again to
toggle it.

### Cursor style

The cursor shape is **built in** — no plugin required: `Alt+1`, `Alt+2`, and
`Alt+3` select the bar, block, and underline shapes (also in the palette as
*Cursor: bar*, *Cursor: block*, and *Cursor: underline*). Scripts can change it
with `set_cursor_style`, which accepts `"bar"`, `"block"`, or `"underline"` —
exactly what `plugins/cursor.tw` does:

```textwrite
# plugins/cursor.tw
fn cursor_bar() { set_cursor_style("bar") }
fn cursor_block() { set_cursor_style("block") }
fn cursor_underline() { set_cursor_style("underline") }

command_with_desc("TextWrite: Cursor bar", "Use a thin vertical bar cursor", "cursor_bar")
command_with_desc("TextWrite: Cursor block", "Use a filled block cursor", "cursor_block")
command_with_desc("TextWrite: Cursor underline", "Use an underline cursor", "cursor_underline")

bind("alt+1", "TextWrite: Cursor bar")
bind("alt+2", "TextWrite: Cursor block")
bind("alt+3", "TextWrite: Cursor underline")
```

## Automations

An **automation** is a `.tw` file you run against the live editor. Press
**`Alt+R`**, enter a path (the `.tw` extension is optional), and Fast Text
*compiles* the file — parsing it and resolving every `import` — then *plays it
back* against the current buffer. Steps run in order and `wait(ms)` pauses before
the next step, so an automation can animate the cursor or type with a rhythm.

The editor automation package lives at `fast-text/automator`. Import it to get
friendly step helpers:

```textwrite
# automations/cursor-demo.tw  —  run with Alt+R
import "fast-text/automator"

cursor_demo(250)          # bar -> block -> underline, 250 ms apart
cursor("bar")             # leave the cursor as a bar
```

| helper | effect |
| --- | --- |
| `cursor(style)` | set the cursor shape (`"bar"`, `"block"`, `"underline"`) |
| `type_text(text)` | insert text at the cursor |
| `pause(ms)` / `wait(ms)` | wait `ms` before the next step |
| `move(dir, extend)` | move the caret (`"word_left"`, `"line_end"`, …) |
| `animate_cursor(styles, ms)` | cycle through a list of styles |
| `sequence(steps, ms)` | run zero-argument functions, pausing between |
| `cursor_demo(ms)` | the built-in bar → block → underline demo |

`automations/` ships two runnable demos: `cursor-demo.tw` and `type-demo.tw`.
Because a `.tw` file under `plugins/` runs its top-level code when it loads, keep
one-shot automations out of `plugins/` — put them anywhere else and run them with
`Alt+R`.

### Imports

TextWrite scripts can share code with `import`:

```textwrite
import "fast-text/automator"           # merge the module's names into this scope
import "fast-text/automator" as auto   # ...or keep them under a namespace

auto.cursor("block")
```

A module is any `.tw` file (resolved relative to the importing file, or the
`plugins/` directory) or a built-in package such as `fast-text/automator`. Its
top-level `fn`s and `let`s are its exports. Each module runs once, and circular
imports are reported as errors.

## TextWrite

**TextWrite** is Fast Text's own scripting language: a small, complete,
dependency-free interpreter written from scratch in Rust (crate `textwrite`,
binaries `textwrite` and the short alias `tw`). It reads like a mix of Rust and
English — Rust-style braces with word operators (`is`, `is not`, `and`, `or`,
`not`, `repeat N times`, `for x in ...`).

```textwrite
# comment
let name = "world"

fn greet(who) {
    print "Hello, {who}!"
}

greet(name)

for n in range(1, 6) {
    if n % 2 is 0 {
        print "{n} is even"
    }
}

repeat 3 times {
    print "again"
}
```

It is a full general-purpose language, not just a macro layer:

- **Values:** ints, floats, strings (with `{interpolation}`), booleans, `none`,
  ordered lists, and insertion-ordered maps.
- **Functions & closures:** `fn name(a, b) { ... }` and anonymous
  `fn(x) { ... }`; closures capture their environment (counters, memoization).
- **Control flow:** `if` / `else if` / `else`, `while`, `for x in ...`,
  `repeat N times`, `break`, `continue`, `return`.
- **Modules:** `import "path"` merges a module's top-level `fn`s and `let`s into
  the current scope; `import "path" as name` keeps them under a namespace.
  Built-in packages (like `fast-text/automator`) and `.tw` files both work.
- **Operators:** `+ - * / % **`, comparisons, `+= -= *= /= %=`, the English
  `is` / `is not` / `and` / `or` / `not`, and command-call style (`print "hi"`
  as well as `print("hi")`).
- **Standard library:** `len`, `type`, `str`/`int`/`float`, `range`, list and map
  helpers (`push`, `pop`, `sort`, `keys`, `values`, `get`, `has`), string helpers
  (`split`, `join`, `upper`, `trim`, `replace`, …), math (`abs`, `floor`,
  `sqrt`, `min`, `max`, …), higher-order (`map`, `filter`, `reduce`, `each`,
  `any`, `all`), `assert`/`error`, and (on native targets) file I/O. Many are
  also available as methods: `"hi".upper()`, `[3, 1, 2].sort()`.

The crate ships a REPL (`tw` with no arguments), a file runner (`tw script.tw`),
and `tw eval '1 + 2 * 3'`.

```sh
cargo install --path crates/textwrite   # installs `textwrite` and `tw`
tw                                       # start the REPL
tw examples.tw                           # run a script
```

The language is implemented in four passes — `lexer` → `parser` → `interp` over
an `ast`, with `stdlib` builtins — all hand-written with **zero dependencies**,
so it is portable (including to WebAssembly) and easy to embed. See
`crates/textwrite/README.md` for the full language reference and
`crates/textwrite/examples/` for runnable programs.

Inside the editor, TextWrite is wired up through the `fast-text-textwrite` crate,
which implements the language's [`Host`] hook and turns script calls into the
same `EditorAction`s Rhai plugins use. That means `.tw` files in `plugins/`
appear in `Ctrl+P` and can bind keys, and `Ctrl+E` evaluates a snippet in place.

[`Host`]: crates/textwrite/src/host.rs

## Frontends

All four frontends share the same engine:

| Frontend | Crate | What it is |
| --- | --- | --- |
| **Terminal** | `fast-text-tui` | ratatui-based TUI — the `fast-text` / `ft` binaries. Full-featured: tabs, plugins, LSP, themes. |
| **Desktop** | `fast-text-gui` | egui/eframe window. Same highlighter, painted into a multiline text widget. |
| **Web** | `fast-text-wasm` + `web/` | The engine compiled to WebAssembly; a small HTML/JS shell drives it. |
| **Library** | `fast-text-core` | The engine itself, embeddable in any Rust program. |

`fast-text-core` knows nothing about terminals, windows, or browsers; it just
tracks text, selections, and history.

## Engine design

- **Buffer** (`fast-text-core/src/buffer.rs`) — a `ropey` rope with load/save,
  dirty tracking, and a `revision` counter so frontends can invalidate caches
  cheaply.
- **Document** (`document.rs`) — the high-level API: cursor movement with a
  sticky *goal column*, selection-aware edits, find/replace, undo/redo.
- **History** (`history.rs`) — reversible `Edit`s grouped into transactions.
  Bursts of typing and streams of backspaces coalesce into single undo steps.
- **Syntax** (`fast-text-syntax`) — a stateful lexer that carries block-comment
  and multi-line-string state between lines, so it colorizes scrollable files
  without re-parsing from the top every frame. Language definitions are a data
  table: a curated core plus 300+ entries generated from Helix, addressed by a
  cheap `Copy` `LanguageId` handle.

## Status & roadmap

Implemented today:

- [x] Rope buffer, editing, undo/redo, selections, clipboard (internal)
- [x] Goal-column cursor motion, word navigation, select-all
- [x] Find with wrap-around, replace-all, go-to-line
- [x] Syntax highlighting for **340+ languages**: a curated core (Rust,
  JavaScript, TypeScript, Python, Ruby, Java, C, C++, C#, Go, HTML, CSS, JSON,
  TOML, Markdown, Shell, TextWrite, plain text) plus a Helix-derived registry covering
  Zig, Kotlin, Haskell, Scala, Clojure, Elixir, Dart, Lua, Perl, R, Julia, SQL,
  YAML, XML, Vue, Svelte, Dockerfile, Make, Nix, HCL, GraphQL, and hundreds
  more — including C/C++ headers (`.h`, `.hpp`, `.hh`, `.hxx`, `.h++`, `.ipp`,
  `.tpp`, `.tcc`)
- [x] `Ctrl+L` fuzzy language picker over the whole registry
- [x] Multi-buffer tabs, cursorline highlight, status bar, minibuffer prompts,
  help overlay
- [x] A Visual-Studio-style folder explorer sidebar (`Ctrl+B`; `Ctrl+Shift+O`
  to pick the root) with expand/collapse, hidden-file toggle, and keyboard,
  mouse, and scroll-wheel navigation
- [x] Tab switching (`Ctrl+Tab`, `Ctrl+PageUp`/`PageDown`) and closing, with
  clickable tabs and `✕` close buttons
- [x] Dark/light themes
- [x] Rhai plugin system with a command palette and plugin keybindings
- [x] LSP client: diagnostics, completion, hover, go-to-definition, with
  Helix-derived server detection for 200+ server configs
- [x] WebAssembly bindings + browser frontend (`web/`)
- [x] Native desktop frontend (`fast-text-gui`, egui/eframe)
- [x] TextWrite: a hand-written, dependency-free scripting language with a REPL,
      CLI, closures, collections, and a standard library (`textwrite` / `tw`)
- [x] TextWrite editor scripts: `.tw` files under `plugins/` register commands
      and keybindings, and `Ctrl+E` evaluates a snippet in the running editor
- [x] Scriptable cursor shape (bar / block / underline) from Rhai or TextWrite
- [x] TextWrite `import` modules (built-in packages + `.tw` files) and a
      `fast-text/automator` package for timed editor automations (`Alt+R`)
- [x] Scriptable cursor shape (bar / block / underline), built in on `Alt+1`/`2`/`3`
- [x] Visual-Studio-style plugin manager (`Ctrl+M`): lists every plugin
      with its commands, keybindings, and load errors, and can enable, disable,
      install, or uninstall them
- [x] 189 automated tests (core, syntax, plugins, LSP, wasm, GUI, TUI, TextWrite)

Planned:

- [ ] Config file + keybinding customization
- [ ] Signed releases and prebuilt binaries

## License

MIT
