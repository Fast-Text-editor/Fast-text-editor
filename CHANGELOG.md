# Changelog

All notable changes to Fast Text are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
  `PREFIX/share/fast-text/installed-version`.

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

[Unreleased]: https://github.com/Fast-Text-editor/Fast-text-editor/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Fast-Text-editor/Fast-text-editor/releases/tag/v0.1.0
