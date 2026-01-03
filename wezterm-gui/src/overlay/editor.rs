use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{InputText, KeyAssignment};
use config::{configuration, AnsiColor, ColorAttribute};
use luahelper::impl_lua_conversion_dynamic;
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use std::rc::Rc;
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::surface::{Change, CursorShape, CursorVisibility, Position};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use wezterm_dynamic::{FromDynamic, ToDynamic};
use wezterm_term::{AttributeChange, CellAttributes, Intensity};

struct EditorColors {
    text_fg: ColorAttribute,
    line_number_fg: ColorAttribute,
    status_fg: ColorAttribute,
    status_bg: ColorAttribute,
    normal_mode_fg: ColorAttribute,
    normal_mode_bg: ColorAttribute,
    insert_mode_fg: ColorAttribute,
    insert_mode_bg: ColorAttribute,
    replace_mode_fg: ColorAttribute,
    replace_mode_bg: ColorAttribute,
    command_mode_fg: ColorAttribute,
    command_mode_bg: ColorAttribute,
    visual_mode_fg: ColorAttribute,
    visual_mode_bg: ColorAttribute,
    selection_bg: ColorAttribute,
    selection_fg: ColorAttribute,
    search_match_bg: ColorAttribute,
    search_match_fg: ColorAttribute,
    search_current_match_bg: ColorAttribute,
    search_current_match_fg: ColorAttribute,
    normal_mode_text: String,
    insert_mode_text: String,
    replace_mode_text: String,
    command_mode_text: String,
    visual_mode_text: String,
    visual_line_mode_text: String,
    visual_block_mode_text: String,
}

impl EditorColors {
    fn new() -> Self {
        let config = configuration();
        let colors = &config.resolved_palette;

        Self {
            text_fg: colors.foreground.map_or(ColorAttribute::Default, |c| {
                ColorAttribute::TrueColorWithDefaultFallback(c.into())
            }),
            line_number_fg: colors
                .input_text_line_number_fg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::Grey.into()), |c| {
                    c.into()
                }),
            status_fg: colors.input_text_status_fg.map_or_else(
                || {
                    colors.background.map_or(ColorAttribute::Default, |c| {
                        ColorAttribute::TrueColorWithDefaultFallback(c.into())
                    })
                },
                |c| c.into(),
            ),
            status_bg: colors.input_text_status_bg.map_or(
                ColorAttribute::PaletteIndex(AnsiColor::Purple.into()),
                |c| c.into(),
            ),
            normal_mode_fg: colors.input_text_normal_mode_fg.map_or_else(
                || {
                    colors.background.map_or(ColorAttribute::Default, |c| {
                        ColorAttribute::TrueColorWithDefaultFallback(c.into())
                    })
                },
                |c| c.into(),
            ),
            normal_mode_bg: colors
                .input_text_normal_mode_bg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::Green.into()), |c| {
                    c.into()
                }),
            insert_mode_fg: colors.input_text_insert_mode_fg.map_or_else(
                || {
                    colors.background.map_or(ColorAttribute::Default, |c| {
                        ColorAttribute::TrueColorWithDefaultFallback(c.into())
                    })
                },
                |c| c.into(),
            ),
            insert_mode_bg: colors
                .input_text_insert_mode_bg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::Blue.into()), |c| {
                    c.into()
                }),
            replace_mode_fg: colors.input_text_replace_mode_fg.map_or_else(
                || {
                    colors.background.map_or(ColorAttribute::Default, |c| {
                        ColorAttribute::TrueColorWithDefaultFallback(c.into())
                    })
                },
                |c| c.into(),
            ),
            replace_mode_bg: colors
                .input_text_replace_mode_bg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::Red.into()), |c| {
                    c.into()
                }),
            command_mode_fg: colors.input_text_command_mode_fg.map_or_else(
                || {
                    colors.background.map_or(ColorAttribute::Default, |c| {
                        ColorAttribute::TrueColorWithDefaultFallback(c.into())
                    })
                },
                |c| c.into(),
            ),
            command_mode_bg: colors.input_text_command_mode_bg.map_or(
                ColorAttribute::PaletteIndex(AnsiColor::Yellow.into()),
                |c| c.into(),
            ),
            visual_mode_fg: colors.input_text_visual_mode_fg.map_or_else(
                || {
                    colors.background.map_or(ColorAttribute::Default, |c| {
                        ColorAttribute::TrueColorWithDefaultFallback(c.into())
                    })
                },
                |c| c.into(),
            ),
            visual_mode_bg: colors.input_text_visual_mode_bg.map_or(
                ColorAttribute::PaletteIndex(AnsiColor::Purple.into()),
                |c| c.into(),
            ),
            selection_bg: colors
                .selection_bg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::Navy.into()), |c| {
                    ColorAttribute::TrueColorWithDefaultFallback(c.into())
                }),
            selection_fg: colors
                .selection_fg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::White.into()), |c| {
                    ColorAttribute::TrueColorWithDefaultFallback(c.into())
                }),
            search_match_fg: colors
                .input_text_search_match_fg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::Black.into()), |c| {
                    c.into()
                }),
            search_match_bg: colors.input_text_search_match_bg.map_or(
                ColorAttribute::PaletteIndex(AnsiColor::Yellow.into()),
                |c| c.into(),
            ),
            search_current_match_fg: colors
                .input_text_search_current_match_fg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::White.into()), |c| {
                    c.into()
                }),
            search_current_match_bg: colors
                .input_text_search_current_match_bg
                .map_or(ColorAttribute::PaletteIndex(AnsiColor::Navy.into()), |c| {
                    c.into()
                }),
            normal_mode_text: config.input_text_normal_mode_text.clone(),
            insert_mode_text: config.input_text_insert_mode_text.clone(),
            replace_mode_text: config.input_text_replace_mode_text.clone(),
            command_mode_text: config.input_text_command_mode_text.clone(),
            visual_mode_text: config.input_text_visual_mode_text.clone(),
            visual_line_mode_text: config.input_text_visual_line_mode_text.clone(),
            visual_block_mode_text: config.input_text_visual_block_mode_text.clone(),
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum EditorMode {
    Normal,
    Insert,
    Replace,
    Search,
    Visual,
    VisualLine,
    VisualBlock,
}

/// Direction for search and motion operations
#[derive(Clone, Copy, Debug, PartialEq)]
enum Direction {
    Forward,
    Backward,
}

#[cfg(test)]
impl Direction {
    fn opposite(self) -> Self {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
        }
    }
}

/// Type of word for word-based motions
#[derive(Clone, Copy, Debug, PartialEq)]
enum WordType {
    /// Regular word (w, b, e, ge) - splits on punctuation
    Word,
    /// Long word (W, B, E, gE) - only splits on whitespace
    LongWord,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum InsertStyle {
    Before,
    After,
    LineStart,
    LineEnd,
    NewLineBelow,
    NewLineAbove,
}

/// Inclusive (f/F) lands on character, exclusive (t/T) lands before/after
#[derive(Clone, Copy, Debug, PartialEq)]
enum CharSearchType {
    Find,
    To,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TextObjectKind {
    Inner,
    Around,
}

#[derive(Clone, Copy, Debug)]
enum ScreenPosition {
    Top(usize),
    Middle,
    Bottom(usize),
}

#[derive(Clone, Debug)]
enum TextObject {
    Word(WordType),
    Pair(char),
    Paragraph,
    Sentence,
}

#[derive(Clone, Debug)]
enum EditTarget {
    Char,
    CharBackward,
    Line,
    WordStart(WordType, Direction),
    WordEnd(WordType, Direction),
    ToEndOfLine,
    Inner(TextObject),
    Around(TextObject),
    ToChar(char, CharSearchType, Direction),
}

#[derive(Clone, Debug)]
enum LastChange {
    None,
    Delete(EditTarget),
    Change(EditTarget),
    InsertText(String, InsertStyle),
    ToggleCase,
    JoinLines,
    ReplaceChar(char),
    ReplaceMode(String),
    IncrementNumber,
    DecrementNumber,
    PasteAfter,
    PasteBefore,
    /// (num_rows, col_width)
    DeleteBlock(usize, usize),
    /// (num_rows, col_width, text)
    ChangeBlock(usize, usize, String),
    /// (num_rows, text)
    InsertBlock(usize, String),
    /// (num_rows, col_offset, text)
    AppendBlock(usize, usize, String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BlockInsertType {
    Change(usize),
    Insert,
    Append(usize),
}

struct NumberAtCursor {
    start: usize,
    end: usize,
    digits: String,
    is_hex: bool,
    is_negative: bool,
}

const GUTTER_WIDTH: usize = 6;
const EMPTY_GUTTER: &str = "      ";
const LINE_CONTINUES_ABOVE: &str = "  <<< ";
const RESERVED_ROWS: usize = 2;
const PENDING_KEYS_PADDING: usize = 11;
const POSITION_WIDTH: usize = 18;

struct EditorState<'a> {
    args: &'a InputText,
    window: GuiWin,
    pane: MuxPane,
    lines: Vec<String>,
    lines_version: u64,
    history_version: u64,
    cursor: (usize, usize),
    desired_col: usize,
    mode: EditorMode,
    colors: EditorColors,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
    history: Vec<(Vec<String>, (usize, usize))>,
    history_idx: usize,
    viewport_top: usize,
    pending_keys: Vec<KeyCode>,
    pending_operator: Option<char>,
    count_prefix: Option<usize>,
    last_change: LastChange,
    last_count: usize,
    insert_buffer: String,
    insert_style: InsertStyle,
    /// (start_row, num_rows, insert_col, type)
    block_insert_info: Option<(usize, usize, usize, BlockInsertType)>,
    last_char_search: Option<(char, char)>,
    yank_buffer: String,
    yank_is_linewise: bool,
    yank_is_block: bool,
    search_pattern: String,
    search_direction: Direction,
    search_display_direction: Direction,
    search_input: String,
    search_highlight: bool,
    current_match: Option<(usize, usize)>,
    search_start_pos: (usize, usize),
    search_saved_pattern: String,
    visual_start: (usize, usize),
    replace_originals: Vec<Option<char>>,
    replace_start_pos: (usize, usize),
}

impl<'a> EditorState<'a> {
    fn new(
        args: &'a InputText,
        window: GuiWin,
        pane: MuxPane,
        buf: &'a mut BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        let initial_text = args.initial_value.clone().unwrap_or_default();
        let lines: Vec<String> = initial_text.lines().map(|s| s.to_string()).collect();
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };

        Self {
            args,
            window,
            pane,
            lines: lines.clone(),
            lines_version: 0,
            history_version: 0,
            cursor: (0, 0),
            desired_col: 0,
            mode: EditorMode::Normal,
            colors: EditorColors::new(),
            buf,
            history: vec![(lines, (0, 0))],
            history_idx: 0,
            viewport_top: 0,
            pending_keys: Vec::new(),
            pending_operator: None,
            count_prefix: None,
            last_change: LastChange::None,
            last_count: 1,
            insert_buffer: String::new(),
            insert_style: InsertStyle::Before,
            block_insert_info: None,
            last_char_search: None,
            yank_buffer: String::new(),
            yank_is_linewise: false,
            yank_is_block: false,
            search_pattern: String::new(),
            search_direction: Direction::Forward,
            search_display_direction: Direction::Forward,
            search_input: String::new(),
            search_highlight: false,
            current_match: None,
            search_start_pos: (0, 0),
            search_saved_pattern: String::new(),
            visual_start: (0, 0),
            replace_originals: Vec::new(),
            replace_start_pos: (0, 0),
        }
    }

    fn take_count(&mut self) -> usize {
        self.count_prefix.take().unwrap_or(1)
    }

    fn add_count_digit(&mut self, digit: char) {
        let d = digit.to_digit(10).unwrap_or(0) as usize;
        self.count_prefix = Some(self.count_prefix.unwrap_or(0) * 10 + d);
    }

    fn set_last_change(&mut self, change: LastChange, count: usize) {
        self.last_change = change;
        self.last_count = count;
    }

    fn wrapped_line_rows(char_count: usize, content_width: usize) -> usize {
        if content_width == 0 || char_count == 0 {
            1
        } else {
            (char_count + content_width - 1) / content_width
        }
    }

    fn cursor_visual_position(cursor_col: usize, content_width: usize) -> (usize, usize) {
        if content_width == 0 {
            (0, cursor_col)
        } else {
            (cursor_col / content_width, cursor_col % content_width)
        }
    }

    fn is_insert_like_mode(&self) -> bool {
        self.mode == EditorMode::Insert || self.mode == EditorMode::Replace
    }

    fn replace_char_at_cursor(&mut self, c: char) -> Option<char> {
        let line = &self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        if self.cursor.1 < chars.len() {
            let original = chars[self.cursor.1];
            let mut new_chars = chars;
            new_chars[self.cursor.1] = c;
            self.lines[self.cursor.0] = new_chars.into_iter().collect();
            self.cursor.1 += 1;
            self.lines_version += 1;
            Some(original)
        } else {
            self.insert_char(c);
            None
        }
    }

    fn restore_char_at_cursor(&mut self, orig_char: char) {
        let line = &self.lines[self.cursor.0];
        let mut chars: Vec<char> = line.chars().collect();
        if self.cursor.1 < chars.len() {
            chars[self.cursor.1] = orig_char;
            self.lines[self.cursor.0] = chars.into_iter().collect();
            self.lines_version += 1;
        }
    }

    fn record_change(&mut self) {
        if self.history_idx < self.history.len() - 1 {
            self.history.truncate(self.history_idx + 1);
        }
        if self.lines_version != self.history_version {
            self.history.push((self.lines.clone(), self.cursor));
            self.history_idx = self.history.len() - 1;
            self.history_version = self.lines_version;
        }
    }

    /// Defers recording until exiting insert mode for change operations
    fn maybe_record_change(&mut self) {
        if !self.is_insert_like_mode() {
            self.record_change();
        }
    }

    fn save_undo_state(&mut self) {
        self.save_undo_state_with_cursor(self.cursor);
    }

    fn save_undo_state_with_cursor(&mut self, cursor: (usize, usize)) {
        if self.history_idx < self.history.len() - 1 {
            self.history.truncate(self.history_idx + 1);
        }
        // Cursor movements alone don't create new undo points
        if self.lines_version == self.history_version {
            if let Some(entry) = self.history.last_mut() {
                entry.1 = cursor;
            }
            return;
        }
        self.history.push((self.lines.clone(), cursor));
        self.history_idx = self.history.len() - 1;
        self.history_version = self.lines_version;
    }

    fn undo(&mut self) {
        if self.history_idx > 0 {
            self.history_idx -= 1;
            let (lines, cursor) = &self.history[self.history_idx];
            self.lines = lines.clone();
            self.cursor = *cursor;
        }
    }

    fn redo(&mut self) {
        if self.history_idx < self.history.len() - 1 {
            let cursor_at_change_start = self.history[self.history_idx].1;
            self.history_idx += 1;
            let (lines, _) = &self.history[self.history_idx];
            self.lines = lines.clone();
            self.cursor = cursor_at_change_start;
            self.clamp_cursor();
        }
    }

    fn move_cursor(&mut self, row: isize, col: isize) {
        let new_row = (self.cursor.0 as isize + row)
            .max(0)
            .min((self.lines.len() - 1) as isize) as usize;
        let line_len = self.lines[new_row].chars().count();
        let max_col = if self.is_insert_like_mode() {
            line_len
        } else {
            line_len.saturating_sub(1)
        };

        if col != 0 {
            let new_col = (self.cursor.1 as isize + col).max(0).min(max_col as isize) as usize;
            self.cursor = (new_row, new_col);
            self.desired_col = new_col;
        } else {
            let new_col = self.desired_col.min(max_col);
            self.cursor = (new_row, new_col);
        }
    }

    fn update_desired_col(&mut self) {
        self.desired_col = self.cursor.1;
    }

    fn clamp_cursor(&mut self) {
        if self.cursor.0 >= self.lines.len() {
            self.cursor.0 = self.lines.len().saturating_sub(1);
        }
        let line_len = self.lines[self.cursor.0].chars().count();
        let max_col = if self.is_insert_like_mode() {
            line_len
        } else {
            line_len.saturating_sub(1)
        };
        if self.cursor.1 > max_col {
            self.cursor.1 = max_col;
        }
    }

    fn insert_char(&mut self, c: char) {
        self.insert_char_no_undo(c);
        if !self.is_insert_like_mode() {
            self.record_change();
        }
    }

    fn insert_char_no_undo(&mut self, c: char) {
        let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 >= chars.len() {
            chars.push(c);
        } else {
            chars.insert(self.cursor.1, c);
        }
        self.lines[self.cursor.0] = chars.into_iter().collect();
        self.cursor.1 += 1;
        self.lines_version += 1;
    }

    fn delete_char(&mut self) {
        self.delete_char_no_undo();
        if !self.is_insert_like_mode() {
            self.record_change();
        }
    }

    fn delete_char_no_undo(&mut self) {
        let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if !chars.is_empty() && self.cursor.1 < chars.len() {
            self.yank_buffer = chars[self.cursor.1].to_string();
            self.yank_is_linewise = false;
            chars.remove(self.cursor.1);
            self.lines[self.cursor.0] = chars.into_iter().collect();
            self.clamp_cursor();
            self.update_desired_col();
            self.lines_version += 1;
        }
    }

    fn insert_newline(&mut self) {
        self.insert_newline_no_undo();
        if !self.is_insert_like_mode() {
            self.record_change();
        }
    }

    fn insert_newline_no_undo(&mut self) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        let (before, after): (String, String) = if self.cursor.1 < chars.len() {
            (
                chars[..self.cursor.1].iter().collect(),
                chars[self.cursor.1..].iter().collect(),
            )
        } else {
            (chars.into_iter().collect(), String::new())
        };
        self.lines[self.cursor.0] = before;
        self.lines.insert(self.cursor.0 + 1, after);
        self.cursor.0 += 1;
        self.cursor.1 = 0;
        self.lines_version += 1;
    }

    fn insert_text(&mut self, text: &str) {
        for c in text.chars() {
            if c == '\n' {
                self.insert_newline_no_undo();
            } else if c == '\r' {
                continue;
            } else {
                self.insert_char_no_undo(c);
            }
        }
    }

    fn delete_line(&mut self) {
        self.delete_lines(1);
    }

    fn delete_lines(&mut self, count: usize) {
        self.save_undo_state();
        self.lines_version += 1;

        let end_row = (self.cursor.0 + count).min(self.lines.len());
        let actual_count = end_row - self.cursor.0;

        let yanked: Vec<&str> = self.lines[self.cursor.0..end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        if self.lines.len() > actual_count {
            for _ in 0..actual_count {
                self.lines.remove(self.cursor.0);
            }
            if self.cursor.0 >= self.lines.len() {
                self.cursor.0 = self.lines.len() - 1;
            }
            self.clamp_cursor();
            self.update_desired_col();
            self.record_change();
        } else {
            self.lines.clear();
            self.lines.push(String::new());
            self.cursor = (0, 0);
            self.update_desired_col();
            self.record_change();
        }
    }

    fn delete_to_end_of_file(&mut self) {
        self.save_undo_state();
        let yanked: Vec<&str> = self.lines[self.cursor.0..]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        self.lines_version += 1;
        self.lines.truncate(self.cursor.0 + 1);
        self.lines[self.cursor.0].clear();
        if self.cursor.0 > 0 && self.lines[self.cursor.0].is_empty() {
            self.lines.remove(self.cursor.0);
            self.cursor.0 -= 1;
        }
        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn delete_to_start_of_file(&mut self) {
        self.save_undo_state();
        let yanked: Vec<&str> = self.lines[..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        self.lines_version += 1;
        for _ in 0..=self.cursor.0 {
            self.lines.remove(0);
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor.0 = 0;
        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn delete_line_and_below(&mut self) {
        if self.cursor.0 >= self.lines.len() - 1 {
            self.delete_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        let end_row = (self.cursor.0 + 1).min(self.lines.len() - 1);
        let yanked: Vec<&str> = self.lines[self.cursor.0..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        self.lines.remove(self.cursor.0);
        if self.cursor.0 < self.lines.len() {
            self.lines.remove(self.cursor.0);
        }

        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        if self.cursor.0 >= self.lines.len() {
            self.cursor.0 = self.lines.len() - 1;
        }
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.update_desired_col();
        self.record_change();
    }

    fn delete_line_and_above(&mut self) {
        if self.cursor.0 == 0 {
            self.delete_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        let start_row = self.cursor.0 - 1;
        let yanked: Vec<&str> = self.lines[start_row..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        self.lines.remove(start_row);
        self.lines.remove(start_row);

        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        self.cursor.0 = start_row.min(self.lines.len() - 1);
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.update_desired_col();
        self.record_change();
    }

    fn change_line_and_below(&mut self) {
        if self.cursor.0 >= self.lines.len() - 1 {
            self.substitute_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        let end_row = (self.cursor.0 + 1).min(self.lines.len() - 1);
        let yanked: Vec<&str> = self.lines[self.cursor.0..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        self.lines.remove(self.cursor.0);
        if self.cursor.0 < self.lines.len() {
            self.lines.remove(self.cursor.0);
        }

        self.lines.insert(self.cursor.0, String::new());
        self.cursor.1 = 0;
        self.update_desired_col();
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn change_line_and_above(&mut self) {
        if self.cursor.0 == 0 {
            self.substitute_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        let start_row = self.cursor.0 - 1;
        let yanked: Vec<&str> = self.lines[start_row..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        self.lines.remove(start_row);
        self.lines.remove(start_row);

        self.lines.insert(start_row, String::new());
        self.cursor.0 = start_row;
        self.cursor.1 = 0;
        self.update_desired_col();
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn change_to_start_of_file(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;
        for _ in 0..=self.cursor.0 {
            self.lines.remove(0);
        }
        self.lines.insert(0, String::new());
        self.cursor.0 = 0;
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn change_to_end_of_file(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;
        self.lines.truncate(self.cursor.0);
        self.lines.push(String::new());
        self.cursor.0 = self.lines.len() - 1;
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn yank_lines(&mut self, count: usize) {
        let end_row = (self.cursor.0 + count).min(self.lines.len());
        let yanked: Vec<&str> = self.lines[self.cursor.0..end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
    }

    fn yank_to_end_of_file(&mut self) {
        let yanked: Vec<&str> = self.lines[self.cursor.0..]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
    }

    fn yank_to_start_of_file(&mut self) {
        let yanked: Vec<&str> = self.lines[..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        self.cursor.0 = 0;
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn yank_to_end_of_line(&mut self) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 < chars.len() {
            self.yank_buffer = chars[self.cursor.1..].iter().collect();
        } else {
            self.yank_buffer.clear();
        }
        self.yank_is_linewise = false;
    }

    fn yank_line_and_below(&mut self) {
        let end_row = (self.cursor.0 + 1).min(self.lines.len() - 1);
        let yanked: Vec<&str> = self.lines[self.cursor.0..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
    }

    fn yank_line_and_above(&mut self) {
        let start_row = if self.cursor.0 > 0 {
            self.cursor.0 - 1
        } else {
            0
        };
        let yanked: Vec<&str> = self.lines[start_row..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.clamp_cursor();
            self.update_desired_col();
        }
    }

    fn get_char_left_pos(&self) -> (usize, usize) {
        if self.cursor.1 > 0 {
            (self.cursor.0, self.cursor.1 - 1)
        } else {
            self.cursor
        }
    }

    fn get_char_right_pos(&self) -> (usize, usize) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 < chars.len() {
            (self.cursor.0, self.cursor.1 + 1)
        } else {
            self.cursor
        }
    }

    fn get_word_forward_pos(&self, word_type: WordType) -> (usize, usize) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 >= chars.len() {
            if self.cursor.0 < self.lines.len() - 1 {
                return (self.cursor.0 + 1, 0);
            }
            return self.cursor;
        }

        let mut idx = self.cursor.1;
        let start_char = chars[idx];

        match word_type {
            WordType::Word => {
                if start_char.is_whitespace() {
                    while idx < chars.len() && chars[idx].is_whitespace() {
                        idx += 1;
                    }
                } else if start_char.is_ascii_punctuation() {
                    while idx < chars.len() && chars[idx].is_ascii_punctuation() {
                        idx += 1;
                    }
                    while idx < chars.len() && chars[idx].is_whitespace() {
                        idx += 1;
                    }
                } else {
                    while idx < chars.len()
                        && !chars[idx].is_whitespace()
                        && !chars[idx].is_ascii_punctuation()
                    {
                        idx += 1;
                    }
                    while idx < chars.len() && chars[idx].is_whitespace() {
                        idx += 1;
                    }
                }
            }
            WordType::LongWord => {
                while idx < chars.len() && chars[idx].is_whitespace() {
                    idx += 1;
                }
                while idx < chars.len() && !chars[idx].is_whitespace() {
                    idx += 1;
                }
                while idx < chars.len() && chars[idx].is_whitespace() {
                    idx += 1;
                }
            }
        }

        if idx >= chars.len() {
            if self.cursor.0 < self.lines.len() - 1 {
                (self.cursor.0 + 1, 0)
            } else {
                (self.cursor.0, chars.len())
            }
        } else {
            (self.cursor.0, idx)
        }
    }

    fn move_word_forward(&mut self, word_type: WordType) {
        self.cursor = self.get_word_forward_pos(word_type);
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn get_word_backward_pos(&self, word_type: WordType) -> (usize, usize) {
        let is_same_type = |c: char, ref_c: char, wt: WordType| -> bool {
            match wt {
                WordType::LongWord => !c.is_whitespace(),
                WordType::Word => {
                    if ref_c.is_ascii_punctuation() {
                        c.is_ascii_punctuation()
                    } else {
                        !c.is_whitespace() && !c.is_ascii_punctuation()
                    }
                }
            }
        };

        let find_word_start = |chars: &[char], mut idx: usize, wt: WordType| -> usize {
            while idx > 0 && chars[idx].is_whitespace() {
                idx -= 1;
            }
            if idx == 0 {
                return 0;
            }
            let ref_char = chars[idx];
            while idx > 0 {
                let prev = idx - 1;
                if chars[prev].is_whitespace() || !is_same_type(chars[prev], ref_char, wt) {
                    break;
                }
                idx -= 1;
            }
            idx
        };

        if self.cursor.1 == 0 {
            if self.cursor.0 > 0 {
                let prev_line = self.cursor.0 - 1;
                let chars: Vec<char> = self.lines[prev_line].chars().collect();
                if chars.is_empty() {
                    return (prev_line, 0);
                }
                return (
                    prev_line,
                    find_word_start(&chars, chars.len() - 1, word_type),
                );
            }
            return self.cursor;
        }

        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        let mut idx = self.cursor.1 - 1;

        while idx > 0 && chars[idx].is_whitespace() {
            idx -= 1;
        }

        if idx == 0 && chars[idx].is_whitespace() {
            if self.cursor.0 > 0 {
                let prev_line = self.cursor.0 - 1;
                let prev_chars: Vec<char> = self.lines[prev_line].chars().collect();
                if prev_chars.is_empty() {
                    return (prev_line, 0);
                }
                return (
                    prev_line,
                    find_word_start(&prev_chars, prev_chars.len() - 1, word_type),
                );
            }
            return self.cursor;
        }

        (self.cursor.0, find_word_start(&chars, idx, word_type))
    }

    fn move_word_backward(&mut self, word_type: WordType) {
        self.cursor = self.get_word_backward_pos(word_type);
        self.update_desired_col();
    }

    fn get_word_end_pos(&self, word_type: WordType) -> (usize, usize) {
        let is_same_type = |c: char, ref_c: char, wt: WordType| -> bool {
            match wt {
                WordType::LongWord => !c.is_whitespace(),
                WordType::Word => {
                    if ref_c.is_ascii_punctuation() {
                        c.is_ascii_punctuation()
                    } else {
                        !c.is_whitespace() && !c.is_ascii_punctuation()
                    }
                }
            }
        };

        let mut curr = self.cursor;
        loop {
            let current_line = &self.lines[curr.0];
            let chars: Vec<char> = current_line.chars().collect();

            if curr.1 >= chars.len().saturating_sub(1) {
                if curr.0 < self.lines.len() - 1 {
                    curr = (curr.0 + 1, 0);
                    continue;
                }
                return curr;
            }

            let mut idx = curr.1 + 1;
            while idx < chars.len() && chars[idx].is_whitespace() {
                idx += 1;
            }

            if idx >= chars.len() {
                if curr.0 < self.lines.len() - 1 {
                    curr = (curr.0 + 1, 0);
                    continue;
                } else {
                    return (curr.0, chars.len().saturating_sub(1));
                }
            }

            let ref_char = chars[idx];
            while idx < chars.len() {
                let next = idx + 1;
                if next >= chars.len() {
                    break;
                }
                if chars[next].is_whitespace() || !is_same_type(chars[next], ref_char, word_type) {
                    break;
                }
                idx += 1;
            }
            return (curr.0, idx);
        }
    }

    fn move_to_word_end(&mut self, word_type: WordType) {
        self.cursor = self.get_word_end_pos(word_type);
        self.update_desired_col();
    }

    fn get_word_end_backward_pos(&self, word_type: WordType) -> (usize, usize) {
        let mut row = self.cursor.0;
        let mut col = self.cursor.1;

        let char_type = |c: char, wt: WordType| -> u8 {
            if c.is_whitespace() {
                0
            } else if wt == WordType::Word && c.is_ascii_punctuation() {
                2
            } else {
                1
            }
        };

        let get_char = |r: usize, c: usize, lines: &[String]| -> Option<char> {
            let chars: Vec<char> = lines[r].chars().collect();
            if c < chars.len() {
                Some(chars[c])
            } else {
                None
            }
        };

        if col > 0 {
            col -= 1;
        } else if row > 0 {
            row -= 1;
            col = self.lines[row].chars().count().saturating_sub(1);
        } else {
            return (0, 0);
        }

        loop {
            let chars: Vec<char> = self.lines[row].chars().collect();
            if chars.is_empty() {
                if row > 0 {
                    row -= 1;
                    col = self.lines[row].chars().count().saturating_sub(1);
                    continue;
                }
                return (0, 0);
            }

            if col < chars.len() && chars[col].is_whitespace() {
                if col > 0 {
                    col -= 1;
                    continue;
                } else if row > 0 {
                    row -= 1;
                    col = self.lines[row].chars().count().saturating_sub(1);
                    continue;
                }
                return (0, 0);
            }
            break;
        }

        let orig_char = get_char(self.cursor.0, self.cursor.1, &self.lines);
        let curr_char = get_char(row, col, &self.lines);

        if let (Some(orig_c), Some(curr_c)) = (orig_char, curr_char) {
            let orig_type = char_type(orig_c, word_type);
            let curr_type = char_type(curr_c, word_type);

            if orig_type != 0 && row == self.cursor.0 {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let mut still_same_word = true;

                for i in (col + 1)..=self.cursor.1 {
                    if i < chars.len() {
                        let t = char_type(chars[i], word_type);
                        // For Word: break on whitespace or type change
                        // For LongWord: break only on whitespace
                        if t == 0 || (word_type == WordType::Word && t != orig_type) {
                            still_same_word = false;
                            break;
                        }
                    }
                }

                let should_skip = match word_type {
                    WordType::Word => still_same_word && curr_type == orig_type,
                    WordType::LongWord => still_same_word,
                };

                if should_skip {
                    while col > 0 {
                        let prev_type = char_type(chars[col - 1], word_type);
                        match word_type {
                            WordType::Word => {
                                if prev_type != curr_type {
                                    break;
                                }
                            }
                            WordType::LongWord => {
                                if prev_type == 0 {
                                    break;
                                }
                            }
                        }
                        col -= 1;
                    }

                    if col > 0 {
                        col -= 1;
                    } else if row > 0 {
                        row -= 1;
                        col = self.lines[row].chars().count().saturating_sub(1);
                    } else {
                        return (0, 0);
                    }

                    loop {
                        let chars: Vec<char> = self.lines[row].chars().collect();
                        if chars.is_empty() {
                            if row > 0 {
                                row -= 1;
                                col = self.lines[row].chars().count().saturating_sub(1);
                                continue;
                            }
                            return (0, 0);
                        }

                        if col < chars.len() && chars[col].is_whitespace() {
                            if col > 0 {
                                col -= 1;
                                continue;
                            } else if row > 0 {
                                row -= 1;
                                col = self.lines[row].chars().count().saturating_sub(1);
                                continue;
                            }
                            return (0, 0);
                        }
                        break;
                    }
                }
            }
        }

        (row, col)
    }

    fn move_to_word_end_backward(&mut self, word_type: WordType) {
        self.cursor = self.get_word_end_backward_pos(word_type);
        self.update_desired_col();
    }

    fn get_first_non_blank_pos(&self) -> (usize, usize) {
        let line = &self.lines[self.cursor.0];
        for (i, ch) in line.chars().enumerate() {
            if !ch.is_whitespace() {
                return (self.cursor.0, i);
            }
        }
        (self.cursor.0, 0)
    }

    fn move_to_first_non_blank(&mut self) {
        self.cursor = self.get_first_non_blank_pos();
        self.update_desired_col();
    }

    /// Get visible lines in the current viewport.
    /// Returns a vector of line indices that are currently visible on screen.
    fn get_visible_lines(&self) -> Vec<usize> {
        let (cols, rows) = self.buf.dimensions();
        let content_start_row = if self.args.title.is_some() { 1 } else { 0 };
        let content_rows = rows.saturating_sub(RESERVED_ROWS + content_start_row);
        let content_width = cols.saturating_sub(GUTTER_WIDTH);

        if content_width == 0 || content_rows == 0 {
            return Vec::new();
        }

        let mut visible_lines = Vec::new();
        let mut visual_row = 0;
        let mut line_idx = self.viewport_top;

        while line_idx < self.lines.len() && visual_row < content_rows {
            visible_lines.push(line_idx);
            let line_visual_rows =
                Self::wrapped_line_rows(self.lines[line_idx].chars().count(), content_width);
            visual_row += line_visual_rows;
            line_idx += 1;
        }

        visible_lines
    }

    /// Move cursor to a screen-relative position (H/M/L commands)
    fn move_to_screen_position(&mut self, position: ScreenPosition) {
        let visible = self.get_visible_lines();
        if visible.is_empty() {
            return;
        }

        let idx = match position {
            ScreenPosition::Top(count) => (count.saturating_sub(1)).min(visible.len() - 1),
            ScreenPosition::Middle => visible.len() / 2,
            ScreenPosition::Bottom(count) => visible.len().saturating_sub(count),
        };

        self.cursor.0 = visible[idx];
        self.move_to_first_non_blank();
    }

    /// Scroll viewport down by count lines (Ctrl-E)
    /// Cursor stays on same line if visible, otherwise moves to stay on screen
    fn scroll_down(&mut self, count: usize) {
        let max_viewport = self.lines.len().saturating_sub(1);
        self.viewport_top = (self.viewport_top + count).min(max_viewport);

        // If cursor is now above viewport, move it down
        if self.cursor.0 < self.viewport_top {
            self.cursor.0 = self.viewport_top;
            self.clamp_cursor();
            self.update_desired_col();
        }
    }

    /// Scroll viewport up by count lines (Ctrl-Y)
    /// Cursor stays on same line if visible, otherwise moves to stay on screen
    fn scroll_up(&mut self, count: usize) {
        self.viewport_top = self.viewport_top.saturating_sub(count);

        // If cursor is now below viewport, move it up
        let visible = self.get_visible_lines();
        if let Some(&last_visible) = visible.last() {
            if self.cursor.0 > last_visible {
                self.cursor.0 = last_visible;
                self.clamp_cursor();
                self.update_desired_col();
            }
        }
    }

    /// Scroll half page down (Ctrl-D)
    /// Both viewport and cursor move down by half a page
    fn scroll_half_page_down(&mut self, count: usize) {
        let (_cols, rows) = self.buf.dimensions();
        let content_start_row = if self.args.title.is_some() { 1 } else { 0 };
        let content_rows = rows.saturating_sub(RESERVED_ROWS + content_start_row);
        let half_page = (content_rows / 2).max(1) * count;

        let max_viewport = self.lines.len().saturating_sub(1);
        self.viewport_top = (self.viewport_top + half_page).min(max_viewport);
        self.cursor.0 = (self.cursor.0 + half_page).min(self.lines.len().saturating_sub(1));
        self.clamp_cursor();
        self.update_desired_col();
    }

    /// Scroll half page up (Ctrl-U)
    /// Both viewport and cursor move up by half a page
    fn scroll_half_page_up(&mut self, count: usize) {
        let (_cols, rows) = self.buf.dimensions();
        let content_start_row = if self.args.title.is_some() { 1 } else { 0 };
        let content_rows = rows.saturating_sub(RESERVED_ROWS + content_start_row);
        let half_page = (content_rows / 2).max(1) * count;

        self.viewport_top = self.viewport_top.saturating_sub(half_page);
        self.cursor.0 = self.cursor.0.saturating_sub(half_page);
        self.clamp_cursor();
        self.update_desired_col();
    }

    /// Scroll viewport so cursor line is at center of screen (zz)
    fn scroll_cursor_to_center(&mut self) {
        let (_cols, rows) = self.buf.dimensions();
        let content_start_row = if self.args.title.is_some() { 1 } else { 0 };
        let content_rows = rows.saturating_sub(RESERVED_ROWS + content_start_row);
        let half = content_rows / 2;

        // Set viewport_top so cursor is in the middle
        self.viewport_top = self.cursor.0.saturating_sub(half);

        // Clamp viewport to valid range
        let max_viewport = self.lines.len().saturating_sub(1);
        self.viewport_top = self.viewport_top.min(max_viewport);
    }

    /// Scroll viewport so cursor line is at top of screen (zt)
    fn scroll_cursor_to_top(&mut self) {
        self.viewport_top = self.cursor.0;

        // Clamp viewport to valid range
        let max_viewport = self.lines.len().saturating_sub(1);
        self.viewport_top = self.viewport_top.min(max_viewport);
    }

    /// Scroll viewport so cursor line is at bottom of screen (zb)
    fn scroll_cursor_to_bottom(&mut self) {
        let (_cols, rows) = self.buf.dimensions();
        let content_start_row = if self.args.title.is_some() { 1 } else { 0 };
        let content_rows = rows.saturating_sub(RESERVED_ROWS + content_start_row);

        // Set viewport_top so cursor is at the bottom visible line
        self.viewport_top = self.cursor.0.saturating_sub(content_rows.saturating_sub(1));

        // Clamp viewport to valid range (at minimum 0)
        let max_viewport = self.lines.len().saturating_sub(1);
        self.viewport_top = self.viewport_top.min(max_viewport);
    }

    fn get_line_start_pos(&self) -> (usize, usize) {
        (self.cursor.0, 0)
    }

    /// Compute proper inner pair selection bounds for Visual mode
    /// Returns (visual_start, cursor) positions for selecting content inside brackets
    fn get_inner_pair_visual_bounds(
        &self,
        open_pos: (usize, usize),
        close_pos: (usize, usize),
    ) -> ((usize, usize), (usize, usize)) {
        let (open_row, open_col) = open_pos;
        let (close_row, close_col) = close_pos;

        // For multi-line pairs where brackets are on their own lines,
        // we need to keep start/end spanning the lines so delete_visual_selection
        // properly handles it as a multi-line deletion.

        if open_row == close_row {
            // Same line: select content between brackets
            if close_col > open_col + 1 {
                // There's content between brackets
                ((open_row, open_col + 1), (close_row, close_col - 1))
            } else {
                // Empty or adjacent brackets - return same position
                ((open_row, open_col + 1), (open_row, open_col + 1))
            }
        } else {
            // Multi-line: start after opening bracket, end before closing bracket
            // Keep positions on the bracket lines to ensure proper multi-line deletion
            let open_line_len = self.lines[open_row].chars().count();

            // Start position: right after opening bracket
            // If bracket is at end of line, use that position (past end) to indicate
            // the selection starts from the next line's beginning
            let start = (open_row, (open_col + 1).min(open_line_len));

            // End position: right before closing bracket
            // If bracket is at start of line (col 0), we need to end at previous line's end
            let end = if close_col > 0 {
                (close_row, close_col - 1)
            } else {
                // Closing bracket at column 0 - end at last char of previous line
                let prev_line_len = self.lines[close_row - 1].chars().count();
                if prev_line_len > 0 {
                    (close_row - 1, prev_line_len - 1)
                } else {
                    (close_row - 1, 0)
                }
            };

            // Ensure start <= end
            if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
                (start, end)
            } else {
                // Content is empty (e.g., just newlines between brackets)
                // Return positions that will result in deleting the empty lines
                (start, start)
            }
        }
    }

    fn get_inner_word_bounds(&self) -> (usize, usize) {
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));

        let is_word_char = |c: char| !c.is_whitespace() && !c.is_ascii_punctuation();
        let is_punct = |c: char| c.is_ascii_punctuation();

        let cur_char = chars[col];
        let char_type_matches: Box<dyn Fn(char) -> bool> = if cur_char.is_whitespace() {
            Box::new(|c: char| c.is_whitespace())
        } else if is_punct(cur_char) {
            Box::new(is_punct)
        } else {
            Box::new(is_word_char)
        };

        let mut start = col;
        while start > 0 && char_type_matches(chars[start - 1]) {
            start -= 1;
        }

        let mut end = col;
        while end < chars.len() && char_type_matches(chars[end]) {
            end += 1;
        }

        (start, end)
    }

    fn get_a_word_bounds(&self) -> (usize, usize) {
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let (word_start, word_end) = self.get_inner_word_bounds();

        let mut end = word_end;
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        if end == word_end {
            let mut start = word_start;
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
            (start, word_end)
        } else {
            (word_start, end)
        }
    }

    fn delete_text_object_on_line(&mut self, start: usize, end: usize) {
        let line_len = self.lines[self.cursor.0].len();
        if start < end && end <= line_len {
            self.save_undo_state();
            self.lines_version += 1;
            self.yank_buffer = self.lines[self.cursor.0][start..end].to_string();
            self.yank_is_linewise = false;
            self.lines[self.cursor.0].replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
            self.update_desired_col();
            self.maybe_record_change();
        }
    }

    fn yank_char_range(
        &self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) -> String {
        let mut yanked = String::new();
        for row in start_row..=end_row {
            let chars: Vec<char> = self.lines[row].chars().collect();
            let start = if row == start_row { start_col } else { 0 };
            let end = if row == end_row {
                (end_col + 1).min(chars.len())
            } else {
                chars.len()
            };
            if start < end {
                yanked.push_str(&chars[start..end].iter().collect::<String>());
            }
            if row < end_row {
                yanked.push('\n');
            }
        }
        yanked
    }

    fn delete_char_range(
        &mut self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) {
        if start_row == end_row {
            let mut chars: Vec<char> = self.lines[start_row].chars().collect();
            let delete_end = (end_col + 1).min(chars.len());
            if start_col < delete_end {
                chars.drain(start_col..delete_end);
                self.lines[start_row] = chars.into_iter().collect();
            }
        } else {
            let start_chars: Vec<char> = self.lines[start_row].chars().collect();
            let end_chars: Vec<char> = self.lines[end_row].chars().collect();

            let before: String = start_chars[..start_col.min(start_chars.len())]
                .iter()
                .collect();
            let after: String = if end_col + 1 < end_chars.len() {
                end_chars[end_col + 1..].iter().collect()
            } else {
                String::new()
            };

            for _ in start_row + 1..=end_row {
                if start_row + 1 < self.lines.len() {
                    self.lines.remove(start_row + 1);
                }
            }

            self.lines[start_row] = before + &after;
        }

        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
    }

    fn delete_inner_word(&mut self) {
        let (start, end) = self.get_inner_word_bounds();
        self.delete_text_object_on_line(start, end);
    }

    fn delete_a_word(&mut self) {
        let (start, end) = self.get_a_word_bounds();
        self.delete_text_object_on_line(start, end);
    }

    fn get_inner_long_word_bounds(&self) -> (usize, usize) {
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));

        if chars[col].is_whitespace() {
            let mut start = col;
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
            let mut end = col;
            while end < chars.len() && chars[end].is_whitespace() {
                end += 1;
            }
            (start, end)
        } else {
            let mut start = col;
            while start > 0 && !chars[start - 1].is_whitespace() {
                start -= 1;
            }
            let mut end = col;
            while end < chars.len() && !chars[end].is_whitespace() {
                end += 1;
            }
            (start, end)
        }
    }

    fn get_a_long_word_bounds(&self) -> (usize, usize) {
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let (word_start, word_end) = self.get_inner_long_word_bounds();

        let mut end = word_end;
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        if end == word_end {
            let mut start = word_start;
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
            (start, word_end)
        } else {
            (word_start, end)
        }
    }

    fn delete_inner_long_word(&mut self) {
        let (start, end) = self.get_inner_long_word_bounds();
        self.delete_text_object_on_line(start, end);
    }

    fn delete_a_long_word(&mut self) {
        let (start, end) = self.get_a_long_word_bounds();
        self.delete_text_object_on_line(start, end);
    }

    fn get_paragraph_bounds(&self, kind: TextObjectKind) -> Option<(usize, usize)> {
        match kind {
            TextObjectKind::Inner => Some(self.get_inner_paragraph_bounds_impl()),
            TextObjectKind::Around => self.get_around_paragraph_bounds_impl(),
        }
    }

    fn get_inner_paragraph_bounds_impl(&self) -> (usize, usize) {
        let mut start_row = self.cursor.0;
        let mut end_row = self.cursor.0;

        if self.lines[self.cursor.0].trim().is_empty() {
            while start_row > 0 && self.lines[start_row - 1].trim().is_empty() {
                start_row -= 1;
            }
            while end_row < self.lines.len() - 1 && self.lines[end_row + 1].trim().is_empty() {
                end_row += 1;
            }
            return (start_row, end_row);
        }

        while start_row > 0 && !self.lines[start_row - 1].trim().is_empty() {
            start_row -= 1;
        }

        while end_row < self.lines.len() - 1 && !self.lines[end_row + 1].trim().is_empty() {
            end_row += 1;
        }

        (start_row, end_row)
    }

    /// Includes trailing blank lines, or leading if at end of file
    fn get_around_paragraph_bounds_impl(&self) -> Option<(usize, usize)> {
        if self.lines[self.cursor.0].trim().is_empty() {
            let mut start_row = self.cursor.0;
            while start_row > 0 && self.lines[start_row - 1].trim().is_empty() {
                start_row -= 1;
            }
            let mut end_row = self.cursor.0;
            while end_row < self.lines.len() - 1 && self.lines[end_row + 1].trim().is_empty() {
                end_row += 1;
            }

            if end_row >= self.lines.len() - 1 {
                return None;
            }

            end_row += 1;
            while end_row < self.lines.len() - 1 && !self.lines[end_row + 1].trim().is_empty() {
                end_row += 1;
            }

            return Some((start_row, end_row));
        }

        let (mut start_row, mut end_row) = self.get_inner_paragraph_bounds_impl();

        let original_end = end_row;
        while end_row < self.lines.len() - 1 && self.lines[end_row + 1].trim().is_empty() {
            end_row += 1;
        }

        if end_row == original_end {
            while start_row > 0 && self.lines[start_row - 1].trim().is_empty() {
                start_row -= 1;
            }
        }

        Some((start_row, end_row))
    }

    fn delete_paragraph(&mut self, kind: TextObjectKind) {
        let Some((start_row, end_row)) = self.get_paragraph_bounds(kind) else {
            return; // Do nothing for Around when no valid bounds
        };

        self.save_undo_state();
        self.lines_version += 1;

        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        for _ in start_row..=end_row {
            self.lines.remove(start_row);
        }

        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        self.cursor.0 = start_row.min(self.lines.len() - 1);
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.clamp_cursor();
        self.update_desired_col();
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    fn yank_paragraph(&mut self, kind: TextObjectKind) {
        let Some((start_row, end_row)) = self.get_paragraph_bounds(kind) else {
            return; // Do nothing for Around when no valid bounds
        };

        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        self.cursor.0 = start_row;
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn change_paragraph(&mut self, kind: TextObjectKind) {
        let Some((start_row, end_row)) = self.get_paragraph_bounds(kind) else {
            return;
        };

        self.save_undo_state();
        self.lines_version += 1;

        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        for _ in start_row..=end_row {
            self.lines.remove(start_row);
        }

        self.lines.insert(start_row, String::new());

        self.cursor.0 = start_row;
        self.cursor.1 = 0;
        self.update_desired_col();
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn get_inner_sentence_bounds(&self) -> (usize, usize, usize, usize) {
        let para_start = {
            let mut row = self.cursor.0;
            while row > 0 && !self.lines[row - 1].trim().is_empty() {
                row -= 1;
            }
            row
        };

        let (mut start_row, mut start_col) =
            self.find_sentence_start_for_end(self.cursor.0, self.cursor.1);

        if start_row < para_start {
            start_row = para_start;
            start_col = self.find_line_start(start_row);
        }

        while start_row < self.lines.len() && self.lines[start_row].trim().is_empty() {
            start_row += 1;
            start_col = 0;
        }
        if start_row < self.lines.len() {
            let chars: Vec<char> = self.lines[start_row].chars().collect();
            while start_col < chars.len() && chars[start_col].is_whitespace() {
                start_col += 1;
            }
        }

        let mut end_row = self.cursor.0;
        let mut end_col = self.cursor.1;

        loop {
            let chars: Vec<char> = self.lines[end_row].chars().collect();

            while end_col < chars.len() {
                if Self::is_valid_sentence_end(&chars, end_col) {
                    // Found sentence end - include closing chars after punctuation
                    let mut final_col = end_col;
                    while final_col + 1 < chars.len()
                        && Self::is_sentence_closing_char(chars[final_col + 1])
                    {
                        final_col += 1;
                    }
                    return (start_row, start_col, end_row, final_col);
                }
                end_col += 1;
            }

            if end_row < self.lines.len() - 1 {
                if self.lines[end_row + 1].trim().is_empty() {
                    let line_end = chars.len().saturating_sub(1);
                    return (start_row, start_col, end_row, line_end);
                }
                end_row += 1;
                end_col = 0;
            } else {
                let line_end = chars.len().saturating_sub(1);
                return (start_row, start_col, end_row, line_end);
            }
        }
    }

    fn get_a_sentence_bounds(&self) -> (usize, usize, usize, usize) {
        let (start_row, start_col, end_row, end_col) = self.get_inner_sentence_bounds();

        let mut new_end_row = end_row;
        let mut new_end_col = end_col;

        let chars: Vec<char> = self.lines[new_end_row].chars().collect();
        let mut next_col = new_end_col + 1;

        while next_col < chars.len() && chars[next_col].is_whitespace() {
            new_end_col = next_col;
            next_col += 1;
        }

        if new_end_col > end_col || next_col < chars.len() {
            return (start_row, start_col, new_end_row, new_end_col);
        }

        if new_end_row < self.lines.len() - 1 && !self.lines[new_end_row + 1].trim().is_empty() {
            let next_chars: Vec<char> = self.lines[new_end_row + 1].chars().collect();
            if !next_chars.is_empty() && next_chars[0].is_whitespace() {
                new_end_row += 1;
                new_end_col = 0;
                while new_end_col + 1 < next_chars.len()
                    && next_chars[new_end_col + 1].is_whitespace()
                {
                    new_end_col += 1;
                }
                return (start_row, start_col, new_end_row, new_end_col);
            }
        }

        if start_col > 0 {
            let start_chars: Vec<char> = self.lines[start_row].chars().collect();
            let mut new_start_col = start_col;
            while new_start_col > 0 && start_chars[new_start_col - 1].is_whitespace() {
                new_start_col -= 1;
            }
            if new_start_col < start_col {
                return (start_row, new_start_col, end_row, end_col);
            }
        }

        (start_row, start_col, end_row, end_col)
    }

    fn get_sentence_bounds(&self, kind: TextObjectKind) -> (usize, usize, usize, usize) {
        match kind {
            TextObjectKind::Inner => self.get_inner_sentence_bounds(),
            TextObjectKind::Around => self.get_a_sentence_bounds(),
        }
    }

    fn delete_sentence(&mut self, kind: TextObjectKind) {
        self.save_undo_state();
        self.lines_version += 1;

        let (start_row, start_col, end_row, end_col) = self.get_sentence_bounds(kind);

        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);
        self.yank_is_linewise = false;

        self.delete_char_range(start_row, start_col, end_row, end_col);

        // Around: remove empty line if there's a line after it
        if kind == TextObjectKind::Around
            && self.lines[start_row].is_empty()
            && start_row < self.lines.len() - 1
        {
            self.lines.remove(start_row);
            self.cursor = (start_row.min(self.lines.len() - 1), 0);
            let first_non_blank = self.get_first_non_blank_in_line(self.cursor.0);
            self.cursor.1 = first_non_blank;
        } else {
            self.cursor = (start_row.min(self.lines.len() - 1), start_col);
        }

        self.clamp_cursor();
        self.update_desired_col();
        self.maybe_record_change();
    }

    fn change_sentence(&mut self, kind: TextObjectKind) {
        self.save_undo_state();
        self.lines_version += 1;

        let (start_row, start_col, end_row, end_col) = self.get_sentence_bounds(kind);

        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);
        self.yank_is_linewise = false;
        self.delete_char_range(start_row, start_col, end_row, end_col);

        self.cursor = (start_row.min(self.lines.len() - 1), start_col);
        self.clamp_cursor();
        self.update_desired_col();
        self.mode = EditorMode::Insert;
    }

    fn yank_sentence(&mut self, kind: TextObjectKind) {
        let (start_row, start_col, end_row, end_col) = self.get_sentence_bounds(kind);

        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);

        if kind == TextObjectKind::Around {
            let end_line_len = self.lines[end_row].chars().count();
            self.yank_is_linewise =
                start_col == 0 && end_col + 1 >= end_line_len && end_row < self.lines.len() - 1;
        } else {
            self.yank_is_linewise = false;
        }

        self.cursor = (start_row, start_col);
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn get_matching_pair(open: char) -> Option<char> {
        match open {
            '(' => Some(')'),
            ')' => Some('('),
            '[' => Some(']'),
            ']' => Some('['),
            '{' => Some('}'),
            '}' => Some('{'),
            '<' => Some('>'),
            '>' => Some('<'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            _ => None,
        }
    }

    fn is_open_pair(c: char) -> bool {
        matches!(c, '(' | '[' | '{' | '<')
    }

    fn find_pair_bounds(&self, pair_char: char) -> Option<((usize, usize), (usize, usize))> {
        let (open, close) = if pair_char == '"' || pair_char == '\'' || pair_char == '`' {
            (pair_char, pair_char)
        } else if Self::is_open_pair(pair_char) {
            (pair_char, Self::get_matching_pair(pair_char)?)
        } else {
            (Self::get_matching_pair(pair_char)?, pair_char)
        };

        let cur_row = self.cursor.0;
        let line = &self.lines[cur_row];
        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));

        if open == close {
            let mut left = None;
            for i in (0..=col).rev() {
                if chars[i] == open {
                    left = Some(i);
                    break;
                }
            }

            let mut right = None;
            let start_search = if left == Some(col) { col + 1 } else { col };
            for i in start_search..chars.len() {
                if chars[i] == close {
                    right = Some(i);
                    break;
                }
            }

            match (left, right) {
                (Some(l), Some(r)) => Some(((cur_row, l), (cur_row, r))),
                _ => None,
            }
        } else {
            let cur_char = if !chars.is_empty() { chars[col] } else { ' ' };

            let open_pos: Option<(usize, usize)>;
            let close_pos: Option<(usize, usize)>;

            if cur_char == close {
                close_pos = Some((cur_row, col));
                open_pos = self.find_matching_open(open, close, cur_row, col);
            } else if cur_char == open {
                open_pos = Some((cur_row, col));
                close_pos = self.find_matching_close(open, close, cur_row, col);
            } else {
                open_pos = self.find_matching_open(open, close, cur_row, col + 1);
                if let Some((open_row, open_col)) = open_pos {
                    close_pos = self.find_matching_close(open, close, open_row, open_col);
                } else {
                    close_pos = None;
                }
            }

            match (open_pos, close_pos) {
                (Some(o), Some(c)) => Some((o, c)),
                _ => None,
            }
        }
    }

    fn find_matching_open(
        &self,
        open: char,
        close: char,
        start_row: usize,
        start_col: usize,
    ) -> Option<(usize, usize)> {
        let mut depth = 0i32;
        let mut row = start_row;
        let mut search_end = start_col;

        loop {
            let line = &self.lines[row];
            let chars: Vec<char> = line.chars().collect();
            let end = search_end.min(chars.len());

            for i in (0..end).rev() {
                if chars[i] == close {
                    depth += 1;
                } else if chars[i] == open {
                    if depth == 0 {
                        return Some((row, i));
                    }
                    depth -= 1;
                }
            }

            if row == 0 {
                break;
            }
            row -= 1;
            search_end = self.lines[row].chars().count();
        }
        None
    }

    fn find_matching_close(
        &self,
        open: char,
        close: char,
        start_row: usize,
        start_col: usize,
    ) -> Option<(usize, usize)> {
        let mut depth = 0i32;
        let mut row = start_row;
        let mut search_start = start_col + 1;

        loop {
            let line = &self.lines[row];
            let chars: Vec<char> = line.chars().collect();

            for i in search_start..chars.len() {
                if chars[i] == open {
                    depth += 1;
                } else if chars[i] == close {
                    if depth == 0 {
                        return Some((row, i));
                    }
                    depth -= 1;
                }
            }

            if row >= self.lines.len() - 1 {
                break;
            }
            row += 1;
            search_start = 0;
        }
        None
    }

    fn delete_inner_pair(&mut self, pair_char: char) {
        if let Some(((open_row, open_col), (close_row, close_col))) =
            self.find_pair_bounds(pair_char)
        {
            self.yank_inner_pair(pair_char);

            self.save_undo_state();
            self.lines_version += 1;
            if open_row == close_row {
                let line = &mut self.lines[open_row];
                if open_col + 1 < close_col {
                    line.replace_range((open_col + 1)..close_col, "");
                }
                self.cursor.0 = open_row;
                self.cursor.1 = open_col + 1;
            } else {
                let is_change = self.mode == EditorMode::Insert;
                let has_content_lines = close_row - open_row > 1;

                let first_line: String = self.lines[open_row].chars().take(open_col + 1).collect();
                self.lines[open_row] = first_line;

                let last_line: String = self.lines[close_row].chars().skip(close_col).collect();
                self.lines[close_row] = last_line;

                for _ in (open_row + 1)..close_row {
                    self.lines.remove(open_row + 1);
                }

                if is_change && has_content_lines {
                    self.lines.insert(open_row + 1, String::new());
                    self.cursor.0 = open_row + 1;
                    self.cursor.1 = 0;
                } else {
                    self.cursor.0 = open_row + 1;
                    self.cursor.1 = 0;
                }
            }
            self.clamp_cursor();
            self.update_desired_col();
            self.maybe_record_change();
        }
    }

    fn delete_around_pair(&mut self, pair_char: char) {
        if let Some(((open_row, open_col), (close_row, close_col))) =
            self.find_pair_bounds(pair_char)
        {
            self.yank_around_pair(pair_char);

            self.save_undo_state();
            self.lines_version += 1;
            if open_row == close_row {
                let line = &mut self.lines[open_row];
                line.replace_range(open_col..=close_col, "");
                self.cursor.0 = open_row;
                self.cursor.1 = open_col;
            } else {
                let first_line_prefix: String =
                    self.lines[open_row].chars().take(open_col).collect();
                let last_line_suffix: String =
                    self.lines[close_row].chars().skip(close_col + 1).collect();

                self.lines[open_row] = first_line_prefix + &last_line_suffix;

                for _ in (open_row + 1)..=close_row {
                    self.lines.remove(open_row + 1);
                }

                self.cursor.0 = open_row;
                self.cursor.1 = open_col;
            }
            self.clamp_cursor();
            self.update_desired_col();
            self.maybe_record_change();
        }
    }

    fn is_matchable_bracket(c: char) -> bool {
        matches!(c, '(' | ')' | '[' | ']' | '{' | '}')
    }

    fn jump_to_prev_unmatched(&mut self, open: char, close: char) {
        let mut depth = 0i32;
        let mut row = self.cursor.0;
        let mut start_col = self.cursor.1;

        loop {
            let line = &self.lines[row];
            let chars: Vec<char> = line.chars().collect();
            let search_end = if row == self.cursor.0 {
                start_col.min(chars.len())
            } else {
                chars.len()
            };

            for i in (0..search_end).rev() {
                if chars[i] == close {
                    depth += 1;
                } else if chars[i] == open {
                    if depth == 0 {
                        self.cursor.0 = row;
                        self.cursor.1 = i;
                        return;
                    }
                    depth -= 1;
                }
            }

            if row == 0 {
                break;
            }
            row -= 1;
            start_col = self.lines[row].len();
        }
    }

    fn jump_to_next_unmatched(&mut self, open: char, close: char) {
        let mut depth = 0i32;
        let mut row = self.cursor.0;
        let mut start_col = self.cursor.1;

        loop {
            let line = &self.lines[row];
            let chars: Vec<char> = line.chars().collect();
            let search_start = if row == self.cursor.0 {
                (start_col + 1).min(chars.len())
            } else {
                0
            };

            for i in search_start..chars.len() {
                if chars[i] == open {
                    depth += 1;
                } else if chars[i] == close {
                    if depth == 0 {
                        self.cursor.0 = row;
                        self.cursor.1 = i;
                        return;
                    }
                    depth -= 1;
                }
            }

            if row >= self.lines.len() - 1 {
                break;
            }
            row += 1;
            start_col = 0;
        }
    }

    fn jump_to_matching_bracket(&mut self) {
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return;
        }

        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));
        let cur_char = chars[col];

        if !Self::is_matchable_bracket(cur_char) {
            for i in col..chars.len() {
                if Self::is_matchable_bracket(chars[i]) {
                    self.cursor.1 = i;
                    self.jump_to_matching_bracket();
                    return;
                }
            }
            return;
        }

        let matching = match Self::get_matching_pair(cur_char) {
            Some(m) => m,
            None => return,
        };

        if Self::is_open_pair(cur_char) {
            let mut depth = 0i32;
            let mut row = self.cursor.0;
            let start_col = col;

            loop {
                let line = &self.lines[row];
                let chars: Vec<char> = line.chars().collect();
                let search_start = if row == self.cursor.0 {
                    start_col + 1
                } else {
                    0
                };

                for i in search_start..chars.len() {
                    if chars[i] == cur_char {
                        depth += 1;
                    } else if chars[i] == matching {
                        if depth == 0 {
                            self.cursor.0 = row;
                            self.cursor.1 = i;
                            return;
                        }
                        depth -= 1;
                    }
                }

                if row >= self.lines.len() - 1 {
                    break;
                }
                row += 1;
            }
        } else {
            let mut depth = 0i32;
            let mut row = self.cursor.0;
            let mut search_end = col;

            loop {
                let line = &self.lines[row];
                let chars: Vec<char> = line.chars().collect();
                let end = if row == self.cursor.0 {
                    search_end
                } else {
                    chars.len()
                };

                for i in (0..end).rev() {
                    if chars[i] == cur_char {
                        depth += 1;
                    } else if chars[i] == matching {
                        if depth == 0 {
                            self.cursor.0 = row;
                            self.cursor.1 = i;
                            return;
                        }
                        depth -= 1;
                    }
                }

                if row == 0 {
                    break;
                }
                row -= 1;
                search_end = self.lines[row].len();
            }
        }
    }

    fn get_paragraph_backward_pos(&self) -> (usize, usize) {
        let mut row = self.cursor.0;

        while row > 0 && self.lines[row].trim().is_empty() {
            row -= 1;
        }

        while row > 0 && !self.lines[row].trim().is_empty() {
            row -= 1;
        }

        (row, 0)
    }

    fn get_paragraph_forward_pos(&self) -> (usize, usize) {
        let mut row = self.cursor.0;
        let last_row = self.lines.len().saturating_sub(1);

        while row < last_row && self.lines[row].trim().is_empty() {
            row += 1;
        }
        while row < last_row && !self.lines[row].trim().is_empty() {
            row += 1;
        }

        if row == last_row && !self.lines[row].trim().is_empty() {
            (row, self.lines[row].chars().count())
        } else {
            (row, 0)
        }
    }

    fn move_paragraph_backward(&mut self) {
        let mut row = self.cursor.0;

        while row > 0 && self.lines[row].trim().is_empty() {
            row -= 1;
        }

        while row > 0 && !self.lines[row].trim().is_empty() {
            row -= 1;
        }

        self.cursor.0 = row;
        self.cursor.1 = 0;
        self.update_desired_col();
    }

    fn move_paragraph_forward(&mut self) {
        let mut row = self.cursor.0;
        let last_row = self.lines.len().saturating_sub(1);

        while row < last_row && self.lines[row].trim().is_empty() {
            row += 1;
        }
        while row < last_row && !self.lines[row].trim().is_empty() {
            row += 1;
        }

        self.cursor.0 = row;

        if row == last_row && !self.lines[row].trim().is_empty() {
            self.cursor.1 = self.lines[row].chars().count().saturating_sub(1);
        } else {
            self.cursor.1 = 0;
        }
        self.update_desired_col();
    }

    fn is_sentence_end_punct(c: char) -> bool {
        matches!(c, '.' | '!' | '?')
    }

    fn is_sentence_closing_char(c: char) -> bool {
        matches!(c, ')' | ']' | '"' | '\'')
    }

    /// Ends with '.', '!', or '?' optionally followed by closing chars, then whitespace or EOL
    fn is_valid_sentence_end(chars: &[char], col: usize) -> bool {
        if col >= chars.len() || !Self::is_sentence_end_punct(chars[col]) {
            return false;
        }
        let mut after_col = col + 1;
        while after_col < chars.len() && Self::is_sentence_closing_char(chars[after_col]) {
            after_col += 1;
        }
        after_col >= chars.len() || chars[after_col].is_whitespace()
    }

    fn get_sentence_backward_pos(&self) -> (usize, usize) {
        self.compute_sentence_backward_pos(self.cursor.0, self.cursor.1)
    }

    fn get_sentence_forward_pos(&self) -> (usize, usize) {
        self.compute_sentence_forward_pos(self.cursor.0, self.cursor.1)
    }

    fn compute_sentence_backward_pos(&self, start_row: usize, start_col: usize) -> (usize, usize) {
        let started_on_blank = self.lines[start_row].trim().is_empty();

        // Search backward from cursor position for sentence end or paragraph start
        let mut row = start_row;
        let mut col = if start_col > 0 {
            start_col - 1
        } else if start_row > 0 {
            row = start_row - 1;
            self.lines[row].chars().count().saturating_sub(1)
        } else {
            return (0, 0);
        };

        loop {
            // If we're on a blank line and we didn't start on a blank line, stop here
            if self.lines[row].trim().is_empty() {
                if !started_on_blank {
                    return (row, 0);
                }
                // Started on blank line, continue searching in previous paragraph
                if row > 0 {
                    row -= 1;
                    col = self.lines[row].chars().count().saturating_sub(1);
                    continue;
                } else {
                    return (0, 0);
                }
            }

            let chars: Vec<char> = self.lines[row].chars().collect();

            // Search backward through current line
            loop {
                if Self::is_valid_sentence_end(&chars, col) {
                    // Found sentence end - find where the sentence after this starts
                    if let Some((sent_row, sent_col)) =
                        self.find_sentence_start_after_pos(row, col + 1)
                    {
                        // Check if this sentence start is before our starting position
                        if sent_row < start_row || (sent_row == start_row && sent_col < start_col) {
                            return (sent_row, sent_col);
                        }
                    } else {
                        // This sentence ends at paragraph boundary (last sentence of paragraph)
                        // Find the start of this sentence
                        return self.find_sentence_start_for_end(row, col);
                    }
                }
                if col == 0 {
                    break;
                }
                col -= 1;
            }

            // Check if previous line is blank (paragraph boundary)
            if row > 0 {
                if self.lines[row - 1].trim().is_empty() {
                    // At paragraph boundary
                    let para_start_col = self.find_line_start(row);
                    if row == start_row && para_start_col == start_col {
                        return (row - 1, 0);
                    } else if row < start_row || (row == start_row && para_start_col < start_col) {
                        return (row, para_start_col);
                    } else {
                        return (row - 1, 0);
                    }
                }
                row -= 1;
                col = self.lines[row].chars().count().saturating_sub(1);
            } else {
                return (0, 0);
            }
        }
    }

    fn compute_sentence_forward_pos(&self, start_row: usize, start_col: usize) -> (usize, usize) {
        let mut row = start_row;
        let mut col = start_col;
        let last_row = self.lines.len().saturating_sub(1);

        // If starting on a blank line, skip to next paragraph's first sentence
        if self.lines[row].trim().is_empty() {
            while row < last_row && self.lines[row].trim().is_empty() {
                row += 1;
            }
            if self.lines[row].trim().is_empty() {
                return (last_row, 0);
            }
            let chars: Vec<char> = self.lines[row].chars().collect();
            let start_col = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0);
            return (row, start_col);
        }

        // Check if we're in whitespace/closing chars after a sentence end
        // If so, just skip to the next sentence start
        let chars: Vec<char> = self.lines[row].chars().collect();
        if col < chars.len()
            && (chars[col].is_whitespace() || Self::is_sentence_closing_char(chars[col]))
        {
            // Look backward to see if there's a sentence end before us
            let mut check_col = col;
            // Skip back past whitespace and closing chars
            while check_col > 0
                && (chars[check_col - 1].is_whitespace()
                    || Self::is_sentence_closing_char(chars[check_col - 1]))
            {
                check_col -= 1;
            }

            // Check if what's before is sentence-ending punctuation
            // Also check previous line if we're at the start of a line
            let prev_line_ends_sentence = if check_col == 0 && row > 0 {
                let prev_chars: Vec<char> = self.lines[row - 1].chars().collect();
                if !prev_chars.is_empty() {
                    // Find the last non-whitespace character on previous line
                    let mut prev_col = prev_chars.len() - 1;
                    while prev_col > 0 && prev_chars[prev_col].is_whitespace() {
                        prev_col -= 1;
                    }
                    // Skip closing chars
                    while prev_col > 0 && Self::is_sentence_closing_char(prev_chars[prev_col]) {
                        prev_col -= 1;
                    }
                    Self::is_sentence_end_punct(prev_chars[prev_col])
                } else {
                    false
                }
            } else {
                false
            };

            if (check_col > 0 && Self::is_sentence_end_punct(chars[check_col - 1]))
                || prev_line_ends_sentence
            {
                // We're in the whitespace after a sentence end, skip to next sentence start
                let mut c = col;
                while c < chars.len()
                    && (chars[c].is_whitespace() || Self::is_sentence_closing_char(chars[c]))
                {
                    c += 1;
                }
                if c < chars.len() {
                    return (row, c);
                }
                // Continue to next line
                if row < last_row {
                    let next_row = row + 1;
                    if self.lines[next_row].trim().is_empty() {
                        return (next_row, 0);
                    }
                    let next_chars: Vec<char> = self.lines[next_row].chars().collect();
                    let start = next_chars
                        .iter()
                        .position(|ch| !ch.is_whitespace())
                        .unwrap_or(0);
                    return (next_row, start);
                } else {
                    return (last_row, self.lines[last_row].chars().count());
                }
            }
        }

        loop {
            let chars: Vec<char> = self.lines[row].chars().collect();

            // Search forward through current line for sentence end
            while col < chars.len() {
                if Self::is_valid_sentence_end(&chars, col) {
                    // Found sentence end, skip closing chars and whitespace
                    col += 1;
                    while col < chars.len() && Self::is_sentence_closing_char(chars[col]) {
                        col += 1;
                    }

                    // Skip whitespace (including across lines)
                    loop {
                        let cur_chars: Vec<char> = self.lines[row].chars().collect();
                        while col < cur_chars.len()
                            && Self::is_sentence_closing_char(cur_chars[col])
                        {
                            col += 1;
                        }
                        while col < cur_chars.len() && cur_chars[col].is_whitespace() {
                            col += 1;
                        }
                        if col < cur_chars.len() {
                            return (row, col);
                        }
                        if row < last_row {
                            row += 1;
                            col = 0;
                            if self.lines[row].trim().is_empty() {
                                return (row, 0);
                            }
                        } else {
                            // End of file - return position past last character for exclusive motions
                            return (last_row, self.lines[last_row].chars().count());
                        }
                    }
                }
                col += 1;
            }

            // Move to next line
            if row < last_row {
                row += 1;
                col = 0;
                if self.lines[row].trim().is_empty() {
                    return (row, 0);
                }
            } else {
                // End of file - return position past last character for exclusive motions
                return (last_row, self.lines[last_row].chars().count());
            }
        }
    }

    fn find_sentence_start_after_pos(
        &self,
        from_row: usize,
        from_col: usize,
    ) -> Option<(usize, usize)> {
        let mut r = from_row;
        let mut c = from_col;
        loop {
            let chars: Vec<char> = self.lines[r].chars().collect();
            while c < chars.len() && Self::is_sentence_closing_char(chars[c]) {
                c += 1;
            }
            while c < chars.len() && chars[c].is_whitespace() {
                c += 1;
            }
            if c < chars.len() {
                return Some((r, c));
            }
            if r < self.lines.len() - 1 {
                r += 1;
                c = 0;
                if self.lines[r].trim().is_empty() {
                    return None;
                }
            } else {
                return None;
            }
        }
    }

    fn find_line_start(&self, row: usize) -> usize {
        let chars: Vec<char> = self.lines[row].chars().collect();
        chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0)
    }

    fn find_sentence_start_for_end(&self, end_row: usize, end_col: usize) -> (usize, usize) {
        let mut r = end_row;
        let mut c = if end_col > 0 {
            end_col - 1
        } else if end_row > 0 {
            r = end_row - 1;
            self.lines[r].chars().count().saturating_sub(1)
        } else {
            return (0, 0);
        };

        loop {
            let chars: Vec<char> = self.lines[r].chars().collect();
            loop {
                if Self::is_valid_sentence_end(&chars, c) {
                    if let Some(pos) = self.find_sentence_start_after_pos(r, c + 1) {
                        return pos;
                    }
                }
                if c == 0 {
                    break;
                }
                c -= 1;
            }

            if r > 0 {
                if self.lines[r - 1].trim().is_empty() {
                    return (r, self.find_line_start(r));
                }
                r -= 1;
                c = self.lines[r].chars().count().saturating_sub(1);
            } else {
                return (0, 0);
            }
        }
    }

    fn move_sentence_backward(&mut self) {
        let new_pos = self.get_sentence_backward_pos();
        self.cursor = new_pos;
        self.update_desired_col();
    }

    fn move_sentence_forward(&mut self) {
        let new_pos = self.get_sentence_forward_pos();
        self.cursor = new_pos;
        self.update_desired_col();
    }

    fn find_char_forward(&self, target: char) -> Option<usize> {
        let line = &self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        for i in (self.cursor.1 + 1)..chars.len() {
            if chars[i] == target {
                return Some(i);
            }
        }
        None
    }

    fn find_char_backward(&self, target: char) -> Option<usize> {
        let line = &self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        for i in (0..self.cursor.1).rev() {
            if chars[i] == target {
                return Some(i);
            }
        }
        None
    }

    fn move_to_char_forward(&mut self, target: char) {
        if let Some(pos) = self.find_char_forward(target) {
            self.cursor.1 = pos;
        }
    }

    fn move_to_char_backward(&mut self, target: char) {
        if let Some(pos) = self.find_char_backward(target) {
            self.cursor.1 = pos;
        }
    }

    fn move_till_char_forward(&mut self, target: char) {
        if let Some(pos) = self.find_char_forward(target) {
            if pos > 0 {
                self.cursor.1 = pos - 1;
            }
        }
    }

    fn move_till_char_backward(&mut self, target: char) {
        if let Some(pos) = self.find_char_backward(target) {
            self.cursor.1 = pos + 1;
        }
    }

    fn repeat_char_search(&mut self, reverse: bool) {
        if let Some((search_type, target)) = self.last_char_search {
            let effective_type = if reverse {
                match search_type {
                    'f' => 'F',
                    'F' => 'f',
                    't' => 'T',
                    'T' => 't',
                    _ => search_type,
                }
            } else {
                search_type
            };

            match effective_type {
                'f' => self.move_to_char_forward(target),
                'F' => self.move_to_char_backward(target),
                't' => {
                    let original_pos = self.cursor.1;
                    let line_len = self.lines[self.cursor.0].len();
                    if self.cursor.1 + 1 < line_len {
                        self.cursor.1 += 1;
                        if self.find_char_forward(target).is_some() {
                            self.move_till_char_forward(target);
                        } else {
                            self.cursor.1 = original_pos;
                        }
                    }
                }
                'T' => {
                    let original_pos = self.cursor.1;
                    if self.cursor.1 > 0 {
                        self.cursor.1 -= 1;
                        if self.find_char_backward(target).is_some() {
                            self.move_till_char_backward(target);
                        } else {
                            self.cursor.1 = original_pos;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn delete_to_char_forward(&mut self, target: char, inclusive: bool) {
        if let Some(pos) = self.find_char_forward(target) {
            let end_pos = if inclusive { pos } else { pos - 1 };
            if end_pos >= self.cursor.1 {
                let line = &mut self.lines[self.cursor.0];
                line.replace_range(self.cursor.1..=end_pos, "");
                self.clamp_cursor();
                self.record_change();
            }
        }
    }

    fn delete_to_char_backward(&mut self, target: char, inclusive: bool) {
        if let Some(pos) = self.find_char_backward(target) {
            let start_pos = if inclusive { pos } else { pos + 1 };
            if start_pos <= self.cursor.1 {
                let line = &mut self.lines[self.cursor.0];
                line.replace_range(start_pos..self.cursor.1, "");
                self.cursor.1 = start_pos;
                self.clamp_cursor();
                self.record_change();
            }
        }
    }

    fn delete_range_multiline(
        &mut self,
        start: (usize, usize),
        end: (usize, usize),
        inclusive: bool,
    ) {
        self.lines_version += 1;
        let (start_row, start_col) = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
            start
        } else {
            end
        };
        let (end_row, end_col) = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
            end
        } else {
            start
        };

        let actual_end_col = if inclusive { end_col + 1 } else { end_col };

        if start_row == end_row {
            let chars: Vec<char> = self.lines[start_row].chars().collect();
            let end_clamped = actual_end_col.min(chars.len());
            if start_col < end_clamped {
                self.yank_buffer = chars[start_col..end_clamped].iter().collect();
                self.yank_is_linewise = false;
                self.lines[start_row] = chars[..start_col]
                    .iter()
                    .chain(&chars[end_clamped..])
                    .collect();
            }
            self.cursor.0 = start_row;
            self.cursor.1 = start_col;
        } else {
            let mut yanked = String::new();
            let first_chars: Vec<char> = self.lines[start_row].chars().collect();
            yanked.extend(&first_chars[start_col..]);
            for row in (start_row + 1)..end_row {
                yanked.push('\n');
                yanked.push_str(&self.lines[row]);
            }
            if end_row > start_row {
                yanked.push('\n');
                let last_chars: Vec<char> = self.lines[end_row].chars().collect();
                let end_clamped = actual_end_col.min(last_chars.len());
                yanked.extend(&last_chars[..end_clamped]);
            }
            self.yank_buffer = yanked;
            self.yank_is_linewise = false;

            let first_part: String = self.lines[start_row].chars().take(start_col).collect();
            let last_part: String = self.lines[end_row].chars().skip(actual_end_col).collect();

            self.lines[start_row] = first_part + &last_part;

            for _ in (start_row + 1)..=end_row {
                self.lines.remove(start_row + 1);
            }

            self.cursor.0 = start_row;
            self.cursor.1 = start_col;
        }

        self.clamp_cursor();
        self.record_change();
    }

    fn delete_to_matching_bracket(&mut self) {
        self.save_undo_state();
        let start = (self.cursor.0, self.cursor.1);

        // Find the target position
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return;
        }
        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));
        let cur_char = chars[col];

        // Find a bracket if not on one
        let bracket_col = if Self::is_matchable_bracket(cur_char) {
            col
        } else {
            let mut found = None;
            for i in col..chars.len() {
                if Self::is_matchable_bracket(chars[i]) {
                    found = Some(i);
                    break;
                }
            }
            match found {
                Some(c) => c,
                None => return,
            }
        };

        let original_cursor = self.cursor;
        self.cursor.1 = bracket_col;

        // Find matching bracket position
        let bracket_char = self.lines[self.cursor.0].chars().nth(bracket_col).unwrap();
        let matching = match Self::get_matching_pair(bracket_char) {
            Some(m) => m,
            None => {
                self.cursor = original_cursor;
                return;
            }
        };

        let end_pos = if Self::is_open_pair(bracket_char) {
            self.find_matching_close(bracket_char, matching, self.cursor.0, bracket_col)
        } else {
            self.find_matching_open(matching, bracket_char, self.cursor.0, bracket_col)
        };

        self.cursor = original_cursor;

        if let Some(end) = end_pos {
            self.delete_range_multiline(start, end, true);
        }
    }

    fn delete_to_prev_unmatched(&mut self, open: char, close: char) {
        self.save_undo_state();
        let start = (self.cursor.0, self.cursor.1);
        if let Some(end) = self.find_unmatched_backward(open, close) {
            self.delete_range_multiline(end, start, false);
        }
    }

    fn delete_to_next_unmatched(&mut self, open: char, close: char) {
        self.save_undo_state();
        let start = (self.cursor.0, self.cursor.1);
        if let Some(end) = self.find_unmatched_forward(open, close) {
            self.delete_range_multiline(start, end, false);
        }
    }

    fn find_unmatched_backward(&self, open: char, close: char) -> Option<(usize, usize)> {
        let mut depth = 0i32;
        let mut row = self.cursor.0;
        let mut search_end = self.cursor.1;

        loop {
            let line = &self.lines[row];
            let chars: Vec<char> = line.chars().collect();
            let end = search_end.min(chars.len());

            for i in (0..end).rev() {
                if chars[i] == close {
                    depth += 1;
                } else if chars[i] == open {
                    if depth == 0 {
                        return Some((row, i));
                    }
                    depth -= 1;
                }
            }

            if row == 0 {
                break;
            }
            row -= 1;
            search_end = self.lines[row].len();
        }
        None
    }

    fn find_unmatched_forward(&self, open: char, close: char) -> Option<(usize, usize)> {
        let mut depth = 0i32;
        let mut row = self.cursor.0;
        let mut search_start = self.cursor.1 + 1;

        loop {
            let line = &self.lines[row];
            let chars: Vec<char> = line.chars().collect();

            for i in search_start..chars.len() {
                if chars[i] == open {
                    depth += 1;
                } else if chars[i] == close {
                    if depth == 0 {
                        return Some((row, i));
                    }
                    depth -= 1;
                }
            }

            if row >= self.lines.len() - 1 {
                break;
            }
            row += 1;
            search_start = 0;
        }
        None
    }

    fn repeat_last_change(&mut self) {
        let has_explicit_count = self.count_prefix.is_some();
        let explicit_count = self.take_count();
        let use_count = if has_explicit_count {
            self.last_count = explicit_count;
            explicit_count
        } else {
            self.last_count
        };

        match self.last_change.clone() {
            LastChange::None => {}
            LastChange::Delete(target) => self.execute_delete_target(&target, use_count),
            LastChange::Change(target) => {
                self.execute_change_target(&target, use_count);
                self.insert_saved_text();
            }
            LastChange::InsertText(text, style) => {
                self.save_undo_state();
                self.mode = EditorMode::Insert;
                match style {
                    InsertStyle::Before => {}
                    InsertStyle::After => {
                        if self.cursor.1 < self.lines[self.cursor.0].len() {
                            self.cursor.1 += 1;
                        }
                    }
                    InsertStyle::LineStart => {
                        self.cursor.1 = 0;
                        let line = &self.lines[self.cursor.0];
                        for (i, ch) in line.chars().enumerate() {
                            if !ch.is_whitespace() {
                                self.cursor.1 = i;
                                break;
                            }
                        }
                    }
                    InsertStyle::LineEnd => {
                        self.cursor.1 = self.lines[self.cursor.0].len();
                    }
                    InsertStyle::NewLineBelow => {
                        self.lines_version += 1;
                        self.lines.insert(self.cursor.0 + 1, String::new());
                        self.cursor.0 += 1;
                        self.cursor.1 = 0;
                    }
                    InsertStyle::NewLineAbove => {
                        self.lines_version += 1;
                        self.lines.insert(self.cursor.0, String::new());
                        self.cursor.1 = 0;
                    }
                }
                for c in text.chars() {
                    if c == '\n' {
                        self.insert_newline();
                    } else {
                        self.insert_char(c);
                    }
                }
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                }
                self.mode = EditorMode::Normal;
                self.record_change();
            }
            LastChange::ToggleCase => self.toggle_case(),
            LastChange::JoinLines => self.join_lines(),
            LastChange::ReplaceChar(c) => self.replace_char(c),
            LastChange::ReplaceMode(text) => {
                self.save_undo_state();
                self.mode = EditorMode::Replace;
                for c in text.chars() {
                    if c == '\n' {
                        self.insert_newline();
                    } else {
                        self.replace_char_at_cursor(c);
                    }
                }
                self.mode = EditorMode::Normal;
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                }
                self.record_change();
            }
            LastChange::IncrementNumber => self.increment_number(use_count),
            LastChange::DecrementNumber => self.decrement_number(use_count),
            LastChange::PasteAfter => {
                self.paste_after_count(use_count);
                self.update_desired_col();
            }
            LastChange::PasteBefore => {
                self.paste_before_count(use_count);
                self.update_desired_col();
            }
            LastChange::DeleteBlock(num_rows, col_width) => {
                self.delete_block_at_cursor(num_rows, col_width);
            }
            LastChange::ChangeBlock(num_rows, col_width, ref text) => {
                let text = text.clone();
                self.change_block_at_cursor(num_rows, col_width, &text);
            }
            LastChange::InsertBlock(num_rows, ref text) => {
                let text = text.clone();
                self.insert_block_at_cursor(num_rows, &text);
            }
            LastChange::AppendBlock(num_rows, col_offset, ref text) => {
                let text = text.clone();
                self.insert_block_at_cursor_with_offset(num_rows, col_offset, &text);
            }
        }
    }

    fn delete_block_at_cursor(&mut self, num_rows: usize, col_width: usize) {
        self.save_undo_state();
        self.lines_version += 1;

        let start_row = self.cursor.0;
        let start_col = self.cursor.1;
        let end_col = start_col + col_width;

        let mut yanked_lines = Vec::new();
        for i in 0..num_rows {
            let row = start_row + i;
            if row >= self.lines.len() {
                break;
            }
            let chars: Vec<char> = self.lines[row].chars().collect();
            let line_len = chars.len();
            let sel_start = start_col.min(line_len);
            let sel_end = end_col.min(line_len);
            if sel_start < sel_end {
                yanked_lines.push(chars[sel_start..sel_end].iter().collect::<String>());
            } else {
                yanked_lines.push(String::new());
            }
        }
        self.yank_buffer = yanked_lines.join("\n");
        self.yank_is_linewise = false;
        self.yank_is_block = true;

        for i in 0..num_rows {
            let row = start_row + i;
            if row >= self.lines.len() {
                break;
            }
            let chars: Vec<char> = self.lines[row].chars().collect();
            let line_len = chars.len();
            let sel_start = start_col.min(line_len);
            let sel_end = end_col.min(line_len);
            if sel_start < sel_end {
                let new_line: String = chars[..sel_start].iter().chain(&chars[sel_end..]).collect();
                self.lines[row] = new_line;
            }
        }

        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn change_block_at_cursor(&mut self, num_rows: usize, col_width: usize, text: &str) {
        self.save_undo_state();
        self.lines_version += 1;

        let start_row = self.cursor.0;
        let start_col = self.cursor.1;
        let end_col = start_col + col_width;

        for i in 0..num_rows {
            let row = start_row + i;
            if row >= self.lines.len() {
                break;
            }
            let mut chars: Vec<char> = self.lines[row].chars().collect();
            let line_len = chars.len();
            let sel_start = start_col.min(line_len);
            let sel_end = end_col.min(line_len);

            if sel_start < sel_end {
                chars.drain(sel_start..sel_end);
            }

            let mut offset = 0;
            for c in text.chars() {
                if c == '\n' {
                    continue;
                }
                chars.insert(sel_start + offset, c);
                offset += 1;
            }
            self.lines[row] = chars.into_iter().collect();
        }

        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn insert_block_at_cursor(&mut self, num_rows: usize, text: &str) {
        self.save_undo_state();
        self.lines_version += 1;

        let start_row = self.cursor.0;
        let insert_col = self.cursor.1;

        for i in 0..num_rows {
            let row = start_row + i;
            if row >= self.lines.len() {
                break;
            }
            let chars: Vec<char> = self.lines[row].chars().collect();

            if chars.len() < insert_col {
                continue;
            }

            let mut chars = chars;

            let mut offset = 0;
            for c in text.chars() {
                if c == '\n' {
                    continue;
                }
                chars.insert(insert_col + offset, c);
                offset += 1;
            }
            self.lines[row] = chars.into_iter().collect();
        }

        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn insert_block_at_cursor_with_offset(
        &mut self,
        num_rows: usize,
        col_offset: usize,
        text: &str,
    ) {
        self.save_undo_state();
        self.lines_version += 1;

        let start_row = self.cursor.0;
        let insert_col = self.cursor.1 + col_offset;

        for i in 0..num_rows {
            let row = start_row + i;
            if row >= self.lines.len() {
                break;
            }
            let mut chars: Vec<char> = self.lines[row].chars().collect();

            while chars.len() < insert_col {
                chars.push(' ');
            }

            let mut offset = 0;
            for c in text.chars() {
                if c == '\n' {
                    continue;
                }
                chars.insert(insert_col + offset, c);
                offset += 1;
            }
            self.lines[row] = chars.into_iter().collect();
        }

        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn execute_delete_target(&mut self, target: &EditTarget, count: usize) {
        match target {
            EditTarget::Char => {
                self.save_undo_state();
                for _ in 0..count {
                    self.delete_char_no_undo();
                }
                self.record_change();
            }
            EditTarget::CharBackward => {
                self.save_undo_state();
                for _ in 0..count {
                    if self.cursor.1 > 0 {
                        self.cursor.1 -= 1;
                        self.delete_char_no_undo();
                    }
                }
                self.record_change();
            }
            EditTarget::Line => self.delete_lines(count),
            EditTarget::WordStart(wt, dir) => match dir {
                Direction::Forward => {
                    self.perform_delete_motion_with_count(
                        |s| s.get_word_forward_pos(*wt),
                        count,
                        false,
                        true,
                        false,
                    );
                }
                Direction::Backward => {
                    self.perform_delete_motion_with_count(
                        |s| s.get_word_backward_pos(*wt),
                        count,
                        false,
                        true,
                        false,
                    );
                }
            },
            EditTarget::WordEnd(wt, dir) => match dir {
                Direction::Forward => {
                    self.perform_delete_motion_with_count(
                        |s| s.get_word_end_pos(*wt),
                        count,
                        true,
                        true,
                        false,
                    );
                }
                Direction::Backward => {
                    self.perform_delete_motion_with_count(
                        |s| s.get_word_end_backward_pos(*wt),
                        count,
                        true,
                        true,
                        false,
                    );
                }
            },
            EditTarget::ToEndOfLine => self.delete_to_end_of_line(),
            EditTarget::Inner(obj) => match obj {
                TextObject::Word(wt) => match wt {
                    WordType::Word => self.delete_inner_word(),
                    WordType::LongWord => self.delete_inner_long_word(),
                },
                TextObject::Pair(c) => self.delete_inner_pair(*c),
                TextObject::Paragraph => self.delete_paragraph(TextObjectKind::Inner),
                TextObject::Sentence => self.delete_sentence(TextObjectKind::Inner),
            },
            EditTarget::Around(obj) => match obj {
                TextObject::Word(wt) => match wt {
                    WordType::Word => self.delete_a_word(),
                    WordType::LongWord => self.delete_a_long_word(),
                },
                TextObject::Pair(c) => self.delete_around_pair(*c),
                TextObject::Paragraph => self.delete_paragraph(TextObjectKind::Around),
                TextObject::Sentence => self.delete_sentence(TextObjectKind::Around),
            },
            EditTarget::ToChar(c, search_type, dir) => {
                let inclusive = *search_type == CharSearchType::Find;
                match dir {
                    Direction::Forward => self.delete_to_char_forward(*c, inclusive),
                    Direction::Backward => self.delete_to_char_backward(*c, inclusive),
                }
            }
        }
    }

    fn execute_change_target(&mut self, target: &EditTarget, count: usize) {
        match target {
            EditTarget::Char => {
                self.substitute_char();
            }
            EditTarget::CharBackward => {
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                    self.substitute_char();
                }
            }
            EditTarget::Line => {
                self.substitute_lines(count);
            }
            EditTarget::WordStart(wt, dir) => {
                self.mode = EditorMode::Insert;
                match dir {
                    Direction::Forward => {
                        let line = &self.lines[self.cursor.0];
                        let chars: Vec<char> = line.chars().collect();
                        let on_whitespace =
                            self.cursor.1 < chars.len() && chars[self.cursor.1].is_whitespace();
                        if on_whitespace {
                            self.perform_delete_motion_with_count(
                                |s| s.get_word_forward_pos(*wt),
                                count,
                                false,
                                false,
                                false,
                            );
                        } else {
                            self.perform_delete_motion_with_count(
                                |s| s.get_word_end_pos(*wt),
                                count,
                                true,
                                false,
                                false,
                            );
                        }
                    }
                    Direction::Backward => {
                        self.perform_delete_motion_with_count(
                            |s| s.get_word_backward_pos(*wt),
                            count,
                            false,
                            false,
                            false,
                        );
                    }
                }
            }
            EditTarget::WordEnd(wt, dir) => {
                self.mode = EditorMode::Insert;
                match dir {
                    Direction::Forward => {
                        self.perform_delete_motion_with_count(
                            |s| s.get_word_end_pos(*wt),
                            count,
                            true,
                            false,
                            false,
                        );
                    }
                    Direction::Backward => {
                        self.perform_delete_motion_with_count(
                            |s| s.get_word_end_backward_pos(*wt),
                            count,
                            true,
                            false,
                            false,
                        );
                    }
                }
            }
            EditTarget::ToEndOfLine => {
                self.change_to_end_of_line();
            }
            EditTarget::Inner(obj) => match obj {
                TextObject::Word(wt) => {
                    self.mode = EditorMode::Insert;
                    match wt {
                        WordType::Word => self.delete_inner_word(),
                        WordType::LongWord => self.delete_inner_long_word(),
                    }
                }
                TextObject::Pair(c) => {
                    self.mode = EditorMode::Insert;
                    self.delete_inner_pair(*c);
                }
                TextObject::Paragraph => {
                    self.change_paragraph(TextObjectKind::Inner);
                }
                TextObject::Sentence => {
                    self.change_sentence(TextObjectKind::Inner);
                }
            },
            EditTarget::Around(obj) => match obj {
                TextObject::Word(wt) => {
                    self.mode = EditorMode::Insert;
                    match wt {
                        WordType::Word => self.delete_a_word(),
                        WordType::LongWord => self.delete_a_long_word(),
                    }
                }
                TextObject::Pair(c) => {
                    self.mode = EditorMode::Insert;
                    self.delete_around_pair(*c);
                }
                TextObject::Paragraph => {
                    self.change_paragraph(TextObjectKind::Around);
                }
                TextObject::Sentence => {
                    self.change_sentence(TextObjectKind::Around);
                }
            },
            EditTarget::ToChar(c, search_type, dir) => {
                self.mode = EditorMode::Insert;
                let inclusive = *search_type == CharSearchType::Find;
                match dir {
                    Direction::Forward => self.delete_to_char_forward(*c, inclusive),
                    Direction::Backward => self.delete_to_char_backward(*c, inclusive),
                }
            }
        }
    }

    fn insert_saved_text(&mut self) {
        let text = self.insert_buffer.clone();
        for c in text.chars() {
            self.insert_char(c);
        }
        self.mode = EditorMode::Normal;
        if self.cursor.1 > 0 {
            self.cursor.1 -= 1;
        }
        self.clamp_cursor();
    }

    fn delete_to_end_of_line(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 < chars.len() {
            self.yank_buffer = chars[self.cursor.1..].iter().collect();
            self.yank_is_linewise = false;
            self.lines[self.cursor.0] = chars[..self.cursor.1].iter().collect();
        }
        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn change_to_end_of_line(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;

        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 < chars.len() {
            self.yank_buffer = chars[self.cursor.1..].iter().collect();
            self.yank_is_linewise = false;
            self.lines[self.cursor.0] = chars[..self.cursor.1].iter().collect();
        }

        self.mode = EditorMode::Insert;
        let line_len = self.lines[self.cursor.0].len();
        self.cursor.1 = line_len;
    }

    fn substitute_line(&mut self) {
        self.substitute_lines(1);
    }

    fn substitute_lines(&mut self, count: usize) {
        self.save_undo_state();
        self.lines_version += 1;

        let line = &self.lines[self.cursor.0];
        let mut indent = String::new();
        for ch in line.chars() {
            if ch.is_whitespace() {
                indent.push(ch);
            } else {
                break;
            }
        }

        let end_row = (self.cursor.0 + count).min(self.lines.len());
        let actual_count = end_row - self.cursor.0;

        let yanked: Vec<&str> = self.lines[self.cursor.0..end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        for _ in 0..actual_count {
            if self.cursor.0 < self.lines.len() {
                self.lines.remove(self.cursor.0);
            }
        }
        let indent_len = indent.len();
        self.lines.insert(self.cursor.0, indent);

        self.cursor.1 = indent_len;
        self.mode = EditorMode::Insert;
    }

    fn substitute_char(&mut self) {
        let line_len = self.lines[self.cursor.0].len();
        if line_len > 0 && self.cursor.1 < line_len {
            self.save_undo_state();
            self.lines_version += 1;
            self.lines[self.cursor.0].remove(self.cursor.1);
            self.mode = EditorMode::Insert;
        }
    }

    fn replace_char(&mut self, replacement: char) {
        let line_len = self.lines[self.cursor.0].len();
        if line_len > 0 && self.cursor.1 < line_len {
            self.save_undo_state();
            self.lines_version += 1;
            let line = &mut self.lines[self.cursor.0];
            let mut chars: Vec<char> = line.chars().collect();
            chars[self.cursor.1] = replacement;
            *line = chars.into_iter().collect();
            self.record_change();
        }
    }

    fn join_lines(&mut self) {
        if self.cursor.0 < self.lines.len() - 1 {
            self.save_undo_state();
            self.lines_version += 1;
            let next_line = self.lines.remove(self.cursor.0 + 1);
            let current_line = &mut self.lines[self.cursor.0];

            let mut needs_space = true;
            if current_line.ends_with(' ') || next_line.starts_with(')') {
                needs_space = false;
            }
            if current_line.is_empty() {
                needs_space = false;
            }

            let next_line_trimmed = next_line.trim_start();

            self.cursor.1 = current_line.len();
            if needs_space {
                current_line.push(' ');
                self.cursor.1 += 0;
            }

            current_line.push_str(next_line_trimmed);
            self.record_change();
        }
    }

    fn toggle_case(&mut self) {
        let line = &mut self.lines[self.cursor.0];
        if let Some(ch) = line.chars().nth(self.cursor.1) {
            let new_ch = if ch.is_lowercase() {
                ch.to_uppercase().next().unwrap()
            } else {
                ch.to_lowercase().next().unwrap()
            };

            let mut chars: Vec<char> = line.chars().collect();
            chars[self.cursor.1] = new_ch;
            *line = chars.into_iter().collect();

            self.move_cursor(0, 1);
            self.record_change();
        }
    }

    fn find_number_at_cursor(&self) -> Option<NumberAtCursor> {
        let line = &self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return None;
        }

        let pos = self.cursor.1.min(chars.len() - 1);

        self.try_find_hex_number(&chars, pos)
            .or_else(|| self.try_find_decimal_number(&chars, pos))
    }

    fn try_find_hex_number(&self, chars: &[char], pos: usize) -> Option<NumberAtCursor> {
        let find_hex_start = |from: usize| -> Option<usize> {
            let mut i = from;
            while i > 0 && chars[i - 1].is_ascii_hexdigit() {
                i -= 1;
            }
            if i >= 2 && chars[i - 1].to_ascii_lowercase() == 'x' && chars[i - 2] == '0' {
                Some(i - 2)
            } else if i >= 1 && chars[i].to_ascii_lowercase() == 'x' && chars[i - 1] == '0' {
                Some(i - 1)
            } else {
                None
            }
        };

        let hex_start = if chars[pos].is_ascii_hexdigit() {
            find_hex_start(pos)
        } else if chars[pos].to_ascii_lowercase() == 'x' && pos > 0 && chars[pos - 1] == '0' {
            Some(pos - 1)
        } else {
            None
        };

        let hex_start = hex_start?;

        let mut end = hex_start + 2;
        while end < chars.len() && chars[end].is_ascii_hexdigit() {
            end += 1;
        }

        let digits: String = chars[(hex_start + 2)..end].iter().collect();
        if digits.is_empty() {
            return None;
        }

        let is_negative = hex_start > 0 && chars[hex_start - 1] == '-';
        let start = if is_negative {
            hex_start - 1
        } else {
            hex_start
        };

        Some(NumberAtCursor {
            start,
            end,
            digits,
            is_hex: true,
            is_negative,
        })
    }

    fn try_find_decimal_number(&self, chars: &[char], mut pos: usize) -> Option<NumberAtCursor> {
        let mut is_negative = false;

        if !chars[pos].is_ascii_digit() {
            if chars[pos] == '-' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit() {
                is_negative = true;
                pos += 1;
            } else {
                let found_pos = (pos..chars.len()).find(|&i| chars[i].is_ascii_digit());
                match found_pos {
                    Some(i) => {
                        is_negative = i > 0 && chars[i - 1] == '-';
                        pos = i;
                    }
                    None => return None,
                }
            }
        }

        if self.try_find_hex_number(chars, pos).is_some() {
            return None;
        }

        let mut num_start = pos;
        while num_start > 0 && chars[num_start - 1].is_ascii_digit() {
            num_start -= 1;
        }

        if !is_negative && num_start > 0 && chars[num_start - 1] == '-' {
            is_negative = true;
        }

        let mut end = pos;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }

        let digits: String = chars[num_start..end].iter().collect();
        if digits.is_empty() {
            return None;
        }

        let start = if is_negative {
            num_start - 1
        } else {
            num_start
        };

        Some(NumberAtCursor {
            start,
            end,
            digits,
            is_hex: false,
            is_negative,
        })
    }

    fn increment_number(&mut self, count: usize) {
        self.modify_number(count as i64);
    }

    fn decrement_number(&mut self, count: usize) {
        self.modify_number(-(count as i64));
    }

    fn modify_number(&mut self, delta: i64) {
        let Some(num) = self.find_number_at_cursor() else {
            return;
        };

        let new_num_str = if num.is_hex {
            self.format_hex_number(&num.digits, delta)
        } else {
            self.format_decimal_number(&num.digits, num.is_negative, delta)
        };

        self.save_undo_state();
        self.lines_version += 1;
        let line = &mut self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        let before: String = chars[..num.start].iter().collect();
        let after: String = chars[num.end..].iter().collect();
        *line = format!("{}{}{}", before, new_num_str, after);

        let new_end = num.start + new_num_str.chars().count();
        self.cursor.1 = new_end.saturating_sub(1);
        self.update_desired_col();
        self.record_change();
    }

    fn format_hex_number(&self, digits: &str, delta: i64) -> String {
        let width = digits.len();
        let parsed = u64::from_str_radix(digits, 16).unwrap_or(0);
        let new_value = if delta >= 0 {
            parsed.wrapping_add(delta as u64)
        } else {
            parsed.wrapping_sub((-delta) as u64)
        };
        format!("0x{:0>width$x}", new_value, width = width)
    }

    fn format_decimal_number(&self, digits: &str, is_negative: bool, delta: i64) -> String {
        let parsed = digits.parse::<i64>().unwrap_or(0);
        let value = if is_negative { -parsed } else { parsed };
        let new_value = value + delta;

        let has_leading_zeros = digits.len() > 1 && digits.starts_with('0');

        if has_leading_zeros {
            let width = digits.len();
            if new_value >= 0 {
                format!("{:0>width$}", new_value, width = width)
            } else {
                format!("-{:0>width$}", -new_value, width = width)
            }
        } else {
            format!("{}", new_value)
        }
    }

    fn render(&mut self) -> termwiz::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        self.buf.add_changes(vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorVisibility(CursorVisibility::Hidden),
        ]);

        let mode_text_raw = match self.mode {
            EditorMode::Normal => &self.colors.normal_mode_text,
            EditorMode::Insert => &self.colors.insert_mode_text,
            EditorMode::Replace => &self.colors.replace_mode_text,
            EditorMode::Search => &self.colors.command_mode_text,
            EditorMode::Visual => &self.colors.visual_mode_text,
            EditorMode::VisualLine => &self.colors.visual_line_mode_text,
            EditorMode::VisualBlock => &self.colors.visual_block_mode_text,
        };
        let mode_text = format!(" {} ", mode_text_raw);
        let mode_len = mode_text_raw.len() + 2;

        let mut pending_str = String::new();
        if let Some(count) = self.count_prefix {
            pending_str.push_str(&count.to_string());
        }
        if let Some(op) = self.pending_operator {
            pending_str.push(op);
        }
        for key in &self.pending_keys {
            if let KeyCode::Char(c) = key {
                pending_str.push(*c);
            }
        }

        let (mode_fg, mode_bg) = match self.mode {
            EditorMode::Normal => (self.colors.normal_mode_fg, self.colors.normal_mode_bg),
            EditorMode::Insert => (self.colors.insert_mode_fg, self.colors.insert_mode_bg),
            EditorMode::Replace => (self.colors.replace_mode_fg, self.colors.replace_mode_bg),
            EditorMode::Search => (self.colors.command_mode_fg, self.colors.command_mode_bg),
            EditorMode::Visual | EditorMode::VisualLine | EditorMode::VisualBlock => {
                (self.colors.visual_mode_fg, self.colors.visual_mode_bg)
            }
        };

        let line_is_empty = self.lines[self.cursor.0].is_empty();
        let buffer_is_empty = self.lines.len() == 1 && self.lines[0].is_empty();
        let row_display = if buffer_is_empty {
            "0".to_string()
        } else {
            (self.cursor.0 + 1).to_string()
        };
        let col_display = if line_is_empty && self.mode == EditorMode::Normal {
            "0-1".to_string()
        } else {
            (self.cursor.1 + 1).to_string()
        };
        let position_text = format!("{},{}", row_display, col_display);
        let position_padding = POSITION_WIDTH.saturating_sub(position_text.len());
        let position = format!("{}{}", position_text, " ".repeat(position_padding));
        let middle_width = cols.saturating_sub(mode_len + POSITION_WIDTH);

        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(rows - 2),
            },
            Change::Attribute(AttributeChange::Background(mode_bg)),
            Change::Attribute(AttributeChange::Foreground(mode_fg)),
            Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
            Change::Text(mode_text),
            Change::Attribute(AttributeChange::Intensity(Intensity::Normal)),
            Change::Attribute(AttributeChange::Background(self.colors.status_bg)),
            Change::Attribute(AttributeChange::Foreground(self.colors.status_fg)),
            Change::Text(format!("{:width$}", "", width = middle_width)),
            Change::Text(position),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        if self.mode == EditorMode::Search {
            let prompt = match self.search_direction {
                Direction::Forward => "/",
                Direction::Backward => "?",
            };
            let search_text = format!("{}{}", prompt, self.search_input);
            self.buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(rows - 1),
                },
                Change::Text(format!("{:<width$}", search_text, width = cols)),
                Change::AllAttributes(CellAttributes::default()),
            ]);
        } else {
            let search_display = if self.search_pattern.is_empty() {
                String::new()
            } else {
                let prompt = if self.search_display_direction == Direction::Forward {
                    "/"
                } else {
                    "?"
                };
                format!("{}{}", prompt, self.search_pattern)
            };

            let pending_with_padding = if pending_str.is_empty() {
                String::new()
            } else {
                let padding = PENDING_KEYS_PADDING.saturating_sub(pending_str.len());
                format!("{}{}", pending_str, " ".repeat(padding))
            };
            let left_width = cols.saturating_sub(pending_with_padding.len());

            self.buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(rows - 1),
                },
                Change::Text(format!(
                    "{:<left$}{}",
                    search_display,
                    pending_with_padding,
                    left = left_width
                )),
                Change::AllAttributes(CellAttributes::default()),
            ]);
        }

        let content_start_row;

        if let Some(title) = &self.args.title {
            self.buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(0),
                },
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Text(title.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            content_start_row = 1;
        } else {
            content_start_row = 0;
        }

        let content_rows = rows.saturating_sub(RESERVED_ROWS + content_start_row);
        let content_width = cols.saturating_sub(GUTTER_WIDTH);

        let mut wrap_row_offset: usize = 0;

        if content_width > 0 {
            let cursor_line_chars = self.lines[self.cursor.0].chars().count();
            let cursor_line_visual_rows = Self::wrapped_line_rows(cursor_line_chars, content_width);
            let (cursor_row_in_line, _) =
                Self::cursor_visual_position(self.cursor.1, content_width);

            if self.cursor.0 < self.viewport_top {
                self.viewport_top = self.cursor.0;
            }

            if cursor_line_visual_rows <= content_rows {
                loop {
                    let mut visual_rows_before_cursor = 0;
                    for line_idx in self.viewport_top..self.cursor.0 {
                        let char_count = self.lines[line_idx].chars().count();
                        visual_rows_before_cursor +=
                            Self::wrapped_line_rows(char_count, content_width);
                    }

                    let cursor_line_end_row = visual_rows_before_cursor + cursor_line_visual_rows;
                    if cursor_line_end_row <= content_rows {
                        break;
                    } else {
                        self.viewport_top += 1;
                        if self.viewport_top > self.cursor.0 {
                            self.viewport_top = self.cursor.0;
                            break;
                        }
                    }
                }
            } else {
                self.viewport_top = self.cursor.0;

                if cursor_row_in_line >= content_rows {
                    let margin = content_rows / 3;
                    wrap_row_offset = cursor_row_in_line.saturating_sub(margin);
                }
            }
        }

        let selection = if self.mode == EditorMode::Visual
            || self.mode == EditorMode::VisualLine
            || self.mode == EditorMode::VisualBlock
        {
            let (start, end) = if self.visual_start.0 < self.cursor.0
                || (self.visual_start.0 == self.cursor.0 && self.visual_start.1 <= self.cursor.1)
            {
                (self.visual_start, self.cursor)
            } else {
                (self.cursor, self.visual_start)
            };
            Some((start, end))
        } else {
            None
        };

        let block_col_bounds = if self.mode == EditorMode::VisualBlock {
            let min_col = self.visual_start.1.min(self.cursor.1);
            let max_col = self.visual_start.1.max(self.cursor.1);
            Some((min_col, max_col))
        } else {
            None
        };

        let mut cursor_screen_row: Option<usize> = None;
        let mut cursor_screen_col: Option<usize> = None;

        let mut visual_row = 0;
        let mut line_idx = self.viewport_top;

        while visual_row < content_rows && line_idx < self.lines.len() {
            let chars: Vec<char> = self.lines[line_idx].chars().collect();
            let line_len = chars.len();
            let line_visual_rows = Self::wrapped_line_rows(line_len, content_width);

            let start_wrap_row = if line_idx == self.viewport_top {
                wrap_row_offset
            } else {
                0
            };

            for wrap_row in start_wrap_row..line_visual_rows {
                if visual_row >= content_rows {
                    break;
                }

                let start_col = wrap_row * content_width;
                let end_col = ((wrap_row + 1) * content_width).min(line_len);

                let line_number_text = if start_wrap_row > 0 && wrap_row == start_wrap_row {
                    LINE_CONTINUES_ABOVE.to_string()
                } else if wrap_row == 0 {
                    if line_idx == self.cursor.0 {
                        format!("  {:<3} ", self.cursor.0 + 1)
                    } else {
                        let rel_num = (line_idx as isize - self.cursor.0 as isize).unsigned_abs();
                        format!("  {:>3} ", rel_num)
                    }
                } else {
                    EMPTY_GUTTER.to_string()
                };

                self.buf.add_changes(vec![
                    Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(content_start_row + visual_row),
                    },
                    Change::Attribute(AttributeChange::Foreground(self.colors.line_number_fg)),
                    Change::Text(line_number_text),
                    Change::AllAttributes(CellAttributes::default()),
                ]);

                if line_idx == self.cursor.0 {
                    let (cursor_wrap_row, col_in_row) =
                        Self::cursor_visual_position(self.cursor.1, content_width);
                    if wrap_row == cursor_wrap_row {
                        cursor_screen_row = Some(content_start_row + visual_row);
                        cursor_screen_col = Some(GUTTER_WIDTH + col_in_row);
                    }
                }

                let segment: String = if line_len == 0 && wrap_row == 0 {
                    String::new()
                } else {
                    chars[start_col..end_col].iter().collect()
                };

                let line_in_selection = selection
                    .map(|(sel_start, sel_end)| line_idx >= sel_start.0 && line_idx <= sel_end.0)
                    .unwrap_or(false);

                if line_in_selection && self.mode == EditorMode::VisualLine {
                    self.buf.add_changes(vec![
                        Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                        Change::Attribute(AttributeChange::Foreground(self.colors.selection_fg)),
                        Change::Text(if segment.is_empty() && wrap_row == 0 {
                            " ".to_string()
                        } else {
                            segment.clone()
                        }),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);
                } else if line_in_selection && self.mode == EditorMode::VisualBlock {
                    let (min_col, max_col) = block_col_bounds.unwrap();
                    let (sel_start, sel_end) = selection.unwrap();
                    let is_edge_line = line_idx == sel_start.0 || line_idx == sel_end.0;
                    self.render_wrapped_segment_with_block_selection(
                        &chars,
                        start_col,
                        end_col,
                        min_col,
                        max_col,
                        is_edge_line,
                    );
                } else if line_in_selection && self.mode == EditorMode::Visual {
                    let (sel_start, sel_end) = selection.unwrap();
                    self.render_wrapped_segment_with_selection(
                        &chars, start_col, end_col, line_idx, sel_start, sel_end,
                    );
                } else {
                    self.render_segment_with_search_highlight(&segment, line_idx, start_col);
                }

                visual_row += 1;
            }

            line_idx += 1;
        }

        let (cursor_screen_x, cursor_screen_y, cursor_shape) = if self.mode == EditorMode::Search {
            let x = 1 + self.search_input.len();
            (x, rows - 1, CursorShape::SteadyBlock)
        } else {
            let y = cursor_screen_row.unwrap_or(content_start_row);
            let x = cursor_screen_col.unwrap_or(GUTTER_WIDTH);
            let shape = if self.pending_operator.is_some() {
                CursorShape::SteadyUnderline
            } else if self.pending_keys.contains(&KeyCode::Char('r')) {
                CursorShape::SteadyUnderline
            } else {
                match self.mode {
                    EditorMode::Normal => CursorShape::SteadyBlock,
                    EditorMode::Insert => CursorShape::SteadyBar,
                    EditorMode::Replace => CursorShape::SteadyUnderline,
                    EditorMode::Search => CursorShape::SteadyBar,
                    EditorMode::Visual | EditorMode::VisualLine | EditorMode::VisualBlock => {
                        CursorShape::SteadyBlock
                    }
                }
            };
            (x, y, shape)
        };

        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(cursor_screen_x),
                y: Position::Absolute(cursor_screen_y),
            },
            Change::CursorVisibility(CursorVisibility::Visible),
            Change::CursorShape(cursor_shape),
        ]);

        self.buf.flush()?;

        Ok(())
    }

    fn render_segment_with_search_highlight(
        &mut self,
        segment: &str,
        line_idx: usize,
        start_col: usize,
    ) {
        if !self.search_highlight || self.search_pattern.is_empty() {
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(segment.to_string()),
            ]);
            return;
        }

        let line = &self.lines[line_idx];
        let pattern = &self.search_pattern;
        let is_current_line = self.current_match.map_or(false, |(r, _)| r == line_idx);
        let end_col = start_col + segment.chars().count();

        let mut matches: Vec<(usize, usize)> = Vec::new();
        let mut search_start = 0;
        while let Some(pos) = line[search_start..].find(pattern) {
            let match_start = search_start + pos;
            let match_end = match_start + pattern.len();
            // Check if this match overlaps with our segment
            if match_end > start_col && match_start < end_col {
                matches.push((match_start, match_end));
            }
            search_start = match_end;
            if search_start >= line.len() {
                break;
            }
        }

        if matches.is_empty() {
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(segment.to_string()),
            ]);
            return;
        }

        // Render segment with highlighted matches
        let segment_chars: Vec<char> = segment.chars().collect();
        let mut last_pos = 0;

        for (match_start, match_end) in matches {
            // Calculate positions relative to segment
            let seg_match_start = match_start.saturating_sub(start_col);
            let seg_match_end = (match_end.saturating_sub(start_col)).min(segment_chars.len());

            // Text before match (within segment)
            if seg_match_start > last_pos {
                let before: String = segment_chars[last_pos..seg_match_start].iter().collect();
                self.buf.add_changes(vec![
                    Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                    Change::Text(before),
                ]);
            }

            let is_current = is_current_line
                && self
                    .current_match
                    .map_or(false, |(_, c)| c >= match_start && c < match_end);

            let actual_start = seg_match_start.max(last_pos);
            if actual_start < seg_match_end {
                let matched: String = segment_chars[actual_start..seg_match_end].iter().collect();
                let (match_bg, match_fg) = if is_current {
                    (
                        self.colors.search_current_match_bg,
                        self.colors.search_current_match_fg,
                    )
                } else {
                    (self.colors.search_match_bg, self.colors.search_match_fg)
                };
                self.buf.add_changes(vec![
                    Change::Attribute(AttributeChange::Background(match_bg)),
                    Change::Attribute(AttributeChange::Foreground(match_fg)),
                    Change::Text(matched),
                    Change::AllAttributes(CellAttributes::default()),
                ]);
            }

            last_pos = seg_match_end;
        }

        if last_pos < segment_chars.len() {
            let after: String = segment_chars[last_pos..].iter().collect();
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(after),
            ]);
        }
    }

    fn render_wrapped_segment_with_selection(
        &mut self,
        chars: &[char],
        start_col: usize,
        end_col: usize,
        line_idx: usize,
        sel_start: (usize, usize),
        sel_end: (usize, usize),
    ) {
        let line_len = chars.len();

        let sel_col_start = if line_idx == sel_start.0 {
            sel_start.1
        } else {
            0
        };
        let sel_col_end = if line_idx == sel_end.0 {
            sel_end.1 + 1
        } else {
            line_len
        };

        let seg_sel_start = sel_col_start.max(start_col).min(end_col);
        let seg_sel_end = sel_col_end.max(start_col).min(end_col);

        if start_col < seg_sel_start {
            let before: String = chars[start_col..seg_sel_start].iter().collect();
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(before),
            ]);
        }

        if seg_sel_start < seg_sel_end {
            let selected: String = chars[seg_sel_start..seg_sel_end].iter().collect();
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                Change::Attribute(AttributeChange::Foreground(self.colors.selection_fg)),
                Change::Text(selected),
                Change::AllAttributes(CellAttributes::default()),
            ]);
        } else if chars.is_empty() && start_col == 0 {
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                Change::Attribute(AttributeChange::Foreground(self.colors.selection_fg)),
                Change::Text(" ".to_string()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
        }

        if seg_sel_end < end_col {
            let after: String = chars[seg_sel_end..end_col].iter().collect();
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(after),
            ]);
        }

        self.buf
            .add_changes(vec![Change::AllAttributes(CellAttributes::default())]);
    }

    fn render_wrapped_segment_with_block_selection(
        &mut self,
        chars: &[char],
        start_col: usize,
        end_col: usize,
        block_min_col: usize,
        block_max_col: usize,
        is_edge_line: bool,
    ) {
        let sel_col_start = block_min_col;
        let sel_col_end = block_max_col + 1;

        let seg_sel_start = sel_col_start.max(start_col).min(end_col);
        let seg_sel_end = sel_col_end.max(start_col).min(end_col);

        if start_col < seg_sel_start {
            let before: String = chars[start_col..seg_sel_start].iter().collect();
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(before),
            ]);
        }

        if seg_sel_start < seg_sel_end {
            let selected: String = chars[seg_sel_start..seg_sel_end].iter().collect();
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                Change::Attribute(AttributeChange::Foreground(self.colors.selection_fg)),
                Change::Text(selected),
                Change::AllAttributes(CellAttributes::default()),
            ]);
        } else if chars.is_empty() && is_edge_line {
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                Change::Text(" ".to_string()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
        }

        if seg_sel_end < end_col {
            let after: String = chars[seg_sel_end..end_col].iter().collect();
            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(after),
            ]);
        }

        self.buf
            .add_changes(vec![Change::AllAttributes(CellAttributes::default())]);
    }

    fn perform_delete_motion_with_count<F>(
        &mut self,
        motion: F,
        count: usize,
        is_inclusive: bool,
        delete_empty_lines: bool,
        allow_linewise: bool,
    ) where
        F: Fn(&EditorState) -> (usize, usize),
    {
        self.save_undo_state();
        self.lines_version += 1;
        let start = self.cursor;

        let mut end = self.cursor;
        for _ in 0..count {
            let old_cursor = self.cursor;
            self.cursor = end;
            end = motion(self);
            self.cursor = old_cursor;
        }

        self.perform_delete_motion_inner(
            start,
            end,
            is_inclusive,
            delete_empty_lines,
            allow_linewise,
        );
    }

    fn perform_delete_motion<F>(
        &mut self,
        motion: F,
        is_inclusive: bool,
        delete_empty_lines: bool,
        allow_linewise: bool,
    ) where
        F: Fn(&EditorState) -> (usize, usize),
    {
        self.save_undo_state();
        self.lines_version += 1;
        let start = self.cursor;
        let end = motion(self);
        self.perform_delete_motion_inner(
            start,
            end,
            is_inclusive,
            delete_empty_lines,
            allow_linewise,
        );
    }

    fn perform_delete_motion_inner(
        &mut self,
        start: (usize, usize),
        end: (usize, usize),
        is_inclusive: bool,
        delete_empty_lines: bool,
        allow_linewise: bool,
    ) {
        if end.0 < start.0 {
            let mut deleted_text = String::new();

            if end.1 == 0 && !delete_empty_lines {
                if !self.lines[end.0].trim().is_empty() {
                    deleted_text.push_str(&self.lines[end.0]);
                    deleted_text.push('\n');
                    self.lines[end.0] = String::new();
                }

                for row in (end.0 + 1)..start.0 {
                    deleted_text.push_str(&self.lines[row]);
                    deleted_text.push('\n');
                }

                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                deleted_text.push_str(&start_chars[..start.1].iter().collect::<String>());
                let start_suffix: String = start_chars[start.1..].iter().collect();

                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                self.lines[start.0] = start_suffix;

                for _ in (end.0 + 1)..start.0 {
                    self.lines.remove(end.0 + 1);
                }

                self.cursor.0 = end.0;
                self.cursor.1 = 0;
            } else {
                let end_chars: Vec<char> = self.lines[end.0].chars().collect();
                let end_prefix: String = end_chars[..end.1].iter().collect();
                deleted_text.push_str(&end_chars[end.1..].iter().collect::<String>());

                for row in (end.0 + 1)..start.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                deleted_text.push('\n');
                deleted_text.push_str(&start_chars[..start.1].iter().collect::<String>());
                let start_suffix: String = start_chars[start.1..].iter().collect();

                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                let new_line = format!("{}{}", end_prefix, start_suffix);

                for _ in end.0..start.0 {
                    self.lines.remove(end.0 + 1);
                }
                self.lines[end.0] = new_line;

                self.cursor.0 = end.0;
                self.cursor.1 = end.1;
            }
        } else if end.0 == start.0 && end.1 < start.1 {
            let range_start = end.1;
            let range_end = if is_inclusive {
                (start.1 + 1).min(self.lines[start.0].chars().count())
            } else {
                start.1
            };
            let mut chars: Vec<char> = self.lines[start.0].chars().collect();
            if range_start < range_end && range_end <= chars.len() {
                self.yank_buffer = chars[range_start..range_end].iter().collect();
                self.yank_is_linewise = false;
                chars.drain(range_start..range_end);
                self.lines[start.0] = chars.into_iter().collect();
            }
            self.cursor.1 = range_start;
        } else if end.0 > start.0 {
            let mut deleted_text = String::new();

            let first_non_blank = self.get_first_non_blank_in_line(start.0);
            let is_first_line_of_para = start.0 == 0 || self.lines[start.0 - 1].trim().is_empty();
            let cursor_qualifies_for_linewise = allow_linewise
                && if is_first_line_of_para {
                    start.1 <= first_non_blank
                } else {
                    start.1 == first_non_blank
                };

            if end.1 == 0 && !cursor_qualifies_for_linewise {
                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                let start_prefix: String = start_chars[..start.1].iter().collect();
                deleted_text.push_str(&start_chars[start.1..].iter().collect::<String>());

                for row in (start.0 + 1)..end.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                self.lines[start.0] = start_prefix;

                for _ in (start.0 + 1)..end.0 {
                    self.lines.remove(start.0 + 1);
                }

                self.cursor = start;
                let line_len = self.lines[self.cursor.0].chars().count();
                let max_col = if self.mode == EditorMode::Insert {
                    line_len
                } else {
                    line_len.saturating_sub(1)
                };
                if self.cursor.1 > max_col {
                    self.cursor.1 = max_col;
                }
            } else if end.1 == 0 && cursor_qualifies_for_linewise {
                deleted_text.push_str(&self.lines[start.0]);
                for row in (start.0 + 1)..end.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                self.yank_buffer = deleted_text;
                self.yank_is_linewise = true;

                if delete_empty_lines {
                    for _ in start.0..end.0 {
                        self.lines.remove(start.0);
                    }

                    if self.lines.is_empty() {
                        self.lines.push(String::new());
                    }

                    self.cursor.0 = start.0.min(self.lines.len() - 1);
                    self.cursor.1 = 0;
                } else {
                    self.lines[start.0] = String::new();

                    for _ in (start.0 + 1)..end.0 {
                        self.lines.remove(start.0 + 1);
                    }

                    self.cursor.0 = start.0;
                    self.cursor.1 = 0;
                }
            } else {
                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                let start_line_was_blank = start_chars.iter().all(|c| c.is_whitespace());
                let start_prefix: String = start_chars[..start.1].iter().collect();
                deleted_text.push_str(&start_chars[start.1..].iter().collect::<String>());

                for row in (start.0 + 1)..end.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                let end_chars: Vec<char> = self.lines[end.0].chars().collect();
                let end_col = if is_inclusive {
                    (end.1 + 1).min(end_chars.len())
                } else {
                    end.1
                };
                deleted_text.push('\n');
                deleted_text.push_str(&end_chars[..end_col].iter().collect::<String>());
                let end_suffix: String = end_chars[end_col..].iter().collect();

                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                let new_line = format!("{}{}", start_prefix, end_suffix);

                for _ in start.0..end.0 {
                    self.lines.remove(start.0 + 1);
                }
                self.lines[start.0] = new_line.clone();

                self.cursor = start;

                if delete_empty_lines && new_line.is_empty() {
                    if start_line_was_blank && start.0 > 0 {
                        self.lines.remove(start.0);
                        self.cursor.0 = start.0 - 1;
                        self.cursor.1 = 0;
                    } else if start.0 > 0 && self.lines[start.0 - 1].trim().is_empty() {
                        self.lines.remove(start.0 - 1);
                        self.cursor.0 = start.0 - 1;
                    }
                }
            }
        } else {
            let mut range_end = end.1;
            if is_inclusive {
                range_end += 1;
            }
            let mut chars: Vec<char> = self.lines[start.0].chars().collect();
            if range_end > chars.len() {
                range_end = chars.len();
            }

            if start.1 < range_end {
                self.yank_buffer = chars[start.1..range_end].iter().collect();
                self.yank_is_linewise = false;
                chars.drain(start.1..range_end);
                self.lines[start.0] = chars.into_iter().collect();
            }
        }
        self.clamp_cursor();
        self.update_desired_col();
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    fn perform_yank_motion<F>(&mut self, motion: F, is_inclusive: bool)
    where
        F: Fn(&EditorState) -> (usize, usize),
    {
        let start = self.cursor;
        let end = motion(self);

        if end.0 < start.0 {
            let first_non_blank = self.get_first_non_blank_in_line(start.0);
            let is_first_line_of_para = start.0 == 0 || self.lines[start.0 - 1].trim().is_empty();
            let cursor_qualifies_for_linewise = if is_first_line_of_para {
                start.1 <= first_non_blank
            } else {
                start.1 == first_non_blank
            };
            let is_linewise = end.1 == 0 && cursor_qualifies_for_linewise;

            let mut yanked_text = String::new();
            let end_chars: Vec<char> = self.lines[end.0].chars().collect();
            let start_chars: Vec<char> = self.lines[start.0].chars().collect();

            if is_linewise {
                yanked_text.push_str(&end_chars.iter().collect::<String>());

                for row in (end.0 + 1)..start.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                yanked_text.push('\n');
                yanked_text.push_str(&start_chars.iter().collect::<String>());
            } else {
                yanked_text.push_str(&end_chars[end.1..].iter().collect::<String>());

                for row in (end.0 + 1)..start.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                yanked_text.push('\n');
                yanked_text.push_str(&start_chars[..start.1].iter().collect::<String>());
            }

            self.yank_buffer = yanked_text;
            self.yank_is_linewise = is_linewise;

            self.cursor = end;
            self.update_desired_col();
        } else if end.0 == start.0 && end.1 < start.1 {
            let chars: Vec<char> = self.lines[start.0].chars().collect();
            let yank_end = if is_inclusive {
                (start.1 + 1).min(chars.len())
            } else {
                start.1
            };
            if end.1 < yank_end && yank_end <= chars.len() {
                self.yank_buffer = chars[end.1..yank_end].iter().collect();
            }
            self.yank_is_linewise = false;

            self.cursor.1 = end.1;
            self.update_desired_col();
        } else if end.0 > start.0 {
            let end_chars: Vec<char> = self.lines[end.0].chars().collect();
            let end_col = if is_inclusive {
                (end.1 + 1).min(end_chars.len())
            } else {
                end.1
            };

            let first_non_blank = self.get_first_non_blank_in_line(start.0);
            let is_first_line_of_para = start.0 == 0 || self.lines[start.0 - 1].trim().is_empty();
            let cursor_qualifies_for_linewise = if is_first_line_of_para {
                start.1 <= first_non_blank
            } else {
                start.1 == first_non_blank
            };
            let is_linewise = cursor_qualifies_for_linewise && end_col == 0;

            let mut yanked_text = String::new();
            let start_chars: Vec<char> = self.lines[start.0].chars().collect();

            if is_linewise {
                yanked_text.push_str(&start_chars.iter().collect::<String>());

                for row in (start.0 + 1)..end.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }
            } else {
                yanked_text.push_str(&start_chars[start.1..].iter().collect::<String>());

                for row in (start.0 + 1)..end.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                if end_col > 0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&end_chars[..end_col].iter().collect::<String>());
                }
            }

            self.yank_buffer = yanked_text;
            self.yank_is_linewise = is_linewise;
        } else {
            let chars: Vec<char> = self.lines[start.0].chars().collect();
            let mut range_end = end.1;
            if is_inclusive {
                range_end += 1;
            }
            if range_end > chars.len() {
                range_end = chars.len();
            }
            if start.1 < range_end {
                self.yank_buffer = chars[start.1..range_end].iter().collect();
            }
            self.yank_is_linewise = false;
        }
    }

    fn perform_yank_motion_with_count<F>(&mut self, motion: F, count: usize, is_inclusive: bool)
    where
        F: Fn(&EditorState) -> (usize, usize),
    {
        let start = self.cursor;

        let mut end = self.cursor;
        for _ in 0..count {
            let old_cursor = self.cursor;
            self.cursor = end;
            end = motion(self);
            self.cursor = old_cursor;
        }

        self.perform_yank_motion_inner(start, end, is_inclusive);
    }

    fn perform_yank_motion_inner(
        &mut self,
        start: (usize, usize),
        end: (usize, usize),
        is_inclusive: bool,
    ) {
        if end.0 < start.0 {
            let first_non_blank = self.get_first_non_blank_in_line(start.0);
            let is_first_line_of_para = start.0 == 0 || self.lines[start.0 - 1].trim().is_empty();
            let cursor_qualifies_for_linewise = if is_first_line_of_para {
                start.1 <= first_non_blank
            } else {
                start.1 == first_non_blank
            };
            let is_linewise = end.1 == 0 && cursor_qualifies_for_linewise;

            let mut yanked_text = String::new();
            let end_chars: Vec<char> = self.lines[end.0].chars().collect();
            let start_chars: Vec<char> = self.lines[start.0].chars().collect();

            if is_linewise {
                yanked_text.push_str(&end_chars.iter().collect::<String>());

                for row in (end.0 + 1)..start.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                yanked_text.push('\n');
                yanked_text.push_str(&start_chars.iter().collect::<String>());
            } else {
                yanked_text.push_str(&end_chars[end.1..].iter().collect::<String>());

                for row in (end.0 + 1)..start.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                yanked_text.push('\n');
                yanked_text.push_str(&start_chars[..start.1].iter().collect::<String>());
            }

            self.yank_buffer = yanked_text;
            self.yank_is_linewise = is_linewise;

            self.cursor = end;
            self.update_desired_col();
        } else if end.0 == start.0 && end.1 < start.1 {
            let chars: Vec<char> = self.lines[start.0].chars().collect();
            let yank_end = if is_inclusive {
                (start.1 + 1).min(chars.len())
            } else {
                start.1
            };
            if end.1 < yank_end && yank_end <= chars.len() {
                self.yank_buffer = chars[end.1..yank_end].iter().collect();
            }
            self.yank_is_linewise = false;

            self.cursor.1 = end.1;
            self.update_desired_col();
        } else if end.0 > start.0 {
            let end_chars: Vec<char> = self.lines[end.0].chars().collect();
            let end_col = if is_inclusive {
                (end.1 + 1).min(end_chars.len())
            } else {
                end.1
            };

            let first_non_blank = self.get_first_non_blank_in_line(start.0);
            let is_first_line_of_para = start.0 == 0 || self.lines[start.0 - 1].trim().is_empty();
            let cursor_qualifies_for_linewise = if is_first_line_of_para {
                start.1 <= first_non_blank
            } else {
                start.1 == first_non_blank
            };
            let is_linewise = cursor_qualifies_for_linewise && end_col == 0;

            let mut yanked_text = String::new();
            let start_chars: Vec<char> = self.lines[start.0].chars().collect();

            if is_linewise {
                yanked_text.push_str(&start_chars.iter().collect::<String>());

                for row in (start.0 + 1)..end.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }
            } else {
                yanked_text.push_str(&start_chars[start.1..].iter().collect::<String>());

                for row in (start.0 + 1)..end.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                if end_col > 0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&end_chars[..end_col].iter().collect::<String>());
                }
            }

            self.yank_buffer = yanked_text;
            self.yank_is_linewise = is_linewise;
        } else {
            let chars: Vec<char> = self.lines[start.0].chars().collect();
            let mut range_end = end.1;
            if is_inclusive {
                range_end += 1;
            }
            if range_end > chars.len() {
                range_end = chars.len();
            }
            if start.1 < range_end {
                self.yank_buffer = chars[start.1..range_end].iter().collect();
            }
            self.yank_is_linewise = false;
        }
    }

    fn yank_text_object_on_line(&mut self, start: usize, end: usize) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if start < end && end <= chars.len() {
            self.yank_buffer = chars[start..end].iter().collect();
            self.yank_is_linewise = false;
        }
    }

    fn yank_inner_word(&mut self) {
        let (start, end) = self.get_inner_word_bounds();
        self.yank_text_object_on_line(start, end);
    }

    fn yank_a_word(&mut self) {
        let (start, end) = self.get_a_word_bounds();
        self.yank_text_object_on_line(start, end);
    }

    fn yank_inner_long_word(&mut self) {
        let (start, end) = self.get_inner_long_word_bounds();
        self.yank_text_object_on_line(start, end);
    }

    fn yank_a_long_word(&mut self) {
        let (start, end) = self.get_a_long_word_bounds();
        self.yank_text_object_on_line(start, end);
    }

    fn yank_inner_pair(&mut self, pair_char: char) {
        if let Some(((open_row, open_col), (close_row, close_col))) =
            self.find_pair_bounds(pair_char)
        {
            if open_row == close_row {
                let chars: Vec<char> = self.lines[open_row].chars().collect();
                if open_col + 1 < close_col {
                    self.yank_buffer = chars[open_col + 1..close_col].iter().collect();
                } else {
                    self.yank_buffer.clear();
                }
                self.yank_is_linewise = false;
            } else {
                let mut yanked = String::new();

                // Skip whitespace-only content after open bracket
                let first_chars: Vec<char> = self.lines[open_row].chars().collect();
                let after_open: String = first_chars[open_col + 1..].iter().collect();
                let has_first_line_content = !after_open.trim().is_empty();
                if has_first_line_content {
                    yanked.push_str(&after_open);
                }

                for row in (open_row + 1)..close_row {
                    if !yanked.is_empty() || !has_first_line_content {
                        yanked.push('\n');
                    }
                    yanked.push_str(&self.lines[row]);
                }

                // Skip whitespace-only content before close bracket
                if close_row > open_row {
                    let last_chars: Vec<char> = self.lines[close_row].chars().collect();
                    let before_close: String = last_chars[..close_col].iter().collect();
                    let has_last_line_content = !before_close.trim().is_empty();
                    if has_last_line_content {
                        if !yanked.is_empty() {
                            yanked.push('\n');
                        }
                        yanked.push_str(&before_close);
                    }
                }

                self.yank_buffer = yanked;
                self.yank_is_linewise = false;
            }
        }
    }

    fn yank_around_pair(&mut self, pair_char: char) {
        if let Some(((open_row, open_col), (close_row, close_col))) =
            self.find_pair_bounds(pair_char)
        {
            if open_row == close_row {
                let chars: Vec<char> = self.lines[open_row].chars().collect();
                self.yank_buffer = chars[open_col..=close_col].iter().collect();
                self.yank_is_linewise = false;
            } else {
                let mut yanked = String::new();
                let first_chars: Vec<char> = self.lines[open_row].chars().collect();
                yanked.extend(&first_chars[open_col..]);
                for row in (open_row + 1)..close_row {
                    yanked.push('\n');
                    yanked.push_str(&self.lines[row]);
                }
                if close_row > open_row {
                    yanked.push('\n');
                    let last_chars: Vec<char> = self.lines[close_row].chars().collect();
                    yanked.extend(&last_chars[..=close_col]);
                }
                self.yank_buffer = yanked;
                self.yank_is_linewise = false;
            }
        }
    }

    fn yank_to_char_forward(&mut self, target: char, inclusive: bool) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 + 1 < chars.len() {
            if let Some(rel_pos) = chars[self.cursor.1 + 1..].iter().position(|&c| c == target) {
                let target_col = self.cursor.1 + 1 + rel_pos;
                let end_col = if inclusive {
                    target_col + 1
                } else {
                    target_col
                };
                self.yank_buffer = chars[self.cursor.1..end_col].iter().collect();
                self.yank_is_linewise = false;
            }
        }
    }

    fn yank_to_char_backward(&mut self, target: char, inclusive: bool) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 > 0 {
            if let Some(rel_pos) = chars[..self.cursor.1].iter().rposition(|&c| c == target) {
                let start_col = if inclusive { rel_pos } else { rel_pos + 1 };
                self.yank_buffer = chars[start_col..self.cursor.1].iter().collect();
                self.yank_is_linewise = false;
                self.cursor.1 = start_col;
                self.update_desired_col();
            }
        }
    }

    fn yank_to_matching_bracket(&mut self) {
        let saved_cursor = self.cursor;
        self.jump_to_matching_bracket();
        if self.cursor != saved_cursor {
            let end = self.cursor;
            if saved_cursor.0 == end.0 {
                let (start_col, end_col) = if saved_cursor.1 <= end.1 {
                    (saved_cursor.1, end.1 + 1)
                } else {
                    (end.1, saved_cursor.1 + 1)
                };
                let chars: Vec<char> = self.lines[saved_cursor.0].chars().collect();
                if end_col <= chars.len() {
                    self.yank_buffer = chars[start_col..end_col].iter().collect();
                    self.yank_is_linewise = false;
                }
                self.cursor = (saved_cursor.0, start_col);
                self.update_desired_col();
            } else {
                let (start_pos, end_pos) = if saved_cursor.0 < end.0
                    || (saved_cursor.0 == end.0 && saved_cursor.1 < end.1)
                {
                    (saved_cursor, end)
                } else {
                    (end, saved_cursor)
                };
                let mut yanked = String::new();
                let first_chars: Vec<char> = self.lines[start_pos.0].chars().collect();
                yanked.extend(&first_chars[start_pos.1..]);
                for row in (start_pos.0 + 1)..end_pos.0 {
                    yanked.push('\n');
                    yanked.push_str(&self.lines[row]);
                }
                yanked.push('\n');
                let last_chars: Vec<char> = self.lines[end_pos.0].chars().collect();
                yanked.extend(&last_chars[..=end_pos.1]);
                self.yank_buffer = yanked;
                self.yank_is_linewise = false;
                self.cursor = start_pos;
                self.update_desired_col();
            }
        }
    }

    fn yank_to_prev_unmatched(&mut self, open: char, close: char) {
        let saved_cursor = self.cursor;
        self.jump_to_prev_unmatched(open, close);
        if self.cursor != saved_cursor {
            let target = self.cursor;
            if target.0 == saved_cursor.0 {
                let chars: Vec<char> = self.lines[target.0].chars().collect();
                if target.1 < saved_cursor.1 && saved_cursor.1 <= chars.len() {
                    self.yank_buffer = chars[target.1 + 1..saved_cursor.1].iter().collect();
                    self.yank_is_linewise = false;
                }
            }
            self.update_desired_col();
        }
    }

    fn yank_to_next_unmatched(&mut self, open: char, close: char) {
        let saved_cursor = self.cursor;
        self.jump_to_next_unmatched(open, close);
        if self.cursor != saved_cursor {
            let target = self.cursor;
            self.cursor = saved_cursor;
            if saved_cursor.0 == target.0 {
                let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
                if saved_cursor.1 < target.1 && target.1 <= chars.len() {
                    self.yank_buffer = chars[saved_cursor.1..target.1].iter().collect();
                    self.yank_is_linewise = false;
                }
            }
        }
    }

    fn paste_after_count(&mut self, count: usize) {
        self.save_undo_state();
        self.lines_version += 1;
        if self.yank_buffer.is_empty() || count == 0 {
            return;
        }

        if self.yank_is_block {
            let paste_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let start_row = self.cursor.0;

            for (i, paste_line) in paste_lines.iter().enumerate() {
                let target_row = start_row + i;
                if target_row >= self.lines.len() {
                    self.lines.push(String::new());
                }

                let mut chars: Vec<char> = self.lines[target_row].chars().collect();
                let insert_pos = if chars.is_empty() {
                    0
                } else {
                    (self.cursor.1 + 1).min(chars.len())
                };

                while chars.len() < insert_pos {
                    chars.push(' ');
                }

                let repeated_paste: String = paste_line.repeat(count);
                let paste_chars: Vec<char> = repeated_paste.chars().collect();

                for (j, c) in paste_chars.iter().enumerate() {
                    chars.insert(insert_pos + j, *c);
                }
                self.lines[target_row] = chars.into_iter().collect();
            }

            let first_line_chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = if first_line_chars.is_empty() {
                0
            } else {
                (self.cursor.1 + 1).min(first_line_chars.len())
            };
            self.cursor.1 = insert_pos;
        } else if self.yank_is_linewise {
            let base_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let mut all_lines: Vec<String> = Vec::new();
            for _ in 0..count {
                for line in &base_lines {
                    all_lines.push(line.to_string());
                }
            }
            for (i, line) in all_lines.iter().enumerate() {
                self.lines.insert(self.cursor.0 + 1 + i, line.clone());
            }
            self.cursor.0 += 1;
            self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        } else if self.yank_buffer.contains('\n') {
            let repeated_buffer = std::iter::repeat(self.yank_buffer.as_str())
                .take(count)
                .collect::<Vec<_>>()
                .join("");
            let paste_lines: Vec<&str> = repeated_buffer.split('\n').collect();
            let current_line_chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = if current_line_chars.is_empty() {
                0
            } else {
                self.cursor.1 + 1
            };

            let before: String = current_line_chars[..insert_pos.min(current_line_chars.len())]
                .iter()
                .collect();
            let after: String = current_line_chars[insert_pos.min(current_line_chars.len())..]
                .iter()
                .collect();

            self.lines[self.cursor.0] = before + paste_lines[0];

            for (i, paste_line) in paste_lines[1..paste_lines.len() - 1].iter().enumerate() {
                self.lines
                    .insert(self.cursor.0 + 1 + i, paste_line.to_string());
            }

            if paste_lines.len() > 1 {
                let last_paste_line = paste_lines[paste_lines.len() - 1];
                self.lines.insert(
                    self.cursor.0 + paste_lines.len() - 1,
                    last_paste_line.to_string() + &after,
                );
            }

            self.cursor.0 += paste_lines.len() - 1;
            let last_paste_chars = paste_lines[paste_lines.len() - 1].chars().count();
            self.cursor.1 = if last_paste_chars > 0 {
                last_paste_chars - 1
            } else {
                0
            };
        } else {
            let repeated_buffer: String = self.yank_buffer.repeat(count);
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = if chars.is_empty() {
                0
            } else {
                (self.cursor.1 + 1).min(chars.len())
            };
            let paste_chars: Vec<char> = repeated_buffer.chars().collect();

            for (i, c) in paste_chars.iter().enumerate() {
                chars.insert(insert_pos + i, *c);
            }
            self.lines[self.cursor.0] = chars.into_iter().collect();

            self.cursor.1 = insert_pos + paste_chars.len().saturating_sub(1);
        }
        self.clamp_cursor();
        self.record_change();
    }

    fn paste_before_count(&mut self, count: usize) {
        self.save_undo_state();
        self.lines_version += 1;
        if self.yank_buffer.is_empty() || count == 0 {
            return;
        }

        if self.yank_is_block {
            let paste_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let start_row = self.cursor.0;
            let insert_col = self.cursor.1;

            for (i, paste_line) in paste_lines.iter().enumerate() {
                let target_row = start_row + i;
                if target_row >= self.lines.len() {
                    self.lines.push(String::new());
                }

                let mut chars: Vec<char> = self.lines[target_row].chars().collect();
                let insert_pos = insert_col.min(chars.len());

                while chars.len() < insert_pos {
                    chars.push(' ');
                }

                let repeated_paste: String = paste_line.repeat(count);
                let paste_chars: Vec<char> = repeated_paste.chars().collect();

                for (j, c) in paste_chars.iter().enumerate() {
                    chars.insert(insert_pos + j, *c);
                }
                self.lines[target_row] = chars.into_iter().collect();
            }
        } else if self.yank_is_linewise {
            let base_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let mut all_lines: Vec<String> = Vec::new();
            for _ in 0..count {
                for line in &base_lines {
                    all_lines.push(line.to_string());
                }
            }
            for (i, line) in all_lines.iter().enumerate() {
                self.lines.insert(self.cursor.0 + i, line.clone());
            }
            self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        } else if self.yank_buffer.contains('\n') {
            let repeated_buffer = std::iter::repeat(self.yank_buffer.as_str())
                .take(count)
                .collect::<Vec<_>>()
                .join("");
            let paste_lines: Vec<&str> = repeated_buffer.split('\n').collect();
            let current_line_chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = self.cursor.1.min(current_line_chars.len());

            let before: String = current_line_chars[..insert_pos].iter().collect();
            let after: String = current_line_chars[insert_pos..].iter().collect();

            self.lines[self.cursor.0] = before + paste_lines[0];

            for (i, paste_line) in paste_lines[1..paste_lines.len() - 1].iter().enumerate() {
                self.lines
                    .insert(self.cursor.0 + 1 + i, paste_line.to_string());
            }

            if paste_lines.len() > 1 {
                let last_paste_line = paste_lines[paste_lines.len() - 1];
                self.lines.insert(
                    self.cursor.0 + paste_lines.len() - 1,
                    last_paste_line.to_string() + &after,
                );
            }

            self.cursor.0 += paste_lines.len() - 1;
            let last_paste_chars = paste_lines[paste_lines.len() - 1].chars().count();
            self.cursor.1 = if last_paste_chars > 0 {
                last_paste_chars - 1
            } else {
                0
            };
        } else {
            let repeated_buffer: String = self.yank_buffer.repeat(count);
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = self.cursor.1.min(chars.len());
            let paste_chars: Vec<char> = repeated_buffer.chars().collect();

            for (i, c) in paste_chars.iter().enumerate() {
                chars.insert(insert_pos + i, *c);
            }
            self.lines[self.cursor.0] = chars.into_iter().collect();

            self.cursor.1 = insert_pos + paste_chars.len().saturating_sub(1);
        }
        self.clamp_cursor();
        self.record_change();
    }

    fn get_first_non_blank_in_line(&self, row: usize) -> usize {
        let line = &self.lines[row];
        line.chars().position(|c| !c.is_whitespace()).unwrap_or(0)
    }

    fn perform_incremental_search(&mut self) {
        if self.search_input.is_empty() {
            self.cursor = self.search_start_pos;
            self.search_highlight = false;
            self.current_match = None;
            return;
        }

        self.search_pattern = self.search_input.clone();
        self.search_highlight = true;

        let pattern = self.search_input.clone();
        let start_row = self.search_start_pos.0;
        let start_col = self.search_start_pos.1;

        match self.search_direction {
            Direction::Forward => {
                let current_line = &self.lines[start_row];
                if start_col < current_line.len() {
                    if let Some(pos) = current_line[start_col..].find(&pattern) {
                        self.cursor.0 = start_row;
                        self.cursor.1 =
                            start_col + current_line[start_col..][..pos].chars().count();
                        self.current_match = Some(self.cursor);
                        return;
                    }
                }
                for row in (start_row + 1)..self.lines.len() {
                    if let Some(pos) = self.lines[row].find(&pattern) {
                        self.cursor.0 = row;
                        self.cursor.1 = self.lines[row][..pos].chars().count();
                        self.current_match = Some(self.cursor);
                        return;
                    }
                }
                for row in 0..=start_row {
                    let search_end = if row == start_row {
                        start_col
                    } else {
                        self.lines[row].len()
                    };
                    if search_end > 0 {
                        if let Some(pos) = self.lines[row][..search_end].find(&pattern) {
                            self.cursor.0 = row;
                            self.cursor.1 = self.lines[row][..pos].chars().count();
                            self.current_match = Some(self.cursor);
                            return;
                        }
                    }
                }
            }
            Direction::Backward => {
                let current_line = &self.lines[start_row];
                if start_col > 0 {
                    if let Some(pos) = current_line[..start_col].rfind(&pattern) {
                        self.cursor.0 = start_row;
                        self.cursor.1 = current_line[..pos].chars().count();
                        self.current_match = Some(self.cursor);
                        return;
                    }
                }
                for row in (0..start_row).rev() {
                    if let Some(pos) = self.lines[row].rfind(&pattern) {
                        self.cursor.0 = row;
                        self.cursor.1 = self.lines[row][..pos].chars().count();
                        self.current_match = Some(self.cursor);
                        return;
                    }
                }
                for row in (start_row..self.lines.len()).rev() {
                    let search_start = if row == start_row { start_col } else { 0 };
                    if search_start < self.lines[row].len() {
                        if let Some(pos) = self.lines[row][search_start..].rfind(&pattern) {
                            self.cursor.0 = row;
                            self.cursor.1 = self.lines[row][..search_start].chars().count()
                                + self.lines[row][search_start..][..pos].chars().count();
                            self.current_match = Some(self.cursor);
                            return;
                        }
                    }
                }
            }
        }
        self.cursor = self.search_start_pos;
        self.current_match = None;
    }

    fn search_next(&mut self) {
        if self.search_pattern.is_empty() {
            return;
        }
        match self.search_direction {
            Direction::Forward => self.search_forward_from_cursor(),
            Direction::Backward => self.search_backward_from_cursor(),
        }
        self.current_match = Some(self.cursor);
    }

    fn search_prev(&mut self) {
        if self.search_pattern.is_empty() {
            return;
        }
        match self.search_direction {
            Direction::Forward => self.search_backward_from_cursor(),
            Direction::Backward => self.search_forward_from_cursor(),
        }
        self.current_match = Some(self.cursor);
    }

    fn search_forward_from_cursor(&mut self) {
        let pattern = &self.search_pattern;
        if pattern.is_empty() {
            return;
        }

        let start_row = self.cursor.0;
        let start_col = self.cursor.1 + 1;

        let current_line = &self.lines[start_row];
        let chars: Vec<char> = current_line.chars().collect();
        if start_col < chars.len() {
            let search_str: String = chars[start_col..].iter().collect();
            if let Some(pos) = search_str.find(pattern) {
                let char_pos = search_str[..pos].chars().count();
                self.cursor.1 = start_col + char_pos;
                return;
            }
        }

        for row in (start_row + 1)..self.lines.len() {
            if let Some(pos) = self.lines[row].find(pattern) {
                let char_pos = self.lines[row][..pos].chars().count();
                self.cursor.0 = row;
                self.cursor.1 = char_pos;
                return;
            }
        }

        for row in 0..=start_row {
            let search_end = if row == start_row {
                self.cursor.1
            } else {
                self.lines[row].len()
            };
            let search_str = &self.lines[row][..search_end.min(self.lines[row].len())];
            if let Some(pos) = search_str.find(pattern) {
                let char_pos = search_str[..pos].chars().count();
                self.cursor.0 = row;
                self.cursor.1 = char_pos;
                return;
            }
        }
    }

    fn search_backward_from_cursor(&mut self) {
        let pattern = &self.search_pattern;
        if pattern.is_empty() {
            return;
        }

        let start_row = self.cursor.0;
        let start_col = self.cursor.1;

        let current_line = &self.lines[start_row];
        let chars: Vec<char> = current_line.chars().collect();
        if start_col > 0 {
            let search_str: String = chars[..start_col].iter().collect();
            if let Some(pos) = search_str.rfind(pattern) {
                let char_pos = search_str[..pos].chars().count();
                self.cursor.1 = char_pos;
                return;
            }
        }

        for row in (0..start_row).rev() {
            if let Some(pos) = self.lines[row].rfind(pattern) {
                let char_pos = self.lines[row][..pos].chars().count();
                self.cursor.0 = row;
                self.cursor.1 = char_pos;
                return;
            }
        }

        for row in (start_row..self.lines.len()).rev() {
            let search_start = if row == start_row {
                let chars: Vec<char> = self.lines[row].chars().collect();
                if start_col < chars.len() {
                    chars[..start_col].iter().map(|c| c.len_utf8()).sum()
                } else {
                    0
                }
            } else {
                0
            };
            let search_str = &self.lines[row][search_start..];
            if let Some(pos) = search_str.rfind(pattern) {
                let full_str = &self.lines[row][..search_start + pos];
                let char_pos = full_str.chars().count();
                self.cursor.0 = row;
                self.cursor.1 = char_pos;
                return;
            }
        }
    }

    fn search_word_under_cursor(&mut self, forward: bool) {
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return;
        }

        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));

        let search_col = if chars[col].is_whitespace() {
            match chars[col..].iter().position(|c| !c.is_whitespace()) {
                Some(offset) => col + offset,
                None => return,
            }
        } else {
            col
        };

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        let is_punct = |c: char| !c.is_whitespace() && !is_word_char(c);

        let char_matches: &dyn Fn(char) -> bool = if is_word_char(chars[search_col]) {
            &is_word_char
        } else {
            &is_punct
        };

        let mut start = search_col;
        let mut end = search_col;

        while start > 0 && char_matches(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && char_matches(chars[end]) {
            end += 1;
        }

        if start >= end {
            return;
        }

        let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
        let byte_end: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();

        self.search_pattern = line[byte_start..byte_end].to_string();
        self.search_direction = if forward {
            Direction::Forward
        } else {
            Direction::Backward
        };
        self.search_display_direction = self.search_direction;
        self.search_highlight = true;
        self.cursor.1 = start;

        if forward {
            self.search_forward_from_cursor();
        } else {
            self.search_backward_from_cursor();
        }
        self.current_match = Some(self.cursor);
    }

    fn get_visual_selection(&self) -> ((usize, usize), (usize, usize)) {
        if self.visual_start.0 < self.cursor.0
            || (self.visual_start.0 == self.cursor.0 && self.visual_start.1 <= self.cursor.1)
        {
            (self.visual_start, self.cursor)
        } else {
            (self.cursor, self.visual_start)
        }
    }

    fn get_visual_block_bounds(&self) -> (usize, usize, usize, usize) {
        let min_row = self.visual_start.0.min(self.cursor.0);
        let max_row = self.visual_start.0.max(self.cursor.0);
        let min_col = self.visual_start.1.min(self.cursor.1);
        let max_col = self.visual_start.1.max(self.cursor.1);
        (min_row, max_row, min_col, max_col)
    }

    fn delete_visual_selection(&mut self) {
        let undo_cursor = if self.mode == EditorMode::VisualBlock {
            let (min_row, _, min_col, _) = self.get_visual_block_bounds();
            (min_row, min_col)
        } else {
            let (start, _) = self.get_visual_selection();
            start
        };

        self.save_undo_state_with_cursor(undo_cursor);
        self.delete_visual_selection_no_undo();
        self.record_change();
    }

    fn delete_visual_selection_no_undo(&mut self) {
        let (start, end) = self.get_visual_selection();

        if self.mode == EditorMode::VisualBlock {
            let (min_row, max_row, min_col, max_col) = self.get_visual_block_bounds();

            let mut yanked_lines = Vec::new();
            for row in min_row..=max_row {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = min_col.min(line_len);
                let sel_end = (max_col + 1).min(line_len);
                if sel_start < sel_end {
                    yanked_lines.push(chars[sel_start..sel_end].iter().collect::<String>());
                } else {
                    yanked_lines.push(String::new());
                }
            }
            self.yank_buffer = yanked_lines.join("\n");
            self.yank_is_linewise = false;
            self.yank_is_block = true;

            self.lines_version += 1;
            for row in min_row..=max_row {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = min_col.min(line_len);
                let sel_end = (max_col + 1).min(line_len);
                if sel_start < sel_end {
                    let new_line: String =
                        chars[..sel_start].iter().chain(&chars[sel_end..]).collect();
                    self.lines[row] = new_line;
                }
            }

            self.cursor = (min_row, min_col);
            self.clamp_cursor();
            self.update_desired_col();
            return;
        } else if self.mode == EditorMode::VisualLine {
            let yanked: Vec<&str> = self.lines[start.0..=end.0]
                .iter()
                .map(|s| s.as_str())
                .collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
            self.yank_is_block = false;

            self.lines_version += 1;
            for _ in start.0..=end.0 {
                if self.lines.len() > 1 {
                    self.lines.remove(start.0);
                } else {
                    self.lines[0].clear();
                }
            }
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.cursor.0 = start.0.min(self.lines.len().saturating_sub(1));
            self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        } else {
            if start.0 == end.0 {
                let line = &self.lines[start.0];
                let chars: Vec<char> = line.chars().collect();
                let sel_end = (end.1 + 1).min(chars.len());
                self.yank_buffer = chars[start.1..sel_end].iter().collect();
                self.yank_is_linewise = false;

                self.lines_version += 1;
                let new_line: String = chars[..start.1].iter().chain(&chars[sel_end..]).collect();
                self.lines[start.0] = new_line;
                self.cursor = start;
            } else {
                let mut yanked = String::new();
                let first_chars: Vec<char> = self.lines[start.0].chars().collect();
                yanked.extend(&first_chars[start.1..]);
                for row in (start.0 + 1)..end.0 {
                    yanked.push('\n');
                    yanked.push_str(&self.lines[row]);
                }
                yanked.push('\n');
                let last_chars: Vec<char> = self.lines[end.0].chars().collect();
                let sel_end = (end.1 + 1).min(last_chars.len());
                yanked.extend(&last_chars[..sel_end]);
                self.yank_buffer = yanked;
                self.yank_is_linewise = false;

                self.lines_version += 1;
                let first_part: String = first_chars[..start.1].iter().collect();
                let first_part_len = first_part.chars().count();
                let last_part: String = last_chars[sel_end..].iter().collect();
                self.lines[start.0] = first_part + &last_part;
                for _ in (start.0 + 1)..=end.0 {
                    self.lines.remove(start.0 + 1);
                }
                let merged_line_len = self.lines[start.0].chars().count();
                if first_part_len < merged_line_len {
                    self.cursor = (start.0, first_part_len);
                } else if start.0 + 1 < self.lines.len() {
                    self.cursor = (start.0 + 1, 0);
                } else {
                    self.cursor = (start.0, first_part_len.saturating_sub(1));
                }
            }
        }
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn yank_visual_selection(&mut self) {
        let (start, end) = self.get_visual_selection();

        if self.mode == EditorMode::VisualBlock {
            let (min_row, max_row, min_col, max_col) = self.get_visual_block_bounds();

            let mut yanked_lines = Vec::new();
            for row in min_row..=max_row {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = min_col.min(line_len);
                let sel_end = (max_col + 1).min(line_len);
                if sel_start < sel_end {
                    yanked_lines.push(chars[sel_start..sel_end].iter().collect::<String>());
                } else {
                    yanked_lines.push(String::new());
                }
            }
            self.yank_buffer = yanked_lines.join("\n");
            self.yank_is_linewise = false;
            self.yank_is_block = true;
            self.cursor = (min_row, min_col);
        } else if self.mode == EditorMode::VisualLine {
            let yanked: Vec<&str> = self.lines[start.0..=end.0]
                .iter()
                .map(|s| s.as_str())
                .collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
            self.yank_is_block = false;
            self.cursor = start;
        } else {
            self.yank_is_block = false;
            if start.0 == end.0 {
                let chars: Vec<char> = self.lines[start.0].chars().collect();
                let sel_end = (end.1 + 1).min(chars.len());
                self.yank_buffer = chars[start.1..sel_end].iter().collect();
                self.yank_is_linewise = false;
            } else {
                let mut yanked = String::new();
                let first_chars: Vec<char> = self.lines[start.0].chars().collect();
                yanked.extend(&first_chars[start.1..]);
                for row in (start.0 + 1)..end.0 {
                    yanked.push('\n');
                    yanked.push_str(&self.lines[row]);
                }
                yanked.push('\n');
                let last_chars: Vec<char> = self.lines[end.0].chars().collect();
                let sel_end = (end.1 + 1).min(last_chars.len());
                yanked.extend(&last_chars[..sel_end]);
                self.yank_buffer = yanked;
                self.yank_is_linewise = false;
            }
            self.cursor = start;
        }
    }

    fn delete_visual_lines(&mut self) {
        let (start, end) = self.get_visual_selection();
        self.save_undo_state_with_cursor((start.0, 0));
        self.lines_version += 1;

        let yanked: Vec<String> = self.lines[start.0..=end.0]
            .iter()
            .map(|s| s.to_string())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        self.yank_is_block = false;

        self.lines.drain(start.0..=end.0);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        self.cursor.0 = start.0.min(self.lines.len().saturating_sub(1));
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.record_change();
        self.mode = EditorMode::Normal;
    }

    fn change_visual_lines(&mut self) {
        let (start, end) = self.get_visual_selection();
        self.save_undo_state_with_cursor((start.0, 0));
        self.lines_version += 1;

        let yanked: Vec<String> = self.lines[start.0..=end.0]
            .iter()
            .map(|s| s.to_string())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        self.yank_is_block = false;

        self.lines.drain(start.0..=end.0);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        } else {
            self.lines.insert(start.0, String::new());
        }

        self.cursor = (start.0, 0);
        self.mode = EditorMode::Insert;
        self.insert_buffer.clear();
    }

    fn delete_block_to_eol(&mut self) {
        let (start, end) = self.get_visual_selection();
        let min_col = self.visual_start.1.min(self.cursor.1);
        self.save_undo_state_with_cursor((start.0, min_col));
        self.lines_version += 1;

        let mut yanked_lines = Vec::new();
        for row in start.0..=end.0 {
            let chars: Vec<char> = self.lines[row].chars().collect();
            if min_col < chars.len() {
                yanked_lines.push(chars[min_col..].iter().collect::<String>());
                self.lines[row] = chars[..min_col].iter().collect();
            } else {
                yanked_lines.push(String::new());
            }
        }
        self.yank_buffer = yanked_lines.join("\n");
        self.yank_is_linewise = false;
        self.yank_is_block = true;

        self.cursor = (start.0, min_col);
        self.clamp_cursor();
        self.record_change();
        self.mode = EditorMode::Normal;
    }

    fn change_block_to_eol(&mut self) {
        let (start, end) = self.get_visual_selection();
        let num_rows = end.0 - start.0 + 1;
        let min_col = self.visual_start.1.min(self.cursor.1);
        self.save_undo_state_with_cursor((start.0, min_col));
        self.lines_version += 1;

        let mut yanked_lines = Vec::new();
        for row in start.0..=end.0 {
            let chars: Vec<char> = self.lines[row].chars().collect();
            if min_col < chars.len() {
                yanked_lines.push(chars[min_col..].iter().collect::<String>());
                self.lines[row] = chars[..min_col].iter().collect();
            } else {
                yanked_lines.push(String::new());
            }
        }
        self.yank_buffer = yanked_lines.join("\n");
        self.yank_is_linewise = false;
        self.yank_is_block = true;

        self.block_insert_info = Some((start.0, num_rows, min_col, BlockInsertType::Insert));
        self.mode = EditorMode::Insert;
        self.insert_buffer.clear();
        self.cursor = (start.0, min_col);
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        self.render()?;
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            if let InputEvent::Resized { cols, rows } = event {
                self.buf.resize(cols, rows);
                self.render()?;
                continue;
            }

            match self.mode {
                EditorMode::Normal => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('C'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        self.cancel();
                        break;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('R'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        self.redo();
                        self.update_desired_col();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('A'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        let count = self.take_count();
                        self.increment_number(count);
                        self.set_last_change(LastChange::IncrementNumber, count);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('X'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        let count = self.take_count();
                        self.decrement_number(count);
                        self.set_last_change(LastChange::DecrementNumber, count);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('E'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        let count = self.take_count();
                        self.scroll_down(count);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('Y'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        let count = self.take_count();
                        self.scroll_up(count);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('D'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        let count = self.take_count();
                        self.scroll_half_page_down(count);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('U'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        let count = self.take_count();
                        self.scroll_half_page_up(count);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('V'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        self.mode = EditorMode::VisualBlock;
                        self.visual_start = self.cursor;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        ..
                    }) => {
                        if c.is_ascii_digit() && (c != '0' || self.count_prefix.is_some()) {
                            self.add_count_digit(c);
                            self.render()?;
                            continue;
                        }

                        if self.pending_operator.is_some() && !self.pending_keys.is_empty() {
                            let op = self.pending_operator.unwrap();
                            let first = self.pending_keys[0];
                            self.pending_operator = None;
                            self.pending_keys.clear();

                            if first == KeyCode::Char('g') && c == 'g' {
                                if op == 'd' {
                                    self.delete_to_start_of_file();
                                } else if op == 'c' {
                                    self.change_to_start_of_file();
                                } else if op == 'y' {
                                    self.yank_to_start_of_file();
                                }
                            } else if first == KeyCode::Char('g') && c == 'e' {
                                // dge / cge / yge - delete/change/yank backward to end of previous word
                                let count = self.take_count();
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.perform_delete_motion_with_count(
                                        |s| s.get_word_end_backward_pos(WordType::Word),
                                        count,
                                        true,
                                        false,
                                        false,
                                    );
                                    self.set_last_change(
                                        LastChange::Change(EditTarget::WordEnd(
                                            WordType::Word,
                                            Direction::Backward,
                                        )),
                                        count,
                                    );
                                } else if op == 'y' {
                                    self.perform_yank_motion_with_count(
                                        |s| s.get_word_end_backward_pos(WordType::Word),
                                        count,
                                        true,
                                    );
                                } else {
                                    self.perform_delete_motion_with_count(
                                        |s| s.get_word_end_backward_pos(WordType::Word),
                                        count,
                                        true,
                                        true,
                                        false,
                                    );
                                    self.set_last_change(
                                        LastChange::Delete(EditTarget::WordEnd(
                                            WordType::Word,
                                            Direction::Backward,
                                        )),
                                        count,
                                    );
                                }
                            } else if first == KeyCode::Char('g') && c == 'E' {
                                // dgE / cgE / ygE - delete/change/yank backward to end of previous WORD
                                let count = self.take_count();
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.perform_delete_motion_with_count(
                                        |s| s.get_word_end_backward_pos(WordType::LongWord),
                                        count,
                                        true,
                                        false,
                                        false,
                                    );
                                    self.set_last_change(
                                        LastChange::Change(EditTarget::WordEnd(
                                            WordType::LongWord,
                                            Direction::Backward,
                                        )),
                                        count,
                                    );
                                } else if op == 'y' {
                                    self.perform_yank_motion_with_count(
                                        |s| s.get_word_end_backward_pos(WordType::LongWord),
                                        count,
                                        true,
                                    );
                                } else {
                                    self.perform_delete_motion_with_count(
                                        |s| s.get_word_end_backward_pos(WordType::LongWord),
                                        count,
                                        true,
                                        true,
                                        false,
                                    );
                                    self.set_last_change(
                                        LastChange::Delete(EditTarget::WordEnd(
                                            WordType::LongWord,
                                            Direction::Backward,
                                        )),
                                        count,
                                    );
                                }
                            } else if first == KeyCode::Char('i') && c == 'w' {
                                // diw / ciw / yiw - delete/change/yank inner word
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::Inner(
                                        TextObject::Word(WordType::Word),
                                    ));
                                    self.delete_inner_word();
                                } else if op == 'y' {
                                    self.yank_inner_word();
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::Inner(
                                        TextObject::Word(WordType::Word),
                                    ));
                                    self.delete_inner_word();
                                }
                            } else if first == KeyCode::Char('a') && c == 'w' {
                                // daw / caw / yaw - delete/change/yank a word
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::Around(
                                        TextObject::Word(WordType::Word),
                                    ));
                                    self.delete_a_word();
                                } else if op == 'y' {
                                    self.yank_a_word();
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::Around(
                                        TextObject::Word(WordType::Word),
                                    ));
                                    self.delete_a_word();
                                }
                            } else if first == KeyCode::Char('i') && c == 'W' {
                                // diW / ciW / yiW - delete/change/yank inner WORD
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::Inner(
                                        TextObject::Word(WordType::LongWord),
                                    ));
                                    self.delete_inner_long_word();
                                } else if op == 'y' {
                                    self.yank_inner_long_word();
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::Inner(
                                        TextObject::Word(WordType::LongWord),
                                    ));
                                    self.delete_inner_long_word();
                                }
                            } else if first == KeyCode::Char('a') && c == 'W' {
                                // daW / caW / yaW - delete/change/yank a WORD
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::Around(
                                        TextObject::Word(WordType::LongWord),
                                    ));
                                    self.delete_a_long_word();
                                } else if op == 'y' {
                                    self.yank_a_long_word();
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::Around(
                                        TextObject::Word(WordType::LongWord),
                                    ));
                                    self.delete_a_long_word();
                                }
                            } else if first == KeyCode::Char('i')
                                && matches!(
                                    c,
                                    '(' | ')'
                                        | '['
                                        | ']'
                                        | '{'
                                        | '}'
                                        | '<'
                                        | '>'
                                        | '"'
                                        | '\''
                                        | '`'
                                )
                            {
                                // di( di) di[ di] di{ di} di< di> di" di' di` etc.
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change =
                                        LastChange::Change(EditTarget::Inner(TextObject::Pair(c)));
                                    self.delete_inner_pair(c);
                                } else if op == 'y' {
                                    self.yank_inner_pair(c);
                                } else {
                                    self.last_change =
                                        LastChange::Delete(EditTarget::Inner(TextObject::Pair(c)));
                                    self.delete_inner_pair(c);
                                }
                            } else if first == KeyCode::Char('a')
                                && matches!(
                                    c,
                                    '(' | ')'
                                        | '['
                                        | ']'
                                        | '{'
                                        | '}'
                                        | '<'
                                        | '>'
                                        | '"'
                                        | '\''
                                        | '`'
                                )
                            {
                                // da( da) da[ da] da{ da} da< da> da" da' da` etc.
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change =
                                        LastChange::Change(EditTarget::Around(TextObject::Pair(c)));
                                    self.delete_around_pair(c);
                                } else if op == 'y' {
                                    self.yank_around_pair(c);
                                } else {
                                    self.last_change =
                                        LastChange::Delete(EditTarget::Around(TextObject::Pair(c)));
                                    self.delete_around_pair(c);
                                }
                            } else if first == KeyCode::Char('i') && c == 'p' {
                                // dip / cip / yip - delete/change/yank inner paragraph
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change = LastChange::Change(EditTarget::Inner(
                                        TextObject::Paragraph,
                                    ));
                                    self.change_paragraph(TextObjectKind::Inner);
                                } else if op == 'y' {
                                    self.yank_paragraph(TextObjectKind::Inner);
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::Inner(
                                        TextObject::Paragraph,
                                    ));
                                    self.delete_paragraph(TextObjectKind::Inner);
                                }
                            } else if first == KeyCode::Char('a') && c == 'p' {
                                // dap / cap / yap - delete/change/yank a paragraph
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change = LastChange::Change(EditTarget::Around(
                                        TextObject::Paragraph,
                                    ));
                                    self.change_paragraph(TextObjectKind::Around);
                                } else if op == 'y' {
                                    self.yank_paragraph(TextObjectKind::Around);
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::Around(
                                        TextObject::Paragraph,
                                    ));
                                    self.delete_paragraph(TextObjectKind::Around);
                                }
                            } else if first == KeyCode::Char('i') && c == 's' {
                                // dis / cis / yis - delete/change/yank inner sentence
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change =
                                        LastChange::Change(EditTarget::Inner(TextObject::Sentence));
                                    self.change_sentence(TextObjectKind::Inner);
                                } else if op == 'y' {
                                    self.yank_sentence(TextObjectKind::Inner);
                                } else {
                                    self.last_change =
                                        LastChange::Delete(EditTarget::Inner(TextObject::Sentence));
                                    self.delete_sentence(TextObjectKind::Inner);
                                }
                            } else if first == KeyCode::Char('a') && c == 's' {
                                // das / cas / yas - delete/change/yank a sentence
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change = LastChange::Change(EditTarget::Around(
                                        TextObject::Sentence,
                                    ));
                                    self.change_sentence(TextObjectKind::Around);
                                } else if op == 'y' {
                                    self.yank_sentence(TextObjectKind::Around);
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::Around(
                                        TextObject::Sentence,
                                    ));
                                    self.delete_sentence(TextObjectKind::Around);
                                }
                            } else if first == KeyCode::Char('[') && (c == '(' || c == '{') {
                                // d[( d[{ c[( c[{ y[( y[{ - delete/change/yank to previous unmatched bracket
                                let (open, close) = if c == '(' { ('(', ')') } else { ('{', '}') };
                                if op == 'c' {
                                    self.mode = EditorMode::Insert;
                                    self.delete_to_prev_unmatched(open, close);
                                } else if op == 'y' {
                                    self.yank_to_prev_unmatched(open, close);
                                } else {
                                    self.delete_to_prev_unmatched(open, close);
                                }
                            } else if first == KeyCode::Char(']') && (c == ')' || c == '}') {
                                // d]) d]} c]) c]} y]) y]} - delete/change/yank to next unmatched bracket
                                let (open, close) = if c == ')' { ('(', ')') } else { ('{', '}') };
                                if op == 'c' {
                                    self.mode = EditorMode::Insert;
                                    self.delete_to_next_unmatched(open, close);
                                } else if op == 'y' {
                                    self.yank_to_next_unmatched(open, close);
                                } else {
                                    self.delete_to_next_unmatched(open, close);
                                }
                            } else if first == KeyCode::Char('f') {
                                // df{char} / cf{char} / yf{char} - delete/change/yank to char (inclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::ToChar(
                                        c,
                                        CharSearchType::Find,
                                        Direction::Forward,
                                    ));
                                    self.delete_to_char_forward(c, true);
                                } else if op == 'y' {
                                    self.yank_to_char_forward(c, true);
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::ToChar(
                                        c,
                                        CharSearchType::Find,
                                        Direction::Forward,
                                    ));
                                    self.delete_to_char_forward(c, true);
                                }
                            } else if first == KeyCode::Char('F') {
                                // dF{char} / cF{char} / yF{char} - delete/change/yank backward to char (inclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::ToChar(
                                        c,
                                        CharSearchType::Find,
                                        Direction::Backward,
                                    ));
                                    self.delete_to_char_backward(c, true);
                                } else if op == 'y' {
                                    self.yank_to_char_backward(c, true);
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::ToChar(
                                        c,
                                        CharSearchType::Find,
                                        Direction::Backward,
                                    ));
                                    self.delete_to_char_backward(c, true);
                                }
                            } else if first == KeyCode::Char('t') {
                                // dt{char} / ct{char} / yt{char} - delete/change/yank till char (exclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::ToChar(
                                        c,
                                        CharSearchType::To,
                                        Direction::Forward,
                                    ));
                                    self.delete_to_char_forward(c, false);
                                } else if op == 'y' {
                                    self.yank_to_char_forward(c, false);
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::ToChar(
                                        c,
                                        CharSearchType::To,
                                        Direction::Forward,
                                    ));
                                    self.delete_to_char_forward(c, false);
                                }
                            } else if first == KeyCode::Char('T') {
                                // dT{char} / cT{char} / yT{char} - delete/change/yank backward till char (exclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::Change(EditTarget::ToChar(
                                        c,
                                        CharSearchType::To,
                                        Direction::Backward,
                                    ));
                                    self.delete_to_char_backward(c, false);
                                } else if op == 'y' {
                                    self.yank_to_char_backward(c, false);
                                } else {
                                    self.last_change = LastChange::Delete(EditTarget::ToChar(
                                        c,
                                        CharSearchType::To,
                                        Direction::Backward,
                                    ));
                                    self.delete_to_char_backward(c, false);
                                }
                            }

                            self.render()?;
                            continue;
                        }

                        if self.pending_operator.is_some() {
                            let op = self.pending_operator.unwrap();
                            self.pending_operator = None;

                            if op == 'c' {
                                match c {
                                    'c' => {
                                        let count = self.take_count();
                                        self.substitute_lines(count); // cc == S
                                        self.set_last_change(
                                            LastChange::Change(EditTarget::Line),
                                            count,
                                        );
                                    }
                                    'w' => {
                                        // cw behavior depends on what we're on:
                                        // - On a word: cw is like ce (change to end of word)
                                        // - On whitespace: cw uses w motion (change whitespace to start of next word)
                                        // - On punctuation: cw is like ce (change to end of punctuation)
                                        let count = self.take_count();
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        let chars: Vec<char> =
                                            self.lines[self.cursor.0].chars().collect();
                                        let on_whitespace = self.cursor.1 < chars.len()
                                            && chars[self.cursor.1].is_whitespace();
                                        if on_whitespace {
                                            self.perform_delete_motion_with_count(
                                                |s| s.get_word_forward_pos(WordType::Word),
                                                count,
                                                false,
                                                false,
                                                false,
                                            );
                                        } else {
                                            self.perform_delete_motion_with_count(
                                                |s| s.get_word_end_pos(WordType::Word),
                                                count,
                                                true,
                                                false,
                                                false,
                                            );
                                        }
                                        self.set_last_change(
                                            LastChange::Change(EditTarget::WordStart(
                                                WordType::Word,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'W' => {
                                        // cW behavior: like cw but for WORD
                                        let count = self.take_count();
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        let chars: Vec<char> =
                                            self.lines[self.cursor.0].chars().collect();
                                        let on_whitespace = self.cursor.1 < chars.len()
                                            && chars[self.cursor.1].is_whitespace();
                                        if on_whitespace {
                                            self.perform_delete_motion_with_count(
                                                |s| s.get_word_forward_pos(WordType::LongWord),
                                                count,
                                                false,
                                                false,
                                                false,
                                            );
                                        } else {
                                            self.perform_delete_motion_with_count(
                                                |s| s.get_word_end_pos(WordType::LongWord),
                                                count,
                                                true,
                                                false,
                                                false,
                                            );
                                        }
                                        self.set_last_change(
                                            LastChange::Change(EditTarget::WordStart(
                                                WordType::LongWord,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'e' => {
                                        let count = self.take_count();
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_end_pos(WordType::Word),
                                            count,
                                            true,
                                            false,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Change(EditTarget::WordEnd(
                                                WordType::Word,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'E' => {
                                        let count = self.take_count();
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_end_pos(WordType::LongWord),
                                            count,
                                            true,
                                            false,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Change(EditTarget::WordEnd(
                                                WordType::LongWord,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'b' => {
                                        let count = self.take_count();
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_backward_pos(WordType::Word),
                                            count,
                                            false,
                                            false,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Change(EditTarget::WordStart(
                                                WordType::Word,
                                                Direction::Backward,
                                            )),
                                            count,
                                        );
                                    }
                                    'B' => {
                                        let count = self.take_count();
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_backward_pos(WordType::LongWord),
                                            count,
                                            false,
                                            false,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Change(EditTarget::WordStart(
                                                WordType::LongWord,
                                                Direction::Backward,
                                            )),
                                            count,
                                        );
                                    }
                                    '$' => self.change_to_end_of_line(),
                                    '^' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_first_non_blank_pos(),
                                            false,
                                            false,
                                            false,
                                        );
                                    }
                                    '0' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_line_start_pos(),
                                            false,
                                            false,
                                            false,
                                        );
                                    }
                                    'h' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_char_left_pos(),
                                            false,
                                            false,
                                            false,
                                        );
                                    }
                                    'l' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_char_right_pos(),
                                            false,
                                            false,
                                            false,
                                        );
                                    }
                                    'j' => {
                                        self.change_line_and_below();
                                    }
                                    'k' => {
                                        self.change_line_and_above();
                                    }
                                    'G' => self.change_to_end_of_file(),
                                    'g' => {
                                        // Wait for second 'g' to complete 'cgg'
                                        self.pending_keys.push(KeyCode::Char('g'));
                                        self.pending_operator = Some('c');
                                        self.render()?;
                                        continue;
                                    }
                                    'i' | 'a' => {
                                        // Wait for text object (e.g., 'w' for ciw/caw)
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('c');
                                        self.render()?;
                                        continue;
                                    }
                                    '%' => {
                                        self.delete_to_matching_bracket();
                                        self.mode = EditorMode::Insert;
                                    }
                                    '(' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_sentence_backward_pos(),
                                            false,
                                            false,
                                            true,
                                        );
                                    }
                                    ')' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_sentence_forward_pos(),
                                            false,
                                            false,
                                            true,
                                        );
                                    }
                                    '{' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_paragraph_backward_pos(),
                                            false,
                                            false,
                                            true,
                                        );
                                    }
                                    '}' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_paragraph_forward_pos(),
                                            false,
                                            false,
                                            true,
                                        );
                                    }
                                    '[' | ']' | 'f' | 'F' | 't' | 'T' => {
                                        // Wait for target char/bracket
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('c');
                                        self.render()?;
                                        continue;
                                    }
                                    _ => { /* Ignore other motions for now */ }
                                }
                            } else if op == 'd' {
                                match c {
                                    'd' => {
                                        let count = self.take_count();
                                        self.delete_lines(count);
                                        self.set_last_change(
                                            LastChange::Delete(EditTarget::Line),
                                            count,
                                        );
                                    }
                                    'w' => {
                                        let count = self.take_count();
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_forward_pos(WordType::Word),
                                            count,
                                            false,
                                            true,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Delete(EditTarget::WordStart(
                                                WordType::Word,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'W' => {
                                        let count = self.take_count();
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_forward_pos(WordType::LongWord),
                                            count,
                                            false,
                                            true,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Delete(EditTarget::WordStart(
                                                WordType::LongWord,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'e' => {
                                        let count = self.take_count();
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_end_pos(WordType::Word),
                                            count,
                                            true,
                                            true,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Delete(EditTarget::WordEnd(
                                                WordType::Word,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'E' => {
                                        let count = self.take_count();
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_end_pos(WordType::LongWord),
                                            count,
                                            true,
                                            true,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Delete(EditTarget::WordEnd(
                                                WordType::LongWord,
                                                Direction::Forward,
                                            )),
                                            count,
                                        );
                                    }
                                    'b' => {
                                        let count = self.take_count();
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_backward_pos(WordType::Word),
                                            count,
                                            false,
                                            true,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Delete(EditTarget::WordStart(
                                                WordType::Word,
                                                Direction::Backward,
                                            )),
                                            count,
                                        );
                                    }
                                    'B' => {
                                        let count = self.take_count();
                                        self.perform_delete_motion_with_count(
                                            |s| s.get_word_backward_pos(WordType::LongWord),
                                            count,
                                            false,
                                            true,
                                            false,
                                        );
                                        self.set_last_change(
                                            LastChange::Delete(EditTarget::WordStart(
                                                WordType::LongWord,
                                                Direction::Backward,
                                            )),
                                            count,
                                        );
                                    }
                                    '$' => self.delete_to_end_of_line(),
                                    '^' => self.perform_delete_motion(
                                        |s| s.get_first_non_blank_pos(),
                                        false,
                                        true,
                                        false,
                                    ),
                                    '0' => self.perform_delete_motion(
                                        |s| s.get_line_start_pos(),
                                        false,
                                        true,
                                        false,
                                    ),
                                    'h' => self.perform_delete_motion(
                                        |s| s.get_char_left_pos(),
                                        false,
                                        true,
                                        false,
                                    ),
                                    'l' => self.perform_delete_motion(
                                        |s| s.get_char_right_pos(),
                                        false,
                                        true,
                                        false,
                                    ),
                                    'j' => self.delete_line_and_below(),
                                    'k' => self.delete_line_and_above(),
                                    'G' => self.delete_to_end_of_file(),
                                    'g' => {
                                        // Wait for second 'g' to complete 'dgg'
                                        self.pending_keys.push(KeyCode::Char('g'));
                                        self.pending_operator = Some('d');
                                        self.render()?;
                                        continue;
                                    }
                                    'i' | 'a' => {
                                        // Wait for text object (e.g., 'w' for diw/daw)
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('d');
                                        self.render()?;
                                        continue;
                                    }
                                    '%' => self.delete_to_matching_bracket(),
                                    '(' => self.perform_delete_motion(
                                        |s| s.get_sentence_backward_pos(),
                                        false,
                                        true,
                                        true,
                                    ),
                                    ')' => self.perform_delete_motion(
                                        |s| s.get_sentence_forward_pos(),
                                        false,
                                        true,
                                        true,
                                    ),
                                    '{' => self.perform_delete_motion(
                                        |s| s.get_paragraph_backward_pos(),
                                        false,
                                        true,
                                        true,
                                    ),
                                    '}' => self.perform_delete_motion(
                                        |s| s.get_paragraph_forward_pos(),
                                        false,
                                        true,
                                        true,
                                    ),
                                    '[' | ']' | 'f' | 'F' | 't' | 'T' => {
                                        // Wait for target char/bracket
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('d');
                                        self.render()?;
                                        continue;
                                    }
                                    _ => {}
                                }
                            } else if op == 'y' {
                                match c {
                                    'y' => {
                                        let count = self.take_count();
                                        self.yank_lines(count);
                                    }
                                    'w' => {
                                        let count = self.take_count();
                                        self.perform_yank_motion_with_count(
                                            |s| s.get_word_forward_pos(WordType::Word),
                                            count,
                                            false,
                                        );
                                    }
                                    'W' => {
                                        let count = self.take_count();
                                        self.perform_yank_motion_with_count(
                                            |s| s.get_word_forward_pos(WordType::LongWord),
                                            count,
                                            false,
                                        );
                                    }
                                    'e' => {
                                        let count = self.take_count();
                                        self.perform_yank_motion_with_count(
                                            |s| s.get_word_end_pos(WordType::Word),
                                            count,
                                            true,
                                        );
                                    }
                                    'E' => {
                                        let count = self.take_count();
                                        self.perform_yank_motion_with_count(
                                            |s| s.get_word_end_pos(WordType::LongWord),
                                            count,
                                            true,
                                        );
                                    }
                                    'b' => {
                                        let count = self.take_count();
                                        self.perform_yank_motion_with_count(
                                            |s| s.get_word_backward_pos(WordType::Word),
                                            count,
                                            false,
                                        );
                                    }
                                    'B' => {
                                        let count = self.take_count();
                                        self.perform_yank_motion_with_count(
                                            |s| s.get_word_backward_pos(WordType::LongWord),
                                            count,
                                            false,
                                        );
                                    }
                                    '$' => self.yank_to_end_of_line(),
                                    '^' => self.perform_yank_motion(
                                        |s| s.get_first_non_blank_pos(),
                                        false,
                                    ),
                                    '0' => {
                                        self.perform_yank_motion(|s| s.get_line_start_pos(), false)
                                    }
                                    'h' => {
                                        self.perform_yank_motion(|s| s.get_char_left_pos(), false)
                                    }
                                    'l' => {
                                        self.perform_yank_motion(|s| s.get_char_right_pos(), false)
                                    }
                                    'j' => self.yank_line_and_below(),
                                    'k' => self.yank_line_and_above(),
                                    'G' => self.yank_to_end_of_file(),
                                    'g' => {
                                        // Wait for second 'g' to complete 'ygg'
                                        self.pending_keys.push(KeyCode::Char('g'));
                                        self.pending_operator = Some('y');
                                        self.render()?;
                                        continue;
                                    }
                                    'i' | 'a' => {
                                        // Wait for text object (e.g., 'w' for yiw/yaw)
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('y');
                                        self.render()?;
                                        continue;
                                    }
                                    '%' => self.yank_to_matching_bracket(),
                                    '(' => self.perform_yank_motion(
                                        |s| s.get_sentence_backward_pos(),
                                        false,
                                    ),
                                    ')' => self.perform_yank_motion(
                                        |s| s.get_sentence_forward_pos(),
                                        false,
                                    ),
                                    '{' => self.perform_yank_motion(
                                        |s| s.get_paragraph_backward_pos(),
                                        false,
                                    ),
                                    '}' => self.perform_yank_motion(
                                        |s| s.get_paragraph_forward_pos(),
                                        false,
                                    ),
                                    '[' | ']' | 'f' | 'F' | 't' | 'T' => {
                                        // Wait for target char/bracket
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('y');
                                        self.render()?;
                                        continue;
                                    }
                                    _ => {}
                                }
                            }

                            self.render()?;
                            continue;
                        }

                        if !self.pending_keys.is_empty() {
                            let first = self.pending_keys[0];
                            if first == KeyCode::Char('g') && c == 'g' {
                                self.cursor.0 = 0;
                                // Use desired_col like vertical movement
                                let line_len = self.lines[self.cursor.0].chars().count();
                                let max_col = if self.mode == EditorMode::Insert {
                                    line_len
                                } else {
                                    line_len.saturating_sub(1)
                                };
                                self.cursor.1 = self.desired_col.min(max_col);
                            } else if first == KeyCode::Char('g') && c == 'e' {
                                // ge - move backward to end of previous word
                                self.move_to_word_end_backward(WordType::Word);
                            } else if first == KeyCode::Char('g') && c == 'E' {
                                // gE - move backward to end of previous WORD
                                self.move_to_word_end_backward(WordType::LongWord);
                            } else if first == KeyCode::Char('z') && c == 'z' {
                                // zz - scroll cursor line to center of screen
                                self.scroll_cursor_to_center();
                            } else if first == KeyCode::Char('z') && c == 't' {
                                // zt - scroll cursor line to top of screen
                                self.scroll_cursor_to_top();
                            } else if first == KeyCode::Char('z') && c == 'b' {
                                // zb - scroll cursor line to bottom of screen
                                self.scroll_cursor_to_bottom();
                            } else if first == KeyCode::Char('Z') && c == 'Z' {
                                self.submit();
                                break;
                            } else if first == KeyCode::Char('Z') && c == 'Q' {
                                self.cancel();
                                break;
                            } else if first == KeyCode::Char('[') && c == '(' {
                                self.jump_to_prev_unmatched('(', ')');
                                self.update_desired_col();
                            } else if first == KeyCode::Char('[') && c == '{' {
                                self.jump_to_prev_unmatched('{', '}');
                                self.update_desired_col();
                            } else if first == KeyCode::Char(']') && c == ')' {
                                self.jump_to_next_unmatched('(', ')');
                                self.update_desired_col();
                            } else if first == KeyCode::Char(']') && c == '}' {
                                self.jump_to_next_unmatched('{', '}');
                                self.update_desired_col();
                            } else if first == KeyCode::Char('f') {
                                self.move_to_char_forward(c);
                                self.last_char_search = Some(('f', c));
                                self.update_desired_col();
                            } else if first == KeyCode::Char('F') {
                                self.move_to_char_backward(c);
                                self.last_char_search = Some(('F', c));
                                self.update_desired_col();
                            } else if first == KeyCode::Char('t') {
                                self.move_till_char_forward(c);
                                self.last_char_search = Some(('t', c));
                                self.update_desired_col();
                            } else if first == KeyCode::Char('T') {
                                self.move_till_char_backward(c);
                                self.last_char_search = Some(('T', c));
                                self.update_desired_col();
                            } else if first == KeyCode::Char('r') {
                                self.replace_char(c);
                                self.last_change = LastChange::ReplaceChar(c);
                            }
                            self.pending_keys.clear();
                            self.render()?;
                            continue;
                        }

                        if c == 'g'
                            || c == 'z'
                            || c == 'Z'
                            || c == '['
                            || c == ']'
                            || c == 'f'
                            || c == 'F'
                            || c == 't'
                            || c == 'T'
                            || c == 'r'
                        {
                            self.pending_keys.push(KeyCode::Char(c));
                            self.render()?;
                            continue;
                        }

                        if c == 'c' || c == 'd' || c == 'y' {
                            self.pending_operator = Some(c);
                            self.render()?;
                            continue;
                        }

                        match c {
                            'i' => {
                                self.save_undo_state(); // Save state before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::Before;
                                self.mode = EditorMode::Insert;
                            }
                            'I' => {
                                self.save_undo_state(); // Save state before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::LineStart;
                                self.cursor.1 = 0;
                                let line = &self.lines[self.cursor.0];
                                for (i, ch) in line.chars().enumerate() {
                                    if !ch.is_whitespace() {
                                        self.cursor.1 = i;
                                        break;
                                    }
                                }
                                self.mode = EditorMode::Insert;
                            }
                            'a' => {
                                self.save_undo_state(); // Save state before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::After;
                                self.mode = EditorMode::Insert;
                                self.move_cursor(0, 1);
                            }
                            'A' => {
                                self.save_undo_state(); // Save state before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::LineEnd;
                                self.cursor.1 = self.lines[self.cursor.0].len();
                                self.mode = EditorMode::Insert;
                            }
                            'R' => {
                                self.save_undo_state(); // Save state before replace
                                self.insert_buffer.clear();
                                self.replace_originals.clear();
                                self.replace_start_pos = self.cursor;
                                self.mode = EditorMode::Replace;
                            }
                            'o' => {
                                self.save_undo_state(); // Save state before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::NewLineBelow;
                                self.lines_version += 1;
                                self.lines.insert(self.cursor.0 + 1, String::new());
                                self.cursor.0 += 1;
                                self.cursor.1 = 0;
                                self.mode = EditorMode::Insert;
                            }
                            'O' => {
                                self.save_undo_state(); // Save state before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::NewLineAbove;
                                self.lines_version += 1;
                                self.lines.insert(self.cursor.0, String::new());
                                self.cursor.1 = 0;
                                self.mode = EditorMode::Insert;
                            }
                            'h' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_cursor(0, -1);
                                }
                            }
                            'j' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_cursor(1, 0);
                                }
                            }
                            'k' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_cursor(-1, 0);
                                }
                            }
                            'l' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_cursor(0, 1);
                                }
                            }
                            'w' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_word_forward(WordType::Word);
                                }
                            }
                            'W' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_word_forward(WordType::LongWord);
                                }
                            }
                            'e' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_to_word_end(WordType::Word);
                                }
                            }
                            'E' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_to_word_end(WordType::LongWord);
                                }
                            }
                            'b' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_word_backward(WordType::Word);
                                }
                            }
                            'B' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.move_word_backward(WordType::LongWord);
                                }
                            }
                            'x' => {
                                let count = self.take_count();
                                self.save_undo_state();
                                for _ in 0..count {
                                    self.delete_char_no_undo();
                                }
                                self.record_change();
                                self.set_last_change(LastChange::Delete(EditTarget::Char), count);
                            }
                            'X' => {
                                let count = self.take_count();
                                self.save_undo_state();
                                for _ in 0..count {
                                    if self.cursor.1 > 0 {
                                        self.cursor.1 -= 1;
                                        self.delete_char_no_undo();
                                    }
                                }
                                self.record_change();
                                self.set_last_change(
                                    LastChange::Delete(EditTarget::CharBackward),
                                    count,
                                );
                            }
                            'u' => {
                                self.undo();
                                self.update_desired_col();
                            }
                            '0' => {
                                self.cursor.1 = 0;
                                self.update_desired_col();
                            }
                            '^' => {
                                self.move_to_first_non_blank();
                                self.update_desired_col();
                            }
                            '$' => {
                                self.cursor.1 =
                                    self.lines[self.cursor.0].chars().count().saturating_sub(1);
                                self.update_desired_col();
                            }
                            'G' => {
                                // With count: go to line N, without count: go to last line
                                let target_line = if let Some(count) = self.count_prefix.take() {
                                    // Line numbers are 1-based
                                    (count.saturating_sub(1)).min(self.lines.len() - 1)
                                } else {
                                    self.lines.len() - 1
                                };
                                self.cursor.0 = target_line;
                                // Use desired_col like vertical movement
                                let line_len = self.lines[self.cursor.0].chars().count();
                                let max_col = if self.mode == EditorMode::Insert {
                                    line_len
                                } else {
                                    line_len.saturating_sub(1)
                                };
                                self.cursor.1 = self.desired_col.min(max_col);
                            }
                            'H' => {
                                let count = self.take_count();
                                self.move_to_screen_position(ScreenPosition::Top(count));
                            }
                            'M' => {
                                self.count_prefix = None; // M ignores count
                                self.move_to_screen_position(ScreenPosition::Middle);
                            }
                            'L' => {
                                let count = self.take_count();
                                self.move_to_screen_position(ScreenPosition::Bottom(count));
                            }
                            'D' => {
                                self.delete_to_end_of_line();
                                self.last_change = LastChange::Delete(EditTarget::ToEndOfLine);
                            }
                            'C' => {
                                self.insert_buffer.clear();
                                self.change_to_end_of_line();
                                self.last_change = LastChange::Change(EditTarget::ToEndOfLine);
                            }
                            'S' => {
                                self.insert_buffer.clear();
                                self.substitute_line();
                                self.last_change = LastChange::Change(EditTarget::Line);
                            }
                            's' => {
                                self.insert_buffer.clear();
                                self.substitute_char();
                                self.last_change = LastChange::Change(EditTarget::Char);
                            }
                            'J' => {
                                self.join_lines();
                                self.last_change = LastChange::JoinLines;
                            }
                            '~' => {
                                self.toggle_case();
                                self.last_change = LastChange::ToggleCase;
                            }
                            '%' => {
                                self.jump_to_matching_bracket();
                                self.update_desired_col();
                            }
                            '{' => {
                                self.move_paragraph_backward();
                            }
                            '}' => {
                                self.move_paragraph_forward();
                            }
                            '(' => {
                                self.move_sentence_backward();
                            }
                            ')' => {
                                self.move_sentence_forward();
                            }
                            '.' => {
                                self.repeat_last_change();
                                self.update_desired_col();
                            }
                            ';' => {
                                self.repeat_char_search(false); // Same direction
                                self.update_desired_col();
                            }
                            ',' => {
                                self.repeat_char_search(true); // Opposite direction
                                self.update_desired_col();
                            }
                            'p' => {
                                let count = self.take_count();
                                self.paste_after_count(count);
                                self.set_last_change(LastChange::PasteAfter, count);
                                self.update_desired_col();
                            }
                            'P' => {
                                let count = self.take_count();
                                self.paste_before_count(count);
                                self.set_last_change(LastChange::PasteBefore, count);
                                self.update_desired_col();
                            }
                            'Y' => self.yank_to_end_of_line(), // Y yanks to end of line (like y$)
                            '/' => {
                                // Enter forward search mode
                                self.search_saved_pattern = self.search_pattern.clone();
                                self.mode = EditorMode::Search;
                                self.search_direction = Direction::Forward;
                                self.search_input.clear();
                                self.search_start_pos = self.cursor;
                            }
                            '?' => {
                                // Enter backward search mode
                                self.search_saved_pattern = self.search_pattern.clone();
                                self.mode = EditorMode::Search;
                                self.search_direction = Direction::Backward;
                                self.search_input.clear();
                                self.search_start_pos = self.cursor;
                            }
                            'n' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.search_next();
                                }
                                // Display same direction as original search
                                self.search_display_direction = self.search_direction;
                                self.update_desired_col();
                            }
                            'N' => {
                                let count = self.take_count();
                                for _ in 0..count {
                                    self.search_prev();
                                }
                                // Display opposite direction (searching in reverse)
                                self.search_display_direction = match self.search_direction {
                                    Direction::Forward => Direction::Backward,
                                    Direction::Backward => Direction::Forward,
                                };
                                self.update_desired_col();
                            }
                            '*' => {
                                self.search_word_under_cursor(true); // Forward
                                self.update_desired_col();
                            }
                            '#' => {
                                self.search_word_under_cursor(false); // Backward
                                self.update_desired_col();
                            }
                            'v' => {
                                // Enter character-wise visual mode
                                self.mode = EditorMode::Visual;
                                self.visual_start = self.cursor;
                            }
                            'V' => {
                                // Enter line-wise visual mode
                                self.mode = EditorMode::VisualLine;
                                self.visual_start = self.cursor;
                            }
                            _ => {}
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        self.pending_keys.clear();
                        self.pending_operator = None;
                        self.count_prefix = None;
                        // Turn off search highlighting and clear search pattern
                        self.search_highlight = false;
                        self.search_pattern.clear();
                        self.current_match = None;
                    }
                    InputEvent::Paste(text) => {
                        // In Normal mode, paste inserts after cursor (like vim p)
                        // Move cursor right first (to insert after current position)
                        let line = &self.lines[self.cursor.0];
                        let line_len = line.chars().count();
                        if line_len > 0 && self.cursor.1 < line_len {
                            self.cursor.1 += 1;
                        }
                        self.insert_text(&text);
                        // Move cursor back to last inserted char (vim behavior)
                        if self.cursor.1 > 0 {
                            self.cursor.1 -= 1;
                        }
                        self.clamp_cursor();
                        // Record the change so undo works correctly
                        self.record_change();
                    }
                    _ => {}
                },
                EditorMode::Insert => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        // Handle block insert: insert text on all affected rows
                        let block_info = self.block_insert_info.take();
                        if let Some((start_row, num_rows, insert_col, block_type)) = block_info {
                            // Insert the typed text on rows 1..num_rows (row 0 already has the text)
                            let text_to_insert = self.insert_buffer.clone();
                            if !text_to_insert.is_empty() {
                                for i in 1..num_rows {
                                    let row = start_row + i;
                                    if row >= self.lines.len() {
                                        break;
                                    }
                                    let chars: Vec<char> = self.lines[row].chars().collect();

                                    // For I (Insert): skip lines shorter than insert_col
                                    // For A (Append): pad with spaces to reach insert_col
                                    // For Change: skip lines shorter than insert_col
                                    match block_type {
                                        BlockInsertType::Insert | BlockInsertType::Change(_) => {
                                            if chars.len() < insert_col {
                                                continue;
                                            }
                                        }
                                        BlockInsertType::Append(_) => {
                                            // Don't skip - we'll pad with spaces below
                                        }
                                    }

                                    let mut chars = chars;

                                    // Pad with spaces if needed (for A command)
                                    while chars.len() < insert_col {
                                        chars.push(' ');
                                    }

                                    // Insert the text
                                    let mut offset = 0;
                                    for c in text_to_insert.chars() {
                                        if c == '\n' {
                                            // Skip newlines in block insert
                                            continue;
                                        }
                                        chars.insert(insert_col + offset, c);
                                        offset += 1;
                                    }
                                    self.lines[row] = chars.into_iter().collect();
                                }
                            }

                            // Record the appropriate LastChange for repeat
                            if !text_to_insert.is_empty() {
                                match block_type {
                                    BlockInsertType::Change(col_width) => {
                                        self.last_change = LastChange::ChangeBlock(
                                            num_rows,
                                            col_width,
                                            text_to_insert,
                                        );
                                        self.last_count = 1;
                                    }
                                    BlockInsertType::Insert => {
                                        self.last_change =
                                            LastChange::InsertBlock(num_rows, text_to_insert);
                                        self.last_count = 1;
                                    }
                                    BlockInsertType::Append(col_offset) => {
                                        self.last_change = LastChange::AppendBlock(
                                            num_rows,
                                            col_offset,
                                            text_to_insert,
                                        );
                                        self.last_count = 1;
                                    }
                                }
                            }

                            self.mode = EditorMode::Normal;
                            // Move cursor left first (Vim behavior when leaving Insert mode)
                            if self.cursor.1 > 0 {
                                self.cursor.1 -= 1;
                            }
                            self.clamp_cursor();
                            self.update_desired_col();
                            self.record_change();
                        } else {
                            self.mode = EditorMode::Normal;
                            // Move cursor left first (Vim behavior when leaving Insert mode)
                            if self.cursor.1 > 0 {
                                self.cursor.1 -= 1;
                            }
                            // Then clamp to ensure we're within line bounds
                            self.clamp_cursor();
                            // Update desired_col after insert
                            self.update_desired_col();
                            // Record state after insert session and cursor adjustment for redo to work
                            self.record_change();
                            // Save insert buffer as last change if we have text and it's not a change operation
                            if !self.insert_buffer.is_empty() {
                                match &self.last_change {
                                    LastChange::Change(_) => {
                                        // Keep the change operation as last_change
                                    }
                                    _ => {
                                        self.last_change = LastChange::InsertText(
                                            self.insert_buffer.clone(),
                                            self.insert_style,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        modifiers,
                    }) => {
                        if !modifiers.contains(Modifiers::CTRL)
                            && !modifiers.contains(Modifiers::ALT)
                        {
                            self.insert_char(c);
                            self.insert_buffer.push(c);
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) => {
                        if self.cursor.1 > 0 {
                            self.cursor.1 -= 1;
                            self.delete_char();
                        } else if self.cursor.0 > 0 {
                            // Join lines logic
                            self.lines_version += 1;
                            let current_line = self.lines.remove(self.cursor.0);
                            self.cursor.0 -= 1;
                            self.cursor.1 = self.lines[self.cursor.0].len();
                            self.lines[self.cursor.0].push_str(&current_line);
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        self.insert_newline();
                        self.insert_buffer.push('\n');
                    }
                    InputEvent::Paste(text) => {
                        self.insert_text(&text);
                        self.insert_buffer.push_str(&text);
                    }
                    _ => {}
                },
                EditorMode::Replace => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        self.mode = EditorMode::Normal;
                        // Move cursor left first (Vim behavior when leaving Replace mode)
                        if self.cursor.1 > 0 {
                            self.cursor.1 -= 1;
                        }
                        self.clamp_cursor();
                        self.update_desired_col();
                        self.record_change();
                        // Save replace buffer as last change
                        if !self.insert_buffer.is_empty() {
                            self.last_change = LastChange::ReplaceMode(self.insert_buffer.clone());
                        }
                        self.replace_originals.clear();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        modifiers,
                    }) => {
                        if !modifiers.contains(Modifiers::CTRL)
                            && !modifiers.contains(Modifiers::ALT)
                        {
                            let original = self.replace_char_at_cursor(c);
                            self.replace_originals.push(original);
                            self.insert_buffer.push(c);
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) => {
                        if let Some(original) = self.replace_originals.pop() {
                            if self.cursor.1 > 0 {
                                self.cursor.1 -= 1;
                                if let Some(orig_char) = original {
                                    self.restore_char_at_cursor(orig_char);
                                } else {
                                    // Was inserted (no original) - delete it
                                    self.delete_char();
                                }
                                self.insert_buffer.pop();
                            }
                        } else if self.cursor.1 > self.replace_start_pos.1
                            || self.cursor.0 > self.replace_start_pos.0
                        {
                            // Allow backspace only if we're past the start position
                            if self.cursor.1 > 0 {
                                self.cursor.1 -= 1;
                            }
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        // In Replace mode, Enter typically inserts a newline
                        self.replace_originals.push(None);
                        self.insert_newline();
                        self.insert_buffer.push('\n');
                    }
                    InputEvent::Paste(text) => {
                        // In Replace mode, paste replaces characters
                        for c in text.chars() {
                            if c == '\n' {
                                self.replace_originals.push(None);
                                self.insert_newline();
                                self.insert_buffer.push('\n');
                            } else if c == '\r' {
                                // Skip carriage returns
                                continue;
                            } else {
                                let original = self.replace_char_at_cursor(c);
                                self.replace_originals.push(original);
                                self.insert_buffer.push(c);
                            }
                        }
                    }
                    _ => {}
                },
                EditorMode::Search => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        // Cancel search, go back to normal mode and restore cursor
                        self.mode = EditorMode::Normal;
                        self.cursor = self.search_start_pos;
                        self.search_input.clear();
                        // Restore previous search pattern if it exists in the document
                        let pattern_exists = !self.search_saved_pattern.is_empty()
                            && self
                                .lines
                                .iter()
                                .any(|line| line.contains(&self.search_saved_pattern));
                        if pattern_exists {
                            self.search_pattern = self.search_saved_pattern.clone();
                            self.search_highlight = true;
                            self.current_match = Some(self.cursor);
                        } else {
                            self.search_pattern.clear();
                            self.search_highlight = false;
                            self.current_match = None;
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        // Confirm search - cursor is already on match from incremental search
                        self.mode = EditorMode::Normal;
                        // Only set pattern and enable highlighting if a match was found
                        if self.current_match.is_some() {
                            self.search_pattern = self.search_input.clone();
                            self.search_highlight = true;
                            self.search_display_direction = self.search_direction;
                        } else {
                            // No match found - don't display the pattern
                            self.search_pattern.clear();
                            self.search_highlight = false;
                        }
                        self.search_input.clear();
                        self.update_desired_col();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) => {
                        self.search_input.pop();
                        // Incremental search: update as we type
                        self.perform_incremental_search();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        modifiers,
                    }) => {
                        if !modifiers.contains(Modifiers::CTRL)
                            && !modifiers.contains(Modifiers::ALT)
                        {
                            self.search_input.push(c);
                            // Incremental search: update as we type
                            self.perform_incremental_search();
                        }
                    }
                    InputEvent::Paste(text) => {
                        // In Search mode, paste text into search input (skip newlines)
                        for c in text.chars() {
                            if c != '\n' && c != '\r' {
                                self.search_input.push(c);
                            }
                        }
                        self.perform_incremental_search();
                    }
                    _ => {}
                },
                EditorMode::Visual | EditorMode::VisualLine | EditorMode::VisualBlock => {
                    match event {
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Escape,
                            ..
                        }) => {
                            self.mode = EditorMode::Normal;
                        }
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Char('V'),
                            modifiers: Modifiers::CTRL,
                        }) => {
                            // Switch to/from visual block mode
                            if self.mode == EditorMode::VisualBlock {
                                self.mode = EditorMode::Normal;
                            } else {
                                self.mode = EditorMode::VisualBlock;
                            }
                        }
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Char('D'),
                            modifiers: Modifiers::CTRL,
                        }) => {
                            let count = self.take_count();
                            self.scroll_half_page_down(count);
                        }
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Char('U'),
                            modifiers: Modifiers::CTRL,
                        }) => {
                            let count = self.take_count();
                            self.scroll_half_page_up(count);
                        }
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Char(c),
                            ..
                        }) => {
                            // Check for pending text object keys FIRST
                            if let Some(KeyCode::Char(pending)) = self.pending_keys.first().copied()
                            {
                                let handled = match (pending, c) {
                                    ('i', '(' | ')') => {
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('(')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', '(' | ')') => {
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('(')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', '[' | ']') => {
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('[')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', '[' | ']') => {
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('[')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', '{' | '}') => {
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('{')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', '{' | '}') => {
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('{')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', '<' | '>') => {
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('<')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', '<' | '>') => {
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('<')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', '"') => {
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('"')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', '"') => {
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('"')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', '\'') => {
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('\'')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', '\'') => {
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('\'')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', '`') => {
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('`')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', '`') => {
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('`')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', 'w') => {
                                        let (start, end) = self.get_inner_word_bounds();
                                        self.visual_start = (self.cursor.0, start);
                                        self.cursor.1 = end.saturating_sub(1);
                                        true
                                    }
                                    ('a', 'w') => {
                                        let (start, end) = self.get_a_word_bounds();
                                        self.visual_start = (self.cursor.0, start);
                                        self.cursor.1 = end.saturating_sub(1);
                                        true
                                    }
                                    ('i', 'W') => {
                                        let (start, end) = self.get_inner_long_word_bounds();
                                        self.visual_start = (self.cursor.0, start);
                                        self.cursor.1 = end.saturating_sub(1);
                                        true
                                    }
                                    ('a', 'W') => {
                                        let (start, end) = self.get_a_long_word_bounds();
                                        self.visual_start = (self.cursor.0, start);
                                        self.cursor.1 = end.saturating_sub(1);
                                        true
                                    }
                                    ('i', 'b') => {
                                        // ib is same as i(
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('(')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', 'b') => {
                                        // ab is same as a(
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('(')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('i', 'B') => {
                                        // iB is same as i{
                                        if let Some((open_pos, close_pos)) =
                                            self.find_pair_bounds('{')
                                        {
                                            let (start, end) = self
                                                .get_inner_pair_visual_bounds(open_pos, close_pos);
                                            self.visual_start = start;
                                            self.cursor = end;
                                        }
                                        true
                                    }
                                    ('a', 'B') => {
                                        // aB is same as a{
                                        if let Some((
                                            (open_row, open_col),
                                            (close_row, close_col),
                                        )) = self.find_pair_bounds('{')
                                        {
                                            self.visual_start = (open_row, open_col);
                                            self.cursor = (close_row, close_col);
                                        }
                                        true
                                    }
                                    ('g', 'g') => {
                                        // gg - go to first line
                                        self.cursor.0 = 0;
                                        let line_len = self.lines[self.cursor.0].chars().count();
                                        let max_col = line_len.saturating_sub(1);
                                        self.cursor.1 = self.desired_col.min(max_col);
                                        true
                                    }
                                    ('i', 'p') => {
                                        // ip - inner paragraph
                                        if let Some((start_row, end_row)) =
                                            self.get_paragraph_bounds(TextObjectKind::Inner)
                                        {
                                            self.visual_start = (start_row, 0);
                                            let end_col = self.lines[end_row]
                                                .chars()
                                                .count()
                                                .saturating_sub(1);
                                            self.cursor = (end_row, end_col);
                                            // Switch to linewise visual mode for paragraph selection
                                            self.mode = EditorMode::VisualLine;
                                        }
                                        true
                                    }
                                    ('a', 'p') => {
                                        // ap - a paragraph (includes trailing/leading blank lines)
                                        if let Some((start_row, end_row)) =
                                            self.get_paragraph_bounds(TextObjectKind::Around)
                                        {
                                            self.visual_start = (start_row, 0);
                                            let end_col = self.lines[end_row]
                                                .chars()
                                                .count()
                                                .saturating_sub(1);
                                            self.cursor = (end_row, end_col);
                                            // Switch to linewise visual mode for paragraph selection
                                            self.mode = EditorMode::VisualLine;
                                        }
                                        true
                                    }
                                    ('i', 's') => {
                                        // is - inner sentence
                                        let (start_row, start_col, end_row, end_col) =
                                            self.get_sentence_bounds(TextObjectKind::Inner);
                                        self.visual_start = (start_row, start_col);
                                        self.cursor = (end_row, end_col);
                                        // Keep in character visual mode for sentence selection
                                        self.mode = EditorMode::Visual;
                                        true
                                    }
                                    ('a', 's') => {
                                        // as - a sentence (includes trailing whitespace)
                                        let (start_row, start_col, end_row, end_col) =
                                            self.get_sentence_bounds(TextObjectKind::Around);
                                        self.visual_start = (start_row, start_col);
                                        self.cursor = (end_row, end_col);
                                        // Keep in character visual mode for sentence selection
                                        self.mode = EditorMode::Visual;
                                        true
                                    }
                                    _ => false,
                                };
                                self.pending_keys.clear();
                                if handled {
                                    // Text object was handled, skip normal key processing
                                } else {
                                    // Unknown text object, ignore
                                }
                            } else {
                                // No pending key - handle as normal keys
                                match c {
                                    // Movement keys extend selection
                                    'h' => self.move_cursor(0, -1),
                                    'j' => self.move_cursor(1, 0),
                                    'k' => self.move_cursor(-1, 0),
                                    'l' => self.move_cursor(0, 1),
                                    'w' => self.move_word_forward(WordType::Word),
                                    'W' => self.move_word_forward(WordType::LongWord),
                                    'b' => self.move_word_backward(WordType::Word),
                                    'B' => self.move_word_backward(WordType::LongWord),
                                    'e' => self.move_to_word_end(WordType::Word),
                                    'E' => self.move_to_word_end(WordType::LongWord),
                                    '0' => {
                                        self.cursor.1 = 0;
                                        self.update_desired_col();
                                    }
                                    '^' => self.move_to_first_non_blank(),
                                    '$' => {
                                        self.cursor.1 = self.lines[self.cursor.0]
                                            .chars()
                                            .count()
                                            .saturating_sub(1);
                                        self.update_desired_col();
                                    }
                                    'G' => {
                                        self.cursor.0 = self.lines.len() - 1;
                                        // Use desired_col like vertical movement
                                        let line_len = self.lines[self.cursor.0].chars().count();
                                        let max_col = if self.mode == EditorMode::Insert {
                                            line_len
                                        } else {
                                            line_len.saturating_sub(1)
                                        };
                                        self.cursor.1 = self.desired_col.min(max_col);
                                    }
                                    '%' => {
                                        self.jump_to_matching_bracket();
                                        self.update_desired_col();
                                    }
                                    '{' => {
                                        self.move_paragraph_backward();
                                    }
                                    '}' => {
                                        self.move_paragraph_forward();
                                    }
                                    '(' => {
                                        self.move_sentence_backward();
                                    }
                                    ')' => {
                                        self.move_sentence_forward();
                                    }
                                    // Operations on selection
                                    'd' | 'x' | 'X' => {
                                        // Record block dimensions for repeat if in block mode
                                        if self.mode == EditorMode::VisualBlock {
                                            let (start, end) = self.get_visual_selection();
                                            let num_rows = end.0 - start.0 + 1;
                                            let min_col = self.visual_start.1.min(self.cursor.1);
                                            let max_col = self.visual_start.1.max(self.cursor.1);
                                            let col_width = max_col - min_col + 1;
                                            self.delete_visual_selection();
                                            self.set_last_change(
                                                LastChange::DeleteBlock(num_rows, col_width),
                                                1,
                                            );
                                        } else {
                                            self.delete_visual_selection();
                                        }
                                        self.mode = EditorMode::Normal;
                                    }
                                    'y' => {
                                        self.yank_visual_selection();
                                        self.mode = EditorMode::Normal;
                                    }
                                    'c' | 's' => {
                                        // Save cursor position, delete without recording
                                        // (record_change called when exiting insert mode)
                                        // 's' in visual mode is the same as 'c' (substitute)
                                        if self.mode == EditorMode::VisualBlock {
                                            // Track block info for inserting on all lines
                                            let (start, end) = self.get_visual_selection();
                                            let num_rows = end.0 - start.0 + 1;
                                            let min_col = self.visual_start.1.min(self.cursor.1);
                                            let max_col = self.visual_start.1.max(self.cursor.1);
                                            let col_width = max_col - min_col + 1;
                                            // Save undo state with top-left of selection
                                            self.save_undo_state_with_cursor((start.0, min_col));
                                            self.delete_visual_selection_no_undo();
                                            // Store block info for insert with col_width for repeat
                                            self.block_insert_info = Some((
                                                start.0,
                                                num_rows,
                                                min_col,
                                                BlockInsertType::Change(col_width),
                                            ));
                                            self.mode = EditorMode::Insert;
                                            self.insert_buffer.clear();
                                            // Position cursor at insert column
                                            self.cursor = (start.0, min_col);
                                        } else {
                                            // Save undo state with start of selection
                                            let (start, _) = self.get_visual_selection();
                                            self.save_undo_state_with_cursor(start);
                                            self.delete_visual_selection_no_undo();
                                            self.mode = EditorMode::Insert;
                                            self.insert_buffer.clear();
                                        }
                                    }
                                    'I' => {
                                        // Insert at left edge of block on all lines
                                        if self.mode == EditorMode::VisualBlock {
                                            let (start, end) = self.get_visual_selection();
                                            let num_rows = end.0 - start.0 + 1;
                                            let insert_col = self.visual_start.1.min(self.cursor.1);
                                            // Save undo state with top-left of selection
                                            self.save_undo_state_with_cursor((start.0, insert_col));
                                            // Store block info for insert (no deletion)
                                            self.block_insert_info = Some((
                                                start.0,
                                                num_rows,
                                                insert_col,
                                                BlockInsertType::Insert,
                                            ));
                                            self.mode = EditorMode::Insert;
                                            self.insert_buffer.clear();
                                            // Position cursor at insert column
                                            self.cursor = (start.0, insert_col);
                                        }
                                    }
                                    'A' => {
                                        // Append at right edge of block on all lines
                                        if self.mode == EditorMode::VisualBlock {
                                            let (start, end) = self.get_visual_selection();
                                            let num_rows = end.0 - start.0 + 1;
                                            let min_col = self.visual_start.1.min(self.cursor.1);
                                            let max_col = self.visual_start.1.max(self.cursor.1);
                                            let insert_col = max_col + 1;
                                            // Calculate offset from left edge of selection
                                            let col_offset = insert_col - min_col;
                                            // Save undo state with top-left of selection
                                            self.save_undo_state_with_cursor((start.0, min_col));
                                            // Store block info for insert (no deletion)
                                            self.block_insert_info = Some((
                                                start.0,
                                                num_rows,
                                                insert_col,
                                                BlockInsertType::Append(col_offset),
                                            ));
                                            self.mode = EditorMode::Insert;
                                            self.insert_buffer.clear();
                                            // Position cursor at insert column
                                            self.cursor = (start.0, insert_col);
                                        }
                                    }
                                    'D' => {
                                        // In Visual/VisualLine mode, D deletes entire lines (linewise)
                                        // In VisualBlock mode, delete from left edge to end of line
                                        if self.mode == EditorMode::VisualBlock {
                                            self.delete_block_to_eol();
                                        } else {
                                            self.delete_visual_lines();
                                        }
                                    }
                                    'C' => {
                                        // In Visual/VisualLine mode, C deletes entire lines and enters insert
                                        // In VisualBlock mode, delete from left edge to end of line, then insert
                                        if self.mode == EditorMode::VisualBlock {
                                            self.change_block_to_eol();
                                        } else {
                                            self.change_visual_lines();
                                        }
                                    }
                                    'S' => {
                                        // S in visual mode: delete entire lines and enter insert mode
                                        // This is linewise regardless of current visual mode
                                        self.change_visual_lines();
                                    }
                                    // Toggle case
                                    '~' => {
                                        let (start, end) = self.get_visual_selection();
                                        // Save undo state with start of selection
                                        self.save_undo_state_with_cursor(start);
                                        self.lines_version += 1;
                                        if self.mode == EditorMode::VisualLine {
                                            for row in start.0..=end.0 {
                                                let toggled: String = self.lines[row]
                                                    .chars()
                                                    .map(|c| {
                                                        if c.is_uppercase() {
                                                            c.to_lowercase().next().unwrap_or(c)
                                                        } else {
                                                            c.to_uppercase().next().unwrap_or(c)
                                                        }
                                                    })
                                                    .collect();
                                                self.lines[row] = toggled;
                                            }
                                        } else {
                                            // Character-wise toggle
                                            for row in start.0..=end.0 {
                                                let chars: Vec<char> =
                                                    self.lines[row].chars().collect();
                                                let col_start =
                                                    if row == start.0 { start.1 } else { 0 };
                                                let col_end = if row == end.0 {
                                                    (end.1 + 1).min(chars.len())
                                                } else {
                                                    chars.len()
                                                };
                                                let toggled: String = chars
                                                    .iter()
                                                    .enumerate()
                                                    .map(|(i, &c)| {
                                                        if i >= col_start && i < col_end {
                                                            if c.is_uppercase() {
                                                                c.to_lowercase().next().unwrap_or(c)
                                                            } else {
                                                                c.to_uppercase().next().unwrap_or(c)
                                                            }
                                                        } else {
                                                            c
                                                        }
                                                    })
                                                    .collect();
                                                self.lines[row] = toggled;
                                            }
                                        }
                                        self.cursor = start;
                                        self.record_change();
                                        self.mode = EditorMode::Normal;
                                    }
                                    // Switch visual modes
                                    'v' => {
                                        if self.mode == EditorMode::Visual {
                                            self.mode = EditorMode::Normal;
                                        } else {
                                            self.mode = EditorMode::Visual;
                                        }
                                    }
                                    'V' => {
                                        if self.mode == EditorMode::VisualLine {
                                            self.mode = EditorMode::Normal;
                                        } else {
                                            self.mode = EditorMode::VisualLine;
                                        }
                                    }
                                    // Swap anchor and cursor
                                    'o' => {
                                        std::mem::swap(&mut self.cursor, &mut self.visual_start);
                                        self.update_desired_col();
                                    }
                                    // Swap to opposite horizontal end (visual block mode)
                                    'O' => {
                                        if self.mode == EditorMode::VisualBlock {
                                            // In block mode, swap only the column positions
                                            std::mem::swap(
                                                &mut self.cursor.1,
                                                &mut self.visual_start.1,
                                            );
                                            self.update_desired_col();
                                        } else {
                                            // In other visual modes, O behaves like o
                                            std::mem::swap(
                                                &mut self.cursor,
                                                &mut self.visual_start,
                                            );
                                            self.update_desired_col();
                                        }
                                    }
                                    // Text object selection - 'i' for inner, 'a' for around
                                    'i' | 'a' => {
                                        self.pending_keys.push(KeyCode::Char(c));
                                    }
                                    // 'g' prefix for gg command
                                    'g' => {
                                        self.pending_keys.push(KeyCode::Char('g'));
                                    }
                                    _ => {
                                        // Clear pending keys on unrecognized input
                                        self.pending_keys.clear();
                                    }
                                } // close match c
                            } // close else
                        } // close => for Char(c)
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::UpArrow,
                            ..
                        }) => self.move_cursor(-1, 0),
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::DownArrow,
                            ..
                        }) => self.move_cursor(1, 0),
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::LeftArrow,
                            ..
                        }) => self.move_cursor(0, -1),
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::RightArrow,
                            ..
                        }) => self.move_cursor(0, 1),
                        InputEvent::Paste(text) => {
                            // In Visual mode, paste replaces the selected text
                            // Save cursor position, then delete+insert as one undo unit
                            self.save_undo_state();
                            self.delete_visual_selection_no_undo();
                            self.insert_text(&text);
                            // Move cursor back to last inserted char
                            if self.cursor.1 > 0 {
                                self.cursor.1 -= 1;
                            }
                            self.clamp_cursor();
                            self.mode = EditorMode::Normal;
                            self.record_change();
                        }
                        _ => {}
                    }
                }
            }
            self.render()?;
        }
        Ok(())
    }

    fn submit(&self) {
        let name = match *self.args.action {
            KeyAssignment::EmitEvent(ref id) => id,
            _ => {
                log::error!("InputText requires action to be defined by wezterm.action_callback");
                return;
            }
        };

        let result = InputTextResult {
            text: self.lines.join("\n"),
        };

        self.trigger_event(name, Some(result));
    }

    fn cancel(&self) {
        let name = match *self.args.action {
            KeyAssignment::EmitEvent(ref id) => id,
            _ => {
                log::error!("InputText requires action to be defined by wezterm.action_callback");
                return;
            }
        };

        self.trigger_event(name, None);
    }

    fn trigger_event(&self, name: &str, result: Option<InputTextResult>) {
        let name = name.to_string();
        let window = self.window.clone();
        let pane = self.pane;

        promise::spawn::spawn_into_main_thread(async move {
            trampoline(name, window, pane, result);
            anyhow::Result::<()>::Ok(())
        })
        .detach();
    }
}

#[derive(FromDynamic, ToDynamic)]
struct InputTextResult {
    text: String,
}
impl_lua_conversion_dynamic!(InputTextResult);

fn trampoline(name: String, window: GuiWin, pane: MuxPane, result: Option<InputTextResult>) {
    promise::spawn::spawn(async move {
        config::with_lua_config_on_main_thread(move |lua| do_event(lua, name, window, pane, result))
            .await
    })
    .detach();
}

async fn do_event(
    lua: Option<Rc<mlua::Lua>>,
    name: String,
    window: GuiWin,
    pane: MuxPane,
    result: Option<InputTextResult>,
) -> anyhow::Result<()> {
    if let Some(lua) = lua {
        let args = if let Some(result) = result {
            lua.pack_multi((window, pane, result))?
        } else {
            lua.pack_multi((window, pane))?
        };

        if let Err(err) = config::lua::emit_event(&lua, (name.clone(), args)).await {
            log::error!("while processing {} event: {:#}", name, err);
        }
    }

    Ok(())
}

pub fn show_input_text_overlay(
    term: TermWizTerminal,
    args: InputText,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let mut state = EditorState::new(&args, window, pane, &mut buf);

    state.run_loop()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simplified editor state for testing core logic
    struct TestEditor {
        lines: Vec<String>,
        cursor: (usize, usize),
        mode: EditorMode,
        yank_buffer: String,
        yank_is_linewise: bool,
        yank_is_block: bool,
        history: Vec<(Vec<String>, (usize, usize))>,
        history_idx: usize,
        lines_version: u64,
        history_version: u64,
        count_prefix: Option<usize>,
        last_change: LastChange,
        last_count: usize,
        viewport_top: usize,
        screen_height: usize,         // Number of visible lines for H/M/L tests
        visual_start: (usize, usize), // Anchor point for visual selection
    }

    impl TestEditor {
        fn new(text: &str) -> Self {
            let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
            let lines = if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            };
            Self {
                lines: lines.clone(),
                cursor: (0, 0),
                mode: EditorMode::Normal,
                yank_buffer: String::new(),
                yank_is_linewise: false,
                yank_is_block: false,
                history: vec![(lines, (0, 0))],
                history_idx: 0,
                lines_version: 0,
                history_version: 0,
                count_prefix: None,
                last_change: LastChange::None,
                last_count: 1,
                viewport_top: 0,
                screen_height: 24,    // Default screen height for tests
                visual_start: (0, 0), // Default visual start
            }
        }

        fn with_screen_height(mut self, height: usize) -> Self {
            self.screen_height = height;
            self
        }

        fn with_cursor(mut self, row: usize, col: usize) -> Self {
            self.cursor = (row, col);
            self
        }

        fn text(&self) -> String {
            self.lines.join("\n")
        }

        fn take_count(&mut self) -> usize {
            self.count_prefix.take().unwrap_or(1)
        }

        fn add_count_digit(&mut self, digit: char) {
            let d = digit.to_digit(10).unwrap_or(0) as usize;
            self.count_prefix = Some(self.count_prefix.unwrap_or(0) * 10 + d);
        }

        fn set_last_change(&mut self, change: LastChange, count: usize) {
            self.last_change = change;
            self.last_count = count;
        }

        fn is_insert_like_mode(&self) -> bool {
            self.mode == EditorMode::Insert || self.mode == EditorMode::Replace
        }

        /// Delete `count` characters at cursor (like 3x)
        fn delete_chars(&mut self, count: usize) {
            self.save_undo_state();
            self.lines_version += 1;
            for _ in 0..count {
                let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
                if self.cursor.1 < chars.len() {
                    chars.remove(self.cursor.1);
                    self.lines[self.cursor.0] = chars.into_iter().collect();
                }
            }
            self.clamp_cursor();
            self.record_change();
        }

        /// Delete `count` lines at cursor (like 3dd)
        fn delete_lines(&mut self, count: usize) {
            self.save_undo_state();
            self.lines_version += 1;
            let mut yanked = Vec::new();
            for _ in 0..count {
                if self.lines.len() > 1 && self.cursor.0 < self.lines.len() {
                    yanked.push(self.lines.remove(self.cursor.0));
                } else if self.lines.len() == 1 {
                    yanked.push(self.lines[0].clone());
                    self.lines[0].clear();
                    break;
                }
            }
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
            self.clamp_cursor();
            self.record_change();
        }

        /// Paste after cursor `count` times (like 3p)
        fn paste_after_count(&mut self, count: usize) {
            if self.yank_buffer.is_empty() || count == 0 {
                return;
            }
            self.save_undo_state();
            self.lines_version += 1;

            if self.yank_is_linewise {
                let base_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
                for _ in 0..count {
                    for (i, line) in base_lines.iter().enumerate() {
                        self.lines.insert(self.cursor.0 + 1 + i, line.to_string());
                    }
                }
                self.cursor.0 += 1;
                self.cursor.1 = 0;
            } else {
                let repeated: String = self.yank_buffer.repeat(count);
                let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
                let insert_pos = (self.cursor.1 + 1).min(chars.len());
                for (i, c) in repeated.chars().enumerate() {
                    chars.insert(insert_pos + i, c);
                }
                self.lines[self.cursor.0] = chars.into_iter().collect();
                self.cursor.1 = insert_pos + repeated.len().saturating_sub(1);
            }
            self.clamp_cursor();
            self.record_change();
        }

        /// Replace character at cursor (like R mode)
        fn replace_char_at_cursor(&mut self, c: char) -> Option<char> {
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let original = if self.cursor.1 < chars.len() {
                let orig = chars[self.cursor.1];
                chars[self.cursor.1] = c;
                Some(orig)
            } else {
                chars.push(c);
                None
            };
            self.lines[self.cursor.0] = chars.into_iter().collect();
            self.cursor.1 += 1;
            self.lines_version += 1;
            original
        }

        /// Restore character at cursor (for Replace mode backspace)
        fn restore_char_at_cursor(&mut self, orig_char: char) {
            if self.cursor.1 == 0 {
                return;
            }
            self.cursor.1 -= 1;
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            if self.cursor.1 < chars.len() {
                chars[self.cursor.1] = orig_char;
                self.lines[self.cursor.0] = chars.into_iter().collect();
                self.lines_version += 1;
            }
        }

        /// Yank `count` lines at cursor (like 3yy)
        fn yank_lines(&mut self, count: usize) {
            let end = (self.cursor.0 + count).min(self.lines.len());
            let yanked: Vec<&str> = self.lines[self.cursor.0..end]
                .iter()
                .map(|s| s.as_str())
                .collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
        }

        /// Substitute `count` lines at cursor (like 3cc)
        fn substitute_lines(&mut self, count: usize) {
            self.save_undo_state();
            self.lines_version += 1;

            let mut yanked = Vec::new();
            let start_row = self.cursor.0;

            // Yank the lines first
            for i in 0..count {
                if start_row + i < self.lines.len() {
                    yanked.push(self.lines[start_row + i].clone());
                }
            }
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;

            // Delete lines except first, then clear first
            let lines_to_delete = (count - 1).min(self.lines.len().saturating_sub(start_row + 1));
            for _ in 0..lines_to_delete {
                if start_row + 1 < self.lines.len() {
                    self.lines.remove(start_row + 1);
                }
            }

            // Clear the current line
            if start_row < self.lines.len() {
                self.lines[start_row].clear();
            }

            self.cursor.1 = 0;
            self.mode = EditorMode::Insert;
            self.record_change();
        }

        fn clamp_cursor(&mut self) {
            // Clamp row first
            if self.cursor.0 >= self.lines.len() {
                self.cursor.0 = self.lines.len().saturating_sub(1);
            }
            // Then clamp column
            let line_len = self.lines[self.cursor.0].chars().count();
            let max_col = if self.mode == EditorMode::Insert {
                line_len
            } else {
                line_len.saturating_sub(1)
            };
            if self.cursor.1 > max_col {
                self.cursor.1 = max_col;
            }
        }

        fn record_change(&mut self) {
            if self.lines_version != self.history_version {
                // Trim future history if we made new changes after undo
                if self.history_idx < self.history.len() - 1 {
                    self.history.truncate(self.history_idx + 1);
                }
                self.history.push((self.lines.clone(), self.cursor));
                self.history_idx = self.history.len() - 1;
                self.history_version = self.lines_version;
            }
        }

        fn save_undo_state(&mut self) {
            // Truncate redo history
            if self.history_idx < self.history.len() - 1 {
                self.history.truncate(self.history_idx + 1);
            }
            // If lines haven't changed since last history push, just update cursor
            if self.lines_version == self.history_version {
                if let Some(entry) = self.history.last_mut() {
                    entry.1 = self.cursor;
                }
                return;
            }
            // Lines are different, push new entry
            self.history.push((self.lines.clone(), self.cursor));
            self.history_idx = self.history.len() - 1;
            self.history_version = self.lines_version;
        }

        fn save_undo_state_with_cursor(&mut self, cursor: (usize, usize)) {
            // Truncate redo history
            if self.history_idx < self.history.len() - 1 {
                self.history.truncate(self.history_idx + 1);
            }
            // If lines haven't changed since last history push, just update cursor
            if self.lines_version == self.history_version {
                if let Some(entry) = self.history.last_mut() {
                    entry.1 = cursor;
                }
                return;
            }
            // Lines are different, push new entry with specified cursor
            self.history.push((self.lines.clone(), cursor));
            self.history_idx = self.history.len() - 1;
            self.history_version = self.lines_version;
        }

        fn undo(&mut self) {
            if self.history_idx > 0 {
                self.history_idx -= 1;
                let (lines, cursor) = &self.history[self.history_idx];
                self.lines = lines.clone();
                self.cursor = *cursor;
            }
        }

        fn redo(&mut self) {
            if self.history_idx < self.history.len() - 1 {
                let cursor_at_change_start = self.history[self.history_idx].1;
                self.history_idx += 1;
                let (lines, _) = &self.history[self.history_idx];
                self.lines = lines.clone();
                self.cursor = cursor_at_change_start;
                self.clamp_cursor();
            }
        }

        fn insert_char(&mut self, c: char) {
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            if self.cursor.1 >= chars.len() {
                chars.push(c);
            } else {
                chars.insert(self.cursor.1, c);
            }
            self.lines[self.cursor.0] = chars.into_iter().collect();
            self.cursor.1 += 1;
            self.lines_version += 1;
        }

        fn delete_char(&mut self) {
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            if self.cursor.1 < chars.len() {
                chars.remove(self.cursor.1);
                self.lines[self.cursor.0] = chars.into_iter().collect();
                self.lines_version += 1;
                self.clamp_cursor();
            }
        }

        /// Delete character before cursor (X command)
        fn delete_char_backward(&mut self) {
            if self.cursor.1 > 0 {
                self.cursor.1 -= 1;
                self.delete_char();
            }
        }

        /// Get visible lines based on viewport_top and screen_height
        fn get_visible_lines(&self) -> Vec<usize> {
            let mut visible = Vec::new();
            for i in 0..self.screen_height {
                let line_idx = self.viewport_top + i;
                if line_idx < self.lines.len() {
                    visible.push(line_idx);
                }
            }
            visible
        }

        /// Move cursor to screen position (H/M/L commands)
        fn move_to_screen_position(&mut self, position: ScreenPosition) {
            let visible = self.get_visible_lines();
            if visible.is_empty() {
                return;
            }

            let idx = match position {
                ScreenPosition::Top(count) => (count.saturating_sub(1)).min(visible.len() - 1),
                ScreenPosition::Middle => visible.len() / 2,
                ScreenPosition::Bottom(count) => visible.len().saturating_sub(count),
            };

            self.cursor.0 = visible[idx];
            self.clamp_cursor();
        }

        /// Scroll viewport down (Ctrl-E)
        fn scroll_down(&mut self, count: usize) {
            let max_viewport = self.lines.len().saturating_sub(1);
            self.viewport_top = (self.viewport_top + count).min(max_viewport);

            // If cursor is now above viewport, move it down
            if self.cursor.0 < self.viewport_top {
                self.cursor.0 = self.viewport_top;
                self.clamp_cursor();
            }
        }

        /// Scroll viewport up (Ctrl-Y)
        fn scroll_up(&mut self, count: usize) {
            self.viewport_top = self.viewport_top.saturating_sub(count);

            // If cursor is now below viewport, move it up
            let visible = self.get_visible_lines();
            if let Some(&last_visible) = visible.last() {
                if self.cursor.0 > last_visible {
                    self.cursor.0 = last_visible;
                    self.clamp_cursor();
                }
            }
        }

        /// Scroll half page down (Ctrl-D)
        /// Both viewport and cursor move down by half a page
        fn scroll_half_page_down(&mut self, count: usize) {
            let half_page = (self.screen_height / 2).max(1) * count;

            let max_viewport = self.lines.len().saturating_sub(1);
            self.viewport_top = (self.viewport_top + half_page).min(max_viewport);
            self.cursor.0 = (self.cursor.0 + half_page).min(self.lines.len().saturating_sub(1));
            self.clamp_cursor();
        }

        /// Scroll half page up (Ctrl-U)
        /// Both viewport and cursor move up by half a page
        fn scroll_half_page_up(&mut self, count: usize) {
            let half_page = (self.screen_height / 2).max(1) * count;

            self.viewport_top = self.viewport_top.saturating_sub(half_page);
            self.cursor.0 = self.cursor.0.saturating_sub(half_page);
            self.clamp_cursor();
        }

        /// Scroll viewport so cursor line is at center of screen (zz)
        fn scroll_cursor_to_center(&mut self) {
            let half = self.screen_height / 2;

            // Set viewport_top so cursor is in the middle
            self.viewport_top = self.cursor.0.saturating_sub(half);

            // Clamp viewport to valid range
            let max_viewport = self.lines.len().saturating_sub(1);
            self.viewport_top = self.viewport_top.min(max_viewport);
        }

        /// Scroll viewport so cursor line is at top of screen (zt)
        fn scroll_cursor_to_top(&mut self) {
            self.viewport_top = self.cursor.0;

            // Clamp viewport to valid range
            let max_viewport = self.lines.len().saturating_sub(1);
            self.viewport_top = self.viewport_top.min(max_viewport);
        }

        /// Scroll viewport so cursor line is at bottom of screen (zb)
        fn scroll_cursor_to_bottom(&mut self) {
            // Set viewport_top so cursor is at the bottom visible line
            self.viewport_top = self
                .cursor
                .0
                .saturating_sub(self.screen_height.saturating_sub(1));

            // Clamp viewport to valid range (at minimum 0)
            let max_viewport = self.lines.len().saturating_sub(1);
            self.viewport_top = self.viewport_top.min(max_viewport);
        }

        fn delete_line(&mut self, row: usize) {
            if self.lines.len() > 1 && row < self.lines.len() {
                self.lines.remove(row);
                self.lines_version += 1;
                self.clamp_cursor();
            } else if self.lines.len() == 1 {
                self.lines[0].clear();
                self.cursor = (0, 0);
                self.lines_version += 1;
            }
        }

        fn find_pair_bounds(&self, pair_char: char) -> Option<((usize, usize), (usize, usize))> {
            let (open, close) = match pair_char {
                '(' | ')' => ('(', ')'),
                '[' | ']' => ('[', ']'),
                '{' | '}' => ('{', '}'),
                '<' | '>' => ('<', '>'),
                '"' => ('"', '"'),
                '\'' => ('\'', '\''),
                '`' => ('`', '`'),
                _ => return None,
            };

            let line = &self.lines[self.cursor.0];
            let chars: Vec<char> = line.chars().collect();
            let current_char = chars.get(self.cursor.1).copied();

            // Check if cursor is on an opening or closing bracket
            if current_char == Some(open) {
                // Cursor on opening bracket - search forward for closing
                if let Some((close_row, close_col)) =
                    self.find_matching_close(open, close, self.cursor.0, self.cursor.1)
                {
                    return Some(((self.cursor.0, self.cursor.1), (close_row, close_col)));
                }
            } else if current_char == Some(close) && open != close {
                // Cursor on closing bracket - search backward for opening
                if let Some((open_row, open_col)) =
                    self.find_matching_open(open, close, self.cursor.0, self.cursor.1)
                {
                    return Some(((open_row, open_col), (self.cursor.0, self.cursor.1)));
                }
            }

            // Not on a bracket, search outward
            if let Some((open_row, open_col)) =
                self.find_matching_open(open, close, self.cursor.0, self.cursor.1)
            {
                if let Some((close_row, close_col)) =
                    self.find_matching_close(open, close, open_row, open_col)
                {
                    return Some(((open_row, open_col), (close_row, close_col)));
                }
            }

            None
        }

        fn find_matching_open(
            &self,
            open: char,
            close: char,
            start_row: usize,
            start_col: usize,
        ) -> Option<(usize, usize)> {
            let mut depth = 0;
            let mut row = start_row;
            let mut col = start_col;

            loop {
                let line_chars: Vec<char> = self.lines[row].chars().collect();
                let search_end = if row == start_row {
                    col
                } else {
                    line_chars.len()
                };

                for i in (0..search_end).rev() {
                    let c = line_chars[i];
                    if c == close && open != close {
                        depth += 1;
                    } else if c == open {
                        if depth == 0 {
                            return Some((row, i));
                        }
                        depth -= 1;
                    }
                }

                if row == 0 {
                    break;
                }
                row -= 1;
                col = self.lines[row].chars().count();
            }
            None
        }

        fn find_matching_close(
            &self,
            open: char,
            close: char,
            start_row: usize,
            start_col: usize,
        ) -> Option<(usize, usize)> {
            let mut depth = 0;
            let mut row = start_row;
            let mut col = start_col;

            loop {
                let line_chars: Vec<char> = self.lines[row].chars().collect();
                let start_idx = if row == start_row { col + 1 } else { 0 };

                for i in start_idx..line_chars.len() {
                    let c = line_chars[i];
                    if c == open && open != close {
                        depth += 1;
                    } else if c == close {
                        if depth == 0 {
                            return Some((row, i));
                        }
                        depth -= 1;
                    }
                }

                row += 1;
                if row >= self.lines.len() {
                    break;
                }
                col = 0;
            }
            None
        }

        fn yank_inner_pair(&mut self, pair_char: char) {
            if let Some(((open_row, open_col), (close_row, close_col))) =
                self.find_pair_bounds(pair_char)
            {
                if open_row == close_row {
                    let chars: Vec<char> = self.lines[open_row].chars().collect();
                    if open_col + 1 < close_col {
                        self.yank_buffer = chars[open_col + 1..close_col].iter().collect();
                    } else {
                        self.yank_buffer.clear();
                    }
                    self.yank_is_linewise = false;
                } else {
                    let mut yanked = String::new();
                    let first_chars: Vec<char> = self.lines[open_row].chars().collect();
                    let after_open: String = first_chars[open_col + 1..].iter().collect();
                    let has_first_line_content = !after_open.trim().is_empty();
                    if has_first_line_content {
                        yanked.push_str(&after_open);
                    }

                    for row in (open_row + 1)..close_row {
                        if !yanked.is_empty() || !has_first_line_content {
                            yanked.push('\n');
                        }
                        yanked.push_str(&self.lines[row]);
                    }

                    if close_row > open_row {
                        let last_chars: Vec<char> = self.lines[close_row].chars().collect();
                        let before_close: String = last_chars[..close_col].iter().collect();
                        let has_last_line_content = !before_close.trim().is_empty();
                        if has_last_line_content {
                            if !yanked.is_empty() {
                                yanked.push('\n');
                            }
                            yanked.push_str(&before_close);
                        }
                    }

                    self.yank_buffer = yanked;
                    self.yank_is_linewise = false;
                }
            }
        }

        fn delete_inner_pair(&mut self, pair_char: char) {
            if let Some(((open_row, open_col), (close_row, close_col))) =
                self.find_pair_bounds(pair_char)
            {
                self.save_undo_state();
                self.lines_version += 1;

                if open_row == close_row {
                    let mut chars: Vec<char> = self.lines[open_row].chars().collect();
                    if open_col + 1 < close_col {
                        chars.drain((open_col + 1)..close_col);
                        self.lines[open_row] = chars.into_iter().collect();
                    }
                    self.cursor = (open_row, open_col + 1);
                } else {
                    // Multi-line deletion
                    let first_chars: Vec<char> = self.lines[open_row].chars().collect();
                    self.lines[open_row] = first_chars[..=open_col].iter().collect();

                    let last_chars: Vec<char> = self.lines[close_row].chars().collect();
                    self.lines[close_row] = last_chars[close_col..].iter().collect();

                    // Remove middle lines
                    if close_row > open_row + 1 {
                        self.lines.drain((open_row + 1)..close_row);
                    }

                    // After drain, the close line is now at open_row + 1
                    let new_close_row = open_row + 1;

                    // For change operations, insert an empty line between brackets
                    if self.mode == EditorMode::Insert && close_row > open_row + 1 {
                        self.lines.insert(new_close_row, String::new());
                        self.cursor = (new_close_row, 0);
                    } else {
                        self.cursor = (new_close_row, 0);
                    }
                }

                self.clamp_cursor();
                // For change operations (mode set to Insert), don't record yet
                if self.mode != EditorMode::Insert {
                    self.record_change();
                }
            }
        }

        /// Record change only if not in Insert/Replace mode
        fn maybe_record_change(&mut self) {
            if !self.is_insert_like_mode() {
                self.record_change();
            }
        }

        /// Helper to delete a text object on the current line given (start, end) bounds
        fn delete_text_object_on_line(&mut self, start: usize, end: usize) {
            let line_len = self.lines[self.cursor.0].len();
            if start < end && end <= line_len {
                self.save_undo_state();
                self.lines_version += 1;
                self.yank_buffer = self.lines[self.cursor.0][start..end].to_string();
                self.yank_is_linewise = false;
                self.lines[self.cursor.0].replace_range(start..end, "");
                self.cursor.1 = start;
                self.clamp_cursor();
                self.maybe_record_change();
            }
        }

        /// Helper to yank a text object on the current line given (start, end) bounds
        fn yank_text_object_on_line(&mut self, start: usize, end: usize) {
            let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            if start < end && end <= chars.len() {
                self.yank_buffer = chars[start..end].iter().collect();
                self.yank_is_linewise = false;
            }
        }

        fn get_inner_word_bounds(&self) -> (usize, usize) {
            let line = &self.lines[self.cursor.0];
            if line.is_empty() {
                return (0, 0);
            }
            let chars: Vec<char> = line.chars().collect();
            let col = self.cursor.1.min(chars.len().saturating_sub(1));

            // Find word boundaries
            let mut start = col;
            let mut end = col;

            let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
            let current_is_word = is_word_char(chars[col]);

            if current_is_word {
                while start > 0 && is_word_char(chars[start - 1]) {
                    start -= 1;
                }
                while end < chars.len() && is_word_char(chars[end]) {
                    end += 1;
                }
            } else if chars[col].is_whitespace() {
                while start > 0 && chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                while end < chars.len() && chars[end].is_whitespace() {
                    end += 1;
                }
            } else {
                while start > 0
                    && !is_word_char(chars[start - 1])
                    && !chars[start - 1].is_whitespace()
                {
                    start -= 1;
                }
                while end < chars.len() && !is_word_char(chars[end]) && !chars[end].is_whitespace()
                {
                    end += 1;
                }
            }
            (start, end)
        }

        fn get_a_word_bounds(&self) -> (usize, usize) {
            let (word_start, word_end) = self.get_inner_word_bounds();
            let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();

            // Try to include trailing whitespace
            let mut end = word_end;
            while end < chars.len() && chars[end].is_whitespace() {
                end += 1;
            }

            if end == word_end {
                // No trailing, try leading
                let mut start = word_start;
                while start > 0 && chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                (start, word_end)
            } else {
                (word_start, end)
            }
        }

        fn get_inner_long_word_bounds(&self) -> (usize, usize) {
            let line = &self.lines[self.cursor.0];
            if line.is_empty() {
                return (0, 0);
            }
            let chars: Vec<char> = line.chars().collect();
            let col = self.cursor.1.min(chars.len().saturating_sub(1));

            let mut start = col;
            let mut end = col;

            if !chars[col].is_whitespace() {
                while start > 0 && !chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                while end < chars.len() && !chars[end].is_whitespace() {
                    end += 1;
                }
            } else {
                while start > 0 && chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                while end < chars.len() && chars[end].is_whitespace() {
                    end += 1;
                }
            }
            (start, end)
        }

        fn delete_inner_word(&mut self) {
            let (start, end) = self.get_inner_word_bounds();
            self.delete_text_object_on_line(start, end);
        }

        fn delete_a_word(&mut self) {
            let (start, end) = self.get_a_word_bounds();
            self.delete_text_object_on_line(start, end);
        }

        fn delete_inner_long_word(&mut self) {
            let (start, end) = self.get_inner_long_word_bounds();
            self.delete_text_object_on_line(start, end);
        }

        fn get_word_forward_pos(&self) -> (usize, usize) {
            // Simple implementation: move to next word
            let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let mut col = self.cursor.1;

            // Skip current word
            while col < chars.len() && !chars[col].is_whitespace() {
                col += 1;
            }
            // Skip whitespace
            while col < chars.len() && chars[col].is_whitespace() {
                col += 1;
            }

            // If we reached end of line, go to next line
            if col >= chars.len() && self.cursor.0 < self.lines.len() - 1 {
                return (self.cursor.0 + 1, 0);
            }
            (self.cursor.0, col)
        }

        /// Helper for testing delete motions
        fn perform_delete_motion_for_test(
            &mut self,
            end: (usize, usize),
            is_inclusive: bool,
            _delete_empty_lines: bool,
            allow_linewise: bool,
        ) {
            self.save_undo_state();
            self.lines_version += 1;

            let start = self.cursor;

            if end.0 > start.0 {
                // Forward multi-line
                if end.1 == 0 && !allow_linewise {
                    // Preserve line structure
                    let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                    let start_prefix: String = start_chars[..start.1].iter().collect();
                    self.yank_buffer = start_chars[start.1..].iter().collect::<String>();
                    self.yank_is_linewise = false;
                    self.lines[start.0] = start_prefix;
                    for _ in (start.0 + 1)..end.0 {
                        self.lines.remove(start.0 + 1);
                    }
                    self.cursor = start;
                } else {
                    // Merge lines
                    let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                    let start_prefix: String = start_chars[..start.1].iter().collect();
                    let end_chars: Vec<char> = self.lines[end.0].chars().collect();
                    let end_suffix: String = end_chars[end.1..].iter().collect();

                    self.lines[start.0] = start_prefix + &end_suffix;
                    for _ in start.0 + 1..=end.0 {
                        self.lines.remove(start.0 + 1);
                    }
                    self.cursor = start;
                }
            } else if end.0 == start.0 && end.1 > start.1 {
                // Forward same line
                let mut chars: Vec<char> = self.lines[start.0].chars().collect();
                let range_end = if is_inclusive {
                    (end.1 + 1).min(chars.len())
                } else {
                    end.1
                };
                if start.1 < range_end {
                    self.yank_buffer = chars[start.1..range_end].iter().collect();
                    self.yank_is_linewise = false;
                    chars.drain(start.1..range_end);
                    self.lines[start.0] = chars.into_iter().collect();
                }
            }

            self.clamp_cursor();
            self.maybe_record_change();
        }

        // ============ Visual Block Mode Methods ============

        fn get_visual_selection(&self) -> ((usize, usize), (usize, usize)) {
            if self.visual_start.0 < self.cursor.0
                || (self.visual_start.0 == self.cursor.0 && self.visual_start.1 <= self.cursor.1)
            {
                (self.visual_start, self.cursor)
            } else {
                (self.cursor, self.visual_start)
            }
        }

        fn delete_visual_block(&mut self) {
            let (start, end) = self.get_visual_selection();
            let min_col = self.visual_start.1.min(self.cursor.1);
            let max_col = self.visual_start.1.max(self.cursor.1);
            let min_row = start.0;
            let max_row = end.0;

            // Yank the block
            let mut yanked_lines = Vec::new();
            for row in min_row..=max_row {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = min_col.min(line_len);
                let sel_end = (max_col + 1).min(line_len);
                if sel_start < sel_end {
                    yanked_lines.push(chars[sel_start..sel_end].iter().collect::<String>());
                } else {
                    yanked_lines.push(String::new());
                }
            }
            self.yank_buffer = yanked_lines.join("\n");
            self.yank_is_linewise = false;
            self.yank_is_block = true;

            // Move cursor to top-left BEFORE saving undo state (Neovim behavior)
            self.cursor = (min_row, min_col);
            self.save_undo_state();

            // Delete the block
            self.lines_version += 1;
            for row in min_row..=max_row {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = min_col.min(line_len);
                let sel_end = (max_col + 1).min(line_len);
                if sel_start < sel_end {
                    let new_line: String =
                        chars[..sel_start].iter().chain(&chars[sel_end..]).collect();
                    self.lines[row] = new_line;
                }
            }

            self.clamp_cursor();
            self.record_change();
        }

        fn yank_visual_block(&mut self) {
            let (start, end) = self.get_visual_selection();
            let min_col = self.visual_start.1.min(self.cursor.1);
            let max_col = self.visual_start.1.max(self.cursor.1);
            let min_row = start.0;
            let max_row = end.0;

            let mut yanked_lines = Vec::new();
            for row in min_row..=max_row {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = min_col.min(line_len);
                let sel_end = (max_col + 1).min(line_len);
                if sel_start < sel_end {
                    yanked_lines.push(chars[sel_start..sel_end].iter().collect::<String>());
                } else {
                    yanked_lines.push(String::new());
                }
            }
            self.yank_buffer = yanked_lines.join("\n");
            self.yank_is_linewise = false;
            self.yank_is_block = true;
            self.cursor = (min_row, min_col);
        }

        fn paste_block_after(&mut self) {
            if self.yank_buffer.is_empty() || !self.yank_is_block {
                return;
            }
            self.save_undo_state();
            self.lines_version += 1;

            let paste_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let start_row = self.cursor.0;

            for (i, paste_line) in paste_lines.iter().enumerate() {
                let target_row = start_row + i;
                if target_row >= self.lines.len() {
                    self.lines.push(String::new());
                }

                let mut chars: Vec<char> = self.lines[target_row].chars().collect();
                let insert_pos = if chars.is_empty() {
                    0
                } else {
                    (self.cursor.1 + 1).min(chars.len())
                };

                // Pad with spaces if needed
                while chars.len() < insert_pos {
                    chars.push(' ');
                }

                let paste_chars: Vec<char> = paste_line.chars().collect();
                for (j, c) in paste_chars.iter().enumerate() {
                    chars.insert(insert_pos + j, *c);
                }
                self.lines[target_row] = chars.into_iter().collect();
            }

            self.record_change();
        }

        fn delete_block_at_cursor(&mut self, num_rows: usize, col_width: usize) {
            self.save_undo_state();
            self.lines_version += 1;

            let start_row = self.cursor.0;
            let start_col = self.cursor.1;
            let end_col = start_col + col_width;

            // Yank the block first
            let mut yanked_lines = Vec::new();
            for i in 0..num_rows {
                let row = start_row + i;
                if row >= self.lines.len() {
                    break;
                }
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = start_col.min(line_len);
                let sel_end = end_col.min(line_len);
                if sel_start < sel_end {
                    yanked_lines.push(chars[sel_start..sel_end].iter().collect::<String>());
                } else {
                    yanked_lines.push(String::new());
                }
            }
            self.yank_buffer = yanked_lines.join("\n");
            self.yank_is_linewise = false;
            self.yank_is_block = true;

            // Delete the block
            for i in 0..num_rows {
                let row = start_row + i;
                if row >= self.lines.len() {
                    break;
                }
                let chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = start_col.min(line_len);
                let sel_end = end_col.min(line_len);
                if sel_start < sel_end {
                    let new_line: String =
                        chars[..sel_start].iter().chain(&chars[sel_end..]).collect();
                    self.lines[row] = new_line;
                }
            }

            self.clamp_cursor();
            self.record_change();
        }

        fn change_block_at_cursor(&mut self, num_rows: usize, col_width: usize, text: &str) {
            self.save_undo_state();
            self.lines_version += 1;

            let start_row = self.cursor.0;
            let start_col = self.cursor.1;
            let end_col = start_col + col_width;

            // Delete the block and insert text on each row
            for i in 0..num_rows {
                let row = start_row + i;
                if row >= self.lines.len() {
                    break;
                }
                let mut chars: Vec<char> = self.lines[row].chars().collect();
                let line_len = chars.len();
                let sel_start = start_col.min(line_len);
                let sel_end = end_col.min(line_len);

                // Delete the block portion
                if sel_start < sel_end {
                    chars.drain(sel_start..sel_end);
                }

                // Insert the text at sel_start
                let mut offset = 0;
                for c in text.chars() {
                    if c == '\n' {
                        continue;
                    }
                    chars.insert(sel_start + offset, c);
                    offset += 1;
                }
                self.lines[row] = chars.into_iter().collect();
            }

            self.clamp_cursor();
            self.record_change();
        }

        fn insert_block_at_cursor(&mut self, num_rows: usize, text: &str) {
            self.save_undo_state();
            self.lines_version += 1;

            let start_row = self.cursor.0;
            let insert_col = self.cursor.1;

            // Insert text on each row
            for i in 0..num_rows {
                let row = start_row + i;
                if row >= self.lines.len() {
                    break;
                }
                let chars: Vec<char> = self.lines[row].chars().collect();

                // Skip lines that don't reach the insert column
                if chars.len() < insert_col {
                    continue;
                }

                let mut chars = chars;

                // Insert the text
                let mut offset = 0;
                for c in text.chars() {
                    if c == '\n' {
                        continue;
                    }
                    chars.insert(insert_col + offset, c);
                    offset += 1;
                }
                self.lines[row] = chars.into_iter().collect();
            }

            self.clamp_cursor();
            self.record_change();
        }

        fn insert_block_at_cursor_with_offset(
            &mut self,
            num_rows: usize,
            col_offset: usize,
            text: &str,
        ) {
            self.save_undo_state();
            self.lines_version += 1;

            let start_row = self.cursor.0;
            let insert_col = self.cursor.1 + col_offset;

            // Insert text on each row
            for i in 0..num_rows {
                let row = start_row + i;
                if row >= self.lines.len() {
                    break;
                }
                let mut chars: Vec<char> = self.lines[row].chars().collect();

                // Pad with spaces if needed to reach insert position
                while chars.len() < insert_col {
                    chars.push(' ');
                }

                // Insert the text
                let mut offset = 0;
                for c in text.chars() {
                    if c == '\n' {
                        continue;
                    }
                    chars.insert(insert_col + offset, c);
                    offset += 1;
                }
                self.lines[row] = chars.into_iter().collect();
            }

            self.clamp_cursor();
            self.record_change();
        }

        // ============ Visual Mode Helper Methods ============

        fn delete_visual_selection(&mut self) {
            let (start, end) = self.get_visual_selection();

            if self.mode == EditorMode::VisualLine {
                // Delete entire lines
                let yanked: Vec<&str> = self.lines[start.0..=end.0]
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                self.yank_buffer = yanked.join("\n");
                self.yank_is_linewise = true;
                self.yank_is_block = false;

                self.save_undo_state_with_cursor(start);
                self.lines_version += 1;
                for _ in start.0..=end.0 {
                    if self.lines.len() > 1 {
                        self.lines.remove(start.0);
                    } else {
                        self.lines[0].clear();
                    }
                }
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.cursor.0 = start.0.min(self.lines.len().saturating_sub(1));
                self.cursor.1 = 0;
            } else {
                // Character-wise deletion
                let line = &self.lines[start.0];
                let chars: Vec<char> = line.chars().collect();
                let sel_end = (end.1 + 1).min(chars.len());
                self.yank_buffer = chars[start.1..sel_end].iter().collect();
                self.yank_is_linewise = false;
                self.yank_is_block = false;

                self.save_undo_state_with_cursor(start);
                self.lines_version += 1;
                let new_line: String = chars[..start.1].iter().chain(&chars[sel_end..]).collect();
                self.lines[start.0] = new_line;
                self.cursor = start;
            }
            self.clamp_cursor();
            self.mode = EditorMode::Normal;
            self.record_change();
        }

        fn delete_visual_lines(&mut self) {
            let (start, end) = self.get_visual_selection();
            self.save_undo_state_with_cursor((start.0, 0));
            self.lines_version += 1;

            // Yank the lines first
            let yanked: Vec<String> = self.lines[start.0..=end.0]
                .iter()
                .map(|s| s.to_string())
                .collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
            self.yank_is_block = false;

            // Delete the lines
            self.lines.drain(start.0..=end.0);
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }

            self.cursor.0 = start.0.min(self.lines.len().saturating_sub(1));
            self.cursor.1 = 0;
            self.record_change();
            self.mode = EditorMode::Normal;
        }

        fn change_visual_lines(&mut self) {
            let (start, end) = self.get_visual_selection();
            self.save_undo_state_with_cursor((start.0, 0));
            self.lines_version += 1;

            // Yank the lines first
            let yanked: Vec<String> = self.lines[start.0..=end.0]
                .iter()
                .map(|s| s.to_string())
                .collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
            self.yank_is_block = false;

            // Delete the lines, leave one empty line for insert
            self.lines.drain(start.0..=end.0);
            if self.lines.is_empty() {
                self.lines.push(String::new());
            } else {
                self.lines.insert(start.0, String::new());
            }

            self.cursor = (start.0, 0);
            self.mode = EditorMode::Insert;
        }

        fn delete_block_to_eol(&mut self) {
            let (start, end) = self.get_visual_selection();
            let min_col = self.visual_start.1.min(self.cursor.1);
            self.save_undo_state_with_cursor((start.0, min_col));
            self.lines_version += 1;

            // Delete from min_col to end of each line
            let mut yanked_lines = Vec::new();
            for row in start.0..=end.0 {
                let chars: Vec<char> = self.lines[row].chars().collect();
                if min_col < chars.len() {
                    yanked_lines.push(chars[min_col..].iter().collect::<String>());
                    self.lines[row] = chars[..min_col].iter().collect();
                } else {
                    yanked_lines.push(String::new());
                }
            }
            self.yank_buffer = yanked_lines.join("\n");
            self.yank_is_linewise = false;
            self.yank_is_block = true;

            self.cursor = (start.0, min_col);
            self.clamp_cursor();
            self.record_change();
            self.mode = EditorMode::Normal;
        }

        fn change_block_to_eol(&mut self) {
            let (start, end) = self.get_visual_selection();
            let min_col = self.visual_start.1.min(self.cursor.1);
            self.save_undo_state_with_cursor((start.0, min_col));
            self.lines_version += 1;

            // Delete from min_col to end of each line
            let mut yanked_lines = Vec::new();
            for row in start.0..=end.0 {
                let chars: Vec<char> = self.lines[row].chars().collect();
                if min_col < chars.len() {
                    yanked_lines.push(chars[min_col..].iter().collect::<String>());
                    self.lines[row] = chars[..min_col].iter().collect();
                } else {
                    yanked_lines.push(String::new());
                }
            }
            self.yank_buffer = yanked_lines.join("\n");
            self.yank_is_linewise = false;
            self.yank_is_block = true;

            self.cursor = (start.0, min_col);
            self.mode = EditorMode::Insert;
        }

        // ============ Number Manipulation Methods ============

        fn increment_number(&mut self, count: usize) {
            self.modify_number(count as i64);
        }

        fn decrement_number(&mut self, count: usize) {
            self.modify_number(-(count as i64));
        }

        fn modify_number(&mut self, delta: i64) {
            let Some(num) = self.find_number_at_cursor() else {
                return;
            };

            let new_num_str = if num.is_hex {
                self.format_hex_number(&num.digits, delta)
            } else {
                self.format_decimal_number(&num.digits, num.is_negative, delta)
            };

            // Replace in line
            self.save_undo_state();
            self.lines_version += 1;
            let line = &mut self.lines[self.cursor.0];
            let chars: Vec<char> = line.chars().collect();
            let before: String = chars[..num.start].iter().collect();
            let after: String = chars[num.end..].iter().collect();
            *line = format!("{}{}{}", before, new_num_str, after);

            // Position cursor at the last digit of the new number
            let new_end = num.start + new_num_str.chars().count();
            self.cursor.1 = new_end.saturating_sub(1);
            self.record_change();
        }

        fn find_number_at_cursor(&self) -> Option<NumberAtCursor> {
            let line = &self.lines[self.cursor.0];
            let chars: Vec<char> = line.chars().collect();

            // Try to find hex number first (0x prefix)
            if let Some(num) = self.try_find_hex_number(&chars, self.cursor.1) {
                return Some(num);
            }

            // Try decimal number at cursor
            if let Some(num) = self.try_find_decimal_number(&chars, self.cursor.1) {
                return Some(num);
            }

            // Search forward for a number
            for pos in self.cursor.1..chars.len() {
                if let Some(num) = self.try_find_hex_number(&chars, pos) {
                    return Some(num);
                }
                if let Some(num) = self.try_find_decimal_number(&chars, pos) {
                    return Some(num);
                }
            }

            None
        }

        fn try_find_hex_number(&self, chars: &[char], pos: usize) -> Option<NumberAtCursor> {
            if pos >= chars.len() {
                return None;
            }

            // Check if we're on a hex digit or 'x'
            let c = chars[pos];
            if !c.is_ascii_hexdigit() && c != 'x' && c != 'X' {
                return None;
            }

            // Search backward for 0x prefix
            let mut prefix_start = pos;
            while prefix_start > 0 {
                let prev = chars[prefix_start - 1];
                if prev.is_ascii_hexdigit() || prev == 'x' || prev == 'X' {
                    prefix_start -= 1;
                } else if prev == '0' && prefix_start >= 1 {
                    // Check if this is the 0 of 0x
                    prefix_start -= 1;
                    break;
                } else {
                    break;
                }
            }

            // Verify we have 0x prefix
            if prefix_start + 1 >= chars.len() {
                return None;
            }
            if chars[prefix_start] != '0'
                || (chars[prefix_start + 1] != 'x' && chars[prefix_start + 1] != 'X')
            {
                return None;
            }

            // Find end of hex digits
            let mut end = prefix_start + 2;
            while end < chars.len() && chars[end].is_ascii_hexdigit() {
                end += 1;
            }

            if end <= prefix_start + 2 {
                return None; // No digits after 0x
            }

            let digits: String = chars[prefix_start + 2..end].iter().collect();

            Some(NumberAtCursor {
                start: prefix_start,
                end,
                digits,
                is_hex: true,
                is_negative: false,
            })
        }

        fn try_find_decimal_number(&self, chars: &[char], pos: usize) -> Option<NumberAtCursor> {
            if pos >= chars.len() {
                return None;
            }

            let c = chars[pos];
            let is_negative = c == '-';

            // Must be on a digit or negative sign followed by digit
            if !c.is_ascii_digit() && !is_negative {
                return None;
            }

            if is_negative {
                if pos + 1 >= chars.len() || !chars[pos + 1].is_ascii_digit() {
                    return None;
                }
            }

            // Check if this digit is part of a hex number
            if self.try_find_hex_number(chars, pos).is_some() {
                return None;
            }

            // Find start of decimal number
            let mut num_start = pos;
            while num_start > 0 && chars[num_start - 1].is_ascii_digit() {
                num_start -= 1;
            }

            // Check for negative sign
            let is_negative = if !is_negative && num_start > 0 && chars[num_start - 1] == '-' {
                true
            } else {
                is_negative
            };

            // Find end of decimal number
            let mut end = pos;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }

            let digits: String = chars[num_start..end].iter().collect();
            if digits.is_empty() {
                return None;
            }

            let start = if is_negative && num_start > 0 && chars[num_start - 1] == '-' {
                num_start - 1
            } else {
                num_start
            };

            Some(NumberAtCursor {
                start,
                end,
                digits,
                is_hex: false,
                is_negative,
            })
        }

        fn format_hex_number(&self, digits: &str, delta: i64) -> String {
            let width = digits.len();
            let parsed = u64::from_str_radix(digits, 16).unwrap_or(0);
            let new_value = if delta >= 0 {
                parsed.wrapping_add(delta as u64)
            } else {
                parsed.wrapping_sub((-delta) as u64)
            };
            format!("0x{:0>width$x}", new_value, width = width)
        }

        fn format_decimal_number(&self, digits: &str, is_negative: bool, delta: i64) -> String {
            let parsed = digits.parse::<i64>().unwrap_or(0);
            let value = if is_negative { -parsed } else { parsed };
            let new_value = value + delta;

            // Only preserve width if original number has leading zeros
            let has_leading_zeros = digits.len() > 1 && digits.starts_with('0');

            if has_leading_zeros {
                let width = digits.len();
                if new_value >= 0 {
                    format!("{:0>width$}", new_value, width = width)
                } else {
                    format!("-{:0>width$}", -new_value, width = width)
                }
            } else {
                format!("{}", new_value)
            }
        }

        fn repeat_last_change(&mut self) {
            let has_explicit_count = self.count_prefix.is_some();
            let explicit_count = self.take_count();
            let use_count = if has_explicit_count {
                self.last_count = explicit_count;
                explicit_count
            } else {
                self.last_count
            };

            match self.last_change.clone() {
                LastChange::None => {}
                LastChange::IncrementNumber => self.increment_number(use_count),
                LastChange::DecrementNumber => self.decrement_number(use_count),
                _ => {}
            }
        }
    }

    // ============ Basic Cursor Tests ============

    #[test]
    fn test_clamp_cursor_row() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (5, 0); // Row out of bounds
        editor.clamp_cursor();
        assert_eq!(editor.cursor.0, 2); // Should clamp to last line
    }

    #[test]
    fn test_clamp_cursor_col_normal_mode() {
        let mut editor = TestEditor::new("hello");
        editor.cursor = (0, 10); // Column out of bounds
        editor.clamp_cursor();
        assert_eq!(editor.cursor.1, 4); // Should clamp to last char (len-1 in normal mode)
    }

    #[test]
    fn test_clamp_cursor_col_insert_mode() {
        let mut editor = TestEditor::new("hello");
        editor.mode = EditorMode::Insert;
        editor.cursor = (0, 10);
        editor.clamp_cursor();
        assert_eq!(editor.cursor.1, 5); // Should clamp to end (len in insert mode)
    }

    #[test]
    fn test_clamp_cursor_empty_line() {
        let mut editor = TestEditor::new("");
        editor.cursor = (0, 5);
        editor.clamp_cursor();
        assert_eq!(editor.cursor.1, 0);
    }

    // ============ Insert/Delete Tests ============

    #[test]
    fn test_insert_char_at_beginning() {
        let mut editor = TestEditor::new("hello");
        editor.insert_char('X');
        assert_eq!(editor.text(), "Xhello");
        assert_eq!(editor.cursor, (0, 1));
    }

    #[test]
    fn test_insert_char_in_middle() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 2);
        editor.insert_char('X');
        assert_eq!(editor.text(), "heXllo");
        assert_eq!(editor.cursor, (0, 3));
    }

    #[test]
    fn test_insert_char_at_end() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 5);
        editor.insert_char('X');
        assert_eq!(editor.text(), "helloX");
        assert_eq!(editor.cursor, (0, 6));
    }

    #[test]
    fn test_delete_char() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 2);
        editor.delete_char();
        assert_eq!(editor.text(), "helo");
    }

    #[test]
    fn test_delete_char_at_end_no_op() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 5);
        editor.delete_char();
        assert_eq!(editor.text(), "hello"); // No change
    }

    // ============ Undo/Redo Tests ============

    #[test]
    fn test_undo_single_change() {
        let mut editor = TestEditor::new("hello");
        editor.insert_char('X');
        editor.lines_version += 1;
        editor.record_change();
        assert_eq!(editor.text(), "Xhello");

        editor.undo();
        assert_eq!(editor.text(), "hello");
    }

    #[test]
    fn test_redo_after_undo() {
        let mut editor = TestEditor::new("hello");
        editor.insert_char('X');
        editor.lines_version += 1;
        editor.record_change();

        editor.undo();
        assert_eq!(editor.text(), "hello");

        editor.redo();
        assert_eq!(editor.text(), "Xhello");
    }

    #[test]
    fn test_undo_at_beginning_no_op() {
        let mut editor = TestEditor::new("hello");
        editor.undo();
        assert_eq!(editor.text(), "hello");
        assert_eq!(editor.history_idx, 0);
    }

    #[test]
    fn test_redo_at_end_no_op() {
        let mut editor = TestEditor::new("hello");
        editor.redo();
        assert_eq!(editor.text(), "hello");
    }

    #[test]
    fn test_redo_clamps_cursor() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (1, 0);
        editor.delete_line(1);
        editor.record_change();
        assert_eq!(editor.text(), "line1\nline3");

        editor.undo();
        assert_eq!(editor.text(), "line1\nline2\nline3");

        // Simulate cursor being on a line that will be deleted
        editor.cursor = (2, 3);
        editor.redo();
        // Cursor should be clamped since line 2 no longer has 4 chars
        assert!(editor.cursor.0 <= 1);
    }

    // ============ Pair Finding Tests ============

    #[test]
    fn test_find_pair_bounds_same_line() {
        let editor = TestEditor::new("(hello)").with_cursor(0, 1);
        let bounds = editor.find_pair_bounds('(');
        assert_eq!(bounds, Some(((0, 0), (0, 6))));
    }

    #[test]
    fn test_find_pair_bounds_cursor_on_open() {
        let editor = TestEditor::new("(hello)").with_cursor(0, 0);
        let bounds = editor.find_pair_bounds('(');
        assert_eq!(bounds, Some(((0, 0), (0, 6))));
    }

    #[test]
    fn test_find_pair_bounds_cursor_on_close() {
        let editor = TestEditor::new("(hello)").with_cursor(0, 6);
        let bounds = editor.find_pair_bounds(')');
        assert_eq!(bounds, Some(((0, 0), (0, 6))));
    }

    #[test]
    fn test_find_pair_bounds_nested() {
        let editor = TestEditor::new("((inner))").with_cursor(0, 2);
        let bounds = editor.find_pair_bounds('(');
        assert_eq!(bounds, Some(((0, 1), (0, 7))));
    }

    #[test]
    fn test_find_pair_bounds_multi_line() {
        let editor = TestEditor::new("(\nhello\n)").with_cursor(1, 2);
        let bounds = editor.find_pair_bounds('(');
        assert_eq!(bounds, Some(((0, 0), (2, 0))));
    }

    #[test]
    fn test_find_pair_bounds_no_match() {
        let editor = TestEditor::new("hello").with_cursor(0, 2);
        let bounds = editor.find_pair_bounds('(');
        assert_eq!(bounds, None);
    }

    #[test]
    fn test_find_pair_bounds_square_brackets() {
        let editor = TestEditor::new("[a, b, c]").with_cursor(0, 3);
        let bounds = editor.find_pair_bounds('[');
        assert_eq!(bounds, Some(((0, 0), (0, 8))));
    }

    #[test]
    fn test_find_pair_bounds_curly_braces() {
        let editor = TestEditor::new("{ key: value }").with_cursor(0, 5);
        let bounds = editor.find_pair_bounds('{');
        assert_eq!(bounds, Some(((0, 0), (0, 13))));
    }

    // ============ Yank Inner Pair Tests ============

    #[test]
    fn test_yank_inner_pair_same_line() {
        let mut editor = TestEditor::new("(hello)").with_cursor(0, 3);
        editor.yank_inner_pair('(');
        assert_eq!(editor.yank_buffer, "hello");
        assert!(!editor.yank_is_linewise);
    }

    #[test]
    fn test_yank_inner_pair_empty() {
        let mut editor = TestEditor::new("()").with_cursor(0, 0);
        editor.yank_inner_pair('(');
        assert_eq!(editor.yank_buffer, "");
    }

    #[test]
    fn test_yank_inner_pair_multi_line_brackets_alone() {
        let mut editor = TestEditor::new("(\nhello\n)").with_cursor(1, 2);
        editor.yank_inner_pair('(');
        assert_eq!(editor.yank_buffer, "\nhello");
    }

    #[test]
    fn test_yank_inner_pair_multi_line_with_whitespace() {
        let mut editor = TestEditor::new("(  \nhello\n  )").with_cursor(1, 2);
        editor.yank_inner_pair('(');
        // Whitespace-only content after open and before close should be skipped
        assert_eq!(editor.yank_buffer, "\nhello");
    }

    #[test]
    fn test_yank_inner_pair_multi_line_with_content() {
        let mut editor = TestEditor::new("(start\nmiddle\nend)").with_cursor(1, 2);
        editor.yank_inner_pair('(');
        assert_eq!(editor.yank_buffer, "start\nmiddle\nend");
    }

    // ============ Delete Inner Pair Tests ============

    #[test]
    fn test_delete_inner_pair_same_line() {
        let mut editor = TestEditor::new("(hello)").with_cursor(0, 3);
        editor.delete_inner_pair('(');
        assert_eq!(editor.text(), "()");
        assert_eq!(editor.cursor, (0, 1));
    }

    #[test]
    fn test_delete_inner_pair_multi_line() {
        let mut editor = TestEditor::new("(\nhello\n)").with_cursor(1, 2);
        editor.delete_inner_pair('(');
        // Should keep brackets on separate lines
        assert_eq!(editor.lines.len(), 2);
        assert_eq!(editor.lines[0], "(");
        assert_eq!(editor.lines[1], ")");
    }

    #[test]
    fn test_delete_inner_pair_nested() {
        let mut editor = TestEditor::new("((inner))").with_cursor(0, 3);
        editor.delete_inner_pair('(');
        assert_eq!(editor.text(), "(())");
    }

    // ============ Unicode Tests ============

    #[test]
    fn test_insert_unicode() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 2);
        editor.insert_char('日');
        assert_eq!(editor.text(), "he日llo");
    }

    #[test]
    fn test_cursor_with_unicode() {
        let mut editor = TestEditor::new("日本語");
        editor.cursor = (0, 10);
        editor.clamp_cursor();
        assert_eq!(editor.cursor.1, 2); // 3 chars, max is 2 in normal mode
    }

    #[test]
    fn test_delete_unicode() {
        let mut editor = TestEditor::new("日本語").with_cursor(0, 1);
        editor.delete_char();
        assert_eq!(editor.text(), "日語");
    }

    // ============ Undo/Redo with Multi-line Operations ============

    #[test]
    fn test_undo_redo_multiline_delete_clamps_cursor() {
        // This tests the fix for the panic when doing di( on multi-line, undo, redo
        let mut editor = TestEditor::new("(\ntesting1\n)");
        editor.cursor = (1, 3); // On "testing1"

        // Simulate di( - delete inner content
        editor.delete_inner_pair('(');
        // After delete, we should have 2 lines: "(" and ")"
        assert_eq!(editor.lines.len(), 2);

        // Undo
        editor.undo();
        assert_eq!(editor.lines.len(), 3);
        assert_eq!(editor.text(), "(\ntesting1\n)");

        // Redo - this should not panic even if cursor was on line 1
        editor.cursor = (1, 5); // Position that might be invalid after redo
        editor.redo();
        // Cursor should be clamped to valid position
        assert!(editor.cursor.0 < editor.lines.len());
        assert!(editor.cursor.1 <= editor.lines[editor.cursor.0].chars().count());
    }

    #[test]
    fn test_redo_clamps_row_when_lines_deleted() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5");
        editor.cursor = (2, 0);

        // Delete multiple lines
        editor.delete_line(2);
        editor.delete_line(2);
        editor.delete_line(2);
        editor.record_change();

        assert_eq!(editor.text(), "line1\nline2");

        editor.undo();
        assert_eq!(editor.text(), "line1\nline2\nline3\nline4\nline5");

        // Set cursor to a line that won't exist after redo
        editor.cursor = (4, 0);
        editor.redo();

        // Should not panic, cursor should be clamped
        assert!(editor.cursor.0 < editor.lines.len());
    }

    // ============ Delete Line Tests ============

    #[test]
    fn test_delete_line_single() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (1, 0);
        editor.delete_line(1);
        assert_eq!(editor.text(), "line1\nline3");
    }

    #[test]
    fn test_delete_line_last_line() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (2, 0);
        editor.delete_line(2);
        assert_eq!(editor.text(), "line1\nline2");
    }

    #[test]
    fn test_delete_only_line_clears() {
        let mut editor = TestEditor::new("only line");
        editor.delete_line(0);
        assert_eq!(editor.text(), "");
        assert_eq!(editor.cursor, (0, 0));
    }

    // ============ Multi-line Yank Tests ============

    #[test]
    fn test_yank_inner_pair_bracket_with_leading_whitespace() {
        // Bracket is first non-whitespace but not first char
        let mut editor = TestEditor::new("  (\n  testing\n  )").with_cursor(1, 3);
        editor.yank_inner_pair('(');
        assert_eq!(editor.yank_buffer, "\n  testing");
    }

    #[test]
    fn test_yank_inner_pair_content_after_open_bracket() {
        let mut editor = TestEditor::new("(start\nmiddle\nend)").with_cursor(1, 0);
        editor.yank_inner_pair('(');
        // Should include "start" as-is since there's content after (
        assert_eq!(editor.yank_buffer, "start\nmiddle\nend");
    }

    #[test]
    fn test_yank_inner_pair_content_before_close_bracket() {
        let mut editor = TestEditor::new("(\nmiddle\nend)").with_cursor(1, 0);
        editor.yank_inner_pair('(');
        // Should include "end" as-is since there's content before )
        assert_eq!(editor.yank_buffer, "\nmiddle\nend");
    }

    // ============ Cursor Clamping Edge Cases ============

    #[test]
    fn test_clamp_cursor_on_empty_lines() {
        let mut editor = TestEditor::new("\n\n");
        editor.cursor = (1, 5);
        editor.clamp_cursor();
        assert_eq!(editor.cursor.1, 0);
    }

    #[test]
    fn test_clamp_cursor_row_beyond_last() {
        let mut editor = TestEditor::new("a\nb");
        editor.cursor = (10, 0);
        editor.clamp_cursor();
        assert_eq!(editor.cursor.0, 1); // Clamped to last line
    }

    // ============ History Edge Cases ============

    #[test]
    fn test_multiple_undo_redo_cycles() {
        let mut editor = TestEditor::new("start");

        // Make several changes
        editor.insert_char('1');
        editor.record_change();
        editor.insert_char('2');
        editor.record_change();
        editor.insert_char('3');
        editor.record_change();

        assert_eq!(editor.text(), "123start");

        // Undo all
        editor.undo();
        assert_eq!(editor.text(), "12start");
        editor.undo();
        assert_eq!(editor.text(), "1start");
        editor.undo();
        assert_eq!(editor.text(), "start");

        // Redo all
        editor.redo();
        assert_eq!(editor.text(), "1start");
        editor.redo();
        assert_eq!(editor.text(), "12start");
        editor.redo();
        assert_eq!(editor.text(), "123start");
    }

    #[test]
    fn test_new_change_after_undo_truncates_history() {
        let mut editor = TestEditor::new("start");

        editor.insert_char('A');
        editor.record_change();
        editor.insert_char('B');
        editor.record_change();

        // Undo one change
        editor.undo();
        assert_eq!(editor.text(), "Astart");

        // Make a new change - should truncate redo history
        editor.insert_char('X');
        editor.record_change();
        assert_eq!(editor.text(), "AXstart");

        // Redo should do nothing (history truncated)
        editor.redo();
        assert_eq!(editor.text(), "AXstart");
    }

    // ============ Pair Matching Edge Cases ============

    #[test]
    fn test_find_pair_deeply_nested() {
        let editor = TestEditor::new("(((deep)))").with_cursor(0, 3);
        let bounds = editor.find_pair_bounds('(');
        // Cursor is on 'd', should find innermost pair
        assert_eq!(bounds, Some(((0, 2), (0, 7))));
    }

    #[test]
    fn test_find_pair_multiple_pairs_on_line() {
        let editor = TestEditor::new("(a) (b) (c)").with_cursor(0, 5);
        let bounds = editor.find_pair_bounds('(');
        // Cursor is on 'b', should find middle pair
        assert_eq!(bounds, Some(((0, 4), (0, 6))));
    }

    #[test]
    fn test_find_pair_quotes() {
        let editor = TestEditor::new("\"hello\"").with_cursor(0, 3);
        let bounds = editor.find_pair_bounds('"');
        assert_eq!(bounds, Some(((0, 0), (0, 6))));
    }

    #[test]
    fn test_delete_inner_pair_empty() {
        let mut editor = TestEditor::new("()").with_cursor(0, 0);
        editor.delete_inner_pair('(');
        assert_eq!(editor.text(), "()");
    }

    // ============ Undo Behavior Tests ============

    #[test]
    fn test_delete_inner_word_undo() {
        // diw should create a single undo entry
        let mut editor = TestEditor::new("hello world");
        editor.cursor = (0, 0);
        editor.delete_inner_word();
        assert_eq!(editor.text(), " world");

        // Undo should restore the original
        editor.undo();
        assert_eq!(editor.text(), "hello world");
    }

    #[test]
    fn test_delete_a_word_undo() {
        // daw should create a single undo entry
        let mut editor = TestEditor::new("hello world");
        editor.cursor = (0, 0);
        editor.delete_a_word();
        assert_eq!(editor.text(), "world");

        // Undo should restore the original
        editor.undo();
        assert_eq!(editor.text(), "hello world");
    }

    #[test]
    fn test_delete_inner_long_word_undo() {
        let mut editor = TestEditor::new("hello-world test");
        editor.cursor = (0, 0);
        editor.delete_inner_long_word();
        assert_eq!(editor.text(), " test");

        editor.undo();
        assert_eq!(editor.text(), "hello-world test");
    }

    // ============ Word Motion Delete Tests ============

    #[test]
    fn test_dw_single_word_line_keeps_line() {
        // dw on single word should delete word but keep the line
        let mut editor = TestEditor::new("word\nnext line");
        editor.cursor = (0, 0);
        // Simulate dw - delete to next word position
        let end = editor.get_word_forward_pos();
        editor.perform_delete_motion_for_test(end, false, true, false);

        // Line should still exist (empty), not deleted
        assert_eq!(editor.lines.len(), 2);
        assert_eq!(editor.lines[0], "");
        assert_eq!(editor.lines[1], "next line");
    }

    #[test]
    fn test_dw_preserves_line_structure() {
        // dw should not merge lines or delete entire line
        let mut editor = TestEditor::new("one two\nthree");
        editor.cursor = (0, 4); // on 'two'
        let end = editor.get_word_forward_pos();
        editor.perform_delete_motion_for_test(end, false, true, false);

        // Should delete 'two' but keep both lines
        assert_eq!(editor.lines.len(), 2);
        assert_eq!(editor.lines[0], "one ");
    }

    // ============ Text Object Helper Tests ============

    #[test]
    fn test_delete_text_object_on_line() {
        let mut editor = TestEditor::new("hello world");
        editor.delete_text_object_on_line(0, 5);
        assert_eq!(editor.text(), " world");
        assert_eq!(editor.cursor.1, 0);
    }

    #[test]
    fn test_yank_text_object_on_line() {
        let mut editor = TestEditor::new("hello world");
        editor.yank_text_object_on_line(6, 11);
        assert_eq!(editor.yank_buffer, "world");
        assert!(!editor.yank_is_linewise);
    }

    #[test]
    fn test_maybe_record_change_normal_mode() {
        let mut editor = TestEditor::new("test");
        editor.mode = EditorMode::Normal;
        editor.lines_version = 1;
        editor.maybe_record_change();
        // Should record change in normal mode
        assert!(editor.history.len() > 1);
    }

    #[test]
    fn test_maybe_record_change_insert_mode() {
        let mut editor = TestEditor::new("test");
        editor.mode = EditorMode::Insert;
        editor.lines_version = 1;
        let history_len_before = editor.history.len();
        editor.maybe_record_change();
        // Should NOT record change in insert mode
        assert_eq!(editor.history.len(), history_len_before);
    }

    // ============ Count Prefix Tests ============

    #[test]
    fn test_add_count_digit_single() {
        let mut editor = TestEditor::new("test");
        editor.add_count_digit('5');
        assert_eq!(editor.count_prefix, Some(5));
    }

    #[test]
    fn test_add_count_digit_multiple() {
        let mut editor = TestEditor::new("test");
        editor.add_count_digit('3');
        editor.add_count_digit('2');
        assert_eq!(editor.count_prefix, Some(32));
    }

    #[test]
    fn test_take_count_with_prefix() {
        let mut editor = TestEditor::new("test");
        editor.add_count_digit('5');
        let count = editor.take_count();
        assert_eq!(count, 5);
        assert_eq!(editor.count_prefix, None);
    }

    #[test]
    fn test_take_count_without_prefix() {
        let mut editor = TestEditor::new("test");
        let count = editor.take_count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_set_last_change() {
        let mut editor = TestEditor::new("test");
        editor.set_last_change(LastChange::Delete(EditTarget::Line), 3);
        assert!(matches!(
            editor.last_change,
            LastChange::Delete(EditTarget::Line)
        ));
        assert_eq!(editor.last_count, 3);
    }

    // ============ Count with Delete Tests ============

    #[test]
    fn test_delete_chars_with_count() {
        let mut editor = TestEditor::new("hello world");
        editor.cursor = (0, 0);
        editor.delete_chars(3); // 3x
        assert_eq!(editor.text(), "lo world");
    }

    #[test]
    fn test_delete_lines_with_count() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4");
        editor.cursor = (0, 0);
        editor.delete_lines(2); // 2dd
        assert_eq!(editor.text(), "line3\nline4");
        assert_eq!(editor.yank_buffer, "line1\nline2");
        assert!(editor.yank_is_linewise);
    }

    #[test]
    fn test_delete_lines_single_undo() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (0, 0);
        let history_before = editor.history.len();
        editor.delete_lines(2); // 2dd should be single undo
        assert_eq!(editor.history.len(), history_before + 1);

        // Undo should restore both lines
        editor.undo();
        assert_eq!(editor.text(), "line1\nline2\nline3");
    }

    // ============ Count with Paste Tests ============

    #[test]
    fn test_paste_after_with_count_characterwise() {
        let mut editor = TestEditor::new("hello");
        editor.yank_buffer = "X".to_string();
        editor.yank_is_linewise = false;
        editor.cursor = (0, 0);
        editor.paste_after_count(3); // 3p
        assert_eq!(editor.text(), "hXXXello");
    }

    #[test]
    fn test_paste_after_with_count_linewise() {
        let mut editor = TestEditor::new("first\nlast");
        editor.yank_buffer = "middle".to_string();
        editor.yank_is_linewise = true;
        editor.cursor = (0, 0);
        editor.paste_after_count(2); // 2p
        assert_eq!(editor.lines.len(), 4);
        assert_eq!(editor.lines[1], "middle");
        assert_eq!(editor.lines[2], "middle");
    }

    #[test]
    fn test_paste_single_undo() {
        let mut editor = TestEditor::new("test");
        editor.yank_buffer = "X".to_string();
        editor.yank_is_linewise = false;
        editor.cursor = (0, 0);
        let history_before = editor.history.len();
        editor.paste_after_count(3);
        assert_eq!(editor.history.len(), history_before + 1);

        // Undo should remove all 3 pastes
        editor.undo();
        assert_eq!(editor.text(), "test");
    }

    // Tests for wrapped line helpers
    #[test]
    fn test_wrapped_line_rows_empty() {
        assert_eq!(EditorState::wrapped_line_rows(0, 80), 1);
    }

    #[test]
    fn test_wrapped_line_rows_fits_single_row() {
        assert_eq!(EditorState::wrapped_line_rows(40, 80), 1);
        assert_eq!(EditorState::wrapped_line_rows(80, 80), 1);
    }

    #[test]
    fn test_wrapped_line_rows_wraps_to_two() {
        assert_eq!(EditorState::wrapped_line_rows(81, 80), 2);
        assert_eq!(EditorState::wrapped_line_rows(160, 80), 2);
    }

    #[test]
    fn test_wrapped_line_rows_wraps_to_multiple() {
        assert_eq!(EditorState::wrapped_line_rows(161, 80), 3);
        assert_eq!(EditorState::wrapped_line_rows(240, 80), 3);
        assert_eq!(EditorState::wrapped_line_rows(241, 80), 4);
    }

    #[test]
    fn test_wrapped_line_rows_zero_width() {
        // Zero width should return 1 (safe default)
        assert_eq!(EditorState::wrapped_line_rows(100, 0), 1);
    }

    #[test]
    fn test_cursor_visual_position_first_row() {
        // Cursor at column 0 in 80-char width
        let (row, col) = EditorState::cursor_visual_position(0, 80);
        assert_eq!(row, 0);
        assert_eq!(col, 0);

        // Cursor at column 79 (last char of first row)
        let (row, col) = EditorState::cursor_visual_position(79, 80);
        assert_eq!(row, 0);
        assert_eq!(col, 79);
    }

    #[test]
    fn test_cursor_visual_position_second_row() {
        // Cursor at column 80 (first char of second row)
        let (row, col) = EditorState::cursor_visual_position(80, 80);
        assert_eq!(row, 1);
        assert_eq!(col, 0);

        // Cursor at column 159 (last char of second row)
        let (row, col) = EditorState::cursor_visual_position(159, 80);
        assert_eq!(row, 1);
        assert_eq!(col, 79);
    }

    #[test]
    fn test_cursor_visual_position_third_row() {
        let (row, col) = EditorState::cursor_visual_position(160, 80);
        assert_eq!(row, 2);
        assert_eq!(col, 0);
    }

    #[test]
    fn test_cursor_visual_position_zero_width() {
        // Zero width should handle gracefully
        let (row, col) = EditorState::cursor_visual_position(50, 0);
        assert_eq!(row, 0);
        assert_eq!(col, 50);
    }

    // Tests for Replace mode
    #[test]
    fn test_replace_char_at_cursor() {
        let mut editor = TestEditor::new("hello");
        editor.cursor = (0, 1);
        let original = editor.replace_char_at_cursor('X');
        assert_eq!(original, Some('e'));
        assert_eq!(editor.text(), "hXllo");
        assert_eq!(editor.cursor.1, 2);
    }

    #[test]
    fn test_replace_char_at_end() {
        let mut editor = TestEditor::new("hello");
        editor.cursor = (0, 4);
        let original = editor.replace_char_at_cursor('X');
        assert_eq!(original, Some('o'));
        assert_eq!(editor.text(), "hellX");
        assert_eq!(editor.cursor.1, 5);
    }

    #[test]
    fn test_replace_char_past_end() {
        let mut editor = TestEditor::new("hello");
        editor.cursor = (0, 5);
        let original = editor.replace_char_at_cursor('X');
        assert_eq!(original, None);
        assert_eq!(editor.text(), "helloX");
        assert_eq!(editor.cursor.1, 6);
    }

    #[test]
    fn test_replace_char_on_empty_line() {
        let mut editor = TestEditor::new("");
        editor.cursor = (0, 0);
        let original = editor.replace_char_at_cursor('X');
        assert_eq!(original, None);
        assert_eq!(editor.text(), "X");
    }

    #[test]
    fn test_restore_char_at_cursor() {
        let mut editor = TestEditor::new("hXllo");
        editor.cursor = (0, 2);
        editor.restore_char_at_cursor('e');
        assert_eq!(editor.text(), "hello");
        assert_eq!(editor.cursor.1, 1);
    }

    // Tests for yank_lines with count
    #[test]
    fn test_yank_lines_single() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (0, 0);
        editor.yank_lines(1);
        assert_eq!(editor.yank_buffer, "line1");
        assert!(editor.yank_is_linewise);
    }

    #[test]
    fn test_yank_lines_multiple() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (0, 0);
        editor.yank_lines(2);
        assert_eq!(editor.yank_buffer, "line1\nline2");
        assert!(editor.yank_is_linewise);
    }

    #[test]
    fn test_yank_lines_exceeds_buffer() {
        let mut editor = TestEditor::new("line1\nline2");
        editor.cursor = (0, 0);
        editor.yank_lines(5);
        assert_eq!(editor.yank_buffer, "line1\nline2");
        assert!(editor.yank_is_linewise);
    }

    // Tests for is_insert_like_mode
    #[test]
    fn test_is_insert_like_mode_normal() {
        let editor = TestEditor::new("test");
        assert!(!editor.is_insert_like_mode());
    }

    #[test]
    fn test_is_insert_like_mode_insert() {
        let mut editor = TestEditor::new("test");
        editor.mode = EditorMode::Insert;
        assert!(editor.is_insert_like_mode());
    }

    #[test]
    fn test_is_insert_like_mode_replace() {
        let mut editor = TestEditor::new("test");
        editor.mode = EditorMode::Replace;
        assert!(editor.is_insert_like_mode());
    }

    // Tests for direction enum
    #[test]
    fn test_direction_opposite() {
        assert_eq!(Direction::Forward.opposite(), Direction::Backward);
        assert_eq!(Direction::Backward.opposite(), Direction::Forward);
    }

    // Tests for substitute_lines (cc)
    #[test]
    fn test_substitute_lines_single() {
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.cursor = (1, 0);
        editor.substitute_lines(1);
        assert_eq!(editor.text(), "line1\n\nline3");
        assert_eq!(editor.yank_buffer, "line2");
        assert!(editor.yank_is_linewise);
        assert_eq!(editor.mode, EditorMode::Insert);
    }

    #[test]
    fn test_substitute_lines_multiple() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4");
        editor.cursor = (1, 0);
        editor.substitute_lines(2);
        assert_eq!(editor.text(), "line1\n\nline4");
        assert_eq!(editor.yank_buffer, "line2\nline3");
        assert!(editor.yank_is_linewise);
    }

    #[test]
    fn test_substitute_lines_exceeds_buffer() {
        let mut editor = TestEditor::new("line1\nline2");
        editor.cursor = (0, 0);
        editor.substitute_lines(5);
        assert_eq!(editor.text(), "");
        assert_eq!(editor.yank_buffer, "line1\nline2");
    }

    // Tests for delete_lines count
    #[test]
    fn test_delete_lines_multiple() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4");
        editor.cursor = (1, 0);
        editor.delete_lines(2);
        assert_eq!(editor.text(), "line1\nline4");
        assert_eq!(editor.yank_buffer, "line2\nline3");
    }

    #[test]
    fn test_delete_lines_exceeds_buffer() {
        let mut editor = TestEditor::new("line1\nline2");
        editor.cursor = (0, 0);
        editor.delete_lines(5);
        // Should delete both lines and keep empty buffer
        assert_eq!(editor.text(), "");
    }

    // Tests for delete_chars count
    #[test]
    fn test_delete_chars_multiple() {
        let mut editor = TestEditor::new("hello world");
        editor.cursor = (0, 0);
        editor.delete_chars(5);
        assert_eq!(editor.text(), " world");
    }

    #[test]
    fn test_delete_chars_exceeds_line() {
        let mut editor = TestEditor::new("hi");
        editor.cursor = (0, 0);
        editor.delete_chars(10);
        assert_eq!(editor.text(), "");
    }

    // ============ X (Delete Char Backward) Tests ============

    #[test]
    fn test_delete_char_backward_basic() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 3);
        editor.delete_char_backward();
        assert_eq!(editor.text(), "helo");
        assert_eq!(editor.cursor.1, 2);
    }

    #[test]
    fn test_delete_char_backward_at_beginning() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 0);
        editor.delete_char_backward();
        assert_eq!(editor.text(), "hello"); // No change
        assert_eq!(editor.cursor.1, 0);
    }

    #[test]
    fn test_delete_char_backward_at_end() {
        // In normal mode, cursor max is len-1 (position 4 for "hello")
        let mut editor = TestEditor::new("hello").with_cursor(0, 4);
        editor.delete_char_backward();
        assert_eq!(editor.text(), "helo");
        assert_eq!(editor.cursor.1, 3);
    }

    #[test]
    fn test_delete_char_backward_with_count() {
        let mut editor = TestEditor::new("hello").with_cursor(0, 4);
        // Simulate 3X
        for _ in 0..3 {
            editor.delete_char_backward();
        }
        assert_eq!(editor.text(), "ho");
        assert_eq!(editor.cursor.1, 1);
    }

    #[test]
    fn test_delete_char_backward_unicode() {
        let mut editor = TestEditor::new("日本語").with_cursor(0, 2);
        editor.delete_char_backward();
        assert_eq!(editor.text(), "日語");
        assert_eq!(editor.cursor.1, 1);
    }

    // ============ H/M/L (Screen Position) Tests ============

    #[test]
    fn test_move_to_screen_top() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(5)
            .with_cursor(2, 0);
        editor.move_to_screen_position(ScreenPosition::Top(1));
        assert_eq!(editor.cursor.0, 0);
    }

    #[test]
    fn test_move_to_screen_top_with_count() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(5)
            .with_cursor(0, 0);
        editor.move_to_screen_position(ScreenPosition::Top(3));
        assert_eq!(editor.cursor.0, 2); // 3rd line from top (0-indexed: 2)
    }

    #[test]
    fn test_move_to_screen_middle() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(5)
            .with_cursor(0, 0);
        editor.move_to_screen_position(ScreenPosition::Middle);
        assert_eq!(editor.cursor.0, 2); // Middle of 5 lines
    }

    #[test]
    fn test_move_to_screen_bottom() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(5)
            .with_cursor(0, 0);
        editor.move_to_screen_position(ScreenPosition::Bottom(1));
        assert_eq!(editor.cursor.0, 4); // Last visible line
    }

    #[test]
    fn test_move_to_screen_bottom_with_count() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(5)
            .with_cursor(0, 0);
        editor.move_to_screen_position(ScreenPosition::Bottom(2));
        assert_eq!(editor.cursor.0, 3); // 2nd from bottom
    }

    #[test]
    fn test_move_to_screen_with_viewport_offset() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5\nline6\nline7")
            .with_screen_height(3)
            .with_cursor(0, 0);
        editor.viewport_top = 2; // Viewport shows lines 2, 3, 4

        editor.move_to_screen_position(ScreenPosition::Top(1));
        assert_eq!(editor.cursor.0, 2);

        editor.move_to_screen_position(ScreenPosition::Middle);
        assert_eq!(editor.cursor.0, 3);

        editor.move_to_screen_position(ScreenPosition::Bottom(1));
        assert_eq!(editor.cursor.0, 4);
    }

    #[test]
    fn test_move_to_screen_fewer_lines_than_height() {
        let mut editor = TestEditor::new("line1\nline2")
            .with_screen_height(10)
            .with_cursor(0, 0);

        editor.move_to_screen_position(ScreenPosition::Bottom(1));
        assert_eq!(editor.cursor.0, 1); // Last line even though screen is larger
    }

    // ============ Ctrl-E/Ctrl-Y (Scroll) Tests ============

    #[test]
    fn test_scroll_down_basic() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(3)
            .with_cursor(0, 0);
        editor.scroll_down(1);
        assert_eq!(editor.viewport_top, 1);
        assert_eq!(editor.cursor.0, 1); // Cursor moved to stay visible
    }

    #[test]
    fn test_scroll_down_cursor_stays_visible() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(3)
            .with_cursor(2, 0);
        editor.scroll_down(1);
        assert_eq!(editor.viewport_top, 1);
        assert_eq!(editor.cursor.0, 2); // Cursor stays on same line (still visible)
    }

    #[test]
    fn test_scroll_down_with_count() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(3)
            .with_cursor(0, 0);
        editor.scroll_down(3);
        assert_eq!(editor.viewport_top, 3);
        assert_eq!(editor.cursor.0, 3); // Cursor moved to viewport_top
    }

    #[test]
    fn test_scroll_down_clamps_to_end() {
        let mut editor = TestEditor::new("line1\nline2\nline3")
            .with_screen_height(3)
            .with_cursor(0, 0);
        editor.scroll_down(10);
        assert_eq!(editor.viewport_top, 2); // Clamped to last line
    }

    #[test]
    fn test_scroll_up_basic() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(3)
            .with_cursor(4, 0);
        editor.viewport_top = 2;
        editor.scroll_up(1);
        assert_eq!(editor.viewport_top, 1);
        assert_eq!(editor.cursor.0, 3); // Cursor adjusted to stay visible
    }

    #[test]
    fn test_scroll_up_cursor_stays_visible() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(3)
            .with_cursor(2, 0);
        editor.viewport_top = 2;
        editor.scroll_up(1);
        assert_eq!(editor.viewport_top, 1);
        assert_eq!(editor.cursor.0, 2); // Cursor stays on same line
    }

    #[test]
    fn test_scroll_up_clamps_to_start() {
        let mut editor = TestEditor::new("line1\nline2\nline3")
            .with_screen_height(3)
            .with_cursor(0, 0);
        editor.viewport_top = 1;
        editor.scroll_up(10);
        assert_eq!(editor.viewport_top, 0); // Clamped to start
    }

    #[test]
    fn test_scroll_up_with_count() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(3)
            .with_cursor(4, 0);
        editor.viewport_top = 4;
        editor.scroll_up(3);
        assert_eq!(editor.viewport_top, 1);
        assert_eq!(editor.cursor.0, 3); // Cursor adjusted to last visible
    }

    // ============ Ctrl-D/Ctrl-U (Half Page Scroll) Tests ============

    #[test]
    fn test_scroll_half_page_down_basic() {
        // With screen_height=10, half_page=5
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12",
        )
        .with_screen_height(10)
        .with_cursor(0, 0);
        editor.scroll_half_page_down(1);
        assert_eq!(editor.viewport_top, 5);
        assert_eq!(editor.cursor.0, 5);
    }

    #[test]
    fn test_scroll_half_page_down_with_count() {
        // With screen_height=10, half_page=5, count=2 means move 10 lines
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15",
        )
        .with_screen_height(10)
        .with_cursor(0, 0);
        editor.scroll_half_page_down(2);
        assert_eq!(editor.viewport_top, 10);
        assert_eq!(editor.cursor.0, 10);
    }

    #[test]
    fn test_scroll_half_page_down_clamps_to_end() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(10)
            .with_cursor(0, 0);
        editor.scroll_half_page_down(1);
        // Should clamp to last line (4)
        assert_eq!(editor.viewport_top, 4);
        assert_eq!(editor.cursor.0, 4);
    }

    #[test]
    fn test_scroll_half_page_down_from_middle() {
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12",
        )
        .with_screen_height(10)
        .with_cursor(3, 0);
        editor.viewport_top = 2;
        editor.scroll_half_page_down(1);
        assert_eq!(editor.viewport_top, 7);
        assert_eq!(editor.cursor.0, 8);
    }

    #[test]
    fn test_scroll_half_page_up_basic() {
        // With screen_height=10, half_page=5
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12",
        )
        .with_screen_height(10)
        .with_cursor(10, 0);
        editor.viewport_top = 8;
        editor.scroll_half_page_up(1);
        assert_eq!(editor.viewport_top, 3);
        assert_eq!(editor.cursor.0, 5);
    }

    #[test]
    fn test_scroll_half_page_up_with_count() {
        // With screen_height=10, half_page=5, count=2 means move 10 lines
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15",
        )
        .with_screen_height(10)
        .with_cursor(12, 0);
        editor.viewport_top = 10;
        editor.scroll_half_page_up(2);
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 2);
    }

    #[test]
    fn test_scroll_half_page_up_clamps_to_start() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(10)
            .with_cursor(2, 0);
        editor.viewport_top = 1;
        editor.scroll_half_page_up(1);
        // Should clamp to first line (0)
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 0);
    }

    #[test]
    fn test_scroll_half_page_up_from_middle() {
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12",
        )
        .with_screen_height(10)
        .with_cursor(8, 0);
        editor.viewport_top = 5;
        editor.scroll_half_page_up(1);
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 3);
    }

    #[test]
    fn test_scroll_half_page_down_preserves_column() {
        let mut editor = TestEditor::new(
            "hello world\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\ntest line",
        )
        .with_screen_height(10)
        .with_cursor(0, 6);
        editor.scroll_half_page_down(1);
        assert_eq!(editor.cursor.0, 5);
        // Column should be clamped to line length
        assert!(editor.cursor.1 <= 4); // "line6" has 5 chars
    }

    #[test]
    fn test_scroll_half_page_small_screen() {
        // With screen_height=2, half_page=1
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(2)
            .with_cursor(0, 0);
        editor.scroll_half_page_down(1);
        assert_eq!(editor.viewport_top, 1);
        assert_eq!(editor.cursor.0, 1);

        editor.scroll_half_page_up(1);
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 0);
    }

    // ============ zz/zt/zb (Scroll Cursor Position) Tests ============

    #[test]
    fn test_scroll_cursor_to_center_basic() {
        // With screen_height=10, cursor at line 10 should center viewport around line 10
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20",
        )
        .with_screen_height(10)
        .with_cursor(10, 0);
        editor.viewport_top = 0;
        editor.scroll_cursor_to_center();
        // With screen_height=10, half = 5, so viewport_top = 10 - 5 = 5
        assert_eq!(editor.viewport_top, 5);
        assert_eq!(editor.cursor.0, 10); // Cursor unchanged
    }

    #[test]
    fn test_scroll_cursor_to_center_at_start() {
        // Cursor at line 0 should set viewport_top to 0 (can't center above start)
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10",
        )
        .with_screen_height(10)
        .with_cursor(0, 0);
        editor.viewport_top = 5;
        editor.scroll_cursor_to_center();
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 0);
    }

    #[test]
    fn test_scroll_cursor_to_center_near_end() {
        // Cursor near end should still center as much as possible
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10",
        )
        .with_screen_height(10)
        .with_cursor(9, 0);
        editor.viewport_top = 0;
        editor.scroll_cursor_to_center();
        // With screen_height=10, half = 5, so viewport_top = 9 - 5 = 4
        assert_eq!(editor.viewport_top, 4);
        assert_eq!(editor.cursor.0, 9);
    }

    #[test]
    fn test_scroll_cursor_to_top_basic() {
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10",
        )
        .with_screen_height(10)
        .with_cursor(5, 0);
        editor.viewport_top = 0;
        editor.scroll_cursor_to_top();
        // Viewport should start at cursor line
        assert_eq!(editor.viewport_top, 5);
        assert_eq!(editor.cursor.0, 5);
    }

    #[test]
    fn test_scroll_cursor_to_top_at_start() {
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4\nline5")
            .with_screen_height(10)
            .with_cursor(0, 0);
        editor.viewport_top = 3;
        editor.scroll_cursor_to_top();
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 0);
    }

    #[test]
    fn test_scroll_cursor_to_top_clamps_to_max() {
        // When cursor is at line 9 (last line) and we scroll it to top,
        // viewport_top should be clamped to max (last line)
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10",
        )
        .with_screen_height(10)
        .with_cursor(9, 0);
        editor.viewport_top = 0;
        editor.scroll_cursor_to_top();
        assert_eq!(editor.viewport_top, 9);
        assert_eq!(editor.cursor.0, 9);
    }

    #[test]
    fn test_scroll_cursor_to_bottom_basic() {
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20",
        )
        .with_screen_height(10)
        .with_cursor(15, 0);
        editor.viewport_top = 15;
        editor.scroll_cursor_to_bottom();
        // With screen_height=10, cursor at 15 should set viewport_top = 15 - 9 = 6
        assert_eq!(editor.viewport_top, 6);
        assert_eq!(editor.cursor.0, 15);
    }

    #[test]
    fn test_scroll_cursor_to_bottom_at_start() {
        // Cursor at line 0, trying to scroll to bottom should set viewport_top to 0
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10",
        )
        .with_screen_height(10)
        .with_cursor(0, 0);
        editor.viewport_top = 5;
        editor.scroll_cursor_to_bottom();
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 0);
    }

    #[test]
    fn test_scroll_cursor_to_bottom_near_start() {
        // Cursor at line 3, with screen_height=10, should set viewport_top = 3 - 9 = 0 (clamped)
        let mut editor = TestEditor::new(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10",
        )
        .with_screen_height(10)
        .with_cursor(3, 0);
        editor.viewport_top = 5;
        editor.scroll_cursor_to_bottom();
        assert_eq!(editor.viewport_top, 0);
        assert_eq!(editor.cursor.0, 3);
    }

    // ============ Increment/Decrement Number Tests ============

    #[test]
    fn test_increment_number_basic() {
        let mut editor = TestEditor::new("value = 42").with_cursor(0, 8);
        editor.increment_number(1);
        assert_eq!(editor.text(), "value = 43");
    }

    #[test]
    fn test_decrement_number_basic() {
        let mut editor = TestEditor::new("value = 42").with_cursor(0, 8);
        editor.decrement_number(1);
        assert_eq!(editor.text(), "value = 41");
    }

    #[test]
    fn test_increment_number_with_count() {
        let mut editor = TestEditor::new("value = 10").with_cursor(0, 8);
        editor.increment_number(5);
        assert_eq!(editor.text(), "value = 15");
    }

    #[test]
    fn test_decrement_number_with_count() {
        let mut editor = TestEditor::new("value = 20").with_cursor(0, 8);
        editor.decrement_number(7);
        assert_eq!(editor.text(), "value = 13");
    }

    #[test]
    fn test_increment_number_large_count() {
        let mut editor = TestEditor::new("x = 0").with_cursor(0, 4);
        editor.increment_number(100);
        assert_eq!(editor.text(), "x = 100");
    }

    #[test]
    fn test_decrement_number_to_negative() {
        let mut editor = TestEditor::new("val = 5").with_cursor(0, 6);
        editor.decrement_number(10);
        assert_eq!(editor.text(), "val = -5");
    }

    #[test]
    fn test_increment_negative_number() {
        let mut editor = TestEditor::new("temp = -10").with_cursor(0, 7);
        editor.increment_number(3);
        assert_eq!(editor.text(), "temp = -7");
    }

    #[test]
    fn test_increment_negative_to_positive() {
        let mut editor = TestEditor::new("x = -5").with_cursor(0, 4);
        editor.increment_number(10);
        assert_eq!(editor.text(), "x = 5");
    }

    #[test]
    fn test_increment_hex_number() {
        let mut editor = TestEditor::new("addr = 0x0f").with_cursor(0, 9);
        editor.increment_number(1);
        assert_eq!(editor.text(), "addr = 0x10");
    }

    #[test]
    fn test_decrement_hex_number() {
        let mut editor = TestEditor::new("val = 0x10").with_cursor(0, 8);
        editor.decrement_number(1);
        assert_eq!(editor.text(), "val = 0x0f");
    }

    #[test]
    fn test_increment_hex_with_count() {
        let mut editor = TestEditor::new("x = 0x00").with_cursor(0, 6);
        editor.increment_number(16);
        assert_eq!(editor.text(), "x = 0x10");
    }

    #[test]
    fn test_increment_preserves_leading_zeros() {
        let mut editor = TestEditor::new("code = 007").with_cursor(0, 9);
        editor.increment_number(1);
        assert_eq!(editor.text(), "code = 008");
    }

    #[test]
    fn test_increment_cursor_on_number() {
        // Cursor anywhere on the number should work
        let mut editor = TestEditor::new("num = 123").with_cursor(0, 6);
        editor.increment_number(1);
        assert_eq!(editor.text(), "num = 124");
    }

    #[test]
    fn test_increment_finds_number_after_cursor() {
        // Cursor before the number should find it
        let mut editor = TestEditor::new("x = 42").with_cursor(0, 0);
        editor.increment_number(1);
        assert_eq!(editor.text(), "x = 43");
    }

    #[test]
    fn test_increment_no_number_no_change() {
        let mut editor = TestEditor::new("hello world").with_cursor(0, 0);
        editor.increment_number(1);
        assert_eq!(editor.text(), "hello world");
    }

    #[test]
    fn test_repeat_increment_with_same_count() {
        let mut editor = TestEditor::new("x = 10").with_cursor(0, 4);
        editor.increment_number(5);
        editor.set_last_change(LastChange::IncrementNumber, 5);
        assert_eq!(editor.text(), "x = 15");

        // Repeat without explicit count should use last count
        editor.cursor = (0, 4);
        editor.repeat_last_change();
        assert_eq!(editor.text(), "x = 20");
    }

    #[test]
    fn test_repeat_increment_with_new_count() {
        let mut editor = TestEditor::new("x = 10").with_cursor(0, 4);
        editor.increment_number(5);
        editor.set_last_change(LastChange::IncrementNumber, 5);
        assert_eq!(editor.text(), "x = 15");

        // Repeat with explicit count should use new count
        editor.cursor = (0, 4);
        editor.count_prefix = Some(3);
        editor.repeat_last_change();
        assert_eq!(editor.text(), "x = 18");
    }

    #[test]
    fn test_repeat_decrement_with_same_count() {
        let mut editor = TestEditor::new("x = 100").with_cursor(0, 4);
        editor.decrement_number(10);
        editor.set_last_change(LastChange::DecrementNumber, 10);
        assert_eq!(editor.text(), "x = 90");

        // Repeat without explicit count
        editor.cursor = (0, 4);
        editor.repeat_last_change();
        assert_eq!(editor.text(), "x = 80");
    }

    #[test]
    fn test_increment_cursor_position() {
        let mut editor = TestEditor::new("x = 9").with_cursor(0, 4);
        editor.increment_number(1);
        assert_eq!(editor.text(), "x = 10");
        // Cursor should be on the last digit
        assert_eq!(editor.cursor.1, 5);
    }

    #[test]
    fn test_decrement_cursor_position() {
        let mut editor = TestEditor::new("x = 10").with_cursor(0, 4);
        editor.decrement_number(1);
        assert_eq!(editor.text(), "x = 9");
        // Cursor should be on the last digit
        assert_eq!(editor.cursor.1, 4);
    }

    // ============ Visual Block Mode Tests ============

    #[test]
    fn test_visual_block_yank_single_column() {
        let mut editor = TestEditor::new("abc\ndef\nghi");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 1);
        editor.yank_visual_block();
        assert_eq!(editor.yank_buffer, "b\ne\nh");
        assert!(!editor.yank_is_linewise);
    }

    #[test]
    fn test_visual_block_yank_multiple_columns() {
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 2);
        editor.yank_visual_block();
        assert_eq!(editor.yank_buffer, "bc\nfg\njk");
    }

    #[test]
    fn test_visual_block_yank_with_short_lines() {
        let mut editor = TestEditor::new("abcdef\nab\nabcdef");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 2);
        editor.cursor = (2, 4);
        editor.yank_visual_block();
        // Second line is shorter, so it contributes empty string
        assert_eq!(editor.yank_buffer, "cde\n\ncde");
    }

    #[test]
    fn test_visual_block_delete_single_column() {
        let mut editor = TestEditor::new("abc\ndef\nghi");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 1);
        editor.delete_visual_block();
        assert_eq!(editor.text(), "ac\ndf\ngi");
        assert_eq!(editor.yank_buffer, "b\ne\nh");
    }

    #[test]
    fn test_visual_block_delete_multiple_columns() {
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 2);
        editor.delete_visual_block();
        assert_eq!(editor.text(), "ad\neh\nil");
        assert_eq!(editor.yank_buffer, "bc\nfg\njk");
    }

    #[test]
    fn test_visual_block_delete_with_short_lines() {
        let mut editor = TestEditor::new("abcdef\nab\nabcdef");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 2);
        editor.cursor = (2, 4);
        editor.delete_visual_block();
        // First line: abcdef -> abf (columns 2-4 removed)
        // Second line: ab -> ab (nothing in columns 2-4)
        // Third line: abcdef -> abf
        assert_eq!(editor.text(), "abf\nab\nabf");
    }

    #[test]
    fn test_visual_block_cursor_position_after_delete() {
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 2);
        editor.delete_visual_block();
        // Cursor should be at top-left of block
        assert_eq!(editor.cursor, (0, 1));
    }

    #[test]
    fn test_visual_block_reverse_selection() {
        // Selection from bottom-right to top-left
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (2, 2);
        editor.cursor = (0, 1);
        editor.yank_visual_block();
        assert_eq!(editor.yank_buffer, "bc\nfg\njk");
    }

    #[test]
    fn test_visual_block_single_line() {
        let mut editor = TestEditor::new("abcdef");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (0, 3);
        editor.yank_visual_block();
        assert_eq!(editor.yank_buffer, "bcd");
    }

    #[test]
    fn test_visual_block_delete_at_end_of_lines() {
        let mut editor = TestEditor::new("ab\ncd\nef");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 1);
        editor.delete_visual_block();
        assert_eq!(editor.text(), "a\nc\ne");
    }

    #[test]
    fn test_visual_block_swap_horizontal_end() {
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 3);

        // O swaps only column positions in block mode
        std::mem::swap(&mut editor.cursor.1, &mut editor.visual_start.1);

        // Now cursor column should be 1, visual_start column should be 3
        assert_eq!(editor.cursor, (2, 1));
        assert_eq!(editor.visual_start, (0, 3));

        // The block selection should still cover the same area
        editor.yank_visual_block();
        assert_eq!(editor.yank_buffer, "bcd\nfgh\njkl");
    }

    #[test]
    fn test_visual_block_o_swaps_diagonally() {
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 3);

        // o swaps entire positions
        std::mem::swap(&mut editor.cursor, &mut editor.visual_start);

        assert_eq!(editor.cursor, (0, 1));
        assert_eq!(editor.visual_start, (2, 3));
    }

    #[test]
    fn test_visual_block_paste_distributes_across_lines() {
        // User's scenario: yank 'sti' from all rows, paste at end of line 1
        // testing1\ntesting2\ntesting3
        // Select 'sti' (columns 2-4) in visual block mode from all rows
        // Then paste at end of testing1
        // Expected: testing1sti\ntesting2sti\ntesting3sti

        let mut editor = TestEditor::new("testing1\ntesting2\ntesting3");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 2);
        editor.cursor = (2, 4);
        editor.yank_visual_block();
        assert_eq!(editor.yank_buffer, "sti\nsti\nsti");
        assert!(editor.yank_is_block);

        // Now move cursor to end of first line (testing1 = 8 chars, last char at index 7)
        editor.mode = EditorMode::Normal;
        editor.cursor = (0, 7);

        // Paste after cursor (p command)
        editor.paste_block_after();

        assert_eq!(editor.text(), "testing1sti\ntesting2sti\ntesting3sti");
    }

    #[test]
    fn test_visual_block_paste_creates_lines_if_needed() {
        // Paste a 3-line block when there are only 2 lines
        let mut editor = TestEditor::new("line1\nline2");
        editor.yank_buffer = "aa\nbb\ncc".to_string();
        editor.yank_is_block = true;
        editor.yank_is_linewise = false;
        editor.cursor = (0, 4); // End of line1

        editor.paste_block_after();

        assert_eq!(editor.text(), "line1aa\nline2bb\ncc");
    }

    #[test]
    fn test_visual_block_paste_pads_with_spaces() {
        // Paste block at column beyond line length
        let mut editor = TestEditor::new("ab\ncd\nef");
        editor.yank_buffer = "x\ny\nz".to_string();
        editor.yank_is_block = true;
        editor.yank_is_linewise = false;
        editor.cursor = (0, 1); // After 'b' in "ab"

        editor.paste_block_after();

        assert_eq!(editor.text(), "abx\ncdy\nefz");
    }

    #[test]
    fn test_visual_block_delete_repeat() {
        // Test that . repeats a visual block delete
        let mut editor = TestEditor::new("abcd\nefgh\nijkl\nmnop");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (1, 2);

        // Delete block (columns 1-2, rows 0-1) -> "bc" and "fg"
        editor.delete_visual_block();
        // Set last change for repeat
        editor.last_change = LastChange::DeleteBlock(2, 2);

        assert_eq!(editor.text(), "ad\neh\nijkl\nmnop");

        // Now move cursor to row 2, col 1 and repeat
        editor.mode = EditorMode::Normal;
        editor.cursor = (2, 1);
        editor.delete_block_at_cursor(2, 2);

        // Should delete "jk" and "no" (columns 1-2, rows 2-3)
        assert_eq!(editor.text(), "ad\neh\nil\nmp");
    }

    #[test]
    fn test_visual_block_change_inserts_on_all_lines() {
        // User's scenario: visual block change should insert text on all affected lines
        // testing1\ntesting2\ntesting3
        // Select 'sti' (columns 2-4) and change to 'ABC'
        // Expected: teABCng1\nteABCng2\nteABCng3

        let mut editor = TestEditor::new("testing1\ntesting2\ntesting3");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 2);
        editor.cursor = (2, 4);

        // Simulate 'c' command: delete block and store block info
        let (start, end) = editor.get_visual_selection();
        let num_rows = end.0 - start.0 + 1;
        let insert_col = editor.visual_start.1.min(editor.cursor.1);

        // Delete the block (simulating delete_visual_selection_no_undo)
        editor.delete_visual_block();

        // First line should now be "teng1" (sti removed)
        // Second line: "teng2"
        // Third line: "teng3"
        assert_eq!(editor.text(), "teng1\nteng2\nteng3");

        // Now simulate typing "ABC" on first line (which happens during insert mode)
        let insert_text = "ABC";
        {
            let mut chars: Vec<char> = editor.lines[start.0].chars().collect();
            for (j, c) in insert_text.chars().enumerate() {
                chars.insert(insert_col + j, c);
            }
            editor.lines[start.0] = chars.into_iter().collect();
        }

        // Now simulate exiting insert mode - insert on remaining rows
        for i in 1..num_rows {
            let row = start.0 + i;
            if row >= editor.lines.len() {
                break;
            }
            let mut chars: Vec<char> = editor.lines[row].chars().collect();
            let pos = insert_col.min(chars.len());
            for (j, c) in insert_text.chars().enumerate() {
                chars.insert(pos + j, c);
            }
            editor.lines[row] = chars.into_iter().collect();
        }

        assert_eq!(editor.text(), "teABCng1\nteABCng2\nteABCng3");
    }

    #[test]
    fn test_visual_block_insert_i() {
        // Test I command: insert at left edge of block on all lines
        // abc\ndef\nghi -> select columns 1-2 on all rows -> press I -> type "X"
        // Expected: aXbc\ndXef\ngXhi

        let mut editor = TestEditor::new("abc\ndef\nghi");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 2);

        // Simulate 'I' command
        let (start, end) = editor.get_visual_selection();
        let num_rows = end.0 - start.0 + 1;
        let insert_col = editor.visual_start.1.min(editor.cursor.1); // left edge = 1

        // Simulate typing "X" on first line
        let insert_text = "X";
        {
            let mut chars: Vec<char> = editor.lines[start.0].chars().collect();
            for (j, c) in insert_text.chars().enumerate() {
                chars.insert(insert_col + j, c);
            }
            editor.lines[start.0] = chars.into_iter().collect();
        }

        // Simulate exiting insert mode - insert on remaining rows
        for i in 1..num_rows {
            let row = start.0 + i;
            let mut chars: Vec<char> = editor.lines[row].chars().collect();
            let pos = insert_col.min(chars.len());
            for (j, c) in insert_text.chars().enumerate() {
                chars.insert(pos + j, c);
            }
            editor.lines[row] = chars.into_iter().collect();
        }

        assert_eq!(editor.text(), "aXbc\ndXef\ngXhi");
    }

    #[test]
    fn test_visual_block_insert_skips_short_lines() {
        // Test that I command skips lines shorter than the selection's left edge
        // testing1\nte\ntesting3 -> select col 7 on all rows -> press I -> type "ABC"
        // Expected: testingABC1\nte\ntestingABC3 (middle line skipped)

        let mut editor = TestEditor::new("testing1\nte\ntesting3");
        editor.cursor = (0, 7); // cursor at col 7

        // Simulate the repeat of a block insert at col 7
        editor.insert_block_at_cursor(3, "ABC");

        // Line "te" (len=2) should be skipped because 2 < 7
        assert_eq!(editor.text(), "testingABC1\nte\ntestingABC3");
    }

    #[test]
    fn test_visual_block_append_pads_short_lines() {
        // Test that A command pads short lines with spaces instead of skipping
        // testing1\nte\ntesting3 -> select col 7 on all rows -> press A -> type "ABC"
        // Expected: testing1ABC\nte      ABC\ntesting3ABC (middle line padded)

        let mut editor = TestEditor::new("testing1\nte\ntesting3");
        editor.cursor = (0, 7); // cursor at col 7 (left edge of selection)

        // For A, offset = 1 (insert at col 8, which is col 7 + 1)
        editor.insert_block_at_cursor_with_offset(3, 1, "ABC");

        // Line "te" should be padded with spaces to col 8, then ABC inserted
        // "te" + 6 spaces = "te      " (8 chars), then "ABC" = "te      ABC"
        assert_eq!(editor.text(), "testing1ABC\nte      ABC\ntesting3ABC");
    }

    #[test]
    fn test_visual_block_append_a() {
        // Test A command: append at right edge of block on all lines
        // abc\ndef\nghi -> select columns 1-2 on all rows -> press A -> type "X"
        // Expected: abcX\ndefX\nghiX

        let mut editor = TestEditor::new("abc\ndef\nghi");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 1);
        editor.cursor = (2, 2);

        // Simulate 'A' command
        let (start, end) = editor.get_visual_selection();
        let num_rows = end.0 - start.0 + 1;
        let insert_col = editor.visual_start.1.max(editor.cursor.1) + 1; // right edge + 1 = 3

        // Simulate typing "X" on first line
        let insert_text = "X";
        {
            let mut chars: Vec<char> = editor.lines[start.0].chars().collect();
            // Pad if needed
            while chars.len() < insert_col {
                chars.push(' ');
            }
            for (j, c) in insert_text.chars().enumerate() {
                chars.insert(insert_col + j, c);
            }
            editor.lines[start.0] = chars.into_iter().collect();
        }

        // Simulate exiting insert mode - insert on remaining rows
        for i in 1..num_rows {
            let row = start.0 + i;
            let mut chars: Vec<char> = editor.lines[row].chars().collect();
            // Pad if needed
            while chars.len() < insert_col {
                chars.push(' ');
            }
            for (j, c) in insert_text.chars().enumerate() {
                chars.insert(insert_col + j, c);
            }
            editor.lines[row] = chars.into_iter().collect();
        }

        assert_eq!(editor.text(), "abcX\ndefX\nghiX");
    }

    #[test]
    fn test_visual_block_change_repeat() {
        // Test that . repeats a visual block change on all lines
        // Initial: "testing1\ntesting2\ntesting3"
        // Block select 'sti' (cols 2-4) on all lines, press c, type ABC, press Esc
        // Result: "teABCng1\nteABCng2\nteABCng3"
        // Move to row 0, col 5 and press . to repeat
        // Expected: "teABCABC1\nteABCABC2\nteABCABC3" (delete 3 chars at col 5-7, insert ABC)

        let mut editor = TestEditor::new("teABCng1\nteABCng2\nteABCng3");

        // Set up last change as if we had done a block change
        editor.last_change = LastChange::ChangeBlock(3, 3, "ABC".to_string());
        editor.last_count = 1;
        editor.cursor = (0, 5); // Position at 'n'

        // Repeat the change
        editor.change_block_at_cursor(3, 3, "ABC");

        // ng1, ng2, ng3 at cols 5-7 become ABC on each line
        assert_eq!(editor.text(), "teABCABC\nteABCABC\nteABCABC");
    }

    #[test]
    fn test_visual_block_insert_repeat() {
        // Test that . repeats a visual block insert on all lines
        // Initial: "abc\ndef\nghi"
        // Block select col 1 on all lines, press I, type XX, press Esc
        // Result: "aXXbc\ndXXef\ngXXhi"
        // Move to row 0, col 0 and press . to repeat
        // Expected: "XXaXXbc\nXXdXXef\nXXgXXhi"

        let mut editor = TestEditor::new("aXXbc\ndXXef\ngXXhi");

        // Set up last change as if we had done a block insert
        editor.last_change = LastChange::InsertBlock(3, "XX".to_string());
        editor.last_count = 1;
        editor.cursor = (0, 0);

        // Repeat the insert
        editor.insert_block_at_cursor(3, "XX");

        assert_eq!(editor.text(), "XXaXXbc\nXXdXXef\nXXgXXhi");
    }

    #[test]
    fn test_visual_block_append_repeat() {
        // Test that . repeats a visual block append on all lines at correct offset
        // User's scenario:
        // 1. Original text: "testing1\ntesting2\ntesting3"
        // 2. Cursor at 's' (col 2), visual block select 'sti' (cols 2-4) on all lines
        // 3. Press A, type "ABC", press Esc
        // 4. Result: "testiABCng1\ntestiABCng2\ntestiABCng3"
        // 5. Go to new text, cursor at 's' (col 2), press .
        // 6. Expected: same result (insert at col 2 + offset 3 = col 5)

        let mut editor = TestEditor::new("testing1\ntesting2\ntesting3");

        // Set up last change as if we had done a block append
        // Original selection: cols 2-4, so offset = (4 + 1) - 2 = 3
        editor.last_change = LastChange::AppendBlock(3, 3, "ABC".to_string());
        editor.last_count = 1;
        editor.cursor = (0, 2); // cursor at 's'

        // Repeat the append (insert at cursor + offset = col 5)
        editor.insert_block_at_cursor_with_offset(3, 3, "ABC");

        assert_eq!(editor.text(), "testiABCng1\ntestiABCng2\ntestiABCng3");
    }

    #[test]
    fn test_visual_block_undo_restores_cursor_to_top_left() {
        // Test that undo after visual block delete restores cursor to top-left of block
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        // Cursor at bottom-right (row 2, col 2), visual_start at top-left (row 0, col 1)
        editor.visual_start = (0, 1);
        editor.cursor = (2, 2);

        // Delete the block - function moves cursor to top-left before saving undo
        editor.delete_visual_block();

        // Verify deletion worked - columns 1-2 deleted from all rows
        assert_eq!(editor.text(), "ad\neh\nil");

        // Now undo
        editor.undo();

        // Text should be restored
        assert_eq!(editor.text(), "abcd\nefgh\nijkl");
        // Cursor should be at top-left of the block (row 0, col 1)
        assert_eq!(editor.cursor, (0, 1));
    }

    #[test]
    fn test_visual_block_undo_cursor_when_selecting_upward() {
        // Test undo when selection goes from bottom to top (visual_start at bottom)
        let mut editor = TestEditor::new("abcd\nefgh\nijkl");
        editor.mode = EditorMode::VisualBlock;
        // visual_start at bottom-right (row 2, col 2), cursor at top-left (row 0, col 1)
        editor.visual_start = (2, 2);
        editor.cursor = (0, 1);

        // Delete the block
        editor.delete_visual_block();

        assert_eq!(editor.text(), "ad\neh\nil");

        // Undo
        editor.undo();

        assert_eq!(editor.text(), "abcd\nefgh\nijkl");
        // Cursor should be at top-left (row 0, col 1)
        assert_eq!(editor.cursor, (0, 1));
    }

    #[test]
    fn test_visual_block_d_uppercase_deletes_to_end() {
        // Test D command: delete from left edge of block to end of line
        // testing1\ntesting2\ntesting3 -> select cols 4-5 on all rows -> press D
        // Expected: test\ntest\ntest (delete from col 4 to end)

        let mut editor = TestEditor::new("testing1\ntesting2\ntesting3");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 4);
        editor.cursor = (2, 5);

        // Simulate D command: delete from min_col (4) to end of each line
        let (start, end) = editor.get_visual_selection();
        let min_col = editor.visual_start.1.min(editor.cursor.1);

        editor.save_undo_state();
        for row in start.0..=end.0 {
            let chars: Vec<char> = editor.lines[row].chars().collect();
            if min_col < chars.len() {
                editor.lines[row] = chars[..min_col].iter().collect();
            }
        }

        assert_eq!(editor.text(), "test\ntest\ntest");
    }

    #[test]
    fn test_visual_block_c_uppercase_changes_to_end() {
        // Test C command: delete from left edge to end, then insert on all lines
        // testing1\ntesting2\ntesting3 -> select cols 4-5 on all rows -> press C -> type "XYZ"
        // Expected: testXYZ\ntestXYZ\ntestXYZ

        let mut editor = TestEditor::new("testing1\ntesting2\ntesting3");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 4);
        editor.cursor = (2, 5);

        // Simulate C command: delete from min_col (4) to end of each line
        let (start, end) = editor.get_visual_selection();
        let min_col = editor.visual_start.1.min(editor.cursor.1);
        let num_rows = end.0 - start.0 + 1;

        editor.save_undo_state();
        for row in start.0..=end.0 {
            let chars: Vec<char> = editor.lines[row].chars().collect();
            if min_col < chars.len() {
                editor.lines[row] = chars[..min_col].iter().collect();
            }
        }

        // Now simulate typing "XYZ" on first line
        let insert_text = "XYZ";
        editor.lines[start.0].push_str(insert_text);

        // Simulate exiting insert mode - insert on remaining rows
        for i in 1..num_rows {
            let row = start.0 + i;
            editor.lines[row].push_str(insert_text);
        }

        assert_eq!(editor.text(), "testXYZ\ntestXYZ\ntestXYZ");
    }

    // ============ Visual Mode (non-block) Tests ============

    #[test]
    fn test_visual_d_deletes_selection() {
        // Character-wise visual mode: d deletes selected text
        let mut editor = TestEditor::new("hello world");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (0, 0);
        editor.cursor = (0, 4); // Select "hello"
        editor.delete_visual_selection();
        assert_eq!(editor.text(), " world");
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_visual_line_d_deletes_lines() {
        // Line-wise visual mode: d deletes entire lines
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.mode = EditorMode::VisualLine;
        editor.visual_start = (0, 0);
        editor.cursor = (1, 0); // Select lines 1 and 2
        editor.delete_visual_selection();
        assert_eq!(editor.text(), "line3");
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_visual_d_uppercase_deletes_entire_lines() {
        // D in Visual mode deletes entire lines (linewise behavior)
        let mut editor = TestEditor::new("hello world\nfoo bar\nbaz qux");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (0, 3);
        editor.cursor = (1, 2); // Select partial text across two lines
        editor.delete_visual_lines();
        // D should delete entire lines 0 and 1, not just the selected text
        assert_eq!(editor.text(), "baz qux");
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_visual_line_d_uppercase_deletes_lines() {
        // D in VisualLine mode behaves like d
        let mut editor = TestEditor::new("line1\nline2\nline3\nline4");
        editor.mode = EditorMode::VisualLine;
        editor.visual_start = (1, 0);
        editor.cursor = (2, 0); // Select lines 2 and 3
        editor.delete_visual_lines();
        assert_eq!(editor.text(), "line1\nline4");
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_visual_c_uppercase_changes_entire_lines() {
        // C in Visual mode deletes entire lines and enters insert mode
        let mut editor = TestEditor::new("hello world\nfoo bar\nbaz qux");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (0, 3);
        editor.cursor = (1, 2); // Select partial text across two lines
        editor.change_visual_lines();
        // C should delete entire lines 0 and 1, leave empty line for insert
        assert_eq!(editor.text(), "\nbaz qux");
        assert_eq!(editor.mode, EditorMode::Insert);
        assert_eq!(editor.cursor, (0, 0));
    }

    #[test]
    fn test_visual_s_uppercase_changes_entire_lines() {
        // S in Visual mode deletes entire lines and enters insert mode (same as C)
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (0, 2);
        editor.cursor = (1, 3);
        editor.change_visual_lines();
        assert_eq!(editor.text(), "\nline3");
        assert_eq!(editor.mode, EditorMode::Insert);
    }

    #[test]
    fn test_visual_line_s_uppercase_changes_lines() {
        // S in VisualLine mode
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.mode = EditorMode::VisualLine;
        editor.visual_start = (0, 0);
        editor.cursor = (1, 0);
        editor.change_visual_lines();
        assert_eq!(editor.text(), "\nline3");
        assert_eq!(editor.mode, EditorMode::Insert);
    }

    #[test]
    fn test_visual_delete_undo_cursor_position() {
        // Undo after visual delete should restore cursor to start of selection
        let mut editor = TestEditor::new("hello world");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (0, 6); // Start at 'w'
        editor.cursor = (0, 10); // End at 'd' - select "world"
        editor.delete_visual_selection();
        assert_eq!(editor.text(), "hello ");

        // Undo should restore text and cursor to start of selection
        editor.undo();
        assert_eq!(editor.text(), "hello world");
        assert_eq!(editor.cursor, (0, 6)); // Cursor at start of selection
    }

    #[test]
    fn test_visual_lines_delete_undo_cursor_position() {
        // Undo after D (linewise delete) should restore cursor to start
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (0, 2);
        editor.cursor = (1, 3);
        editor.delete_visual_lines();
        assert_eq!(editor.text(), "line3");

        editor.undo();
        assert_eq!(editor.text(), "line1\nline2\nline3");
        assert_eq!(editor.cursor.0, 0); // Cursor on first line of selection
    }

    #[test]
    fn test_visual_lines_change_undo_cursor_position() {
        // Undo after C (linewise change) should restore cursor to start
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (1, 0);
        editor.cursor = (2, 0);
        editor.change_visual_lines();
        assert_eq!(editor.text(), "line1\n");
        assert_eq!(editor.mode, EditorMode::Insert);

        // Simulate exiting insert mode (which records the change)
        editor.mode = EditorMode::Normal;
        editor.record_change();
        editor.undo();
        assert_eq!(editor.text(), "line1\nline2\nline3");
        assert_eq!(editor.cursor.0, 1); // Cursor on first line of selection
    }

    #[test]
    fn test_visual_block_delete_to_eol() {
        // D in VisualBlock mode deletes from left edge to end of line
        let mut editor = TestEditor::new("testing1\ntesting2\ntesting3");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 4);
        editor.cursor = (2, 6);
        editor.delete_block_to_eol();
        assert_eq!(editor.text(), "test\ntest\ntest");
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_visual_block_change_to_eol() {
        // C in VisualBlock mode deletes from left edge to EOL and enters insert
        let mut editor = TestEditor::new("testing1\ntesting2\ntesting3");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 4);
        editor.cursor = (2, 6);
        editor.change_block_to_eol();
        assert_eq!(editor.text(), "test\ntest\ntest");
        assert_eq!(editor.mode, EditorMode::Insert);
        assert_eq!(editor.cursor, (0, 4)); // Cursor at insert position
    }

    #[test]
    fn test_visual_yank_buffer_is_linewise_for_d_uppercase() {
        // D in Visual mode should set yank_is_linewise = true
        let mut editor = TestEditor::new("line1\nline2\nline3");
        editor.mode = EditorMode::Visual;
        editor.visual_start = (0, 2);
        editor.cursor = (1, 3);
        editor.delete_visual_lines();
        assert!(editor.yank_is_linewise);
        assert!(!editor.yank_is_block);
        assert_eq!(editor.yank_buffer, "line1\nline2");
    }

    #[test]
    fn test_visual_block_to_eol_yank_is_block() {
        // D in VisualBlock mode should set yank_is_block = true
        let mut editor = TestEditor::new("testing1\ntesting2");
        editor.mode = EditorMode::VisualBlock;
        editor.visual_start = (0, 4);
        editor.cursor = (1, 6);
        editor.delete_block_to_eol();
        assert!(editor.yank_is_block);
        assert!(!editor.yank_is_linewise);
        assert_eq!(editor.yank_buffer, "ing1\ning2");
    }
}
