//! A small but real VT100/xterm terminal emulator for the console panel.
//!
//! `portable-pty` owns the child shell and its pseudo-terminal; this module
//! parses the shell's output with `vte` into a character grid and maps editor
//! key events back to terminal byte sequences. No process is started until the
//! event loop opens the panel and gives us a size, so tests stay hermetic.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthChar;
use vte::{Params, Parser, Perform};

/// One character cell, including the attributes needed to render it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub modifier: Modifier,
}

impl Cell {
    fn new(fg: Color, bg: Color, modifier: Modifier) -> Self {
        Self {
            ch: ' ',
            fg,
            bg,
            modifier,
        }
    }

    pub fn style(&self) -> Style {
        Style::default()
            .fg(self.fg)
            .bg(self.bg)
            .add_modifier(self.modifier)
    }
}

const MAX_SCROLLBACK: usize = 2000;

/// The visible screen plus scrollback, built by feeding PTY bytes to `vte`.
#[derive(Debug)]
pub struct Screen {
    cols: usize,
    rows: usize,
    cells: Vec<Vec<Cell>>,
    scrollback: Vec<Vec<Cell>>,
    /// Column, row of the terminal cursor.
    cursor: (usize, usize),
    saved_cursor: Option<(usize, usize)>,
    fg: Color,
    bg: Color,
    modifier: Modifier,
    wrap_pending: bool,
    top_margin: usize,
    bottom_margin: usize,
}

impl Screen {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1) as usize;
        let rows = rows.max(1) as usize;
        Self {
            cols,
            rows,
            cells: vec![vec![Cell::new(Color::Reset, Color::Reset, Modifier::empty()); cols]; rows],
            scrollback: Vec::new(),
            cursor: (0, 0),
            saved_cursor: None,
            fg: Color::Reset,
            bg: Color::Reset,
            modifier: Modifier::empty(),
            wrap_pending: false,
            top_margin: 0,
            bottom_margin: rows - 1,
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// Resizes the grid, reusing scrollback when the panel grows again.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1) as usize;
        let rows = rows.max(1) as usize;
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        let mut all = std::mem::take(&mut self.scrollback);
        all.extend(std::mem::take(&mut self.cells));
        let kept: Vec<Vec<Cell>> = all
            .into_iter()
            .map(|mut line| {
                line.resize(
                    cols,
                    Cell::new(Color::Reset, Color::Reset, Modifier::empty()),
                );
                line.truncate(cols);
                line
            })
            .collect();
        let start = kept.len().saturating_sub(rows);
        self.scrollback = kept[..start].to_vec();
        if self.scrollback.len() > MAX_SCROLLBACK {
            self.scrollback
                .drain(..self.scrollback.len() - MAX_SCROLLBACK);
        }
        self.cells = kept[start..].to_vec();
        while self.cells.len() < rows {
            self.cells.push(vec![
                Cell::new(Color::Reset, Color::Reset, Modifier::empty());
                cols
            ]);
        }
        self.cols = cols;
        self.rows = rows;
        self.top_margin = 0;
        self.bottom_margin = rows - 1;
        self.cursor.0 = self.cursor.0.min(cols - 1);
        self.cursor.1 = self.cursor.1.min(rows - 1);
        self.wrap_pending = false;
    }

    /// Visible rows, oldest first, including `scroll` lines of history.
    pub fn lines(&self, scroll: usize) -> Vec<&[Cell]> {
        if scroll == 0 {
            return self.cells.iter().map(Vec::as_slice).collect();
        }
        let history = self.scrollback.len().min(scroll);
        let from = self.scrollback.len() - history;
        let mut lines: Vec<&[Cell]> = self.scrollback[from..].iter().map(Vec::as_slice).collect();
        let keep = self.rows.saturating_sub(lines.len());
        lines.extend(self.cells.iter().rev().take(keep).rev().map(Vec::as_slice));
        lines
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    fn blank_line(&self) -> Vec<Cell> {
        vec![Cell::new(Color::Reset, Color::Reset, Modifier::empty()); self.cols]
    }

    fn scroll_up(&mut self) {
        let top = self.cells.remove(self.top_margin);
        if self.scrollback.len() == MAX_SCROLLBACK {
            self.scrollback.remove(0);
        }
        self.scrollback.push(top);
        let blank = self.blank_line();
        self.cells.insert(self.bottom_margin, blank);
    }

    fn newline(&mut self) {
        self.cursor.0 = 0;
        self.wrap_pending = false;
        if self.cursor.1 == self.bottom_margin {
            self.scroll_up();
        } else {
            self.cursor.1 = (self.cursor.1 + 1).min(self.rows - 1);
        }
    }

    fn put_char(&mut self, ch: char) {
        let width = ch.width().unwrap_or(1).max(1);
        if self.wrap_pending {
            self.newline();
        }
        for offset in 0..width {
            if self.cursor.0 >= self.cols {
                self.newline();
            }
            let row = self.cursor.1;
            let col = self.cursor.0;
            self.cells[row][col] = Cell {
                ch: if offset == 0 { ch } else { '\0' },
                fg: self.fg,
                bg: self.bg,
                modifier: self.modifier,
            };
            self.cursor.0 += 1;
        }
        self.wrap_pending = self.cursor.0 >= self.cols;
    }
}

impl Perform for Screen {
    fn print(&mut self, ch: char) {
        self.put_char(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => {
                self.cursor.0 = 0;
                self.wrap_pending = false;
            }
            b'\n' | 0x0b | 0x0c => self.newline(),
            0x08 => {
                self.cursor.0 = self.cursor.0.saturating_sub(1);
                self.wrap_pending = false;
            }
            b'\t' => {
                let next = ((self.cursor.0 / 8) + 1) * 8;
                self.cursor.0 = next.min(self.cols - 1);
                self.wrap_pending = self.cursor.0 >= self.cols;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        if !intermediates.is_empty() {
            return;
        }
        let values: Vec<u16> = params
            .iter()
            .map(|part| part.first().copied().unwrap_or(0))
            .collect();
        let param = |index: usize, default: usize| -> usize {
            values
                .get(index)
                .copied()
                .filter(|value| *value != 0)
                .map(usize::from)
                .unwrap_or(default)
        };
        match action {
            'A' => self.cursor_up(param(0, 1)),
            'B' => self.cursor_down(param(0, 1)),
            'C' => self.cursor_forward(param(0, 1)),
            'D' => self.cursor_back(param(0, 1)),
            'E' => {
                self.cursor_down(param(0, 1));
                self.cursor.0 = 0;
            }
            'F' => {
                self.cursor_up(param(0, 1));
                self.cursor.0 = 0;
            }
            'G' | '`' => self.cursor.0 = param(0, 1).saturating_sub(1).min(self.cols - 1),
            'H' | 'f' => {
                self.cursor.1 = param(0, 1).saturating_sub(1).min(self.rows - 1);
                self.cursor.0 = param(1, 1).saturating_sub(1).min(self.cols - 1);
                self.wrap_pending = false;
            }
            'J' => self.erase_display(param(0, 0)),
            'K' => self.erase_line(param(0, 0)),
            'm' => self.set_attributes(&values),
            'r' => {
                // Scroll regions: most shells never set one, and programs that
                // do still behave acceptably with the full screen as the region.
            }
            's' => self.saved_cursor = Some(self.cursor),
            'u' => {
                if let Some(saved) = self.saved_cursor {
                    self.cursor = saved;
                    self.wrap_pending = false;
                }
            }
            'h' | 'l' => {}
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        if intermediates.is_empty() && byte == b'7' {
            self.saved_cursor = Some(self.cursor);
        } else if intermediates.is_empty() && byte == b'8' {
            if let Some(saved) = self.saved_cursor {
                self.cursor = saved;
            }
        } else if intermediates.is_empty() && byte == b'c' {
            *self = Self::new(self.cols as u16, self.rows as u16);
        }
    }
}

impl Screen {
    fn cursor_up(&mut self, count: usize) {
        self.cursor.1 = self.cursor.1.saturating_sub(count).max(self.top_margin);
        self.wrap_pending = false;
    }

    fn cursor_down(&mut self, count: usize) {
        self.cursor.1 = self.cursor.1.saturating_add(count).min(self.bottom_margin);
        self.wrap_pending = false;
    }

    fn cursor_forward(&mut self, count: usize) {
        self.cursor.0 = self.cursor.0.saturating_add(count).min(self.cols - 1);
        self.wrap_pending = false;
    }

    fn cursor_back(&mut self, count: usize) {
        self.cursor.0 = self.cursor.0.saturating_sub(count);
        self.wrap_pending = false;
    }

    fn erase_line(&mut self, mode: usize) {
        let row = self.cursor.1;
        let (start, end) = match mode {
            0 => (self.cursor.0, self.cols),
            1 => (0, self.cursor.0 + 1),
            2 => (0, self.cols),
            _ => return,
        };
        for col in start.min(self.cols)..end.min(self.cols) {
            self.cells[row][col] = Cell::new(self.fg, self.bg, self.modifier);
        }
    }

    fn erase_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.erase_line(0);
                for row in self.cursor.1 + 1..self.rows {
                    self.cells[row] = self.blank_line();
                }
            }
            1 => {
                for row in 0..self.cursor.1 {
                    self.cells[row] = self.blank_line();
                }
                self.erase_line(1);
            }
            2 | 3 => {
                let blank = self.blank_line();
                for row in &mut self.cells {
                    *row = blank.clone();
                }
                if mode == 3 {
                    self.scrollback.clear();
                }
                self.cursor = (0, 0);
            }
            _ => {}
        }
        self.wrap_pending = false;
    }

    fn set_attributes(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.fg = Color::Reset;
            self.bg = Color::Reset;
            self.modifier = Modifier::empty();
            return;
        }
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => {
                    self.fg = Color::Reset;
                    self.bg = Color::Reset;
                    self.modifier = Modifier::empty();
                }
                1 => self.modifier.insert(Modifier::BOLD),
                2 => self.modifier.insert(Modifier::DIM),
                3 => self.modifier.insert(Modifier::ITALIC),
                4 => self.modifier.insert(Modifier::UNDERLINED),
                5 => self.modifier.insert(Modifier::SLOW_BLINK),
                7 => self.modifier.insert(Modifier::REVERSED),
                8 => self.modifier.insert(Modifier::HIDDEN),
                9 => self.modifier.insert(Modifier::CROSSED_OUT),
                21 | 22 => self.modifier.remove(Modifier::BOLD),
                23 => self.modifier.remove(Modifier::ITALIC),
                24 => self.modifier.remove(Modifier::UNDERLINED),
                27 => self.modifier.remove(Modifier::REVERSED),
                29 => self.modifier.remove(Modifier::CROSSED_OUT),
                39 => self.fg = Color::Reset,
                49 => self.bg = Color::Reset,
                30..=37 => self.fg = ansi_color(params[i] - 30, false),
                40..=47 => self.bg = ansi_color(params[i] - 40, false),
                90..=97 => self.fg = ansi_color(params[i] - 90, true),
                100..=107 => self.bg = ansi_color(params[i] - 100, true),
                38 => {
                    if let Some((color, used)) = extended_color(params, i + 1) {
                        self.fg = color;
                        i += used;
                    }
                }
                48 => {
                    if let Some((color, used)) = extended_color(params, i + 1) {
                        self.bg = color;
                        i += used;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

fn ansi_color(index: u16, bright: bool) -> Color {
    if index >= 8 {
        return Color::Reset;
    }
    Color::Indexed(if bright { index + 8 } else { index } as u8)
}

fn extended_color(params: &[u16], at: usize) -> Option<(Color, usize)> {
    match params.get(at).copied()? {
        5 => Some((
            Color::Indexed(params.get(at + 1).copied().unwrap_or(0) as u8),
            2,
        )),
        2 => {
            let red = params.get(at + 1).copied().unwrap_or(0) as u8;
            let green = params.get(at + 2).copied().unwrap_or(0) as u8;
            let blue = params.get(at + 3).copied().unwrap_or(0) as u8;
            Some((Color::Rgb(red, green, blue), 4))
        }
        _ => None,
    }
}

/// A live shell attached to a pseudo-terminal.
pub struct ShellSession {
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Option<Box<dyn Child + Send>>,
    rx: Receiver<Vec<u8>>,
    parser: Parser,
    pub screen: Screen,
    pub alive: bool,
    size: PtySize,
}

impl std::fmt::Debug for ShellSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellSession")
            .field("alive", &self.alive)
            .field("size", &self.size)
            .finish()
    }
}

impl ShellSession {
    /// Spawns the user's shell in a PTY sized to the console panel.
    ///
    /// This is only called from the event loop. Tests never construct a
    /// `ShellSession`, so they never spawn a process.
    pub fn spawn(cols: u16, rows: u16, cwd: &Path) -> Result<Self, String> {
        let size = PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pty = native_pty_system();
        let pair = pty
            .openpty(size)
            .map_err(|e| format!("cannot open a terminal: {e}"))?;

        let mut command = if cfg!(windows) {
            let shell = std::env::var_os("COMSPEC")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("cmd.exe"));
            CommandBuilder::new(shell)
        } else {
            let shell = std::env::var_os("SHELL")
                .filter(|value| !value.is_empty())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("/bin/sh"));
            let mut command = CommandBuilder::new(shell);
            command.arg("-i");
            command
        };
        command.env("TERM", "xterm-256color");
        command.cwd(cwd);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| format!("cannot start a shell: {e}"))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("cannot read the terminal: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("cannot write to the terminal: {e}"))?;
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if tx.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            master: Some(pair.master),
            writer: Some(writer),
            child: Some(child),
            rx,
            parser: Parser::new(),
            screen: Screen::new(cols, rows),
            alive: true,
            size,
        })
    }

    /// Drains available output and notices when the shell exits.
    pub fn pump(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(bytes) => self.parser.advance(&mut self.screen, &bytes),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive = false;
                    break;
                }
            }
        }
        if self.alive {
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(_)) = child.try_wait() {
                    self.alive = false;
                }
            }
        }
    }

    /// Sends already-encoded key bytes to the shell.
    pub fn write(&mut self, bytes: &[u8]) {
        if !self.alive {
            return;
        }
        if let Some(writer) = self.writer.as_mut() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    /// Keeps the PTY's window size in step with the panel.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if (cols, rows) == (self.size.cols, self.size.rows) {
            return;
        }
        self.size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        if let Some(master) = self.master.as_ref() {
            let _ = master.resize(self.size);
        }
        self.screen.resize(cols, rows);
    }
}

impl Drop for ShellSession {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

/// Encodes a crossterm key for a terminal application.
///
/// Control characters, the escape prefix, cursor keys and function keys all
/// follow the sequences xterm sends, so readline, `vim`, `htop` and friends
/// behave exactly as they do in a standalone terminal.
pub fn key_bytes(key: KeyEvent) -> Vec<u8> {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let mut bytes = match key.code {
        KeyCode::Char(ch) => {
            if ctrl && ch.is_ascii_alphabetic() {
                vec![ch.to_ascii_lowercase() as u8 & 0x1f]
            } else if ctrl && ch == '[' {
                vec![0x1b]
            } else if ctrl && ch == ' ' {
                vec![0x00]
            } else {
                let mut buffer = [0_u8; 4];
                ch.encode_utf8(&mut buffer).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => b"\x1b[Z".to_vec(),
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::F(number) => {
            let code = match number {
                1..=5 => 10 + number,
                6..=10 => 11 + number,
                11 => 23,
                12 => 24,
                _ => return Vec::new(),
            };
            format!("\x1b[{code}~").into_bytes()
        }
        _ => return Vec::new(),
    };
    if alt && !bytes.starts_with(&[0x1b]) {
        let mut prefixed = vec![0x1b];
        prefixed.append(&mut bytes);
        bytes = prefixed;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn feed(screen: &mut Screen, text: &str) {
        let mut parser = Parser::new();
        for byte in text.as_bytes() {
            parser.advance(screen, &[*byte]);
        }
    }

    #[test]
    fn parses_plain_text_and_wraps() {
        let mut screen = Screen::new(4, 2);
        feed(&mut screen, "abcdefg");
        assert_eq!(screen.lines(0)[0][0].ch, 'a');
        assert_eq!(screen.lines(0)[0][3].ch, 'd');
        assert_eq!(screen.lines(0)[1][0].ch, 'e');
        assert_eq!(screen.cursor(), (3, 1));
    }

    #[test]
    fn parses_colours_and_clears() {
        let mut screen = Screen::new(10, 2);
        feed(&mut screen, "\x1b[31;44mX\x1b[0mY");
        assert_eq!(screen.lines(0)[0][0].fg, Color::Indexed(1));
        assert_eq!(screen.lines(0)[0][0].bg, Color::Indexed(4));
        assert_eq!(screen.lines(0)[0][1].fg, Color::Reset);
        feed(&mut screen, "\r\n\x1b[2JZ");
        assert_eq!(screen.lines(0)[0][0].ch, 'Z');
    }

    #[test]
    fn scrolling_saves_history() {
        let mut screen = Screen::new(4, 2);
        feed(&mut screen, "one\r\ntwo\r\nthree");
        // "three" autowraps once at four columns, so two whole rows scroll away.
        assert_eq!(screen.scrollback_len(), 2);
        assert_eq!(screen.lines(0)[0][0].ch, 't');
        assert_eq!(screen.lines(1)[0][0].ch, 't');
        assert_eq!(screen.lines(2)[0][0].ch, 'o');
        assert_eq!(screen.lines(2)[1][0].ch, 't');
    }

    #[test]
    fn resize_preserves_the_bottom_of_the_screen() {
        let mut screen = Screen::new(4, 2);
        feed(&mut screen, "one\r\ntwo");
        screen.resize(2, 2);
        assert_eq!(screen.lines(0)[0][0].ch, 'o');
        assert_eq!(screen.lines(0)[1][0].ch, 't');
    }

    #[test]
    fn encodes_shell_key_sequences() {
        assert_eq!(key_bytes(key(KeyCode::Enter, KeyModifiers::NONE)), b"\r");
        assert_eq!(key_bytes(key(KeyCode::Up, KeyModifiers::NONE)), b"\x1b[A");
        assert_eq!(
            key_bytes(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            b"\x03"
        );
        assert_eq!(
            key_bytes(key(KeyCode::Char('x'), KeyModifiers::ALT)),
            b"\x1bx"
        );
        assert_eq!(key_bytes(key(KeyCode::Tab, KeyModifiers::SHIFT)), b"\x1b[Z");
    }
}
