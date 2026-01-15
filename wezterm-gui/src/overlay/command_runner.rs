use crate::overlay::quickselect;
use config::configuration;
use config::keyassignment::{CommandRunner, CommandRunnerCommand};
use mux::termwiztermtab::TermWizTerminal;
use regex::Regex;
use smol::channel::{Receiver, Sender};
use smol::io::AsyncReadExt;
use smol::process::{Child, Command, Stdio};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};
use termwiz::cell::{AttributeChange, CellAttributes, Intensity};
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
struct WrappedSegment {
    text: String,
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
    unicode_column_width(s, None)
}

fn compile_search_regex(pattern: &str, mode: SearchMode) -> Option<Regex> {
    let regex = match mode {
        SearchMode::CaseInsensitive => Regex::new(&format!("(?i){}", regex::escape(pattern))),
        SearchMode::CaseSensitive => Regex::new(&regex::escape(pattern)),
        SearchMode::Regex => Regex::new(pattern),
    };

    regex.ok()
}

fn column_widths_at_byte_positions(s: &str, start_byte: usize, end_byte: usize) -> (usize, usize) {
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
            if start_width.is_some() {
                return (start_width.unwrap(), end_width.unwrap());
            }
        }

        current_byte += ch.len_utf8();
        current_width += char_column_width(ch);
    }

    (
        start_width.unwrap_or(current_width),
        end_width.unwrap_or(current_width),
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
            if start_byte.is_some() {
                break;
            }
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
        let (start, end) = column_widths_at_byte_positions(line, m.start(), m.end());
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

fn wrap_line_with_highlights(
    line: &str,
    highlights: &[HighlightRange],
    max_width: usize,
    line_number: Option<usize>,
) -> Vec<WrappedSegment> {
    if max_width == 0 {
        return vec![WrappedSegment {
            text: String::new(),
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
            let segment_text = line[segment_start_byte..current_byte].to_string();
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
        let segment_text = line[segment_start_byte..current_byte].to_string();
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
        let (start_byte, end_byte) = get_segment_split_indices(&segment.text, hl.start, hl.end);
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
        changes.extend([
            AttributeChange::Background(bg).into(),
            AttributeChange::Foreground(fg).into(),
            Change::Text(segment.text[start_byte..end_byte].to_string()),
            Change::AllAttributes(Default::default()),
        ]);

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

/// Helper for rendering segments with optional colors and width limiting.
/// This avoids duplicating the push_segment closure pattern across render methods.
struct SegmentWriter<'a> {
    changes: &'a mut Vec<Change>,
    remaining: usize,
}

impl<'a> SegmentWriter<'a> {
    fn new(changes: &'a mut Vec<Change>, max_width: usize) -> Self {
        Self {
            changes,
            remaining: max_width,
        }
    }

    fn push(&mut self, text: &str, color: Option<ColorAttribute>) {
        if self.remaining == 0 || text.is_empty() {
            return;
        }
        let segment: String = text.chars().take(self.remaining).collect();
        self.remaining = self.remaining.saturating_sub(segment.chars().count());
        if let Some(color) = color {
            self.changes.push(AttributeChange::Foreground(color).into());
        }
        self.changes.push(Change::Text(segment));
        if color.is_some() {
            self.changes.push(Change::AllAttributes(Default::default()));
        }
    }

    fn fill_remaining(&mut self) {
        if self.remaining > 0 {
            self.changes.push(Change::Text(" ".repeat(self.remaining)));
            self.remaining = 0;
        }
    }
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
    output_lines: VecDeque<String>,
    line_byte_lengths: VecDeque<usize>,
    pending_line: Vec<u8>,
    output_size: usize,
    output_generation: u64,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
}

impl CommandState {
    fn new(config: CommandRunnerCommand) -> Self {
        Self {
            config,
            status: CommandStatus::Pending,
            output_lines: VecDeque::new(),
            line_byte_lengths: VecDeque::new(),
            pending_line: Vec::new(),
            output_size: 0,
            output_generation: 0,
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
        if data.is_empty() {
            return;
        }
        self.output_size = self.output_size.saturating_add(data.len());
        self.pending_line.extend_from_slice(data);
        self.update_lines_from_pending();
        self.trim_output_to_limit();
        self.output_generation = self.output_generation.wrapping_add(1);
    }

    fn decode_line(bytes: &[u8]) -> String {
        let line = String::from_utf8_lossy(bytes);
        if line.ends_with('\r') {
            line.trim_end_matches('\r').to_string()
        } else {
            line.into_owned()
        }
    }

    fn update_lines_from_pending(&mut self) {
        if self.pending_line.is_empty() && self.output_lines.is_empty() {
            return;
        }

        let mut iter = self.pending_line.split(|&b| b == b'\n');
        let Some(first) = iter.next() else {
            return;
        };

        if self.output_lines.is_empty() {
            self.output_lines.clear();
            self.line_byte_lengths.clear();
            self.output_lines.push_back(Self::decode_line(first));
            self.line_byte_lengths.push_back(first.len());
        } else {
            if let Some(last) = self.output_lines.back_mut() {
                *last = Self::decode_line(first);
            }
            if let Some(last_len) = self.line_byte_lengths.back_mut() {
                *last_len = first.len();
            } else {
                self.line_byte_lengths.push_back(first.len());
            }
        }

        let mut last_seg = first;
        let mut saw_newline = false;
        for seg in iter {
            saw_newline = true;
            self.output_lines.push_back(Self::decode_line(seg));
            self.line_byte_lengths.push_back(seg.len());
            last_seg = seg;
        }

        if saw_newline {
            self.pending_line = last_seg.to_vec();
        }
    }

    fn trim_output_to_limit(&mut self) {
        if self.output_size <= MAX_OUTPUT_SIZE {
            return;
        }

        while self.output_size > MAX_OUTPUT_SIZE && self.output_lines.len() > 1 {
            let line_len = *self.line_byte_lengths.front().unwrap_or(&0);
            let line_with_newline = line_len + 1;
            self.output_lines.pop_front();
            self.line_byte_lengths.pop_front();
            self.output_size = self.output_size.saturating_sub(line_with_newline);
        }

        if self.output_size <= MAX_OUTPUT_SIZE {
            return;
        }

        if self.output_lines.len() == 1 {
            let line_len = *self.line_byte_lengths.front().unwrap_or(&0);
            if line_len > MAX_OUTPUT_SIZE {
                let drop_in_line = self.output_size - MAX_OUTPUT_SIZE;
                if drop_in_line > 0 {
                    let line = self.output_lines.front_mut().unwrap();
                    let bytes = line.as_bytes();
                    let updated = if drop_in_line >= bytes.len() {
                        String::new()
                    } else {
                        String::from_utf8_lossy(&bytes[drop_in_line..]).into_owned()
                    };
                    *line = updated;
                    if let Some(len) = self.line_byte_lengths.front_mut() {
                        *len = len.saturating_sub(drop_in_line);
                    }
                    if drop_in_line >= self.pending_line.len() {
                        self.pending_line.clear();
                    } else {
                        self.pending_line.drain(..drop_in_line);
                    }
                    self.output_size = self.output_size.saturating_sub(drop_in_line);
                }
            }
        }
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

#[derive(Debug, Clone)]
struct RegexCache {
    pattern: String,
    mode: SearchMode,
    regex: Regex,
}

#[derive(Debug, Clone)]
struct WrappedRowsCache {
    command_idx: usize,
    max_width: usize,
    filter_active: bool,
    output_generation: u64,
    filtered_generation: u64,
    regex_pattern: Option<String>,
    rows: Rc<Vec<WrappedSegment>>,
}

/// Main state for the command runner overlay
struct CommandRunnerState {
    commands: Vec<CommandState>,
    view_mode: ViewMode,
    list_selection: usize,
    scroll_offset: usize,
    filtered_lines: Vec<(usize, String)>, // (original_line_idx, line)
    active_filter: Option<ActiveFilter>,  // Persists filter params for streaming updates
    filtered_generation: u64,
    wrapped_rows_cache: Option<WrappedRowsCache>,
    regex_cache: Option<RegexCache>,
    current_match_idx: Option<usize>,
    current_match_command: Option<usize>,
    current_line_idx: Option<usize>,
    current_line_command: Option<usize>,
    auto_close_on_success: bool,
    screen_rows: usize,
    screen_cols: usize,
    count_buffer: String,
    list_selection_input: String,
    list_alphabet: String,
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
            filtered_generation: 0,
            wrapped_rows_cache: None,
            regex_cache: None,
            current_match_idx: None,
            current_match_command: None,
            current_line_idx: None,
            current_line_command: None,
            auto_close_on_success: args.auto_close_on_success,
            screen_rows: 24,
            screen_cols: 80,
            count_buffer: String::new(),
            list_selection_input: String::new(),
            list_alphabet: args.alphabet,
            colors: CommandRunnerColors::new(),
            window,
        }
    }

    fn take_count(&mut self) -> usize {
        if self.count_buffer.is_empty() {
            return 1;
        }
        let count = self.count_buffer.parse::<usize>().unwrap_or(0);
        self.count_buffer.clear();
        count
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

    /// Get filter parameters for a command, checking ViewMode::Filter first, then active_filter.
    /// Returns (pattern, mode, context) or default values if no filter is active.
    fn get_filter_params(&self, command_idx: usize) -> (String, SearchMode, usize) {
        match &self.view_mode {
            ViewMode::Filter {
                command_idx: filter_idx,
                pattern,
                mode,
                context,
            } if *filter_idx == command_idx => (pattern.clone(), *mode, *context),
            _ => self
                .active_filter
                .as_ref()
                .filter(|af| af.command_idx == command_idx)
                .map(|af| (af.pattern.clone(), af.mode, af.context))
                .unwrap_or_else(|| (String::new(), SearchMode::default(), DEFAULT_CONTEXT_LINES)),
        }
    }

    fn cached_regex(&mut self, pattern: &str, mode: SearchMode) -> Option<Regex> {
        if pattern.is_empty() {
            self.regex_cache = None;
            return None;
        }

        if let Some(cache) = &self.regex_cache {
            if cache.mode == mode && cache.pattern == pattern {
                return Some(cache.regex.clone());
            }
        }

        let regex = compile_search_regex(pattern, mode)?;
        self.regex_cache = Some(RegexCache {
            pattern: pattern.to_string(),
            mode,
            regex: regex.clone(),
        });
        Some(regex)
    }

    fn active_search_regex_for(&mut self, command_idx: usize) -> Option<Regex> {
        let (pattern, mode, _) = self.get_filter_params(command_idx);
        if pattern.is_empty() {
            return None;
        }
        self.cached_regex(&pattern, mode)
    }

    fn output_wrapped_rows(
        &mut self,
        command_idx: usize,
        regex: Option<&Regex>,
    ) -> Rc<Vec<WrappedSegment>> {
        let max_width = self.output_content_width_for(command_idx);
        let filter_active = self.filter_active_for(command_idx);
        let output_generation = self
            .commands
            .get(command_idx)
            .map(|cmd| cmd.output_generation)
            .unwrap_or(0);
        let regex_pattern = regex.map(|regex| regex.as_str().to_string());
        if let Some(cache) = &self.wrapped_rows_cache {
            if cache.command_idx == command_idx
                && cache.max_width == max_width
                && cache.filter_active == filter_active
                && cache.output_generation == output_generation
                && cache.filtered_generation == self.filtered_generation
                && cache.regex_pattern.as_deref() == regex_pattern.as_deref()
            {
                return Rc::clone(&cache.rows);
            }
        }

        let mut rows = Vec::new();
        let mut next_match_id = 0;
        if filter_active {
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

        let rows = Rc::new(rows);
        self.wrapped_rows_cache = Some(WrappedRowsCache {
            command_idx,
            max_width,
            filter_active,
            output_generation,
            filtered_generation: self.filtered_generation,
            regex_pattern,
            rows: Rc::clone(&rows),
        });
        rows
    }

    fn output_wrapped_row_count(&self, command_idx: usize) -> usize {
        let max_width = self.output_content_width_for(command_idx);
        let mut count = 0;
        let filter_active = self.filter_active_for(command_idx);

        if filter_active {
            if self.filtered_lines.is_empty() {
                return 0;
            }
            for (_, line) in &self.filtered_lines {
                count += wrapped_row_count(line, max_width);
            }
        } else if let Some(cmd) = self.commands.get(command_idx) {
            for line in &cmd.output_lines {
                count += wrapped_row_count(line, max_width);
            }
        }

        count
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

    fn logical_line_number_at(rows: &[WrappedSegment], row_idx: usize) -> Option<usize> {
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

    fn next_logical_row(rows: &[WrappedSegment], row_idx: usize) -> Option<usize> {
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

    fn prev_logical_row(rows: &[WrappedSegment], row_idx: usize) -> Option<usize> {
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
                Self::next_logical_row(rows.as_ref(), row_idx)
            } else {
                Self::prev_logical_row(rows.as_ref(), row_idx)
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

    fn match_locations_from_rows(rows: &[WrappedSegment]) -> Vec<MatchLocation> {
        // Match_ids are sequential starting from 0 and encountered in ascending order.
        // We only record the first row where each match_id appears.
        let mut locations: Vec<MatchLocation> = Vec::new();
        let mut current_line_number = None;
        for (row_idx, segment) in rows.iter().enumerate() {
            if segment.line_number.is_some() {
                current_line_number = segment.line_number;
            }
            let line_number = current_line_number.unwrap_or(0);
            for hl in &segment.highlights {
                // Only record first occurrence of each match_id
                if hl.match_id == locations.len() {
                    locations.push(MatchLocation {
                        match_id: hl.match_id,
                        row_idx,
                        line_number,
                    });
                }
            }
        }
        locations
    }

    fn match_locations_for(&mut self, command_idx: usize, regex: &Regex) -> Vec<MatchLocation> {
        let rows = self.output_wrapped_rows(command_idx, Some(regex));
        Self::match_locations_from_rows(rows.as_ref())
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
        fn join_lines(lines: &VecDeque<String>, start: usize, count: usize) -> Option<String> {
            if start >= lines.len() {
                return None;
            }
            let end = (start + count).min(lines.len());
            let mut iter = lines.iter().skip(start).take(end - start);
            let mut out = String::new();
            if let Some(first) = iter.next() {
                out.push_str(first);
                for line in iter {
                    out.push('\n');
                    out.push_str(line);
                }
            }
            Some(out)
        }

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
                    self.commands
                        .get(command_idx)
                        .and_then(|cmd| join_lines(&cmd.output_lines, line_idx, count))
                }
            } else {
                self.commands
                    .get(command_idx)
                    .and_then(|cmd| join_lines(&cmd.output_lines, line_idx, count))
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
        let matches = Self::match_locations_from_rows(rows.as_ref());
        if matches.is_empty() {
            drop(rows);
            self.reset_current_match(command_idx);
            return;
        }

        let max = matches.len();
        let anchor_line = if self.current_line_command == Some(command_idx) {
            self.current_line_idx
                .and_then(|idx| Self::logical_line_number_at(rows.as_ref(), idx))
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
                self.clear_filtered_lines();
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

    fn bump_filtered_generation(&mut self) {
        self.filtered_generation = self.filtered_generation.wrapping_add(1);
    }

    fn clear_filtered_lines(&mut self) {
        self.filtered_lines.clear();
        self.bump_filtered_generation();
    }

    fn clear_active_filter(&mut self) {
        self.active_filter = None;
        self.clear_filtered_lines();
        self.clear_current_match();
    }

    fn apply_filter(&mut self, pattern: &str, mode: SearchMode, context: usize) {
        if pattern.is_empty() {
            self.scroll_offset = 0;
            self.clear_current_match();
            self.clear_filtered_lines();
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

        let regex = match self.cached_regex(pattern, mode) {
            Some(regex) => regex,
            None => {
                self.clear_filtered_lines();
                return;
            }
        };

        let filtered_lines = if let Some(cmd) = self.current_command() {
            let mut include = vec![false; cmd.output_lines.len()];
            for (line_idx, line) in cmd.output_lines.iter().enumerate() {
                if regex.is_match(line) {
                    let start = line_idx.saturating_sub(context);
                    let end = (line_idx + context + 1).min(cmd.output_lines.len());
                    for i in start..end {
                        include[i] = true;
                    }
                }
            }

            let included_count = include.iter().filter(|flag| **flag).count();
            let mut filtered_lines = Vec::with_capacity(included_count);
            for (line_idx, line) in cmd.output_lines.iter().enumerate() {
                if include[line_idx] {
                    filtered_lines.push((line_idx, line.clone()));
                }
            }
            filtered_lines
        } else {
            return;
        };

        self.filtered_lines = filtered_lines;
        self.bump_filtered_generation();

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

    fn list_labels(&self) -> Vec<String> {
        let visible = self.visible_list_rows();
        let count = self.commands.len().min(visible);
        if count == 0 || self.list_alphabet.is_empty() {
            return Vec::new();
        }
        quickselect::compute_labels_for_alphabet_with_preserved_case(&self.list_alphabet, count)
    }

    fn open_output_view(&mut self, command_idx: usize) {
        if command_idx >= self.commands.len() {
            return;
        }
        self.view_mode = ViewMode::Output { command_idx };
        self.scroll_offset = 0;
        if self.commands[command_idx].status.is_running() {
            self.scroll_to_bottom();
        } else {
            self.reset_current_line_to_scroll_offset();
        }
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

        let title = "Command Runner";
        changes.extend([
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
            AttributeChange::Intensity(Intensity::Bold).into(),
            AttributeChange::Foreground(self.colors.list_header_fg).into(),
            Change::Text(format!("{:<width$}", title, width = self.screen_cols)),
            Change::AllAttributes(Default::default()),
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(1),
            },
            AttributeChange::Foreground(self.colors.separator_fg).into(),
            Change::Text("─".repeat(self.screen_cols)),
            Change::AllAttributes(Default::default()),
        ]);

        let visible_rows = self.visible_list_rows();
        let start_row = 2;
        let labels = self.list_labels();
        let max_label_len = labels.iter().map(|label| label.len()).max().unwrap_or(0);
        let label_width = if max_label_len > 0 {
            max_label_len + 3
        } else {
            0
        };

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
                changes.extend([
                    AttributeChange::Foreground(self.colors.list_marker_fg).into(),
                    Change::Text("▌ ".to_string()),
                    Change::AllAttributes(Default::default()),
                ]);
            } else {
                changes.push(Change::Text("  ".to_string()));
            }

            if label_width > 0 {
                if let Some(label) = labels.get(idx) {
                    changes.push(Change::Text(format!(" {label:>max_label_len$}. ")));
                } else {
                    changes.push(Change::Text(" ".repeat(label_width)));
                }
            }

            let status_char = match &cmd.status {
                CommandStatus::Pending => "•",
                CommandStatus::Running => "⏵",
                CommandStatus::Success(_) => "✔",
                CommandStatus::Failed(_) => "✖",
                CommandStatus::Killed => "⏹",
            };
            changes.extend([
                Change::AllAttributes(
                    CellAttributes::default()
                        .set_foreground(cmd.status.color())
                        .clone(),
                ),
                Change::Text(format!("{} ", status_char)),
                Change::AllAttributes(Default::default()),
            ]);

            let status_field_width: usize = 16;
            let title_width = self.screen_cols.saturating_sub(40);
            let title: String = cmd.title().chars().take(title_width).collect();
            if is_selected {
                changes.push(Change::AllAttributes(
                    CellAttributes::default()
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
            let status_attrs = CellAttributes::default()
                .set_foreground(cmd.status.color())
                .clone();
            let mut status_len: usize = 1 + status_label.len();
            if let Some(ref code) = exit_code_string {
                status_len += 3 + code.len();
            }
            let status_padding = status_field_width.saturating_sub(status_len);

            changes.extend([
                Change::AllAttributes(status_attrs.clone()),
                Change::Text(" ".to_string()),
                Change::Text(status_label.to_string()),
            ]);
            if let Some(code) = exit_code_string {
                changes.extend([
                    Change::AllAttributes(Default::default()),
                    Change::Text(" [".to_string()),
                    Change::AllAttributes(status_attrs.clone()),
                    Change::Text(code),
                    Change::AllAttributes(Default::default()),
                    Change::Text("]".to_string()),
                ]);
            }
            if status_padding > 0 {
                changes.extend([
                    Change::AllAttributes(Default::default()),
                    Change::Text(" ".repeat(status_padding)),
                ]);
            }
            changes.push(Change::AllAttributes(Default::default()));

            let elapsed = cmd.elapsed_str();
            changes.push(Change::Text(format!(" {:>6}", elapsed)));

            if is_selected {
                let current_len = 2 + label_width + 2 + title_width + status_field_width + 1 + 6;
                let padding = self.screen_cols.saturating_sub(current_len);
                changes.push(Change::Text(" ".repeat(padding)));
            }
        }

        changes.extend([
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(self.screen_rows - 1),
            },
            Change::Text(" ".repeat(self.screen_cols)),
        ]);

        term.render(&changes)?;
        term.flush()?;

        Ok(())
    }

    fn render_output_view(&mut self, term: &mut TermWizTerminal) -> anyhow::Result<()> {
        let cmd_idx = match self.current_command_idx() {
            Some(idx) => idx,
            None => return Ok(()),
        };
        let (status, cmd_title, status_color, exit_code) = {
            let cmd = &self.commands[cmd_idx];
            let status = cmd.status.clone();
            let cmd_title = cmd.title().to_string();
            let status_color = cmd.status.color();
            let exit_code = match cmd.status {
                CommandStatus::Failed(code) => Some(code.to_string()),
                _ => None,
            };
            (status, cmd_title, status_color, exit_code)
        };

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
        let status = match status {
            CommandStatus::Pending => "Pending",
            CommandStatus::Running => "Running",
            CommandStatus::Success(_) => "Success",
            CommandStatus::Failed(_) => "Failed",
            CommandStatus::Killed => "Killed",
        };
        let label_fg = self.colors.output_label_fg;
        let separator_fg = self.colors.separator_fg;
        let margin_fg = self.colors.margin_fg;
        let line_number_fg = self.colors.line_number_fg;

        // Header line: Command: <title> │ Status: <status> [│ Exit Code: <code>]
        {
            let mut writer = SegmentWriter::new(&mut changes, self.screen_cols);
            writer.push("Command", Some(label_fg));
            writer.push(":", None);
            writer.push(" ", None);
            writer.push(&cmd_title, None);
            writer.push(" │ ", Some(separator_fg));
            writer.push("Status", Some(label_fg));
            writer.push(":", None);
            writer.push(" ", None);
            writer.push(status, Some(status_color));
            if let Some(code) = exit_code.as_deref() {
                writer.push(" │ ", Some(separator_fg));
                writer.push("Exit Code", Some(label_fg));
                writer.push(":", None);
                writer.push(" ", None);
                writer.push(code, Some(status_color));
            }
        }

        let (search_pattern, mode, context) = self.get_filter_params(cmd_idx);
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
        // Search line: Search: <pattern>
        {
            let mut writer = SegmentWriter::new(&mut changes, self.screen_cols);
            writer.push("Search", Some(label_fg));
            writer.push(":", None);
            writer.push(" ", None);
            writer.push(&search_pattern, None);
            writer.fill_remaining();
        }
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(2),
        });
        // Context line: Context: ±<n> │ Mode: <mode>
        {
            let mut writer = SegmentWriter::new(&mut changes, self.screen_cols);
            writer.push("Context", Some(label_fg));
            writer.push(":", None);
            writer.push(" ", None);
            writer.push(&format!("±{}", context), None);
            writer.push(" │ ", Some(separator_fg));
            writer.push("Mode", Some(label_fg));
            writer.push(":", None);
            writer.push(" ", None);
            writer.push(mode.display(), None);
            writer.fill_remaining();
        }
        changes.extend([
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(3),
            },
            AttributeChange::Foreground(margin_fg).into(),
            Change::Text("─".repeat(self.screen_cols)),
            Change::AllAttributes(Default::default()),
        ]);

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
        let current_match_id = if self.current_match_command == Some(cmd_idx) {
            self.current_match_idx.and_then(|idx| {
                let match_locations = Self::match_locations_from_rows(rows.as_ref());
                match_locations.get(idx).map(|loc| loc.match_id)
            })
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
        changes.extend([
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(self.screen_rows - 1),
            },
            Change::Text(" ".repeat(self.screen_cols)),
        ]);

        if let Some(cursor_x) = filter_cursor_x {
            changes.extend([
                Change::CursorVisibility(CursorVisibility::Visible),
                Change::CursorPosition {
                    x: Position::Absolute(cursor_x),
                    y: Position::Absolute(1),
                },
            ]);
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

        changes.extend([
            Change::CursorPosition {
                x: Position::Absolute(x_pos),
                y: Position::Absolute(button_row),
            },
            Change::Text("[Y]es    [N]o".to_string()),
        ]);

        term.render(&changes)?;
        term.flush()?;

        Ok(())
    }

    fn handle_input(
        &mut self,
        event: InputEvent,
        process_tx: &Sender<ProcessMessage>,
    ) -> ControlFlow {
        match &self.view_mode {
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
            // Alphabet-driven selection labels
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers,
            }) if (modifiers == Modifiers::NONE || modifiers == Modifiers::SHIFT)
                && self.list_alphabet.contains(c) =>
            {
                self.reset_count();
                self.list_selection_input.push(c);
                let labels = self.list_labels();
                if let Some(pos) = labels
                    .iter()
                    .position(|label| label == &self.list_selection_input)
                {
                    self.list_selection = pos;
                    self.list_selection_input.clear();
                    self.open_output_view(self.list_selection);
                } else if !labels
                    .iter()
                    .any(|label| label.starts_with(&self.list_selection_input))
                {
                    self.list_selection_input.clear();
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                ..
            }) => {
                self.reset_count();
                self.list_selection_input.pop();
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('j'),
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::DownArrow,
                ..
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                let max = self.commands.len().saturating_sub(1);
                self.list_selection = (self.list_selection + 1).min(max);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('k'),
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::UpArrow,
                ..
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                self.list_selection = self.list_selection.saturating_sub(1);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('g'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                self.list_selection = 0;
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('G'),
                modifiers: Modifiers::SHIFT | Modifiers::NONE,
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                self.list_selection = self.commands.len().saturating_sub(1);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                self.open_output_view(self.list_selection);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('r'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                if self.list_selection < self.commands.len() {
                    let _ = process_tx.try_send(ProcessMessage::Rerun(self.list_selection));
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('C'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                if self.list_selection < self.commands.len() {
                    let _ = process_tx.try_send(ProcessMessage::Kill(self.list_selection));
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('q'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
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
                self.list_selection_input.clear();
            }
            _ => {
                self.reset_count();
                self.list_selection_input.clear();
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
                if c != '0' || !self.count_buffer.is_empty() {
                    self.count_buffer.push(c);
                }
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
                let (pattern, mode, context) = self.get_filter_params(command_idx);
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
                self.list_selection_input.clear();
                self.view_mode = ViewMode::List;
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('q'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.clear_active_filter();
                self.list_selection_input.clear();
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
                    self.clear_filtered_lines();
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
                    self.clear_filtered_lines();
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
                        self.clear_filtered_lines();
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
                self.list_selection_input.clear();
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
                        cmd.output_lines.clear();
                        cmd.line_byte_lengths.clear();
                        cmd.pending_line.clear();
                        cmd.output_size = 0;
                        cmd.output_generation = cmd.output_generation.wrapping_add(1);
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
