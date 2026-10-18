# Changelog

All notable changes to Fast Text are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-10-18

### Added

- **Named themes** (core + TUI + GUI): ten palettes ship with the editor —
  `dark`, `light`, `one-dark`, `monokai`, `dracula`, `nord`, `gruvbox`,
  `tokyo-night`, `solarized-dark` and `solarized-light`. Pick one from the
  command palette (*Theme: Monokai*, …), flip between dark and light with
  `Ctrl+T`, or set it in `config.toml` with `theme = "monokai"`. Themes are
  selectable from the manager's catalog too, and scripts can define their own
  with `define_theme(name, colors)` (a map of `0xRRGGBB` colors, missing keys
  fall back to the dark palette) and switch with `set_theme(name)`. The active
  theme name and the full buffer are exposed to scripts as the `theme` and
  `document_text` globals in both Rhai and TextWrite.
- **Installable first-party plugin catalog** (TUI): the plugin manager gains an
  **Available** tab listing six plugins written in TextWrite and compiled into
  the editor — **Theme pack**, **Case and lines**, **Statistics**,
  **Markdown pro**, **Whitespace** and **Snippets**. Highlight one and press
  `Enter` to install it for real: the source is written into the user plugin
  directory and loaded immediately, where it appears under *Installed* like any
  other plugin and can be edited, disabled or deleted afterwards. No network
  access is required. The sources live under
  `crates/fast-text-tui/plugin-catalog/`.
- **Editing ergonomics** (TUI): auto-closing pairs, auto-indent with block
  expansion, `Ctrl+/` toggle comment, `Ctrl+Shift+D` duplicate line,
  `Ctrl+Shift+K` delete line, `Alt+Up`/`Alt+Down` move line, and a
  bracket-match highlight.
- **Multiple cursors** (core + TUI): `Ctrl+D` selects the next occurrence,
  `Alt+Shift+Up`/`Alt+Shift+Down` add a caret above/below, and edits apply to
  every caret as one undoable transaction.
- **Macros** (TUI): `F6` records, `F7` replays, and named macros can be saved
  and bound through the command palette.
- **Find in files** (TUI): a `Ctrl+Shift+F` project search with a filterable
  results picker that jumps to the hit.
- **Language-server commands** (TUI + `fast-text-lsp`): rename, format, code
  actions and document symbols, all applied through workspace edits.
- **More example plugins**: `text_tools.tw`, `markdown.tw`, and `notes.tw`.
- **TextWrite example suite**: `crates/textwrite/examples/` now has a runnable
  program for every area of the language — `basics`, `operators`, `strings`,
  `lists`, `maps`, `control_flow`, `functions`, `math_and_types`, `errors`,
  `modules` (with the `units` module), file I/O, and a `cookbook` of nine small
  complete programs. `tests/language.rs` runs them all, and the language
  reference lists them in reading order.
- **Update notifications** (`fast-text-update`, TUI + GUI): at startup the
  editor asks GitHub for the newest release, throttled to once a day and
  silent when offline, and offers a window naming the version with **Yes,
  update** / **Not now**. Accepting downloads the archive, swaps the
  executables in place and offers a restart. `Alt+U` (or *Check for updates*
  in the palette, or **Help → Check for updates** in the desktop app) checks
  on demand; the running version is shown in the status bar. Controls:
  `[editor] check_updates` and `FAST_TEXT_UPDATE_CHECK=0`.
- **Installer update support**: `install.sh --check` / `--update` and
  `install.ps1 -Check` / `-Update` compare the recorded installation with the
  newest release, and the installed version is kept in
  `PREFIX/share/fast-text/installed-version`. Simply re-running the installer
  detects the existing copy — including one found on `PATH`, when no
  `--prefix`/`-Prefix` is given — and upgrades it in place, keeping the
  desktop app if it was installed before.

### Fixed

- **`words(text)` builtin** (TextWrite): it was documented and listed among the
  builtins but never implemented, so any script that counted words — including
  the shipped `text_tools.tw` plugin and the new **Statistics** catalog plugin
  — failed at runtime with `unknown builtin \`words\``. It now splits text on
  any run of whitespace and drops empty pieces, like `lines` does for lines.

## [0.1.0]

The first public release.

### Added

- **Engine** (`fast-text-core`): a rope-backed buffer, selections with a sticky
  goal column, transaction-grouped undo/redo, and find/replace.
- **Syntax** (`fast-text-syntax`): a dependency-free, stateful highlighter with
  340+ languages — a hand-curated core plus a registry generated from Helix.
- **Terminal editor** (`fast-text-tui`): the `fast-text` / `ft` binaries, with
  tabs, a Visual-Studio-style folder explorer, a plugin manager, themes, a
  command palette and mouse support.
- **Scripting**: Rhai plugins and the TextWrite language, both driving the same
  `EditorAction` API; live editor automations with timed playback.
- **TextWrite** (`textwrite`): a complete, dependency-free scripting language
  with a REPL, a file runner and `tw eval`.
- **Language servers** (`fast-text-lsp`): diagnostics, completion, hover and
  go-to-definition through a background LSP client, with server commands taken
  from the same Helix-derived table.
- **Frontends**: a native desktop window (`fast-text-gui`) and a WebAssembly
  build with a browser shell (`fast-text-wasm` + `web/`).
- **Configuration**: a hand-written `config.toml` reader with custom
  keybindings, and command-line flags (`--config`, `--help`, `--version`).
- **Installer**: a single self-contained `install.sh` that uses a prebuilt
  release when one exists and otherwise builds from source.
- **Releases**: GitHub Actions builds `fast-text-<target>.tar.gz` for Linux and
  macOS (x86_64 and aarch64) on every `v*` tag.

[Unreleased]: https://github.com/Fast-Text-editor/Fast-text-editor/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Fast-Text-editor/Fast-text-editor/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Fast-Text-editor/Fast-text-editor/releases/tag/v0.1.0
