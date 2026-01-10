//! Scrollback Search with Context overlay
//!
//! This overlay provides a grep-like search interface for the terminal scrollback,
//! showing search results with surrounding context lines.

use crate::termwindow::TermWindowNotif;
use config::configuration;
use config::keyassignment::{ClipboardCopyDestination, KeyAssignment};
use mux::pane::{Pane, PaneId, Pattern, SearchResult};
use mux::termwiztermtab::TermWizTerminal;
use mux::Mux;
use regex::Regex;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};
use termwiz::cell::{AttributeChange, CellAttributes, Intensity};
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::lineedit::LineEditBuffer;
use termwiz::surface::{Change, Position};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use wezterm_term::{unicode_column_width, StableRowIndex};
use window::WindowOps;

const DEFAULT_CONTEXT_LINES: usize = 2;
const SEARCH_DEBOUNCE_MS: u64 = 350;

// Layout constants
/// Width of the "Search: " prompt
const SEARCH_PROMPT_WIDTH: usize = 8;
/// Number of header rows (search line, options line, separator)
const HEADER_ROWS: usize = 3;
/// Left margin for compact view: border(1) + arrow(1) + line_num(5) + separator(3)
const COMPACT_LEFT_MARGIN: usize = 10;
/// Left margin for card view: border(1) + arrow(1) + line_num(5) + separator(3) + border_margin(2)
const CARD_LEFT_MARGIN: usize = 12;
/// Padding for continuation lines: arrow(1) + line_num(5) + separator(3)
const CONTINUATION_PADDING: usize = 9;
/// Card header and footer height combined
const CARD_OVERHEAD: usize = 2;
/// Box borders height in compact view (top + bottom)
const BOX_BORDERS: usize = 2;

#[derive(Debug, Clone, Copy)]
struct ScrollbackSearchColors {
    match_fg: ColorAttribute,
    match_bg: ColorAttribute,
    line_number_fg: ColorAttribute,
    header_fg: ColorAttribute,
    compact_border_fg: ColorAttribute,
    card_border_fg: ColorAttribute,
    card_selected_border_fg: ColorAttribute,
    arrow_fg: ColorAttribute,
    error_fg: ColorAttribute,
}

impl ScrollbackSearchColors {
    fn new() -> Self {
        let config = configuration();
        let colors = &config.resolved_palette;

        Self {
            match_fg: colors
                .scrollback_search_match_fg
                .unwrap_or(AnsiColor::Black.into())
                .into(),
            match_bg: colors
                .scrollback_search_match_bg
                .unwrap_or(AnsiColor::Yellow.into())
                .into(),
            line_number_fg: colors
                .scrollback_search_line_number_fg
                .unwrap_or(AnsiColor::Grey.into())
                .into(),
            header_fg: colors
                .scrollback_search_header_fg
                .unwrap_or(AnsiColor::Grey.into())
                .into(),
            compact_border_fg: colors
                .scrollback_search_compact_border_fg
                .unwrap_or(AnsiColor::Teal.into())
                .into(),
            card_border_fg: colors
                .scrollback_search_card_border_fg
                .unwrap_or(AnsiColor::Grey.into())
                .into(),
            card_selected_border_fg: colors
                .scrollback_search_card_selected_border_fg
                .unwrap_or(AnsiColor::Blue.into())
                .into(),
            arrow_fg: colors
                .scrollback_search_arrow_fg
                .unwrap_or(AnsiColor::Green.into())
                .into(),
            error_fg: colors
                .scrollback_search_error_fg
                .unwrap_or(AnsiColor::Red.into())
                .into(),
        }
    }
}

#[derive(Debug, Clone)]
struct ContextLine {
    line_number: StableRowIndex,
    content: String,
    col_width: usize,
}

#[derive(Debug, Clone)]
struct SearchMatchWithContext {
    line: StableRowIndex,
    match_range: Range<usize>,
    line_content: String,
    content_col_width: usize,
    context_before: Vec<ContextLine>,
    context_after: Vec<ContextLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum SearchMode {
    #[default]
    CaseSensitive,
    CaseInsensitive,
    Regex,
}

impl SearchMode {
    fn cycle(self) -> Self {
        match self {
            SearchMode::CaseSensitive => SearchMode::CaseInsensitive,
            SearchMode::CaseInsensitive => SearchMode::Regex,
            SearchMode::Regex => SearchMode::CaseSensitive,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SearchMode::CaseSensitive => "case-sensitive",
            SearchMode::CaseInsensitive => "case-insensitive",
            SearchMode::Regex => "regex",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SearchOptions {
    mode: SearchMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ViewMode {
    Compact,
    List,
}

struct ScrollbackSearchState {
    pane_id: PaneId,
    window: ::window::Window,
    search_input: LineEditBuffer,
    editing_search: bool,
    options: SearchOptions,
    matches: Vec<SearchMatchWithContext>,
    selected_match: usize,
    scroll_offset: usize,
    view_mode: ViewMode,
    context_lines: usize,
    width: usize,
    height: usize,
    search_history: Vec<String>,
    history_index: Option<usize>,
    pending_search: bool,
    last_search_trigger: Option<Instant>,
    count_buffer: String,
    compiled_regex: Option<Regex>,
    colors: ScrollbackSearchColors,
}

/// Get the column width of a single character
fn char_column_width(c: char) -> usize {
    let mut buf = [0u8; 4];
    let s = c.encode_utf8(&mut buf);
    unicode_column_width(s, None)
}

/// Calculate the column width of a string
fn str_column_width(s: &str) -> usize {
    s.chars().map(char_column_width).sum()
}

/// Calculate column widths at specific byte positions in a single pass.
/// Returns (width_at_start, width_at_end, total_width).
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

/// Calculate byte indices for start and end cell positions in a single pass.
/// Returns (start_byte_idx, end_byte_idx).
fn get_segment_split_indices(s: &str, start_cell: usize, end_cell: usize) -> (usize, usize) {
    let mut current_byte = 0;
    let mut current_cell = 0;
    let mut start_byte = None;
    let mut end_byte = None;

    if start_cell == 0 {
        start_byte = Some(0);
    }

    for ch in s.chars() {
        if start_byte.is_none() && current_cell >= start_cell {
            start_byte = Some(current_byte);
        }
        if end_byte.is_none() && current_cell >= end_cell {
            end_byte = Some(current_byte);
            break;
        }

        current_byte += ch.len_utf8();
        current_cell += char_column_width(ch);
    }

    let start = start_byte.unwrap_or(current_byte);
    let end = end_byte.unwrap_or(current_byte);
    (start, std::cmp::max(start, end))
}

/// A segment of a wrapped line with optional highlight information
#[derive(Debug, Clone)]
struct WrappedSegment<'a> {
    /// The text content for this row
    text: &'a str,
    /// If there's a highlight in this segment: (start_cell, end_cell) relative to this segment
    highlight: Option<(usize, usize)>,
}

/// Calculate highlight range relative to a segment.
/// Returns (local_start, local_end) if the match_range overlaps with the segment.
fn calc_segment_highlight(
    match_range: &Option<Range<usize>>,
    start_cell: usize,
    width: usize,
) -> Option<(usize, usize)> {
    match_range.as_ref().and_then(|r| {
        let seg_end_cell = start_cell + width;
        if r.start < seg_end_cell && r.end > start_cell {
            let local_start = r.start.saturating_sub(start_cell);
            let local_end = (r.end - start_cell).min(width);
            (local_start < local_end).then_some((local_start, local_end))
        } else {
            None
        }
    })
}

/// Push segment text to changes vector, with optional highlight formatting.
fn push_segment_with_highlight(
    changes: &mut Vec<Change>,
    segment: &WrappedSegment,
    colors: &ScrollbackSearchColors,
) {
    if let Some((hl_start, hl_end)) = segment.highlight {
        let (start_byte, end_byte) = get_segment_split_indices(segment.text, hl_start, hl_end);
        let before = &segment.text[..start_byte];
        let matched = &segment.text[start_byte..end_byte];
        let after = &segment.text[end_byte..];

        changes.push(Change::Text(before.to_string()));
        changes.extend([
            AttributeChange::Background(colors.match_bg).into(),
            AttributeChange::Foreground(colors.match_fg).into(),
            Change::Text(matched.to_string()),
            Change::AllAttributes(CellAttributes::default()),
        ]);
        changes.push(Change::Text(after.to_string()));
    } else {
        changes.push(Change::Text(segment.text.to_string()));
    }
}

/// Calculate how many rows a content with given column width will take when wrapped
fn wrapped_row_count(col_width: usize, max_width: usize) -> usize {
    if max_width == 0 || col_width == 0 {
        return 1;
    }
    col_width.div_ceil(max_width)
}

/// Wrap a line into segments that fit within max_width, preserving highlight information.
/// match_range is in cell units (not byte units) relative to the full line.
fn wrap_line_with_highlight<'a>(
    line: &'a str,
    match_range: Option<Range<usize>>,
    max_width: usize,
) -> Vec<WrappedSegment<'a>> {
    if max_width == 0 {
        return vec![WrappedSegment {
            text: "",
            highlight: None,
        }];
    }

    let mut segments = Vec::new();
    let mut current_width = 0;
    let mut current_start_cell = 0; // Cell index where current segment starts in the original line
    let mut cell_idx = 0;
    let mut segment_start_byte = 0;
    let mut current_byte = 0;

    for ch in line.chars() {
        let ch_width = char_column_width(ch);

        if current_width + ch_width > max_width && current_width > 0 {
            segments.push(WrappedSegment {
                text: &line[segment_start_byte..current_byte],
                highlight: calc_segment_highlight(&match_range, current_start_cell, current_width),
            });

            current_start_cell = cell_idx;
            current_width = 0;
            segment_start_byte = current_byte;
        }

        current_byte += ch.len_utf8();
        current_width += ch_width;
        cell_idx += ch_width;
    }

    // Don't forget the last segment
    if current_byte > segment_start_byte || segments.is_empty() {
        segments.push(WrappedSegment {
            text: &line[segment_start_byte..current_byte],
            highlight: calc_segment_highlight(&match_range, current_start_cell, current_width),
        });
    }

    segments
}

impl ScrollbackSearchState {
    fn new(pane_id: PaneId, window: ::window::Window, width: usize, height: usize) -> Self {
        Self {
            pane_id,
            window,
            search_input: LineEditBuffer::new("", 0),
            editing_search: true,
            options: SearchOptions::default(),
            matches: Vec::new(),
            selected_match: 0,
            scroll_offset: 0,
            view_mode: ViewMode::List,
            context_lines: DEFAULT_CONTEXT_LINES,
            width,
            height,
            search_history: Vec::new(),
            history_index: None,
            pending_search: false,
            last_search_trigger: None,
            count_buffer: String::new(),
            compiled_regex: None,
            colors: ScrollbackSearchColors::new(),
        }
    }

    /// Calculate how many terminal rows a match will take in compact view
    fn match_height_compact(&self, m: &SearchMatchWithContext) -> usize {
        let max_content_width = self.width.saturating_sub(COMPACT_LEFT_MARGIN);
        wrapped_row_count(m.content_col_width, max_content_width)
    }

    /// Calculate how many terminal rows a match will take in context (card) view
    fn match_height_card(&self, m: &SearchMatchWithContext, effective_context: usize) -> usize {
        let max_content = self.width.saturating_sub(CARD_LEFT_MARGIN);
        let match_line_rows = wrapped_row_count(m.content_col_width, max_content);

        // Calculate wrapped height for context lines
        let context_before_rows: usize = m
            .context_before
            .iter()
            .rev()
            .take(effective_context)
            .map(|ctx| wrapped_row_count(ctx.col_width, max_content))
            .sum();
        let context_after_rows: usize = m
            .context_after
            .iter()
            .take(effective_context)
            .map(|ctx| wrapped_row_count(ctx.col_width, max_content))
            .sum();

        // header(1) + context_before + match_lines + context_after + footer(1)
        1 + context_before_rows + match_line_rows + context_after_rows + 1
    }

    fn build_pattern(&self) -> Option<Pattern> {
        let text = self.search_input.get_line();
        if text.is_empty() {
            return None;
        }

        Some(match self.options.mode {
            SearchMode::CaseSensitive => Pattern::CaseSensitiveString(text.to_string()),
            SearchMode::CaseInsensitive => Pattern::CaseInSensitiveString(text.to_string()),
            SearchMode::Regex => Pattern::Regex(text.to_string()),
        })
    }

    #[allow(dead_code)]
    async fn perform_search(&mut self) -> anyhow::Result<()> {
        self.matches.clear();
        self.selected_match = 0;
        self.scroll_offset = 0;

        let pattern = match self.build_pattern() {
            Some(p) => p,
            None => return Ok(()),
        };

        let mux = Mux::get();
        let pane = mux
            .get_pane(self.pane_id)
            .ok_or_else(|| anyhow::anyhow!("pane not found"))?;

        let dims = pane.get_dimensions();
        let search_range =
            dims.physical_top..dims.physical_top + dims.scrollback_rows as StableRowIndex;

        let results = pane.search(pattern, search_range, None).await?;

        for result in results {
            let m = self.build_match_with_context(&pane, &result);
            self.matches.push(m);
        }

        let search_text = self.search_input.get_line().to_string();
        if !search_text.is_empty() && !self.search_history.contains(&search_text) {
            self.search_history.push(search_text);
        }

        Ok(())
    }

    fn build_match_with_context(
        &self,
        pane: &Arc<dyn Pane>,
        result: &SearchResult,
    ) -> SearchMatchWithContext {
        // Use get_logical_lines to get the same content the search matched against
        let logical_lines = pane.get_logical_lines(result.start_y..result.end_y + 1);
        let (line_content, first_row, last_row, match_range) =
            if let Some(logical) = logical_lines.first() {
                let first = logical.first_row;
                let last = first + logical.physical_lines.len() as StableRowIndex - 1;
                let content = logical.logical.as_str().trim_end().to_string();

                // Convert physical row coordinates to logical line position
                // result.start_y/start_x are physical row/column, we need position in logical line
                let rows_before_start = (result.start_y - first) as usize;
                let rows_before_end = (result.end_y - first) as usize;

                // Calculate cell offset by summing widths of physical lines before the match
                let mut start_offset = 0usize;
                for i in 0..rows_before_start {
                    if i < logical.physical_lines.len() {
                        start_offset += logical.physical_lines[i]
                            .visible_cells()
                            .map(|c| c.width())
                            .sum::<usize>();
                    }
                }
                let start_cell = start_offset + result.start_x;

                let mut end_offset = 0usize;
                for i in 0..rows_before_end {
                    if i < logical.physical_lines.len() {
                        end_offset += logical.physical_lines[i]
                            .visible_cells()
                            .map(|c| c.width())
                            .sum::<usize>();
                    }
                }
                let end_cell = end_offset + result.end_x;

                (content, first, last, start_cell..end_cell)
            } else {
                (
                    self.fetch_line_content(pane, result.start_y)
                        .trim_end()
                        .to_string(),
                    result.start_y,
                    result.end_y,
                    result.start_x..result.end_x,
                )
            };

        // Context before should end at the first physical row of the logical line
        // Filter out any lines that start at or after first_row (the match line itself)
        let context_before: Vec<_> = self
            .fetch_context_lines(
                pane,
                first_row.saturating_sub(self.context_lines as StableRowIndex),
                first_row,
            )
            .into_iter()
            .filter(|ctx| ctx.line_number < first_row)
            .collect();

        // Context after should start after the last physical row of the logical line
        // Filter out any lines that start at or before last_row (part of the match line)
        let context_after: Vec<_> = self
            .fetch_context_lines(
                pane,
                last_row + 1,
                last_row + 1 + self.context_lines as StableRowIndex,
            )
            .into_iter()
            .filter(|ctx| ctx.line_number > last_row)
            .collect();

        let content_col_width = str_column_width(&line_content);

        SearchMatchWithContext {
            line: first_row,
            match_range,
            line_content,
            content_col_width,
            context_before,
            context_after,
        }
    }

    fn fetch_context_lines(
        &self,
        pane: &Arc<dyn Pane>,
        start: StableRowIndex,
        end: StableRowIndex,
    ) -> Vec<ContextLine> {
        if start >= end {
            return Vec::new();
        }

        // Use logical lines to properly handle wrapped lines as single context entries
        let logical_lines = pane.get_logical_lines(start..end);
        logical_lines
            .into_iter()
            .map(|logical| {
                let content = logical.logical.as_str().trim_end().to_string();
                let col_width = str_column_width(&content);
                ContextLine {
                    line_number: logical.first_row,
                    content,
                    col_width,
                }
            })
            .collect()
    }

    fn fetch_line_content(&self, pane: &Arc<dyn Pane>, line: StableRowIndex) -> String {
        let (_, lines) = pane.get_lines(line..line + 1);
        lines
            .first()
            .map(|l| l.as_str().to_string())
            .unwrap_or_default()
    }

    fn render(&self, buf: &mut BufferedTerminal<TermWizTerminal>) -> anyhow::Result<()> {
        buf.add_changes(vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
        ]);

        self.render_header(buf);

        if self.matches.is_empty()
            && !self.search_input.get_line().is_empty()
            && !self.pending_search
        {
            self.render_no_matches(buf);
        } else {
            match self.view_mode {
                ViewMode::Compact => self.render_compact_list(buf),
                ViewMode::List => self.render_context_list(buf),
            }
        }

        // Position cursor and set visibility based on mode
        if self.editing_search {
            let search_text = self.search_input.get_line();
            let cursor_x = SEARCH_PROMPT_WIDTH + search_text.chars().count();
            buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(cursor_x),
                    y: Position::Absolute(0),
                },
                Change::CursorVisibility(termwiz::surface::CursorVisibility::Visible),
            ]);
        } else {
            buf.add_change(Change::CursorVisibility(
                termwiz::surface::CursorVisibility::Hidden,
            ));
        }

        buf.flush()?;
        Ok(())
    }

    fn render_header(&self, buf: &mut BufferedTerminal<TermWizTerminal>) {
        let search_text = self.search_input.get_line();
        let match_count = self.matches.len();
        let selected = if match_count > 0 {
            self.selected_match + 1
        } else {
            0
        };
        let remaining_width = self
            .width
            .saturating_sub(COMPACT_LEFT_MARGIN + search_text.len());
        let count_text = format!("[{} of {} matches]", selected, match_count);
        let padding = remaining_width.saturating_sub(count_text.len());

        // Check if regex is invalid using the already-compiled regex field
        let is_invalid_regex = matches!(self.options.mode, SearchMode::Regex)
            && !search_text.is_empty()
            && self.compiled_regex.is_none();

        let mut changes: Vec<Change> = vec![
            AttributeChange::Intensity(Intensity::Bold).into(),
            Change::Text("Search".to_string()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(": ".to_string()),
        ];

        if is_invalid_regex {
            changes.push(AttributeChange::Foreground(self.colors.error_fg).into());
        }
        changes.push(Change::Text(search_text.to_string()));
        if is_invalid_regex {
            changes.push(Change::AllAttributes(CellAttributes::default()));
        }

        changes.extend([
            Change::Text(" ".repeat(padding)),
            AttributeChange::Foreground(self.colors.header_fg).into(),
            Change::Text(count_text),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
        ]);

        changes.extend([
            AttributeChange::Foreground(self.colors.header_fg).into(),
            Change::Text(format!(
                "Context: ±{} │ Mode: {}",
                self.context_lines,
                self.options.mode.label(),
            )),
        ]);

        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
            AttributeChange::Foreground(self.colors.header_fg).into(),
            Change::Text("─".repeat(self.width)),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
        ]);

        buf.add_changes(changes);
    }

    fn render_compact_list(&self, buf: &mut BufferedTerminal<TermWizTerminal>) {
        // Account for box borders (top + bottom = 2 lines)
        let visible_height = self
            .height
            .saturating_sub(HEADER_ROWS + CARD_OVERHEAD + BOX_BORDERS);
        let border_color = self.colors.compact_border_fg;

        let header = format!("┌{}", "─".repeat(self.width.saturating_sub(1)));
        buf.add_changes(vec![
            AttributeChange::Foreground(border_color).into(),
            Change::Text(header),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
        ]);

        // Calculate how many matches fit, accounting for wrapped lines
        let mut accumulated_height = 0;
        let mut render_count = 0;
        for m in self.matches.iter().skip(self.scroll_offset) {
            let match_height = self.match_height_compact(m);
            if accumulated_height + match_height > visible_height && render_count > 0 {
                break;
            }
            accumulated_height += match_height;
            render_count += 1;
        }

        for (idx, m) in self
            .matches
            .iter()
            .skip(self.scroll_offset)
            .take(render_count)
            .enumerate()
        {
            let is_selected = self.scroll_offset + idx == self.selected_match;
            self.render_compact_match(buf, m, is_selected, border_color);
        }

        let footer = format!("└{}", "─".repeat(self.width.saturating_sub(1)));
        buf.add_changes(vec![
            AttributeChange::Foreground(border_color).into(),
            Change::Text(footer),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
        ]);
    }

    fn render_compact_match(
        &self,
        buf: &mut BufferedTerminal<TermWizTerminal>,
        m: &SearchMatchWithContext,
        selected: bool,
        border_color: ColorAttribute,
    ) {
        let line = &m.line_content;
        // Account for left border (1) + arrow (1) + line number (5) + " │ " (3) = 10 chars
        let max_content_width = self.width.saturating_sub(COMPACT_LEFT_MARGIN);

        let segments =
            wrap_line_with_highlight(line, Some(m.match_range.clone()), max_content_width);

        for (seg_idx, segment) in segments.iter().enumerate() {
            let mut changes: Vec<Change> = Vec::new();

            changes.extend([
                AttributeChange::Foreground(border_color).into(),
                Change::Text("│".to_string()),
                Change::AllAttributes(CellAttributes::default()),
            ]);

            if seg_idx == 0 {
                // First line: show selection indicator and line number
                if selected {
                    changes.extend([
                        AttributeChange::Foreground(self.colors.match_bg).into(),
                        AttributeChange::Intensity(Intensity::Bold).into(),
                        Change::Text("▶".to_string()),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);
                } else {
                    changes.push(Change::Text(" ".to_string()));
                }

                changes.extend([
                    AttributeChange::Foreground(self.colors.line_number_fg).into(),
                    Change::Text(format!("{:>5} │ ", m.line + 1)),
                    Change::AllAttributes(CellAttributes::default()),
                ]);
            } else {
                // Continuation lines: use spaces for alignment
                // Space for arrow (1) + line number (5) + " │ " (3) = 9 chars
                changes.push(Change::Text(" ".repeat(CONTINUATION_PADDING)));
            }

            push_segment_with_highlight(&mut changes, segment, &self.colors);

            changes.extend([
                Change::AllAttributes(CellAttributes::default()),
                Change::Text("\r\n".to_string()),
            ]);

            buf.add_changes(changes);
        }
    }

    fn render_context_list(&self, buf: &mut BufferedTerminal<TermWizTerminal>) {
        let visible_height = self.height.saturating_sub(HEADER_ROWS + CARD_OVERHEAD);
        // Card overhead: header(1) + footer(1) = 2 (match lines calculated separately)
        let card_overhead = CARD_OVERHEAD;
        // Max context that fits: (visible_height - overhead) / 2
        let effective_context =
            ((visible_height.saturating_sub(card_overhead)) / 2).min(self.context_lines);

        // Calculate how many matches fit, accounting for wrapped lines
        let mut accumulated_height = 0;
        let mut render_count = 0;
        for m in self.matches.iter().skip(self.scroll_offset) {
            let card_height = self.match_height_card(m, effective_context);
            if accumulated_height + card_height > visible_height && render_count > 0 {
                break;
            }
            accumulated_height += card_height;
            render_count += 1;
        }

        for (idx, m) in self
            .matches
            .iter()
            .skip(self.scroll_offset)
            .take(render_count)
            .enumerate()
        {
            let match_index = self.scroll_offset + idx;
            let is_selected = match_index == self.selected_match;
            self.render_match_card(buf, m, is_selected, match_index, effective_context);
        }
    }

    fn render_match_card(
        &self,
        buf: &mut BufferedTerminal<TermWizTerminal>,
        m: &SearchMatchWithContext,
        selected: bool,
        match_index: usize,
        max_context: usize,
    ) {
        let border_color = if selected {
            self.colors.card_selected_border_fg
        } else {
            self.colors.card_border_fg
        };

        // Use actual match index (1-based for display)
        let match_num = match_index + 1;

        // Build header: ┌─ Match N ── ... ── line XXX ──
        let left_label = format!("┌─ Match {} ", match_num);
        let right_label = format!(" line {} ──", m.line + 1);
        let fill_width = self
            .width
            .saturating_sub(left_label.chars().count() + right_label.chars().count());
        let header = format!("{}{}{}", left_label, "─".repeat(fill_width), right_label);

        buf.add_changes(vec![
            AttributeChange::Foreground(border_color).into(),
            Change::Text(header),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
        ]);

        // Limit context lines to what fits on screen
        let context_before: Vec<_> = m.context_before.iter().rev().take(max_context).collect();
        for ctx in context_before.into_iter().rev() {
            self.render_context_line(buf, ctx, border_color, false);
        }

        self.render_match_line_in_card(buf, m, border_color);

        for ctx in m.context_after.iter().take(max_context) {
            self.render_context_line(buf, ctx, border_color, false);
        }

        // Build footer: └─ ... ─ (open right)
        let footer = format!("└{}", "─".repeat(self.width.saturating_sub(1)));

        buf.add_changes(vec![
            AttributeChange::Foreground(border_color.into()).into(),
            Change::Text(footer),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
        ]);
    }

    fn render_context_line(
        &self,
        buf: &mut BufferedTerminal<TermWizTerminal>,
        ctx: &ContextLine,
        border_color: ColorAttribute,
        _is_match: bool,
    ) {
        let max_content = self.width.saturating_sub(CARD_LEFT_MARGIN);

        let segments = wrap_line_with_highlight(&ctx.content, None, max_content);

        for (seg_idx, segment) in segments.iter().enumerate() {
            let mut changes: Vec<Change> = Vec::new();

            changes.extend([
                AttributeChange::Foreground(border_color).into(),
                Change::Text("│".to_string()),
            ]);

            if seg_idx == 0 {
                // First line: show line number (1-indexed for display)
                changes.extend([
                    AttributeChange::Foreground(self.colors.line_number_fg).into(),
                    Change::Text(format!(" {:>5} │ ", ctx.line_number + 1)),
                    Change::AllAttributes(CellAttributes::default()),
                ]);
            } else {
                // Continuation lines: use spaces for alignment
                changes.extend([
                    Change::AllAttributes(CellAttributes::default()),
                    Change::Text(" ".repeat(CONTINUATION_PADDING)),
                ]);
            }

            changes.extend([
                Change::Text(segment.text.to_string()),
                Change::Text("\r\n".to_string()),
            ]);

            buf.add_changes(changes);
        }
    }

    fn render_match_line_in_card(
        &self,
        buf: &mut BufferedTerminal<TermWizTerminal>,
        m: &SearchMatchWithContext,
        border_color: ColorAttribute,
    ) {
        let line = &m.line_content;
        // Account for border (1) + arrow (1) + line number (5) + " │ " (3) = 10, plus extra border margin
        let max_content = self.width.saturating_sub(CARD_LEFT_MARGIN);

        let segments = wrap_line_with_highlight(line, Some(m.match_range.clone()), max_content);

        for (seg_idx, segment) in segments.iter().enumerate() {
            let mut changes: Vec<Change> = Vec::new();

            changes.extend([
                AttributeChange::Foreground(border_color).into(),
                Change::Text("│".to_string()),
            ]);

            if seg_idx == 0 {
                // First line: show arrow and line number
                changes.extend([
                    AttributeChange::Foreground(self.colors.arrow_fg).into(),
                    AttributeChange::Intensity(Intensity::Bold).into(),
                    Change::Text("▶".to_string()),
                    Change::AllAttributes(CellAttributes::default()),
                    AttributeChange::Foreground(self.colors.line_number_fg).into(),
                    Change::Text(format!("{:>5} │ ", m.line + 1)),
                    Change::AllAttributes(CellAttributes::default()),
                ]);
            } else {
                // Continuation lines: use spaces for alignment
                // Space for arrow (1) + line number (5) + " │ " (3) = 9 chars
                changes.extend([
                    Change::AllAttributes(CellAttributes::default()),
                    Change::Text(" ".repeat(CONTINUATION_PADDING)),
                ]);
            }

            push_segment_with_highlight(&mut changes, segment, &self.colors);

            changes.push(Change::Text("\r\n".to_string()));
            buf.add_changes(changes);
        }
    }

    fn render_no_matches(&self, buf: &mut BufferedTerminal<TermWizTerminal>) {
        let visible_height = self.height.saturating_sub(HEADER_ROWS + CARD_OVERHEAD);
        let middle_row = visible_height / 2;

        buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(HEADER_ROWS + middle_row),
            },
            AttributeChange::Foreground(self.colors.header_fg).into(),
        ]);

        let message = "No matches found";
        let padding = self.width.saturating_sub(message.len()) / 2;
        buf.add_change(Change::Text(format!("{}{}", " ".repeat(padding), message)));
        buf.add_change(Change::AllAttributes(CellAttributes::default()));
    }

    fn handle_input(&mut self, event: InputEvent) -> Option<ScrollbackSearchAction> {
        match event {
            InputEvent::Key(KeyEvent { key, modifiers }) => self.handle_key(key, modifiers),
            InputEvent::Mouse(_) => None,
            _ => None,
        }
    }

    fn handle_key(&mut self, key: KeyCode, modifiers: Modifiers) -> Option<ScrollbackSearchAction> {
        if self.editing_search {
            match (key, modifiers) {
                (KeyCode::Escape, _) => {
                    self.editing_search = false;
                }
                (KeyCode::Enter, _) => {
                    self.editing_search = false;
                    return Some(ScrollbackSearchAction::Search);
                }
                (KeyCode::Char('R'), Modifiers::CTRL) => {
                    self.options.mode = self.options.mode.cycle();
                    return Some(ScrollbackSearchAction::Search);
                }
                (KeyCode::Char(c), Modifiers::NONE) | (KeyCode::Char(c), Modifiers::SHIFT) => {
                    self.search_input.insert_char(c);
                    return Some(ScrollbackSearchAction::Search);
                }
                (KeyCode::Backspace, _) => {
                    self.search_input.kill_text(
                        termwiz::lineedit::Movement::BackwardChar(1),
                        termwiz::lineedit::Movement::BackwardChar(1),
                    );
                    return Some(ScrollbackSearchAction::Search);
                }
                (KeyCode::UpArrow, _) => {
                    self.history_prev();
                }
                (KeyCode::DownArrow, _) => {
                    self.history_next();
                }
                _ => {}
            }
        } else {
            match (key, modifiers) {
                (KeyCode::Char(c), Modifiers::NONE) if c.is_ascii_digit() => {
                    self.count_buffer.push(c);
                }
                (KeyCode::Escape, _) => {
                    self.reset_count();
                }
                (KeyCode::Char('q'), Modifiers::NONE) => {
                    return Some(ScrollbackSearchAction::Cancel);
                }
                (KeyCode::Enter, _) => {
                    self.reset_count();
                    self.yank_match(self.selected_match);
                    return Some(ScrollbackSearchAction::Cancel);
                }
                (KeyCode::Char('/'), Modifiers::NONE) => {
                    self.reset_count();
                    self.editing_search = true;
                }
                (KeyCode::Char('j'), Modifiers::NONE) | (KeyCode::DownArrow, _) => {
                    let count = self.take_count();
                    self.move_down(count);
                }
                (KeyCode::Char('k'), Modifiers::NONE) | (KeyCode::UpArrow, _) => {
                    let count = self.take_count();
                    self.move_up(count);
                }
                (KeyCode::Char('n'), Modifiers::NONE) => {
                    let count = self.take_count();
                    self.move_next_wrap(count);
                }
                (KeyCode::Char('N'), Modifiers::NONE) => {
                    let count = self.take_count();
                    self.move_prev_wrap(count);
                }
                (KeyCode::Char('g'), Modifiers::NONE) => {
                    self.reset_count();
                    self.go_to_first();
                }
                (KeyCode::Char('G'), Modifiers::NONE) => {
                    if self.count_buffer.is_empty() {
                        self.go_to_last();
                    } else {
                        let count = self.take_count();
                        self.go_to_match(count);
                    }
                }
                (KeyCode::Char('c'), Modifiers::NONE) => {
                    let count = if self.count_buffer.is_empty() {
                        DEFAULT_CONTEXT_LINES
                    } else {
                        self.take_count().min(self.max_context_lines())
                    };
                    self.context_lines = count;
                    return Some(ScrollbackSearchAction::RefreshContext);
                }
                (KeyCode::Tab, Modifiers::NONE) => {
                    self.reset_count();
                    self.increase_context();
                    return Some(ScrollbackSearchAction::RefreshContext);
                }
                (KeyCode::Tab, Modifiers::SHIFT) => {
                    self.reset_count();
                    self.decrease_context();
                    return Some(ScrollbackSearchAction::RefreshContext);
                }
                (KeyCode::Char('R'), Modifiers::CTRL) => {
                    self.reset_count();
                    self.options.mode = self.options.mode.cycle();
                    return Some(ScrollbackSearchAction::Search);
                }
                (KeyCode::Char('y'), Modifiers::NONE) => {
                    self.reset_count();
                    return Some(ScrollbackSearchAction::YankMatch(self.selected_match));
                }
                (KeyCode::Char('Y'), Modifiers::NONE) | (KeyCode::Char('Y'), Modifiers::SHIFT) => {
                    self.reset_count();
                    return Some(ScrollbackSearchAction::YankMatchWithContext(
                        self.selected_match,
                    ));
                }
                (KeyCode::Char('v'), Modifiers::NONE) => {
                    self.reset_count();
                    self.toggle_view_mode();
                }
                _ => {
                    self.reset_count();
                }
            }
        }
        None
    }

    fn move_up(&mut self, count: usize) {
        if self.matches.is_empty() || self.selected_match == 0 {
            return;
        }
        self.selected_match = self.selected_match.saturating_sub(count);
        self.adjust_scroll_for_selection();
    }

    fn move_down(&mut self, count: usize) {
        if self.matches.is_empty() || self.selected_match == self.matches.len() - 1 {
            return;
        }
        self.selected_match = (self.selected_match + count).min(self.matches.len() - 1);
        self.adjust_scroll_for_selection();
    }

    fn move_prev_wrap(&mut self, count: usize) {
        if self.matches.is_empty() {
            return;
        }
        for _ in 0..count {
            if self.selected_match > 0 {
                self.selected_match -= 1;
            } else {
                self.selected_match = self.matches.len() - 1;
            }
        }
        self.adjust_scroll_for_selection();
    }

    fn move_next_wrap(&mut self, count: usize) {
        if self.matches.is_empty() {
            return;
        }
        for _ in 0..count {
            if self.selected_match < self.matches.len() - 1 {
                self.selected_match += 1;
            } else {
                self.selected_match = 0;
            }
        }
        self.adjust_scroll_for_selection();
    }

    fn adjust_scroll_for_selection(&mut self) {
        if self.matches.is_empty() {
            return;
        }

        let visible_height = if self.view_mode == ViewMode::Compact {
            self.height
                .saturating_sub(HEADER_ROWS + CARD_OVERHEAD + BOX_BORDERS) // Account for box borders
        } else {
            self.height.saturating_sub(HEADER_ROWS + CARD_OVERHEAD)
        };

        if self.selected_match < self.scroll_offset {
            self.scroll_offset = self.selected_match;
            return;
        }

        // Calculate how many matches fit from current scroll_offset
        // and check if selected_match is visible
        let effective_context = if self.view_mode == ViewMode::Compact {
            0
        } else {
            let card_overhead = CARD_OVERHEAD;
            ((visible_height.saturating_sub(card_overhead)) / 2).min(self.context_lines)
        };

        let mut accumulated_height = 0;
        let mut last_visible_idx = self.scroll_offset;

        for (idx, m) in self.matches.iter().enumerate().skip(self.scroll_offset) {
            let match_height = if self.view_mode == ViewMode::Compact {
                self.match_height_compact(m)
            } else {
                self.match_height_card(m, effective_context)
            };

            if accumulated_height + match_height > visible_height && idx > self.scroll_offset {
                break;
            }
            accumulated_height += match_height;
            last_visible_idx = idx;
        }

        if self.selected_match > last_visible_idx {
            // Need to scroll so selected_match is visible
            // Work backwards from selected_match to find new scroll_offset
            let mut height_needed = 0;
            let mut new_offset = self.selected_match;

            for idx in (0..=self.selected_match).rev() {
                let m = &self.matches[idx];
                let match_height = if self.view_mode == ViewMode::Compact {
                    self.match_height_compact(m)
                } else {
                    self.match_height_card(m, effective_context)
                };

                if height_needed + match_height > visible_height && idx < self.selected_match {
                    break;
                }
                height_needed += match_height;
                new_offset = idx;
            }

            self.scroll_offset = new_offset;
        }
    }

    #[allow(dead_code)]
    fn scroll_to_top(&mut self) {
        self.selected_match = 0;
        self.scroll_offset = 0;
    }

    #[allow(dead_code)]
    fn scroll_to_bottom(&mut self) {
        if !self.matches.is_empty() {
            self.selected_match = self.matches.len() - 1;
            self.adjust_scroll_for_selection();
        }
    }

    fn max_context_lines(&self) -> usize {
        // header rows + card overhead + 1 for match line
        self.height.saturating_sub(HEADER_ROWS + CARD_OVERHEAD + 1) / 2
    }

    fn increase_context(&mut self) {
        if self.context_lines < self.max_context_lines() {
            self.context_lines += 1;
        }
    }

    fn decrease_context(&mut self) {
        self.context_lines = self.context_lines.saturating_sub(1);
    }

    fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Compact => ViewMode::List,
            ViewMode::List => ViewMode::Compact,
        };
    }

    fn history_prev(&mut self) {
        if self.search_history.is_empty() {
            return;
        }
        let new_idx = match self.history_index {
            None => self.search_history.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_index = Some(new_idx);
        let text = self.search_history[new_idx].clone();
        self.search_input = LineEditBuffer::new(&text, text.len());
    }

    fn history_next(&mut self) {
        let Some(current) = self.history_index else {
            return;
        };
        if current >= self.search_history.len().saturating_sub(1) {
            self.history_index = None;
            self.search_input = LineEditBuffer::new("", 0);
        } else {
            self.history_index = Some(current + 1);
            let text = self.search_history[current + 1].clone();
            self.search_input = LineEditBuffer::new(&text, text.len());
        }
    }

    #[allow(dead_code)]
    fn get_selected_match(&self) -> Option<&SearchMatchWithContext> {
        self.matches.get(self.selected_match)
    }

    #[allow(dead_code)]
    fn get_selected_match_content(&self) -> Option<String> {
        self.matches
            .get(self.selected_match)
            .map(|m| m.line_content.clone())
    }

    fn take_count(&mut self) -> usize {
        let count = self.count_buffer.parse::<usize>().unwrap_or(0);
        self.count_buffer.clear();
        count.max(1)
    }

    fn reset_count(&mut self) {
        self.count_buffer.clear();
    }

    fn go_to_first(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected_match = 0;
        self.adjust_scroll_for_selection();
    }

    fn go_to_last(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected_match = self.matches.len() - 1;
        self.adjust_scroll_for_selection();
    }

    fn go_to_match(&mut self, index: usize) {
        if self.matches.is_empty() {
            return;
        }
        // Convert 1-based index to 0-based, clamped to valid range
        let idx = index.saturating_sub(1).min(self.matches.len() - 1);
        self.selected_match = idx;
        self.adjust_scroll_for_selection();
    }

    fn run_loop(
        &mut self,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<Option<StableRowIndex>> {
        loop {
            let timeout = if self.pending_search {
                if let Some(trigger_time) = self.last_search_trigger {
                    let elapsed = trigger_time.elapsed();
                    let debounce = Duration::from_millis(SEARCH_DEBOUNCE_MS);
                    if elapsed >= debounce {
                        self.pending_search = false;
                        self.last_search_trigger = None;
                        self.execute_search();
                        self.render(buf)?;
                        None
                    } else {
                        Some(debounce - elapsed)
                    }
                } else {
                    None
                }
            } else {
                None
            };

            match buf.terminal().poll_input(timeout) {
                Ok(Some(event)) => {
                    if let InputEvent::Resized { cols, rows } = &event {
                        self.width = *cols;
                        self.height = *rows;
                    }

                    if let Some(action) = self.handle_input(event) {
                        match action {
                            ScrollbackSearchAction::Cancel => {
                                return Ok(None);
                            }
                            ScrollbackSearchAction::Search => {
                                self.pending_search = true;
                                self.last_search_trigger = Some(Instant::now());
                            }
                            ScrollbackSearchAction::RefreshContext => {
                                self.execute_search();
                            }
                            ScrollbackSearchAction::YankMatch(idx) => {
                                self.yank_match(idx);
                            }
                            ScrollbackSearchAction::YankMatchWithContext(idx) => {
                                self.yank_match_with_context(idx);
                            }
                        }
                    }

                    self.render(buf)?;
                }
                Ok(None) => {
                    if self.pending_search {
                        if let Some(trigger_time) = self.last_search_trigger {
                            if trigger_time.elapsed() >= Duration::from_millis(SEARCH_DEBOUNCE_MS) {
                                self.pending_search = false;
                                self.last_search_trigger = None;
                                self.execute_search();
                                self.render(buf)?;
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("scrollback search poll error: {}", e);
                    return Err(e.into());
                }
            }
        }
    }

    fn execute_search(&mut self) {
        let pattern = match self.build_pattern() {
            Some(p) => p,
            None => {
                self.matches.clear();
                self.selected_match = 0;
                self.scroll_offset = 0;
                return;
            }
        };

        let mux = Mux::get();
        if let Some(pane) = mux.get_pane(self.pane_id) {
            let dims = pane.get_dimensions();
            let range =
                dims.scrollback_top..dims.scrollback_top + dims.scrollback_rows as StableRowIndex;

            self.matches.clear();

            // For regex mode, perform our own search on trimmed content
            // so that $ matches end of visible content, not padding
            if matches!(self.options.mode, SearchMode::Regex) {
                let pattern_str = self.search_input.get_line();
                self.compiled_regex = Regex::new(pattern_str).ok();
                self.search_trimmed_content(&pane, range);
            } else {
                let results = smol::block_on(pane.search(pattern, range, None)).unwrap_or_default();
                for result in &results {
                    let m = self.build_match_with_context(&pane, result);
                    self.matches.push(m);
                }
            }
            self.selected_match = 0;
            self.scroll_offset = 0;
        }
    }

    fn search_trimmed_content(
        &mut self,
        pane: &Arc<dyn Pane>,
        range: std::ops::Range<StableRowIndex>,
    ) {
        let regex = match self.compiled_regex.as_ref() {
            Some(r) => r,
            None => return,
        };

        let logical_lines = pane.get_logical_lines(range);

        for logical in logical_lines {
            let content = logical.logical.as_str();
            let trimmed = content.trim_end();

            let first_row = logical.first_row;
            let last_row = first_row + logical.physical_lines.len() as StableRowIndex - 1;

            for m in regex.find_iter(trimmed) {
                // Calculate all column widths in a single pass
                let (start_cell, end_cell, content_col_width) =
                    column_widths_at_byte_positions(trimmed, m.start(), m.end());

                // Filter out any lines that overlap with the match line
                let context_before: Vec<_> = self
                    .fetch_context_lines(
                        pane,
                        first_row.saturating_sub(self.context_lines as StableRowIndex),
                        first_row,
                    )
                    .into_iter()
                    .filter(|ctx| ctx.line_number < first_row)
                    .collect();

                let context_after: Vec<_> = self
                    .fetch_context_lines(
                        pane,
                        last_row + 1,
                        last_row + 1 + self.context_lines as StableRowIndex,
                    )
                    .into_iter()
                    .filter(|ctx| ctx.line_number > last_row)
                    .collect();

                self.matches.push(SearchMatchWithContext {
                    line: first_row,
                    match_range: start_cell..end_cell,
                    line_content: trimmed.to_string(),
                    content_col_width,
                    context_before,
                    context_after,
                });
            }
        }
    }

    fn yank_match(&self, idx: usize) {
        if let Some(content) = self.matches.get(idx).map(|m| m.line_content.clone()) {
            self.window.notify(TermWindowNotif::PerformAssignment {
                pane_id: self.pane_id,
                assignment: KeyAssignment::CopyTextTo {
                    text: content,
                    destination: ClipboardCopyDestination::Clipboard,
                },
                tx: None,
            });
        }
    }

    fn yank_match_with_context(&self, idx: usize) {
        let Some(m) = self.matches.get(idx) else {
            return;
        };

        let mut lines = Vec::new();
        for ctx in &m.context_before {
            lines.push(ctx.content.as_str());
        }
        lines.push(&m.line_content);
        for ctx in &m.context_after {
            lines.push(ctx.content.as_str());
        }

        let content = lines.join("\n");

        self.window.notify(TermWindowNotif::PerformAssignment {
            pane_id: self.pane_id,
            assignment: KeyAssignment::CopyTextTo {
                text: content,
                destination: ClipboardCopyDestination::Clipboard,
            },
            tx: None,
        });
    }
}

#[derive(Debug, Clone)]
enum ScrollbackSearchAction {
    Cancel,
    Search,
    RefreshContext, // Immediate re-search (no debounce) for context changes
    YankMatch(usize),
    YankMatchWithContext(usize),
}

pub fn scrollback_search(
    pane_id: PaneId,
    term: TermWizTerminal,
    window: ::window::Window,
) -> anyhow::Result<Option<StableRowIndex>> {
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let size = buf.terminal().get_screen_size()?;
    let mut state = ScrollbackSearchState::new(pane_id, window, size.cols, size.rows);

    state.render(&mut buf)?;
    state.run_loop(&mut buf)
}
