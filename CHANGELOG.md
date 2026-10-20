# Changelog

All notable changes to Fast Text are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-10-20

### Added

- **Top bar** (TUI): a nano-style bar across the top of the window showing
  `fast-text v<version>` in the accent colour, so the running build is always
  visible.
- **Console panel** (TUI): `Alt+C` toggles a VS Code-style console at the
  bottom of the window. Type any command and press `Enter` to run it with
  `sh -c`; stdout, stderr and a non-zero exit code are captured under the
  prompt and up to 1000 lines are kept. `Esc` or `Alt+C` closes it; the panel
  takes keys from the editor only while it is open, and every modal (update
  dialog, help, plugin manager, picker, prompt) still outranks it.
  (The old `Alt+C` *toggle comment* binding moved — commenting stays on
  `Ctrl+/`.)
- **New file / new folder in the explorer** (TUI): with the folder explorer
  open, `n` starts an inline prompt for a **new file** and `N` for a **new
  folder** next to the highlighted row. Names must be a single path component
  (no `/`, no `.`/`..`), existing names are rejected with an error, and a file
  created this way opens immediately. `Esc` cancels.
- **Six new catalog plugins**, written in TextWrite and bundled with the
  editor alongside the first six — the plugin manager's *Available* tab now
  lists twelve: **Comments** (`comments.tw`), **CSV tools** (`csv_tools.tw`),
  **Markdown extras** (`markdown_extras.tw`), **Date converter**
  (`date_time.tw`), **Identifier cases** (`identifier_case.tw`) and **Word
  frequency** (`word_frequency.tw`).
- Installer CI coverage for the fixes below: git-less/cargo-less dry runs for
  both installers, pinned-version records, GUI retention on pinned updates and
  uninstall cleanup.

### Fixed

- `install.sh`: `Ctrl-C` during an install cleaned up the temp directory but
  did **not** stop the script — the traps now exit (`130`/`143`) and leave the
  cleanup to `EXIT`.
- `install.sh`: `--dry-run` demanded `git`, `cargo` and a real checkout before
  reaching its own dry-run guard, so it died on exactly the machines it is for;
  it now prints the whole plan instead.
- `install.sh`: a pinned `--version vX.Y.Z` install whose binary probe failed
  recorded the *newest* release, so a later `--check`/`--update` wrongly said
  "up to date"; it now records the version it installed.
- `install.sh`: `--uninstall` left behind an `installed-version` file that
  older manifests never listed, plus the emptied `share/fast-text` directory.
- `install.ps1`: `-DryRun` from a non-checkout actually **cloned the repo**
  (and required `git`/`cargo`); the clone, the tool checks and the checkout
  checks are now skipped under `-DryRun`.
- `install.ps1`: `-Update -Version vX.Y.Z` silently dropped a previously
  installed GUI app that `install.sh` kept — it now consults the manifest like
  the shell installer does.
- `install.ps1`: `-Version` could never be seen by `Write-VersionFile` — the
  local `$version = $null` wiped the script's `$Version` parameter (PowerShell
  names are case-insensitive), so pinned installs recorded the wrong version.

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

[Unreleased]: https://github.com/Fast-Text-editor/Fast-text-editor/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/Fast-Text-editor/Fast-text-editor/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Fast-Text-editor/Fast-text-editor/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Fast-Text-editor/Fast-text-editor/releases/tag/v0.1.0
