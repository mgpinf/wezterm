use crate::overlay::quickselect;
use config::configuration;
use config::keyassignment::{CommandRunner, CommandRunnerCommand};
use mux::termwiztermtab::TermWizTerminal;
use regex::Regex;
use smol::channel::{Receiver, Sender};
use smol::io::AsyncReadExt;
use smol::process::{Child, Command, Stdio};
use std::collections::VecDeque;
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

/// Keep output processing cooperative so that a continuously streaming
/// command cannot starve terminal input and rendering.
const MAX_OUTPUT_MESSAGES_PER_TICK: usize = 64;
const MAX_OUTPUT_BYTES_PER_TICK: usize = 256 * 1024;
const OUTPUT_DRAIN_TIME_BUDGET: Duration = Duration::from_millis(4);

/// Cap expensive wrapping and terminal rendering while logs are streaming.
const RENDER_INTERVAL: Duration = Duration::from_millis(33);
/// Active filters rescan the retained buffer, so update them at a lower cadence.
const FILTER_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const FILTER_INPUT_DEBOUNCE: Duration = Duration::from_millis(100);
const ASYNC_FILTER_OUTPUT_THRESHOLD: usize = 128 * 1024;
const ELAPSED_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const MAX_INPUT_POLL_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, Copy)]
struct CommandRunnerColors {
    list_header_fg: ColorAttribute,
    list_marker_fg: ColorAttribute,
    output_label_fg: ColorAttribute,
    output_context_label_fg: ColorAttribute,
    output_context_value_fg: ColorAttribute,
    separator_fg: ColorAttribute,
    margin_fg: ColorAttribute,
    line_number_fg: ColorAttribute,
    active_line_number_fg: ColorAttribute,
    current_line_fg: Option<ColorAttribute>,
    current_line_bg: Option<ColorAttribute>,
    match_fg: ColorAttribute,
    match_bg: ColorAttribute,
    current_match_fg: ColorAttribute,
    current_match_bg: ColorAttribute,
}

impl CommandRunnerColors {
    fn new() -> Self {
        let config = configuration();
        let colors = &config.resolved_palette;
        let separator_fg = colors
            .command_runner_output_separator_fg
            .map_or_else(|| ColorAttribute::Default, |fg| fg.into());

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
            output_context_label_fg: colors
                .command_runner_output_context_label_fg
                .map_or(separator_fg, |fg| fg.into()),
            output_context_value_fg: colors
                .command_runner_output_context_value_fg
                .map_or(separator_fg, |fg| fg.into()),
            separator_fg,
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
            current_line_fg: colors
                .command_runner_output_current_line_fg
                .map(|fg| fg.into()),
            current_line_bg: colors
                .command_runner_output_current_line_bg
                .map(|bg| bg.into()),
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
        SearchMode::CaseSmart => {
            if pattern.chars().any(|c| c.is_uppercase()) {
                Regex::new(&regex::escape(pattern))
            } else {
                Regex::new(&format!("(?i){}", regex::escape(pattern)))
            }
        }

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
            if let Some(start_width) = start_width {
                return (start_width, end_width.unwrap());
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

fn contextual_line_indices(
    line_matches: impl ExactSizeIterator<Item = bool>,
    context: usize,
) -> Vec<usize> {
    let line_count = line_matches.len();
    let mut include = vec![false; line_count];
    for (line_idx, is_match) in line_matches.enumerate() {
        if is_match {
            let start = line_idx.saturating_sub(context);
            let end = line_idx
                .saturating_add(context)
                .saturating_add(1)
                .min(line_count);
            for included in &mut include[start..end] {
                *included = true;
            }
        }
    }

    include
        .into_iter()
        .enumerate()
        .filter_map(|(line_idx, included)| included.then_some(line_idx))
        .collect()
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

    // Helper to compute local highlights for a segment
    fn compute_segment_highlights(
        highlights: &[HighlightRange],
        start_cell: usize,
        width: usize,
    ) -> Vec<HighlightRange> {
        let end_cell = start_cell + width;
        let mut segment_highlights: Vec<HighlightRange> = highlights
            .iter()
            .filter_map(|hl| {
                if hl.start < end_cell && hl.end > start_cell {
                    let local_start = hl.start.saturating_sub(start_cell);
                    let local_end = (hl.end - start_cell).min(width);
                    if local_start < local_end {
                        return Some(HighlightRange {
                            start: local_start,
                            end: local_end,
                            match_id: hl.match_id,
                        });
                    }
                }
                None
            })
            .collect();
        segment_highlights.sort_by_key(|hl| hl.start);
        segment_highlights
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
            let segment_highlights =
                compute_segment_highlights(highlights, current_start_cell, current_width);

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
        let segment_highlights =
            compute_segment_highlights(highlights, current_start_cell, current_width);

        segments.push(WrappedSegment {
            text: segment_text,
            highlights: segment_highlights,
            line_number: next_line_number,
        });
    }

    segments
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

fn push_text_with_highlights(
    changes: &mut Vec<Change>,
    segment: &WrappedSegment,
    colors: &CommandRunnerColors,
    current_match_id: Option<usize>,
    line_fg: Option<ColorAttribute>,
    line_bg: Option<ColorAttribute>,
) {
    let has_line_style = line_fg.is_some() || line_bg.is_some();
    let apply_line_style = |changes: &mut Vec<Change>| {
        if let Some(bg) = line_bg {
            changes.push(AttributeChange::Background(bg).into());
        }
        if let Some(fg) = line_fg {
            changes.push(AttributeChange::Foreground(fg).into());
        }
    };

    if segment.highlights.is_empty() {
        if has_line_style {
            apply_line_style(changes);
        }
        changes.push(Change::Text(segment.text.to_string()));
        if has_line_style {
            changes.push(Change::AllAttributes(Default::default()));
        }
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
            if has_line_style {
                apply_line_style(changes);
            }
            changes.push(Change::Text(
                segment.text[last_byte..start_byte].to_string(),
            ));
            if has_line_style {
                changes.push(Change::AllAttributes(Default::default()));
            }
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
        if has_line_style {
            apply_line_style(changes);
        }
        changes.push(Change::Text(segment.text[last_byte..].to_string()));
        if has_line_style {
            changes.push(Change::AllAttributes(Default::default()));
        }
    }
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

    fn push_bold_label(&mut self, text: &str, color: Option<ColorAttribute>) {
        if self.remaining == 0 || text.is_empty() {
            return;
        }

        let segment: String = text.chars().take(self.remaining).collect();
        self.remaining = self.remaining.saturating_sub(segment.chars().count());

        self.changes
            .push(AttributeChange::Intensity(Intensity::Bold).into());
        if let Some(color) = color {
            self.changes.push(AttributeChange::Foreground(color).into());
        }
        self.changes.push(Change::Text(segment));
        self.changes.push(Change::AllAttributes(Default::default()));
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
    pending_utf8: Vec<u8>,
    output_size: usize,
    output_generation: u64,
    run_generation: u64,
    first_line_sequence: u64,
    next_line_sequence: u64,
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
            pending_utf8: Vec::new(),
            output_size: 0,
            output_generation: 0,
            run_generation: 0,
            first_line_sequence: 0,
            next_line_sequence: 0,
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

    fn clear_output(&mut self) {
        self.output_lines.clear();
        self.line_byte_lengths.clear();
        self.pending_utf8.clear();
        self.output_size = 0;
        self.output_generation = self.output_generation.wrapping_add(1);
        self.first_line_sequence = self.next_line_sequence;
    }

    fn begin_new_run(&mut self) {
        self.run_generation = self.run_generation.wrapping_add(1);
        self.status = CommandStatus::Running;
        self.clear_output();
        self.start_time = Some(Instant::now());
        self.end_time = None;
    }

    fn accepts_output(&self, run_generation: u64) -> bool {
        self.run_generation == run_generation
    }

    fn append_output(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.output_size = self.output_size.saturating_add(data.len());
        self.update_lines_from_data(data);
        self.trim_output_to_limit();
        self.output_generation = self.output_generation.wrapping_add(1);
    }

    fn decode_pending_utf8(&mut self, final_chunk: bool) {
        if self.pending_utf8.is_empty() {
            return;
        }

        let bytes = std::mem::take(&mut self.pending_utf8);
        let mut decoded = String::new();
        let mut offset = 0;
        while offset < bytes.len() {
            match std::str::from_utf8(&bytes[offset..]) {
                Ok(valid) => {
                    decoded.push_str(valid);
                    offset = bytes.len();
                }
                Err(error) => {
                    let valid_end = offset + error.valid_up_to();
                    if valid_end > offset {
                        if let Ok(valid) = std::str::from_utf8(&bytes[offset..valid_end]) {
                            decoded.push_str(valid);
                        }
                    }

                    match error.error_len() {
                        Some(error_len) => {
                            decoded.push('\u{fffd}');
                            offset = valid_end.saturating_add(error_len);
                        }
                        None if final_chunk => {
                            decoded.push_str(&String::from_utf8_lossy(&bytes[valid_end..]));
                            offset = bytes.len();
                        }
                        None => {
                            self.pending_utf8.extend_from_slice(&bytes[valid_end..]);
                            offset = bytes.len();
                        }
                    }
                }
            }
        }

        if let Some(line) = self.output_lines.back_mut() {
            line.push_str(&decoded);
        }
    }

    fn ensure_current_line(&mut self) {
        if self.output_lines.is_empty() {
            self.output_lines.push_back(String::new());
            self.line_byte_lengths.push_back(0);
            self.next_line_sequence = self.next_line_sequence.wrapping_add(1);
        }
    }

    fn append_to_current_line(&mut self, data: &[u8]) {
        self.ensure_current_line();
        if data.is_empty() {
            return;
        }

        self.pending_utf8.extend_from_slice(data);
        if let Some(line_len) = self.line_byte_lengths.back_mut() {
            *line_len = line_len.saturating_add(data.len());
        }
        self.decode_pending_utf8(false);
    }

    fn finish_current_line(&mut self) {
        self.ensure_current_line();
        self.decode_pending_utf8(true);
        if let Some(line) = self.output_lines.back_mut() {
            if line.ends_with('\r') {
                line.pop();
            }
        }

        self.output_lines.push_back(String::new());
        self.line_byte_lengths.push_back(0);
        self.next_line_sequence = self.next_line_sequence.wrapping_add(1);
    }

    fn update_lines_from_data(&mut self, data: &[u8]) {
        let mut segment_start = 0;
        for (idx, byte) in data.iter().copied().enumerate() {
            if byte == b'\n' {
                self.append_to_current_line(&data[segment_start..idx]);
                self.finish_current_line();
                segment_start = idx + 1;
            }
        }
        self.append_to_current_line(&data[segment_start..]);
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
            self.first_line_sequence = self.first_line_sequence.wrapping_add(1);
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
                    let mut drop_decoded = drop_in_line.min(line.len());
                    while drop_decoded < line.len() && !line.is_char_boundary(drop_decoded) {
                        drop_decoded += 1;
                    }
                    line.drain(..drop_decoded);
                    if let Some(len) = self.line_byte_lengths.front_mut() {
                        *len = len.saturating_sub(drop_in_line);
                    }
                    if drop_in_line >= line_len {
                        self.pending_utf8.clear();
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
    CaseSmart,
    Regex,
}

impl SearchMode {
    fn next(self) -> Self {
        match self {
            SearchMode::CaseSensitive => SearchMode::CaseInsensitive,
            SearchMode::CaseInsensitive => SearchMode::CaseSmart,
            SearchMode::CaseSmart => SearchMode::Regex,
            SearchMode::Regex => SearchMode::CaseSensitive,
        }
    }

    fn display(&self) -> &'static str {
        match self {
            SearchMode::CaseInsensitive => "case-insensitive",
            SearchMode::CaseSensitive => "case-sensitive",
            SearchMode::CaseSmart => "case-smart",
            SearchMode::Regex => "regex",
        }
    }
}

/// View mode for the command runner
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
struct FilterMatchCache {
    command_idx: usize,
    pattern: String,
    mode: SearchMode,
    output_generation: u64,
    first_line_sequence: u64,
    line_matches: VecDeque<bool>,
}

struct PendingFilterRequest {
    request_generation: u64,
    command_idx: usize,
    pattern: String,
    mode: SearchMode,
    context: usize,
    requested_at: Instant,
    immediate: bool,
}

impl PendingFilterRequest {
    fn is_due(&self, now: Instant) -> bool {
        self.immediate || now.saturating_duration_since(self.requested_at) >= FILTER_INPUT_DEBOUNCE
    }
}

struct FilterWork {
    request_generation: u64,
    command_idx: usize,
    command_run_generation: u64,
    output_generation: u64,
    first_line_sequence: u64,
    pattern: String,
    mode: SearchMode,
    context: usize,
    lines: Vec<String>,
}

struct FilterJobResult {
    request_generation: u64,
    command_idx: usize,
    command_run_generation: u64,
    output_generation: u64,
    first_line_sequence: u64,
    pattern: String,
    mode: SearchMode,
    context: usize,
    regex: Option<Regex>,
    line_matches: Vec<bool>,
}

fn compute_filter_job(work: FilterWork) -> FilterJobResult {
    let regex = compile_search_regex(&work.pattern, work.mode);
    let line_matches = regex
        .as_ref()
        .map(|regex| work.lines.iter().map(|line| regex.is_match(line)).collect())
        .unwrap_or_else(|| vec![false; work.lines.len()]);

    FilterJobResult {
        request_generation: work.request_generation,
        command_idx: work.command_idx,
        command_run_generation: work.command_run_generation,
        output_generation: work.output_generation,
        first_line_sequence: work.first_line_sequence,
        pattern: work.pattern,
        mode: work.mode,
        context: work.context,
        regex,
        line_matches,
    }
}

fn update_filter_match_cache(
    command: &CommandState,
    cache: &mut FilterMatchCache,
    command_idx: usize,
    pattern: &str,
    mode: SearchMode,
    regex: &Regex,
) -> bool {
    if cache.command_idx != command_idx || cache.pattern != pattern || cache.mode != mode {
        return false;
    }
    if cache.output_generation == command.output_generation {
        return true;
    }

    let Some(removed_line_count) = command
        .first_line_sequence
        .checked_sub(cache.first_line_sequence)
    else {
        return false;
    };
    if removed_line_count > cache.line_matches.len() as u64 {
        return false;
    }
    let removed_line_count = removed_line_count as usize;
    cache.line_matches.drain(..removed_line_count);
    if cache.line_matches.len() > command.output_lines.len() {
        return false;
    }

    // The current tail can change until it is terminated by a newline.
    cache.line_matches.pop_back();
    let reused_line_count = cache.line_matches.len();
    cache.line_matches.extend(
        command
            .output_lines
            .iter()
            .skip(reused_line_count)
            .map(|line| regex.is_match(line)),
    );
    cache.output_generation = command.output_generation;
    cache.first_line_sequence = command.first_line_sequence;
    true
}

#[derive(Debug, Clone)]
struct WrappedRowCountCache {
    command_idx: usize,
    max_width: usize,
    output_generation: u64,
    first_line_sequence: u64,
    line_row_counts: VecDeque<usize>,
    total_rows: usize,
}

#[derive(Debug, Clone)]
struct FilteredLayoutCache {
    command_idx: usize,
    max_width: usize,
    filtered_generation: u64,
    regex_pattern: Option<String>,
    line_indices: Vec<usize>,
    line_start_rows: Vec<usize>,
    line_row_counts: Vec<usize>,
    line_match_starts: Vec<usize>,
    match_locations: Vec<MatchLocation>,
    total_rows: usize,
}

fn measure_line_and_append_match_locations(
    line: &str,
    max_width: usize,
    highlights: &[HighlightRange],
    line_start_row: usize,
    line_number: usize,
    locations: &mut Vec<MatchLocation>,
) -> usize {
    if max_width == 0 {
        return 1;
    }

    fn record_row_matches(
        highlights: &[HighlightRange],
        next_highlight: &mut usize,
        row_start: usize,
        row_end: usize,
        row_idx: usize,
        line_start_row: usize,
        line_number: usize,
        locations: &mut Vec<MatchLocation>,
    ) {
        while let Some(highlight) = highlights.get(*next_highlight) {
            if highlight.end <= row_start {
                *next_highlight += 1;
                continue;
            }
            if highlight.start >= row_end {
                break;
            }
            debug_assert_eq!(highlight.match_id, locations.len());
            locations.push(MatchLocation {
                row_idx: line_start_row.saturating_add(row_idx),
                line_number,
            });
            *next_highlight += 1;
        }
    }

    let mut row_idx = 0usize;
    let mut row_start = 0usize;
    let mut row_width = 0usize;
    let mut cell_idx = 0usize;
    let mut next_highlight = 0usize;
    for ch in line.chars() {
        let ch_width = char_column_width(ch);
        if row_width + ch_width > max_width && row_width > 0 {
            let row_end = row_start.saturating_add(row_width);
            record_row_matches(
                highlights,
                &mut next_highlight,
                row_start,
                row_end,
                row_idx,
                line_start_row,
                line_number,
                locations,
            );
            row_idx = row_idx.saturating_add(1);
            row_start = cell_idx;
            row_width = 0;
        }
        row_width = row_width.saturating_add(ch_width);
        cell_idx = cell_idx.saturating_add(ch_width);
    }

    let row_end = row_start.saturating_add(row_width);
    record_row_matches(
        highlights,
        &mut next_highlight,
        row_start,
        row_end,
        row_idx,
        line_start_row,
        line_number,
        locations,
    );
    row_idx.saturating_add(1)
}

fn build_filtered_layout(
    command: &CommandState,
    command_idx: usize,
    filtered_lines: &[usize],
    filtered_generation: u64,
    max_width: usize,
    regex: Option<&Regex>,
) -> FilteredLayoutCache {
    let mut line_indices = Vec::with_capacity(filtered_lines.len());
    let mut line_start_rows = Vec::with_capacity(filtered_lines.len());
    let mut line_row_counts = Vec::with_capacity(filtered_lines.len());
    let mut line_match_starts = Vec::with_capacity(filtered_lines.len());
    let mut match_locations = Vec::new();
    let mut total_rows = 0usize;
    let mut next_match_id = 0usize;

    for line_idx in filtered_lines {
        let Some(line) = command.output_lines.get(*line_idx) else {
            continue;
        };
        line_indices.push(*line_idx);
        line_start_rows.push(total_rows);
        line_match_starts.push(next_match_id);
        let highlights = regex
            .filter(|_| max_width > 0)
            .map(|regex| match_ranges_in_cells(line, regex, &mut next_match_id))
            .unwrap_or_default();
        let row_count = measure_line_and_append_match_locations(
            line,
            max_width,
            &highlights,
            total_rows,
            *line_idx + 1,
            &mut match_locations,
        );
        line_row_counts.push(row_count);
        total_rows = total_rows.saturating_add(row_count);
    }

    FilteredLayoutCache {
        command_idx,
        max_width,
        filtered_generation,
        regex_pattern: regex.map(|regex| regex.as_str().to_string()),
        line_indices,
        line_start_rows,
        line_row_counts,
        line_match_starts,
        match_locations,
        total_rows,
    }
}

fn filtered_line_position_at_row(cache: &FilteredLayoutCache, row_idx: usize) -> Option<usize> {
    if row_idx >= cache.total_rows {
        return None;
    }
    match cache.line_start_rows.binary_search(&row_idx) {
        Ok(line_position) => Some(line_position),
        Err(0) => None,
        Err(line_position) => Some(line_position - 1),
    }
}

fn wrap_filtered_visible_rows(
    command: &CommandState,
    cache: &FilteredLayoutCache,
    regex: Option<&Regex>,
    start_row: usize,
    row_count: usize,
) -> VecDeque<WrappedSegment> {
    if row_count == 0 {
        return VecDeque::new();
    }
    let Some(first_line_position) = filtered_line_position_at_row(cache, start_row) else {
        return VecDeque::new();
    };
    let mut line_start_row = cache.line_start_rows[first_line_position];
    let mut next_match_id = cache.line_match_starts[first_line_position];

    let mut rows = VecDeque::with_capacity(row_count);
    for line_position in first_line_position..cache.line_indices.len() {
        if rows.len() >= row_count {
            break;
        }
        let line_idx = cache.line_indices[line_position];
        let Some(line) = command.output_lines.get(line_idx) else {
            continue;
        };
        let highlights = regex
            .map(|regex| match_ranges_in_cells(line, regex, &mut next_match_id))
            .unwrap_or_default();
        let segments =
            wrap_line_with_highlights(line, &highlights, cache.max_width, Some(line_idx + 1));
        let skip = start_row.saturating_sub(line_start_row);
        rows.extend(segments.into_iter().skip(skip).take(row_count - rows.len()));
        line_start_row = line_start_row.saturating_add(cache.line_row_counts[line_position]);
    }
    rows
}

fn update_wrapped_row_count_cache(
    command: &CommandState,
    cache: &mut WrappedRowCountCache,
    command_idx: usize,
    max_width: usize,
) -> bool {
    if cache.command_idx != command_idx || cache.max_width != max_width {
        return false;
    }
    if cache.output_generation == command.output_generation {
        return true;
    }

    let Some(removed_line_count) = command
        .first_line_sequence
        .checked_sub(cache.first_line_sequence)
    else {
        return false;
    };
    if removed_line_count > cache.line_row_counts.len() as u64 {
        return false;
    }
    let removed_line_count = removed_line_count as usize;
    let removed_row_count = cache
        .line_row_counts
        .iter()
        .take(removed_line_count)
        .copied()
        .sum::<usize>();
    cache.line_row_counts.drain(..removed_line_count);
    cache.total_rows = cache.total_rows.saturating_sub(removed_row_count);
    if cache.line_row_counts.len() > command.output_lines.len() {
        return false;
    }

    if let Some(last_row_count) = cache.line_row_counts.pop_back() {
        cache.total_rows = cache.total_rows.saturating_sub(last_row_count);
    }
    let reused_line_count = cache.line_row_counts.len();
    for line in command.output_lines.iter().skip(reused_line_count) {
        let row_count = wrapped_row_count(line, max_width);
        cache.line_row_counts.push_back(row_count);
        cache.total_rows = cache.total_rows.saturating_add(row_count);
    }
    cache.output_generation = command.output_generation;
    cache.first_line_sequence = command.first_line_sequence;
    true
}

fn line_index_at_wrapped_row(
    cache: &WrappedRowCountCache,
    row_idx: usize,
) -> Option<(usize, usize)> {
    if row_idx >= cache.total_rows {
        return None;
    }

    if row_idx < cache.total_rows / 2 {
        let mut line_start_row: usize = 0;
        for (line_idx, row_count) in cache.line_row_counts.iter().copied().enumerate() {
            if row_idx < line_start_row.saturating_add(row_count) {
                return Some((line_idx, line_start_row));
            }
            line_start_row = line_start_row.saturating_add(row_count);
        }
    } else {
        let mut line_end_row = cache.total_rows;
        for (line_idx, row_count) in cache.line_row_counts.iter().copied().enumerate().rev() {
            let line_start_row = line_end_row.saturating_sub(row_count);
            if row_idx >= line_start_row {
                return Some((line_idx, line_start_row));
            }
            line_end_row = line_start_row;
        }
    }
    None
}

fn clamp_paused_view(
    total_rows: usize,
    visible_rows: usize,
    scroll_offset: usize,
    current_line: Option<usize>,
) -> (usize, Option<usize>) {
    let scroll_offset = scroll_offset.min(total_rows.saturating_sub(visible_rows));
    let current_line = if total_rows == 0 {
        None
    } else {
        Some(
            current_line
                .unwrap_or(scroll_offset)
                .min(total_rows.saturating_sub(1)),
        )
    };
    (scroll_offset, current_line)
}

/// Main state for the command runner applet
struct CommandRunnerState {
    commands: Vec<CommandState>,
    view_mode: ViewMode,
    list_selection: usize,
    scroll_offset: usize,
    filtered_lines: Vec<usize>, // Original indexes into the current command's retained lines
    active_filter: Option<ActiveFilter>, // Persists filter params for streaming updates
    displayed_filter: Option<ActiveFilter>,
    filter_edit_original_lines: Option<Vec<usize>>,
    pending_filter_request: Option<PendingFilterRequest>,
    filter_request_generation: u64,
    filter_job_in_flight: Option<(u64, usize)>,
    filtered_generation: u64,
    wrapped_row_count_cache: Option<WrappedRowCountCache>,
    filtered_layout_cache: Option<FilteredLayoutCache>,
    regex_cache: Option<RegexCache>,
    filter_match_cache: Option<FilterMatchCache>,
    current_match_idx: Option<usize>,
    current_match_command: Option<usize>,
    current_line_idx: Option<usize>,
    current_line_command: Option<usize>,
    follow_output: bool,
    auto_close_on_success: bool,
    screen_rows: usize,
    screen_cols: usize,
    count_buffer: String,
    list_selection_input: String,
    list_alphabet: String,
    colors: CommandRunnerColors,
    window: Window,
    last_output_render: Option<(usize, usize, usize)>,
}

// ============================================================================
// Construction and Core State
// ============================================================================

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
            displayed_filter: None,
            filter_edit_original_lines: None,
            pending_filter_request: None,
            filter_request_generation: 0,
            filter_job_in_flight: None,
            filtered_generation: 0,
            wrapped_row_count_cache: None,
            filtered_layout_cache: None,
            regex_cache: None,
            filter_match_cache: None,
            current_match_idx: None,
            current_match_command: None,
            current_line_idx: None,
            current_line_command: None,
            follow_output: true,
            auto_close_on_success: args.auto_close_on_success,
            screen_rows: 24,
            screen_cols: 80,
            count_buffer: String::new(),
            list_selection_input: String::new(),
            list_alphabet: args.alphabet,
            colors: CommandRunnerColors::new(),
            window,
            last_output_render: None,
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

    fn current_command_idx(&self) -> Option<usize> {
        match &self.view_mode {
            ViewMode::Output { command_idx } | ViewMode::Filter { command_idx, .. } => {
                Some(*command_idx)
            }
            _ => None,
        }
    }
}

// ============================================================================
// Output Configuration and Filter State
// ============================================================================

impl CommandRunnerState {
    fn output_line_count(&mut self) -> usize {
        match self.current_command_idx() {
            Some(idx) => self.output_wrapped_row_count(idx),
            None => 0,
        }
    }

    fn clear_command_output(&mut self, command_idx: usize) {
        let Some(cmd) = self.commands.get_mut(command_idx) else {
            return;
        };

        cmd.clear_output();

        if self.current_command_idx() == Some(command_idx) {
            self.scroll_offset = 0;
            self.clear_current_match();
            self.clear_filtered_lines();
            self.reset_current_line_to_scroll_offset();
        }
    }

    fn filter_active_for(&self, command_idx: usize) -> bool {
        self.displayed_filter
            .as_ref()
            .filter(|filter| filter.command_idx == command_idx && !filter.pattern.is_empty())
            .is_some()
    }

    fn filter_refresh_active_for(&self, command_idx: usize) -> bool {
        let (pattern, _, _) = self.get_filter_params(command_idx);
        !pattern.is_empty() || self.filter_active_for(command_idx)
    }

    fn filter_update_pending_for(&self, command_idx: usize) -> bool {
        self.pending_filter_request
            .as_ref()
            .filter(|request| {
                request.command_idx == command_idx
                    && request.request_generation == self.filter_request_generation
            })
            .is_some()
            || self
                .filter_job_in_flight
                .filter(|(generation, idx)| {
                    *idx == command_idx && *generation == self.filter_request_generation
                })
                .is_some()
    }

    fn line_number_width_for(&self, command_idx: usize) -> usize {
        let max_line_idx = if !self.filtered_lines.is_empty() {
            self.filtered_lines.iter().copied().max()
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
        let (pattern, mode) = self
            .displayed_filter
            .as_ref()
            .filter(|filter| filter.command_idx == command_idx && !filter.pattern.is_empty())
            .map(|filter| (filter.pattern.clone(), filter.mode))?;
        self.cached_regex(&pattern, mode)
    }
}

// ============================================================================
// Row Wrapping and Caching
// ============================================================================

impl CommandRunnerState {
    fn ensure_filtered_layout(&mut self, command_idx: usize, regex: Option<&Regex>) -> bool {
        let max_width = self.output_content_width_for(command_idx);
        let regex_pattern = regex.map(|regex| regex.as_str());
        if self
            .filtered_layout_cache
            .as_ref()
            .filter(|cache| {
                cache.command_idx == command_idx
                    && cache.max_width == max_width
                    && cache.filtered_generation == self.filtered_generation
                    && cache.regex_pattern.as_deref() == regex_pattern
            })
            .is_some()
        {
            return true;
        }

        let Some(command) = self.commands.get(command_idx) else {
            self.filtered_layout_cache = None;
            return false;
        };
        self.filtered_layout_cache = Some(build_filtered_layout(
            command,
            command_idx,
            &self.filtered_lines,
            self.filtered_generation,
            max_width,
            regex,
        ));
        true
    }

    fn filtered_wrapped_row_count(&mut self, command_idx: usize, regex: Option<&Regex>) -> usize {
        if !self.ensure_filtered_layout(command_idx, regex) {
            return 0;
        }
        self.filtered_layout_cache
            .as_ref()
            .map(|cache| cache.total_rows)
            .unwrap_or(0)
    }

    fn filtered_line_number_at_row(
        &mut self,
        command_idx: usize,
        regex: Option<&Regex>,
        row_idx: usize,
    ) -> Option<usize> {
        self.ensure_filtered_layout(command_idx, regex);
        let cache = self.filtered_layout_cache.as_ref()?;
        let line_position = filtered_line_position_at_row(cache, row_idx)?;
        cache
            .line_indices
            .get(line_position)
            .map(|line_idx| line_idx + 1)
    }

    fn filtered_row_for_line_number(
        &mut self,
        command_idx: usize,
        regex: Option<&Regex>,
        line_number: usize,
    ) -> Option<usize> {
        self.ensure_filtered_layout(command_idx, regex);
        let cache = self.filtered_layout_cache.as_ref()?;
        let line_idx = line_number.checked_sub(1)?;
        let line_position = cache.line_indices.binary_search(&line_idx).ok()?;
        cache.line_start_rows.get(line_position).copied()
    }

    fn filtered_visible_rows(
        &mut self,
        command_idx: usize,
        regex: Option<&Regex>,
        start_row: usize,
        row_count: usize,
    ) -> VecDeque<WrappedSegment> {
        if row_count == 0 || !self.ensure_filtered_layout(command_idx, regex) {
            return VecDeque::new();
        }
        let Some(cache) = self.filtered_layout_cache.as_ref() else {
            return VecDeque::new();
        };
        let Some(command) = self.commands.get(command_idx) else {
            return VecDeque::new();
        };
        wrap_filtered_visible_rows(command, cache, regex, start_row, row_count)
    }

    fn unfiltered_wrapped_row_count(&mut self, command_idx: usize) -> usize {
        let max_width = self.output_content_width_for(command_idx);
        let cache_updated = match (
            self.commands.get(command_idx),
            self.wrapped_row_count_cache.as_mut(),
        ) {
            (Some(command), Some(cache)) => {
                update_wrapped_row_count_cache(command, cache, command_idx, max_width)
            }
            _ => false,
        };

        if !cache_updated {
            let Some(command) = self.commands.get(command_idx) else {
                return 0;
            };
            let line_row_counts: VecDeque<usize> = command
                .output_lines
                .iter()
                .map(|line| wrapped_row_count(line, max_width))
                .collect();
            let total_rows = line_row_counts.iter().copied().sum();
            self.wrapped_row_count_cache = Some(WrappedRowCountCache {
                command_idx,
                max_width,
                output_generation: command.output_generation,
                first_line_sequence: command.first_line_sequence,
                line_row_counts,
                total_rows,
            });
        }

        self.wrapped_row_count_cache
            .as_ref()
            .map(|cache| cache.total_rows)
            .unwrap_or(0)
    }

    fn unfiltered_line_number_at_row(
        &mut self,
        command_idx: usize,
        row_idx: usize,
    ) -> Option<usize> {
        self.unfiltered_wrapped_row_count(command_idx);
        let cache = self.wrapped_row_count_cache.as_ref()?;
        line_index_at_wrapped_row(cache, row_idx).map(|(line_idx, _)| line_idx + 1)
    }

    fn unfiltered_visible_rows(
        &mut self,
        command_idx: usize,
        start_row: usize,
        row_count: usize,
    ) -> Vec<WrappedSegment> {
        self.unfiltered_wrapped_row_count(command_idx);
        let Some(cache) = self.wrapped_row_count_cache.as_ref() else {
            return Vec::new();
        };
        let Some(command) = self.commands.get(command_idx) else {
            return Vec::new();
        };

        let Some((first_line_idx, mut line_start_row)) =
            line_index_at_wrapped_row(cache, start_row)
        else {
            return Vec::new();
        };
        let mut rows = Vec::with_capacity(row_count);
        for line_idx in first_line_idx..command.output_lines.len() {
            let Some(line) = command.output_lines.get(line_idx) else {
                break;
            };
            let Some(line_row_count) = cache.line_row_counts.get(line_idx).copied() else {
                break;
            };
            let line_end_row = line_start_row.saturating_add(line_row_count);
            if rows.len() >= row_count {
                break;
            }

            let segments =
                wrap_line_with_highlights(line, &[], cache.max_width, Some(line_idx + 1));
            let skip = start_row.saturating_sub(line_start_row);
            rows.extend(segments.into_iter().skip(skip).take(row_count - rows.len()));
            line_start_row = line_end_row;
        }
        rows
    }

    fn output_wrapped_row_count(&mut self, command_idx: usize) -> usize {
        if !self.filter_active_for(command_idx) {
            self.unfiltered_wrapped_row_count(command_idx)
        } else {
            let regex = self.active_search_regex_for(command_idx);
            self.filtered_wrapped_row_count(command_idx, regex.as_ref())
        }
    }

    fn output_line_number_at_row(&mut self, command_idx: usize, row_idx: usize) -> Option<usize> {
        if self.filter_active_for(command_idx) {
            let regex = self.active_search_regex_for(command_idx);
            self.filtered_line_number_at_row(command_idx, regex.as_ref(), row_idx)
        } else {
            self.unfiltered_line_number_at_row(command_idx, row_idx)
        }
    }

    fn output_row_for_line_number(
        &mut self,
        command_idx: usize,
        line_number: usize,
    ) -> Option<usize> {
        if self.filter_active_for(command_idx) {
            let regex = self.active_search_regex_for(command_idx);
            return self.filtered_row_for_line_number(command_idx, regex.as_ref(), line_number);
        }

        self.unfiltered_wrapped_row_count(command_idx);
        let cache = self.wrapped_row_count_cache.as_ref()?;
        let line_idx = line_number.checked_sub(1)?;
        if line_idx >= cache.line_row_counts.len() {
            return None;
        }
        Some(cache.line_row_counts.iter().take(line_idx).copied().sum())
    }

    fn next_output_line_row(&mut self, command_idx: usize, row_idx: usize) -> Option<usize> {
        if self.filter_active_for(command_idx) {
            let regex = self.active_search_regex_for(command_idx);
            self.ensure_filtered_layout(command_idx, regex.as_ref());
            let cache = self.filtered_layout_cache.as_ref()?;
            let line_position = filtered_line_position_at_row(cache, row_idx)?;
            return cache.line_start_rows.get(line_position + 1).copied();
        }

        self.unfiltered_wrapped_row_count(command_idx);
        let cache = self.wrapped_row_count_cache.as_ref()?;
        let (line_idx, line_start_row) = line_index_at_wrapped_row(cache, row_idx)?;
        cache
            .line_row_counts
            .get(line_idx)
            .map(|row_count| line_start_row.saturating_add(*row_count))
            .filter(|next_row| *next_row < cache.total_rows)
    }

    fn previous_output_line_row(&mut self, command_idx: usize, row_idx: usize) -> Option<usize> {
        if self.filter_active_for(command_idx) {
            let regex = self.active_search_regex_for(command_idx);
            self.ensure_filtered_layout(command_idx, regex.as_ref());
            let cache = self.filtered_layout_cache.as_ref()?;
            let line_position = filtered_line_position_at_row(cache, row_idx)?;
            return line_position
                .checked_sub(1)
                .and_then(|previous| cache.line_start_rows.get(previous).copied());
        }

        self.unfiltered_wrapped_row_count(command_idx);
        let cache = self.wrapped_row_count_cache.as_ref()?;
        let (line_idx, _) = line_index_at_wrapped_row(cache, row_idx)?;
        let previous_line = line_idx.checked_sub(1)?;
        Some(
            cache
                .line_row_counts
                .iter()
                .take(previous_line)
                .copied()
                .sum(),
        )
    }
}

// ============================================================================
// Line Navigation
// ============================================================================

impl CommandRunnerState {
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
        self.follow_output = false;
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

    fn move_current_line(&mut self, command_idx: usize, delta: isize) {
        self.follow_output = false;
        let total_rows = self.output_wrapped_row_count(command_idx);
        let current = match self.current_line_for(command_idx, total_rows) {
            Some(idx) => idx,
            None => return,
        };

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
                self.next_output_line_row(command_idx, row_idx)
            } else {
                self.previous_output_line_row(command_idx, row_idx)
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
}

// ============================================================================
// Match Navigation
// ============================================================================

impl CommandRunnerState {
    fn ensure_current_match(&mut self, command_idx: usize, regex: &Regex) -> Option<MatchLocation> {
        self.ensure_filtered_layout(command_idx, Some(regex));
        let match_count = self
            .filtered_layout_cache
            .as_ref()
            .filter(|cache| cache.command_idx == command_idx)
            .map(|cache| cache.match_locations.len())
            .unwrap_or(0);
        if match_count == 0 {
            self.reset_current_match(command_idx);
            return None;
        }

        if self.current_match_command != Some(command_idx) {
            self.current_match_command = Some(command_idx);
            self.current_match_idx = None;
        }

        let idx = match self.current_match_idx {
            Some(idx) if idx < match_count => idx,
            _ => {
                self.current_match_idx = Some(0);
                self.current_match_command = Some(command_idx);
                0
            }
        };
        self.current_match_idx = Some(idx);
        self.filtered_layout_cache
            .as_ref()?
            .match_locations
            .get(idx)
            .copied()
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
        if self.active_search_regex_for(command_idx).is_none() {
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
        let line_number = self.output_line_number_at_row(command_idx, current_row);

        let text = if let Some(line_number) = line_number {
            let line_idx = line_number.saturating_sub(1);
            if self.filter_active_for(command_idx) && !self.filtered_lines.is_empty() {
                let start = self.filtered_lines.iter().position(|idx| *idx == line_idx);
                if let Some(start) = start {
                    let end = (start + count).min(self.filtered_lines.len());
                    let mut lines = Vec::with_capacity(end - start);
                    if let Some(command) = self.commands.get(command_idx) {
                        for filtered_idx in &self.filtered_lines[start..end] {
                            if let Some(line) = command.output_lines.get(*filtered_idx) {
                                lines.push(line.as_str());
                            }
                        }
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
            None
        };

        if let Some(text) = text {
            self.window.set_clipboard(Clipboard::Clipboard, text);
        }
    }

    fn move_current_match(&mut self, command_idx: usize, forward: bool) {
        self.follow_output = false;
        let regex = match self.active_search_regex_for(command_idx) {
            Some(regex) => regex,
            None => return,
        };
        if self.current_match_command != Some(command_idx) {
            self.reset_current_match(command_idx);
        }
        self.ensure_filtered_layout(command_idx, Some(&regex));
        let match_count = self
            .filtered_layout_cache
            .as_ref()
            .map(|cache| cache.match_locations.len())
            .unwrap_or(0);
        if match_count == 0 {
            self.reset_current_match(command_idx);
            return;
        }

        let anchor_line = if self.current_line_command == Some(command_idx) {
            match self.current_line_idx {
                Some(idx) => self.output_line_number_at_row(command_idx, idx),
                None => None,
            }
        } else {
            None
        };
        let (next_idx, row_idx) = {
            let matches = &self.filtered_layout_cache.as_ref().unwrap().match_locations;
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
                        .unwrap_or(match_count - 1)
                }
            } else {
                match self.current_match_idx {
                    Some(idx) if idx < match_count => {
                        if forward {
                            (idx + 1) % match_count
                        } else {
                            (idx + match_count - 1) % match_count
                        }
                    }
                    _ => {
                        if forward {
                            0
                        } else {
                            match_count - 1
                        }
                    }
                }
            };
            (next_idx, matches[next_idx].row_idx)
        };

        self.current_match_idx = Some(next_idx);
        self.current_line_command = Some(command_idx);
        self.current_line_idx = Some(row_idx);
        self.reveal_current_line(row_idx);
    }
}

// ============================================================================
// Filter Management
// ============================================================================

impl CommandRunnerState {
    fn refresh_streaming_view(&mut self, command_idx: usize) {
        if self.current_command_idx() != Some(command_idx) {
            return;
        }

        if self.filter_update_pending_for(command_idx) {
            return;
        }

        if self.filter_refresh_active_for(command_idx) {
            let (pattern, mode, context) = self.get_filter_params(command_idx);
            self.request_filter_update(command_idx, pattern, mode, context, true);
        } else if self.follow_output {
            self.scroll_to_bottom();
        }
    }

    fn reapply_filter(&mut self) {
        if let Some(command_idx) = self.current_command_idx() {
            let (pattern, mode, context) = self.get_filter_params(command_idx);
            self.request_filter_update(command_idx, pattern, mode, context, true);
        }
    }

    fn begin_filter_edit(&mut self, command_idx: usize) {
        self.follow_output = false;
        self.cancel_filter_update();
        self.filter_edit_original_lines = Some(self.filtered_lines.clone());
        self.displayed_filter = self
            .active_filter
            .as_ref()
            .filter(|filter| filter.command_idx == command_idx)
            .cloned();
    }

    fn cancel_filter_update(&mut self) {
        self.filter_request_generation = self.filter_request_generation.wrapping_add(1);
        self.pending_filter_request = None;
        self.filter_job_in_flight = None;
    }

    fn filter_query_is_current(
        &self,
        command_idx: usize,
        pattern: &str,
        mode: SearchMode,
        context: usize,
    ) -> bool {
        match &self.view_mode {
            ViewMode::Filter {
                command_idx: current_idx,
                pattern: current_pattern,
                mode: current_mode,
                context: current_context,
            } => {
                *current_idx == command_idx
                    && current_pattern == pattern
                    && *current_mode == mode
                    && *current_context == context
            }
            ViewMode::Output {
                command_idx: current_idx,
            } => {
                *current_idx == command_idx
                    && self
                        .active_filter
                        .as_ref()
                        .filter(|filter| filter.command_idx == command_idx)
                        .map(|filter| {
                            filter.pattern == pattern
                                && filter.mode == mode
                                && filter.context == context
                        })
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    fn request_filter_update(
        &mut self,
        command_idx: usize,
        pattern: String,
        mode: SearchMode,
        context: usize,
        immediate: bool,
    ) {
        self.cancel_filter_update();

        if pattern.is_empty() {
            self.displayed_filter = None;
            self.filter_match_cache = None;
            self.scroll_offset = 0;
            self.clear_current_match();
            self.clear_filtered_lines();
            self.reset_current_line_to_scroll_offset();
            return;
        }

        let Some(command) = self.commands.get(command_idx) else {
            return;
        };
        if command.output_size < ASYNC_FILTER_OUTPUT_THRESHOLD {
            self.apply_filter(&pattern, mode, context);
            return;
        }

        self.pending_filter_request = Some(PendingFilterRequest {
            request_generation: self.filter_request_generation,
            command_idx,
            pattern,
            mode,
            context,
            requested_at: Instant::now(),
            immediate,
        });
    }

    fn take_due_filter_work(&mut self) -> Option<FilterWork> {
        if !self
            .pending_filter_request
            .as_ref()
            .map(|request| request.is_due(Instant::now()))
            .unwrap_or(false)
        {
            return None;
        }

        let request = self.pending_filter_request.take()?;
        if request.request_generation != self.filter_request_generation
            || !self.filter_query_is_current(
                request.command_idx,
                &request.pattern,
                request.mode,
                request.context,
            )
        {
            return None;
        }

        let command = self.commands.get(request.command_idx)?;
        let work = FilterWork {
            request_generation: request.request_generation,
            command_idx: request.command_idx,
            command_run_generation: command.run_generation,
            output_generation: command.output_generation,
            first_line_sequence: command.first_line_sequence,
            pattern: request.pattern,
            mode: request.mode,
            context: request.context,
            lines: command.output_lines.iter().cloned().collect(),
        };
        self.filter_job_in_flight = Some((work.request_generation, work.command_idx));
        Some(work)
    }

    fn apply_filter_job_result(&mut self, result: FilterJobResult) -> bool {
        if self.filter_job_in_flight == Some((result.request_generation, result.command_idx)) {
            self.filter_job_in_flight = None;
        }
        if result.request_generation != self.filter_request_generation
            || !self.filter_query_is_current(
                result.command_idx,
                &result.pattern,
                result.mode,
                result.context,
            )
        {
            return false;
        }

        let Some(command) = self.commands.get(result.command_idx) else {
            return false;
        };
        if command.run_generation != result.command_run_generation {
            self.request_filter_update(
                result.command_idx,
                result.pattern,
                result.mode,
                result.context,
                true,
            );
            return false;
        }

        let preserve_view = self.filter_update_preserves_view(
            result.command_idx,
            &result.pattern,
            result.mode,
            result.context,
        );
        let previous_scroll_offset = self.scroll_offset;
        let previous_current_line = self.current_line_idx;

        let mut cache = FilterMatchCache {
            command_idx: result.command_idx,
            pattern: result.pattern.clone(),
            mode: result.mode,
            output_generation: result.output_generation,
            first_line_sequence: result.first_line_sequence,
            line_matches: result.line_matches.into(),
        };
        let cache_is_current = if let Some(regex) = result.regex.as_ref() {
            update_filter_match_cache(
                command,
                &mut cache,
                result.command_idx,
                &result.pattern,
                result.mode,
                regex,
            )
        } else {
            cache.output_generation = command.output_generation;
            cache.first_line_sequence = command.first_line_sequence;
            cache.line_matches = vec![false; command.output_lines.len()].into();
            true
        };
        if !cache_is_current {
            self.request_filter_update(
                result.command_idx,
                result.pattern,
                result.mode,
                result.context,
                true,
            );
            return false;
        }

        self.filtered_lines =
            contextual_line_indices(cache.line_matches.iter().copied(), result.context);
        self.filter_match_cache = Some(cache);
        self.displayed_filter = Some(ActiveFilter {
            command_idx: result.command_idx,
            pattern: result.pattern.clone(),
            mode: result.mode,
            context: result.context,
        });
        self.bump_filtered_generation();
        let follow_output = self.should_follow_output(result.command_idx);
        if !preserve_view {
            self.scroll_offset = 0;
        }

        if let Some(regex) = result.regex {
            self.regex_cache = Some(RegexCache {
                pattern: result.pattern,
                mode: result.mode,
                regex: regex.clone(),
            });
            if follow_output {
                self.clear_current_match();
                self.scroll_to_bottom();
            } else if preserve_view {
                self.restore_paused_filter_view(
                    result.command_idx,
                    previous_scroll_offset,
                    previous_current_line,
                );
            } else if let Some(current_match) =
                self.ensure_current_match(result.command_idx, &regex)
            {
                self.current_line_command = Some(result.command_idx);
                self.current_line_idx = Some(current_match.row_idx);
                self.reveal_current_line(current_match.row_idx);
            } else {
                self.reset_current_line_to_scroll_offset();
            }
        } else {
            self.clear_current_match();
            if preserve_view {
                self.restore_paused_filter_view(
                    result.command_idx,
                    previous_scroll_offset,
                    previous_current_line,
                );
            } else {
                self.reset_current_line_to_scroll_offset();
            }
        }
        true
    }

    fn filter_update_preserves_view(
        &self,
        command_idx: usize,
        pattern: &str,
        mode: SearchMode,
        context: usize,
    ) -> bool {
        !self.follow_output
            && matches!(
                &self.view_mode,
                ViewMode::Output {
                    command_idx: current_idx
                } if *current_idx == command_idx
            )
            && self
                .displayed_filter
                .as_ref()
                .filter(|filter| filter.command_idx == command_idx)
                .map(|filter| {
                    filter.pattern == pattern && filter.mode == mode && filter.context == context
                })
                .unwrap_or(false)
    }

    fn restore_paused_filter_view(
        &mut self,
        command_idx: usize,
        scroll_offset: usize,
        current_line: Option<usize>,
    ) {
        let total_rows = self.output_wrapped_row_count(command_idx);
        let (scroll_offset, current_line) = clamp_paused_view(
            total_rows,
            self.visible_output_rows(),
            scroll_offset,
            current_line,
        );
        self.scroll_offset = scroll_offset;
        self.current_line_command = Some(command_idx);
        self.current_line_idx = current_line;
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
        self.cancel_filter_update();
        self.active_filter = None;
        self.displayed_filter = None;
        self.filter_edit_original_lines = None;
        self.filter_match_cache = None;
        self.clear_filtered_lines();
        self.clear_current_match();
    }

    fn apply_filter(&mut self, pattern: &str, mode: SearchMode, context: usize) {
        if pattern.is_empty() {
            self.displayed_filter = None;
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
        let preserve_view = self.filter_update_preserves_view(command_idx, pattern, mode, context);
        let previous_scroll_offset = self.scroll_offset;
        let previous_current_line = self.current_line_idx;

        let regex = match self.cached_regex(pattern, mode) {
            Some(regex) => regex,
            None => {
                self.displayed_filter = Some(ActiveFilter {
                    command_idx,
                    pattern: pattern.to_string(),
                    mode,
                    context,
                });
                self.filter_match_cache = None;
                self.clear_filtered_lines();
                return;
            }
        };

        let Some(command) = self.commands.get(command_idx) else {
            return;
        };
        let cache_updated = self
            .filter_match_cache
            .as_mut()
            .map(|cache| {
                update_filter_match_cache(command, cache, command_idx, pattern, mode, &regex)
            })
            .unwrap_or(false);
        if !cache_updated {
            self.filter_match_cache = Some(FilterMatchCache {
                command_idx,
                pattern: pattern.to_string(),
                mode,
                output_generation: command.output_generation,
                first_line_sequence: command.first_line_sequence,
                line_matches: command
                    .output_lines
                    .iter()
                    .map(|line| regex.is_match(line))
                    .collect(),
            });
        }
        let filtered_lines = self
            .filter_match_cache
            .as_ref()
            .map(|cache| contextual_line_indices(cache.line_matches.iter().copied(), context))
            .unwrap_or_default();

        self.filtered_lines = filtered_lines;
        self.displayed_filter = Some(ActiveFilter {
            command_idx,
            pattern: pattern.to_string(),
            mode,
            context,
        });
        self.bump_filtered_generation();

        if self.should_follow_output(command_idx) {
            self.clear_current_match();
            self.scroll_to_bottom();
            return;
        }
        if preserve_view {
            self.restore_paused_filter_view(
                command_idx,
                previous_scroll_offset,
                previous_current_line,
            );
            return;
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
}

// ============================================================================
// View and Scrolling
// ============================================================================

impl CommandRunnerState {
    fn should_follow_output(&self, command_idx: usize) -> bool {
        self.follow_output
            && matches!(
                &self.view_mode,
                ViewMode::Output {
                    command_idx: current_idx
                } if *current_idx == command_idx
            )
    }

    fn resume_follow(&mut self, command_idx: usize) {
        if self.current_command_idx() != Some(command_idx) {
            return;
        }
        self.follow_output = true;
        self.clear_current_match();
        self.scroll_to_bottom();
    }

    fn toggle_follow(&mut self, command_idx: usize) {
        if self.follow_output {
            self.follow_output = false;
        } else {
            self.resume_follow(command_idx);
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
        self.follow_output = self.commands[command_idx].status.is_running();
        if self.follow_output {
            self.scroll_to_bottom();
        } else {
            self.reset_current_line_to_scroll_offset();
        }
    }

    fn scroll_down(&mut self, amount: usize) {
        self.follow_output = false;
        let max_offset = self
            .output_line_count()
            .saturating_sub(self.visible_output_rows());
        self.scroll_offset = (self.scroll_offset + amount).min(max_offset);
    }

    fn scroll_up(&mut self, amount: usize) {
        self.follow_output = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    fn scroll_to_bottom(&mut self) {
        let max_offset = self
            .output_line_count()
            .saturating_sub(self.visible_output_rows());
        self.scroll_offset = max_offset;
        self.reset_current_line_to_scroll_offset();
    }
}

// ============================================================================
// Rendering
// ============================================================================

impl CommandRunnerState {
    fn render(&mut self, term: &mut TermWizTerminal) -> anyhow::Result<()> {
        let size = term.get_screen_size()?;
        self.screen_rows = size.rows;
        self.screen_cols = size.cols;

        if let Some(command_idx) = self.current_command_idx() {
            let render_key = (command_idx, size.rows, size.cols);
            let clear_screen = self.last_output_render != Some(render_key);
            self.last_output_render = Some(render_key);
            self.render_output_view(term, clear_screen)
        } else if matches!(&self.view_mode, ViewMode::List) {
            self.last_output_render = None;
            self.render_list_view(term)
        } else {
            self.last_output_render = None;
            self.render_confirm_quit(term)
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

    fn render_output_view(
        &mut self,
        term: &mut TermWizTerminal,
        clear_screen: bool,
    ) -> anyhow::Result<()> {
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
        let mut changes = Vec::new();
        if clear_screen {
            changes.push(Change::ClearScreen(ColorAttribute::Default));
        }
        changes.push(Change::CursorVisibility(cursor_visibility));

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(0),
        });
        let command_running = status.is_running();
        let status = match status {
            CommandStatus::Pending => "Pending",
            CommandStatus::Running => "Running",
            CommandStatus::Success(_) => "Success",
            CommandStatus::Failed(_) => "Failed",
            CommandStatus::Killed => "Killed",
        };
        let label_fg = self.colors.output_label_fg;
        let context_label_fg = self.colors.output_context_label_fg;
        let context_value_fg = self.colors.output_context_value_fg;
        let separator_fg = self.colors.separator_fg;
        let margin_fg = self.colors.margin_fg;
        let line_number_fg = self.colors.line_number_fg;

        // Header line: Command: <title> │ Status: <status> [│ Exit Code: <code>]
        {
            let mut writer = SegmentWriter::new(&mut changes, self.screen_cols);
            writer.push_bold_label("Command", Some(label_fg));
            writer.push(":", None);
            writer.push(" ", None);
            writer.push(&cmd_title, None);
            writer.push(" │ ", Some(separator_fg));
            writer.push_bold_label("Status", Some(label_fg));
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
            if command_running {
                let follow_color: ColorAttribute = if self.follow_output {
                    AnsiColor::Green.into()
                } else {
                    AnsiColor::Yellow.into()
                };
                writer.push(" │ ", Some(separator_fg));
                writer.push_bold_label("Follow", Some(label_fg));
                writer.push(":", None);
                writer.push(" ", None);
                writer.push(
                    if self.follow_output { "live" } else { "paused" },
                    Some(follow_color),
                );
            }
            writer.fill_remaining();
        }

        let (search_pattern, mode, context) = self.get_filter_params(cmd_idx);
        let filter_update_pending = self.filter_update_pending_for(cmd_idx);
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
        let gutter_width = if number_width > 0 {
            number_width + 1
        } else {
            0
        };
        let content_width = self.screen_cols.saturating_sub(gutter_width).max(1);

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(1),
        });
        // Search line: Search: <pattern>
        {
            let mut writer = SegmentWriter::new(&mut changes, self.screen_cols);
            writer.push_bold_label("Search", Some(label_fg));
            writer.push(":", None);
            writer.push(" ", None);
            writer.push(&search_pattern, None);
            if filter_update_pending {
                writer.push("  Filtering…", Some(context_value_fg));
            }
            writer.fill_remaining();
        }
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(2),
        });
        // Context line: Context: ±<n> │ Mode: <mode>
        {
            let mut writer = SegmentWriter::new(&mut changes, self.screen_cols);
            writer.push("Context", Some(context_label_fg));
            writer.push(":", Some(context_label_fg));
            writer.push(" ", Some(context_label_fg));
            writer.push(&format!("±{}", context), Some(context_value_fg));
            writer.push(" │ ", Some(separator_fg));
            writer.push("Mode", Some(context_label_fg));
            writer.push(":", Some(context_label_fg));
            writer.push(" ", Some(context_label_fg));
            writer.push(mode.display(), Some(context_value_fg));
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
        let filtered_output = self.filter_active_for(cmd_idx);
        let (rows, row_base, line_count, first_logical_line_number) = if filtered_output {
            let line_count = self.filtered_wrapped_row_count(cmd_idx, regex.as_ref());
            let first_logical_line_number =
                self.filtered_line_number_at_row(cmd_idx, regex.as_ref(), self.scroll_offset);
            let rows = self.filtered_visible_rows(
                cmd_idx,
                regex.as_ref(),
                self.scroll_offset,
                visible_rows,
            );
            (
                rows,
                self.scroll_offset,
                line_count,
                first_logical_line_number,
            )
        } else {
            let line_count = self.unfiltered_wrapped_row_count(cmd_idx);
            let first_logical_line_number =
                self.unfiltered_line_number_at_row(cmd_idx, self.scroll_offset);
            let rows = VecDeque::from(self.unfiltered_visible_rows(
                cmd_idx,
                self.scroll_offset,
                visible_rows,
            ));
            (
                rows,
                self.scroll_offset,
                line_count,
                first_logical_line_number,
            )
        };
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
            let row_idx = current_row.unwrap_or(0);
            if filtered_output {
                self.filtered_line_number_at_row(cmd_idx, regex.as_ref(), row_idx)
            } else {
                self.unfiltered_line_number_at_row(cmd_idx, row_idx)
            }
        };
        let current_match_id = if filtered_output && self.current_match_command == Some(cmd_idx) {
            self.current_match_idx
        } else {
            None
        };

        let mut logical_line_number = first_logical_line_number;
        for row in 0..visible_rows {
            let line_idx = self.scroll_offset + row;
            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(start_row + row),
            });
            changes.extend([
                Change::AllAttributes(Default::default()),
                Change::ClearToEndOfLine(ColorAttribute::Default),
            ]);

            if line_idx < line_count {
                if let Some(segment) = rows.get(line_idx.saturating_sub(row_base)) {
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
                    if let Some(line_number) = segment.line_number {
                        logical_line_number = Some(line_number);
                    }
                    let is_current_line =
                        active_line_number.is_some() && logical_line_number == active_line_number;
                    let (line_fg, line_bg) = if is_current_line {
                        (self.colors.current_line_fg, self.colors.current_line_bg)
                    } else {
                        (None, None)
                    };
                    let segment_width = str_column_width(&segment.text).min(content_width);
                    push_text_with_highlights(
                        &mut changes,
                        segment,
                        &self.colors,
                        current_match_id,
                        line_fg,
                        line_bg,
                    );
                    if is_current_line {
                        if let Some(bg) = line_bg {
                            let remaining = content_width.saturating_sub(segment_width);
                            if remaining > 0 {
                                changes.push(AttributeChange::Background(bg).into());
                                if let Some(fg) = line_fg {
                                    changes.push(AttributeChange::Foreground(fg).into());
                                }
                                changes.push(Change::Text(" ".repeat(remaining)));
                                changes.push(Change::AllAttributes(Default::default()));
                            }
                        }
                    }
                }
            }
        }

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(self.screen_rows - 1),
        });
        {
            let mut writer = SegmentWriter::new(&mut changes, self.screen_cols);
            if command_running && !filter_active {
                let help = if self.follow_output {
                    "[Space] pause follow"
                } else {
                    "[Space] resume follow  [G/End] jump to latest"
                };
                writer.push(help, Some(context_value_fg));
            }
            writer.fill_remaining();
        }

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
}

// ============================================================================
// Input Handling
// ============================================================================

impl CommandRunnerState {
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
                key: KeyCode::Char('L'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                self.list_selection_input.clear();
                if self.list_selection < self.commands.len() {
                    self.clear_command_output(self.list_selection);
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
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(' '),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.toggle_follow(command_idx);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::End,
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                self.resume_follow(command_idx);
            }
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
                if self.count_buffer.is_empty() {
                    self.reset_count();
                    self.resume_follow(command_idx);
                } else {
                    let count = self.take_count();
                    let total_rows = self.output_wrapped_row_count(command_idx);
                    if total_rows == 0 {
                        return ControlFlow::Continue;
                    }
                    if let Some(row_idx) = self.output_row_for_line_number(command_idx, count) {
                        self.clear_current_match_if_present(command_idx);
                        self.set_current_line(command_idx, row_idx, total_rows);
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
                    self.request_filter_update(command_idx, pattern, mode, context, true);
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('/'),
                modifiers: Modifiers::NONE,
            }) => {
                self.reset_count();
                let (pattern, mode, context) = self.get_filter_params(command_idx);
                self.begin_filter_edit(command_idx);
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
                    if self.follow_output {
                        self.resume_follow(command_idx);
                    } else {
                        self.scroll_offset = 0;
                        self.reset_current_line_to_scroll_offset();
                    }
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
                self.resume_follow(command_idx);
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
                key: KeyCode::Char('L'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.reset_count();
                self.clear_command_output(command_idx);
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
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern: pattern.clone(),
                    mode,
                    context,
                };
                self.request_filter_update(command_idx, pattern, mode, context, false);
            }
            // Tab increases context, Shift-Tab decreases
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE,
            }) => {
                context += 1;
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern: pattern.clone(),
                    mode,
                    context,
                };
                self.request_filter_update(command_idx, pattern, mode, context, false);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::SHIFT,
            }) => {
                if context > 0 {
                    context -= 1;
                    self.view_mode = ViewMode::Filter {
                        command_idx,
                        pattern: pattern.clone(),
                        mode,
                        context,
                    };
                    self.request_filter_update(command_idx, pattern, mode, context, false);
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                ..
            }) => {
                pattern.pop();
                self.reset_current_match(command_idx);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern: pattern.clone(),
                    mode,
                    context,
                };
                self.request_filter_update(command_idx, pattern, mode, context, false);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('U'),
                modifiers: Modifiers::CTRL,
            }) => {
                pattern.clear();
                self.reset_current_match(command_idx);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern: pattern.clone(),
                    mode,
                    context,
                };
                self.request_filter_update(command_idx, pattern, mode, context, false);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('L'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.clear_command_output(command_idx);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern: pattern.clone(),
                    mode,
                    context,
                };
                self.request_filter_update(command_idx, pattern, mode, context, true);
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) => {
                self.cancel_filter_update();
                self.filtered_lines = self.filter_edit_original_lines.take().unwrap_or_default();
                let committed_filter = self
                    .active_filter
                    .as_ref()
                    .filter(|af| af.command_idx == command_idx)
                    .cloned();
                self.displayed_filter = committed_filter.clone();
                self.bump_filtered_generation();
                self.scroll_offset = 0;
                self.clear_current_match();
                self.view_mode = ViewMode::Output { command_idx };
                if let Some(filter) = committed_filter {
                    self.request_filter_update(
                        command_idx,
                        filter.pattern,
                        filter.mode,
                        filter.context,
                        true,
                    );
                } else {
                    self.clear_filtered_lines();
                    self.reset_current_line_to_scroll_offset();
                }
            }
            // Enter accepts the filter and goes back to output view (keeping filtered results)
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }) => {
                self.filter_edit_original_lines = None;
                if pattern.is_empty() {
                    self.clear_active_filter();
                    self.view_mode = ViewMode::Output { command_idx };
                    self.reset_current_line_to_scroll_offset();
                } else {
                    self.set_active_filter(command_idx, pattern.clone(), mode, context);
                    self.view_mode = ViewMode::Output { command_idx };
                    self.request_filter_update(command_idx, pattern, mode, context, true);
                }
            }
            // Character input for pattern
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers: Modifiers::NONE | Modifiers::SHIFT,
            }) => {
                pattern.push(c);
                self.reset_current_match(command_idx);
                self.view_mode = ViewMode::Filter {
                    command_idx,
                    pattern: pattern.clone(),
                    mode,
                    context,
                };
                self.request_filter_update(command_idx, pattern, mode, context, false);
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
    Output {
        idx: usize,
        run_generation: u64,
        data: Vec<u8>,
    },
}

#[derive(Default)]
struct OutputBatchBuffer {
    data: Vec<u8>,
    has_data: bool,
}

struct OutputBatch {
    per_command: Vec<OutputBatchBuffer>,
    message_count: usize,
    byte_count: usize,
}

impl OutputBatch {
    fn new(command_count: usize) -> Self {
        Self {
            per_command: (0..command_count)
                .map(|_| OutputBatchBuffer::default())
                .collect(),
            message_count: 0,
            byte_count: 0,
        }
    }

    fn reset(&mut self) {
        for buffer in &mut self.per_command {
            buffer.data.clear();
            buffer.has_data = false;
        }
        self.message_count = 0;
        self.byte_count = 0;
    }

    fn push(&mut self, idx: usize, data: Vec<u8>) {
        self.message_count = self.message_count.saturating_add(1);
        self.byte_count = self.byte_count.saturating_add(data.len());

        let Some(buffer) = self.per_command.get_mut(idx) else {
            return;
        };
        if !buffer.has_data && data.len() > buffer.data.capacity() {
            buffer.data = data;
        } else {
            buffer.data.extend_from_slice(&data);
        }
        buffer.has_data = true;
    }

    fn reached_size_limit(&self) -> bool {
        self.message_count >= MAX_OUTPUT_MESSAGES_PER_TICK
            || self.byte_count >= MAX_OUTPUT_BYTES_PER_TICK
    }
}

/// Ensures that closing the applet also terminates commands that are still
/// running, including when terminal I/O returns an error.
struct ChildProcesses(Vec<Option<Child>>);

impl Drop for ChildProcesses {
    fn drop(&mut self) {
        for child in self.0.iter_mut().flatten() {
            let _ = child.kill();
        }
    }
}

/// Spawn a command and return channels for output
async fn spawn_command(
    idx: usize,
    run_generation: u64,
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
                        if tx
                            .send(OutputMessage::Output {
                                idx,
                                run_generation,
                                data: buf[..n].to_vec(),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
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
                        if tx
                            .send(OutputMessage::Output {
                                idx,
                                run_generation,
                                data: buf[..n].to_vec(),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .detach();
    }

    Ok(child)
}

/// Main entry point for the command runner applet
pub fn run_command_runner(
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
    let (filter_result_tx, filter_result_rx): (Sender<FilterJobResult>, Receiver<FilterJobResult>) =
        smol::channel::bounded(16);

    // Store child processes
    let mut children = ChildProcesses((0..state.commands.len()).map(|_| None).collect());

    // Spawn all commands initially
    for (idx, cmd) in state.commands.iter_mut().enumerate() {
        let config = cmd.config.clone();
        let tx = output_tx.clone();
        let run_generation = cmd.run_generation;
        cmd.status = CommandStatus::Running;
        cmd.start_time = Some(Instant::now());

        // Spawn in a blocking context since we're in a sync function
        let child_result = smol::block_on(spawn_command(idx, run_generation, &config, tx));
        match child_result {
            Ok(child) => {
                children.0[idx] = Some(child);
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
    let mut force_render = true;
    let mut last_render = Instant::now();
    let mut last_elapsed_refresh = Instant::now();
    let mut last_filter_refresh = Instant::now();
    let mut pending_streaming_refresh = None;
    let mut force_streaming_refresh = false;
    let mut output_batch = OutputBatch::new(state.commands.len());
    loop {
        while let Ok(result) = filter_result_rx.try_recv() {
            if state.apply_filter_job_result(result) {
                dirty = true;
            }
        }

        if let Some(work) = state.take_due_filter_work() {
            let result_tx = filter_result_tx.clone();
            smol::spawn(async move {
                let result = smol::unblock(move || compute_filter_job(work)).await;
                let _ = result_tx.send(result).await;
            })
            .detach();
            dirty = true;
        }

        // Drain only a bounded amount of output so that a command producing an
        // endless stream cannot starve input, process control, or rendering.
        // Chunks are coalesced per command and applied once below.
        let drain_started = Instant::now();
        output_batch.reset();
        let mut drain_budget_exhausted = false;
        loop {
            if output_batch.reached_size_limit()
                || drain_started.elapsed() >= OUTPUT_DRAIN_TIME_BUDGET
            {
                drain_budget_exhausted = true;
                break;
            }

            match output_rx.try_recv() {
                Ok(OutputMessage::Output {
                    idx,
                    run_generation,
                    data,
                }) => {
                    let accepts_output = state
                        .commands
                        .get(idx)
                        .map(|cmd| cmd.accepts_output(run_generation))
                        .unwrap_or(false);
                    if accepts_output {
                        output_batch.push(idx, data);
                    }
                }
                Err(_) => break,
            }
        }

        for (idx, buffer) in output_batch.per_command.iter().enumerate() {
            if !buffer.has_data {
                continue;
            }
            let is_current_command = state.current_command_idx() == Some(idx);
            let filter_active = is_current_command && state.filter_refresh_active_for(idx);
            if let Some(cmd) = state.commands.get_mut(idx) {
                cmd.append_output(&buffer.data);
                if is_current_command {
                    pending_streaming_refresh = Some(idx);
                    if !filter_active {
                        dirty = true;
                    }
                }
            }
        }

        // Check for finished processes
        for (idx, child_opt) in children.0.iter_mut().enumerate() {
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
                    if let Some(Some(child)) = children.0.get_mut(idx) {
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
                    if let Some(Some(child)) = children.0.get_mut(idx) {
                        let _ = child.kill();
                    }

                    let is_current_command = state.current_command_idx() == Some(idx);
                    if let Some(cmd) = state.commands.get_mut(idx) {
                        cmd.begin_new_run();
                        dirty = true;
                        if is_current_command {
                            pending_streaming_refresh = Some(idx);
                            force_streaming_refresh = true;
                        }

                        let config = cmd.config.clone();
                        let run_generation = cmd.run_generation;
                        let tx = output_tx.clone();
                        let child_result =
                            smol::block_on(spawn_command(idx, run_generation, &config, tx));
                        match child_result {
                            Ok(child) => {
                                children.0[idx] = Some(child);
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

        // Elapsed time is the only visible state that changes merely because a
        // command is running, and it has one-second precision.
        if state.any_running() && last_elapsed_refresh.elapsed() >= ELAPSED_REFRESH_INTERVAL {
            dirty = true;
            last_elapsed_refresh = Instant::now();
        }

        if let Some(command_idx) = pending_streaming_refresh {
            if force_streaming_refresh
                || !state.filter_refresh_active_for(command_idx)
                || last_filter_refresh.elapsed() >= FILTER_REFRESH_INTERVAL
            {
                dirty = true;
            }
        }

        // Refresh wrapping/filter state and render no more than once per frame.
        // Output continues to be ingested between frames.
        let render_due = force_render || (dirty && last_render.elapsed() >= RENDER_INTERVAL);
        if render_due {
            if let Some(command_idx) = pending_streaming_refresh {
                let filter_active = state.filter_refresh_active_for(command_idx);
                if force_streaming_refresh
                    || !filter_active
                    || last_filter_refresh.elapsed() >= FILTER_REFRESH_INTERVAL
                {
                    pending_streaming_refresh = None;
                    force_streaming_refresh = false;
                    state.refresh_streaming_view(command_idx);
                    if filter_active {
                        last_filter_refresh = Instant::now();
                    }
                }
            }
            state.render(&mut term)?;
            dirty = false;
            force_render = false;
            last_render = Instant::now();
        }

        let poll_timeout = if drain_budget_exhausted {
            Duration::ZERO
        } else if dirty {
            let until_render = RENDER_INTERVAL
                .checked_sub(last_render.elapsed())
                .unwrap_or(Duration::ZERO);
            MAX_INPUT_POLL_INTERVAL.min(until_render)
        } else {
            MAX_INPUT_POLL_INTERVAL
        };

        // Poll input even when output is backlogged. A zero timeout services
        // pending input without delaying the next bounded output batch.
        match term.poll_input(Some(poll_timeout))? {
            Some(event) => {
                if let Some(command_idx) = pending_streaming_refresh.take() {
                    let filter_active = state.filter_refresh_active_for(command_idx);
                    force_streaming_refresh = false;
                    state.refresh_streaming_view(command_idx);
                    if filter_active {
                        last_filter_refresh = Instant::now();
                    }
                }

                match state.handle_input(event, &process_tx) {
                    ControlFlow::Continue => {
                        dirty = true;
                    }
                    ControlFlow::Exit => break,
                }
            }
            None => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn command_state() -> CommandState {
        CommandState::new(CommandRunnerCommand {
            title: None,
            args: vec!["test".to_string()],
            cwd: None,
            set_environment_variables: Default::default(),
        })
    }

    #[test]
    fn output_batch_coalesces_chunks_per_command_in_arrival_order() {
        let mut batch = OutputBatch::new(2);
        batch.push(0, b"first".to_vec());
        batch.push(1, b"other".to_vec());
        batch.push(0, b"-second".to_vec());

        assert_eq!(batch.message_count, 3);
        assert_eq!(batch.byte_count, 17);
        assert!(batch.per_command[0].has_data);
        assert!(batch.per_command[1].has_data);
        assert_eq!(batch.per_command[0].data.as_slice(), b"first-second");
        assert_eq!(batch.per_command[1].data.as_slice(), b"other");
    }

    #[test]
    fn output_batch_enforces_message_and_byte_limits() {
        let mut messages = OutputBatch::new(1);
        for _ in 0..MAX_OUTPUT_MESSAGES_PER_TICK {
            messages.push(0, Vec::new());
        }
        assert!(messages.reached_size_limit());

        let mut bytes = OutputBatch::new(1);
        bytes.push(0, vec![0; MAX_OUTPUT_BYTES_PER_TICK]);
        assert!(bytes.reached_size_limit());
    }

    #[test]
    fn output_batch_reset_retains_buffer_capacity() {
        let mut batch = OutputBatch::new(1);
        batch.push(0, vec![0; 1024]);
        let buffer_ptr = batch.per_command[0].data.as_ptr();
        let buffer_capacity = batch.per_command[0].data.capacity();

        batch.reset();
        assert_eq!(batch.message_count, 0);
        assert_eq!(batch.byte_count, 0);
        assert!(!batch.per_command[0].has_data);
        assert!(batch.per_command[0].data.is_empty());
        assert_eq!(batch.per_command[0].data.capacity(), buffer_capacity);

        batch.push(0, b"next".to_vec());
        assert_eq!(batch.per_command[0].data.as_ptr(), buffer_ptr);
        assert_eq!(batch.per_command[0].data.as_slice(), b"next");
    }

    #[test]
    fn command_line_sequences_survive_clear_and_front_trimming() {
        let mut command = command_state();
        command.append_output(b"one\ntwo");
        assert_eq!(command.first_line_sequence, 0);
        assert_eq!(command.next_line_sequence, 2);

        command.clear_output();
        assert_eq!(command.first_line_sequence, 2);
        command.append_output(b"three");
        assert_eq!(command.first_line_sequence, 2);
        assert_eq!(command.next_line_sequence, 3);

        let oversized = format!("{}\ntail", "x".repeat(MAX_OUTPUT_SIZE));
        command.clear_output();
        command.append_output(oversized.as_bytes());
        assert_eq!(
            command.output_lines.front().map(String::as_str),
            Some("tail")
        );
        assert_eq!(command.first_line_sequence, 4);
        assert_eq!(command.next_line_sequence, 5);
    }

    #[test]
    fn filtering_keeps_only_indexes_for_matches_and_context() {
        let lines = VecDeque::from([
            "before".to_string(),
            "first match".to_string(),
            "between".to_string(),
            "second match".to_string(),
            "after".to_string(),
            "excluded".to_string(),
        ]);
        let regex = Regex::new("match").unwrap();
        let line_matches: Vec<bool> = lines.iter().map(|line| regex.is_match(line)).collect();

        assert_eq!(
            contextual_line_indices(line_matches.iter().copied(), 1),
            vec![0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn large_filter_job_computes_matches_from_its_snapshot() {
        let result = compute_filter_job(FilterWork {
            request_generation: 7,
            command_idx: 2,
            command_run_generation: 3,
            output_generation: 11,
            first_line_sequence: 5,
            pattern: "error".to_string(),
            mode: SearchMode::CaseInsensitive,
            context: 1,
            lines: vec![
                "ok".to_string(),
                "ERROR: failed".to_string(),
                "done".to_string(),
            ],
        });

        assert_eq!(result.request_generation, 7);
        assert_eq!(result.command_idx, 2);
        assert_eq!(result.line_matches, vec![false, true, false]);
        assert!(result.regex.is_some());
    }

    #[test]
    fn filter_requests_wait_for_debounce_unless_marked_immediate() {
        let requested_at = Instant::now();
        let request = PendingFilterRequest {
            request_generation: 1,
            command_idx: 0,
            pattern: "needle".to_string(),
            mode: SearchMode::CaseSensitive,
            context: 0,
            requested_at,
            immediate: false,
        };

        assert!(!request.is_due(requested_at));
        assert!(request.is_due(requested_at + FILTER_INPUT_DEBOUNCE));

        let immediate_request = PendingFilterRequest {
            immediate: true,
            ..request
        };
        assert!(immediate_request.is_due(requested_at));
    }

    #[test]
    fn rerun_rejects_output_from_an_older_generation() {
        let mut command = command_state();
        let first_generation = command.run_generation;
        assert!(command.accepts_output(first_generation));

        command.append_output(b"old output");
        command.begin_new_run();

        assert!(!command.accepts_output(first_generation));
        assert!(command.accepts_output(command.run_generation));
        assert!(command.output_lines.is_empty());
    }

    #[test]
    fn fragmented_utf8_is_decoded_without_reprocessing_the_line() {
        let mut command = command_state();
        command.append_output(&[0xe2, 0x82]);
        assert_eq!(command.output_lines.back().map(String::as_str), Some(""));
        assert_eq!(command.pending_utf8, vec![0xe2, 0x82]);

        command.append_output(&[0xac, b'\n', b'x']);
        assert_eq!(
            command
                .output_lines
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["€", "x"]
        );
        assert_eq!(
            command
                .line_byte_lengths
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![3, 1]
        );
        assert!(command.pending_utf8.is_empty());
    }

    #[test]
    fn incremental_decoder_preserves_crlf_and_lossy_utf8_behavior() {
        let mut command = command_state();
        command.append_output(b"first\r\n");
        command.append_output(&[0xe2, b'\n']);

        assert_eq!(
            command
                .output_lines
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["first", "�", ""]
        );
    }

    #[test]
    fn incremental_filter_cache_rechecks_only_the_mutable_tail() {
        let regex = Regex::new("match").unwrap();
        let mut command = command_state();
        command.append_output(b"no hit yet\nstable");
        let mut cache = FilterMatchCache {
            command_idx: 0,
            pattern: "match".to_string(),
            mode: SearchMode::CaseSensitive,
            output_generation: command.output_generation,
            first_line_sequence: command.first_line_sequence,
            line_matches: command
                .output_lines
                .iter()
                .map(|line| regex.is_match(line))
                .collect(),
        };

        command.append_output(b" match\nnew line");
        assert!(update_filter_match_cache(
            &command,
            &mut cache,
            0,
            "match",
            SearchMode::CaseSensitive,
            &regex,
        ));
        assert_eq!(cache.line_matches, VecDeque::from([false, true, false]));
    }

    #[test]
    fn incremental_filter_cache_discards_trimmed_prefix() {
        let regex = Regex::new("match").unwrap();
        let mut command = command_state();
        command.append_output(b"old\nmatch\ntail");
        let mut cache = FilterMatchCache {
            command_idx: 0,
            pattern: "match".to_string(),
            mode: SearchMode::CaseSensitive,
            output_generation: command.output_generation,
            first_line_sequence: command.first_line_sequence,
            line_matches: command
                .output_lines
                .iter()
                .map(|line| regex.is_match(line))
                .collect(),
        };

        command.output_lines.pop_front();
        command.line_byte_lengths.pop_front();
        command.first_line_sequence += 1;
        command.output_generation += 1;

        assert!(update_filter_match_cache(
            &command,
            &mut cache,
            0,
            "match",
            SearchMode::CaseSensitive,
            &regex,
        ));
        assert_eq!(cache.line_matches, VecDeque::from([true, false]));
    }

    #[test]
    fn incremental_row_counts_reuse_completed_lines() {
        let mut command = command_state();
        command.append_output(b"one\ntwo");
        let mut cache = WrappedRowCountCache {
            command_idx: 0,
            max_width: 3,
            output_generation: command.output_generation,
            first_line_sequence: command.first_line_sequence,
            line_row_counts: VecDeque::from([1, 1]),
            total_rows: 2,
        };

        command.append_output(b"-continued\nthree");
        assert!(update_wrapped_row_count_cache(&command, &mut cache, 0, 3,));
        assert_eq!(cache.line_row_counts, VecDeque::from([1, 5, 2]));
        assert_eq!(cache.total_rows, 8);
        assert_eq!(line_index_at_wrapped_row(&cache, 0), Some((0, 0)));
        assert_eq!(line_index_at_wrapped_row(&cache, 6), Some((2, 6)));
    }

    #[test]
    fn filtered_layout_tracks_wrapped_rows_and_match_offsets() {
        let mut command = command_state();
        command.append_output(b"match\nctx\nmatch match");
        let regex = Regex::new("match").unwrap();

        let cache = build_filtered_layout(&command, 0, &[0, 1, 2], 7, 4, Some(&regex));

        assert_eq!(cache.line_indices, vec![0, 1, 2]);
        assert_eq!(cache.line_start_rows, vec![0, 2, 3]);
        assert_eq!(cache.line_row_counts, vec![2, 1, 3]);
        assert_eq!(cache.line_match_starts, vec![0, 1, 1]);
        assert_eq!(
            cache
                .match_locations
                .iter()
                .map(|location| (location.row_idx, location.line_number))
                .collect::<Vec<_>>(),
            vec![(0, 1), (3, 3), (4, 3)]
        );
        assert_eq!(cache.total_rows, 6);
        assert_eq!(filtered_line_position_at_row(&cache, 0), Some(0));
        assert_eq!(filtered_line_position_at_row(&cache, 1), Some(0));
        assert_eq!(filtered_line_position_at_row(&cache, 2), Some(1));
        assert_eq!(filtered_line_position_at_row(&cache, 5), Some(2));
        assert_eq!(filtered_line_position_at_row(&cache, 6), None);

        let rows = wrap_filtered_visible_rows(&command, &cache, Some(&regex), 1, 3);
        assert_eq!(
            rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
            vec!["h", "ctx", "matc"]
        );
        assert_eq!(
            rows.iter().map(|row| row.line_number).collect::<Vec<_>>(),
            vec![None, Some(2), Some(3)]
        );
        assert_eq!(rows[0].highlights[0].match_id, 0);
        assert_eq!(rows[2].highlights[0].match_id, 1);
    }

    #[test]
    fn filtered_layout_does_not_assign_ids_to_zero_width_matches() {
        let mut command = command_state();
        command.append_output(b"first\nsecond");
        let regex = Regex::new("^").unwrap();

        let cache = build_filtered_layout(&command, 0, &[0, 1], 1, 80, Some(&regex));

        assert_eq!(cache.line_match_starts, vec![0, 0]);
        assert!(cache.match_locations.is_empty());
    }

    #[test]
    fn match_locations_follow_wide_character_wrapping() {
        let mut command = command_state();
        command.append_output("a界b".as_bytes());
        let regex = Regex::new("界").unwrap();

        let cache = build_filtered_layout(&command, 0, &[0], 1, 2, Some(&regex));

        assert_eq!(cache.match_locations.len(), 1);
        assert_eq!(cache.match_locations[0].row_idx, 1);
    }

    #[test]
    fn filtered_layout_measurement_matches_wrapped_row_count() {
        for (line, max_width) in [("", 4), ("abcdef", 2), ("a界b", 2), ("e\u{301}x", 1)] {
            let measured = measure_line_and_append_match_locations(
                line,
                max_width,
                &[],
                0,
                1,
                &mut Vec::new(),
            );

            assert_eq!(measured, wrapped_row_count(line, max_width));
        }
    }

    #[test]
    fn paused_view_is_preserved_and_clamped_as_output_changes() {
        assert_eq!(clamp_paused_view(100, 20, 50, Some(55)), (50, Some(55)));
        assert_eq!(clamp_paused_view(40, 20, 50, Some(55)), (20, Some(39)));
        assert_eq!(clamp_paused_view(0, 20, 50, Some(55)), (0, None));
    }
}
