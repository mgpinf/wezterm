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
    selection_bg: ColorAttribute,
    selection_fg: ColorAttribute,
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
                .transient_context_label_fg
                .unwrap_or(AnsiColor::Grey.into())
                .into(),
            status_fg: colors.background.map_or(ColorAttribute::Default, |c| {
                ColorAttribute::TrueColorWithDefaultFallback(c.into())
            }),
            status_bg: colors
                .transient_entry_active_flag_fg
                .unwrap_or(AnsiColor::Purple.into())
                .into(),
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
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum EditorMode {
    Normal,
    Insert,
    Search,
    Visual,     // Character-wise visual selection (v)
    VisualLine, // Line-wise visual selection (V)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SearchDirection {
    Forward,
    Backward,
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
    Before,       // i - insert before cursor
    After,        // a - insert after cursor
    LineStart,    // I - insert at first non-blank of line
    LineEnd,      // A - insert at end of line
    NewLineBelow, // o - open new line below
    NewLineAbove, // O - open new line above
}

#[derive(Clone, Debug)]
enum LastChange {
    None,
    DeleteChar,                      // x
    DeleteLine,                      // dd
    DeleteWordMotion(WordType),      // dw, dW
    DeleteWordBackward(WordType),    // db, dB
    DeleteWordEnd(WordType),         // de, dE
    DeleteWordEndBackward(WordType), // dge, dgE
    DeleteToEndOfLine,               // D
    DeleteInnerWord,                 // diw
    DeleteAWord,                     // daw
    DeleteInnerLongWord,             // diW
    DeleteALongWord,                 // daW
    DeleteInnerPair(char),           // di( di{ etc.
    DeleteAroundPair(char),          // da( da{ etc.
    DeleteInnerParagraph,            // dip
    DeleteAParagraph,                // dap
    DeleteInnerSentence,             // dis
    DeleteASentence,                 // das
    DeleteToChar(char, bool),        // df{char}, dt{char} (inclusive flag)
    DeleteBackToChar(char, bool),    // dF{char}, dT{char} (inclusive flag)
    SubstituteLine,                  // S, cc
    SubstituteChar,                  // s
    ChangeToEndOfLine,               // C
    ChangeWordMotion(WordType),      // cw, cW
    ChangeWordBackward(WordType),    // cb, cB
    ChangeWordEnd(WordType),         // ce, cE
    ChangeWordEndBackward(WordType), // cge, cgE
    ChangeInnerWord,                 // ciw
    ChangeAWord,                     // caw
    ChangeInnerLongWord,             // ciW
    ChangeALongWord,                 // caW
    ChangeInnerPair(char),           // ci( ci{ etc.
    ChangeAroundPair(char),          // ca( ca{ etc.
    ChangeInnerParagraph,            // cip
    ChangeAParagraph,                // cap
    ChangeInnerSentence,             // cis
    ChangeASentence,                 // cas
    ChangeToChar(char, bool),        // cf{char}, ct{char}
    ChangeBackToChar(char, bool),    // cF{char}, cT{char}
    InsertText(String, InsertStyle), // Text inserted in insert mode
    ToggleCase,                      // ~
    JoinLines,                       // J
    ReplaceChar(char),               // r{char}
    IncrementNumber,                 // Ctrl-A
    DecrementNumber,                 // Ctrl-X
}

/// Information about a number found at cursor position
struct NumberAtCursor {
    /// Start column (includes negative sign if present)
    start: usize,
    /// End column (exclusive)
    end: usize,
    /// The digits of the number (without prefix or sign)
    digits: String,
    /// Whether this is a hexadecimal number
    is_hex: bool,
    /// Whether the number is negative
    is_negative: bool,
}

struct EditorState<'a> {
    args: &'a InputText,
    window: GuiWin,
    pane: MuxPane,
    lines: Vec<String>,
    lines_version: u64,     // Increments on any line modification
    history_version: u64,   // Version when last pushed to history
    cursor: (usize, usize), // row, col
    desired_col: usize,     // Desired column for vertical movement (Vim behavior)
    mode: EditorMode,
    colors: EditorColors,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
    history: Vec<(Vec<String>, (usize, usize))>,
    history_idx: usize,
    viewport_top: usize,
    pending_keys: Vec<KeyCode>,
    pending_operator: Option<char>, // 'd', 'c', 'y'
    last_change: LastChange,
    insert_buffer: String,     // Buffer to track text inserted in insert mode
    insert_style: InsertStyle, // Style of insert (i, a, I, A)
    last_char_search: Option<(char, char)>, // (search_type: f/F/t/T, character)
    yank_buffer: String,       // Buffer to store yanked text
    yank_is_linewise: bool,    // Whether the yank was linewise (yy, dG, etc.)
    search_pattern: String,    // Current search pattern
    search_direction: SearchDirection, // Current search direction
    search_input: String,      // Input buffer for search mode
    visual_start: (usize, usize), // Anchor point for visual selection (row, col)
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
            last_change: LastChange::None,
            insert_buffer: String::new(),
            insert_style: InsertStyle::Before,
            last_char_search: None,
            yank_buffer: String::new(),
            yank_is_linewise: false,
            search_pattern: String::new(),
            search_direction: SearchDirection::Forward,
            search_input: String::new(),
            visual_start: (0, 0),
        }
    }

    fn record_change(&mut self) {
        // Truncate redo history
        if self.history_idx < self.history.len() - 1 {
            self.history.truncate(self.history_idx + 1);
        }
        // Push a new entry when lines are modified (O(1) version check)
        if self.lines_version != self.history_version {
            self.history.push((self.lines.clone(), self.cursor));
            self.history_idx = self.history.len() - 1;
            self.history_version = self.lines_version;
        }
    }

    /// Record change only if not in Insert mode.
    /// For change operations (c, s, C, S, etc.), we defer recording until exiting insert mode.
    fn maybe_record_change(&mut self) {
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    /// Save current state before making changes (for undo to restore to)
    fn save_undo_state(&mut self) {
        // Truncate redo history
        if self.history_idx < self.history.len() - 1 {
            self.history.truncate(self.history_idx + 1);
        }
        // If lines haven't changed since last history push, just update cursor position
        // (cursor movements alone don't create new undo points)
        // O(1) comparison using version numbers instead of O(n) content comparison
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
            // Save cursor from current entry (where change started) - Vim behavior
            let cursor_at_change_start = self.history[self.history_idx].1;
            self.history_idx += 1;
            let (lines, _) = &self.history[self.history_idx];
            self.lines = lines.clone();
            // Use cursor from previous entry (start of change), not destination entry
            self.cursor = cursor_at_change_start;
            // Clamp cursor to valid position in case change start is past end of line
            self.clamp_cursor();
        }
    }

    fn move_cursor(&mut self, row: isize, col: isize) {
        let new_row = (self.cursor.0 as isize + row)
            .max(0)
            .min((self.lines.len() - 1) as isize) as usize;
        let line_len = self.lines[new_row].chars().count();
        let max_col = if self.mode == EditorMode::Insert {
            line_len
        } else {
            line_len.saturating_sub(1)
        };

        if col != 0 {
            // Horizontal movement - update desired_col to actual position
            let new_col = (self.cursor.1 as isize + col).max(0).min(max_col as isize) as usize;
            self.cursor = (new_row, new_col);
            self.desired_col = new_col;
        } else {
            // Vertical movement - try to reach desired_col
            let new_col = self.desired_col.min(max_col);
            self.cursor = (new_row, new_col);
        }
    }

    /// Update desired_col to current cursor position (call after explicit column changes)
    fn update_desired_col(&mut self) {
        self.desired_col = self.cursor.1;
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
        // Only record change if not in insert mode (batch insert mode changes)
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    fn delete_char(&mut self) {
        let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if !chars.is_empty() && self.cursor.1 < chars.len() {
            // Store deleted char in yank buffer
            self.yank_buffer = chars[self.cursor.1].to_string();
            self.yank_is_linewise = false;
            chars.remove(self.cursor.1);
            self.lines[self.cursor.0] = chars.into_iter().collect();
            self.clamp_cursor();
            self.update_desired_col();
            self.lines_version += 1;
            // Only record change if not in insert mode (batch insert mode changes)
            if self.mode != EditorMode::Insert {
                self.record_change();
            }
        }
    }

    fn insert_newline(&mut self) {
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
        // Only record change if not in insert mode (batch insert mode changes)
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    fn delete_line(&mut self) {
        // Save state before deletion for undo (preserves cursor position)
        self.save_undo_state();
        self.lines_version += 1;
        // Store deleted line in yank buffer
        self.yank_buffer = self.lines[self.cursor.0].clone();
        self.yank_is_linewise = true;
        if self.lines.len() > 1 {
            self.lines.remove(self.cursor.0);
            if self.cursor.0 >= self.lines.len() {
                self.cursor.0 = self.lines.len() - 1;
            }
            self.clamp_cursor();
            self.update_desired_col();
            self.record_change();
        } else {
            self.lines[0].clear();
            self.cursor.1 = 0;
            self.update_desired_col();
            self.record_change();
        }
    }

    fn delete_to_end_of_file(&mut self) {
        // Delete from current line to end of file (linewise)
        self.save_undo_state();
        // Store deleted lines in yank buffer
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
        // Preserve column position, clamp if line is shorter
        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn delete_to_start_of_file(&mut self) {
        // Delete from start of file to current line (linewise)
        self.save_undo_state();
        // Store deleted lines in yank buffer
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
        // Preserve column position, clamp if line is shorter
        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn delete_line_and_below(&mut self) {
        // Delete current line and line below (dj) - linewise
        if self.cursor.0 >= self.lines.len() - 1 {
            // No line below, just delete current line
            self.delete_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        // Store deleted lines in yank buffer
        let end_row = (self.cursor.0 + 1).min(self.lines.len() - 1);
        let yanked: Vec<&str> = self.lines[self.cursor.0..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the two lines
        self.lines.remove(self.cursor.0);
        if self.cursor.0 < self.lines.len() {
            self.lines.remove(self.cursor.0);
        }

        // Ensure at least one line exists
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        // Clamp cursor
        if self.cursor.0 >= self.lines.len() {
            self.cursor.0 = self.lines.len() - 1;
        }
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.update_desired_col();
        self.record_change();
    }

    fn delete_line_and_above(&mut self) {
        // Delete current line and line above (dk) - linewise
        if self.cursor.0 == 0 {
            // No line above, just delete current line
            self.delete_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        // Store deleted lines in yank buffer
        let start_row = self.cursor.0 - 1;
        let yanked: Vec<&str> = self.lines[start_row..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the two lines (remove upper first, then current which is now at start_row)
        self.lines.remove(start_row);
        self.lines.remove(start_row);

        // Ensure at least one line exists
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        // Move cursor up
        self.cursor.0 = start_row.min(self.lines.len() - 1);
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.update_desired_col();
        self.record_change();
    }

    fn change_line_and_below(&mut self) {
        // Change current line and line below (cj) - linewise, enter insert mode
        if self.cursor.0 >= self.lines.len() - 1 {
            // No line below, just substitute current line
            self.substitute_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        // Store deleted lines in yank buffer
        let end_row = (self.cursor.0 + 1).min(self.lines.len() - 1);
        let yanked: Vec<&str> = self.lines[self.cursor.0..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the two lines
        self.lines.remove(self.cursor.0);
        if self.cursor.0 < self.lines.len() {
            self.lines.remove(self.cursor.0);
        }

        // Insert blank line for typing
        self.lines.insert(self.cursor.0, String::new());
        self.cursor.1 = 0;
        self.update_desired_col();
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn change_line_and_above(&mut self) {
        // Change current line and line above (ck) - linewise, enter insert mode
        if self.cursor.0 == 0 {
            // No line above, just substitute current line
            self.substitute_line();
            return;
        }
        self.save_undo_state();
        self.lines_version += 1;

        // Store deleted lines in yank buffer
        let start_row = self.cursor.0 - 1;
        let yanked: Vec<&str> = self.lines[start_row..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the two lines
        self.lines.remove(start_row);
        self.lines.remove(start_row);

        // Insert blank line for typing at start_row
        self.lines.insert(start_row, String::new());
        self.cursor.0 = start_row;
        self.cursor.1 = 0;
        self.update_desired_col();
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn change_to_start_of_file(&mut self) {
        // Delete from start of file to current line, insert blank line for typing
        self.save_undo_state();
        self.lines_version += 1;
        for _ in 0..=self.cursor.0 {
            self.lines.remove(0);
        }
        // Insert blank line at the top for typing
        self.lines.insert(0, String::new());
        self.cursor.0 = 0;
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn change_to_end_of_file(&mut self) {
        // Delete from current line to end, leave blank line for typing
        self.save_undo_state();
        self.lines_version += 1;
        self.lines.truncate(self.cursor.0);
        self.lines.push(String::new());
        self.cursor.0 = self.lines.len() - 1;
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn yank_line(&mut self) {
        // Yank current line (linewise)
        self.yank_buffer = self.lines[self.cursor.0].clone();
        self.yank_is_linewise = true;
    }

    fn yank_to_end_of_file(&mut self) {
        // Yank from current line to end of file (linewise)
        let yanked: Vec<&str> = self.lines[self.cursor.0..]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
    }

    fn yank_to_start_of_file(&mut self) {
        // Yank from start of file to current line (linewise)
        let yanked: Vec<&str> = self.lines[..=self.cursor.0]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        // Move cursor to first line, maintaining column (like Neovim)
        self.cursor.0 = 0;
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn yank_to_end_of_line(&mut self) {
        // Yank from cursor to end of line (characterwise)
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 < chars.len() {
            self.yank_buffer = chars[self.cursor.1..].iter().collect();
        } else {
            self.yank_buffer.clear();
        }
        self.yank_is_linewise = false;
    }

    fn yank_line_and_below(&mut self) {
        // Yank current line and line below (yj) - linewise
        let end_row = (self.cursor.0 + 1).min(self.lines.len() - 1);
        let yanked: Vec<&str> = self.lines[self.cursor.0..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
    }

    fn yank_line_and_above(&mut self) {
        // Yank current line and line above (yk) - linewise
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
        // Move cursor to upper line (like Neovim)
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

    /// Get position of next word/WORD (w/W motion)
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
                    // Starting on whitespace: skip whitespace to find start of next word
                    while idx < chars.len() && chars[idx].is_whitespace() {
                        idx += 1;
                    }
                } else if start_char.is_ascii_punctuation() {
                    // Starting on punctuation: skip punctuation, then skip whitespace
                    while idx < chars.len() && chars[idx].is_ascii_punctuation() {
                        idx += 1;
                    }
                    while idx < chars.len() && chars[idx].is_whitespace() {
                        idx += 1;
                    }
                } else {
                    // Starting on word: skip word, then skip whitespace
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
                // Skip current non-whitespace, then skip whitespace
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

    /// Get position of previous word/WORD start (b/B motion)
    fn get_word_backward_pos(&self, word_type: WordType) -> (usize, usize) {
        // Helper to check if char is same type as reference char
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

        // Helper to find start of word on a line, given starting index
        let find_word_start = |chars: &[char], mut idx: usize, wt: WordType| -> usize {
            // Skip trailing whitespace
            while idx > 0 && chars[idx].is_whitespace() {
                idx -= 1;
            }
            if idx == 0 {
                return 0;
            }
            // Find start of word
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

        // Skip whitespace
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

    /// Get position of word/WORD end (e/E motion)
    fn get_word_end_pos(&self, word_type: WordType) -> (usize, usize) {
        // Helper to check if next char is same type as current
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
            // Skip whitespace
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

            // Find end of word
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

    /// Get position of end of previous word/WORD (ge/gE motion)
    fn get_word_end_backward_pos(&self, word_type: WordType) -> (usize, usize) {
        let mut row = self.cursor.0;
        let mut col = self.cursor.1;

        // Helper to get char type: 0 = whitespace, 1 = word, 2 = punct
        // For LongWord, only whitespace (0) vs non-whitespace (1) matters
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

        // Step 1: Move back one position
        if col > 0 {
            col -= 1;
        } else if row > 0 {
            row -= 1;
            col = self.lines[row].chars().count().saturating_sub(1);
        } else {
            return (0, 0);
        }

        // Step 2: Skip whitespace and empty lines backward
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

        // Step 3: Now we're on a non-whitespace char
        // Check if we started from a non-whitespace position in the same word
        let orig_char = get_char(self.cursor.0, self.cursor.1, &self.lines);
        let curr_char = get_char(row, col, &self.lines);

        if let (Some(orig_c), Some(curr_c)) = (orig_char, curr_char) {
            let orig_type = char_type(orig_c, word_type);
            let curr_type = char_type(curr_c, word_type);

            // If we started on a non-whitespace and are still on the same line
            // we need to check if we're in the same continuous word/WORD
            if orig_type != 0 && row == self.cursor.0 {
                let chars: Vec<char> = self.lines[row].chars().collect();
                let mut still_same_word = true;

                // Check characters between col and cursor for continuity
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
                    // Skip backward through this word/WORD entirely
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

                    // Now move back one more and skip whitespace again
                    if col > 0 {
                        col -= 1;
                    } else if row > 0 {
                        row -= 1;
                        col = self.lines[row].chars().count().saturating_sub(1);
                    } else {
                        return (0, 0);
                    }

                    // Skip whitespace again
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
        // Returns (start, end) of the word under cursor (not including surrounding whitespace)
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));

        // Determine the type of character under cursor
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

        // Find start of word
        let mut start = col;
        while start > 0 && char_type_matches(chars[start - 1]) {
            start -= 1;
        }

        // Find end of word
        let mut end = col;
        while end < chars.len() && char_type_matches(chars[end]) {
            end += 1;
        }

        (start, end)
    }

    fn get_a_word_bounds(&self) -> (usize, usize) {
        // Returns (start, end) of the word under cursor including trailing whitespace
        // (or leading whitespace if at end of line)
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let (word_start, word_end) = self.get_inner_word_bounds();

        // Try to include trailing whitespace first
        let mut end = word_end;
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        // If no trailing whitespace was found, try leading whitespace
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
            self.update_desired_col();
            self.maybe_record_change();
        }
    }

    /// Yank a character-wise range (potentially multi-line) into the yank buffer.
    /// Returns the yanked text. end_col is inclusive.
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

    /// Delete a character-wise range (potentially multi-line).
    /// end_col is inclusive. Returns after deletion with cursor positioned at start.
    fn delete_char_range(
        &mut self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) {
        if start_row == end_row {
            // Single line deletion
            let mut chars: Vec<char> = self.lines[start_row].chars().collect();
            let delete_end = (end_col + 1).min(chars.len());
            if start_col < delete_end {
                chars.drain(start_col..delete_end);
                self.lines[start_row] = chars.into_iter().collect();
            }
        } else {
            // Multi-line deletion
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

            // Remove lines between start and end
            for _ in start_row + 1..=end_row {
                if start_row + 1 < self.lines.len() {
                    self.lines.remove(start_row + 1);
                }
            }

            // Combine remaining parts
            self.lines[start_row] = before + &after;
        }

        // Ensure at least one line exists
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
        // Returns (start, end) of the WORD under cursor (whitespace-delimited)
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.1.min(chars.len().saturating_sub(1));

        // For WORD, only whitespace is a delimiter
        if chars[col].is_whitespace() {
            // Cursor is on whitespace, select the whitespace block
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
            // Cursor is on non-whitespace
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
        // Returns (start, end) of the WORD under cursor including trailing whitespace
        let line = &self.lines[self.cursor.0];
        if line.is_empty() {
            return (0, 0);
        }

        let chars: Vec<char> = line.chars().collect();
        let (word_start, word_end) = self.get_inner_long_word_bounds();

        // Try to include trailing whitespace first
        let mut end = word_end;
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        // If no trailing whitespace was found, try leading whitespace
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

    fn get_inner_paragraph_bounds(&self) -> (usize, usize) {
        // Find the bounds of the current paragraph (lines between blank lines)
        let mut start_row = self.cursor.0;
        let mut end_row = self.cursor.0;

        // If we're on a blank line, find all consecutive blank lines
        if self.lines[self.cursor.0].trim().is_empty() {
            // Find start of blank line group
            while start_row > 0 && self.lines[start_row - 1].trim().is_empty() {
                start_row -= 1;
            }
            // Find end of blank line group
            while end_row < self.lines.len() - 1 && self.lines[end_row + 1].trim().is_empty() {
                end_row += 1;
            }
            return (start_row, end_row);
        }

        // Find start of paragraph (first non-blank line after a blank line or start of file)
        while start_row > 0 && !self.lines[start_row - 1].trim().is_empty() {
            start_row -= 1;
        }

        // Find end of paragraph (last non-blank line before a blank line or end of file)
        while end_row < self.lines.len() - 1 && !self.lines[end_row + 1].trim().is_empty() {
            end_row += 1;
        }

        (start_row, end_row)
    }

    fn get_a_paragraph_bounds(&self) -> Option<(usize, usize)> {
        // Like inner paragraph, but includes blank lines
        // Vim behavior: include trailing blank lines if they exist,
        // otherwise include leading blank lines (for last paragraph)

        // Special case: if on a blank line, include blank lines + following paragraph
        if self.lines[self.cursor.0].trim().is_empty() {
            // Find start of blank line group
            let mut start_row = self.cursor.0;
            while start_row > 0 && self.lines[start_row - 1].trim().is_empty() {
                start_row -= 1;
            }
            // Find end of blank line group
            let mut end_row = self.cursor.0;
            while end_row < self.lines.len() - 1 && self.lines[end_row + 1].trim().is_empty() {
                end_row += 1;
            }

            // Check if there's a paragraph after the blank lines
            if end_row >= self.lines.len() - 1 {
                // No paragraph after, return None to indicate "do nothing"
                return None;
            }

            // Include the following paragraph
            end_row += 1; // Move to first line of next paragraph
            while end_row < self.lines.len() - 1 && !self.lines[end_row + 1].trim().is_empty() {
                end_row += 1;
            }

            return Some((start_row, end_row));
        }

        let (mut start_row, mut end_row) = self.get_inner_paragraph_bounds();

        // First, try to include trailing blank lines
        let original_end = end_row;
        while end_row < self.lines.len() - 1 && self.lines[end_row + 1].trim().is_empty() {
            end_row += 1;
        }

        // If no trailing blank lines were found, include leading blank lines instead
        if end_row == original_end {
            while start_row > 0 && self.lines[start_row - 1].trim().is_empty() {
                start_row -= 1;
            }
        }

        Some((start_row, end_row))
    }

    fn delete_inner_paragraph(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;
        let (start_row, end_row) = self.get_inner_paragraph_bounds();

        // Store deleted lines in yank buffer
        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the lines
        for _ in start_row..=end_row {
            self.lines.remove(start_row);
        }

        // Ensure at least one line exists
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        // Position cursor
        self.cursor.0 = start_row.min(self.lines.len() - 1);
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.clamp_cursor();
        self.update_desired_col();
        // For change operations (Insert mode), don't record yet
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    fn delete_a_paragraph(&mut self) {
        // Get bounds - returns None if on blank line with no following paragraph
        let Some((start_row, end_row)) = self.get_a_paragraph_bounds() else {
            return; // Do nothing
        };

        self.save_undo_state();
        self.lines_version += 1;

        // Store deleted lines in yank buffer
        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the lines
        for _ in start_row..=end_row {
            self.lines.remove(start_row);
        }

        // Ensure at least one line exists
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        // Position cursor
        self.cursor.0 = start_row.min(self.lines.len() - 1);
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.clamp_cursor();
        self.update_desired_col();
        // For change operations (Insert mode), don't record yet
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    fn yank_inner_paragraph(&mut self) {
        let (start_row, end_row) = self.get_inner_paragraph_bounds();
        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        // Move cursor to start of paragraph (like Neovim)
        self.cursor.0 = start_row;
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn yank_a_paragraph(&mut self) {
        // Get bounds - returns None if on blank line with no following paragraph
        let Some((start_row, end_row)) = self.get_a_paragraph_bounds() else {
            return; // Do nothing
        };

        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        // Move cursor to start of paragraph (like Neovim)
        self.cursor.0 = start_row;
        self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn change_inner_paragraph(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;
        let (start_row, end_row) = self.get_inner_paragraph_bounds();

        // Store deleted lines in yank buffer
        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the lines
        for _ in start_row..=end_row {
            self.lines.remove(start_row);
        }

        // Insert a blank line for typing
        self.lines.insert(start_row, String::new());

        // Position cursor on the blank line
        self.cursor.0 = start_row;
        self.cursor.1 = 0;
        self.update_desired_col();
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn change_a_paragraph(&mut self) {
        // Get bounds - returns None if on blank line with no following paragraph
        let Some((start_row, end_row)) = self.get_a_paragraph_bounds() else {
            return; // Do nothing
        };

        self.save_undo_state();
        self.lines_version += 1;

        // Store deleted lines in yank buffer
        let yanked: Vec<&str> = self.lines[start_row..=end_row]
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;

        // Remove the lines
        for _ in start_row..=end_row {
            self.lines.remove(start_row);
        }

        // Insert a blank line for typing
        self.lines.insert(start_row, String::new());

        // Position cursor on the blank line
        self.cursor.0 = start_row;
        self.cursor.1 = 0;
        self.update_desired_col();
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    /// Get the bounds of the current sentence (for `is` text object).
    /// Returns (start_row, start_col, end_row, end_col) where end is inclusive.
    fn get_inner_sentence_bounds(&self) -> (usize, usize, usize, usize) {
        // First, find the start of the current paragraph (don't cross blank lines)
        let para_start = {
            let mut row = self.cursor.0;
            while row > 0 && !self.lines[row - 1].trim().is_empty() {
                row -= 1;
            }
            row
        };

        // Find the start of the current sentence within the current paragraph
        let (mut start_row, mut start_col) =
            self.find_sentence_start_for_end(self.cursor.0, self.cursor.1);

        // Ensure start doesn't go before the paragraph boundary
        if start_row < para_start {
            start_row = para_start;
            start_col = self.find_line_start(start_row);
        }

        // If the found start is on a blank line, adjust to first non-blank line after it
        while start_row < self.lines.len() && self.lines[start_row].trim().is_empty() {
            start_row += 1;
            start_col = 0;
        }
        // Find first non-whitespace character on the start line
        if start_row < self.lines.len() {
            let chars: Vec<char> = self.lines[start_row].chars().collect();
            while start_col < chars.len() && chars[start_col].is_whitespace() {
                start_col += 1;
            }
        }

        // Find the end of the current sentence
        // Search forward from cursor for sentence-ending punctuation
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

            // Move to next line
            if end_row < self.lines.len() - 1 {
                // Check if next line is blank (paragraph boundary)
                if self.lines[end_row + 1].trim().is_empty() {
                    // End of paragraph - sentence ends at end of current line
                    let line_end = chars.len().saturating_sub(1);
                    return (start_row, start_col, end_row, line_end);
                }
                end_row += 1;
                end_col = 0;
            } else {
                // End of file - sentence ends at end of file
                let line_end = chars.len().saturating_sub(1);
                return (start_row, start_col, end_row, line_end);
            }
        }
    }

    /// Get the bounds of "a sentence" (for `as` text object).
    /// Like inner sentence but includes trailing whitespace (or leading if at end of paragraph).
    fn get_a_sentence_bounds(&self) -> (usize, usize, usize, usize) {
        let (start_row, start_col, end_row, end_col) = self.get_inner_sentence_bounds();

        // Try to include trailing whitespace first
        let mut new_end_row = end_row;
        let mut new_end_col = end_col;

        let chars: Vec<char> = self.lines[new_end_row].chars().collect();
        let mut next_col = new_end_col + 1;

        // Skip any trailing whitespace on the same line
        while next_col < chars.len() && chars[next_col].is_whitespace() {
            new_end_col = next_col;
            next_col += 1;
        }

        // If we found trailing whitespace or non-whitespace content after, return
        if new_end_col > end_col || next_col < chars.len() {
            return (start_row, start_col, new_end_row, new_end_col);
        }

        // Check next line for leading whitespace of next sentence
        if new_end_row < self.lines.len() - 1 && !self.lines[new_end_row + 1].trim().is_empty() {
            let next_chars: Vec<char> = self.lines[new_end_row + 1].chars().collect();
            if !next_chars.is_empty() && next_chars[0].is_whitespace() {
                // Include this line's whitespace
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

        // No trailing whitespace, try including leading whitespace instead
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

        // No whitespace to include, return inner bounds
        (start_row, start_col, end_row, end_col)
    }

    fn delete_inner_sentence(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;

        let (start_row, start_col, end_row, end_col) = self.get_inner_sentence_bounds();

        // Yank the sentence
        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);
        self.yank_is_linewise = false;

        // Delete the sentence
        self.delete_char_range(start_row, start_col, end_row, end_col);

        self.cursor = (start_row.min(self.lines.len() - 1), start_col);
        self.clamp_cursor();
        self.update_desired_col();
        self.maybe_record_change();
    }

    fn delete_a_sentence(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;

        let (start_row, start_col, end_row, end_col) = self.get_a_sentence_bounds();

        // Yank the sentence
        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);
        self.yank_is_linewise = false;

        // Delete the sentence
        self.delete_char_range(start_row, start_col, end_row, end_col);

        // If the line became empty after deletion and there's a line after it,
        // remove the empty line (like Neovim does for das)
        // But keep the empty line if it's the last line (Neovim behavior)
        if self.lines[start_row].is_empty() && start_row < self.lines.len() - 1 {
            self.lines.remove(start_row);
            // Cursor stays at start_row (now pointing to what was the next line)
            self.cursor = (start_row.min(self.lines.len() - 1), 0);
            // Find first non-whitespace on the new line
            let first_non_blank = self.get_first_non_blank_in_line(self.cursor.0);
            self.cursor.1 = first_non_blank;
        } else {
            self.cursor = (start_row.min(self.lines.len() - 1), start_col);
        }

        self.clamp_cursor();
        self.update_desired_col();
        self.maybe_record_change();
    }

    fn change_inner_sentence(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;

        let (start_row, start_col, end_row, end_col) = self.get_inner_sentence_bounds();

        // Yank and delete the sentence
        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);
        self.yank_is_linewise = false;
        self.delete_char_range(start_row, start_col, end_row, end_col);

        self.cursor = (start_row.min(self.lines.len() - 1), start_col);
        self.clamp_cursor();
        self.update_desired_col();
        self.mode = EditorMode::Insert;
    }

    fn change_a_sentence(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;

        let (start_row, start_col, end_row, end_col) = self.get_a_sentence_bounds();

        // Yank and delete the sentence
        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);
        self.yank_is_linewise = false;
        self.delete_char_range(start_row, start_col, end_row, end_col);

        self.cursor = (start_row.min(self.lines.len() - 1), start_col);
        self.clamp_cursor();
        self.update_desired_col();
        self.mode = EditorMode::Insert;
    }

    fn yank_inner_sentence(&mut self) {
        let (start_row, start_col, end_row, end_col) = self.get_inner_sentence_bounds();

        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);
        self.yank_is_linewise = false;

        // Move cursor to start of sentence
        self.cursor = (start_row, start_col);
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn yank_a_sentence(&mut self) {
        let (start_row, start_col, end_row, end_col) = self.get_a_sentence_bounds();

        self.yank_buffer = self.yank_char_range(start_row, start_col, end_row, end_col);

        // If sentence spans entire line (starts at col 0, ends at line end) and there's a next line,
        // treat as linewise. Last line of file is character-wise (no trailing newline).
        let end_line_len = self.lines[end_row].chars().count();
        self.yank_is_linewise =
            start_col == 0 && end_col + 1 >= end_line_len && end_row < self.lines.len() - 1;

        // Move cursor to start of sentence
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
        // Find matching pair around cursor (multi-line support)
        // Returns ((open_row, open_col), (close_row, close_col))
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
            // Quote-style pairs: only search on current line
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
            // Bracket-style pairs: multi-line search
            let cur_char = if !chars.is_empty() { chars[col] } else { ' ' };

            let open_pos: Option<(usize, usize)>;
            let close_pos: Option<(usize, usize)>;

            if cur_char == close {
                // Cursor is on closing bracket
                close_pos = Some((cur_row, col));
                open_pos = self.find_matching_open(open, close, cur_row, col);
            } else if cur_char == open {
                // Cursor is on opening bracket
                open_pos = Some((cur_row, col));
                close_pos = self.find_matching_close(open, close, cur_row, col);
            } else {
                // Cursor is inside - search both directions
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
            // Store content to be deleted in yank buffer (reuse yank logic)
            self.yank_inner_pair(pair_char);

            self.save_undo_state();
            self.lines_version += 1;
            if open_row == close_row {
                // Same line - simple case
                let line = &mut self.lines[open_row];
                if open_col + 1 < close_col {
                    line.replace_range((open_col + 1)..close_col, "");
                }
                // Cursor on closing bracket (now at open_col + 1)
                self.cursor.0 = open_row;
                self.cursor.1 = open_col + 1;
            } else {
                // Multi-line deletion - preserve line structure like Neovim
                let is_change = self.mode == EditorMode::Insert;
                let has_content_lines = close_row - open_row > 1;

                // Truncate first line after open bracket
                let first_line: String = self.lines[open_row].chars().take(open_col + 1).collect();
                self.lines[open_row] = first_line;

                // Truncate last line before close bracket
                let last_line: String = self.lines[close_row].chars().skip(close_col).collect();
                self.lines[close_row] = last_line;

                // Remove lines in between (but keep open_row and close_row)
                for _ in (open_row + 1)..close_row {
                    self.lines.remove(open_row + 1);
                }

                if is_change && has_content_lines {
                    // For ci( with content lines: insert empty line between brackets
                    self.lines.insert(open_row + 1, String::new());
                    self.cursor.0 = open_row + 1;
                    self.cursor.1 = 0;
                } else {
                    // For di( or ci( without content lines: cursor on closing bracket
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
            // Store content to be deleted in yank buffer (reuse yank logic)
            self.yank_around_pair(pair_char);

            self.save_undo_state();
            self.lines_version += 1;
            if open_row == close_row {
                // Same line - simple case
                let line = &mut self.lines[open_row];
                line.replace_range(open_col..=close_col, "");
                self.cursor.0 = open_row;
                self.cursor.1 = open_col;
            } else {
                // Multi-line deletion
                // Keep content before open bracket on first line
                let first_line_prefix: String =
                    self.lines[open_row].chars().take(open_col).collect();
                // Keep content after close bracket on last line
                let last_line_suffix: String =
                    self.lines[close_row].chars().skip(close_col + 1).collect();

                // Combine and replace
                self.lines[open_row] = first_line_prefix + &last_line_suffix;

                // Remove lines in between
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
        // Jump to previous unmatched opening bracket (multi-line)
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
        // Jump to next unmatched closing bracket (multi-line)
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

        // Check if cursor is on a matchable bracket (only (), [], {})
        if !Self::is_matchable_bracket(cur_char) {
            // Not on a bracket, search forward for one on current line
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

        // Determine if we're on an open or close bracket
        if Self::is_open_pair(cur_char) {
            // Search forward for matching close (multi-line)
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
            // On closing bracket - search backward for matching open (multi-line)
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

    /// Get the position of the previous paragraph boundary without moving cursor.
    fn get_paragraph_backward_pos(&self) -> (usize, usize) {
        let mut row = self.cursor.0;

        // Skip up past any blank lines we're currently on
        while row > 0 && self.lines[row].trim().is_empty() {
            row -= 1;
        }

        // Skip up past non-blank lines (the paragraph content)
        while row > 0 && !self.lines[row].trim().is_empty() {
            row -= 1;
        }

        // Now row is either on a blank line or at 0
        (row, 0)
    }

    /// Get the position of the next paragraph boundary without moving cursor.
    fn get_paragraph_forward_pos(&self) -> (usize, usize) {
        let mut row = self.cursor.0;
        let last_row = self.lines.len().saturating_sub(1);

        // Skip current blank lines (if any)
        while row < last_row && self.lines[row].trim().is_empty() {
            row += 1;
        }
        // Skip non-blank lines to find the next blank line
        while row < last_row && !self.lines[row].trim().is_empty() {
            row += 1;
        }

        // If we're on the last row and it's not blank, return position past end for exclusive motions
        if row == last_row && !self.lines[row].trim().is_empty() {
            (row, self.lines[row].chars().count())
        } else {
            (row, 0)
        }
    }

    fn move_paragraph_backward(&mut self) {
        // Move to previous paragraph boundary (blank line or start of file)
        let mut row = self.cursor.0;

        // Skip up past any blank lines we're currently on
        while row > 0 && self.lines[row].trim().is_empty() {
            row -= 1;
        }

        // Skip up past non-blank lines (the paragraph content)
        while row > 0 && !self.lines[row].trim().is_empty() {
            row -= 1;
        }

        // Now row is either on a blank line or at 0
        self.cursor.0 = row;
        self.cursor.1 = 0;
        self.update_desired_col();
    }

    fn move_paragraph_forward(&mut self) {
        // Move to next paragraph boundary (next blank line or end of file)
        let mut row = self.cursor.0;
        let last_row = self.lines.len().saturating_sub(1);

        // Skip current blank lines (if any)
        while row < last_row && self.lines[row].trim().is_empty() {
            row += 1;
        }
        // Skip non-blank lines to find the next blank line
        while row < last_row && !self.lines[row].trim().is_empty() {
            row += 1;
        }

        self.cursor.0 = row;

        // If we're on the last row and it's not blank, go to the last character
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

    /// Check if a sentence ends at position col in the given chars.
    /// A sentence ends with '.', '!', or '?' optionally followed by closing chars,
    /// then whitespace or end of line.
    fn is_valid_sentence_end(chars: &[char], col: usize) -> bool {
        if col >= chars.len() || !Self::is_sentence_end_punct(chars[col]) {
            return false;
        }
        // Skip past any closing chars after the punctuation
        let mut after_col = col + 1;
        while after_col < chars.len() && Self::is_sentence_closing_char(chars[after_col]) {
            after_col += 1;
        }
        // Must be followed by whitespace or end of line
        after_col >= chars.len() || chars[after_col].is_whitespace()
    }

    /// Get the position of the previous sentence start without moving cursor.
    fn get_sentence_backward_pos(&self) -> (usize, usize) {
        self.compute_sentence_backward_pos(self.cursor.0, self.cursor.1)
    }

    /// Get the position of the next sentence start without moving cursor.
    fn get_sentence_forward_pos(&self) -> (usize, usize) {
        self.compute_sentence_forward_pos(self.cursor.0, self.cursor.1)
    }

    /// Compute the position of the previous sentence start from a given position.
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

    /// Compute the position of the next sentence start from a given position.
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

    /// Find sentence start after a given position (after sentence end punctuation).
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

    /// Find first non-whitespace of a line.
    fn find_line_start(&self, row: usize) -> usize {
        let chars: Vec<char> = self.lines[row].chars().collect();
        chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0)
    }

    /// Find the start of a sentence that ends at (end_row, end_col).
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
        // Use the unified sentence backward position logic
        let new_pos = self.get_sentence_backward_pos();
        self.cursor = new_pos;
        self.update_desired_col();
    }

    fn move_sentence_forward(&mut self) {
        // Use the unified sentence forward position logic
        let new_pos = self.get_sentence_forward_pos();
        self.cursor = new_pos;
        self.update_desired_col();
    }

    fn find_char_forward(&self, target: char) -> Option<usize> {
        // Find next occurrence of target char on current line
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
        // Find previous occurrence of target char on current line
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
        // f{char} - move to next occurrence
        if let Some(pos) = self.find_char_forward(target) {
            self.cursor.1 = pos;
        }
    }

    fn move_to_char_backward(&mut self, target: char) {
        // F{char} - move to previous occurrence
        if let Some(pos) = self.find_char_backward(target) {
            self.cursor.1 = pos;
        }
    }

    fn move_till_char_forward(&mut self, target: char) {
        // t{char} - move to just before next occurrence
        if let Some(pos) = self.find_char_forward(target) {
            if pos > 0 {
                self.cursor.1 = pos - 1;
            }
        }
    }

    fn move_till_char_backward(&mut self, target: char) {
        // T{char} - move to just after previous occurrence
        if let Some(pos) = self.find_char_backward(target) {
            self.cursor.1 = pos + 1;
        }
    }

    fn repeat_char_search(&mut self, reverse: bool) {
        // ; repeats last f/F/t/T, , repeats in opposite direction
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
                    // For t repeat, we need to move past the character we're before
                    // to find the next occurrence. Save position to restore if not found.
                    let original_pos = self.cursor.1;
                    let line_len = self.lines[self.cursor.0].len();
                    if self.cursor.1 + 1 < line_len {
                        self.cursor.1 += 1; // Move past current position
                        if self.find_char_forward(target).is_some() {
                            self.move_till_char_forward(target);
                        } else {
                            self.cursor.1 = original_pos; // Restore if not found
                        }
                    }
                }
                'T' => {
                    // For T repeat, we need to move before the character we're after
                    // Save position to restore if not found.
                    let original_pos = self.cursor.1;
                    if self.cursor.1 > 0 {
                        self.cursor.1 -= 1; // Move before current position
                        if self.find_char_backward(target).is_some() {
                            self.move_till_char_backward(target);
                        } else {
                            self.cursor.1 = original_pos; // Restore if not found
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn delete_to_char_forward(&mut self, target: char, inclusive: bool) {
        // df{char} or dt{char}
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
        // dF{char} or dT{char}
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
        // Delete from start position to end position (multi-line support)
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
            // Same line - store deleted text in yank buffer
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
            // Multi-line - store deleted text in yank buffer
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

        // Temporarily move cursor to bracket
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

        // Search for match
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
            // Exclusive - don't include the closing bracket
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
        match self.last_change.clone() {
            LastChange::None => {}
            LastChange::DeleteChar => self.delete_char(),
            LastChange::DeleteLine => self.delete_line(),
            LastChange::DeleteWordMotion(wt) => {
                self.perform_delete_motion(|s| s.get_word_forward_pos(wt), false, true, false);
            }
            LastChange::DeleteWordBackward(wt) => {
                self.perform_delete_motion(|s| s.get_word_backward_pos(wt), false, true, false);
            }
            LastChange::DeleteWordEnd(wt) => {
                self.perform_delete_motion(|s| s.get_word_end_pos(wt), true, true, false);
            }
            LastChange::DeleteWordEndBackward(wt) => {
                self.perform_delete_motion(|s| s.get_word_end_backward_pos(wt), true, true, false);
            }
            LastChange::DeleteToEndOfLine => self.delete_to_end_of_line(),
            LastChange::DeleteInnerWord => self.delete_inner_word(),
            LastChange::DeleteAWord => self.delete_a_word(),
            LastChange::DeleteInnerLongWord => self.delete_inner_long_word(),
            LastChange::DeleteALongWord => self.delete_a_long_word(),
            LastChange::DeleteInnerPair(c) => self.delete_inner_pair(c),
            LastChange::DeleteAroundPair(c) => self.delete_around_pair(c),
            LastChange::DeleteInnerParagraph => self.delete_inner_paragraph(),
            LastChange::DeleteAParagraph => self.delete_a_paragraph(),
            LastChange::DeleteInnerSentence => self.delete_inner_sentence(),
            LastChange::DeleteASentence => self.delete_a_sentence(),
            LastChange::DeleteToChar(c, inclusive) => self.delete_to_char_forward(c, inclusive),
            LastChange::DeleteBackToChar(c, inclusive) => {
                self.delete_to_char_backward(c, inclusive)
            }
            LastChange::SubstituteLine => self.substitute_line(),
            LastChange::SubstituteChar => self.substitute_char(),
            LastChange::ChangeToEndOfLine => self.change_to_end_of_line(),
            LastChange::ChangeWordMotion(wt) => {
                self.mode = EditorMode::Insert;
                // cw behaves like ce when on a word
                let line = &self.lines[self.cursor.0];
                let chars: Vec<char> = line.chars().collect();
                let on_whitespace =
                    self.cursor.1 < chars.len() && chars[self.cursor.1].is_whitespace();
                if on_whitespace {
                    self.perform_delete_motion(|s| s.get_word_forward_pos(wt), false, false, false);
                } else {
                    self.perform_delete_motion(|s| s.get_word_end_pos(wt), true, false, false);
                }
                self.insert_saved_text();
            }
            LastChange::ChangeWordBackward(wt) => {
                self.mode = EditorMode::Insert;
                self.perform_delete_motion(|s| s.get_word_backward_pos(wt), false, false, false);
                self.insert_saved_text();
            }
            LastChange::ChangeWordEnd(wt) => {
                self.mode = EditorMode::Insert;
                self.perform_delete_motion(|s| s.get_word_end_pos(wt), true, false, false);
                self.insert_saved_text();
            }
            LastChange::ChangeWordEndBackward(wt) => {
                self.mode = EditorMode::Insert;
                self.perform_delete_motion(|s| s.get_word_end_backward_pos(wt), true, false, false);
                self.insert_saved_text();
            }
            LastChange::ChangeInnerWord => {
                self.mode = EditorMode::Insert;
                self.delete_inner_word();
                self.insert_saved_text();
            }
            LastChange::ChangeAWord => {
                self.mode = EditorMode::Insert;
                self.delete_a_word();
                self.insert_saved_text();
            }
            LastChange::ChangeInnerLongWord => {
                self.mode = EditorMode::Insert;
                self.delete_inner_long_word();
                self.insert_saved_text();
            }
            LastChange::ChangeALongWord => {
                self.mode = EditorMode::Insert;
                self.delete_a_long_word();
                self.insert_saved_text();
            }
            LastChange::ChangeInnerPair(c) => {
                self.mode = EditorMode::Insert;
                self.delete_inner_pair(c);
                self.insert_saved_text();
            }
            LastChange::ChangeAroundPair(c) => {
                self.mode = EditorMode::Insert;
                self.delete_around_pair(c);
                self.insert_saved_text();
            }
            LastChange::ChangeInnerParagraph => {
                self.change_inner_paragraph();
                self.insert_saved_text();
            }
            LastChange::ChangeAParagraph => {
                self.change_a_paragraph();
                self.insert_saved_text();
            }
            LastChange::ChangeInnerSentence => {
                self.change_inner_sentence();
                self.insert_saved_text();
            }
            LastChange::ChangeASentence => {
                self.change_a_sentence();
                self.insert_saved_text();
            }
            LastChange::ChangeToChar(c, inclusive) => {
                self.mode = EditorMode::Insert;
                self.delete_to_char_forward(c, inclusive);
                self.insert_saved_text();
            }
            LastChange::ChangeBackToChar(c, inclusive) => {
                self.mode = EditorMode::Insert;
                self.delete_to_char_backward(c, inclusive);
                self.insert_saved_text();
            }
            LastChange::InsertText(text, style) => {
                // Save undo state before making changes
                self.save_undo_state();
                // Temporarily enter Insert mode to batch changes
                self.mode = EditorMode::Insert;
                // Position cursor based on insert style
                match style {
                    InsertStyle::Before => {
                        // i - insert before cursor, no movement needed
                    }
                    InsertStyle::After => {
                        // a - insert after cursor
                        if self.cursor.1 < self.lines[self.cursor.0].len() {
                            self.cursor.1 += 1;
                        }
                    }
                    InsertStyle::LineStart => {
                        // I - insert at first non-blank of line
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
                        // A - insert at end of line
                        self.cursor.1 = self.lines[self.cursor.0].len();
                    }
                    InsertStyle::NewLineBelow => {
                        // o - open new line below current line
                        self.lines_version += 1;
                        self.lines.insert(self.cursor.0 + 1, String::new());
                        self.cursor.0 += 1;
                        self.cursor.1 = 0;
                    }
                    InsertStyle::NewLineAbove => {
                        // O - open new line above current line
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
                // Move cursor back like Escape does
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                }
                // Return to Normal mode and record final state
                self.mode = EditorMode::Normal;
                self.record_change();
            }
            LastChange::ToggleCase => self.toggle_case(),
            LastChange::JoinLines => self.join_lines(),
            LastChange::ReplaceChar(c) => self.replace_char(c),
            LastChange::IncrementNumber => self.increment_number(),
            LastChange::DecrementNumber => self.decrement_number(),
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
        self.save_undo_state(); // Save state before deletion
        self.lines_version += 1;
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 < chars.len() {
            // Store deleted text in yank buffer
            self.yank_buffer = chars[self.cursor.1..].iter().collect();
            self.yank_is_linewise = false;
            self.lines[self.cursor.0] = chars[..self.cursor.1].iter().collect();
        }
        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn change_to_end_of_line(&mut self) {
        // Save undo state ONCE (for the entire change operation)
        self.save_undo_state();
        self.lines_version += 1;

        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 < chars.len() {
            // Store deleted text in yank buffer
            self.yank_buffer = chars[self.cursor.1..].iter().collect();
            self.yank_is_linewise = false;
            self.lines[self.cursor.0] = chars[..self.cursor.1].iter().collect();
        }

        // Don't call record_change() here - it will be called when exiting insert mode
        self.mode = EditorMode::Insert;
        let line_len = self.lines[self.cursor.0].len();
        self.cursor.1 = line_len;
    }

    fn substitute_line(&mut self) {
        // Save undo state ONCE (for the entire change operation)
        self.save_undo_state();
        self.lines_version += 1;

        let line = &mut self.lines[self.cursor.0];
        let mut indent = String::new();
        for ch in line.chars() {
            if ch.is_whitespace() {
                indent.push(ch);
            } else {
                break;
            }
        }

        *line = indent;
        self.cursor.1 = line.len();
        self.mode = EditorMode::Insert;
        // Don't call record_change() here - it will be called when exiting insert mode
    }

    fn substitute_char(&mut self) {
        let line_len = self.lines[self.cursor.0].len();
        if line_len > 0 && self.cursor.1 < line_len {
            // Save undo state ONCE (for the entire change operation)
            self.save_undo_state();
            self.lines_version += 1;
            self.lines[self.cursor.0].remove(self.cursor.1);
            self.mode = EditorMode::Insert;
            // Don't call record_change() here - it will be called when exiting insert mode
        }
    }

    fn replace_char(&mut self, replacement: char) {
        // r{char} - replace character under cursor without entering insert mode
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

    /// Find a number under or after the cursor on the current line.
    fn find_number_at_cursor(&self) -> Option<NumberAtCursor> {
        let line = &self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return None;
        }

        let pos = self.cursor.1.min(chars.len() - 1);

        // Try to find a hex number first, then fall back to decimal
        self.try_find_hex_number(&chars, pos)
            .or_else(|| self.try_find_decimal_number(&chars, pos))
    }

    /// Try to find a hex number (0x...) at or after the given position
    fn try_find_hex_number(&self, chars: &[char], pos: usize) -> Option<NumberAtCursor> {
        // Find where the 0x prefix might be by looking backwards through hex digits
        let find_hex_start = |from: usize| -> Option<usize> {
            let mut i = from;
            while i > 0 && chars[i - 1].is_ascii_hexdigit() {
                i -= 1;
            }
            // Check for 0x prefix
            if i >= 2 && chars[i - 1].to_ascii_lowercase() == 'x' && chars[i - 2] == '0' {
                Some(i - 2)
            } else if i >= 1 && chars[i].to_ascii_lowercase() == 'x' && chars[i - 1] == '0' {
                Some(i - 1)
            } else {
                None
            }
        };

        // Check various cases where cursor might be on a hex number
        let hex_start = if chars[pos].is_ascii_hexdigit() {
            // On a hex digit (0-9, a-f, A-F)
            find_hex_start(pos)
        } else if chars[pos].to_ascii_lowercase() == 'x' && pos > 0 && chars[pos - 1] == '0' {
            // On 'x' of 0x
            Some(pos - 1)
        } else {
            None
        };

        let hex_start = hex_start?;

        // Find end of hex number
        let mut end = hex_start + 2; // Skip 0x
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

    /// Try to find a decimal number at or after the given position
    fn try_find_decimal_number(&self, chars: &[char], mut pos: usize) -> Option<NumberAtCursor> {
        let mut is_negative = false;

        // If not on a digit, search forward
        if !chars[pos].is_ascii_digit() {
            if chars[pos] == '-' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit() {
                is_negative = true;
                pos += 1;
            } else {
                // Search forward for a digit
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

        // Check if this digit is part of a hex number
        if self.try_find_hex_number(chars, pos).is_some() {
            return None; // Let hex handling take care of it
        }

        // Find start of decimal number
        let mut num_start = pos;
        while num_start > 0 && chars[num_start - 1].is_ascii_digit() {
            num_start -= 1;
        }

        // Check for negative sign
        if !is_negative && num_start > 0 && chars[num_start - 1] == '-' {
            is_negative = true;
        }

        // Find end of decimal number
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

    fn increment_number(&mut self) {
        self.modify_number(1);
    }

    fn decrement_number(&mut self) {
        self.modify_number(-1);
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
        self.update_desired_col();
        self.record_change();
    }

    /// Format a hex number after applying delta (unsigned wrapping)
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

    /// Format a decimal number after applying delta (signed)
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

    fn render(&mut self) -> termwiz::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        self.buf.add_changes(vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorVisibility(CursorVisibility::Hidden),
        ]);

        // Status bar
        let mode_text = match self.mode {
            EditorMode::Normal => " -- NORMAL --",
            EditorMode::Insert => " -- INSERT --",
            EditorMode::Search => "",
            EditorMode::Visual => " -- VISUAL --",
            EditorMode::VisualLine => " -- VISUAL LINE --",
        };

        // Build pending keys string (shown on the right like Neovim)
        let mut pending_str = String::new();
        if let Some(op) = self.pending_operator {
            pending_str.push(op);
        }
        for key in &self.pending_keys {
            if let KeyCode::Char(c) = key {
                pending_str.push(*c);
            }
        }

        let status_text = if self.mode == EditorMode::Search {
            let prompt = match self.search_direction {
                SearchDirection::Forward => "/",
                SearchDirection::Backward => "?",
            };
            format!("{}{}", prompt, self.search_input)
        } else {
            let position = format!("{}:{}", self.cursor.0 + 1, self.cursor.1 + 1);
            if pending_str.is_empty() {
                format!("{}  {}", mode_text, position)
            } else {
                // Calculate spacing: mode on left, pending keys on right
                let left_part = format!("{}  {}", mode_text, position);
                let right_part = &pending_str;
                let padding = cols.saturating_sub(left_part.len() + right_part.len() + 1);
                format!("{}{:>width$}{}", left_part, "", right_part, width = padding)
            }
        };

        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(rows - 1),
            },
            Change::Attribute(AttributeChange::Background(self.colors.status_bg)),
            Change::Attribute(AttributeChange::Foreground(self.colors.status_fg)),
            Change::Text(format!("{:<width$}", status_text, width = cols)),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        // Adjust viewport
        let content_rows = rows.saturating_sub(2); // Title + Status
        if self.cursor.0 < self.viewport_top {
            self.viewport_top = self.cursor.0;
        } else if self.cursor.0 >= self.viewport_top + content_rows {
            self.viewport_top = self.cursor.0 - content_rows + 1;
        }

        // Title
        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
            Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
            Change::Text(self.args.title.clone()),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        // Content
        // Calculate selection range if in visual mode
        let selection = if self.mode == EditorMode::Visual || self.mode == EditorMode::VisualLine {
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

        for i in 0..content_rows {
            let line_idx = self.viewport_top + i;
            if line_idx >= self.lines.len() {
                break;
            }

            // Line number
            self.buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(1 + i),
                },
                Change::Attribute(AttributeChange::Foreground(self.colors.line_number_fg)),
                Change::Text(format!("{:>3} ", line_idx + 1)),
                Change::AllAttributes(CellAttributes::default()),
            ]);

            // Line content with selection highlighting
            let line = &self.lines[line_idx];
            if let Some((sel_start, sel_end)) = selection {
                // Check if this line is part of the selection
                let line_in_selection = line_idx >= sel_start.0 && line_idx <= sel_end.0;

                if line_in_selection && self.mode == EditorMode::VisualLine {
                    // Entire line is selected in VisualLine mode
                    self.buf.add_changes(vec![
                        Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                        Change::Attribute(AttributeChange::Foreground(self.colors.selection_fg)),
                        Change::Text(if line.is_empty() {
                            " ".to_string()
                        } else {
                            line.clone()
                        }),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);
                } else if line_in_selection && self.mode == EditorMode::Visual {
                    // Character-wise selection
                    let chars: Vec<char> = line.chars().collect();
                    let line_len = chars.len();

                    // Handle empty lines - show a highlighted space like Neovim
                    if line_len == 0 {
                        self.buf.add_changes(vec![
                            Change::Attribute(AttributeChange::Background(
                                self.colors.selection_bg,
                            )),
                            Change::Attribute(AttributeChange::Foreground(
                                self.colors.selection_fg,
                            )),
                            Change::Text(" ".to_string()),
                            Change::AllAttributes(CellAttributes::default()),
                        ]);
                    } else {
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

                        // Text before selection
                        if sel_col_start > 0 {
                            let before: String =
                                chars[..sel_col_start.min(line_len)].iter().collect();
                            self.buf.add_changes(vec![
                                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                                Change::Text(before),
                            ]);
                        }

                        // Selected text
                        if sel_col_start < line_len {
                            let selected: String = chars
                                [sel_col_start.min(line_len)..sel_col_end.min(line_len)]
                                .iter()
                                .collect();
                            if !selected.is_empty() {
                                self.buf.add_changes(vec![
                                    Change::Attribute(AttributeChange::Background(
                                        self.colors.selection_bg,
                                    )),
                                    Change::Attribute(AttributeChange::Foreground(
                                        self.colors.selection_fg,
                                    )),
                                    Change::Text(selected),
                                    Change::AllAttributes(CellAttributes::default()),
                                ]);
                            }
                        }

                        // Text after selection
                        if sel_col_end < line_len {
                            let after: String = chars[sel_col_end..].iter().collect();
                            self.buf.add_changes(vec![
                                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                                Change::Text(after),
                            ]);
                        }

                        self.buf
                            .add_changes(vec![Change::AllAttributes(CellAttributes::default())]);
                    }
                } else {
                    // Not in selection range
                    self.buf.add_changes(vec![
                        Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                        Change::Text(line.clone()),
                    ]);
                }
            } else {
                // Not in visual mode
                self.buf.add_changes(vec![
                    Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                    Change::Text(line.clone()),
                ]);
            }
        }

        // Cursor
        let cursor_screen_y = 1 + (self.cursor.0 - self.viewport_top);
        let cursor_screen_x = 4 + self.cursor.1; // 4 for line number width

        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(cursor_screen_x),
                y: Position::Absolute(cursor_screen_y),
            },
            Change::CursorVisibility(CursorVisibility::Visible),
            Change::CursorShape(if self.pending_operator.is_some() {
                // Operator-pending mode (d, c, y waiting for motion)
                CursorShape::SteadyUnderline
            } else {
                match self.mode {
                    EditorMode::Normal => CursorShape::SteadyBlock,
                    EditorMode::Insert => CursorShape::SteadyBar,
                    EditorMode::Search => CursorShape::SteadyUnderline,
                    EditorMode::Visual | EditorMode::VisualLine => CursorShape::SteadyBlock,
                }
            }),
        ]);

        self.buf.flush()?;

        Ok(())
    }

    // Helper to perform delete action based on a motion
    // delete_empty_lines: if true, delete the entire line when backward motion would empty it
    // allow_linewise: if true, allow linewise deletion when cursor is at start of line (for sentence/paragraph motions)
    fn perform_delete_motion<F>(
        &mut self,
        motion: F,
        is_inclusive: bool,
        delete_empty_lines: bool,
        allow_linewise: bool,
    ) where
        F: Fn(&EditorState) -> (usize, usize),
    {
        // Record state before deletion for undo (preserves cursor position)
        self.save_undo_state();
        self.lines_version += 1;
        let start = self.cursor;
        let end = motion(self);

        // Handle direction - use character-based operations
        if end.0 < start.0 {
            // Backward motion crossing to previous line - need to handle multi-line deletion
            // Special case: if end.1 == 0, don't merge with end line, keep it separate

            let mut deleted_text = String::new();

            if end.1 == 0 && !delete_empty_lines {
                // Change operation landing at start of a line - preserve line structure
                // Keep line end.0 (clear it if it has content), put suffix on separate line

                // If line end.0 has content (first paragraph case), clear it and add to deleted text
                if !self.lines[end.0].trim().is_empty() {
                    deleted_text.push_str(&self.lines[end.0]);
                    deleted_text.push('\n');
                    self.lines[end.0] = String::new();
                }

                // Add content from line after end to deleted text
                for row in (end.0 + 1)..start.0 {
                    deleted_text.push_str(&self.lines[row]);
                    deleted_text.push('\n');
                }

                // Get the part to delete from start line (before cursor)
                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                deleted_text.push_str(&start_chars[..start.1].iter().collect::<String>());
                let start_suffix: String = start_chars[start.1..].iter().collect();

                // Store in yank buffer
                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                // Update start line to just the suffix
                self.lines[start.0] = start_suffix;

                // Remove intermediate lines (from end.0+1 to start.0-1)
                for _ in (end.0 + 1)..start.0 {
                    self.lines.remove(end.0 + 1);
                }

                // Move cursor to line end.0
                self.cursor.0 = end.0;
                self.cursor.1 = 0;
            } else {
                // Motion lands in middle of a line - merge start and end lines

                // Get the part to keep from end line (before end position)
                let end_chars: Vec<char> = self.lines[end.0].chars().collect();
                let end_prefix: String = end_chars[..end.1].iter().collect();
                deleted_text.push_str(&end_chars[end.1..].iter().collect::<String>());

                // Add intermediate lines to deleted text
                for row in (end.0 + 1)..start.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                // Get the part to keep from start line (at and after cursor)
                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                deleted_text.push('\n');
                deleted_text.push_str(&start_chars[..start.1].iter().collect::<String>());
                let start_suffix: String = start_chars[start.1..].iter().collect();

                // Store in yank buffer
                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                // Join the kept parts
                let new_line = format!("{}{}", end_prefix, start_suffix);

                // Remove lines from start.0 down to end.0+1, then update end.0
                for _ in end.0..start.0 {
                    self.lines.remove(end.0 + 1);
                }
                self.lines[end.0] = new_line;

                // Move cursor to the deletion point
                self.cursor.0 = end.0;
                self.cursor.1 = end.1;
            }
        } else if end.0 == start.0 && end.1 < start.1 {
            // Backward motion on same line (db, dB, dge)
            // For inclusive motions (dge), include the character at cursor position
            let range_start = end.1;
            let range_end = if is_inclusive {
                (start.1 + 1).min(self.lines[start.0].chars().count())
            } else {
                start.1
            };
            let mut chars: Vec<char> = self.lines[start.0].chars().collect();
            if range_start < range_end && range_end <= chars.len() {
                // Store deleted text in yank buffer
                self.yank_buffer = chars[range_start..range_end].iter().collect();
                self.yank_is_linewise = false;
                chars.drain(range_start..range_end);
                self.lines[start.0] = chars.into_iter().collect();
            }
            self.cursor.1 = range_start; // Move cursor to start of deletion
        } else if end.0 > start.0 {
            // Forward motion crossing to next line - need to handle multi-line deletion
            // Special case: if end.1 == 0, don't merge with end line, keep it separate

            let mut deleted_text = String::new();

            // Check if cursor position qualifies for linewise delete
            // Only applies to sentence/paragraph motions (allow_linewise = true)
            // First line of paragraph: cursor at or before first non-whitespace
            // Other lines: cursor at first non-whitespace only
            let first_non_blank = self.get_first_non_blank_in_line(start.0);
            let is_first_line_of_para = start.0 == 0 || self.lines[start.0 - 1].trim().is_empty();
            let cursor_qualifies_for_linewise = allow_linewise
                && if is_first_line_of_para {
                    start.1 <= first_non_blank // At or before first non-whitespace
                } else {
                    start.1 == first_non_blank // Exactly at first non-whitespace
                };

            if end.1 == 0 && !cursor_qualifies_for_linewise {
                // Motion lands at start of next line, cursor is after first non-whitespace
                // Preserve line structure (Neovim behavior for d) from middle of line)
                // Only delete from cursor to end of start line, plus intermediate lines

                // Get the part to keep from start line (before cursor)
                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                let start_prefix: String = start_chars[..start.1].iter().collect();
                deleted_text.push_str(&start_chars[start.1..].iter().collect::<String>());

                // Add intermediate lines to deleted text (but not end line)
                for row in (start.0 + 1)..end.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                // Store in yank buffer
                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                // Update start line to just the prefix
                self.lines[start.0] = start_prefix;

                // Remove intermediate lines (from start.0+1 to end.0-1)
                for _ in (start.0 + 1)..end.0 {
                    self.lines.remove(start.0 + 1);
                }

                // Cursor stays at start position, but clamp to line length
                // For insert mode (change operations), cursor can be at line_len
                // For normal mode, cursor must be at line_len - 1
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
                // Motion from start/before first char to start of next line - linewise deletion
                // Delete entire lines and shift content up (like Neovim d) from start of sentence)

                // Collect deleted text
                deleted_text.push_str(&self.lines[start.0]);
                for row in (start.0 + 1)..end.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                // Store in yank buffer
                self.yank_buffer = deleted_text;
                self.yank_is_linewise = true;

                if delete_empty_lines {
                    // For delete operations: remove lines entirely
                    for _ in start.0..end.0 {
                        self.lines.remove(start.0);
                    }

                    // Ensure at least one line exists
                    if self.lines.is_empty() {
                        self.lines.push(String::new());
                    }

                    // Cursor stays at start row, column 0
                    self.cursor.0 = start.0.min(self.lines.len() - 1);
                    self.cursor.1 = 0;
                } else {
                    // For change operations: clear the start line, remove intermediate lines
                    // Keep an empty line for the user to type on
                    self.lines[start.0] = String::new();

                    // Remove intermediate lines (from start.0+1 to end.0-1)
                    for _ in (start.0 + 1)..end.0 {
                        self.lines.remove(start.0 + 1);
                    }

                    // Cursor stays at start row, column 0 (on empty line)
                    self.cursor.0 = start.0;
                    self.cursor.1 = 0;
                }
            } else {
                // Motion lands in middle of a line - merge start and end lines

                // Get the part to keep from start line (before cursor)
                let start_chars: Vec<char> = self.lines[start.0].chars().collect();
                let start_line_was_blank = start_chars.iter().all(|c| c.is_whitespace());
                let start_prefix: String = start_chars[..start.1].iter().collect();
                deleted_text.push_str(&start_chars[start.1..].iter().collect::<String>());

                // Add intermediate lines to deleted text
                for row in (start.0 + 1)..end.0 {
                    deleted_text.push('\n');
                    deleted_text.push_str(&self.lines[row]);
                }

                // Get the part to keep from end line (at and after end position)
                let end_chars: Vec<char> = self.lines[end.0].chars().collect();
                let end_col = if is_inclusive {
                    (end.1 + 1).min(end_chars.len())
                } else {
                    end.1
                };
                deleted_text.push('\n');
                deleted_text.push_str(&end_chars[..end_col].iter().collect::<String>());
                let end_suffix: String = end_chars[end_col..].iter().collect();

                // Store in yank buffer
                self.yank_buffer = deleted_text;
                self.yank_is_linewise = false;

                // Join the kept parts
                let new_line = format!("{}{}", start_prefix, end_suffix);

                // Remove lines from end.0 down to start.0+1, then update start.0
                for _ in start.0..end.0 {
                    self.lines.remove(start.0 + 1);
                }
                self.lines[start.0] = new_line.clone();

                // Cursor stays at start position
                self.cursor = start;

                // For delete operations: handle cleanup of empty/blank lines
                if delete_empty_lines && new_line.is_empty() {
                    if start_line_was_blank && start.0 > 0 {
                        // Cursor was on a blank line - remove it entirely and go to start of previous line
                        self.lines.remove(start.0);
                        self.cursor.0 = start.0 - 1;
                        self.cursor.1 = 0;
                    } else if start.0 > 0 && self.lines[start.0 - 1].trim().is_empty() {
                        // There's a preceding blank line (paragraph boundary) - remove it
                        self.lines.remove(start.0 - 1);
                        self.cursor.0 = start.0 - 1;
                    }
                }
            }
        } else {
            // Forward motion on same line (dw, de)
            // w: exclusive. delete [start, end)
            // e: inclusive. delete [start, end] -> delete [start, end + 1)
            let mut range_end = end.1;
            if is_inclusive {
                range_end += 1;
            }
            let mut chars: Vec<char> = self.lines[start.0].chars().collect();
            // Cap at character count
            if range_end > chars.len() {
                range_end = chars.len();
            }

            if start.1 < range_end {
                // Store deleted text in yank buffer
                self.yank_buffer = chars[start.1..range_end].iter().collect();
                self.yank_is_linewise = false;
                chars.drain(start.1..range_end);
                self.lines[start.0] = chars.into_iter().collect();
            }
        }
        self.clamp_cursor();
        self.update_desired_col();
        // For change operations (Insert mode), don't record yet - will be recorded when exiting insert
        if self.mode != EditorMode::Insert {
            self.record_change();
        }
    }

    // Helper to perform yank action based on a motion
    fn perform_yank_motion<F>(&mut self, motion: F, is_inclusive: bool)
    where
        F: Fn(&EditorState) -> (usize, usize),
    {
        let start = self.cursor;
        let end = motion(self);

        // Handle direction - use character-based operations
        if end.0 < start.0 {
            // Backward motion crossing to previous line - yank multi-line

            // Determine if this is a linewise yank
            // First line of paragraph: cursor at or before first non-whitespace
            // Other lines: cursor at first non-whitespace only
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
                // Linewise yank: include entire lines
                yanked_text.push_str(&end_chars.iter().collect::<String>());

                // Intermediate lines
                for row in (end.0 + 1)..start.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                // Include entire start line
                yanked_text.push('\n');
                yanked_text.push_str(&start_chars.iter().collect::<String>());
            } else {
                // Character-wise yank
                yanked_text.push_str(&end_chars[end.1..].iter().collect::<String>());

                // Intermediate lines
                for row in (end.0 + 1)..start.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                // From start of start line to cursor
                yanked_text.push('\n');
                yanked_text.push_str(&start_chars[..start.1].iter().collect::<String>());
            }

            self.yank_buffer = yanked_text;
            self.yank_is_linewise = is_linewise;

            // Move cursor to start of yanked region (like Neovim)
            self.cursor = end;
            self.update_desired_col();
        } else if end.0 == start.0 && end.1 < start.1 {
            // Backward motion on same line (yb, yB, yge)
            // For inclusive motions (yge), include the character at cursor position
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

            // Move cursor to start of yanked region (like Neovim)
            self.cursor.1 = end.1;
            self.update_desired_col();
        } else if end.0 > start.0 {
            // Forward motion crossing to next line - yank multi-line
            let end_chars: Vec<char> = self.lines[end.0].chars().collect();
            let end_col = if is_inclusive {
                (end.1 + 1).min(end_chars.len())
            } else {
                end.1
            };

            // Determine if this is a linewise yank
            // First line of paragraph: cursor at or before first non-whitespace
            // Other lines: cursor at first non-whitespace only
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
                // Linewise yank: include entire line from column 0 (including leading whitespace)
                yanked_text.push_str(&start_chars.iter().collect::<String>());

                // Intermediate lines
                for row in (start.0 + 1)..end.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }
                // Don't add trailing newline for linewise
            } else {
                // Character-wise yank: from cursor position
                yanked_text.push_str(&start_chars[start.1..].iter().collect::<String>());

                // Intermediate lines
                for row in (start.0 + 1)..end.0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&self.lines[row]);
                }

                // Add content from the end line if there's something to add
                if end_col > 0 {
                    yanked_text.push('\n');
                    yanked_text.push_str(&end_chars[..end_col].iter().collect::<String>());
                }
            }

            self.yank_buffer = yanked_text;
            self.yank_is_linewise = is_linewise;
        } else {
            // Forward motion on same line (yw, ye)
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

    /// Helper to yank a text object on the current line given (start, end) bounds
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
                // Same line - use character indices
                let chars: Vec<char> = self.lines[open_row].chars().collect();
                if open_col + 1 < close_col {
                    self.yank_buffer = chars[open_col + 1..close_col].iter().collect();
                } else {
                    self.yank_buffer.clear();
                }
                self.yank_is_linewise = false;
            } else {
                // Multi-line: yank content between brackets
                let mut yanked = String::new();

                // First line: from after open bracket (character-based)
                // Only skip whitespace if there's NO content after open bracket on same line
                let first_chars: Vec<char> = self.lines[open_row].chars().collect();
                let after_open: String = first_chars[open_col + 1..].iter().collect();
                let has_first_line_content = !after_open.trim().is_empty();
                if has_first_line_content {
                    // Has content - include everything after open bracket as-is
                    yanked.push_str(&after_open);
                }
                // If no content (only whitespace), skip it entirely

                // Middle lines
                for row in (open_row + 1)..close_row {
                    // Add leading newline if there's content before, or if open bracket is alone
                    if !yanked.is_empty() || !has_first_line_content {
                        yanked.push('\n');
                    }
                    yanked.push_str(&self.lines[row]);
                }

                // Last line: up to close bracket (character-based)
                // Only skip whitespace if there's NO content before close bracket on same line
                if close_row > open_row {
                    let last_chars: Vec<char> = self.lines[close_row].chars().collect();
                    let before_close: String = last_chars[..close_col].iter().collect();
                    let has_last_line_content = !before_close.trim().is_empty();
                    if has_last_line_content {
                        // Has content - include everything before close bracket as-is
                        if !yanked.is_empty() {
                            yanked.push('\n');
                        }
                        yanked.push_str(&before_close);
                    }
                    // If no content (only whitespace), skip it entirely
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
                // Same line - use character indices
                let chars: Vec<char> = self.lines[open_row].chars().collect();
                self.yank_buffer = chars[open_col..=close_col].iter().collect();
                self.yank_is_linewise = false;
            } else {
                // Multi-line: yank including brackets
                let mut yanked = String::new();
                // First line: from open bracket (character-based)
                let first_chars: Vec<char> = self.lines[open_row].chars().collect();
                yanked.extend(&first_chars[open_col..]);
                // Middle lines
                for row in (open_row + 1)..close_row {
                    yanked.push('\n');
                    yanked.push_str(&self.lines[row]);
                }
                // Last line: up to and including close bracket (character-based)
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
            // Search for target character starting after cursor
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
            // Search for target character backwards before cursor
            if let Some(rel_pos) = chars[..self.cursor.1].iter().rposition(|&c| c == target) {
                let start_col = if inclusive { rel_pos } else { rel_pos + 1 };
                self.yank_buffer = chars[start_col..self.cursor.1].iter().collect();
                self.yank_is_linewise = false;
                // Move cursor to start of yanked region (like Neovim)
                self.cursor.1 = start_col;
                self.update_desired_col();
            }
        }
    }

    fn yank_to_matching_bracket(&mut self) {
        let saved_cursor = self.cursor;
        self.jump_to_matching_bracket();
        // Check if cursor moved (indicating a match was found)
        if self.cursor != saved_cursor {
            let end = self.cursor;
            // Handle multi-line case
            if saved_cursor.0 == end.0 {
                // Same line - use character indices
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
                // Move cursor to opening bracket (smaller column)
                self.cursor = (saved_cursor.0, start_col);
                self.update_desired_col();
            } else {
                // Multi-line: yank from start to end including brackets
                let (start_pos, end_pos) = if saved_cursor.0 < end.0
                    || (saved_cursor.0 == end.0 && saved_cursor.1 < end.1)
                {
                    (saved_cursor, end)
                } else {
                    (end, saved_cursor)
                };
                let mut yanked = String::new();
                // First line: from start position (character-based)
                let first_chars: Vec<char> = self.lines[start_pos.0].chars().collect();
                yanked.extend(&first_chars[start_pos.1..]);
                // Middle lines
                for row in (start_pos.0 + 1)..end_pos.0 {
                    yanked.push('\n');
                    yanked.push_str(&self.lines[row]);
                }
                // Last line: up to and including end position (character-based)
                yanked.push('\n');
                let last_chars: Vec<char> = self.lines[end_pos.0].chars().collect();
                yanked.extend(&last_chars[..=end_pos.1]);
                self.yank_buffer = yanked;
                self.yank_is_linewise = false;
                // Move cursor to opening bracket (earlier position)
                self.cursor = start_pos;
                self.update_desired_col();
            }
        }
    }

    fn yank_to_prev_unmatched(&mut self, open: char, close: char) {
        let saved_cursor = self.cursor;
        self.jump_to_prev_unmatched(open, close);
        // Check if cursor moved
        if self.cursor != saved_cursor {
            let target = self.cursor;
            // Yank from target to saved_cursor (exclusive of target position)
            if target.0 == saved_cursor.0 {
                // Same line - use character indices
                let chars: Vec<char> = self.lines[target.0].chars().collect();
                if target.1 < saved_cursor.1 && saved_cursor.1 <= chars.len() {
                    self.yank_buffer = chars[target.1 + 1..saved_cursor.1].iter().collect();
                    self.yank_is_linewise = false;
                }
            }
            // Multi-line yank not supported for this motion for simplicity
            // Cursor stays at target (backward motion moves cursor like Neovim)
            self.update_desired_col();
        }
    }

    fn yank_to_next_unmatched(&mut self, open: char, close: char) {
        let saved_cursor = self.cursor;
        self.jump_to_next_unmatched(open, close);
        // Check if cursor moved
        if self.cursor != saved_cursor {
            let target = self.cursor;
            self.cursor = saved_cursor;
            // Yank from cursor to target (exclusive of target position)
            if saved_cursor.0 == target.0 {
                // Same line - use character indices
                let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
                if saved_cursor.1 < target.1 && target.1 <= chars.len() {
                    self.yank_buffer = chars[saved_cursor.1..target.1].iter().collect();
                    self.yank_is_linewise = false;
                }
            }
            // Multi-line yank not supported for this motion for simplicity
        }
    }

    fn paste_after(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;
        if self.yank_buffer.is_empty() {
            return;
        }
        if self.yank_is_linewise {
            // Insert yanked lines below current line
            // Use split('\n') instead of lines() to preserve trailing blank lines
            let new_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            for (i, line) in new_lines.iter().enumerate() {
                self.lines.insert(self.cursor.0 + 1 + i, line.to_string());
            }
            // Move cursor to first non-blank of first inserted line
            self.cursor.0 += 1;
            self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        } else if self.yank_buffer.contains('\n') {
            // Multi-line characterwise paste (e.g., from yi( across lines)
            let paste_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let current_line_chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = if current_line_chars.is_empty() {
                0
            } else {
                self.cursor.1 + 1
            };

            // Split current line at insert position
            let before: String = current_line_chars[..insert_pos.min(current_line_chars.len())]
                .iter()
                .collect();
            let after: String = current_line_chars[insert_pos.min(current_line_chars.len())..]
                .iter()
                .collect();

            // First part: before + first paste line
            self.lines[self.cursor.0] = before + paste_lines[0];

            // Middle lines
            for (i, paste_line) in paste_lines[1..paste_lines.len() - 1].iter().enumerate() {
                self.lines
                    .insert(self.cursor.0 + 1 + i, paste_line.to_string());
            }

            // Last part: last paste line + after
            if paste_lines.len() > 1 {
                let last_paste_line = paste_lines[paste_lines.len() - 1];
                self.lines.insert(
                    self.cursor.0 + paste_lines.len() - 1,
                    last_paste_line.to_string() + &after,
                );
            }

            // Move cursor to end of pasted text (last character of last paste line)
            self.cursor.0 += paste_lines.len() - 1;
            let last_paste_chars = paste_lines[paste_lines.len() - 1].chars().count();
            self.cursor.1 = if last_paste_chars > 0 {
                last_paste_chars - 1
            } else {
                0
            };
        } else {
            // Single line characterwise paste - use character-based insertion
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = if chars.is_empty() {
                0
            } else {
                (self.cursor.1 + 1).min(chars.len())
            };
            let paste_chars: Vec<char> = self.yank_buffer.chars().collect();

            // Insert paste characters
            for (i, c) in paste_chars.iter().enumerate() {
                chars.insert(insert_pos + i, *c);
            }
            self.lines[self.cursor.0] = chars.into_iter().collect();

            // Move cursor to last character of pasted text
            self.cursor.1 = insert_pos + paste_chars.len().saturating_sub(1);
        }
        self.clamp_cursor();
        self.record_change();
    }

    fn paste_before(&mut self) {
        self.save_undo_state();
        self.lines_version += 1;
        if self.yank_buffer.is_empty() {
            return;
        }
        if self.yank_is_linewise {
            // Insert yanked lines above current line
            // Use split('\n') instead of lines() to preserve trailing blank lines
            let new_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            for (i, line) in new_lines.iter().enumerate() {
                self.lines.insert(self.cursor.0 + i, line.to_string());
            }
            // Move cursor to first non-blank of first inserted line
            self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        } else if self.yank_buffer.contains('\n') {
            // Multi-line characterwise paste
            let paste_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let current_line_chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = self.cursor.1.min(current_line_chars.len());

            // Split current line at insert position
            let before: String = current_line_chars[..insert_pos].iter().collect();
            let after: String = current_line_chars[insert_pos..].iter().collect();

            // First part: before + first paste line
            self.lines[self.cursor.0] = before + paste_lines[0];

            // Middle lines
            for (i, paste_line) in paste_lines[1..paste_lines.len() - 1].iter().enumerate() {
                self.lines
                    .insert(self.cursor.0 + 1 + i, paste_line.to_string());
            }

            // Last part: last paste line + after
            if paste_lines.len() > 1 {
                let last_paste_line = paste_lines[paste_lines.len() - 1];
                self.lines.insert(
                    self.cursor.0 + paste_lines.len() - 1,
                    last_paste_line.to_string() + &after,
                );
            }

            // Move cursor to end of pasted text
            self.cursor.0 += paste_lines.len() - 1;
            let last_paste_chars = paste_lines[paste_lines.len() - 1].chars().count();
            self.cursor.1 = if last_paste_chars > 0 {
                last_paste_chars - 1
            } else {
                0
            };
        } else {
            // Single line characterwise paste - use character-based insertion
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = self.cursor.1.min(chars.len());
            let paste_chars: Vec<char> = self.yank_buffer.chars().collect();

            // Insert paste characters
            for (i, c) in paste_chars.iter().enumerate() {
                chars.insert(insert_pos + i, *c);
            }
            self.lines[self.cursor.0] = chars.into_iter().collect();

            // Move cursor to last character of pasted text
            self.cursor.1 = insert_pos + paste_chars.len().saturating_sub(1);
        }
        self.clamp_cursor();
        self.record_change();
    }

    fn get_first_non_blank_in_line(&self, row: usize) -> usize {
        let line = &self.lines[row];
        line.chars().position(|c| !c.is_whitespace()).unwrap_or(0)
    }

    // Search functionality
    fn search_next(&mut self) {
        if self.search_pattern.is_empty() {
            return;
        }
        match self.search_direction {
            SearchDirection::Forward => self.search_forward_from_cursor(),
            SearchDirection::Backward => self.search_backward_from_cursor(),
        }
    }

    fn search_prev(&mut self) {
        if self.search_pattern.is_empty() {
            return;
        }
        // Search in opposite direction
        match self.search_direction {
            SearchDirection::Forward => self.search_backward_from_cursor(),
            SearchDirection::Backward => self.search_forward_from_cursor(),
        }
    }

    fn search_forward_from_cursor(&mut self) {
        let pattern = &self.search_pattern;
        if pattern.is_empty() {
            return;
        }

        // Start searching from current position + 1
        let start_row = self.cursor.0;
        let start_col = self.cursor.1 + 1;

        // Search in current line from cursor position
        let current_line = &self.lines[start_row];
        let chars: Vec<char> = current_line.chars().collect();
        if start_col < chars.len() {
            let search_str: String = chars[start_col..].iter().collect();
            if let Some(pos) = search_str.find(pattern) {
                // Convert byte position to char position
                let char_pos = search_str[..pos].chars().count();
                self.cursor.1 = start_col + char_pos;
                return;
            }
        }

        // Search in subsequent lines
        for row in (start_row + 1)..self.lines.len() {
            if let Some(pos) = self.lines[row].find(pattern) {
                // Convert byte position to char position
                let char_pos = self.lines[row][..pos].chars().count();
                self.cursor.0 = row;
                self.cursor.1 = char_pos;
                return;
            }
        }

        // Wrap around to beginning
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

        // Search in current line before cursor
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

        // Search in previous lines (from end)
        for row in (0..start_row).rev() {
            if let Some(pos) = self.lines[row].rfind(pattern) {
                let char_pos = self.lines[row][..pos].chars().count();
                self.cursor.0 = row;
                self.cursor.1 = char_pos;
                return;
            }
        }

        // Wrap around to end
        for row in (start_row..self.lines.len()).rev() {
            let search_start = if row == start_row {
                // Get byte offset for start_col
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
        // Get the word under cursor
        let (start, end) = self.get_inner_word_bounds();
        let line = &self.lines[self.cursor.0];
        if start < end && end <= line.len() {
            let word = &line[start..end];
            // Use word boundaries for exact word match
            self.search_pattern = word.to_string();
            self.search_direction = if forward {
                SearchDirection::Forward
            } else {
                SearchDirection::Backward
            };
            // Move to next/previous occurrence
            if forward {
                self.search_forward_from_cursor();
            } else {
                self.search_backward_from_cursor();
            }
        }
    }

    // Visual mode helpers
    fn get_visual_selection(&self) -> ((usize, usize), (usize, usize)) {
        // Returns (start, end) where start <= end
        if self.visual_start.0 < self.cursor.0
            || (self.visual_start.0 == self.cursor.0 && self.visual_start.1 <= self.cursor.1)
        {
            (self.visual_start, self.cursor)
        } else {
            (self.cursor, self.visual_start)
        }
    }

    fn delete_visual_selection(&mut self) {
        self.save_undo_state();
        let (start, end) = self.get_visual_selection();

        if self.mode == EditorMode::VisualLine {
            // Delete entire lines
            let yanked: Vec<&str> = self.lines[start.0..=end.0]
                .iter()
                .map(|s| s.as_str())
                .collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;

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
            // Character-wise deletion
            if start.0 == end.0 {
                // Same line
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
                // Multi-line
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
                // Position cursor at start of remaining content after deletion
                let merged_line_len = self.lines[start.0].chars().count();
                if first_part_len < merged_line_len {
                    // There's content after first_part on the merged line
                    self.cursor = (start.0, first_part_len);
                } else if start.0 + 1 < self.lines.len() {
                    // No content left on merged line after first_part, go to next line
                    self.cursor = (start.0 + 1, 0);
                } else {
                    // Stay on current line at valid position
                    self.cursor = (start.0, first_part_len.saturating_sub(1));
                }
            }
        }
        self.clamp_cursor();
        self.update_desired_col();
        self.record_change();
    }

    fn yank_visual_selection(&mut self) {
        let (start, end) = self.get_visual_selection();

        if self.mode == EditorMode::VisualLine {
            // Yank entire lines
            let yanked: Vec<&str> = self.lines[start.0..=end.0]
                .iter()
                .map(|s| s.as_str())
                .collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
        } else {
            // Character-wise yank
            if start.0 == end.0 {
                // Same line
                let chars: Vec<char> = self.lines[start.0].chars().collect();
                let sel_end = (end.1 + 1).min(chars.len());
                self.yank_buffer = chars[start.1..sel_end].iter().collect();
                self.yank_is_linewise = false;
            } else {
                // Multi-line
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
        }
        // Move cursor to start of selection (Vim behavior)
        self.cursor = start;
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        self.render()?;
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            match self.mode {
                EditorMode::Normal => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('C'),
                        modifiers: Modifiers::CTRL,
                    }) => break,
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
                        self.increment_number();
                        self.last_change = LastChange::IncrementNumber;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('X'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        self.decrement_number();
                        self.last_change = LastChange::DecrementNumber;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        ..
                    }) => {
                        // Handle operator + pending keys first (e.g., dgg, cgg)
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
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.perform_delete_motion(
                                        |s| s.get_word_end_backward_pos(WordType::Word),
                                        true,
                                        false,
                                        false,
                                    );
                                    self.last_change = LastChange::ChangeWordEndBackward(WordType::Word);
                                } else if op == 'y' {
                                    self.perform_yank_motion(
                                        |s| s.get_word_end_backward_pos(WordType::Word),
                                        true,
                                    );
                                } else {
                                    self.perform_delete_motion(
                                        |s| s.get_word_end_backward_pos(WordType::Word),
                                        true,
                                        true,
                                        false,
                                    );
                                    self.last_change = LastChange::DeleteWordEndBackward(WordType::Word);
                                }
                            } else if first == KeyCode::Char('g') && c == 'E' {
                                // dgE / cgE / ygE - delete/change/yank backward to end of previous WORD
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.perform_delete_motion(
                                        |s| s.get_word_end_backward_pos(WordType::LongWord),
                                        true,
                                        false,
                                        false,
                                    );
                                    self.last_change = LastChange::ChangeWordEndBackward(WordType::LongWord);
                                } else if op == 'y' {
                                    self.perform_yank_motion(
                                        |s| s.get_word_end_backward_pos(WordType::LongWord),
                                        true,
                                    );
                                } else {
                                    self.perform_delete_motion(
                                        |s| s.get_word_end_backward_pos(WordType::LongWord),
                                        true,
                                        true,
                                        false,
                                    );
                                    self.last_change = LastChange::DeleteWordEndBackward(WordType::LongWord);
                                }
                            } else if first == KeyCode::Char('i') && c == 'w' {
                                // diw / ciw / yiw - delete/change/yank inner word
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeInnerWord;
                                    self.delete_inner_word();
                                } else if op == 'y' {
                                    self.yank_inner_word();
                                } else {
                                    self.last_change = LastChange::DeleteInnerWord;
                                    self.delete_inner_word();
                                }
                            } else if first == KeyCode::Char('a') && c == 'w' {
                                // daw / caw / yaw - delete/change/yank a word
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeAWord;
                                    self.delete_a_word();
                                } else if op == 'y' {
                                    self.yank_a_word();
                                } else {
                                    self.last_change = LastChange::DeleteAWord;
                                    self.delete_a_word();
                                }
                            } else if first == KeyCode::Char('i') && c == 'W' {
                                // diW / ciW / yiW - delete/change/yank inner WORD
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeInnerLongWord;
                                    self.delete_inner_long_word();
                                } else if op == 'y' {
                                    self.yank_inner_long_word();
                                } else {
                                    self.last_change = LastChange::DeleteInnerLongWord;
                                    self.delete_inner_long_word();
                                }
                            } else if first == KeyCode::Char('a') && c == 'W' {
                                // daW / caW / yaW - delete/change/yank a WORD
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeALongWord;
                                    self.delete_a_long_word();
                                } else if op == 'y' {
                                    self.yank_a_long_word();
                                } else {
                                    self.last_change = LastChange::DeleteALongWord;
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
                                    self.last_change = LastChange::ChangeInnerPair(c);
                                    self.delete_inner_pair(c);
                                } else if op == 'y' {
                                    self.yank_inner_pair(c);
                                } else {
                                    self.last_change = LastChange::DeleteInnerPair(c);
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
                                    self.last_change = LastChange::ChangeAroundPair(c);
                                    self.delete_around_pair(c);
                                } else if op == 'y' {
                                    self.yank_around_pair(c);
                                } else {
                                    self.last_change = LastChange::DeleteAroundPair(c);
                                    self.delete_around_pair(c);
                                }
                            } else if first == KeyCode::Char('i') && c == 'p' {
                                // dip / cip / yip - delete/change/yank inner paragraph
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change = LastChange::ChangeInnerParagraph;
                                    self.change_inner_paragraph();
                                } else if op == 'y' {
                                    self.yank_inner_paragraph();
                                } else {
                                    self.last_change = LastChange::DeleteInnerParagraph;
                                    self.delete_inner_paragraph();
                                }
                            } else if first == KeyCode::Char('a') && c == 'p' {
                                // dap / cap / yap - delete/change/yank a paragraph
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change = LastChange::ChangeAParagraph;
                                    self.change_a_paragraph();
                                } else if op == 'y' {
                                    self.yank_a_paragraph();
                                } else {
                                    self.last_change = LastChange::DeleteAParagraph;
                                    self.delete_a_paragraph();
                                }
                            } else if first == KeyCode::Char('i') && c == 's' {
                                // dis / cis / yis - delete/change/yank inner sentence
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change = LastChange::ChangeInnerSentence;
                                    self.change_inner_sentence();
                                } else if op == 'y' {
                                    self.yank_inner_sentence();
                                } else {
                                    self.last_change = LastChange::DeleteInnerSentence;
                                    self.delete_inner_sentence();
                                }
                            } else if first == KeyCode::Char('a') && c == 's' {
                                // das / cas / yas - delete/change/yank a sentence
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.last_change = LastChange::ChangeASentence;
                                    self.change_a_sentence();
                                } else if op == 'y' {
                                    self.yank_a_sentence();
                                } else {
                                    self.last_change = LastChange::DeleteASentence;
                                    self.delete_a_sentence();
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
                                    self.last_change = LastChange::ChangeToChar(c, true);
                                    self.delete_to_char_forward(c, true);
                                } else if op == 'y' {
                                    self.yank_to_char_forward(c, true);
                                } else {
                                    self.last_change = LastChange::DeleteToChar(c, true);
                                    self.delete_to_char_forward(c, true);
                                }
                            } else if first == KeyCode::Char('F') {
                                // dF{char} / cF{char} / yF{char} - delete/change/yank backward to char (inclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeBackToChar(c, true);
                                    self.delete_to_char_backward(c, true);
                                } else if op == 'y' {
                                    self.yank_to_char_backward(c, true);
                                } else {
                                    self.last_change = LastChange::DeleteBackToChar(c, true);
                                    self.delete_to_char_backward(c, true);
                                }
                            } else if first == KeyCode::Char('t') {
                                // dt{char} / ct{char} / yt{char} - delete/change/yank till char (exclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeToChar(c, false);
                                    self.delete_to_char_forward(c, false);
                                } else if op == 'y' {
                                    self.yank_to_char_forward(c, false);
                                } else {
                                    self.last_change = LastChange::DeleteToChar(c, false);
                                    self.delete_to_char_forward(c, false);
                                }
                            } else if first == KeyCode::Char('T') {
                                // dT{char} / cT{char} / yT{char} - delete/change/yank backward till char (exclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeBackToChar(c, false);
                                    self.delete_to_char_backward(c, false);
                                } else if op == 'y' {
                                    self.yank_to_char_backward(c, false);
                                } else {
                                    self.last_change = LastChange::DeleteBackToChar(c, false);
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
                                    'c' => self.substitute_line(), // cc == S
                                    'w' => {
                                        // cw behavior depends on what we're on:
                                        // - On a word: cw is like ce (change to end of word)
                                        // - On whitespace: cw uses w motion (change whitespace to start of next word)
                                        // - On punctuation: cw is like ce (change to end of punctuation)
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        let chars: Vec<char> =
                                            self.lines[self.cursor.0].chars().collect();
                                        let on_whitespace = self.cursor.1 < chars.len()
                                            && chars[self.cursor.1].is_whitespace();
                                        if on_whitespace {
                                            self.perform_delete_motion(
                                                |s| s.get_word_forward_pos(WordType::Word),
                                                false,
                                                false,
                                                false,
                                            );
                                        } else {
                                            self.perform_delete_motion(
                                                |s| s.get_word_end_pos(WordType::Word),
                                                true,
                                                false,
                                                false,
                                            );
                                        }
                                        self.last_change = LastChange::ChangeWordMotion(WordType::Word);
                                    }
                                    'W' => {
                                        // cW behavior: like cw but for WORD
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        let chars: Vec<char> =
                                            self.lines[self.cursor.0].chars().collect();
                                        let on_whitespace = self.cursor.1 < chars.len()
                                            && chars[self.cursor.1].is_whitespace();
                                        if on_whitespace {
                                            self.perform_delete_motion(
                                                |s| s.get_word_forward_pos(WordType::LongWord),
                                                false,
                                                false,
                                                false,
                                            );
                                        } else {
                                            self.perform_delete_motion(
                                                |s| s.get_word_end_pos(WordType::LongWord),
                                                true,
                                                false,
                                                false,
                                            );
                                        }
                                        self.last_change = LastChange::ChangeWordMotion(WordType::LongWord);
                                    }
                                    'e' => {
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_word_end_pos(WordType::Word),
                                            true,
                                            false,
                                            false,
                                        );
                                        self.last_change = LastChange::ChangeWordEnd(WordType::Word);
                                    }
                                    'E' => {
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_word_end_pos(WordType::LongWord),
                                            true,
                                            false,
                                            false,
                                        );
                                        self.last_change = LastChange::ChangeWordEnd(WordType::LongWord);
                                    }
                                    'b' => {
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_word_backward_pos(WordType::Word),
                                            false,
                                            false,
                                            false,
                                        );
                                        self.last_change = LastChange::ChangeWordBackward(WordType::Word);
                                    }
                                    'B' => {
                                        self.insert_buffer.clear();
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_word_backward_pos(WordType::LongWord),
                                            false,
                                            false,
                                            false,
                                        );
                                        self.last_change = LastChange::ChangeWordBackward(WordType::LongWord);
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
                                        self.delete_line();
                                        self.last_change = LastChange::DeleteLine;
                                    }
                                    'w' => {
                                        self.perform_delete_motion(
                                            |s| s.get_word_forward_pos(WordType::Word),
                                            false,
                                            true,
                                            false,
                                        );
                                        self.last_change = LastChange::DeleteWordMotion(WordType::Word);
                                    }
                                    'W' => {
                                        self.perform_delete_motion(
                                            |s| s.get_word_forward_pos(WordType::LongWord),
                                            false,
                                            true,
                                            false,
                                        );
                                        self.last_change = LastChange::DeleteWordMotion(WordType::LongWord);
                                    }
                                    'e' => {
                                        self.perform_delete_motion(
                                            |s| s.get_word_end_pos(WordType::Word),
                                            true,
                                            true,
                                            false,
                                        );
                                        self.last_change = LastChange::DeleteWordEnd(WordType::Word);
                                    }
                                    'E' => {
                                        self.perform_delete_motion(
                                            |s| s.get_word_end_pos(WordType::LongWord),
                                            true,
                                            true,
                                            false,
                                        );
                                        self.last_change = LastChange::DeleteWordEnd(WordType::LongWord);
                                    }
                                    'b' => {
                                        self.perform_delete_motion(
                                            |s| s.get_word_backward_pos(WordType::Word),
                                            false,
                                            true,
                                            false,
                                        );
                                        self.last_change = LastChange::DeleteWordBackward(WordType::Word);
                                    }
                                    'B' => {
                                        self.perform_delete_motion(
                                            |s| s.get_word_backward_pos(WordType::LongWord),
                                            false,
                                            true,
                                            false,
                                        );
                                        self.last_change = LastChange::DeleteWordBackward(WordType::LongWord);
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
                                    'y' => self.yank_line(),
                                    'w' => self.perform_yank_motion(
                                        |s| s.get_word_forward_pos(WordType::Word),
                                        false,
                                    ),
                                    'W' => self.perform_yank_motion(
                                        |s| s.get_word_forward_pos(WordType::LongWord),
                                        false,
                                    ),
                                    'e' => self.perform_yank_motion(
                                        |s| s.get_word_end_pos(WordType::Word),
                                        true,
                                    ),
                                    'E' => self.perform_yank_motion(
                                        |s| s.get_word_end_pos(WordType::LongWord),
                                        true,
                                    ),
                                    'b' => self.perform_yank_motion(
                                        |s| s.get_word_backward_pos(WordType::Word),
                                        false,
                                    ),
                                    'B' => self.perform_yank_motion(
                                        |s| s.get_word_backward_pos(WordType::LongWord),
                                        false,
                                    ),
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
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('g') && c == 'e' {
                                // ge - move backward to end of previous word
                                self.move_to_word_end_backward(WordType::Word);
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('g') && c == 'E' {
                                // gE - move backward to end of previous WORD
                                self.move_to_word_end_backward(WordType::LongWord);
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('Z') && c == 'Z' {
                                self.submit();
                                break;
                            } else if first == KeyCode::Char('Z') && c == 'Q' {
                                break;
                            } else if first == KeyCode::Char('[') && c == '(' {
                                self.jump_to_prev_unmatched('(', ')');
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('[') && c == '{' {
                                self.jump_to_prev_unmatched('{', '}');
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char(']') && c == ')' {
                                self.jump_to_next_unmatched('(', ')');
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char(']') && c == '}' {
                                self.jump_to_next_unmatched('{', '}');
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('f') {
                                self.move_to_char_forward(c);
                                self.last_char_search = Some(('f', c));
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('F') {
                                self.move_to_char_backward(c);
                                self.last_char_search = Some(('F', c));
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('t') {
                                self.move_till_char_forward(c);
                                self.last_char_search = Some(('t', c));
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('T') {
                                self.move_till_char_backward(c);
                                self.last_char_search = Some(('T', c));
                                self.update_desired_col();
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('r') {
                                self.replace_char(c);
                                self.last_change = LastChange::ReplaceChar(c);
                                self.pending_keys.clear();
                            } else {
                                self.pending_keys.clear();
                            }
                            self.render()?;
                            continue;
                        }

                        if c == 'g'
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
                            'h' => self.move_cursor(0, -1),
                            'j' => self.move_cursor(1, 0),
                            'k' => self.move_cursor(-1, 0),
                            'l' => self.move_cursor(0, 1),
                            'w' => self.move_word_forward(WordType::Word),
                            'W' => self.move_word_forward(WordType::LongWord),
                            'e' => self.move_to_word_end(WordType::Word),
                            'E' => self.move_to_word_end(WordType::LongWord),
                            'b' => self.move_word_backward(WordType::Word),
                            'B' => self.move_word_backward(WordType::LongWord),
                            'x' => {
                                self.delete_char();
                                self.last_change = LastChange::DeleteChar;
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
                            'D' => {
                                self.delete_to_end_of_line();
                                self.last_change = LastChange::DeleteToEndOfLine;
                            }
                            'C' => {
                                self.insert_buffer.clear();
                                self.change_to_end_of_line();
                                self.last_change = LastChange::ChangeToEndOfLine;
                            }
                            'S' => {
                                self.insert_buffer.clear();
                                self.substitute_line();
                                self.last_change = LastChange::SubstituteLine;
                            }
                            's' => {
                                self.insert_buffer.clear();
                                self.substitute_char();
                                self.last_change = LastChange::SubstituteChar;
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
                                self.paste_after();
                                self.update_desired_col();
                            }
                            'P' => {
                                self.paste_before();
                                self.update_desired_col();
                            }
                            'Y' => self.yank_to_end_of_line(), // Y yanks to end of line (like y$)
                            '/' => {
                                // Enter forward search mode
                                self.mode = EditorMode::Search;
                                self.search_direction = SearchDirection::Forward;
                                self.search_input.clear();
                            }
                            '?' => {
                                // Enter backward search mode
                                self.mode = EditorMode::Search;
                                self.search_direction = SearchDirection::Backward;
                                self.search_input.clear();
                            }
                            'n' => {
                                self.search_next();
                                self.update_desired_col();
                            }
                            'N' => {
                                self.search_prev();
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
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        self.submit();
                        break;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        self.pending_keys.clear();
                        self.pending_operator = None;
                    }
                    _ => {}
                },
                EditorMode::Insert => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
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
                                LastChange::ChangeWordMotion(_)
                                | LastChange::ChangeWordBackward(_)
                                | LastChange::ChangeWordEnd(_)
                                | LastChange::ChangeWordEndBackward(_)
                                | LastChange::ChangeInnerWord
                                | LastChange::ChangeAWord
                                | LastChange::ChangeInnerLongWord
                                | LastChange::ChangeALongWord
                                | LastChange::ChangeInnerPair(_)
                                | LastChange::ChangeAroundPair(_)
                                | LastChange::ChangeInnerParagraph
                                | LastChange::ChangeAParagraph
                                | LastChange::ChangeInnerSentence
                                | LastChange::ChangeASentence
                                | LastChange::ChangeToChar(_, _)
                                | LastChange::ChangeBackToChar(_, _)
                                | LastChange::ChangeToEndOfLine
                                | LastChange::SubstituteLine
                                | LastChange::SubstituteChar => {
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
                    _ => {}
                },
                EditorMode::Search => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        // Cancel search, go back to normal mode
                        self.mode = EditorMode::Normal;
                        self.search_input.clear();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        // Confirm search and find first match
                        self.search_pattern = self.search_input.clone();
                        self.mode = EditorMode::Normal;
                        self.search_input.clear();
                        // Perform the search
                        match self.search_direction {
                            SearchDirection::Forward => self.search_forward_from_cursor(),
                            SearchDirection::Backward => self.search_backward_from_cursor(),
                        }
                        self.update_desired_col();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) => {
                        self.search_input.pop();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        modifiers,
                    }) => {
                        if !modifiers.contains(Modifiers::CTRL)
                            && !modifiers.contains(Modifiers::ALT)
                        {
                            self.search_input.push(c);
                        }
                    }
                    _ => {}
                },
                EditorMode::Visual | EditorMode::VisualLine => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        self.mode = EditorMode::Normal;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        ..
                    }) => {
                        // Check for pending text object keys FIRST
                        if let Some(KeyCode::Char(pending)) = self.pending_keys.first().copied() {
                            let handled = match (pending, c) {
                                ('i', '(' | ')') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('(')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '(' | ')') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('(')
                                    {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '[' | ']') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('[')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '[' | ']') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('[')
                                    {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '{' | '}') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('{')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '{' | '}') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('{')
                                    {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '<' | '>') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('<')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '<' | '>') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('<')
                                    {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '"') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('"')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '"') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('"')
                                    {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '\'') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('\'')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '\'') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('\'')
                                    {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '`') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('`')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '`') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('`')
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
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('(')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', 'b') => {
                                    // ab is same as a(
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('(')
                                    {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', 'B') => {
                                    // iB is same as i{
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('{')
                                    {
                                        let (start, end) =
                                            self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', 'B') => {
                                    // aB is same as a{
                                    if let Some(((open_row, open_col), (close_row, close_col))) =
                                        self.find_pair_bounds('{')
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
                                    let (start_row, end_row) = self.get_inner_paragraph_bounds();
                                    self.visual_start = (start_row, 0);
                                    let end_col =
                                        self.lines[end_row].chars().count().saturating_sub(1);
                                    self.cursor = (end_row, end_col);
                                    // Switch to linewise visual mode for paragraph selection
                                    self.mode = EditorMode::VisualLine;
                                    true
                                }
                                ('a', 'p') => {
                                    // ap - a paragraph (includes trailing/leading blank lines)
                                    if let Some((start_row, end_row)) =
                                        self.get_a_paragraph_bounds()
                                    {
                                        self.visual_start = (start_row, 0);
                                        let end_col =
                                            self.lines[end_row].chars().count().saturating_sub(1);
                                        self.cursor = (end_row, end_col);
                                        // Switch to linewise visual mode for paragraph selection
                                        self.mode = EditorMode::VisualLine;
                                    }
                                    true
                                }
                                ('i', 's') => {
                                    // is - inner sentence
                                    let (start_row, start_col, end_row, end_col) =
                                        self.get_inner_sentence_bounds();
                                    self.visual_start = (start_row, start_col);
                                    self.cursor = (end_row, end_col);
                                    // Keep in character visual mode for sentence selection
                                    self.mode = EditorMode::Visual;
                                    true
                                }
                                ('a', 's') => {
                                    // as - a sentence (includes trailing whitespace)
                                    let (start_row, start_col, end_row, end_col) =
                                        self.get_a_sentence_bounds();
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
                                    self.cursor.1 =
                                        self.lines[self.cursor.0].chars().count().saturating_sub(1);
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
                                'd' | 'x' => {
                                    self.delete_visual_selection();
                                    self.mode = EditorMode::Normal;
                                }
                                'y' => {
                                    self.yank_visual_selection();
                                    self.mode = EditorMode::Normal;
                                }
                                'c' => {
                                    self.delete_visual_selection();
                                    self.mode = EditorMode::Insert;
                                    self.insert_buffer.clear();
                                }
                                // Toggle case
                                '~' => {
                                    let (start, end) = self.get_visual_selection();
                                    self.save_undo_state();
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
                    _ => {}
                },
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
        history: Vec<(Vec<String>, (usize, usize))>,
        history_idx: usize,
        lines_version: u64,
        history_version: u64,
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
                history: vec![(lines, (0, 0))],
                history_idx: 0,
                lines_version: 0,
                history_version: 0,
            }
        }

        fn with_cursor(mut self, row: usize, col: usize) -> Self {
            self.cursor = (row, col);
            self
        }

        fn text(&self) -> String {
            self.lines.join("\n")
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
                // For change operations (Insert mode), don't record yet
                if self.mode != EditorMode::Insert {
                    self.record_change();
                }
            }
        }

        /// Record change only if not in Insert mode
        fn maybe_record_change(&mut self) {
            if self.mode != EditorMode::Insert {
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
            delete_empty_lines: bool,
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
        let end = editor.get_word_forward_pos(WordType::Word);
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
        let end = editor.get_word_forward_pos(WordType::Word);
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
}
