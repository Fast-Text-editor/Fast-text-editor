use fast_text_lsp::Severity;
use fast_text_plugin::KeySpec;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Prompt, TabHit};
use crate::plugin_manager::ManagerTab;

const TAB_WIDTH: usize = 4;

/// Color for a diagnostic severity.
fn severity_color(app: &App, severity: Severity) -> Color {
    match severity {
        Severity::Error => app.theme.error,
        Severity::Warning => app.theme.warning,
        Severity::Information | Severity::Hint => app.theme.status_accent,
    }
}

/// Draws the whole application.
pub fn draw(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    // Paint the themed background first.
    frame.buffer_mut().set_style(
        area,
        Style::default()
            .bg(app.theme.background)
            .fg(app.theme.foreground),
    );

    let show_tabs = app.editors.len() > 1;
    let prompt_active = app.prompt.is_active();

    let mut constraints = Vec::new();
    if show_tabs {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(1));
    constraints.push(Constraint::Length(1));
    if prompt_active {
        constraints.push(Constraint::Length(1));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    if show_tabs {
        draw_tabs(app, frame, chunks[idx]);
        idx += 1;
    }
    let content = chunks[idx];
    idx += 1;
    let status = chunks[idx];
    idx += 1;

    // The folder explorer, when visible, claims a column on the left.
    let editor_area = if app.sidebar.visible() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(sidebar_width(content.width)),
                Constraint::Min(1),
            ])
            .split(content);
        draw_sidebar(app, frame, cols[0]);
        cols[1]
    } else {
        content
    };

    app.viewport_rows = editor_area.height;
    app.viewport_cols = editor_area.width.saturating_sub(gutter_width(app));

    draw_content(app, frame, editor_area);
    draw_status(app, frame, status);

    if prompt_active {
        draw_prompt(app, frame, chunks[idx]);
        draw_command_palette(app, frame, chunks[idx]);
        draw_language_palette(app, frame, chunks[idx]);
    }

    draw_completion(app, frame, editor_area);

    if app.picker.is_open() {
        draw_picker(app, frame, area);
    }

    if app.show_help {
        draw_help(app, frame, area);
    }

    if app.plugin_manager.is_open() {
        draw_plugin_manager(app, frame, area);
    }

    // The self-update modal sits on top of everything.
    if app.update.is_dialog_open() {
        draw_update_dialog(app, frame, area);
    }
}

/// Width of the explorer column, kept reasonable on narrow terminals.
fn sidebar_width(width: u16) -> u16 {
    let preferred = (width / 4).clamp(18, 36);
    preferred.min(width.saturating_sub(10))
}

/// Renders the folder explorer tree.
fn draw_sidebar(app: &mut App, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let focused = app.sidebar.focused();
    let border_color = if focused {
        app.theme.status_accent
    } else {
        app.theme.gutter
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {} ", app.sidebar.title()),
            Style::default()
                .fg(app.theme.foreground)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(app.theme.status_bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.sidebar.set_view_height(inner.height as usize);
    app.sidebar.set_area(area, inner);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(error) = app.sidebar.error().map(str::to_owned) {
        frame.render_widget(
            Paragraph::new(Line::from(format!(" {error}")))
                .style(Style::default().fg(app.theme.error).bg(app.theme.status_bg)),
            inner,
        );
        return;
    }

    let selected = app.sidebar.selected();
    let scroll = app.sidebar.scroll();
    let rows = inner.height as usize;
    let mut lines: Vec<Line> = Vec::new();
    for (i, entry) in app
        .sidebar
        .entries()
        .iter()
        .enumerate()
        .skip(scroll)
        .take(rows)
    {
        let is_selected = i == selected;
        let bg = if is_selected {
            app.theme.selection
        } else {
            app.theme.status_bg
        };
        let indent = "  ".repeat(entry.depth);
        let marker = if entry.is_dir {
            if entry.expanded {
                "▾ "
            } else {
                "▸ "
            }
        } else {
            "  "
        };
        let name_style = if entry.is_dir {
            Style::default()
                .fg(app.theme.status_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.foreground)
        };
        let spans = vec![
            Span::styled(
                format!(" {indent}{marker}"),
                Style::default().fg(app.theme.gutter).bg(bg),
            ),
            Span::styled(entry.name.clone(), name_style.bg(bg)),
        ];
        lines.push(Line::from(spans).style(Style::default().bg(bg)));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.status_bg)),
        inner,
    );

    // Point the terminal cursor at the selected row while focused.
    if focused {
        let row = selected.saturating_sub(scroll) as u16;
        if row < inner.height {
            frame.set_cursor_position((inner.x, inner.y + row));
        }
    }
}

/// Renders the completion popup next to the cursor.
fn draw_completion(app: &App, frame: &mut Frame, content: Rect) {
    let Some(state) = &app.completion else {
        return;
    };
    if state.items.is_empty() || content.width == 0 || content.height == 0 {
        return;
    }

    let ed = app.editor();
    let pos = ed.doc.cursor_position();
    let cursor_row = pos.line.saturating_sub(ed.scroll_line) as u16;

    let height = 8u16.min(state.items.len() as u16).min(content.height);
    let mut y = content.y + cursor_row + 1;
    if y + height > content.y + content.height {
        y = (content.y + content.height).saturating_sub(height);
    }
    let width = 48u16.min(content.width);
    let popup = Rect {
        x: content.x,
        y,
        width,
        height,
    };

    let start = state
        .selected
        .saturating_sub((height as usize).saturating_sub(1));
    let lines: Vec<Line> = state
        .items
        .iter()
        .enumerate()
        .skip(start)
        .take(height as usize)
        .map(|(i, item)| {
            let selected = i == state.selected;
            let style = if selected {
                Style::default()
                    .bg(app.theme.selection)
                    .fg(app.theme.foreground)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(app.theme.prompt_bg)
                    .fg(app.theme.foreground)
            };
            let detail = item.detail.as_deref().unwrap_or("");
            let label = format!(" {} {}  {}", item.kind_badge(), item.label, detail);
            Line::from(Span::styled(label, style))
        })
        .collect();

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.prompt_bg)),
        popup,
    );
}

/// Renders the list of matching commands above the minibuffer for `Ctrl+P`.
fn draw_command_palette(app: &App, frame: &mut Frame, prompt_area: Rect) {
    let Prompt::Command { input } = &app.prompt else {
        return;
    };
    let matches = app.command_matches(input);
    if matches.is_empty() {
        return;
    }
    let max_rows = matches.len().min(8) as u16;
    let height = max_rows.min(prompt_area.y);
    if height == 0 {
        return;
    }
    let width = prompt_area.width.min(50);
    let popup = Rect {
        x: prompt_area.x,
        y: prompt_area.y - height,
        width,
        height,
    };

    let lines: Vec<Line> = matches
        .iter()
        .take(height as usize)
        .enumerate()
        .map(|(i, title)| {
            let style = if i == 0 {
                Style::default()
                    .bg(app.theme.selection)
                    .fg(app.theme.foreground)
            } else {
                Style::default()
                    .bg(app.theme.prompt_bg)
                    .fg(app.theme.foreground)
            };
            Line::from(Span::styled(format!(" {title}"), style))
        })
        .collect();

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.prompt_bg)),
        popup,
    );
}

/// Renders the scrollable language picker above the minibuffer (`Ctrl+L`).
fn draw_language_palette(app: &App, frame: &mut Frame, prompt_area: Rect) {
    let Prompt::Language { input, selected } = &app.prompt else {
        return;
    };
    let matches = app.language_matches(input);
    if matches.is_empty() {
        return;
    }
    let max_rows = matches.len().min(12) as u16;
    let height = max_rows.min(prompt_area.y);
    if height == 0 {
        return;
    }
    let width = prompt_area.width.min(50);
    let popup = Rect {
        x: prompt_area.x,
        y: prompt_area.y - height,
        width,
        height,
    };

    // Keep the highlighted row inside the visible window.
    let start = selected.saturating_sub((height as usize).saturating_sub(1));
    let lines: Vec<Line> = matches
        .iter()
        .skip(start)
        .take(height as usize)
        .enumerate()
        .map(|(i, name)| {
            let real = start + i;
            let style = if real == *selected {
                Style::default()
                    .bg(app.theme.selection)
                    .fg(app.theme.foreground)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(app.theme.prompt_bg)
                    .fg(app.theme.foreground)
            };
            Line::from(Span::styled(format!(" {name}"), style))
        })
        .collect();

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.prompt_bg)),
        popup,
    );
}

fn gutter_width(app: &App) -> u16 {
    let lines = app.editor().doc.buffer().len_lines().max(1);
    let digits = lines.to_string().len() as u16;
    // 1 marker column + line number + 1 trailing space.
    digits.max(3) + 2
}

fn draw_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    app.tab_hits.clear();
    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;
    for (i, ed) in app.editors.iter().enumerate() {
        let dirty = if ed.doc.is_dirty() { " ●" } else { "" };
        let label = format!(" {} {}{} ", i + 1, ed.title(), dirty);
        let label_w = UnicodeWidthStr::width(label.as_str()) as u16;
        let style = if i == app.active {
            Style::default()
                .fg(app.theme.tab_active)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.tab_inactive)
        };
        let close_style = if i == app.active {
            Style::default().fg(app.theme.tab_active)
        } else {
            Style::default().fg(app.theme.gutter)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::styled("✕", close_style));

        app.tab_hits.push(TabHit {
            index: i,
            label: Rect::new(x, area.y, label_w, 1),
            close: Rect::new(x + label_w, area.y, 1, 1),
        });
        x += label_w + 1;
        if x < area.x + area.width {
            spans.push(Span::raw(" "));
            x += 1;
        }
        if x >= area.x + area.width {
            break;
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(
            Style::default()
                .bg(app.theme.status_bg)
                .fg(app.theme.status_fg),
        ),
        area,
    );
}

fn draw_content(app: &mut App, frame: &mut Frame, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let gw = gutter_width(app);
    let text_x = area.x + gw;
    let text_width = area.width.saturating_sub(gw) as usize;
    let rows = area.height as usize;

    // --- Scrolling: keep the cursor visible -----------------------------
    let (cursor_line, cursor_col) = {
        let p = app.editor().doc.cursor_position();
        (p.line, p.col)
    };
    let cursor_line_text = app.editor().doc.buffer().line_str(cursor_line);
    let cursor_display = display_col(&cursor_line_text, cursor_col);

    {
        let ed = app.editor_mut();
        if cursor_line < ed.scroll_line {
            ed.scroll_line = cursor_line;
        } else if cursor_line >= ed.scroll_line + rows {
            ed.scroll_line = cursor_line + 1 - rows;
        }
        if cursor_display < ed.scroll_col {
            ed.scroll_col = cursor_display;
        } else if text_width > 0 && cursor_display >= ed.scroll_col + text_width {
            ed.scroll_col = cursor_display + 1 - text_width;
        }
    }

    let top = app.editor().scroll_line;
    let scroll_col = app.editor().scroll_col;
    let sel = app.editor().doc.selection_range();
    let has_sel = app.editor().doc.has_selection();
    // Secondary selections: their ranges paint as selections, and a collapsed
    // caret is drawn as a reversed cell.
    let (extra_ranges, extra_carets) = {
        let doc = &app.editor().doc;
        let mut ranges = Vec::new();
        let mut carets = Vec::new();
        for s in doc.extra_selections() {
            let (start, end) = s.range();
            if start == end {
                let p = doc.position_of(start);
                carets.push((p.line, p.col));
            } else {
                ranges.push((start, end));
            }
        }
        (ranges, carets)
    };

    // Bracket matching: highlight the bracket under the caret and its partner.
    let bracket_marks: Vec<(usize, usize)> = bracket_pair(app)
        .map(|(a, b)| vec![a, b])
        .unwrap_or_default()
        .into_iter()
        .map(|index| {
            let p = app.editor().doc.position_of(index);
            (p.line, p.col)
        })
        .collect();
    // --- Render each visible line ---------------------------------------
    for row in 0..rows {
        let line_idx = top + row;
        let y = area.y + row as u16;
        if line_idx >= app.editor().doc.buffer().len_lines() {
            break;
        }
        let is_cursor_line = line_idx == cursor_line;
        let row_bg = if is_cursor_line {
            app.theme.cursor_line
        } else {
            app.theme.background
        };

        // Fill the row background across the full width.
        for x in area.x..area.x + area.width {
            if let Some(cell) = frame.buffer_mut().cell_mut(Position2::xy(x, y)) {
                cell.set_char(' ');
                cell.set_style(Style::default().bg(row_bg).fg(app.theme.foreground));
            }
        }

        // Gutter: diagnostic marker + line number, right-aligned.
        let diag = app.diagnostic_for_line(line_idx);
        let (marker, marker_style) = match &diag {
            Some(d) => {
                let ch = match d.severity() {
                    Severity::Error => "E",
                    Severity::Warning => "W",
                    Severity::Information | Severity::Hint => "i",
                };
                (
                    ch,
                    Style::default()
                        .fg(severity_color(app, d.severity()))
                        .bg(row_bg)
                        .add_modifier(Modifier::BOLD),
                )
            }
            None => (" ", Style::default().bg(row_bg)),
        };
        frame
            .buffer_mut()
            .set_string(area.x, y, marker, marker_style);

        let num = format!("{:>width$}", line_idx + 1, width = (gw - 2) as usize);
        let num_style = if is_cursor_line {
            Style::default().fg(app.theme.gutter_active).bg(row_bg)
        } else {
            Style::default().fg(app.theme.gutter).bg(row_bg)
        };
        frame.buffer_mut().set_string(area.x + 1, y, num, num_style);

        // Line text + highlights.
        let text = app.editor().doc.buffer().line_str(line_idx);
        let base = app.editor().doc.buffer().char_of_line(line_idx);
        let spans = app.editor_mut().highlights(line_idx);

        render_line_text(
            app,
            frame.buffer_mut(),
            text_x,
            y,
            &text,
            base,
            &spans,
            scroll_col,
            text_width,
            row_bg,
            sel,
            has_sel,
            &extra_ranges,
        );

        // Secondary carets on this line, drawn as reversed cells.
        if !extra_carets.is_empty() {
            let line_text = app.editor().doc.buffer().line_str(line_idx);
            for &(caret_line, caret_col) in &extra_carets {
                if caret_line != line_idx {
                    continue;
                }
                let display = display_col(&line_text, caret_col);
                if display < scroll_col {
                    continue;
                }
                let screen = display - scroll_col;
                if screen >= text_width {
                    continue;
                }
                let x = text_x + screen as u16;
                if let Some(cell) = frame.buffer_mut().cell_mut(Position2::xy(x, y)) {
                    let ch = cell.symbol().chars().next().unwrap_or(' ');
                    cell.set_char(if ch == '\0' { ' ' } else { ch });
                    cell.set_style(
                        Style::default()
                            .bg(app.theme.foreground)
                            .fg(app.theme.background),
                    );
                }
            }
        }

        // Bracket-match highlight for this line, if any.
        for &(bline, bcol) in &bracket_marks {
            if bline != line_idx {
                continue;
            }
            let line_text = app.editor().doc.buffer().line_str(line_idx);
            let display = display_col(&line_text, bcol);
            if display < scroll_col {
                continue;
            }
            let screen = display - scroll_col;
            if screen >= text_width {
                continue;
            }
            let x = text_x + screen as u16;
            if let Some(cell) = frame.buffer_mut().cell_mut(Position2::xy(x, y)) {
                cell.set_style(
                    Style::default()
                        .bg(app.theme.status_accent)
                        .fg(app.theme.background),
                );
            }
        }
    }
    let cursor_row = cursor_line.saturating_sub(top);
    if !app.sidebar.focused() && cursor_line >= top && cursor_line < top + rows {
        let cx = text_x as i64 + cursor_display as i64 - scroll_col as i64;
        if cx >= text_x as i64 {
            frame.set_cursor_position((cx as u16, area.y + cursor_row as u16));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_line_text(
    app: &App,
    buf: &mut Buffer,
    text_x: u16,
    y: u16,
    text: &str,
    line_base: usize,
    spans: &[fast_text_syntax::HighlightSpan],
    scroll_col: usize,
    text_width: usize,
    row_bg: ratatui::style::Color,
    sel: (usize, usize),
    has_sel: bool,
    extra: &[(usize, usize)],
) {
    let theme = &app.theme;
    let chars: Vec<char> = text.chars().collect();
    let mut col = 0usize;

    for (i, &ch) in chars.iter().enumerate() {
        let char_index = line_base + i;

        // Highlight spans are relative to the start of this line.
        let kind = spans
            .iter()
            .find(|s| i >= s.start && i < s.end)
            .map(|s| s.kind)
            .unwrap_or(fast_text_syntax::TokenKind::Plain);

        let selected = (has_sel && char_index >= sel.0 && char_index < sel.1)
            || extra
                .iter()
                .any(|&(s, e)| char_index >= s && char_index < e);

        if ch == '\t' {
            let advance = TAB_WIDTH - (col % TAB_WIDTH);
            for k in 0..advance {
                let abs_col = col + k;
                if abs_col < scroll_col {
                    continue;
                }
                let screen = abs_col - scroll_col;
                if screen >= text_width {
                    return;
                }
                let sel_here = selected;
                let mut style = Style::default().bg(row_bg).patch(theme.token_style(kind));
                if sel_here {
                    style = style.bg(theme.selection);
                }
                if let Some(cell) = buf.cell_mut(Position2::xy(text_x + screen as u16, y)) {
                    cell.set_char(' ');
                    cell.set_style(style);
                }
            }
            col += advance;
            continue;
        }

        let width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);

        if col + width <= scroll_col {
            col += width;
            continue;
        }
        let screen = col as i64 - scroll_col as i64;
        if screen + width as i64 > text_width as i64 {
            break;
        }
        if screen >= 0 {
            let mut style = Style::default().bg(row_bg).patch(theme.token_style(kind));
            if selected {
                style = style.bg(theme.selection);
            }
            if let Some(cell) = buf.cell_mut(Position2::xy(text_x + screen as u16, y)) {
                cell.set_char(ch);
                cell.set_style(style);
            }
            // Blank the continuation cells for wide glyphs.
            for k in 1..width {
                if let Some(cell) =
                    buf.cell_mut(Position2::xy(text_x + (screen as usize + k) as u16, y))
                {
                    cell.set_char(' ');
                    cell.set_style(style);
                }
            }
        }
        col += width;
    }
}

fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    let ed = app.editor();
    let dirty = if ed.doc.is_dirty() { " ●" } else { "" };
    let pos = ed.doc.cursor_position();
    let sel = if ed.doc.has_selection() {
        format!(" ({})", ed.doc.selection().len())
    } else {
        String::new()
    };

    let left = format!(" {}{}  v{}", ed.title(), dirty, env!("CARGO_PKG_VERSION"));
    let lang = format!("{} ", ed.language.name());
    let encoding = if ed.doc.buffer().is_read_only() {
        "RO"
    } else if app.lsp.active_languages().contains(&ed.language) {
        "LSP"
    } else {
        "UTF-8"
    };
    let (right, right_style) = match &app.message {
        Some(msg) => (
            format!(" {msg} "),
            Style::default()
                .bg(app.theme.status_bg)
                .fg(if app.is_error {
                    app.theme.error
                } else {
                    app.theme.warning
                }),
        ),
        None => {
            let diags = app.diagnostics();
            let diag_part = app
                .diagnostics_summary(&diags)
                .map(|s| format!("  ·  {s}"))
                .unwrap_or_default();
            let line_part = app
                .diagnostic_for_line(pos.line)
                .map(|d| format!("  ·  {}", d.summary()))
                .unwrap_or_default();
            (
                format!(
                    "{diag_part}{line_part}  ·  Ln {}, Col {}{}  ·  {} ",
                    pos.line + 1,
                    pos.col + 1,
                    sel,
                    encoding
                ),
                Style::default()
                    .bg(app.theme.status_bg)
                    .fg(app.theme.status_fg),
            )
        }
    };

    let width = area.width as usize;
    let used = left.chars().count() + 1 + lang.chars().count() + right.chars().count();
    let pad = width.saturating_sub(used);

    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .bg(app.theme.status_bg)
                .fg(app.theme.status_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().bg(app.theme.status_bg)),
        Span::styled(
            lang,
            Style::default()
                .bg(app.theme.status_bg)
                .fg(app.theme.status_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " ".repeat(pad),
            Style::default()
                .bg(app.theme.status_bg)
                .fg(app.theme.status_fg),
        ),
        Span::styled(right, right_style),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn draw_prompt(app: &App, frame: &mut Frame, area: Rect) {
    let (label, input) = app.prompt.render().unwrap_or_default();
    let text = format!("{label}{input}");
    let style = Style::default()
        .bg(app.theme.prompt_bg)
        .fg(if app.is_error {
            app.theme.error
        } else {
            app.theme.foreground
        });
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
    // Place the terminal cursor at the end of the input.
    let x = (label.chars().count() + input.chars().count()) as u16;
    if x < area.width {
        frame.set_cursor_position((area.x + x, area.y));
    }
}

fn draw_help(app: &App, frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            " Fast Text — Help ",
            Style::default()
                .fg(app.theme.status_accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(" Ctrl+S  Save          F2      Save as"),
        Line::from(" Ctrl+O  Open file      Ctrl+N  New buffer"),
        Line::from(" Ctrl+W  Close buffer   Ctrl+Q  Quit"),
        Line::from(" Ctrl+Z  Undo          Ctrl+Y  Redo"),
        Line::from(" Ctrl+F  Find          F3/F4   Find next/prev"),
        Line::from(" Ctrl+H  Replace       Ctrl+G  Go to line"),
        Line::from(" Ctrl+L  Language      Ctrl+A  Select all"),
        Line::from(" Ctrl+X/C/V  Cut/Copy/Paste"),
        Line::from(" Ctrl+P  Command palette"),
        Line::from(" Ctrl+E  Eval TextWrite"),
        Line::from(" Ctrl+Space  Complete   F5  Hover   F12  Definition"),
        Line::from(" Ctrl+T  Toggle theme  Esc     Clear selection"),
        Line::from(" Ctrl+B  Explorer      Shift+Ctrl+O  Folder"),
        Line::from(" Ctrl+M / Alt+M  Manage plugins"),
        Line::from(" Ctrl+Tab  Next tab    Ctrl+W  Close tab"),
        Line::from(" Alt+1/2/3  Cursor bar/block/underline"),
        Line::from(" Alt+R  Run automation  Alt+H  Help"),
        Line::from(" Alt+U  Check for updates"),
        Line::from(" Shift+Arrows  Select  Ctrl+←/→  Word move"),
        Line::from(" Alt+↑/↓  Move line   Alt+Shift+↑/↓  Add cursors"),
        Line::from(" Ctrl+D  Select next occurrence"),
        Line::from(" Ctrl+/  Comment   Shift+Ctrl+D  Duplicate line"),
        Line::from(" Shift+Ctrl+K  Delete line  Alt+O  Symbols"),
        Line::from(" Shift+Ctrl+F / Alt+F  Find in files"),
        Line::from(" Alt+N  Rename   Shift+Ctrl+I  Format   Ctrl+.  Code actions"),
        Line::from(" F6  Record macro  F7  Replay macro"),
        Line::from(" Sidebar: ↑↓ ←→ Enter   r reload  . hidden"),
        Line::from(" Mouse: click a file or tab; ✕ closes a tab"),
        Line::from(" Config: $XDG_CONFIG_HOME/fast-text/config.toml — rebind keys here"),
        Line::from(""),
        Line::from(Span::styled(
            " Press any key to close ",
            Style::default().fg(app.theme.gutter),
        )),
    ];

    let popup = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.tab_active))
        .style(Style::default().bg(app.theme.status_bg));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left)
            .style(
                Style::default()
                    .bg(app.theme.status_bg)
                    .fg(app.theme.foreground),
            ),
        popup,
    );
}

/// The self-update modal: offers a new release, shows install progress,
/// offers a restart, or reports a failure.
fn draw_update_dialog(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::app::{UpdateButton, UpdateState};

    let version = env!("CARGO_PKG_VERSION");
    let (title, body, buttons): (
        String,
        Vec<Line<'static>>,
        Vec<(&'static str, UpdateButton)>,
    ) = match app.update.clone() {
        UpdateState::Checking => (
            " Update check ".to_string(),
            vec![
                Line::from(format!(" Checking for a newer Fast Text (v{version})…")),
                Line::from(""),
                Line::from(" Press Esc to keep working; the result appears here."),
            ],
            vec![],
        ),
        UpdateState::Available { tag } => (
            " Update available ".to_string(),
            vec![
                Line::from(format!(" Fast Text {tag} has been released.")),
                Line::from(format!(" You are running v{version}.")),
                Line::from(""),
                Line::from(" Download and install it now?"),
            ],
            vec![
                ("Yes, update", UpdateButton::Yes),
                ("Not now", UpdateButton::No),
            ],
        ),
        UpdateState::Installing { tag, progress } => (
            format!(" Installing {tag} "),
            vec![
                Line::from(format!(" {progress}")),
                Line::from(""),
                Line::from(" Keep this window open until it finishes."),
            ],
            vec![],
        ),
        UpdateState::ReadyToRestart { tag } => (
            " Update installed ".to_string(),
            vec![
                Line::from(format!(" Fast Text {tag} is installed.")),
                Line::from(""),
                Line::from(" Restart now to start using it."),
            ],
            vec![
                ("Restart now", UpdateButton::Restart),
                ("Later", UpdateButton::Later),
            ],
        ),
        UpdateState::Failed { tag, error } => {
            let headline = match &tag {
                Some(tag) => format!(" Fast Text {tag} could not be installed."),
                None => " The update check did not work.".to_string(),
            };
            (
                " Update failed ".to_string(),
                vec![
                    Line::from(headline),
                    Line::from(""),
                    Line::from(format!(" {error}")),
                ],
                vec![("Close", UpdateButton::Close)],
            )
        }
        UpdateState::Idle => return,
    };

    let popup = centered_rect(64, 32, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.status_accent))
        .style(Style::default().bg(app.theme.status_bg))
        .title(Span::styled(
            title,
            Style::default()
                .fg(app.theme.status_accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(body).style(
            Style::default()
                .bg(app.theme.status_bg)
                .fg(app.theme.foreground),
        ),
        rows[0],
    );

    // The button row is redrawn every frame; clicks hit-test against it.
    app.update_hits.clear();
    let row = rows[1];
    let mut x = row.x;
    for (label, button) in buttons {
        let text = format!("[ {label} ]");
        let width = UnicodeWidthStr::width(text.as_str()) as u16;
        if width == 0 || x + width > row.x + row.width {
            break;
        }
        let rect = Rect {
            x,
            y: row.y,
            width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                Style::default()
                    .bg(app.theme.status_bg)
                    .fg(app.theme.status_accent)
                    .add_modifier(Modifier::BOLD),
            ))),
            rect,
        );
        app.update_hits.push((rect, button));
        x += width + 1;
    }
}

/// The centred list overlay: symbols, code actions, find-in-files results.
fn draw_picker(app: &mut App, frame: &mut Frame, area: Rect) {
    let popup = centered_rect(80, 70, area);
    frame.render_widget(Clear, popup);

    let title = app.picker.title().to_string();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.tab_active))
        .style(Style::default().bg(app.theme.status_bg))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(app.theme.status_accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let list_area = rows[0];
    let footer_area = rows[1];

    app.picker.set_view_height(list_area.height as usize);
    app.picker.set_area(popup, list_area);

    let height = list_area.height as usize;
    let start = app.picker.scroll();
    let selected = app.picker.selected();
    let items = app.picker.items();
    let mut lines: Vec<Line> = Vec::new();
    for (index, item) in items.iter().enumerate().skip(start).take(height) {
        let badge = if item.badge.is_empty() {
            String::new()
        } else {
            format!("{} ", item.badge)
        };
        let mut spans = vec![Span::styled(
            format!(" {badge}{}", item.label),
            Style::default().fg(app.theme.foreground),
        )];
        if !item.detail.is_empty() {
            spans.push(Span::styled(
                format!("  {}", item.detail),
                Style::default().fg(app.theme.gutter),
            ));
        }
        let style = if index == selected {
            Style::default()
                .bg(app.theme.selection)
                .fg(app.theme.foreground)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(spans).style(style));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " No matches ",
            Style::default().fg(app.theme.gutter),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.status_bg)),
        list_area,
    );

    let footer = if app.picker.footer().is_empty() {
        " Enter opens · Esc closes ".to_string()
    } else {
        format!(" {} · Enter opens · Esc closes ", app.picker.footer())
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::default()
                .fg(app.theme.gutter)
                .bg(app.theme.status_bg),
        ))),
        footer_area,
    );
}

/// The Visual-Studio-style plugin manager overlay.
fn draw_plugin_manager(app: &mut App, frame: &mut Frame, area: Rect) {
    let popup = centered_rect(92, 84, area);
    frame.render_widget(Clear, popup);

    let total = app.plugin_manager.installed().len();
    let enabled = app.plugin_manager.enabled_count();
    let available = app.plugin_manager.available().len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.tab_active))
        .style(Style::default().bg(app.theme.status_bg))
        .title(Span::styled(
            format!(" Manage Plugins — {enabled}/{total} enabled · {available} to install "),
            Style::default()
                .fg(app.theme.status_accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.width == 0 || inner.height < 4 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let tabs = rows[0];
    let body = rows[1];
    let footer = rows[2];

    draw_plugin_tabs(app, frame, tabs);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(body);
    let list_area = cols[0];
    let detail_area = cols[1];

    app.plugin_manager
        .set_view_height(list_area.height as usize);
    app.plugin_manager.set_area(inner, list_area);

    draw_plugin_list(app, frame, list_area);
    draw_plugin_details(app, frame, detail_area);

    let hint_style = Style::default()
        .fg(app.theme.gutter)
        .bg(app.theme.status_bg);
    let (hint, hint_style) = match app.plugin_manager.pending_delete_name() {
        Some(name) => (
            format!(" Delete {name}?  y = yes, any other key = cancel "),
            Style::default().fg(app.theme.error).bg(app.theme.status_bg),
        ),
        None => match app.plugin_manager.tab() {
            ManagerTab::Installed => (
                " Tab: catalog · Space/Enter toggles · i installs from a path · d deletes · r rescans · Esc closes "
                    .to_string(),
                hint_style,
            ),
            ManagerTab::Available => (
                " Tab: installed · Space/Enter installs the highlighted plugin · r rescans · Esc closes "
                    .to_string(),
                hint_style,
            ),
        },
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, hint_style))),
        footer,
    );
}

/// The Installed / Available tab strip at the top of the manager.
fn draw_plugin_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let manager = &app.plugin_manager;
    let active = manager.tab();
    let mut spans: Vec<Span> = Vec::new();
    for tab in [ManagerTab::Installed, ManagerTab::Available] {
        let count = match tab {
            ManagerTab::Installed => manager.installed().len(),
            ManagerTab::Available => manager.available().len(),
        };
        let style = if tab == active {
            Style::default()
                .bg(app.theme.selection)
                .fg(app.theme.foreground)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.gutter)
        };
        spans.push(Span::styled(format!(" {} ({count}) ", tab.title()), style));
        spans.push(Span::raw(" "));
    }
    let paragraph =
        Paragraph::new(Line::from(spans)).style(Style::default().bg(app.theme.status_bg));
    frame.render_widget(paragraph, area);
}

/// The left-hand list of plugins in the manager overlay.
fn draw_plugin_list(app: &App, frame: &mut Frame, area: Rect) {
    let manager = &app.plugin_manager;
    let height = area.height as usize;
    let scroll = manager.list_scroll();
    let selected = manager.list_selected();
    let mut lines: Vec<Line> = Vec::new();

    match manager.tab() {
        ManagerTab::Installed => {
            let entries = manager.installed();
            let end = (scroll + height).min(entries.len());
            for (index, entry) in entries.iter().enumerate().take(end).skip(scroll) {
                let check = if entry.enabled { "[x]" } else { "[ ]" };
                let mark = if entry.error.is_some() { '!' } else { ' ' };
                let text = format!(" {check}{mark} {}  ({})", entry.name, entry.kind.badge());
                let style = if index == selected {
                    Style::default()
                        .bg(app.theme.selection)
                        .fg(app.theme.foreground)
                        .add_modifier(Modifier::BOLD)
                } else if entry.enabled {
                    Style::default().fg(app.theme.foreground)
                } else {
                    Style::default().fg(app.theme.gutter)
                };
                lines.push(Line::from(Span::styled(text, style)));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    " No plugins installed — press Tab for the catalog ",
                    Style::default().fg(app.theme.gutter),
                )));
            }
        }
        ManagerTab::Available => {
            let available = manager.available();
            let end = (scroll + height).min(available.len());
            for (index, plugin) in available.iter().enumerate().take(end).skip(scroll) {
                let text = format!(" [+] {}  ({})", plugin.title, plugin.file_name);
                let style = if index == selected {
                    Style::default()
                        .bg(app.theme.selection)
                        .fg(app.theme.foreground)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.foreground)
                };
                lines.push(Line::from(Span::styled(text, style)));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    " Everything in the catalog is installed ",
                    Style::default().fg(app.theme.gutter),
                )));
            }
        }
    }
    let paragraph = Paragraph::new(lines).style(Style::default().bg(app.theme.status_bg));
    frame.render_widget(paragraph, area);
}

/// The right-hand details pane of the plugin manager.
fn draw_plugin_details(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let manager = &app.plugin_manager;
    let mut lines: Vec<Line> = Vec::new();

    match manager.tab() {
        ManagerTab::Available => {
            if let Some(plugin) = manager.selected_catalog() {
                lines.push(Line::from(Span::styled(
                    plugin.title.to_string(),
                    Style::default()
                        .fg(theme.status_accent)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(vec![
                    Span::styled("TextWrite", Style::default().fg(theme.tab_active)),
                    Span::raw("  "),
                    Span::styled("not installed", Style::default().fg(theme.warning)),
                ]));
                lines.push(Line::from(Span::styled(
                    plugin.file_name.to_string(),
                    Style::default().fg(theme.gutter),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(plugin.description.to_string()));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Press Enter (or i) to install this plugin.",
                    Style::default().fg(theme.status_accent),
                )));
                if let Some(error) = manager.install_error() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("error: {error}"),
                        Style::default().fg(theme.error),
                    )));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    " Everything in the catalog is installed ",
                    Style::default().fg(theme.gutter),
                )));
            }
        }
        ManagerTab::Installed => {
            if let Some(entry) = manager.selected_entry() {
                lines.push(Line::from(Span::styled(
                    entry.name.clone(),
                    Style::default()
                        .fg(theme.status_accent)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(vec![
                    Span::styled(entry.kind.label(), Style::default().fg(theme.tab_active)),
                    Span::raw("  "),
                    Span::styled(
                        if entry.enabled { "enabled" } else { "disabled" },
                        Style::default().fg(if entry.enabled {
                            theme.foreground
                        } else {
                            theme.gutter
                        }),
                    ),
                ]));
                lines.push(Line::from(Span::styled(
                    entry.path.display().to_string(),
                    Style::default().fg(theme.gutter),
                )));
                if !entry.description.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(entry.description.clone()));
                }
                lines.push(Line::from(""));
                if let Some(error) = &entry.error {
                    lines.push(Line::from(Span::styled(
                        format!("error: {error}"),
                        Style::default().fg(theme.error),
                    )));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    format!("Commands ({})", entry.commands.len()),
                    Style::default().fg(theme.status_accent),
                )));
                for command in &entry.commands {
                    let mut spans = vec![Span::raw(format!("  {}", command.title))];
                    if let Some((key, _)) = entry
                        .keybindings
                        .iter()
                        .find(|(_, title)| title == &command.title)
                    {
                        spans.push(Span::styled(
                            format!("   {}", key.display()),
                            Style::default().fg(theme.status_accent),
                        ));
                    }
                    lines.push(Line::from(spans));
                }
                // Bindings that point at commands the plugin never registered.
                let orphans: Vec<&(KeySpec, String)> = entry
                    .keybindings
                    .iter()
                    .filter(|(_, title)| !entry.commands.iter().any(|c| &c.title == title))
                    .collect();
                if !orphans.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("Keybindings ({})", entry.keybindings.len()),
                        Style::default().fg(theme.status_accent),
                    )));
                    for (key, title) in orphans {
                        lines.push(Line::from(format!("  {}  {title}", key.display())));
                    }
                }
            } else {
                lines.push(Line::from(Span::styled(
                    " No plugins installed ",
                    Style::default().fg(theme.gutter),
                )));
            }
        }
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.status_bg)),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// Display column of `char_col` in `text`, expanding tabs.
fn display_col(text: &str, char_col: usize) -> usize {
    let mut col = 0usize;
    for (i, c) in text.chars().enumerate() {
        if i >= char_col {
            break;
        }
        if c == '\t' {
            col += TAB_WIDTH - (col % TAB_WIDTH);
        } else {
            col += UnicodeWidthChar::width(c).unwrap_or(0).max(1);
        }
    }
    col
}

/// The bracket under (or just before) the caret and its partner, as character
/// indices. Strings and comments are not skipped, which keeps this dependency
/// free and predictable.
fn bracket_pair(app: &App) -> Option<(usize, usize)> {
    let doc = &app.editor().doc;
    let text: Vec<char> = doc.text().chars().collect();
    if text.is_empty() {
        return None;
    }
    let cursor = doc.cursor();
    let idx = if cursor < text.len() && is_bracket(text[cursor]) {
        cursor
    } else if cursor > 0 && is_bracket(text[cursor - 1]) {
        cursor - 1
    } else {
        return None;
    };
    let ch = text[idx];
    let (open, close) = match ch {
        '(' => ('(', ')'),
        '[' => ('[', ']'),
        '{' => ('{', '}'),
        ')' => ('(', ')'),
        ']' => ('[', ']'),
        '}' => ('{', '}'),
        _ => return None,
    };
    if ch == open {
        let mut depth = 0i32;
        for (i, &c) in text.iter().enumerate().skip(idx) {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some((idx, i));
                }
            }
        }
    } else {
        let mut depth = 0i32;
        for i in (0..=idx).rev() {
            let c = text[i];
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
                if depth == 0 {
                    return Some((i, idx));
                }
            }
        }
    }
    None
}

/// Whether `c` is one of the three bracket pairs.
fn is_bracket(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}')
}

/// Small helper so cells can be addressed by (x, y).
struct Position2;
impl Position2 {
    fn xy(x: u16, y: u16) -> Position {
        Position::new(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(app, f)).unwrap();
        format!("{}", terminal.backend())
    }

    #[test]
    fn renders_welcome_text_and_status() {
        let mut app = App::new(vec![]);
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Welcome to Fast Text"), "output was:\n{out}");
        assert!(out.contains("untitled"));
        assert!(out.contains("Ln 1, Col 1"));
    }

    #[test]
    fn renders_the_update_dialog() {
        use crate::app::{UpdateButton, UpdateState};
        let mut app = App::new(vec![]);
        app.update = UpdateState::Available {
            tag: "v9.9.9".to_string(),
        };
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Update available"), "output was:\n{out}");
        assert!(out.contains("v9.9.9"), "output was:\n{out}");
        assert!(out.contains("Yes, update"), "output was:\n{out}");
        // The buttons are recorded as click targets for `App::handle_mouse`.
        assert!(
            app.update_hits
                .iter()
                .any(|(_, button)| *button == UpdateButton::Yes),
            "no Yes button was registered"
        );
    }

    #[test]
    fn the_status_bar_shows_the_version() {
        let mut app = App::new(vec![]);
        let out = render(&mut app, 80, 24);
        assert!(
            out.contains(env!("CARGO_PKG_VERSION")),
            "output was:\n{out}"
        );
    }

    #[test]
    fn renders_line_numbers_in_gutter() {
        let mut app = App::new(vec![]);
        let out = render(&mut app, 80, 24);
        // Welcome buffer has several lines; the gutter shows "1".."n".
        for n in 1..=6 {
            assert!(out.contains(&n.to_string()), "missing line {n} in:\n{out}");
        }
    }

    #[test]
    fn renders_typed_text_after_input() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(vec![]);
        // Select all, delete, then type.
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        for c in "hello".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let out = render(&mut app, 80, 24);
        assert!(out.contains("hello"), "output was:\n{out}");
        // Dirty marker should be present in the status/title.
        assert!(out.contains("●"));
    }

    #[test]
    fn help_overlay_renders() {
        let mut app = App::new(vec![]);
        app.show_help = true;
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Help"));
        assert!(out.contains("Ctrl+S"));
    }

    #[test]
    fn plugin_manager_overlay_renders() {
        let dir = std::env::temp_dir().join(format!("ft-plugins-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("demo.tw"),
            "# A demo plugin.\nfn tick() { insert(\"TW\") }\ncommand_with_desc(\"TextWrite tick\", \"tick\", \"tick\")\nbind(\"alt+9\", \"TextWrite tick\")\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("demo.rhai"),
            "// A Rhai demo.\nfn hi() {}\ncommand(\"Hi\", \"hi\");\nbind(\"alt+8\", \"Hi\");\n",
        )
        .unwrap();

        let mut app = App::new(vec![]);
        app.plugin_manager =
            crate::plugin_manager::PluginManager::discover(vec![dir.clone()], None);
        app.plugin_manager.open();
        let out = render(&mut app, 100, 30);
        assert!(out.contains("Manage Plugins"), "overlay missing:\n{out}");
        assert!(out.contains("demo.tw"), "tw plugin missing:\n{out}");
        assert!(out.contains("demo.rhai"), "rhai plugin missing:\n{out}");
        // The details pane describes the first entry (sorted: demo.rhai).
        assert!(out.contains("Rhai"), "kind missing:\n{out}");
        assert!(out.contains("Hi"), "command missing:\n{out}");
        assert!(out.contains("Alt+8"), "keybinding missing:\n{out}");

        // `d` asks for confirmation in the footer.
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        app.plugin_manager
            .handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        let out = render(&mut app, 100, 30);
        assert!(out.contains("Delete demo.rhai?"), "confirm missing:\n{out}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidebar_renders_the_folder_tree() {
        let dir = std::env::temp_dir().join(format!("ft-explorer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();

        let mut app = App::new(vec![]);
        app.sidebar.set_root(dir.clone());
        app.sidebar.show();
        let out = render(&mut app, 80, 24);
        assert!(out.contains("main.rs"), "tree not drawn:\n{out}");
        assert!(out.contains("src"), "directories not drawn:\n{out}");
        assert!(out.contains("ft-explorer-"), "folder title missing:\n{out}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn language_picker_overlay_renders_matches() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(vec![]);
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
        for c in "zig".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Language:"), "output was:\n{out}");
        assert!(out.contains("Zig"), "output was:\n{out}");
    }

    /// Every foreground color present in the rendered buffer.
    fn fg_colors(app: &mut App, w: u16, h: u16) -> Vec<Color> {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(app, f)).unwrap();
        let buf = terminal.backend().buffer();
        let area = *buf.area();
        let mut out = Vec::new();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell((x, y)) {
                    out.push(cell.fg);
                }
            }
        }
        out
    }

    #[test]
    fn highlights_tokens_on_lines_after_the_first() {
        // Regression: highlight spans are line-relative, so a token on line 3
        // must still be colored (previously only line 1 was).
        let mut app = App::new(vec![]);
        {
            let ed = app.editor_mut();
            ed.doc = fast_text_core::Document::from_str("plain\nplain\n// a comment\n");
            ed.language = fast_text_syntax::LanguageId::Rust;
        }
        let expected = app.theme.comment.fg.expect("comment has a color");
        let colors = fg_colors(&mut app, 80, 24);
        assert!(
            colors.contains(&expected),
            "comment color {expected:?} missing — tokens after line 1 are not highlighted"
        );
    }
}
