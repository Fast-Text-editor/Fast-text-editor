# Plugin catalog

The first-party plugins Fast Text ships and can install for you.

These are plain [TextWrite](../../textwrite/README.md) scripts. The editor
embeds each one with `include_str!` (see `src/catalog.rs`), so the plugin
manager can list them under **Available** and install them for real with no
network access. Installing writes the file into your plugin directory
(`~/.config/fast-text/plugins/`), where it is discovered and loaded like any
other `.tw` file — you can edit, disable or delete it afterwards.

Open the manager with `Alt+M`, press `Tab` to reach the *Available* list,
highlight a plugin and press `Enter`.

| Plugin | What it adds |
| --- | --- |
| `themes.tw` | Every built-in theme as a command, two extra palettes (`Midnight`, `Forest`) defined in TextWrite, and `Alt+T` to cycle |
| `case_and_lines.tw` | Upper/lower/title case, plus sort, reverse, de-duplicate and number the selected lines |
| `stats.tw` | Word, character and line counts for the selection or document, with a reading-time estimate |
| `markdown_pro.tw` | Headings, a table skeleton, fenced code, links, quotes and thematic breaks |
| `whitespace.tw` | Trim trailing spaces, tabs ↔ spaces, collapse runs of blank lines |
| `snippets.tw` | Section banners, dividers, note blocks, TODO markers and checkboxes |
| `comments.tw` | Toggle line comments for over thirty languages, wrap blocks, build commented banners |
| `csv_tools.tw` | Pad columns into alignment, sort by a column, count fields, swap columns, render a bordered table |
| `markdown_extras.tw` | Task lists, footnotes, definition lists, reference links, strikethrough, a generated table of contents |
| `date_time.tw` | Read a date from the selection and rewrite it as ISO, long, short, slash or compact form, with the weekday |
| `identifier_case.tw` | Convert names between snake, camel, Pascal, kebab, CONSTANT and dot.case, or split them into words |
| `word_frequency.tw` | Count the selection's words, rank them, chart them as a comment block, estimate reading time |
| `boxes.tw` | Surround the selection with a single, rounded, double or heavy Unicode box |
| `align.tw` | Line the selected rows up on their first `=`, `:`, `=>` or `|` |
| `cipher.tw` | ROT13 and Atbash over the selection, preserving case |
| `todo_report.tw` | Group every TODO/FIXME/HACK/XXX/NOTE in the document into a report with line numbers |
| `wrap.tw` | Reflow the selection to 72 or 80 columns, or unwrap paragraphs |
| `path_tools.tw` | Switch path separators, keep base names or directories, quote or unquote paths |
| `brackets.tw` | Auto-close/wrap `()`, `[]`, `{}` and quotes, stepping over an existing closer |
| `markdown_preview.tw` | Toggle a Markdown buffer into readable text and back; insert a heading outline |
| `status_line.tw` | Four custom status-line formats with `on_buffer_open` / `on_buffer_save` hooks |
| `toc.tw` | Insert a linked Markdown table of contents with GitHub-style anchors |
| `brace_format.tw` | Indent or compact JSON/CSS-style text without touching string contents |
| `sql_case.tw` | Upper- or lower-case SQL keywords, leaving identifiers and strings alone |
| `markdown_tables.tw` | Build an aligned Markdown table from TSV or CSV, or re-align a pipe table |
| `log_summary.tw` | Count log lines by severity with a sample, or keep only ERROR lines |
| `lorem.tw` | Insert placeholder sentences, paragraphs, lists and titles |
| `slugify.tw` | Convert text to URL slugs and filename-safe fragments |
| `escape.tw` | Escape, unescape and quote strings with JSON-style backslashes |
| `extract.tw` | Extract emails, URLs, numbers and unique lines from text |
| `number_base.tw` | Convert integers between decimal, hexadecimal and binary |
| `checklist.tw` | Toggle Markdown checkboxes, add or remove bullets, and indent/outdent lines |

## Writing your own

Any `.tw` file works. Register commands with `command` / `command_with_desc`,
optional keys with `bind`, and act through the editor API — see the
[TextWrite README](../../textwrite/README.md) and the scripts here for the full
set of functions and globals. To offer a plugin from this catalog, add a file
and one entry to `src/catalog.rs`.
