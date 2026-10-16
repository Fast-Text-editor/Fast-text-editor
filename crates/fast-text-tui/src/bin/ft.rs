//! `ft` — a short alias for the Fast Text terminal editor.
//!
//! Identical to the `fast-text` binary; handy for the command line.

fn main() -> std::io::Result<()> {
    fast_text_tui::run_cli()
}
