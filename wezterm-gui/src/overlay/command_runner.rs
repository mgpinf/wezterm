use config::configuration;
use config::keyassignment::{CommandRunner, CommandRunnerCommand};
use mux::termwiztermtab::TermWizTerminal;
use regex::Regex;
use smol::channel::{Receiver, Sender};
use smol::io::AsyncReadExt;
use smol::process::{Child, Command, Stdio};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};
use termwiz::cell::{AttributeChange, Intensity};
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::Terminal;
use textwrap::fill;
use wezterm_term::unicode_column_width;
use window::{Clipboard, Window, WindowOps};

/// Maximum output buffer size per command (1MB)
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy)]
struct CommandRunnerColors {
    list_header_fg: ColorAttribute,
    list_marker_fg: ColorAttribute,
    output_label_fg: ColorAttribute,
    separator_fg: ColorAttribute,
    margin_fg: ColorAttribute,
    line_number_fg: ColorAttribute,
    active_line_number_fg: ColorAttribute,
    match_fg: ColorAttribute,
    match_bg: ColorAttribute,
    current_match_fg: ColorAttribute,
    current_match_bg: ColorAttribute,
}

impl CommandRunnerColors {
    fn new() -> Self {
        let config = configuration();
        let colors = &config.resolved_palette;

        Self {
            list_header_fg: colors
                .command_runner_list_header_fg
                .unwrap_or(AnsiColor::Grey.into())
                .into(),
            list_marker_fg: colors
                .command_runner_list_marker_fg
                .unwrap_or(AnsiColor::Yellow.into())
                .into(),
            output_label_fg: colors
                .command_runner_output_label_fg
                .unwrap_or(AnsiColor::Olive.into())
                .into(),
            separator_fg: colors
                .command_runner_output_separator_fg
                .map_or_else(|| ColorAttribute::Default, |fg| fg.into()),
            margin_fg: colors
                .command_runner_output_margin_fg
                .unwrap_or(AnsiColor::Teal.into())
                .into(),
            line_number_fg: colors
                .command_runner_output_line_number_fg
                .unwrap_or(AnsiColor::Grey.into())
                .into(),
            active_line_number_fg: colors
                .command_runner_output_active_line_number_fg
                .unwrap_or(AnsiColor::White.into())
                .into(),
            match_fg: colors
                .command_runner_match_fg
                .unwrap_or(AnsiColor::Black.into())
                .into(),
            match_bg: colors
                .command_runner_match_bg
                .unwrap_or(AnsiColor::Yellow.into())
                .into(),
            current_match_fg: colors
                .command_runner_current_match_fg
                .unwrap_or(AnsiColor::White.into())
                .into(),
            current_match_bg: colors
                .command_runner_current_match_bg
                .unwrap_or(AnsiColor::Navy.into())
                .into(),
        }
    }
}

#[derive(Debug, Clone)]
struct HighlightRange {
    start: usize,
    end: usize,
    match_id: usize,
}

#[derive(Debug, Clone)]
struct WrappedSegment<'a> {
    text: &'a str,
    highlights: Vec<HighlightRange>,
    line_number: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct MatchLocation {
    match_id: usize,
    row_idx: usize,
    line_number: usize,
}

fn char_column_width(c: char) -> usize {
    let mut buf = [0u8; 4];
    let s = c.encode_utf8(&mut buf);
    unicode_column_width(s, None)
}

fn str_column_width(s: &str) -> usize {
    s.chars().map(char_column_width).sum()
}

fn compile_search_regex(pattern: &str, mode: SearchMode) -> Option<Regex> {
    let regex = match mode {
        SearchMode::CaseInsensitive => Regex::new(&format!("(?i){}", regex::escape(pattern))),
        SearchMode::CaseSensitive => Regex::new(&regex::escape(pattern)),
        SearchMode::Regex => Regex::new(pattern),
    };

    regex.ok()
}

fn column_widths_at_byte_positions(
    s: &str,
    start_byte: usize,
    end_byte: usize,
) -> (usize, usize, usize) {
    let mut current_byte = 0;
    let mut current_width = 0;
    let mut start_width = None;
    let mut end_width = None;

    for ch in s.chars() {
        if start_width.is_none() && current_byte >= start_byte {
            start_width = Some(current_width);
        }
        if end_width.is_none() && current_byte >= end_byte {
            end_width = Some(current_width);
        }

        current_byte += ch.len_utf8();
        current_width += char_column_width(ch);
    }

    (
        start_width.unwrap_or(current_width),
        end_width.unwrap_or(current_width),
        current_width,
    )
}

fn get_segment_split_indices(s: &str, start_cell: usize, end_cell: usize) -> (usize, usize) {
    let mut current_cell = 0;
    let mut start_byte = None;
    let mut end_byte = None;
    let mut current_byte = 0;

    for ch in s.chars() {
        if start_byte.is_none() && current_cell >= start_cell {
            start_byte = Some(current_byte);
        }
        if end_byte.is_none() && current_cell >= end_cell {
            end_byte = Some(current_byte);
        }

        current_byte += ch.len_utf8();
        current_cell += char_column_width(ch);
    }

    let start = start_byte.unwrap_or(current_byte);
    let end = end_byte.unwrap_or(current_byte);
    (start, std::cmp::max(start, end))
}

fn match_ranges_in_cells(
    line: &str,
    regex: &Regex,
    next_match_id: &mut usize,
) -> Vec<HighlightRange> {
    let mut highlights = Vec::new();

    for m in regex.find_iter(line) {
        let (start, end, _) = column_widths_at_byte_positions(line, m.start(), m.end());
        if start < end {
            let match_id = *next_match_id;
            *next_match_id += 1;
            highlights.push(HighlightRange {
                start,
                end,
                match_id,
            });
        }
    }

    highlights
}

fn wrap_line_with_highlights<'a>(
    line: &'a str,
    highlights: &[HighlightRange],
    max_width: usize,
    line_number: Option<usize>,
) -> Vec<WrappedSegment<'a>> {
    if max_width == 0 {
        return vec![WrappedSegment {
            text: "",
            highlights: Vec::new(),
            line_number,
        }];
    }

    let mut segments = Vec::new();
    let mut current_width = 0;
    let mut current_start_cell = 0;
    let mut cell_idx = 0;
    let mut segment_start_byte = 0;
    let mut current_byte = 0;
    let mut next_line_number = line_number;

    for ch in line.chars() {
        let ch_width = char_column_width(ch);
        if current_width + ch_width > max_width && current_width > 0 {
            let segment_text = &line[segment_start_byte..current_byte];
            let mut segment_highlights = Vec::new();
            let seg_end_cell = current_start_cell + current_width;

            for hl in highlights {
                if hl.start < seg_end_cell && hl.end > current_start_cell {
                    let local_start = hl.start.saturating_sub(current_start_cell);
                    let local_end = (hl.end - current_start_cell).min(current_width);
                    if local_start < local_end {
                        segment_highlights.push(HighlightRange {
                            start: local_start,
                            end: local_end,
                            match_id: hl.match_id,
                        });
                    }
                }
            }

            segment_highlights.sort_by_key(|hl| hl.start);
            segments.push(WrappedSegment {
                text: segment_text,
                highlights: segment_highlights,
                line_number: next_line_number,
            });
            next_line_number = None;
            segment_start_byte = current_byte;
            current_width = 0;
            current_start_cell = cell_idx;
        }

        current_byte += ch.len_utf8();
        current_width += ch_width;
        cell_idx += ch_width;
    }

    if current_byte > segment_start_byte || segments.is_empty() {
        let segment_text = &line[segment_start_byte..current_byte];
        let mut segment_highlights = Vec::new();
        let seg_end_cell = current_start_cell + current_width;

        for hl in highlights {
            if hl.start < seg_end_cell && hl.end > current_start_cell {
                let local_start = hl.start.saturating_sub(current_start_cell);
                let local_end = (hl.end - current_start_cell).min(current_width);
                if local_start < local_end {
                    segment_highlights.push(HighlightRange {
                        start: local_start,
                        end: local_end,
                        match_id: hl.match_id,
                    });
                }
            }
        }

        segment_highlights.sort_by_key(|hl| hl.start);
        segments.push(WrappedSegment {
            text: segment_text,
            highlights: segment_highlights,
            line_number: next_line_number,
        });
    }

    segments
}

fn push_text_with_highlights(
    changes: &mut Vec<Change>,
    segment: &WrappedSegment,
    colors: &CommandRunnerColors,
    current_match_id: Option<usize>,
) {
    if segment.highlights.is_empty() {
        changes.push(Change::Text(segment.text.to_string()));
        return;
    }

    let mut last_byte = 0;

    for hl in &segment.highlights {
        if hl.end <= hl.start {
            continue;
        }
        let (start_byte, end_byte) = get_segment_split_indices(segment.text, hl.start, hl.end);
        if start_byte < last_byte {
            continue;
        }
        if start_byte > last_byte {
            changes.push(Change::Text(
                segment.text[last_byte..start_byte].to_string(),
            ));
        }

        let is_current = current_match_id == Some(hl.match_id);
        let (bg, fg) = if is_current {
            (colors.current_match_bg, colors.current_match_fg)
        } else {
            (colors.match_bg, colors.match_fg)
        };
        changes.push(AttributeChange::Background(bg).into());
        changes.push(AttributeChange::Foreground(fg).into());
        changes.push(Change::Text(segment.text[start_byte..end_byte].to_string()));
        changes.push(Change::AllAttributes(Default::default()));

        last_byte = end_byte;
    }

    if last_byte < segment.text.len() {
        changes.push(Change::Text(segment.text[last_byte..].to_string()));
    }
}

fn wrapped_row_count(line: &str, max_width: usize) -> usize {
    if max_width == 0 {
        return 1;
    }

    let mut count = 1;
    let mut current_width = 0;

    for ch in line.chars() {
        let ch_width = char_column_width(ch);
        if current_width + ch_width > max_width && current_width > 0 {
            count += 1;
            current_width = 0;
        }
        current_width += ch_width;
    }

    count
}

/// Status of a command
#[derive(Debug, Clone, PartialEq)]
enum CommandStatus {
    Pending,
    Running,
    Success(i32),
    Failed(i32),
    Killed,
}

impl CommandStatus {
    fn is_running(&self) -> bool {
        matches!(self, CommandStatus::Running)
    }

    fn is_finished(&self) -> bool {
        matches!(
            self,
            CommandStatus::Success(_) | CommandStatus::Failed(_) | CommandStatus::Killed
        )
    }

    fn color(&self) -> ColorAttribute {
        match self {
            CommandStatus::Pending => AnsiColor::White.into(),
            CommandStatus::Running => AnsiColor::Yellow.into(),
            CommandStatus::Success(_) => AnsiColor::Green.into(),
            CommandStatus::Failed(_) | CommandStatus::Killed => AnsiColor::Red.into(),
        }
    }
}

/// State for a single command
struct CommandState {
    config: CommandRunnerCommand,
    status: CommandStatus,
    output: VecDeque<u8>,
    output_lines: Vec<String>,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
}

impl CommandState {
    fn new(config: CommandRunnerCommand) -> Self {
        Self {
            config,
            status: CommandStatus::Pending,
            output: VecDeque::with_capacity(MAX_OUTPUT_SIZE),
            output_lines: Vec::new(),
            start_time: None,
            end_time: None,
        }
    }

    /// Get the display title (uses title if set, otherwise args[0])
    fn title(&self) -> &str {
        self.config
            .title
            .as_deref()
            .unwrap_or_else(|| self.config.args.first().map(|s| s.as_str()).unwrap_or(""))
    }

    fn elapsed(&self) -> Option<Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            (Some(start), None) => Some(Instant::now().duration_since(start)),
            _ => None,
        }
    }

    fn elapsed_str(&self) -> String {
        match self.elapsed() {
            Some(d) => {
                let secs = d.as_secs();
                let mins = secs / 60;
                let secs = secs % 60;
                if mins > 0 {
                    format!("{}:{:02}", mins, secs)
                } else {
                    format!("0:{:02}", secs)
                }
            }
            None => String::new(),
        }
    }

    fn append_output(&mut self, data: &[u8]) {
        // Ring buffer behavior - remove old data if we exceed the limit
        let new_len = self.output.len() + data.len();
        if new_len > MAX_OUTPUT_SIZE {
            let to_remove = new_len - MAX_OUTPUT_SIZE;
            for _ in 0..to_remove {
                self.output.pop_front();
            }
        }
        self.output.extend(data);
        self.reparse_lines();
    }

    fn reparse_lines(&mut self) {
        let text = String::from_utf8_lossy(self.output.make_contiguous());
        self.output_lines = text
            .split('\n')
            .map(|s| s.trim_end_matches('\r').to_string())
            .collect();
    }
}

/// Search mode type
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum SearchMode {
    #[default]
    CaseSensitive,
    CaseInsensitive,
    Regex,
}

impl SearchMode {
    fn next(self) -> Self {
        match self {
            SearchMode::CaseSensitive => SearchMode::CaseInsensitive,
            SearchMode::CaseInsensitive => SearchMode::Regex,
            SearchMode::Regex => SearchMode::CaseSensitive,
        }
    }

    fn display(&self) -> &'static str {
        match self {
            SearchMode::CaseInsensitive => "case-insensitive",
            SearchMode::CaseSensitive => "case-sensitive",
            SearchMode::Regex => "regex",
        }
    }
}

/// View mode for the overlay
#[derive(Debug, Clone)]
enum ViewMode {
    List,
    Output {
        command_idx: usize,
    },
    Filter {
        command_idx: usize,
        pattern: String,
        mode: SearchMode,
        context: usize,
    },
    ConfirmQuit,
}

/// Control flow for event handling
enum ControlFlow {
    Continue,
    Exit,
}

/// Default context lines for filter mode
const DEFAULT_CONTEXT_LINES: usize = 2;

/// Active filter state (persists after exiting filter mode with Enter)
#[derive(Debug, Clone, Default)]
struct ActiveFilter {
    pattern: String,
    mode: SearchMode,
    context: usize,
    command_idx: usize,
}

/// Main state for the command runner overlay
struct CommandRunnerState {
    commands: Vec<CommandState>,
    view_mode: ViewMode,
    list_selection: usize,
    scroll_offset: usize,
    filtered_lines: Vec<(usize, String)>, // (original_line_idx, line)
    active_filter: Option<ActiveFilter>,  // Persists filter params for streaming updates
    current_match_idx: Option<usize>,
    current_match_command: Option<usize>,
    current_line_idx: Option<usize>,
    current_line_command: Option<usize>,
    auto_close_on_success: bool,
    screen_rows: usize,
    screen_cols: usize,
    count_buffer: String,
    colors: CommandRunnerColors,
    window: Window,
}

impl CommandRunnerState {
    fn new(args: CommandRunner, window: Window) -> Self {
        let commands = args.commands.into_iter().map(CommandState::new).collect();

        Self {
            commands,
            view_mode: ViewMode::List,
            list_selection: 0,
            scroll_offset: 0,
            filtered_lines: Vec::new(),
            active_filter: None,
            current_match_idx: None,
            current_match_command: None,
            current_line_idx: None,
            current_line_command: None,
            auto_close_on_success: args.auto_close_on_success,
            screen_rows: 24,
            screen_cols: 80,
            count_buffer: String::new(),
            colors: CommandRunnerColors::new(),
            window,
        }
    }

    fn take_count(&mut self) -> usize {
        let count = self.count_buffer.parse::<usize>().unwrap_or(0);
        self.count_buffer.clear();
        count.max(1)
    }

    fn reset_count(&mut self) {
        self.count_buffer.clear();
    }

    fn all_finished(&self) -> bool {
        self.commands.iter().all(|c| c.status.is_finished())
    }

    fn all_succeeded(&self) -> bool {
        self.commands
            .iter()
            .all(|c| matches!(c.status, CommandStatus::Success(0)))
    }

    fn any_running(&self) -> bool {
        self.commands.iter().any(|c| c.status.is_running())
    }

    fn current_command(&self) -> Option<&CommandState> {
        match &self.view_mode {
            ViewMode::Output { command_idx } | ViewMode::Filter { command_idx, .. } => {
                self.commands.get(*command_idx)
            }
            _ => None,
        }
    }

    fn current_command_idx(&self) -> Option<usize> {
        match &self.view_mode {
            ViewMode::Output { command_idx } | ViewMode::Filter { command_idx, .. } => {
                Some(*command_idx)
            }
            _ => None,
        }
    }

    fn output_line_count(&self) -> usize {
        let command_idx = match self.current_command_idx() {
            Some(idx) => idx,
            None => return 0,
        };
        let max_width = self.output_content_width_for(command_idx);
        let mut count = 0;
        let filter_active = self.filter_active_for(command_idx);

        // Use filtered lines if available (both in Filter mode and Output mode after Enter)
        if filter_active {
            if self.filtered_lines.is_empty() {
                return 0;
            }
            for (_, line) in &self.filtered_lines {
                count += wrapped_row_count(line, max_width);
            }
        } else if let Some(cmd) = self.current_command() {
            for line in &cmd.output_lines {
                count += wrapped_row_count(line, max_width);
            }
        }

        count
    }

    fn filter_active_for(&self, command_idx: usize) -> bool {
        match &self.view_mode {
            ViewMode::Filter {
                command_idx: filter_idx,
                pattern,
                ..
            } if *filter_idx == command_idx => !pattern.is_empty(),
            _ => self
                .active_filter
                .as_ref()
                .filter(|af| af.command_idx == command_idx && !af.pattern.is_empty())
                .is_some(),
        }
    }

    fn line_number_width_for(&self, command_idx: usize) -> usize {
        let max_line_idx = if !self.filtered_lines.is_empty() {
            self.filtered_lines
                .iter()
                .map(|(line_idx, _)| *line_idx)
                .max()
        } else {
            self.commands
                .get(command_idx)
                .and_then(|cmd| cmd.output_lines.len().checked_sub(1))
        };

        if max_line_idx.is_some() && self.screen_cols > 8 {
            5
        } else {
            0
        }
    }

    fn line_number_gutter_width_for(&self, command_idx: usize) -> usize {
        let width = self.line_number_width_for(command_idx);
        if width > 0 {
            width + 1
        } else {
            0
        }
    }

    fn output_content_width_for(&self, command_idx: usize) -> usize {
        self.screen_cols
            .saturating_sub(self.line_number_gutter_width_for(command_idx))
            .max(1)
    }

    fn active_search_regex_for(&self, command_idx: usize) -> Option<Regex> {
        let (pattern, mode) = match &self.view_mode {
            ViewMode::Filter {
                command_idx: filter_idx,
                pattern,
                mode,
                ..
            } if *filter_idx == command_idx => {
                if pattern.is_empty() {
                    return None;
                }
                Some((pattern.as_str(), *mode))
            }
            _ => self
                .active_filter
                .as_ref()
                .filter(|af| af.command_idx == command_idx && !af.pattern.is_empty())
                .map(|af| (af.pattern.as_str(), af.mode)),
        }?;

        compile_search_regex(pattern, mode)
    }

    fn output_wrapped_rows<'a>(
        &'a self,
        command_idx: usize,
        regex: Option<&Regex>,
    ) -> Vec<WrappedSegment<'a>> {
        let max_width = self.output_content_width_for(command_idx);
        let mut rows = Vec::new();
        let mut next_match_id = 0;
        let filter_active = self.filter_active_for(command_idx);

        if filter_active {
            if self.filtered_lines.is_empty() {
                return rows;
            }
            for (line_idx, line) in &self.filtered_lines {
                let line_number = Some(*line_idx + 1);
                let highlights = regex
                    .map(|regex| match_ranges_in_cells(line, regex, &mut next_match_id))
                    .unwrap_or_default();
                rows.extend(wrap_line_with_highlights(
                    line,
                    &highlights,
                    max_width,
                    line_number,
                ));
            }
        } else if let Some(cmd) = self.commands.get(command_idx) {
            for (line_idx, line) in cmd.output_lines.iter().enumerate() {
                let line_number = Some(line_idx + 1);
                let highlights = regex
                    .map(|regex| match_ranges_in_cells(line, regex, &mut next_match_id))
                    .unwrap_or_default();
                rows.extend(wrap_line_with_highlights(
                    line,
                    &highlights,
                    max_width,
                    line_number,
                ));
            }
        }

        rows
    }

    fn output_wrapped_row_count(&self, command_idx: usize) -> usize {
        let regex = self.active_search_regex_for(command_idx);
        self.output_wrapped_rows(command_idx, regex.as_ref()).len()
    }

    fn current_line_for(&mut self, command_idx: usize, total_rows: usize) -> Option<usize> {
        if total_rows == 0 {
            self.current_line_idx = None;
            self.current_line_command = Some(command_idx);
            return None;
        }

        if self.current_line_command != Some(command_idx) {
            let start_idx = self.scroll_offset.min(total_rows.saturating_sub(1));
            self.current_line_command = Some(command_idx);
            self.current_line_idx = Some(start_idx);
        }

        let idx = match self.current_line_idx {
            Some(idx) if idx < total_rows => idx,
            _ => {
                let clamped = self.scroll_offset.min(total_rows.saturating_sub(1));
                self.current_line_idx = Some(clamped);
                clamped
            }
        };

        Some(idx)
    }

    fn set_current_line(&mut self, command_idx: usize, target: usize, total_rows: usize) {
        if total_rows == 0 {
            self.current_line_idx = None;
            self.current_line_command = Some(command_idx);
            return;
        }
        let clamped = target.min(total_rows.saturating_sub(1));
        self.current_line_command = Some(command_idx);
        self.current_line_idx = Some(clamped);
        self.reveal_current_line(clamped);
    }

    fn logical_line_number_at(rows: &[WrappedSegment<'_>], row_idx: usize) -> Option<usize> {
        if rows.is_empty() {
            return None;
        }
        let mut idx = row_idx.min(rows.len().saturating_sub(1));
        loop {
            if let Some(line_number) = rows[idx].line_number {
                return Some(line_number);
            }
            if idx == 0 {
                return None;
            }
            idx -= 1;
        }
    }

    fn next_logical_row(rows: &[WrappedSegment<'_>], row_idx: usize) -> Option<usize> {
        let current_line = Self::logical_line_number_at(rows, row_idx)?;
        for idx in row_idx.saturating_add(1)..rows.len() {
            if let Some(line_number) = rows[idx].line_number {
                if line_number != current_line {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn prev_logical_row(rows: &[WrappedSegment<'_>], row_idx: usize) -> Option<usize> {
        let current_line = Self::logical_line_number_at(rows, row_idx)?;
        if row_idx == 0 {
            return None;
        }
        for idx in (0..row_idx).rev() {
            if let Some(line_number) = rows[idx].line_number {
                if line_number != current_line {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn move_current_line(&mut self, command_idx: usize, delta: isize) {
        let total_rows = self.output_wrapped_row_count(command_idx);
        let current = match self.current_line_for(command_idx, total_rows) {
            Some(idx) => idx,
            None => return,
        };
        let regex = self.active_search_regex_for(command_idx);
        let rows = self.output_wrapped_rows(command_idx, regex.as_ref());
        if rows.is_empty() {
            self.current_line_idx = None;
            self.current_line_command = Some(command_idx);
            return;
        }

        let steps = if delta >= 0 {
            delta as usize
        } else {
            (-delta) as usize
        };
        if steps == 0 {
            return;
        }

        let mut row_idx = current.min(total_rows.saturating_sub(1));
        for _ in 0..steps {
            let next = if delta >= 0 {
                Self::next_logical_row(&rows, row_idx)
            } else {
                Self::prev_logical_row(&rows, row_idx)
            };
            if let Some(next) = next {
                row_idx = next;
            } else {
                break;
            }
        }

        self.current_line_idx = Some(row_idx);
        self.current_line_command = Some(command_idx);
        self.reveal_current_line(row_idx);
    }

    fn reveal_current_line(&mut self, row_idx: usize) {
        let visible_rows = self.visible_output_rows().max(1);
        if row_idx < self.scroll_offset {
            self.scroll_offset = row_idx;
        } else if row_idx >= self.scroll_offset + visible_rows {
            self.scroll_offset = row_idx + 1 - visible_rows;
        }
    }

    fn reset_current_line_to_scroll_offset(&mut self) {
        let command_idx = match self.current_command_idx() {
            Some(idx) => idx,
            None => return,
        };
        let total_rows = self.output_wrapped_row_count(command_idx);
        if total_rows == 0 {
            self.current_line_idx = None;
            self.current_line_command = Some(command_idx);
            return;
        }
        let clamped = self.scroll_offset.min(total_rows.saturating_sub(1));
        self.current_line_command = Some(command_idx);
        self.current_line_idx = Some(clamped);
    }

    fn match_locations_from_rows(rows: &[WrappedSegment<'_>]) -> Vec<MatchLocation> {
        let mut locations: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
        for (row_idx, segment) in rows.iter().enumerate() {
            for hl in &segment.highlights {
                let line_number = Self::logical_line_number_at(rows, row_idx).unwrap_or(0);
                locations
                    .entry(hl.match_id)
                    .or_insert((row_idx, line_number));
            }
        }
        locations
            .into_iter()
            .map(|(match_id, (row_idx, line_number))| MatchLocation {
                match_id,
                row_idx,
                line_number,
            })
            .collect()
    }

    fn match_locations_for(&self, command_idx: usize, regex: &Regex) -> Vec<MatchLocation> {
        let rows = self.output_wrapped_rows(command_idx, Some(regex));
        Self::match_locations_from_rows(&rows)
    }

    fn ensure_current_match(&mut self, command_idx: usize, regex: &Regex) -> Option<MatchLocation> {
        let matches = self.match_locations_for(command_idx, regex);
        if matches.is_empty() {
            self.reset_current_match(command_idx);
            return None;
        }

        if self.current_match_command != Some(command_idx) {
            self.current_match_command = Some(command_idx);
            self.current_match_idx = None;
        }

        let max = matches.len();
        let idx = match self.current_match_idx {
            Some(idx) if idx < max => idx,
            _ => {
                self.current_match_idx = Some(0);
                self.current_match_command = Some(command_idx);
                0
            }
        };
        self.current_match_idx = Some(idx);
        Some(matches[idx])
    }

    fn clear_current_match(&mut self) {
        self.current_match_idx = None;
        self.current_match_command = None;
    }

    fn reset_current_match(&mut self, command_idx: usize) {
        self.current_match_idx = None;
        self.current_match_command = Some(command_idx);
    }

    fn clear_current_match_if_present(&mut self, command_idx: usize) {
        let regex = match self.active_search_regex_for(command_idx) {
            Some(regex) => regex,
            None => return,
        };
        if self.match_locations_for(command_idx, &regex).is_empty() {
            return;
        }
        self.current_match_command = Some(command_idx);
        self.current_match_idx = None;
    }

    fn copy_current_lines(&mut self, command_idx: usize, count: usize) {
        let total_rows = self.output_wrapped_row_count(command_idx);
        let current_row = match self.current_line_for(command_idx, total_rows) {
            Some(idx) => idx,
            None => return,
        };
        let regex = self.active_search_regex_for(command_idx);
        let rows = self.output_wrapped_rows(command_idx, regex.as_ref());
        if rows.is_empty() {
            return;
        }

        let mut row_idx = current_row.min(rows.len().saturating_sub(1));
        let mut line_number = rows.get(row_idx).and_then(|segment| segment.line_number);
        while line_number.is_none() && row_idx > 0 {
            row_idx -= 1;
            line_number = rows.get(row_idx).and_then(|segment| segment.line_number);
        }

        let text = if let Some(line_number) = line_number {
            let line_idx = line_number.saturating_sub(1);
            if self.filter_active_for(command_idx) && !self.filtered_lines.is_empty() {
                let start = self
                    .filtered_lines
                    .iter()
                    .position(|(idx, _)| *idx == line_idx);
                if let Some(start) = start {
                    let end = (start + count).min(self.filtered_lines.len());
                    let mut lines = Vec::with_capacity(end - start);
                    for (_, line) in &self.filtered_lines[start..end] {
                        lines.push(line.as_str());
                    }
                    Some(lines.join("\n"))
                } else {
                    self.commands.get(command_idx).and_then(|cmd| {
                        if line_idx < cmd.output_lines.len() {
                            let end = (line_idx + count).min(cmd.output_lines.len());
                            Some(cmd.output_lines[line_idx..end].join("\n"))
                        } else {
                            None
                        }
                    })
                }
            } else {
                self.commands.get(command_idx).and_then(|cmd| {
                    if line_idx < cmd.output_lines.len() {
                        let end = (line_idx + count).min(cmd.output_lines.len());
                        Some(cmd.output_lines[line_idx..end].join("\n"))
                    } else {
                        None
                    }
                })
            }
        } else {
            rows.get(self.scroll_offset)
                .map(|segment| segment.text.to_string())
        };

        if let Some(text) = text {
            self.window.set_clipboard(Clipboard::Clipboard, text);
        }
    }

    fn move_current_match(&mut self, command_idx: usize, forward: bool) {
        let regex = match self.active_search_regex_for(command_idx) {
            Some(regex) => regex,
            None => return,
        };
        if self.current_match_command != Some(command_idx) {
            self.reset_current_match(command_idx);
        }
        let rows = self.output_wrapped_rows(command_idx, Some(&regex));
        let matches = Self::match_locations_from_rows(&rows);
        if matches.is_empty() {
            drop(rows);
            self.reset_current_match(command_idx);
            return;
        }

        let max = matches.len();
        let anchor_line = if self.current_line_command == Some(command_idx) {
            self.current_line_idx
                .and_then(|idx| Self::logical_line_number_at(&rows, idx))
        } else {
            None
        };
        let next_idx = if let Some(anchor_line) = anchor_line {
            if forward {
                matches
                    .iter()
                    .position(|loc| loc.line_number > anchor_line)
                    .unwrap_or(0)
            } else {
                matches
                    .iter()
                    .rposition(|loc| loc.line_number < anchor_line)
                    .unwrap_or(max - 1)
            }
        } else {
            match self.current_match_idx {
                Some(idx) if idx < max => {
                    if forward {
                        (idx + 1) % max
                    } else {
                        (idx + max - 1) % max
                    }
                }
                _ => {
                    if forward {
                        0
                    } else {
                        max - 1
                    }
                }
            }
        };

        self.current_match_idx = Some(next_idx);
        let row_idx = matches[next_idx].row_idx;
        self.current_line_command = Some(command_idx);
        self.current_line_idx = Some(row_idx);
        self.reveal_current_line(row_idx);
    }

    fn reapply_filter(&mut self) {
        // Use active_filter if set (for streaming updates after Enter)
        // or get from current Filter view mode
        let filter_params = if let ViewMode::Filter {
            pattern,
            mode,
            context,
            ..
        } = &self.view_mode
        {
            Some((pattern.clone(), *mode, *context))
        } else {
            self.active_filter
                .as_ref()
                .map(|af| (af.pattern.clone(), af.mode, af.context))
        };

        if let Some((pattern, mode, context)) = filter_params {
            if pattern.is_empty() {
                self.filtered_lines.clear();
            } else {
                self.apply_filter(&pattern, mode, context);
            }
        }
    }

    fn set_active_filter(
        &mut self,
        command_idx: usize,
        pattern: String,
        mode: SearchMode,
        context: usize,
    ) {
        self.active_filter = Some(ActiveFilter {
            command_idx,
            pattern,
            mode,
            context,
        });
    }

    fn clear_active_filter(&mut self) {
        self.active_filter = None;
        self.filtered_lines.clear();
        self.clear_current_match();
    }

    fn apply_filter(&mut self, pattern: &str, mode: SearchMode, context: usize) {
        self.filtered_lines.clear();

        if pattern.is_empty() {
            self.scroll_offset = 0;
            self.clear_current_match();
            self.reset_current_line_to_scroll_offset();
            return;
        }

        let command_idx = match self.current_command_idx() {
            Some(idx) => idx,
            None => {
                self.clear_current_match();
                return;
            }
        };

        let output_lines: Vec<String> = match self.current_command() {
            Some(c) => c.output_lines.clone(),
            None => return,
        };

        let regex = match mode {
            SearchMode::CaseInsensitive => Regex::new(&format!("(?i){}", regex::escape(pattern))),
            SearchMode::CaseSensitive => Regex::new(&regex::escape(pattern)),
            SearchMode::Regex => Regex::new(pattern),
        };

        let regex = match regex {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut matching_lines: Vec<usize> = Vec::new();
        for (line_idx, line) in output_lines.iter().enumerate() {
            if regex.is_match(line) {
                matching_lines.push(line_idx);
            }
        }

        let mut included: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for &line_idx in &matching_lines {
            let start = line_idx.saturating_sub(context);
            let end = (line_idx + context + 1).min(output_lines.len());
            for i in start..end {
                included.insert(i);
            }
        }

        let mut sorted: Vec<usize> = included.into_iter().collect();
        sorted.sort();

        for line_idx in sorted {
            if let Some(line) = output_lines.get(line_idx) {
                self.filtered_lines.push((line_idx, line.clone()));
            }
        }

        self.scroll_offset = 0;
        let current_match = self.ensure_current_match(command_idx, &regex);
        if let Some(current_match) = current_match {
            self.current_line_command = Some(command_idx);
            self.current_line_idx = Some(current_match.row_idx);
            self.reveal_current_line(current_match.row_idx);
        } else {
            self.reset_current_line_to_scroll_offset();
        }
    }

    fn visible_output_rows(&self) -> usize {
        // Reserve rows for header, search line, context line, separator, and footer
        self.screen_rows.saturating_sub(5)
    }

    fn visible_list_rows(&self) -> usize {
        // Reserve rows for header and footer
        self.screen_rows.saturating_sub(4)
    }

    fn scroll_down(&mut self, amount: usize) {
        let max_offset = self
            .output_line_count()
            .saturating_sub(self.visible_output_rows());
        self.scroll_offset = (self.scroll_offset + amount).min(max_offset);
    }

    fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    fn scroll_to_bottom(&mut self) {
        let max_offset = self
            .output_line_count()
            .saturating_sub(self.visible_output_rows());
        self.scroll_offset = max_offset;
        self.reset_current_line_to_scroll_offset();
    }

    fn render(&mut self, term: &mut TermWizTerminal) -> anyhow::Result<()> {
        let size = term.get_screen_size()?;
        self.screen_rows = size.rows;
        self.screen_cols = size.cols;

        match &self.view_mode {
            ViewMode::List => self.render_list_view(term),
            ViewMode::Output { .. } | ViewMode::Filter { .. } => self.render_output_view(term),
            ViewMode::ConfirmQuit => self.render_confirm_quit(term),
        }
    }

    fn render_list_view(&self, term: &mut TermWizTerminal) -> anyhow::Result<()> {
        let filter_active = matches!(self.view_mode, ViewMode::Filter { .. });
        let cursor_visibility = if filter_active {
            CursorVisibility::Visible
        } else {
            CursorVisibility::Hidden
        };
        let mut changes = vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorVisibility(cursor_visibility),
        ];

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(0),
        });
        let title = "Command Runner";
        changes.push(AttributeChange::Intensity(Intensity::Bold).into());
        changes.push(AttributeChange::Foreground(self.colors.list_header_fg).into());
        changes.push(Change::Text(format!(
            "{:<width$}",
            title,
            width = self.screen_cols
        )));
        changes.push(Change::AllAttributes(Default::default()));
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(1),
        });
        changes.push(AttributeChange::Foreground(self.colors.separator_fg).into());
        changes.push(Change::Text("─".repeat(self.screen_cols)));
        changes.push(Change::AllAttributes(Default::default()));

        let visible_rows = self.visible_list_rows();
        let start_row = 2;

        for (idx, cmd) in self.commands.iter().enumerate() {
            if idx >= visible_rows {
                break;
            }

            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(start_row + idx),
            });

            let is_selected = idx == self.list_selection;

            if is_selected {
                changes.push(AttributeChange::Foreground(self.colors.list_marker_fg).into());
                changes.push(Change::Text("▌ ".to_string()));
                changes.push(Change::AllAttributes(Default::default()));
            } else {
                changes.push(Change::Text("  ".to_string()));
            }

            let status_char = match &cmd.status {
                CommandStatus::Pending => "•",
                CommandStatus::Running => "⏵",
                CommandStatus::Success(_) => "✔",
                CommandStatus::Failed(_) => "✖",
                CommandStatus::Killed => "⏹",
            };
            changes.push(Change::AllAttributes(
                termwiz::cell::CellAttributes::default()
                    .set_foreground(cmd.status.color())
                    .clone(),
            ));
            changes.push(Change::Text(format!("{} ", status_char)));
            changes.push(Change::AllAttributes(Default::default()));

            let status_field_width: usize = 16;
            let title_width = self.screen_cols.saturating_sub(40);
            let title: String = cmd.title().chars().take(title_width).collect();
            if is_selected {
                changes.push(Change::AllAttributes(
                    termwiz::cell::CellAttributes::default()
                        .set_intensity(Intensity::Bold)
                        .clone(),
                ));
            }
            changes.push(Change::Text(format!(
                "{:<width$}",
                title,
                width = title_width
            )));
            if is_selected {
                changes.push(Change::AllAttributes(Default::default()));
            }

            let status_label = match cmd.status {
                CommandStatus::Pending => "Pending",
                CommandStatus::Running => "Running...",
                CommandStatus::Success(_) => "Success",
                CommandStatus::Failed(_) => "Failed",
                CommandStatus::Killed => "Killed",
            };
            let exit_code_string = match cmd.status {
                CommandStatus::Failed(code) => Some(code.to_string()),
                _ => None,
            };
            let status_attrs = termwiz::cell::CellAttributes::default()
                .set_foreground(cmd.status.color())
                .clone();
            let mut status_len: usize = 1 + status_label.len();
            if let Some(ref code) = exit_code_string {
                status_len += 3 + code.len();
            }
            let status_padding = status_field_width.saturating_sub(status_len);

            changes.push(Change::AllAttributes(status_attrs.clone()));
            changes.push(Change::Text(" ".to_string()));
            changes.push(Change::Text(status_label.to_string()));
            if let Some(code) = exit_code_string {
                changes.push(Change::AllAttributes(Default::default()));
                changes.push(Change::Text(" [".to_string()));
                changes.push(Change::AllAttributes(status_attrs.clone()));
                changes.push(Change::Text(code));
                changes.push(Change::AllAttributes(Default::default()));
                changes.push(Change::Text("]".to_string()));
            }
            if status_padding > 0 {
                changes.push(Change::AllAttributes(Default::default()));
                changes.push(Change::Text(" ".repeat(status_padding)));
            }
            changes.push(Change::AllAttributes(Default::default()));

            let elapsed = cmd.elapsed_str();
            changes.push(Change::Text(format!(" {:>6}", elapsed)));

            if is_selected {
                let current_len = 2 + 2 + title_width + status_field_width + 1 + 6;
                let padding = self.screen_cols.saturating_sub(current_len);
                changes.push(Change::Text(" ".repeat(padding)));
            }
        }

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(self.screen_rows - 1),
        });
        changes.push(Change::Text(" ".repeat(self.screen_cols)));

        term.render(&changes)?;
        term.flush()?;

        Ok(())
    }

    fn render_output_view(&self, term: &mut TermWizTerminal) -> anyhow::Result<()> {
        let cmd_idx = match self.current_command_idx() {
            Some(idx) => idx,
            None => return Ok(()),
        };
        let cmd = &self.commands[cmd_idx];

        let filter_active = matches!(self.view_mode, ViewMode::Filter { .. });
        let cursor_visibility = if filter_active {
            CursorVisibility::Visible
        } else {
            CursorVisibility::Hidden
        };
        let mut changes = vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorVisibility(cursor_visibility),
        ];

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(0),
        });
        let status = match cmd.status {
            CommandStatus::Pending => "Pending",
            CommandStatus::Running => "Running",
            CommandStatus::Success(_) => "Success",
            CommandStatus::Failed(_) => "Failed",
            CommandStatus::Killed => "Killed",
        };
        let cmd_title = cmd.title();
        let status_color = cmd.status.color();
        let label_fg = self.colors.output_label_fg;
        let separator_fg = self.colors.separator_fg;
        let margin_fg = self.colors.margin_fg;
        let line_number_fg = self.colors.line_number_fg;
        let exit_code = match cmd.status {
            CommandStatus::Failed(code) => Some(code.to_string()),
            _ => None,
        };
        let mut remaining = self.screen_cols;
        let mut push_segment = |text: &str, color: Option<ColorAttribute>| {
            if remaining == 0 || text.is_empty() {
                return;
            }
            let segment: String = text.chars().take(remaining).collect();
            remaining = remaining.saturating_sub(segment.chars().count());
            if let Some(color) = color {
                changes.push(AttributeChange::Foreground(color).into());
            }
            changes.push(Change::Text(segment));
            if color.is_some() {
                changes.push(Change::AllAttributes(Default::default()));
            }
        };

        push_segment("Command", Some(label_fg));
        push_segment(":", None);
        push_segment(" ", None);
        push_segment(cmd_title, None);
        push_segment(" │ ", Some(separator_fg));
        push_segment("Status", Some(label_fg));
        push_segment(":", None);
        push_segment(" ", None);
        push_segment(status, Some(status_color));
        if let Some(code) = exit_code.as_deref() {
            push_segment(" │ ", Some(separator_fg));
            push_segment("Exit Code", Some(label_fg));
            push_segment(":", None);
            push_segment(" ", None);
            push_segment(code, Some(status_color));
        }

        let (search_pattern, mode, context) = match &self.view_mode {
            ViewMode::Filter {
                pattern,
                mode,
                context,
                ..
            } => (pattern.as_str(), *mode, *context),
            _ => {
                if let Some(af) = self
                    .active_filter
                    .as_ref()
                    .filter(|af| af.command_idx == cmd_idx)
                {
                    (af.pattern.as_str(), af.mode, af.context)
                } else {
                    ("", SearchMode::default(), DEFAULT_CONTEXT_LINES)
                }
            }
        };
        let search_line = if search_pattern.is_empty() {
            "Search: ".to_string()
        } else {
            format!("Search: {}", search_pattern)
        };
        let search_line_render: String = search_line.chars().take(self.screen_cols).collect();
        let filter_cursor_x = if filter_active {
            Some(str_column_width(&search_line_render).min(self.screen_cols.saturating_sub(1)))
        } else {
            None
        };
        let number_width = self.line_number_width_for(cmd_idx);

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(1),
        });
        let mut remaining = self.screen_cols;
        let mut push_search_segment = |text: &str, color: Option<ColorAttribute>| {
            if remaining == 0 || text.is_empty() {
                return;
            }
            let segment: String = text.chars().take(remaining).collect();
            remaining = remaining.saturating_sub(segment.chars().count());
            if let Some(color) = color {
                changes.push(AttributeChange::Foreground(color).into());
            }
            changes.push(Change::Text(segment));
            if color.is_some() {
                changes.push(Change::AllAttributes(Default::default()));
            }
        };
        push_search_segment("Search", Some(label_fg));
        push_search_segment(":", None);
        push_search_segment(" ", None);
        push_search_segment(search_pattern, None);
        if remaining > 0 {
            changes.push(Change::Text(" ".repeat(remaining)));
        }
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(2),
        });
        let mut remaining = self.screen_cols;
        let mut push_context_segment = |text: &str, color: Option<ColorAttribute>| {
            if remaining == 0 || text.is_empty() {
                return;
            }
            let segment: String = text.chars().take(remaining).collect();
            remaining = remaining.saturating_sub(segment.chars().count());
            if let Some(color) = color {
                changes.push(AttributeChange::Foreground(color).into());
            }
            changes.push(Change::Text(segment));
            if color.is_some() {
                changes.push(Change::AllAttributes(Default::default()));
            }
        };
        push_context_segment("Context", Some(label_fg));
        push_context_segment(":", None);
        push_context_segment(" ", None);
        push_context_segment(&format!("±{}", context), None);
        push_context_segment(" │ ", Some(separator_fg));
        push_context_segment("Mode", Some(label_fg));
        push_context_segment(":", None);
        push_context_segment(" ", None);
        push_context_segment(mode.display(), None);
        if remaining > 0 {
            changes.push(Change::Text(" ".repeat(remaining)));
        }
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(3),
        });
        changes.push(AttributeChange::Foreground(margin_fg).into());
        changes.push(Change::Text("─".repeat(self.screen_cols)));
        changes.push(Change::AllAttributes(Default::default()));

        let visible_rows = self.visible_output_rows();
        let start_row = 4;
        let regex = self.active_search_regex_for(cmd_idx);
        let rows = self.output_wrapped_rows(cmd_idx, regex.as_ref());
        let line_count = rows.len();
        let active_line_number = if line_count == 0 {
            None
        } else {
            let current_row = if self.current_line_command == Some(cmd_idx) {
                self.current_line_idx
                    .filter(|idx| *idx < line_count)
                    .or_else(|| Some(self.scroll_offset.min(line_count - 1)))
            } else {
                Some(self.scroll_offset.min(line_count - 1))
            };
            let mut row_idx = match current_row {
                Some(idx) => idx,
                None => 0,
            };
            let mut line_number = rows.get(row_idx).and_then(|segment| segment.line_number);
            while line_number.is_none() && row_idx > 0 {
                row_idx -= 1;
                line_number = rows.get(row_idx).and_then(|segment| segment.line_number);
            }
            line_number
        };
        let match_locations = Self::match_locations_from_rows(&rows);
        let current_match_id = if self.current_match_command == Some(cmd_idx) {
            self.current_match_idx
                .and_then(|idx| match_locations.get(idx).map(|loc| loc.match_id))
        } else {
            None
        };

        for row in 0..visible_rows {
            let line_idx = self.scroll_offset + row;
            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(start_row + row),
            });

            if line_idx < line_count {
                if let Some(segment) = rows.get(line_idx) {
                    changes.push(Change::AllAttributes(Default::default()));
                    if number_width > 0 {
                        if let Some(line_number) = segment.line_number {
                            let line_fg = if active_line_number == Some(line_number) {
                                self.colors.active_line_number_fg
                            } else {
                                line_number_fg
                            };
                            changes.push(AttributeChange::Foreground(line_fg).into());
                            changes.push(Change::Text(format!(
                                "{:>width$}",
                                line_number,
                                width = number_width
                            )));
                            changes.push(Change::AllAttributes(Default::default()));
                            changes.push(Change::Text(" ".to_string()));
                        } else {
                            changes.push(Change::Text(" ".repeat(number_width + 1)));
                        }
                    }
                    push_text_with_highlights(
                        &mut changes,
                        segment,
                        &self.colors,
                        current_match_id,
                    );
                }
            }
        }

        // Footer bar (blank)
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(self.screen_rows - 1),
        });
        changes.push(Change::Text(" ".repeat(self.screen_cols)));

        if let Some(cursor_x) = filter_cursor_x {
            changes.push(Change::CursorVisibility(CursorVisibility::Visible));
            changes.push(Change::CursorPosition {
                x: Position::Absolute(cursor_x),
                y: Position::Absolute(1),
            });
        }

        term.render(&changes)?;
        term.flush()?;

        Ok(())
    }

    fn render_confirm_quit(&self, term: &mut TermWizTerminal) -> anyhow::Result<()> {
        let mut changes = vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorVisibility(CursorVisibility::Hidden),
        ];

        let message = if self.any_running() {
            "There are running commands. Quit and terminate them?"
        } else {
            "Quit Command Runner?"
        };

        let text_width = (self.screen_cols * 80 / 100).max(1);
        let x_pos = self.screen_cols.saturating_sub(text_width) / 2;
        let wrapped = fill(message, text_width);
        let message_rows = wrapped.lines().count();
        let total_rows = message_rows + 2;
        let top_row = self
            .screen_rows
            .saturating_sub(total_rows)
            .saturating_div(2);
        let button_row = top_row + message_rows + 1;

        for (idx, row) in wrapped.lines().enumerate() {
            changes.push(Change::CursorPosition {
                x: Position::Absolute(x_pos),
                y: Position::Absolute(top_row + idx),
            });
            changes.push(Change::Text(row.trim_end().to_string()));
        }

        changes.push(Change::CursorPosition {
            x: Position::Absolute(x_pos),
            y: Position::Absolute(button_row),
        });
        changes.push(Change::Text("[Y]es    [N]o".to_string()));

        term.render(&changes)?;
        term.flush()?;

        Ok(())
    }

    fn handle_input(
        &mut self,
        event: InputEvent,
        process_tx: &Sender<ProcessMessage>,
    ) -> ControlFlow {
        match &self.view_mode.clone() {
            ViewMode::List => self.handle_list_input(event, process_tx),
            ViewMode::Output { command_idx } => {
                self.handle_output_input(event, *command_idx, process_tx)
            }
            ViewMode::Filter {
                command_idx,
                pattern,
                mode,
                context,
            } => self.handle_filter_input(event, *command_idx, pattern.clone(), *mode, *context),
            ViewMode::ConfirmQuit => self.handle_confirm_quit_input(event),
        }
    }

    fn handle_list_input(
        &mut self,
        event: InputEvent,
        process_tx: &Sender<ProcessMessage>,
    ) -> ControlFlow {
        match event {
            // Numeric keys for count prefix
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers: Modifiers::NONE,
            }) if c.is_ascii_digit() => {
                self.count_buffer.push(c);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('j'),
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::DownArrow,
                ..
            }) => {
                let count = self.take_count();
                let max = self.commands.len().saturating_sub(1);
                self.list_selection = (self.list_selection + count).min(max);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('k'),
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::UpArrow,
                ..
            }) => {
                let count = self.take_count();
                self.list_selection = self.list_selection.saturating_sub(count);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('g'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.list_selection = 0;
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('G'),
                modifiers: Modifiers::SHIFT | Modifiers::NONE,
            }) => {
                if self.count_buffer.is_empty() {
                    self.list_selection = self.commands.len().saturating_sub(1);
                } else {
                    let count = self.take_count();
                    self.list_selection = count
                        .saturating_sub(1)
                        .min(self.commands.len().saturating_sub(1));
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }) => {
                self.reset_count();
                if self.list_selection < self.commands.len() {
                    // Auto-scroll to bottom for running commands
                    self.view_mode = ViewMode::Output {
                        command_idx: self.list_selection,
                    };
                    self.scroll_offset = 0;
                    if self.commands[self.list_selection].status.is_running() {
                        self.scroll_to_bottom();
                    } else {
                        self.reset_current_line_to_scroll_offset();
                    }
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('r'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                if self.list_selection < self.commands.len() {
                    let _ = process_tx.try_send(ProcessMessage::Rerun(self.list_selection));
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('C'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                if self.list_selection < self.commands.len() {
                    let _ = process_tx.try_send(ProcessMessage::Kill(self.list_selection));
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('q'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                if self.any_running() {
                    self.view_mode = ViewMode::ConfirmQuit;
                } else {
                    return ControlFlow::Exit;
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) => {
                self.reset_count();
            }
            _ => {
                self.reset_count();
            }
        }
        ControlFlow::Continue
    }

    fn handle_output_input(
        &mut self,
        event: InputEvent,
        command_idx: usize,
        process_tx: &Sender<ProcessMessage>,
    ) -> ControlFlow {
        match event {
            // Numeric keys for count prefix
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers: Modifiers::NONE,
            }) if c.is_ascii_digit() => {
                self.count_buffer.push(c);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('j'),
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::DownArrow,
                ..
            }) => {
                let count = self.take_count();
                self.clear_current_match_if_present(command_idx);
                self.move_current_line(command_idx, count as isize);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('k'),
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::UpArrow,
                ..
            }) => {
                let count = self.take_count();
                self.clear_current_match_if_present(command_idx);
                self.move_current_line(command_idx, -(count as isize));
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('g'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                let total_rows = self.output_wrapped_row_count(command_idx);
                self.clear_current_match_if_present(command_idx);
                self.set_current_line(command_idx, 0, total_rows);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('G'),
                modifiers: Modifiers::SHIFT | Modifiers::NONE,
            }) => {
                let total_rows = self.output_wrapped_row_count(command_idx);
                if total_rows == 0 {
                    self.reset_count();
                    return ControlFlow::Continue;
                }
                if self.count_buffer.is_empty() {
                    self.reset_count();
                    self.clear_current_match_if_present(command_idx);
                    self.set_current_line(command_idx, total_rows.saturating_sub(1), total_rows);
                } else {
                    let count = self.take_count();
                    if self.filter_active_for(command_idx) {
                        let regex = self.active_search_regex_for(command_idx);
                        let rows = self.output_wrapped_rows(command_idx, regex.as_ref());
                        let target_row = rows.iter().enumerate().find_map(|(idx, segment)| {
                            if segment.line_number == Some(count) {
                                Some(idx)
                            } else {
                                None
                            }
                        });
                        if let Some(row_idx) = target_row {
                            self.clear_current_match_if_present(command_idx);
                            self.set_current_line(command_idx, row_idx, total_rows);
                        }
                    } else {
                        self.clear_current_match_if_present(command_idx);
                        self.set_current_line(command_idx, count.saturating_sub(1), total_rows);
                    }
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('n'),
                modifiers: Modifiers::NONE,
            }) => {
                let count = self.take_count();
                for _ in 0..count {
                    self.move_current_match(command_idx, true);
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('N'),
                modifiers: Modifiers::SHIFT | Modifiers::NONE,
            }) => {
                let count = self.take_count();
                for _ in 0..count {
                    self.move_current_match(command_idx, false);
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('y'),
                modifiers: Modifiers::NONE,
            }) => {
                let count = self.take_count();
                self.copy_current_lines(command_idx, count);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('D'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                let half_page = self.visible_output_rows() / 2;
                self.clear_current_match_if_present(command_idx);
                self.move_current_line(command_idx, half_page as isize);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('U'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                let half_page = self.visible_output_rows() / 2;
                self.clear_current_match_if_present(command_idx);
                self.move_current_line(command_idx, -(half_page as isize));
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('R'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                let (pattern, mode, context) = if let Some(ref mut af) = self
                    .active_filter
                    .as_mut()
                    .filter(|af| af.command_idx == command_idx)
                {
                    af.mode = af.mode.next();
                    (af.pattern.clone(), af.mode, af.context)
                } else {
                    let mode = SearchMode::default().next();
                    self.active_filter = Some(ActiveFilter {
                        command_idx,
                        pattern: String::new(),
                        mode,
                        context: DEFAULT_CONTEXT_LINES,
                    });
                    (String::new(), mode, DEFAULT_CONTEXT_LINES)
                };
                self.reset_current_match(command_idx);
                if !pattern.is_empty() {
                    self.apply_filter(&pattern, mode, context);
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('/'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                let (pattern, mode, context) = self
                    .active_filter
                    .as_ref()
                    .filter(|af| af.command_idx == command_idx)
                    .map(|af| (af.pattern.clone(), af.mode, af.context))
                    .unwrap_or_else(|| {
                        (
                            String::new(),
                            SearchMode::CaseSensitive,
                            DEFAULT_CONTEXT_LINES,
                        )
                    });
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern,
                    mode,
                    context,
                };
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('c'),
                modifiers: Modifiers::NONE,
            }) => {
                if self.count_buffer.is_empty() {
                    self.reset_count();
                    if let Some(ref mut af) = self
                        .active_filter
                        .as_mut()
                        .filter(|af| af.command_idx == command_idx)
                    {
                        af.context = 0;
                        self.reapply_filter();
                        self.scroll_offset = 0;
                        self.reset_current_line_to_scroll_offset();
                    }
                } else {
                    let context = self.take_count();
                    if let Some(ref mut af) = self
                        .active_filter
                        .as_mut()
                        .filter(|af| af.command_idx == command_idx)
                    {
                        af.context = context;
                        self.reapply_filter();
                        self.scroll_offset = 0;
                        self.reset_current_line_to_scroll_offset();
                    }
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('C'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                if self
                    .active_filter
                    .as_ref()
                    .filter(|af| af.command_idx == command_idx)
                    .is_some()
                {
                    self.clear_active_filter();
                    self.scroll_offset = 0;
                    self.reset_current_line_to_scroll_offset();
                }
            }
            // Tab increases context, Shift-Tab decreases (when filter is active)
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                if let Some(ref mut af) = self.active_filter {
                    af.context += 1;
                    self.reapply_filter();
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::SHIFT,
            }) => {
                self.reset_count();
                if let Some(ref mut af) = self.active_filter {
                    if af.context > 0 {
                        af.context -= 1;
                        self.reapply_filter();
                    }
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('r'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                let _ = process_tx.try_send(ProcessMessage::Rerun(command_idx));
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('C'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                let _ = process_tx.try_send(ProcessMessage::Kill(command_idx));
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) => {
                self.reset_count();
                self.clear_active_filter();
                self.view_mode = ViewMode::List;
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('q'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.clear_active_filter();
                self.view_mode = ViewMode::List;
            }
            _ => {
                self.reset_count();
            }
        }
        ControlFlow::Continue
    }

    fn handle_filter_input(
        &mut self,
        event: InputEvent,
        command_idx: usize,
        mut pattern: String,
        mut mode: SearchMode,
        mut context: usize,
    ) -> ControlFlow {
        match event {
            // Scroll navigation in filtered output
            InputEvent::Key(KeyEvent {
                key: KeyCode::DownArrow,
                ..
            }) => {
                self.scroll_down(1);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::UpArrow,
                ..
            }) => {
                self.scroll_up(1);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('R'),
                modifiers: Modifiers::CTRL,
            }) => {
                mode = mode.next();
                self.reset_current_match(command_idx);
                self.apply_filter(&pattern, mode, context);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern,
                    mode,
                    context,
                };
            }
            // Tab increases context, Shift-Tab decreases
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE,
            }) => {
                context += 1;
                self.apply_filter(&pattern, mode, context);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern,
                    mode,
                    context,
                };
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::SHIFT,
            }) => {
                if context > 0 {
                    context -= 1;
                    self.apply_filter(&pattern, mode, context);
                    self.view_mode = ViewMode::Filter {
                        command_idx,
                        pattern,
                        mode,
                        context,
                    };
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                ..
            }) => {
                pattern.pop();
                self.reset_current_match(command_idx);
                self.apply_filter(&pattern, mode, context);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern,
                    mode,
                    context,
                };
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('U'),
                modifiers: Modifiers::CTRL,
            }) => {
                pattern.clear();
                self.reset_current_match(command_idx);
                if self
                    .active_filter
                    .as_ref()
                    .filter(|af| af.command_idx == command_idx)
                    .is_some()
                {
                    self.clear_active_filter();
                } else {
                    self.filtered_lines.clear();
                }
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern,
                    mode,
                    context,
                };
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) => {
                if let Some((pattern, mode, context)) = self
                    .active_filter
                    .as_ref()
                    .filter(|af| af.command_idx == command_idx)
                    .map(|af| (af.pattern.clone(), af.mode, af.context))
                {
                    self.view_mode = ViewMode::Output { command_idx };
                    self.apply_filter(&pattern, mode, context);
                } else {
                    self.filtered_lines.clear();
                    self.view_mode = ViewMode::Output { command_idx };
                    self.reset_current_line_to_scroll_offset();
                }
            }
            // Enter accepts the filter and goes back to output view (keeping filtered results)
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }) => {
                let had_pattern = !pattern.is_empty();
                if pattern.is_empty() {
                    self.clear_current_match();
                    if self
                        .active_filter
                        .as_ref()
                        .filter(|af| af.command_idx == command_idx)
                        .is_some()
                    {
                        self.clear_active_filter();
                    } else {
                        self.filtered_lines.clear();
                    }
                } else {
                    // Save filter params for streaming updates
                    self.set_active_filter(command_idx, pattern, mode, context);
                }
                self.view_mode = ViewMode::Output { command_idx };
                if !had_pattern {
                    self.reset_current_line_to_scroll_offset();
                }
            }
            // Character input for pattern
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers: Modifiers::NONE | Modifiers::SHIFT,
            }) => {
                pattern.push(c);
                self.reset_current_match(command_idx);
                self.apply_filter(&pattern, mode, context);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern,
                    mode,
                    context,
                };
            }
            _ => {}
        }
        ControlFlow::Continue
    }

    fn handle_confirm_quit_input(&mut self, event: InputEvent) -> ControlFlow {
        match event {
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('y' | 'Y'),
                ..
            }) => {
                return ControlFlow::Exit;
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('n' | 'N'),
                ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) => {
                self.view_mode = ViewMode::List;
            }
            _ => {}
        }
        ControlFlow::Continue
    }
}

/// Messages sent to the process manager
enum ProcessMessage {
    Kill(usize),
    Rerun(usize),
}

/// Messages sent from the process manager
enum OutputMessage {
    Output { idx: usize, data: Vec<u8> },
    Started { idx: usize },
}

/// Spawn a command and return channels for output
async fn spawn_command(
    idx: usize,
    config: &CommandRunnerCommand,
    output_tx: Sender<OutputMessage>,
) -> anyhow::Result<Child> {
    if config.args.is_empty() {
        anyhow::bail!("args cannot be empty");
    }

    let mut cmd = Command::new(&config.args[0]);
    if config.args.len() > 1 {
        cmd.args(&config.args[1..]);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());

    if let Some(cwd) = &config.cwd {
        cmd.current_dir(cwd);
    }
    for (k, v) in &config.set_environment_variables {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn()?;
    let _ = output_tx.try_send(OutputMessage::Started { idx });

    // Spawn readers for stdout and stderr
    if let Some(stdout) = child.stdout.take() {
        let tx = output_tx.clone();
        smol::spawn(async move {
            let mut reader = smol::io::BufReader::new(stdout);
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = tx.try_send(OutputMessage::Output {
                            idx,
                            data: buf[..n].to_vec(),
                        });
                    }
                    Err(_) => break,
                }
            }
        })
        .detach();
    }

    if let Some(stderr) = child.stderr.take() {
        let tx = output_tx.clone();
        smol::spawn(async move {
            let mut reader = smol::io::BufReader::new(stderr);
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = tx.try_send(OutputMessage::Output {
                            idx,
                            data: buf[..n].to_vec(),
                        });
                    }
                    Err(_) => break,
                }
            }
        })
        .detach();
    }

    Ok(child)
}

/// Main entry point for the command runner overlay
pub fn show_command_runner_overlay(
    mut term: TermWizTerminal,
    args: CommandRunner,
    window: Window,
) -> anyhow::Result<()> {
    // term.set_raw_mode()?;
    term.no_grab_mouse_in_raw_mode();

    let mut state = CommandRunnerState::new(args, window);

    // Channels for process management
    let (process_tx, process_rx): (Sender<ProcessMessage>, Receiver<ProcessMessage>) =
        smol::channel::bounded(16);
    let (output_tx, output_rx): (Sender<OutputMessage>, Receiver<OutputMessage>) =
        smol::channel::bounded(256);

    // Store child processes
    let mut children: Vec<Option<Child>> = (0..state.commands.len()).map(|_| None).collect();

    // Spawn all commands initially
    for (idx, cmd) in state.commands.iter_mut().enumerate() {
        let config = cmd.config.clone();
        let tx = output_tx.clone();
        cmd.status = CommandStatus::Running;
        cmd.start_time = Some(Instant::now());

        // Spawn in a blocking context since we're in a sync function
        let child_result = smol::block_on(spawn_command(idx, &config, tx));
        match child_result {
            Ok(child) => {
                children[idx] = Some(child);
            }
            Err(e) => {
                cmd.status = CommandStatus::Failed(-1);
                cmd.end_time = Some(Instant::now());
                cmd.append_output(format!("Failed to spawn: {}\n", e).as_bytes());
            }
        }
    }

    // Main event loop
    let mut dirty = true;
    loop {
        // Process output messages (non-blocking)
        while let Ok(msg) = output_rx.try_recv() {
            match msg {
                OutputMessage::Output { idx, data } => {
                    if let Some(cmd) = state.commands.get_mut(idx) {
                        cmd.append_output(&data);
                        dirty = true;
                        // Re-apply filter if viewing this command's output with filter active
                        if let Some(view_idx) = state.current_command_idx() {
                            if view_idx == idx {
                                let has_active_filter = state
                                    .active_filter
                                    .as_ref()
                                    .map(|af| af.command_idx == idx)
                                    .unwrap_or(false)
                                    || matches!(state.view_mode, ViewMode::Filter { .. });

                                if has_active_filter {
                                    state.reapply_filter();
                                } else {
                                    // Auto-scroll if not filtering
                                    state.scroll_to_bottom();
                                }
                            }
                        }
                    }
                }
                OutputMessage::Started { idx } => {
                    if let Some(cmd) = state.commands.get_mut(idx) {
                        cmd.status = CommandStatus::Running;
                        cmd.start_time = Some(Instant::now());
                        dirty = true;
                    }
                }
            }
        }

        // Check for finished processes
        for (idx, child_opt) in children.iter_mut().enumerate() {
            if let Some(child) = child_opt {
                match child.try_status() {
                    Ok(Some(status)) => {
                        let cmd_status = if status.success() {
                            CommandStatus::Success(status.code().unwrap_or(0))
                        } else {
                            CommandStatus::Failed(status.code().unwrap_or(-1))
                        };
                        if let Some(cmd) = state.commands.get_mut(idx) {
                            if cmd.status != CommandStatus::Killed {
                                cmd.status = cmd_status;
                            }
                            if cmd.end_time.is_none() {
                                cmd.end_time = Some(Instant::now());
                            }
                            dirty = true;
                        }
                        *child_opt = None;
                    }
                    Ok(None) => {} // Still running
                    Err(_) => {
                        if let Some(cmd) = state.commands.get_mut(idx) {
                            if cmd.status != CommandStatus::Killed {
                                cmd.status = CommandStatus::Failed(-1);
                            }
                            if cmd.end_time.is_none() {
                                cmd.end_time = Some(Instant::now());
                            }
                            dirty = true;
                        }
                        *child_opt = None;
                    }
                }
            }
        }

        // Process control messages
        while let Ok(msg) = process_rx.try_recv() {
            match msg {
                ProcessMessage::Kill(idx) => {
                    if let Some(Some(child)) = children.get_mut(idx) {
                        let _ = child.kill();
                        if let Some(cmd) = state.commands.get_mut(idx) {
                            cmd.status = CommandStatus::Killed;
                            cmd.end_time = Some(Instant::now());
                            dirty = true;
                        }
                    }
                }
                ProcessMessage::Rerun(idx) => {
                    // Kill existing if running
                    if let Some(Some(child)) = children.get_mut(idx) {
                        let _ = child.kill();
                    }

                    if let Some(cmd) = state.commands.get_mut(idx) {
                        cmd.status = CommandStatus::Running;
                        cmd.output.clear();
                        cmd.output_lines.clear();
                        cmd.start_time = Some(Instant::now());
                        cmd.end_time = None;
                        dirty = true;

                        let config = cmd.config.clone();
                        let tx = output_tx.clone();
                        let child_result = smol::block_on(spawn_command(idx, &config, tx));
                        match child_result {
                            Ok(child) => {
                                children[idx] = Some(child);
                            }
                            Err(e) => {
                                cmd.status = CommandStatus::Failed(-1);
                                cmd.end_time = Some(Instant::now());
                                cmd.append_output(format!("Failed to spawn: {}\n", e).as_bytes());
                                dirty = true;
                            }
                        }
                    }
                }
            }
        }

        // Check auto-close condition
        if state.auto_close_on_success && state.all_finished() && state.all_succeeded() {
            break;
        }

        // Render if running or state changed
        if dirty || state.any_running() {
            state.render(&mut term)?;
            dirty = false;
        }

        // Poll for input with short timeout
        match term.poll_input(Some(Duration::from_millis(50)))? {
            Some(event) => match state.handle_input(event, &process_tx) {
                ControlFlow::Continue => {
                    dirty = true;
                }
                ControlFlow::Exit => break,
            },
            None => {} // Timeout, continue
        }
    }

    // Kill all running processes
    for child in children.iter_mut().flatten() {
        let _ = child.kill();
    }

    Ok(())
}
