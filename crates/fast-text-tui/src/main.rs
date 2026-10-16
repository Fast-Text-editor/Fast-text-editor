//! `fast-text` — the Fast Text terminal editor.
//!
//! This is the long-form binary; `ft` is an identical short alias.

fn main() -> std::io::Result<()> {
    fast_text_tui::run_cli()
}
