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
            selection_bg: colors.selection_bg.map_or(
                ColorAttribute::PaletteIndex(AnsiColor::Navy.into()),
                |c| ColorAttribute::TrueColorWithDefaultFallback(c.into()),
            ),
            selection_fg: colors.selection_fg.map_or(
                ColorAttribute::PaletteIndex(AnsiColor::White.into()),
                |c| ColorAttribute::TrueColorWithDefaultFallback(c.into()),
            ),
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
    DeleteChar,                           // x
    DeleteLine,                           // dd
    DeleteWord,                           // dw
    DeleteLongWord,                       // dW
    DeleteToEndOfLine,                    // D
    DeleteInnerWord,                      // diw
    DeleteAWord,                          // daw
    DeleteInnerLongWord,                  // diW
    DeleteALongWord,                      // daW
    DeleteInnerPair(char),                // di( di{ etc.
    DeleteAroundPair(char),               // da( da{ etc.
    DeleteToChar(char, bool),             // df{char}, dt{char} (inclusive flag)
    DeleteBackToChar(char, bool),         // dF{char}, dT{char} (inclusive flag)
    SubstituteLine,                       // S, cc
    SubstituteChar,                       // s
    ChangeToEndOfLine,                    // C
    ChangeInnerWord,                      // ciw
    ChangeAWord,                          // caw
    ChangeInnerLongWord,                  // ciW
    ChangeALongWord,                      // caW
    ChangeInnerPair(char),                // ci( ci{ etc.
    ChangeAroundPair(char),               // ca( ca{ etc.
    ChangeToChar(char, bool),             // cf{char}, ct{char}
    ChangeBackToChar(char, bool),         // cF{char}, cT{char}
    InsertText(String, InsertStyle),      // Text inserted in insert mode
    ToggleCase,                           // ~
    JoinLines,                            // J
    ReplaceChar(char),                    // r{char}
}

struct EditorState<'a> {
    args: &'a InputText,
    window: GuiWin,
    pane: MuxPane,
    lines: Vec<String>,
    lines_modified: bool, // Track if lines changed since last record
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
    insert_buffer: String,   // Buffer to track text inserted in insert mode
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
            lines_modified: false,
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
        // If lines haven't changed, just update cursor in last entry (O(1) check)
        if !self.lines_modified {
            if let Some(entry) = self.history.last_mut() {
                entry.1 = self.cursor;
            }
            return;
        }
        self.history.push((self.lines.clone(), self.cursor));
        self.history_idx = self.history.len() - 1;
        self.lines_modified = false; // Reset flag after recording
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
        self.lines_modified = true;
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
            self.lines_modified = true;
            // Only record change if not in insert mode (batch insert mode changes)
            if self.mode != EditorMode::Insert {
                self.record_change();
            }
        }
    }

    fn insert_newline(&mut self) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        let (before, after): (String, String) = if self.cursor.1 < chars.len() {
            (chars[..self.cursor.1].iter().collect(), chars[self.cursor.1..].iter().collect())
        } else {
            (chars.into_iter().collect(), String::new())
        };
        self.lines[self.cursor.0] = before;
        self.lines.insert(self.cursor.0 + 1, after);
        self.cursor.0 += 1;
        self.cursor.1 = 0;
        self.lines_modified = true;
        // Only record change if not in insert mode (batch insert mode changes)
        if self.mode != EditorMode::Insert {
        self.record_change();
        }
    }

    fn delete_line(&mut self) {
        self.lines_modified = true;
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
        // Store deleted lines in yank buffer
        let yanked: Vec<&str> = self.lines[self.cursor.0..].iter().map(|s| s.as_str()).collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        
        self.lines_modified = true;
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
        // Store deleted lines in yank buffer
        let yanked: Vec<&str> = self.lines[..=self.cursor.0].iter().map(|s| s.as_str()).collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
        
        self.lines_modified = true;
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

    fn change_to_start_of_file(&mut self) {
        // Delete from start of file to current line, insert blank line for typing
        self.lines_modified = true;
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
        self.lines_modified = true;
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
        let yanked: Vec<&str> = self.lines[self.cursor.0..].iter().map(|s| s.as_str()).collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
    }

    fn yank_to_start_of_file(&mut self) {
        // Yank from start of file to current line (linewise)
        let yanked: Vec<&str> = self.lines[..=self.cursor.0].iter().map(|s| s.as_str()).collect();
        self.yank_buffer = yanked.join("\n");
        self.yank_is_linewise = true;
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

    fn get_word_forward_pos(&self) -> (usize, usize) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 >= chars.len() {
            if self.cursor.0 < self.lines.len() - 1 {
                return (self.cursor.0 + 1, 0);
            }
            return self.cursor;
        }

        let mut idx = self.cursor.1;

        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
        }

        if idx < chars.len() && !chars[idx].is_whitespace() && !chars[idx].is_ascii_punctuation() {
            while idx < chars.len()
                && !chars[idx].is_whitespace()
                && !chars[idx].is_ascii_punctuation()
            {
                idx += 1;
            }
        } else if idx < chars.len() && chars[idx].is_ascii_punctuation() {
            while idx < chars.len() && chars[idx].is_ascii_punctuation() {
                idx += 1;
            }
        }

        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
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

    fn move_word_forward(&mut self) {
        self.cursor = self.get_word_forward_pos();
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn get_word_backward_pos(&self) -> (usize, usize) {
        if self.cursor.1 == 0 {
            if self.cursor.0 > 0 {
                // Go to previous line and find start of last word
                let prev_line = self.cursor.0 - 1;
                let chars: Vec<char> = self.lines[prev_line].chars().collect();
                if chars.is_empty() {
                    return (prev_line, 0);
                }
                
                let mut idx = chars.len() - 1;
                
                // Skip trailing whitespace
                while idx > 0 && chars[idx].is_whitespace() {
                    idx -= 1;
                }
                
                if idx == 0 {
                    return (prev_line, 0);
                }
                
                // Find start of word
                let start_type_is_word = !chars[idx].is_whitespace() && !chars[idx].is_ascii_punctuation();
                let start_type_is_punct = chars[idx].is_ascii_punctuation();
                
                while idx > 0 {
                    let prev = idx - 1;
                    let prev_is_word = !chars[prev].is_whitespace() && !chars[prev].is_ascii_punctuation();
                    let prev_is_punct = chars[prev].is_ascii_punctuation();
                    
                    if start_type_is_word && !prev_is_word {
                        break;
                    }
                    if start_type_is_punct && !prev_is_punct {
                        break;
                    }
                    if chars[prev].is_whitespace() {
                        break;
                    }
                    
                    idx -= 1;
                }
                
                return (prev_line, idx);
            }
            return self.cursor;
        }

        let mut idx = self.cursor.1;
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();

        idx -= 1;
        while idx > 0 && chars[idx].is_whitespace() {
            idx -= 1;
        }

        if idx == 0 && chars[idx].is_whitespace() {
            if self.cursor.0 > 0 {
                // Go to previous line and find start of last word
                let prev_line = self.cursor.0 - 1;
                let prev_chars: Vec<char> = self.lines[prev_line].chars().collect();
                if prev_chars.is_empty() {
                    return (prev_line, 0);
                }
                
                let mut prev_idx = prev_chars.len() - 1;
                
                // Skip trailing whitespace
                while prev_idx > 0 && prev_chars[prev_idx].is_whitespace() {
                    prev_idx -= 1;
                }
                
                if prev_idx == 0 {
                    return (prev_line, 0);
                }
                
                // Find start of word
                let start_type_is_word = !prev_chars[prev_idx].is_whitespace() && !prev_chars[prev_idx].is_ascii_punctuation();
                let start_type_is_punct = prev_chars[prev_idx].is_ascii_punctuation();
                
                while prev_idx > 0 {
                    let prev = prev_idx - 1;
                    let prev_is_word = !prev_chars[prev].is_whitespace() && !prev_chars[prev].is_ascii_punctuation();
                    let prev_is_punct = prev_chars[prev].is_ascii_punctuation();
                    
                    if start_type_is_word && !prev_is_word {
                        break;
                    }
                    if start_type_is_punct && !prev_is_punct {
                        break;
                    }
                    if prev_chars[prev].is_whitespace() {
                        break;
                    }
                    
                    prev_idx -= 1;
                }
                
                return (prev_line, prev_idx);
            }
            return self.cursor;
        }

        let start_type_is_word = !chars[idx].is_whitespace() && !chars[idx].is_ascii_punctuation();
        let start_type_is_punct = chars[idx].is_ascii_punctuation();

        while idx > 0 {
            let prev = idx - 1;
            let prev_is_word = !chars[prev].is_whitespace() && !chars[prev].is_ascii_punctuation();
            let prev_is_punct = chars[prev].is_ascii_punctuation();

            if start_type_is_word && !prev_is_word {
                break;
            }
            if start_type_is_punct && !prev_is_punct {
                break;
            }
            if chars[prev].is_whitespace() {
                break;
            }

            idx -= 1;
        }

        (self.cursor.0, idx)
    }

    fn move_word_backward(&mut self) {
        self.cursor = self.get_word_backward_pos();
        self.update_desired_col();
    }

    fn get_word_end_pos(&self) -> (usize, usize) {
        // Recursive logic replacement for simplicity in non-mutable context
        let mut curr = self.cursor;
        loop {
            let current_line = &self.lines[curr.0];
            let chars: Vec<char> = current_line.chars().collect();

            if curr.1 >= chars.len().saturating_sub(1) {
                if curr.0 < self.lines.len() - 1 {
                    curr = (curr.0 + 1, 0);
                    // Check if next line is empty or starts with something to skip?
                    // Original recursive logic simply called itself.
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

            let start_type_is_word =
                !chars[idx].is_whitespace() && !chars[idx].is_ascii_punctuation();
            let start_type_is_punct = chars[idx].is_ascii_punctuation();

            while idx < chars.len() {
                let next = idx + 1;
                if next >= chars.len() {
                    break;
                }
                let next_is_word =
                    !chars[next].is_whitespace() && !chars[next].is_ascii_punctuation();
                let next_is_punct = chars[next].is_ascii_punctuation();

                if start_type_is_word && !next_is_word {
                    break;
                }
                if start_type_is_punct && !next_is_punct {
                    break;
                }
                if chars[next].is_whitespace() {
                    break;
                }

                idx += 1;
            }
            return (curr.0, idx);
        }
    }

    fn move_to_word_end(&mut self) {
        self.cursor = self.get_word_end_pos();
        self.update_desired_col();
    }

    fn get_long_word_forward_pos(&self) -> (usize, usize) {
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if self.cursor.1 >= chars.len() {
            if self.cursor.0 < self.lines.len() - 1 {
                return (self.cursor.0 + 1, 0);
            }
            return self.cursor;
        }

        let mut idx = self.cursor.1;

        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
        }
        while idx < chars.len() && !chars[idx].is_whitespace() {
            idx += 1;
        }
        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
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

    fn move_long_word_forward(&mut self) {
        self.cursor = self.get_long_word_forward_pos();
        self.clamp_cursor();
        self.update_desired_col();
    }

    fn get_long_word_backward_pos(&self) -> (usize, usize) {
        if self.cursor.1 == 0 {
            if self.cursor.0 > 0 {
                // Go to previous line and find start of last WORD
                let prev_line = self.cursor.0 - 1;
                let chars: Vec<char> = self.lines[prev_line].chars().collect();
                if chars.is_empty() {
                    return (prev_line, 0);
                }
                
                let mut idx = chars.len() - 1;
                
                // Skip trailing whitespace
                while idx > 0 && chars[idx].is_whitespace() {
                    idx -= 1;
                }
                
                if idx == 0 {
                    return (prev_line, 0);
                }
                
                // Find start of WORD (non-whitespace sequence)
                while idx > 0 {
                    let prev = idx - 1;
                    if chars[prev].is_whitespace() {
                        break;
                    }
                    idx -= 1;
                }
                
                return (prev_line, idx);
            }
            return self.cursor;
        }

        let mut idx = self.cursor.1;
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();

        idx -= 1;
        while idx > 0 && chars[idx].is_whitespace() {
            idx -= 1;
        }

        if idx == 0 && chars[idx].is_whitespace() {
            if self.cursor.0 > 0 {
                // Go to previous line and find start of last WORD
                let prev_line = self.cursor.0 - 1;
                let prev_chars: Vec<char> = self.lines[prev_line].chars().collect();
                if prev_chars.is_empty() {
                    return (prev_line, 0);
                }
                
                let mut prev_idx = prev_chars.len() - 1;
                
                // Skip trailing whitespace
                while prev_idx > 0 && prev_chars[prev_idx].is_whitespace() {
                    prev_idx -= 1;
                }
                
                if prev_idx == 0 {
                    return (prev_line, 0);
                }
                
                // Find start of WORD
                while prev_idx > 0 {
                    let prev = prev_idx - 1;
                    if prev_chars[prev].is_whitespace() {
                        break;
                    }
                    prev_idx -= 1;
                }
                
                return (prev_line, prev_idx);
            }
            return self.cursor;
        }

        while idx > 0 {
            let prev = idx - 1;
            if chars[prev].is_whitespace() {
                break;
            }
            idx -= 1;
        }

        (self.cursor.0, idx)
    }

    fn move_long_word_backward(&mut self) {
        self.cursor = self.get_long_word_backward_pos();
        self.update_desired_col();
    }

    fn get_long_word_end_pos(&self) -> (usize, usize) {
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

            while idx < chars.len() {
                let next = idx + 1;
                if next >= chars.len() {
                    break;
                }
                if chars[next].is_whitespace() {
                    break;
                }
                idx += 1;
            }

            return (curr.0, idx);
        }
    }

    fn move_to_long_word_end(&mut self) {
        self.cursor = self.get_long_word_end_pos();
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

    fn get_line_end_pos(&self) -> (usize, usize) {
        (self.cursor.0, self.lines[self.cursor.0].chars().count())
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
        let is_word_char =
            |c: char| !c.is_whitespace() && !c.is_ascii_punctuation();
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

    fn delete_inner_word(&mut self) {
        let (start, end) = self.get_inner_word_bounds();
        let line = &mut self.lines[self.cursor.0];
        if start < end && end <= line.len() {
            // Store deleted text in yank buffer
            self.yank_buffer = line[start..end].to_string();
            self.yank_is_linewise = false;
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
            self.update_desired_col();
            self.record_change();
        }
    }

    fn delete_a_word(&mut self) {
        let (start, end) = self.get_a_word_bounds();
        let line = &mut self.lines[self.cursor.0];
        if start < end && end <= line.len() {
            // Store deleted text in yank buffer
            self.yank_buffer = line[start..end].to_string();
            self.yank_is_linewise = false;
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
            self.update_desired_col();
            self.record_change();
        }
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
        let line = &mut self.lines[self.cursor.0];
        if start < end && end <= line.len() {
            // Store deleted text in yank buffer
            self.yank_buffer = line[start..end].to_string();
            self.yank_is_linewise = false;
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
            self.update_desired_col();
            self.record_change();
        }
    }

    fn delete_a_long_word(&mut self) {
        let (start, end) = self.get_a_long_word_bounds();
        let line = &mut self.lines[self.cursor.0];
        if start < end && end <= line.len() {
            // Store deleted text in yank buffer
            self.yank_buffer = line[start..end].to_string();
            self.yank_is_linewise = false;
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
            self.update_desired_col();
            self.record_change();
        }
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

    fn find_matching_open(&self, open: char, close: char, start_row: usize, start_col: usize) -> Option<(usize, usize)> {
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

    fn find_matching_close(&self, open: char, close: char, start_row: usize, start_col: usize) -> Option<(usize, usize)> {
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
        if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds(pair_char) {
            // Store content to be deleted in yank buffer (reuse yank logic)
            self.yank_inner_pair(pair_char);
            
            self.lines_modified = true;
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
            self.record_change();
        }
    }

    fn delete_around_pair(&mut self, pair_char: char) {
        if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds(pair_char) {
            // Store content to be deleted in yank buffer (reuse yank logic)
            self.yank_around_pair(pair_char);
            
            self.lines_modified = true;
            if open_row == close_row {
                // Same line - simple case
                let line = &mut self.lines[open_row];
                line.replace_range(open_col..=close_col, "");
                self.cursor.0 = open_row;
                self.cursor.1 = open_col;
            } else {
                // Multi-line deletion
                // Keep content before open bracket on first line
                let first_line_prefix: String = self.lines[open_row].chars().take(open_col).collect();
                // Keep content after close bracket on last line
                let last_line_suffix: String = self.lines[close_row].chars().skip(close_col + 1).collect();

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
            self.record_change();
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
            let mut start_col = col;

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

    fn is_sentence_end(c: char) -> bool {
        matches!(c, '.' | '!' | '?')
    }

    fn move_sentence_backward(&mut self) {
        // Move to the beginning of the current sentence, or if already at the beginning,
        // move to the beginning of the previous sentence (or paragraph boundary)
        let start_row = self.cursor.0;
        let start_col = self.cursor.1;
        let started_on_blank = self.lines[start_row].trim().is_empty();

        // Helper: find sentence start after a given position (after sentence end punctuation)
        let find_sentence_start_after = |lines: &[String], from_row: usize, from_col: usize| -> Option<(usize, usize)> {
            let mut r = from_row;
            let mut c = from_col;
            loop {
                let chars: Vec<char> = lines[r].chars().collect();
                while c < chars.len() && chars[c].is_whitespace() {
                    c += 1;
                }
                if c < chars.len() {
                    return Some((r, c));
                }
                if r < lines.len() - 1 {
                    r += 1;
                    c = 0;
                    if lines[r].trim().is_empty() {
                        return None; // Hit paragraph boundary
                    }
                } else {
                    return None;
                }
            }
        };

        // Helper: find first non-whitespace of a line (paragraph/sentence start)
        let find_line_start = |lines: &[String], r: usize| -> usize {
            let chars: Vec<char> = lines[r].chars().collect();
            chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0)
        };

        // Helper: find the start of a sentence that ends at (end_row, end_col)
        // by searching backward for the previous sentence end or paragraph start
        let find_sentence_start = |lines: &[String], end_row: usize, end_col: usize| -> (usize, usize) {
            let mut r = end_row;
            let mut c = if end_col > 0 { end_col - 1 } else if end_row > 0 {
                r = end_row - 1;
                lines[r].chars().count().saturating_sub(1)
            } else {
                return (0, 0);
            };

            loop {
                let chars: Vec<char> = lines[r].chars().collect();
                loop {
                    if c < chars.len() && Self::is_sentence_end(chars[c]) {
                        // Found previous sentence end, the sentence we want starts after this
                        if let Some(pos) = find_sentence_start_after(lines, r, c + 1) {
                            return pos;
                        }
                    }
                    if c == 0 {
                        break;
                    }
                    c -= 1;
                }

                if r > 0 {
                    if lines[r - 1].trim().is_empty() {
                        // Hit paragraph boundary, sentence starts at beginning of this paragraph
                        return (r, find_line_start(lines, r));
                    }
                    r -= 1;
                    c = lines[r].chars().count().saturating_sub(1);
                } else {
                    return (0, 0);
                }
            }
        };

        // Search backward from cursor position for sentence end or paragraph start
        let mut row = start_row;
        let mut col = if start_col > 0 { start_col - 1 } else if start_row > 0 {
            row = start_row - 1;
            self.lines[row].chars().count().saturating_sub(1)
        } else {
            // At start of file
            self.cursor = (0, 0);
            self.update_desired_col();
            return;
        };

        loop {
            // If we're on a blank line and we didn't start on a blank line, stop here
            if self.lines[row].trim().is_empty() {
                if !started_on_blank {
                    self.cursor = (row, 0);
                    self.update_desired_col();
                    return;
                }
                // Started on blank line, continue searching in previous paragraph
                if row > 0 {
                    row -= 1;
                    col = self.lines[row].chars().count().saturating_sub(1);
                    continue;
                } else {
                    self.cursor = (0, 0);
                    self.update_desired_col();
                    return;
                }
            }

            let chars: Vec<char> = self.lines[row].chars().collect();

            // Search backward through current line
            loop {
                if col < chars.len() && Self::is_sentence_end(chars[col]) {
                    // Found sentence end - find where this sentence starts
                    if let Some((sent_row, sent_col)) = find_sentence_start_after(&self.lines, row, col + 1) {
                        // Check if this sentence start is before our starting position
                        if sent_row < start_row || (sent_row == start_row && sent_col < start_col) {
                            // We were in the middle of this sentence - go to its start
                            self.cursor = (sent_row, sent_col);
                            self.update_desired_col();
                            return;
                        }
                        // We were at or after this sentence start, continue searching backward
                    } else {
                        // This sentence ends at paragraph boundary (last sentence of paragraph)
                        // Find the start of this sentence
                        let (sent_start_row, sent_start_col) = find_sentence_start(&self.lines, row, col);
                        self.cursor = (sent_start_row, sent_start_col);
                        self.update_desired_col();
                        return;
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
                    let para_start_col = find_line_start(&self.lines, row);
                    // Check if we were at the paragraph start (first sentence start)
                    if row == start_row && para_start_col == start_col {
                        // We were at the start of first sentence, go to blank line (like {)
                        self.cursor = (row - 1, 0);
                    } else if row < start_row || (row == start_row && para_start_col < start_col) {
                        // We were in the middle of first sentence, go to paragraph start
                        self.cursor = (row, para_start_col);
                    } else {
                        // Go to blank line
                        self.cursor = (row - 1, 0);
                    }
                    self.update_desired_col();
                    return;
                }
                row -= 1;
                col = self.lines[row].chars().count().saturating_sub(1);
            } else {
                // Reached start of file
                self.cursor = (0, 0);
                self.update_desired_col();
                return;
            }
        }
    }

    fn move_sentence_forward(&mut self) {
        // Move to the beginning of the next sentence
        // When on the last sentence of a paragraph, behave like } (go to blank line)
        // When on a blank line, go to first sentence of next paragraph
        let mut row = self.cursor.0;
        let mut col = self.cursor.1;
        let last_row = self.lines.len().saturating_sub(1);
        let started_on_blank = self.lines[row].trim().is_empty();

        // If starting on a blank line, skip to next paragraph's first sentence
        if started_on_blank {
            while row < last_row && self.lines[row].trim().is_empty() {
                row += 1;
            }
            if self.lines[row].trim().is_empty() {
                // All remaining lines are blank, go to last position
                self.cursor.0 = last_row;
                self.cursor.1 = 0;
                self.update_desired_col();
                return;
            }
            // Find first non-whitespace of this line
            let chars: Vec<char> = self.lines[row].chars().collect();
            let start_col = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0);
            self.cursor = (row, start_col);
            self.update_desired_col();
            return;
        }

        loop {
            let chars: Vec<char> = self.lines[row].chars().collect();

            // Search forward through current line for sentence end
            while col < chars.len() {
                if Self::is_sentence_end(chars[col]) {
                    // Found sentence end, skip whitespace to find start of next sentence
                    col += 1;

                    // Skip whitespace (including across lines)
                    loop {
                        let cur_chars: Vec<char> = self.lines[row].chars().collect();
                        while col < cur_chars.len() && cur_chars[col].is_whitespace() {
                            col += 1;
                        }
                        if col < cur_chars.len() {
                            // Found start of next sentence
                            self.cursor = (row, col);
                            self.update_desired_col();
                            return;
                        }
                        // Move to next line
                        if row < last_row {
                            row += 1;
                            col = 0;
                            // Check if line is blank (paragraph boundary)
                            // Stop at the blank line like } does
                            if self.lines[row].trim().is_empty() {
                                self.cursor = (row, 0);
                                self.update_desired_col();
                                return;
                            }
                        } else {
                            // End of file
                            self.cursor.0 = last_row;
                            self.cursor.1 = self.lines[last_row].chars().count().saturating_sub(1);
                            self.update_desired_col();
                            return;
                        }
                    }
                }
                col += 1;
            }

            // Move to next line
            if row < last_row {
                row += 1;
                col = 0;
                // Check if line is blank (paragraph boundary = sentence boundary)
                // Stop at the blank line like } does
                if self.lines[row].trim().is_empty() {
                    self.cursor = (row, 0);
                    self.update_desired_col();
                    return;
                }
            } else {
                // End of file
                self.cursor.0 = last_row;
                self.cursor.1 = self.lines[last_row].chars().count().saturating_sub(1);
                self.update_desired_col();
                return;
            }
        }
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

    fn delete_range_multiline(&mut self, start: (usize, usize), end: (usize, usize), inclusive: bool) {
        // Delete from start position to end position (multi-line support)
        self.lines_modified = true;
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
                self.lines[start_row] = chars[..start_col].iter().chain(&chars[end_clamped..]).collect();
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
        let start = (self.cursor.0, self.cursor.1);
        if let Some(end) = self.find_unmatched_backward(open, close) {
            self.delete_range_multiline(end, start, false);
        }
    }

    fn delete_to_next_unmatched(&mut self, open: char, close: char) {
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
            LastChange::DeleteWord => {
                self.perform_delete_motion(|s| s.get_word_forward_pos(), false, true);
            }
            LastChange::DeleteLongWord => {
                self.perform_delete_motion(|s| s.get_long_word_forward_pos(), false, true);
            }
            LastChange::DeleteToEndOfLine => self.delete_to_end_of_line(),
            LastChange::DeleteInnerWord => self.delete_inner_word(),
            LastChange::DeleteAWord => self.delete_a_word(),
            LastChange::DeleteInnerLongWord => self.delete_inner_long_word(),
            LastChange::DeleteALongWord => self.delete_a_long_word(),
            LastChange::DeleteInnerPair(c) => self.delete_inner_pair(c),
            LastChange::DeleteAroundPair(c) => self.delete_around_pair(c),
            LastChange::DeleteToChar(c, inclusive) => self.delete_to_char_forward(c, inclusive),
            LastChange::DeleteBackToChar(c, inclusive) => self.delete_to_char_backward(c, inclusive),
            LastChange::SubstituteLine => self.substitute_line(),
            LastChange::SubstituteChar => self.substitute_char(),
            LastChange::ChangeToEndOfLine => self.change_to_end_of_line(),
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
                // Temporarily enter Insert mode to batch changes (no individual recording)
                // Note: We don't record before repeat since current state is already in history
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
                        self.lines_modified = true;
                        self.lines.insert(self.cursor.0 + 1, String::new());
                        self.cursor.0 += 1;
                        self.cursor.1 = 0;
                    }
                    InsertStyle::NewLineAbove => {
                        // O - open new line above current line
                        self.lines_modified = true;
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
        self.record_change(); // Record state before deletion
        self.lines_modified = true;
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
        self.delete_to_end_of_line();
        self.mode = EditorMode::Insert;
        let line_len = self.lines[self.cursor.0].len();
        self.cursor.1 = line_len;
    }

    fn substitute_line(&mut self) {
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
        self.record_change();
    }

    fn substitute_char(&mut self) {
        let line = &mut self.lines[self.cursor.0];
        if !line.is_empty() && self.cursor.1 < line.len() {
            line.remove(self.cursor.1);
            self.mode = EditorMode::Insert;
            self.record_change();
        }
    }

    fn replace_char(&mut self, replacement: char) {
        // r{char} - replace character under cursor without entering insert mode
        let line_len = self.lines[self.cursor.0].len();
        if line_len > 0 && self.cursor.1 < line_len {
            self.record_change();
            self.lines_modified = true;
            let line = &mut self.lines[self.cursor.0];
            let mut chars: Vec<char> = line.chars().collect();
            chars[self.cursor.1] = replacement;
            *line = chars.into_iter().collect();
            self.record_change();
        }
    }

    fn join_lines(&mut self) {
        if self.cursor.0 < self.lines.len() - 1 {
            self.lines_modified = true;
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

    fn render(&mut self) -> termwiz::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        self.buf.add_changes(vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorVisibility(CursorVisibility::Hidden),
        ]);

        // Status bar
        let status_text = match self.mode {
            EditorMode::Normal => {
                format!(
                    " -- NORMAL --  {}:{}",
                    self.cursor.0 + 1,
                    self.cursor.1 + 1
                )
            }
            EditorMode::Insert => {
                format!(
                    " -- INSERT --  {}:{}",
                    self.cursor.0 + 1,
                    self.cursor.1 + 1
                )
            }
            EditorMode::Search => {
                let prompt = match self.search_direction {
                    SearchDirection::Forward => "/",
                    SearchDirection::Backward => "?",
                };
                format!("{}{}", prompt, self.search_input)
            }
            EditorMode::Visual => {
                format!(
                    " -- VISUAL --  {}:{}",
                    self.cursor.0 + 1,
                    self.cursor.1 + 1
                )
            }
            EditorMode::VisualLine => {
                format!(
                    " -- VISUAL LINE --  {}:{}",
                    self.cursor.0 + 1,
                    self.cursor.1 + 1
                )
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
                        Change::Text(if line.is_empty() { " ".to_string() } else { line.clone() }),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);
                } else if line_in_selection && self.mode == EditorMode::Visual {
                    // Character-wise selection
                    let chars: Vec<char> = line.chars().collect();
                    let line_len = chars.len();
                    
                    // Handle empty lines - show a highlighted space like Neovim
                    if line_len == 0 {
                        self.buf.add_changes(vec![
                            Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                            Change::Attribute(AttributeChange::Foreground(self.colors.selection_fg)),
                            Change::Text(" ".to_string()),
                            Change::AllAttributes(CellAttributes::default()),
                        ]);
                    } else {
                        let sel_col_start = if line_idx == sel_start.0 { sel_start.1 } else { 0 };
                        let sel_col_end = if line_idx == sel_end.0 { sel_end.1 + 1 } else { line_len };
                        
                        // Text before selection
                        if sel_col_start > 0 {
                            let before: String = chars[..sel_col_start.min(line_len)].iter().collect();
                            self.buf.add_changes(vec![
                                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                                Change::Text(before),
                            ]);
                        }
                        
                        // Selected text
                        if sel_col_start < line_len {
                            let selected: String = chars[sel_col_start.min(line_len)..sel_col_end.min(line_len)].iter().collect();
                            if !selected.is_empty() {
                                self.buf.add_changes(vec![
                                    Change::Attribute(AttributeChange::Background(self.colors.selection_bg)),
                                    Change::Attribute(AttributeChange::Foreground(self.colors.selection_fg)),
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
                        
                        self.buf.add_changes(vec![Change::AllAttributes(CellAttributes::default())]);
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
            Change::CursorShape(match self.mode {
                EditorMode::Normal => CursorShape::SteadyBlock,
                EditorMode::Insert => CursorShape::SteadyBar,
                EditorMode::Search => CursorShape::SteadyUnderline,
                EditorMode::Visual | EditorMode::VisualLine => CursorShape::SteadyBlock,
            }),
        ]);

        self.buf.flush()?;

        Ok(())
    }

    // Helper to perform delete action based on a motion
    // delete_empty_lines: if true, delete the entire line when backward motion would empty it
    fn perform_delete_motion<F>(&mut self, motion: F, is_inclusive: bool, delete_empty_lines: bool)
    where
        F: Fn(&EditorState) -> (usize, usize),
    {
        // Record state before deletion for undo (preserves cursor position)
        self.record_change();
        self.lines_modified = true;
        let start = self.cursor;
        let end = motion(self);

        // Handle direction - use character-based operations
        if end.0 < start.0 {
            // Backward motion crossing to previous line
            if end.1 == 0 && delete_empty_lines {
                // Deleting from start of line = delete entire previous line (for delete ops)
                // Store deleted line in yank buffer
                self.yank_buffer = self.lines[end.0].clone();
                self.yank_is_linewise = true;
                self.lines.remove(end.0);
                // Cursor stays on same line index (which is now different content)
                // but we need to update row since we removed a line above
                self.cursor.0 = end.0;
                self.cursor.1 = 0;
            } else {
                // Delete from end.1 to end of previous line (or clear line for change ops)
                let mut chars: Vec<char> = self.lines[end.0].chars().collect();
                if end.1 < chars.len() {
                    // Store deleted text in yank buffer
                    self.yank_buffer = chars[end.1..].iter().collect();
                    self.yank_is_linewise = false;
                    chars.drain(end.1..);
                    self.lines[end.0] = chars.into_iter().collect();
                }
                // Move cursor to the deletion point on previous line
                self.cursor.0 = end.0;
                self.cursor.1 = end.1;
            }
        } else if end.0 == start.0 && end.1 < start.1 {
            // Backward motion on same line (db, dB)
            // Delete [end.1, start.1) in character indices
            let range_start = end.1;
            let range_end = start.1;
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
            // Forward motion crossing to next line - delete to end of current line
            let mut chars: Vec<char> = self.lines[start.0].chars().collect();
            if start.1 < chars.len() {
                // Store deleted text in yank buffer
                self.yank_buffer = chars[start.1..].iter().collect();
                self.yank_is_linewise = false;
                chars.drain(start.1..);
                self.lines[start.0] = chars.into_iter().collect();
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
        self.record_change();
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
            // Backward motion crossing to previous line - yank from end.1 to end of previous line
            let chars: Vec<char> = self.lines[end.0].chars().collect();
            if end.1 < chars.len() {
                self.yank_buffer = chars[end.1..].iter().collect();
            }
        } else if end.0 == start.0 && end.1 < start.1 {
            // Backward motion on same line (yb, yB)
            let chars: Vec<char> = self.lines[start.0].chars().collect();
            if end.1 < start.1 && start.1 <= chars.len() {
                self.yank_buffer = chars[end.1..start.1].iter().collect();
            }
        } else if end.0 > start.0 {
            // Forward motion crossing to next line - yank to end of current line
            let chars: Vec<char> = self.lines[start.0].chars().collect();
            if start.1 < chars.len() {
                self.yank_buffer = chars[start.1..].iter().collect();
            }
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
        }
        self.yank_is_linewise = false;
    }

    fn yank_inner_word(&mut self) {
        let (start, end) = self.get_inner_word_bounds();
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if start < end && end <= chars.len() {
            self.yank_buffer = chars[start..end].iter().collect();
            self.yank_is_linewise = false;
        }
    }

    fn yank_a_word(&mut self) {
        let (start, end) = self.get_a_word_bounds();
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if start < end && end <= chars.len() {
            self.yank_buffer = chars[start..end].iter().collect();
            self.yank_is_linewise = false;
        }
    }

    fn yank_inner_long_word(&mut self) {
        let (start, end) = self.get_inner_long_word_bounds();
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if start < end && end <= chars.len() {
            self.yank_buffer = chars[start..end].iter().collect();
            self.yank_is_linewise = false;
        }
    }

    fn yank_a_long_word(&mut self) {
        let (start, end) = self.get_a_long_word_bounds();
        let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
        if start < end && end <= chars.len() {
            self.yank_buffer = chars[start..end].iter().collect();
            self.yank_is_linewise = false;
        }
    }

    fn yank_inner_pair(&mut self, pair_char: char) {
        if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds(pair_char) {
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
        if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds(pair_char) {
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
                let end_col = if inclusive { target_col + 1 } else { target_col };
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
            } else {
                // Multi-line: yank from start to end including brackets
                let (start_pos, end_pos) = if saved_cursor.0 < end.0 || (saved_cursor.0 == end.0 && saved_cursor.1 < end.1) {
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
            }
        }
    }

    fn yank_to_prev_unmatched(&mut self, open: char, close: char) {
        let saved_cursor = self.cursor;
        self.jump_to_prev_unmatched(open, close);
        // Check if cursor moved
        if self.cursor != saved_cursor {
            let target = self.cursor;
            self.cursor = saved_cursor;
            // Yank from target to cursor (exclusive of target position)
            if target.0 == saved_cursor.0 {
                // Same line - use character indices
                let chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
                if target.1 < saved_cursor.1 && saved_cursor.1 <= chars.len() {
                    self.yank_buffer = chars[target.1 + 1..saved_cursor.1].iter().collect();
                    self.yank_is_linewise = false;
                }
            }
            // Multi-line yank not supported for this motion for simplicity
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
        self.record_change();
        self.lines_modified = true;
        if self.yank_buffer.is_empty() {
            return;
        }
        if self.yank_is_linewise {
            // Insert yanked lines below current line
            let new_lines: Vec<String> = self.yank_buffer.lines().map(|s| s.to_string()).collect();
            for (i, line) in new_lines.into_iter().enumerate() {
                self.lines.insert(self.cursor.0 + 1 + i, line);
            }
            // Move cursor to first non-blank of first inserted line
            self.cursor.0 += 1;
            self.cursor.1 = self.get_first_non_blank_in_line(self.cursor.0);
        } else if self.yank_buffer.contains('\n') {
            // Multi-line characterwise paste (e.g., from yi( across lines)
            let paste_lines: Vec<&str> = self.yank_buffer.split('\n').collect();
            let current_line_chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = if current_line_chars.is_empty() { 0 } else { self.cursor.1 + 1 };
            
            // Split current line at insert position
            let before: String = current_line_chars[..insert_pos.min(current_line_chars.len())].iter().collect();
            let after: String = current_line_chars[insert_pos.min(current_line_chars.len())..].iter().collect();
            
            // First part: before + first paste line
            self.lines[self.cursor.0] = before + paste_lines[0];
            
            // Middle lines
            for (i, paste_line) in paste_lines[1..paste_lines.len()-1].iter().enumerate() {
                self.lines.insert(self.cursor.0 + 1 + i, paste_line.to_string());
            }
            
            // Last part: last paste line + after
            if paste_lines.len() > 1 {
                let last_paste_line = paste_lines[paste_lines.len() - 1];
                self.lines.insert(self.cursor.0 + paste_lines.len() - 1, last_paste_line.to_string() + &after);
            }
            
            // Move cursor to end of pasted text (last character of last paste line)
            self.cursor.0 += paste_lines.len() - 1;
            let last_paste_chars = paste_lines[paste_lines.len() - 1].chars().count();
            self.cursor.1 = if last_paste_chars > 0 { last_paste_chars - 1 } else { 0 };
        } else {
            // Single line characterwise paste - use character-based insertion
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            let insert_pos = if chars.is_empty() { 0 } else { (self.cursor.1 + 1).min(chars.len()) };
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
        self.record_change();
        self.lines_modified = true;
        if self.yank_buffer.is_empty() {
            return;
        }
        if self.yank_is_linewise {
            // Insert yanked lines above current line
            let new_lines: Vec<String> = self.yank_buffer.lines().map(|s| s.to_string()).collect();
            for (i, line) in new_lines.into_iter().enumerate() {
                self.lines.insert(self.cursor.0 + i, line);
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
            for (i, paste_line) in paste_lines[1..paste_lines.len()-1].iter().enumerate() {
                self.lines.insert(self.cursor.0 + 1 + i, paste_line.to_string());
            }
            
            // Last part: last paste line + after
            if paste_lines.len() > 1 {
                let last_paste_line = paste_lines[paste_lines.len() - 1];
                self.lines.insert(self.cursor.0 + paste_lines.len() - 1, last_paste_line.to_string() + &after);
            }
            
            // Move cursor to end of pasted text
            self.cursor.0 += paste_lines.len() - 1;
            let last_paste_chars = paste_lines[paste_lines.len() - 1].chars().count();
            self.cursor.1 = if last_paste_chars > 0 { last_paste_chars - 1 } else { 0 };
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
        line.chars()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0)
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
            let search_end = if row == start_row { self.cursor.1 } else { self.lines[row].len() };
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
                // Search from cursor to end on the starting row
                let chars: Vec<char> = self.lines[row].chars().collect();
                if start_col < chars.len() {
                    let after_cursor: String = chars[start_col..].iter().collect();
                    // Get byte offset for start_col
                    let byte_offset: usize = chars[..start_col].iter().map(|c| c.len_utf8()).sum();
                    byte_offset
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
        let (start, end) = self.get_visual_selection();
        
        if self.mode == EditorMode::VisualLine {
            // Delete entire lines
            let yanked: Vec<&str> = self.lines[start.0..=end.0].iter().map(|s| s.as_str()).collect();
            self.yank_buffer = yanked.join("\n");
            self.yank_is_linewise = true;
            
            self.lines_modified = true;
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
                
                self.lines_modified = true;
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
                
                self.lines_modified = true;
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
            let yanked: Vec<&str> = self.lines[start.0..=end.0].iter().map(|s| s.as_str()).collect();
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
                                    '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | '`'
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
                                    '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | '`'
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
                                        // cw is equivalent to ce in Vim (change to end of word)
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(|s| s.get_word_end_pos(), true, false);
                                    }
                                    'W' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_long_word_end_pos(),
                                            true,
                                            false,
                                        );
                                    }
                                    'e' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(|s| s.get_word_end_pos(), true, false);
                                    }
                                    'E' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_long_word_end_pos(),
                                            true,
                                            false,
                                        );
                                    }
                                    'b' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_word_backward_pos(),
                                            false,
                                            false,
                                        );
                                    }
                                    'B' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_long_word_backward_pos(),
                                            false,
                                            false,
                                        );
                                    }
                                    '$' => self.change_to_end_of_line(),
                                    '^' => {
                                        self.mode = EditorMode::Insert;
                                        self.perform_delete_motion(
                                            |s| s.get_first_non_blank_pos(),
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
                                        );
                                    }
                                    'G' => self.change_to_end_of_file(),
                                    'g' => {
                                        // Wait for second 'g' to complete 'cgg'
                                        self.pending_keys.push(KeyCode::Char('g'));
                                        self.pending_operator = Some('c');
                                        continue;
                                    }
                                    'i' | 'a' => {
                                        // Wait for text object (e.g., 'w' for ciw/caw)
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('c');
                                        continue;
                                    }
                                    '%' => {
                                        self.delete_to_matching_bracket();
                                        self.mode = EditorMode::Insert;
                                    }
                                    '[' | ']' | 'f' | 'F' | 't' | 'T' => {
                                        // Wait for target char/bracket
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('c');
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
                                        self.perform_delete_motion(|s| s.get_word_forward_pos(), false, true);
                                        self.last_change = LastChange::DeleteWord;
                                    }
                                    'W' => {
                                        self.perform_delete_motion(|s| s.get_long_word_forward_pos(), false, true);
                                        self.last_change = LastChange::DeleteLongWord;
                                    }
                                    'e' => {
                                        self.perform_delete_motion(|s| s.get_word_end_pos(), true, true)
                                    }
                                    'E' => self
                                        .perform_delete_motion(|s| s.get_long_word_end_pos(), true, true),
                                    'b' => self.perform_delete_motion(
                                        |s| s.get_word_backward_pos(),
                                        false,
                                        true,
                                    ),
                                    'B' => self.perform_delete_motion(
                                        |s| s.get_long_word_backward_pos(),
                                        false,
                                        true,
                                    ),
                                    '$' => self.delete_to_end_of_line(),
                                    '^' => self.perform_delete_motion(
                                        |s| s.get_first_non_blank_pos(),
                                        false,
                                        true,
                                    ),
                                    '0' => self.perform_delete_motion(
                                        |s| s.get_line_start_pos(),
                                        false,
                                        true,
                                    ),
                                    'G' => self.delete_to_end_of_file(),
                                    'g' => {
                                        // Wait for second 'g' to complete 'dgg'
                                        self.pending_keys.push(KeyCode::Char('g'));
                                        self.pending_operator = Some('d');
                                        continue;
                                    }
                                    'i' | 'a' => {
                                        // Wait for text object (e.g., 'w' for diw/daw)
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('d');
                                        continue;
                                    }
                                    '%' => self.delete_to_matching_bracket(),
                                    '[' | ']' | 'f' | 'F' | 't' | 'T' => {
                                        // Wait for target char/bracket
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('d');
                                        continue;
                                    }
                                    _ => {}
                                }
                            } else if op == 'y' {
                                match c {
                                    'y' => self.yank_line(),
                                    'w' => self.perform_yank_motion(|s| s.get_word_forward_pos(), false),
                                    'W' => self.perform_yank_motion(|s| s.get_long_word_forward_pos(), false),
                                    'e' => self.perform_yank_motion(|s| s.get_word_end_pos(), true),
                                    'E' => self.perform_yank_motion(|s| s.get_long_word_end_pos(), true),
                                    'b' => self.perform_yank_motion(|s| s.get_word_backward_pos(), false),
                                    'B' => self.perform_yank_motion(|s| s.get_long_word_backward_pos(), false),
                                    '$' => self.yank_to_end_of_line(),
                                    '^' => self.perform_yank_motion(|s| s.get_first_non_blank_pos(), false),
                                    '0' => self.perform_yank_motion(|s| s.get_line_start_pos(), false),
                                    'G' => self.yank_to_end_of_file(),
                                    'g' => {
                                        // Wait for second 'g' to complete 'ygg'
                                        self.pending_keys.push(KeyCode::Char('g'));
                                        self.pending_operator = Some('y');
                                        continue;
                                    }
                                    'i' | 'a' => {
                                        // Wait for text object (e.g., 'w' for yiw/yaw)
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('y');
                                        continue;
                                    }
                                    '%' => self.yank_to_matching_bracket(),
                                    '[' | ']' | 'f' | 'F' | 't' | 'T' => {
                                        // Wait for target char/bracket
                                        self.pending_keys.push(KeyCode::Char(c));
                                        self.pending_operator = Some('y');
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

                        if c == 'g' || c == 'Z' || c == '[' || c == ']'
                            || c == 'f' || c == 'F' || c == 't' || c == 'T'
                            || c == 'r'
                        {
                            self.pending_keys.push(KeyCode::Char(c));
                            continue;
                        }

                        if c == 'c' || c == 'd' || c == 'y' {
                            self.pending_operator = Some(c);
                            continue;
                        }

                        match c {
                            'i' => {
                                self.record_change(); // Record state with current cursor before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::Before;
                                self.mode = EditorMode::Insert;
                            }
                            'I' => {
                                self.record_change(); // Record state with current cursor before insert
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
                                self.record_change(); // Record state with current cursor before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::After;
                                self.mode = EditorMode::Insert;
                                self.move_cursor(0, 1);
                            }
                            'A' => {
                                self.record_change(); // Record state with current cursor before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::LineEnd;
                                self.cursor.1 = self.lines[self.cursor.0].len();
                                self.mode = EditorMode::Insert;
                            }
                            'o' => {
                                self.record_change(); // Record state with current cursor before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::NewLineBelow;
                                self.lines_modified = true;
                                self.lines.insert(self.cursor.0 + 1, String::new());
                                self.cursor.0 += 1;
                                self.cursor.1 = 0;
                                self.mode = EditorMode::Insert;
                            }
                            'O' => {
                                self.record_change(); // Record state with current cursor before insert
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_style = InsertStyle::NewLineAbove;
                                self.lines_modified = true;
                                self.lines.insert(self.cursor.0, String::new());
                                self.cursor.1 = 0;
                                self.mode = EditorMode::Insert;
                            }
                            'h' => self.move_cursor(0, -1),
                            'j' => self.move_cursor(1, 0),
                            'k' => self.move_cursor(-1, 0),
                            'l' => self.move_cursor(0, 1),
                            'w' => self.move_word_forward(),
                            'W' => self.move_long_word_forward(),
                            'e' => self.move_to_word_end(),
                            'E' => self.move_to_long_word_end(),
                            'b' => self.move_word_backward(),
                            'B' => self.move_long_word_backward(),
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
                                self.cursor.1 = self.lines[self.cursor.0].chars().count().saturating_sub(1);
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
                                self.repeat_char_search(true);  // Opposite direction
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
                                self.search_word_under_cursor(true);  // Forward
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
                                LastChange::ChangeInnerWord
                                | LastChange::ChangeAWord
                                | LastChange::ChangeInnerLongWord
                                | LastChange::ChangeALongWord
                                | LastChange::ChangeInnerPair(_)
                                | LastChange::ChangeAroundPair(_)
                                | LastChange::ChangeToChar(_, _)
                                | LastChange::ChangeBackToChar(_, _)
                                | LastChange::ChangeToEndOfLine
                                | LastChange::SubstituteLine
                                | LastChange::SubstituteChar => {
                                    // Keep the change operation as last_change
                                }
                                _ => {
                                    self.last_change = LastChange::InsertText(self.insert_buffer.clone(), self.insert_style);
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
                            self.lines_modified = true;
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
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('(') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '(' | ')') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('(') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '[' | ']') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('[') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '[' | ']') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('[') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '{' | '}') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('{') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '{' | '}') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('{') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '<' | '>') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('<') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '<' | '>') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('<') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '"') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('"') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '"') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('"') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '\'') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('\'') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '\'') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('\'') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', '`') => {
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('`') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', '`') => {
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('`') {
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
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('(') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', 'b') => {
                                    // ab is same as a(
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('(') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
                                    true
                                }
                                ('i', 'B') => {
                                    // iB is same as i{
                                    if let Some((open_pos, close_pos)) = self.find_pair_bounds('{') {
                                        let (start, end) = self.get_inner_pair_visual_bounds(open_pos, close_pos);
                                        self.visual_start = start;
                                        self.cursor = end;
                                    }
                                    true
                                }
                                ('a', 'B') => {
                                    // aB is same as a{
                                    if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds('{') {
                                        self.visual_start = (open_row, open_col);
                                        self.cursor = (close_row, close_col);
                                    }
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
                        'w' => self.move_word_forward(),
                        'W' => self.move_long_word_forward(),
                        'b' => self.move_word_backward(),
                        'B' => self.move_long_word_backward(),
                        'e' => self.move_to_word_end(),
                        'E' => self.move_to_long_word_end(),
                        '0' => {
                            self.cursor.1 = 0;
                            self.update_desired_col();
                        }
                        '^' => self.move_to_first_non_blank(),
                        '$' => {
                            self.cursor.1 = self.lines[self.cursor.0].chars().count().saturating_sub(1);
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
                            self.lines_modified = true;
                            if self.mode == EditorMode::VisualLine {
                                for row in start.0..=end.0 {
                                    let toggled: String = self.lines[row].chars().map(|c| {
                                        if c.is_uppercase() { c.to_lowercase().next().unwrap_or(c) }
                                        else { c.to_uppercase().next().unwrap_or(c) }
                                    }).collect();
                                    self.lines[row] = toggled;
                                }
                            } else {
                                // Character-wise toggle
                                for row in start.0..=end.0 {
                                    let chars: Vec<char> = self.lines[row].chars().collect();
                                    let col_start = if row == start.0 { start.1 } else { 0 };
                                    let col_end = if row == end.0 { (end.1 + 1).min(chars.len()) } else { chars.len() };
                                    let toggled: String = chars.iter().enumerate().map(|(i, &c)| {
                                        if i >= col_start && i < col_end {
                                            if c.is_uppercase() { c.to_lowercase().next().unwrap_or(c) }
                                            else { c.to_uppercase().next().unwrap_or(c) }
                                        } else { c }
                                    }).collect();
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
        lines_modified: bool,
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
                lines_modified: false,
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
            if self.lines_modified {
                // Trim future history if we made new changes after undo
                if self.history_idx < self.history.len() - 1 {
                    self.history.truncate(self.history_idx + 1);
                }
                self.history.push((self.lines.clone(), self.cursor));
                self.history_idx = self.history.len() - 1;
                self.lines_modified = false;
            } else if let Some(last) = self.history.last_mut() {
                // Update cursor position in the last history entry
                last.1 = self.cursor;
            }
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
            self.lines_modified = true;
        }

        fn delete_char(&mut self) {
            let mut chars: Vec<char> = self.lines[self.cursor.0].chars().collect();
            if self.cursor.1 < chars.len() {
                chars.remove(self.cursor.1);
                self.lines[self.cursor.0] = chars.into_iter().collect();
                self.lines_modified = true;
                self.clamp_cursor();
            }
        }

        fn delete_line(&mut self, row: usize) {
            if self.lines.len() > 1 && row < self.lines.len() {
                self.lines.remove(row);
                self.lines_modified = true;
                self.clamp_cursor();
            } else if self.lines.len() == 1 {
                self.lines[0].clear();
                self.cursor = (0, 0);
                self.lines_modified = true;
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
                self.record_change();
                self.lines_modified = true;

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
                self.record_change();
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
        editor.lines_modified = true;
        editor.record_change();
        assert_eq!(editor.text(), "Xhello");

        editor.undo();
        assert_eq!(editor.text(), "hello");
    }

    #[test]
    fn test_redo_after_undo() {
        let mut editor = TestEditor::new("hello");
        editor.insert_char('X');
        editor.lines_modified = true;
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
}

