# Plugins, Themes, and Automations

Fast Text supports Rhai plugins, TextWrite scripts, custom themes, and one-shot TextWrite automations.

## Plugin locations

Drop `.rhai` or `.tw` files into:

- `~/.config/fast-text/plugins/` (user plugins)
- `plugins/` in your working directory

User directory wins on name clashes. `Ctrl+Shift+C` lists commands from loaded scripts.

## Plugin manager

Press **`Alt+M`** (or *Manage plugins* in the command palette) to open the plugin manager.

- **Installed**: plugins found on disk, with runtime, commands, keybindings, and load errors. Toggle on/off without deleting.
- **Available**: first-party catalog compiled into the editor. Press `Enter` to install.

| Key | Action |
| --- | --- |
| `↑` / `↓`, `PageUp` / `PageDown`, `Home` / `End` | Move selection |
| `Tab` / `Shift+Tab` | Switch Installed / Available |
| `Space` / `Enter` / `x` | Toggle or install |
| `i` | Install plugin from path |
| `d` | Uninstall (confirm with `y`) |
| `r` | Rescan plugin directories |
| `Esc` / `q` | Close |

Disabled plugins are remembered in `~/.config/fast-text/plugins.disabled`.

> **Tip:** Some legacy terminals send the same byte for `Ctrl+M` and `Enter`. `Alt+M` works everywhere.

## First-party catalog (26 plugins)

All written in TextWrite and readable under `crates/fast-text-tui/plugin-catalog/`.

| Plugin | What it does |
| --- | --- |
| Theme pack | Midnight and Forest palettes |
| Case and lines | Case conversions and line operations |
| Statistics | Word/line/character statistics |
| Markdown pro | Markdown helpers |
| Whitespace | Trim/normalize whitespace |
| Snippets | Small insert snippets |
| Comments | Toggle line comments for the active language |
| CSV tools | Sort, swap, and prune columns |
| Markdown extras | Tables, fences, task lists |
| Date converter | ISO ⇄ US ⇄ long-form dates |
| Identifier cases | camelCase ⇄ snake_case ⇄ kebab-case ⇄ paths |
| Word frequency | Frequency report |
| Box drawing | Single, rounded, double, heavy boxes |
| Align | Align on `=`, `:`, `=>`, or `|` |
| Ciphers | ROT13 and Atbash |
| TODO report | Group TODO/FIXME markers with line numbers |
| Wrap | Reflow to 72/80 columns or unwrap |
| Path tools | Separators, base names, directories, quoting |
| Smart brackets | Auto-close/wrap with `()`, `[]`, `{}`, quotes |
| Markdown preview | Readable text view and heading outline |
| Status formats | Custom status lines with hooks |
| Linked TOC | Markdown table of contents |
| Brace formatter | Indented/one-line JSON/CSS |
| SQL casing | SQL keyword casing |
| Markdown tables | Markdown table helpers |
| Log summary | Summarize log files |

See [`crates/fast-text-tui/plugin-catalog/README.md`](../crates/fast-text-tui/plugin-catalog/README.md).

## Writing scripts

Scripts use the same `EditorAction` queue. Read editor state from globals like
`selected_text`, `document_text`, `has_selection`, `cursor`, `line`, `file_name`,
`language`, `theme`.

Rhai example:

```rhai
fn wrap_in_quotes() {
    if has_selection { insert("\"" + selected_text + "\""); }
}
command_with_desc("Wrap in quotes", "Surround the selection with \"", "wrap_in_quotes");
bind("ctrl+shift+q", "Wrap in quotes");
```

TextWrite example:

```textwrite
fn wrap_in_quotes() {
    if has_selection {
        insert("\"" + selected_text + "\"")
    }
}
command_with_desc("Wrap in quotes", "Surround the selection with \"", "wrap_in_quotes")
bind("ctrl+shift+q", "Wrap in quotes")
```

For the full bridge (context globals, actions, hooks, status line, errors), see
[TextWrite editor scripting guide](scripting.md).

## Themes

Built-in themes: `dark`, `light`, `one-dark`, `monokai`, `dracula`, `nord`,
`gruvbox`, `tokyo-night`, `solarized-dark`, `solarized-light`, `gojo`.

Set in config:

```toml
[editor]
theme = "monokai"
```

Custom themes go in `~/.config/fast-text/themes/*.toml`:

```toml
name = "midnight"
dark = true

[ui]
background = "#10141c"
foreground = "#d7dae0"
status_accent = "#82aaff"

[syntax]
keyword = "#c792ea"
string = "#c3e88d"
```

Colors accept `#RRGGBB`, `0xRRGGBB`, or `#RGB`. Missing keys fall back to `dark`;
unknown keys warn.

Plugins can define themes at runtime:

```textwrite
define_theme("Midnight", { background: 0x0B0E14, keyword: 0xBB9AF7, string: 0x9ECE6A })
fn midnight() { set_theme("Midnight") }
command_with_desc("Theme: Midnight", "Switch to Midnight", "midnight")
```

## Cursor style

Built-in: `Alt+1` bar, `Alt+2` block, `Alt+3` underline. Scripts can call
`set_cursor_style("bar"|"block"|"underline")`.
