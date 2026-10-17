# Contributing to Fast Text

Thanks for taking the time to help. This document covers the essentials; the
[README](README.md) explains what the project is and how it is laid out.

## Getting set up

Fast Text is a plain Cargo workspace — no submodules, no code generation needed
to build:

```sh
git clone https://github.com/Fast-Text-editor/Fast-text-editor.git
cd Fast-text-editor
cargo build            # builds every crate
cargo run -p fast-text-tui -- README.md   # terminal editor
cargo run -p textwrite                    # TextWrite REPL
```

The only optional system packages are for the native GUI (`fast-text-gui`,
egui/eframe). On Debian/Ubuntu:

```sh
sudo apt-get install libxkbcommon-dev libwayland-dev libx11-dev libxi-dev \
    libgl1-mesa-dev libxcursor-dev libxrandr-dev libxinerama-dev
```

## Before you open a pull request

Run the same checks CI runs:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
sh -n install.sh          # if you touched the installer
```

All three must be clean. New behaviour needs tests: the engine
(`fast-text-core`) and the syntax layer (`fast-text-syntax`) are deliberately
UI-free, so most logic can be tested without a terminal.

## Architecture in one minute

- `fast-text-core` — the engine: rope buffer, selections, transactions,
  undo/redo, find/replace. Knows nothing about terminals or windows.
- `fast-text-syntax` — a stateful, dependency-free lexer. Language definitions
  are a data table (`generated.rs` is produced by `tools/gen_langs.py` from a
  Helix checkout — do not hand-edit it).
- `fast-text-plugin` — Rhai plugins; scripts push `EditorAction`s instead of
  touching the buffer, which keeps them decoupled from the editor.
- `fast-text-textwrite` — the same editor API exposed to TextWrite scripts.
- `fast-text-lsp` — a minimal LSP client. Servers run on a background thread.
- `fast-text-tui` / `fast-text-gui` / `fast-text-wasm` — thin frontends.

When you add a feature, prefer putting the logic in a lower crate and keeping
the frontend a thin shell over it.

## Style

- Match the surrounding code; keep comments about *why*, not *what*.
- No new dependencies without a good reason — the editor core, syntax layer and
  the TextWrite language are intentionally dependency-free, and `crossterm`,
  `ratatui`, `ropey` and `rhai` are the only large ones.
- Prefer explaining a non-obvious decision in a short comment over a long one.

## Reporting bugs

Open an issue with your OS, terminal (if it is a TUI problem), the Fast Text
version (`ft --version`), and the smallest reproduction you can manage. A
failing test is the most welcome form a bug report can take.

## License

By contributing you agree that your contributions are licensed under the
[MIT License](LICENSE).
