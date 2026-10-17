//! A generic, centred list overlay.
//!
//! One small widget backs several features that all boil down to "show a list,
//! pick a row": document symbols, code actions and find-in-files results. The
//! overlay owns only presentation and selection state; the app decides what a
//! chosen row *means* (it keeps the payload alongside the picker).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

/// Assumed list height until the first draw reports the real one.
const DEFAULT_VIEW_HEIGHT: usize = 16;

/// One row of a picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    /// The main text of the row.
    pub label: String,
    /// A dim suffix, e.g. a file:line location.
    pub detail: String,
    /// A short badge drawn before the label (may be empty).
    pub badge: String,
}

impl PickerItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: String::new(),
            badge: String::new(),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = badge.into();
        self
    }
}

/// What a key press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerOutcome {
    /// The key was consumed; keep the picker open.
    Handled,
    /// The picker should close (Esc, or an accepted row).
    Close,
    /// Row `index` was accepted with Enter.
    Chosen(usize),
}

/// A keyboard-driven list overlay.
pub struct Picker {
    title: String,
    items: Vec<PickerItem>,
    selected: usize,
    scroll: usize,
    view_height: usize,
    open: bool,
    footer: String,
    /// Screen rectangles, recorded while drawing (for mouse support).
    outer_area: Rect,
    list_area: Rect,
}

impl Default for Picker {
    fn default() -> Self {
        Self::new()
    }
}

impl Picker {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            items: Vec::new(),
            selected: 0,
            scroll: 0,
            view_height: DEFAULT_VIEW_HEIGHT,
            open: false,
            footer: String::new(),
            outer_area: Rect::default(),
            list_area: Rect::default(),
        }
    }

    /// Opens the picker with a fresh list.
    pub fn open(&mut self, title: impl Into<String>, items: Vec<PickerItem>) {
        self.title = title.into();
        self.items = items;
        self.selected = 0;
        self.scroll = 0;
        self.footer.clear();
        self.open = true;
    }

    /// Replaces the header, keeping the picker open.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Replaces the rows, keeping the picker open (used for live filtering).
    pub fn set_items(&mut self, items: Vec<PickerItem>) {
        self.items = items;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.clamp_scroll();
    }

    pub fn set_footer(&mut self, footer: impl Into<String>) {
        self.footer = footer.into();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.items.clear();
        self.selected = 0;
        self.scroll = 0;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn footer(&self) -> &str {
        &self.footer
    }

    pub fn items(&self) -> &[PickerItem] {
        &self.items
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The currently highlighted row, if any.
    pub fn selected_item(&self) -> Option<&PickerItem> {
        self.items.get(self.selected)
    }

    pub fn set_view_height(&mut self, height: usize) {
        self.view_height = height.max(1);
        self.clamp_scroll();
    }

    /// Records the on-screen rectangles while drawing.
    pub fn set_area(&mut self, outer: Rect, list: Rect) {
        self.outer_area = outer;
        self.list_area = list;
    }

    /// Whether a screen cell is inside the overlay, border included.
    pub fn contains(&self, column: u16, row: u16) -> bool {
        rect_contains(self.outer_area, column, row)
    }

    /// The row index drawn at a screen cell, if any.
    pub fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        if !rect_contains(self.list_area, column, row) {
            return None;
        }
        let index = self.scroll + (row - self.list_area.y) as usize;
        (index < self.items.len()).then_some(index)
    }

    /// Handles one key press.
    pub fn handle_key(&mut self, key: KeyEvent) -> PickerOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.view_height.max(1);
        match key.code {
            KeyCode::Esc => return PickerOutcome::Close,
            KeyCode::Enter => {
                return match self.items.get(self.selected) {
                    Some(_) => PickerOutcome::Chosen(self.selected),
                    None => PickerOutcome::Handled,
                }
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Home => self.select(0),
            KeyCode::End => self.select(self.items.len().saturating_sub(1)),
            KeyCode::PageUp => self.move_selection(-(page as isize)),
            KeyCode::PageDown => self.move_selection(page as isize),
            KeyCode::Char('p') if ctrl => self.move_selection(-1),
            KeyCode::Char('n') if ctrl => self.move_selection(1),
            _ => {}
        }
        PickerOutcome::Handled
    }

    /// Selects a row by index (used by mouse clicks).
    pub fn select_index(&mut self, index: usize) {
        self.select(index);
    }

    fn select(&mut self, index: usize) {
        if self.items.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = index.min(self.items.len() - 1);
        self.clamp_scroll();
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        let next = (self.selected as isize + delta).clamp(0, len as isize - 1) as usize;
        self.selected = next;
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + self.view_height {
            self.scroll = self.selected + 1 - self.view_height.max(1);
        }
        let max = self.items.len().saturating_sub(self.view_height);
        self.scroll = self.scroll.min(max);
    }
}

/// Whether a cell lies inside a rectangle (empty rectangles contain nothing).
fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && column >= rect.x
        && column < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn items(n: usize) -> Vec<PickerItem> {
        (0..n)
            .map(|i| PickerItem::new(format!("item {i}")))
            .collect()
    }

    #[test]
    fn opens_with_the_first_row_selected() {
        let mut picker = Picker::new();
        picker.open("Title", items(3));
        assert!(picker.is_open());
        assert_eq!(picker.selected(), 0);
        assert_eq!(picker.title(), "Title");
        assert_eq!(picker.items().len(), 3);
    }

    #[test]
    fn arrows_and_page_keys_move_within_bounds() {
        let mut picker = Picker::new();
        picker.open("x", items(20));
        picker.set_view_height(5);

        picker.handle_key(key(KeyCode::Down));
        assert_eq!(picker.selected(), 1);
        picker.handle_key(key(KeyCode::Up));
        picker.handle_key(key(KeyCode::Up));
        assert_eq!(picker.selected(), 0, "clamped at the top");

        picker.handle_key(key(KeyCode::PageDown));
        assert_eq!(picker.selected(), 5);
        picker.handle_key(key(KeyCode::End));
        assert_eq!(picker.selected(), 19);
    }

    #[test]
    fn ctrl_p_and_ctrl_n_move_like_arrows() {
        let mut picker = Picker::new();
        picker.open("x", items(3));
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        picker.handle_key(ctrl('n'));
        assert_eq!(picker.selected(), 1);
        picker.handle_key(ctrl('p'));
        assert_eq!(picker.selected(), 0);
    }

    #[test]
    fn enter_chooses_the_highlighted_row_and_escape_closes() {
        let mut picker = Picker::new();
        picker.open("x", items(3));
        picker.handle_key(key(KeyCode::Down));
        assert_eq!(
            picker.handle_key(key(KeyCode::Enter)),
            PickerOutcome::Chosen(1)
        );
        assert_eq!(picker.handle_key(key(KeyCode::Esc)), PickerOutcome::Close);
    }

    #[test]
    fn an_empty_picker_ignores_enter() {
        let mut picker = Picker::new();
        picker.open("x", Vec::new());
        assert_eq!(
            picker.handle_key(key(KeyCode::Enter)),
            PickerOutcome::Handled
        );
    }

    #[test]
    fn filtering_keeps_the_selection_in_range() {
        let mut picker = Picker::new();
        picker.open("x", items(10));
        picker.handle_key(key(KeyCode::End));
        picker.set_items(items(2));
        assert_eq!(picker.selected(), 1);
    }

    #[test]
    fn row_at_maps_screen_cells() {
        let mut picker = Picker::new();
        picker.open("x", items(5));
        picker.set_area(Rect::new(0, 0, 40, 10), Rect::new(1, 2, 38, 6));
        assert!(picker.contains(0, 0));
        assert_eq!(picker.row_at(2, 2), Some(0));
        assert_eq!(picker.row_at(2, 4), Some(2));
        assert_eq!(picker.row_at(0, 2), None, "the border column");
    }
}
