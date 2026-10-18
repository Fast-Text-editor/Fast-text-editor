# Fast Text

[![CI](https://github.com/Fast-Text-editor/Fast-text-editor/actions/workflows/ci.yml/badge.svg)](https://github.com/Fast-Text-editor/Fast-text-editor/actions/workflows/ci.yml)
[![Release](https://github.com/Fast-Text-editor/Fast-text-editor/actions/workflows/release.yml/badge.svg)](https://github.com/Fast-Text-editor/Fast-text-editor/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.77%2B-orange.svg)](https://www.rust-lang.org)

A fast, modern text editor written in Rust. Fast Text is built as a **Cargo
workspace** with a single powerful engine and thin frontends layered on top, so
the editor behaves identically everywhere.

## Quick start

```sh
curl -fsSL https://raw.githubusercontent.com/Fast-Text-editor/Fast-text-editor/main/install.sh | sh
```

That installs the editor (`fast-text`, `ft`) and the TextWrite language
(`textwrite`, `tw`) — see [Install](#install) for the flags and
[Building and running](#building-and-running) to build from source instead.

## Layout

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
├── install.sh             # universal installer: prebuilt release or source build
└── .github/workflows/     # CI (fmt, clippy, tests) and tagged release binaries
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

## Install

One self-contained shell script installs the editor, the TextWrite language and,
optionally, the desktop app. It uses a prebuilt release when one exists for your
platform and otherwise builds from source with cargo:

```sh
curl -fsSL https://raw.githubusercontent.com/Fast-Text-editor/Fast-text-editor/main/install.sh | sh
```

| Flag | Effect |
| --- | --- |
| `--prefix DIR` | Install under `DIR` (default `$HOME/.local`, or `/usr/local` as root) |
| `--bin-dir DIR` | Put the executables in `DIR` instead of `PREFIX/bin` |
| `--from-source` | Skip the prebuilt release and build with cargo |
| `--source DIR` | Build from the checkout in `DIR` (implies `--from-source`) |
| `--gui` | Also install the desktop app |
| `--version TAG` | Install a specific release tag |
| `--no-completions` | Do not install shell completions |
| `--uninstall` | Remove everything a previous run installed |
| `--check` | Compare the installed version with the newest release |
| `--update` | Install the newest release over the current install |
| `--dry-run` | Print what would happen and change nothing |

It installs `fast-text` / `ft`, `textwrite` / `tw`, and bash and zsh
completions, and records every path in `PREFIX/share/fast-text/installed-files`
so `--uninstall` is exact. When the prefix is not writable the script uses
`sudo` if it is available.

Running it again over an existing installation **upgrades that copy in place**:
it reads the recorded version, keeps the desktop app if you had installed it
before, and installs the newest release — so simply re-running the one-liner
delivers new features to the editor you already have. When neither `--prefix`
nor `--bin-dir` is given, an existing `fast-text` on `PATH` decides where the
upgrade lands, so the copy your shell finds is the one that gets updated. If
you installed with `cargo install` or a package manager instead, `--check`
still finds it through `PATH` and tells you the version it sees.

Prebuilt archives are published for **x86_64 and aarch64 Linux** (glibc) and
**x86_64 and aarch64 macOS**; every other platform builds from source
automatically. Releases are produced by `.github/workflows/release.yml` when a
`v*` tag is pushed.

## Updates

Fast Text knows when a newer release exists. At startup it asks GitHub once a
day (in the background, so it never blocks the editor) and, when something
newer is out, opens a small window naming the version with **Yes, update** and
**Not now** buttons. `Yes` downloads the release, unpacks it, swaps the
executables in place and then offers to restart; the editor itself never
re-execs without asking. `Alt+U` (or *Check for updates* in `Ctrl+P`)
checks on demand, and the version you are running is shown in the status bar.

```toml
[editor]
check_updates = true   # set to false to never check
```

Setting `FAST_TEXT_UPDATE_CHECK=0` disables checking whatever the config says.
A release you answer "Not now" to is not offered again until an even newer one
appears. On platforms without a prebuilt archive the check still runs but
reports that you should rebuild from source.

The installer can do the same from the shell:

```sh
sh install.sh           # notices an existing install and upgrades it
sh install.sh --check     # "Update available: v0.1.0 … v0.2.0"
sh install.sh --update    # install the newest release in place
```

`install.ps1` on Windows takes `-Check` and `-Update`. Both record the
installed version in `PREFIX/share/fast-text/installed-version`, and the
desktop app has the same dialog under **Help → Check for updates**.

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
| `Esc` | Collapse selection | `Alt+M` | Plugin manager |
| `Ctrl+B` | Toggle explorer | `Ctrl+Shift+O` | Open folder |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab | `Alt+1` `Alt+2` `Alt+3` | Cursor bar / block / underline |
| `Ctrl+Shift+F` / `Alt+F` | Find in files | `Ctrl+D` | Select next occurrence |
| `Alt+O` | Document symbols | `Alt+N` | Rename symbol |
| `Ctrl+Shift+I` | Format document | `Ctrl+.` | Code actions |
| `Ctrl+/` | Toggle comment | `Ctrl+Shift+D` | Duplicate line |
| `Ctrl+Shift+K` | Delete line | `Alt+↑` / `Alt+↓` | Move line up / down |
| `Alt+Shift+↑` / `Alt+Shift+↓` | Add cursor above / below | `F6` / `F7` | Record / replay macro | `Alt+U` | Check for updates |

A few chords are terminal-dependent (`Ctrl+Shift+<letter>`, `Ctrl+.`). Every
action also has a palette entry and a `[keys]` id, so you can rebind it — for
example `"alt+shift+f" = "find_in_files"`.

`Shift` + arrows extends the selection; `Ctrl` + left/right moves by word.

`Ctrl+L` opens a fuzzy-filterable picker listing **every supported language**
(300+). Type to narrow, `↑`/`↓` to move, `Enter` to apply.

`Ctrl+B` shows a Visual-Studio-style **folder explorer** along the left edge,
rooted at the folder you launched from (or `Ctrl+Shift+O` to pick another).
`↑`/`↓` move, `→` expands or steps in, `←` collapses or steps out, `Enter`
opens a file, `r` reloads, and `.` toggles dotfiles. With the mouse you can
click a file to open it, click a tab to switch, and click a tab's `✕` to close
it.

`Ctrl+Shift+F` (or `Alt+F`) opens **find in files**: type a query, `Tab` toggles
case sensitivity, and `Enter` on a result jumps straight to the hit. Search runs
over the explorer's current folder.

`Ctrl+D` adds the **next occurrence** of the word under the caret as another
caret, and `Alt+Shift+↑`/`Alt+Shift+↓` add one on the line above/below; typing
then edits every caret at once as a single undo step. `F6` starts recording a
**macro**, `F7` replays the last one, and *Save macro…* names one so you can bind
it (`"alt+k" = "macro_tidy imports"`).

With a language server running (see [Language servers](#language-servers)),
`Ctrl+Shift+I` formats the document, `Ctrl+.` offers code actions, `Alt+N`
renames the symbol under the cursor, and `Alt+O` lists document symbols.

## Configuration

Every key above is rebindable. Settings live in
`$XDG_CONFIG_HOME/fast-text/config.toml` (usually
`~/.config/fast-text/config.toml`); `FAST_TEXT_CONFIG` or `--config PATH`
override that. The file is optional — a missing file just means defaults, and
bad lines are reported in the status bar rather than stopping the editor.

```toml
[editor]
cursor_style = "bar"      # bar | block | underline
theme        = "dark"     # dark | light
sidebar      = false      # start with the folder explorer visible
lsp          = true       # talk to language servers (as FAST_TEXT_LSP)
check_updates = true      # look for a newer release at startup (as Alt+U)

[sidebar]
hidden = false            # list dot-files

[keys]
"f1"     = "help"
"alt+m"  = "manage_plugins"
"ctrl+k" = "cursor_block"
```

Pick *Reload config* from the `Ctrl+P` palette to apply changes without
restarting, or *Open config file*, which opens the file and writes a fully
commented template the first time it is needed.

### Custom keybindings

`[keys]` maps a key spelling to a command. The command is either a built-in id
(listed below) or any command title from the `Ctrl+P` palette, including plugin
and TextWrite commands:

```
save, save_as, open, new, close, quit, undo, redo, select_all,
clear_selection, copy, cut, paste, find, find_next, find_prev, replace,
goto_line, language, command_palette, complete, hover, definition,
eval_textwrite, run_automation, toggle_theme, toggle_sidebar, open_folder,
manage_plugins, install_plugin, cursor_bar, cursor_block, cursor_underline,
help, next_tab, prev_tab, reload_config, open_config,
find_in_files, document_symbols, rename_symbol, format_document,
code_actions, toggle_comment, duplicate_line, delete_line, move_line_up,
move_line_down, select_next_occurrence, record_macro, replay_macro,
save_macro, check_updates
```

A named macro can also be bound with the id `macro_<name>`.

Spellings use the same grammar as plugin bindings — `ctrl+k`, `alt+left`, `f5`,
`shift+enter` — and a user binding takes precedence over every built-in one. A
bare letter with no modifier is refused, because it would stop you typing it.

### Command line

| Option | Effect |
| --- | --- |
| `-h`, `--help` | Print usage |
| `-V`, `--version` | Print the version |
| `-c`, `--config PATH` | Read settings from `PATH` |
| `--` | Treat every later argument as a file name |

`FAST_TEXT_LSP=0` turns language servers off even when `config.toml` asks for
them.

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
script can be written in either. The repository ships several examples under
`plugins/`: `example.rhai`, `example.tw`, `cursor.tw`, plus `text_tools.tw`
(sort/uppercase/number lines), `markdown.tw`, and `notes.tw`. Press **`Ctrl+E`**
to evaluate a TextWrite snippet against the current buffer and see the result in
the status bar.

### The plugin manager

Press **`Alt+M`** (or **`Ctrl+M`**, or pick *Manage plugins* from the `Ctrl+P`
palette) to open a Visual-Studio-style **plugin manager**. It lists every
discovered plugin with its runtime, commands, keybindings, and any load error,
and lets you turn plugins on and off without deleting anything:

> **Tip:** a legacy terminal sends the same byte for `Ctrl+M` and `Enter`, so
> `Ctrl+M` only reaches the editor on terminals that speak the kitty / CSI-u
> keyboard protocol. **`Alt+M`** works everywhere, which is why it is the
> primary binding.

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

## License

MIT
