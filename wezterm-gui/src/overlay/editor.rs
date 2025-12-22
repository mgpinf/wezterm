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
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum EditorMode {
    Normal,
    Insert,
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
    InsertText(String, bool),             // Text inserted in insert mode (text, is_after)
    ToggleCase,                           // ~
    JoinLines,                            // J
}

struct EditorState<'a> {
    args: &'a InputText,
    window: GuiWin,
    pane: MuxPane,
    lines: Vec<String>,
    cursor: (usize, usize), // row, col
    mode: EditorMode,
    colors: EditorColors,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
    history: Vec<(Vec<String>, (usize, usize))>,
    history_idx: usize,
    viewport_top: usize,
    pending_keys: Vec<KeyCode>,
    pending_operator: Option<char>, // 'd', 'c', 'y'
    last_change: LastChange,
    insert_buffer: String, // Buffer to track text inserted in insert mode
    insert_after: bool,    // True if insert was via 'a'/'A', false for 'i'/'I'
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
            cursor: (0, 0),
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
            insert_after: false,
        }
    }

    fn record_change(&mut self) {
        // Truncate redo history
        if self.history_idx < self.history.len() - 1 {
            self.history.truncate(self.history_idx + 1);
        }
        self.history.push((self.lines.clone(), self.cursor));
        self.history_idx = self.history.len() - 1;
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
            self.history_idx += 1;
            let (lines, cursor) = &self.history[self.history_idx];
            self.lines = lines.clone();
            self.cursor = *cursor;
        }
    }

    fn move_cursor(&mut self, row: isize, col: isize) {
        let new_row = (self.cursor.0 as isize + row)
            .max(0)
            .min((self.lines.len() - 1) as isize) as usize;
        let line_len = self.lines[new_row].len();
        let max_col = if self.mode == EditorMode::Insert {
            line_len
        } else {
            line_len.saturating_sub(1)
        };

        let new_col = (self.cursor.1 as isize + col).max(0).min(max_col as isize) as usize;
        self.cursor = (new_row, new_col);
    }

    fn clamp_cursor(&mut self) {
        let line_len = self.lines[self.cursor.0].len();
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
        let line = &mut self.lines[self.cursor.0];
        if self.cursor.1 >= line.len() {
            line.push(c);
        } else {
            line.insert(self.cursor.1, c);
        }
        self.cursor.1 += 1;
        self.record_change();
    }

    fn delete_char(&mut self) {
        let line = &mut self.lines[self.cursor.0];
        if !line.is_empty() && self.cursor.1 < line.len() {
            line.remove(self.cursor.1);
            self.clamp_cursor();
            self.record_change();
        }
    }

    fn insert_newline(&mut self) {
        let line = &mut self.lines[self.cursor.0];
        let rest = if self.cursor.1 < line.len() {
            line.split_off(self.cursor.1)
        } else {
            String::new()
        };
        self.lines.insert(self.cursor.0 + 1, rest);
        self.cursor.0 += 1;
        self.cursor.1 = 0;
        self.record_change();
    }

    fn delete_line(&mut self) {
        if self.lines.len() > 1 {
            self.lines.remove(self.cursor.0);
            if self.cursor.0 >= self.lines.len() {
                self.cursor.0 = self.lines.len() - 1;
            }
            self.clamp_cursor();
            self.record_change();
        } else {
            self.lines[0].clear();
            self.cursor.1 = 0;
            self.record_change();
        }
    }

    fn delete_to_end_of_file(&mut self) {
        // Delete from current line to end of file (linewise)
        self.lines.truncate(self.cursor.0 + 1);
        self.lines[self.cursor.0].clear();
        if self.cursor.0 > 0 && self.lines[self.cursor.0].is_empty() {
            self.lines.remove(self.cursor.0);
            self.cursor.0 -= 1;
        }
        // Preserve column position, clamp if line is shorter
        self.clamp_cursor();
        self.record_change();
    }

    fn delete_to_start_of_file(&mut self) {
        // Delete from start of file to current line (linewise)
        for _ in 0..=self.cursor.0 {
            self.lines.remove(0);
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor.0 = 0;
        // Preserve column position, clamp if line is shorter
        self.clamp_cursor();
        self.record_change();
    }

    fn change_to_start_of_file(&mut self) {
        // Delete from start of file to current line, insert blank line for typing
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
        self.lines.truncate(self.cursor.0);
        self.lines.push(String::new());
        self.cursor.0 = self.lines.len() - 1;
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
        self.record_change();
    }

    fn get_word_forward_pos(&self) -> (usize, usize) {
        let current_line = &self.lines[self.cursor.0];
        if self.cursor.1 >= current_line.len() {
            if self.cursor.0 < self.lines.len() - 1 {
                return (self.cursor.0 + 1, 0);
            }
            return self.cursor;
        }

        let mut idx = self.cursor.1;
        let chars: Vec<char> = current_line.chars().collect();

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
    }

    fn get_word_backward_pos(&self) -> (usize, usize) {
        if self.cursor.1 == 0 {
            if self.cursor.0 > 0 {
                return (
                    self.cursor.0 - 1,
                    self.lines[self.cursor.0 - 1].len().saturating_sub(1),
                );
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
                return (
                    self.cursor.0 - 1,
                    self.lines[self.cursor.0 - 1].len().saturating_sub(1),
                );
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
    }

    fn get_long_word_forward_pos(&self) -> (usize, usize) {
        let current_line = &self.lines[self.cursor.0];
        if self.cursor.1 >= current_line.len() {
            if self.cursor.0 < self.lines.len() - 1 {
                return (self.cursor.0 + 1, 0);
            }
            return self.cursor;
        }

        let mut idx = self.cursor.1;
        let chars: Vec<char> = current_line.chars().collect();

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
    }

    fn get_long_word_backward_pos(&self) -> (usize, usize) {
        if self.cursor.1 == 0 {
            if self.cursor.0 > 0 {
                return (
                    self.cursor.0 - 1,
                    self.lines[self.cursor.0 - 1].len().saturating_sub(1),
                );
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
                return (
                    self.cursor.0 - 1,
                    self.lines[self.cursor.0 - 1].len().saturating_sub(1),
                );
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
    }

    fn get_line_start_pos(&self) -> (usize, usize) {
        (self.cursor.0, 0)
    }

    fn get_line_end_pos(&self) -> (usize, usize) {
        (self.cursor.0, self.lines[self.cursor.0].len())
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
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
            self.record_change();
        }
    }

    fn delete_a_word(&mut self) {
        let (start, end) = self.get_a_word_bounds();
        let line = &mut self.lines[self.cursor.0];
        if start < end && end <= line.len() {
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
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
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
            self.record_change();
        }
    }

    fn delete_a_long_word(&mut self) {
        let (start, end) = self.get_a_long_word_bounds();
        let line = &mut self.lines[self.cursor.0];
        if start < end && end <= line.len() {
            line.replace_range(start..end, "");
            self.cursor.1 = start;
            self.clamp_cursor();
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
            search_end = self.lines[row].len();
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
            self.record_change();
        }
    }

    fn delete_around_pair(&mut self, pair_char: char) {
        if let Some(((open_row, open_col), (close_row, close_col))) = self.find_pair_bounds(pair_char) {
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
            // Same line
            let line = &mut self.lines[start_row];
            let end_clamped = actual_end_col.min(line.len());
            if start_col < end_clamped {
                line.replace_range(start_col..end_clamped, "");
            }
            self.cursor.0 = start_row;
            self.cursor.1 = start_col;
        } else {
            // Multi-line
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
                self.perform_delete_motion(|s| s.get_word_forward_pos(), false);
            }
            LastChange::DeleteLongWord => {
                self.perform_delete_motion(|s| s.get_long_word_forward_pos(), false);
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
            LastChange::InsertText(text, is_after) => {
                // For 'a' style insert, move cursor right first
                if is_after && self.cursor.1 < self.lines[self.cursor.0].len() {
                    self.cursor.1 += 1;
                }
                for c in text.chars() {
                    self.insert_char(c);
                }
                // Move cursor back like Escape does
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                }
            }
            LastChange::ToggleCase => self.toggle_case(),
            LastChange::JoinLines => self.join_lines(),
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
        let line = &mut self.lines[self.cursor.0];
        if self.cursor.1 < line.len() {
            line.truncate(self.cursor.1);
        }
        self.clamp_cursor();
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

    fn join_lines(&mut self) {
        if self.cursor.0 < self.lines.len() - 1 {
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
        let mode_str = match self.mode {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
        };
        let status_text = format!(
            " -- {} --  {}:{}",
            mode_str,
            self.cursor.0 + 1,
            self.cursor.1 + 1
        );
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
        for i in 0..content_rows {
            let line_idx = self.viewport_top + i;
            if line_idx >= self.lines.len() {
                break;
            }

            self.buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(1 + i),
                },
                Change::Attribute(AttributeChange::Foreground(self.colors.line_number_fg)),
                Change::Text(format!("{:>3} ", line_idx + 1)),
                Change::Attribute(AttributeChange::Foreground(self.colors.text_fg)),
                Change::Text(self.lines[line_idx].clone()),
            ]);
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
            }),
        ]);

        self.buf.flush()?;

        // Ensure the current history entry reflects our current position.
        // This ensures that if the user undos, they land back at the navigation
        // point where the edit was initiated, rather than a stale (0,0) position.
        if let Some(entry) = self.history.get_mut(self.history_idx) {
            entry.1 = self.cursor;
        }

        Ok(())
    }

    // Helper to perform delete action based on a motion
    fn perform_delete_motion<F>(&mut self, motion: F, is_inclusive: bool)
    where
        F: Fn(&EditorState) -> (usize, usize),
    {
        let start = self.cursor;
        let end = motion(self);

        // Handle direction
        if end.0 < start.0 || (end.0 == start.0 && end.1 < start.1) {
            // Backward motion (db, dB)
            // Let's simplify: delete [end, start).
            let range_end = start.1;
            let range_start = end.1;
            let count = range_end - range_start;
            let line = &mut self.lines[start.0]; // Assume single line for now
            for _ in 0..count {
                line.remove(range_start);
            }
            self.cursor.1 = range_start; // Move cursor to start of deletion
        } else {
            // Forward motion (dw, de)
            // w: exclusive. delete [start, end)
            // e: inclusive. delete [start, end] -> delete [start, end + 1)
            let mut range_end = end.1;
            if is_inclusive {
                range_end += 1;
            }
            let line = &mut self.lines[start.0]; // Assume single line
                                                 // Cap at line length
            if range_end > line.len() {
                range_end = line.len();
            }

            let count = range_end - start.1;
            for _ in 0..count {
                if start.1 < line.len() {
                    line.remove(start.1);
                }
            }
        }
        self.clamp_cursor();
        self.record_change();
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
                    }) => self.redo(),
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
                                }
                            } else if first == KeyCode::Char('i') && c == 'w' {
                                // diw / ciw - delete/change inner word
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeInnerWord;
                                } else {
                                    self.last_change = LastChange::DeleteInnerWord;
                                }
                                self.delete_inner_word();
                            } else if first == KeyCode::Char('a') && c == 'w' {
                                // daw / caw - delete/change a word
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeAWord;
                                } else {
                                    self.last_change = LastChange::DeleteAWord;
                                }
                                self.delete_a_word();
                            } else if first == KeyCode::Char('i') && c == 'W' {
                                // diW / ciW - delete/change inner WORD
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeInnerLongWord;
                                } else {
                                    self.last_change = LastChange::DeleteInnerLongWord;
                                }
                                self.delete_inner_long_word();
                            } else if first == KeyCode::Char('a') && c == 'W' {
                                // daW / caW - delete/change a WORD
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeALongWord;
                                } else {
                                    self.last_change = LastChange::DeleteALongWord;
                                }
                                self.delete_a_long_word();
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
                                } else {
                                    self.last_change = LastChange::DeleteInnerPair(c);
                                }
                                self.delete_inner_pair(c);
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
                                } else {
                                    self.last_change = LastChange::DeleteAroundPair(c);
                                }
                                self.delete_around_pair(c);
                            } else if first == KeyCode::Char('[') && (c == '(' || c == '{') {
                                // d[( d[{ c[( c[{ - delete/change to previous unmatched bracket
                                let (open, close) = if c == '(' { ('(', ')') } else { ('{', '}') };
                                if op == 'c' {
                                    self.mode = EditorMode::Insert;
                                }
                                self.delete_to_prev_unmatched(open, close);
                            } else if first == KeyCode::Char(']') && (c == ')' || c == '}') {
                                // d]) d]} c]) c]} - delete/change to next unmatched bracket
                                let (open, close) = if c == ')' { ('(', ')') } else { ('{', '}') };
                                if op == 'c' {
                                    self.mode = EditorMode::Insert;
                                }
                                self.delete_to_next_unmatched(open, close);
                            } else if first == KeyCode::Char('f') {
                                // df{char} / cf{char} - delete/change to char (inclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeToChar(c, true);
                                } else {
                                    self.last_change = LastChange::DeleteToChar(c, true);
                                }
                                self.delete_to_char_forward(c, true);
                            } else if first == KeyCode::Char('F') {
                                // dF{char} / cF{char} - delete/change backward to char (inclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeBackToChar(c, true);
                                } else {
                                    self.last_change = LastChange::DeleteBackToChar(c, true);
                                }
                                self.delete_to_char_backward(c, true);
                            } else if first == KeyCode::Char('t') {
                                // dt{char} / ct{char} - delete/change till char (exclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeToChar(c, false);
                                } else {
                                    self.last_change = LastChange::DeleteToChar(c, false);
                                }
                                self.delete_to_char_forward(c, false);
                            } else if first == KeyCode::Char('T') {
                                // dT{char} / cT{char} - delete/change backward till char (exclusive)
                                if op == 'c' {
                                    self.insert_buffer.clear();
                                    self.mode = EditorMode::Insert;
                                    self.last_change = LastChange::ChangeBackToChar(c, false);
                                } else {
                                    self.last_change = LastChange::DeleteBackToChar(c, false);
                                }
                                self.delete_to_char_backward(c, false);
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
                                        self.perform_delete_motion(|s| s.get_word_end_pos(), true);
                                        self.mode = EditorMode::Insert;
                                    }
                                    'W' => {
                                        self.perform_delete_motion(
                                            |s| s.get_long_word_end_pos(),
                                            true,
                                        );
                                        self.mode = EditorMode::Insert;
                                    }
                                    'e' => {
                                        self.perform_delete_motion(|s| s.get_word_end_pos(), true);
                                        self.mode = EditorMode::Insert;
                                    }
                                    'E' => {
                                        self.perform_delete_motion(
                                            |s| s.get_long_word_end_pos(),
                                            true,
                                        );
                                        self.mode = EditorMode::Insert;
                                    }
                                    'b' => {
                                        self.perform_delete_motion(
                                            |s| s.get_word_backward_pos(),
                                            false,
                                        );
                                        self.mode = EditorMode::Insert;
                                    }
                                    'B' => {
                                        self.perform_delete_motion(
                                            |s| s.get_long_word_backward_pos(),
                                            false,
                                        );
                                        self.mode = EditorMode::Insert;
                                    }
                                    '$' => self.change_to_end_of_line(),
                                    '^' => {
                                        self.perform_delete_motion(
                                            |s| s.get_first_non_blank_pos(),
                                            false,
                                        );
                                        self.mode = EditorMode::Insert;
                                    }
                                    '0' => {
                                        self.perform_delete_motion(
                                            |s| s.get_line_start_pos(),
                                            false,
                                        );
                                        self.mode = EditorMode::Insert;
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
                                        self.perform_delete_motion(|s| s.get_word_forward_pos(), false);
                                        self.last_change = LastChange::DeleteWord;
                                    }
                                    'W' => {
                                        self.perform_delete_motion(|s| s.get_long_word_forward_pos(), false);
                                        self.last_change = LastChange::DeleteLongWord;
                                    }
                                    'e' => {
                                        self.perform_delete_motion(|s| s.get_word_end_pos(), true)
                                    }
                                    'E' => self
                                        .perform_delete_motion(|s| s.get_long_word_end_pos(), true),
                                    'b' => self.perform_delete_motion(
                                        |s| s.get_word_backward_pos(),
                                        false,
                                    ),
                                    'B' => self.perform_delete_motion(
                                        |s| s.get_long_word_backward_pos(),
                                        false,
                                    ),
                                    '$' => self.delete_to_end_of_line(),
                                    '^' => self.perform_delete_motion(
                                        |s| s.get_first_non_blank_pos(),
                                        false,
                                    ),
                                    '0' => self.perform_delete_motion(
                                        |s| s.get_line_start_pos(),
                                        false,
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
                            }

                            self.render()?;
                            continue;
                        }

                        if !self.pending_keys.is_empty() {
                            let first = self.pending_keys[0];
                            if first == KeyCode::Char('g') && c == 'g' {
                                self.cursor.0 = 0;
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('Z') && c == 'Z' {
                                self.submit();
                                break;
                            } else if first == KeyCode::Char('Z') && c == 'Q' {
                                break;
                            } else if first == KeyCode::Char('[') && c == '(' {
                                self.jump_to_prev_unmatched('(', ')');
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('[') && c == '{' {
                                self.jump_to_prev_unmatched('{', '}');
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char(']') && c == ')' {
                                self.jump_to_next_unmatched('(', ')');
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char(']') && c == '}' {
                                self.jump_to_next_unmatched('{', '}');
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('f') {
                                self.move_to_char_forward(c);
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('F') {
                                self.move_to_char_backward(c);
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('t') {
                                self.move_till_char_forward(c);
                                self.pending_keys.clear();
                            } else if first == KeyCode::Char('T') {
                                self.move_till_char_backward(c);
                                self.pending_keys.clear();
                            } else {
                                self.pending_keys.clear();
                            }
                            self.render()?;
                            continue;
                        }

                        if c == 'g' || c == 'Z' || c == '[' || c == ']'
                            || c == 'f' || c == 'F' || c == 't' || c == 'T'
                        {
                            self.pending_keys.push(KeyCode::Char(c));
                            continue;
                        }

                        if c == 'c' || c == 'd' {
                            self.pending_operator = Some(c);
                            continue;
                        }

                        match c {
                            'i' => {
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_after = false;
                                self.mode = EditorMode::Insert;
                            }
                            'I' => {
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_after = false;
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
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_after = true;
                                self.mode = EditorMode::Insert;
                                self.move_cursor(0, 1);
                            }
                            'A' => {
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_after = true;
                                self.cursor.1 = self.lines[self.cursor.0].len();
                                self.mode = EditorMode::Insert;
                            }
                            'o' => {
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_after = false;
                                self.lines.insert(self.cursor.0 + 1, String::new());
                                self.cursor.0 += 1;
                                self.cursor.1 = 0;
                                self.mode = EditorMode::Insert;
                                self.record_change();
                            }
                            'O' => {
                                self.insert_buffer.clear();
                                self.last_change = LastChange::None;
                                self.insert_after = false;
                                self.lines.insert(self.cursor.0, String::new());
                                self.cursor.1 = 0;
                                self.mode = EditorMode::Insert;
                                self.record_change();
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
                            'u' => self.undo(),
                            '0' => self.cursor.1 = 0,
                            '^' => self.move_to_first_non_blank(),
                            '$' => {
                                self.cursor.1 = self.lines[self.cursor.0].len().saturating_sub(1)
                            }
                            'G' => self.cursor.0 = self.lines.len() - 1,
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
                            '%' => self.jump_to_matching_bracket(),
                            '.' => self.repeat_last_change(),
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
                                    self.last_change = LastChange::InsertText(self.insert_buffer.clone(), self.insert_after);
                                }
                            }
                        }
                        // Move cursor left first (Vim behavior when leaving Insert mode)
                        if self.cursor.1 > 0 {
                            self.cursor.1 -= 1;
                        }
                        // Then clamp to ensure we're within line bounds
                        self.clamp_cursor();
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
                    }
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

