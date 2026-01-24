use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{FormFieldChoice, InputForm, KeyAssignment};
use config::{configuration, AnsiColor, ColorAttribute};
use luahelper::impl_lua_conversion_dynamic;
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use std::rc::Rc;
use termwiz::input::{InputEvent, KeyCode, KeyEvent};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use wezterm_dynamic::{FromDynamic, ToDynamic};
use wezterm_term::{AttributeChange, CellAttributes, Intensity};
use window::Modifiers;

struct FormColors {
    label_fg: ColorAttribute,
    active_label_fg: ColorAttribute,
    placeholder_fg: ColorAttribute,
    input_fg: ColorAttribute,
    required_fg: ColorAttribute,
    border_fg: ColorAttribute,
    separator_fg: ColorAttribute,
}

impl FormColors {
    fn new() -> Self {
        let config = configuration();
        let colors = &config.resolved_palette;

        Self {
            label_fg: colors
                .form_label_fg
                .unwrap_or(AnsiColor::Purple.into())
                .into(),
            active_label_fg: colors
                .form_active_label_fg
                .unwrap_or(AnsiColor::Yellow.into())
                .into(),
            placeholder_fg: colors
                .form_placeholder_fg
                .unwrap_or(AnsiColor::Silver.into())
                .into(),
            input_fg: colors
                .form_input_fg
                .unwrap_or(AnsiColor::White.into())
                .into(),
            required_fg: colors
                .form_required_fg
                .unwrap_or(AnsiColor::Red.into())
                .into(),
            border_fg: colors
                .form_border_fg
                .map_or_else(|| AnsiColor::Grey.into(), |fg_color| fg_color.into()),
            separator_fg: colors
                .form_separator_fg
                .map_or_else(|| ColorAttribute::Default, |fg_color| fg_color.into()),
        }
    }
}

/// State for selector fields
struct SelectorFieldState {
    /// The filter/search term for fuzzy matching
    filter_term: String,
    /// Filtered choices after applying fuzzy search
    filtered_choices: Vec<FormFieldChoice>,
    /// Currently highlighted index in the dropdown
    active_choice_idx: usize,
    /// Whether the dropdown is currently open
    dropdown_open: bool,
    /// Scroll offset for the dropdown
    top_row: usize,
}

impl SelectorFieldState {
    fn new(choices: &[FormFieldChoice], initial_value: Option<&str>) -> Self {
        let filtered_choices = choices.to_vec();
        let active_choice_idx = if let Some(initial) = initial_value {
            choices
                .iter()
                .position(|c| c.id.as_deref().unwrap_or(&c.label) == initial)
                .unwrap_or(0)
        } else {
            0
        };

        Self {
            filter_term: String::new(),
            filtered_choices,
            active_choice_idx,
            dropdown_open: false,
            top_row: 0,
        }
    }

    fn update_filter(&mut self, choices: &[FormFieldChoice]) {
        if self.filter_term.is_empty() {
            self.filtered_choices = choices.to_vec();
            self.active_choice_idx = 0;
            self.top_row = 0;
            return;
        }

        let pattern = matcher_pattern(&self.filter_term);
        let mut scored: Vec<(usize, u32)> = choices
            .iter()
            .enumerate()
            .filter_map(|(idx, choice)| {
                let score = matcher_score(&pattern, &choice.label)?;
                Some((idx, score))
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));

        self.filtered_choices = scored
            .into_iter()
            .map(|(idx, _)| choices[idx].clone())
            .collect();

        self.active_choice_idx = 0;
        self.top_row = 0;
    }
}

struct FormState<'a> {
    args: &'a InputForm,
    window: GuiWin,
    pane: MuxPane,
    active_idx: usize,
    field_values: Vec<String>,
    field_cursors: Vec<usize>,
    /// State for selector fields (only populated for fields with choices)
    selector_states: Vec<Option<SelectorFieldState>>,
    colors: FormColors,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
}

impl<'a> FormState<'a> {
    fn new(
        args: &'a InputForm,
        window: GuiWin,
        pane: MuxPane,
        buf: &'a mut BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        let field_values: Vec<String> = args
            .fields
            .iter()
            .map(|f| {
                if !f.choices.is_empty() {
                    // For selector fields, use the initial value or the first choice's value
                    f.initial_value.clone().unwrap_or_else(|| {
                        f.choices
                            .first()
                            .map(|c| c.id.clone().unwrap_or_else(|| c.label.clone()))
                            .unwrap_or_default()
                    })
                } else {
                    f.initial_value.clone().unwrap_or_default()
                }
            })
            .collect();
        let field_cursors = field_values.iter().map(|v| v.chars().count()).collect();

        let selector_states: Vec<Option<SelectorFieldState>> = args
            .fields
            .iter()
            .map(|f| {
                if !f.choices.is_empty() {
                    Some(SelectorFieldState::new(
                        &f.choices,
                        f.initial_value.as_deref(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        Self {
            args,
            window,
            pane,
            active_idx: 0,
            field_values,
            field_cursors,
            selector_states,
            colors: FormColors::new(),
            buf,
        }
    }

    fn is_selector_field(&self, idx: usize) -> bool {
        self.selector_states
            .get(idx)
            .map(|s| s.is_some())
            .unwrap_or(false)
    }

    fn is_dropdown_open(&self, idx: usize) -> bool {
        self.selector_states
            .get(idx)
            .and_then(|s| s.as_ref())
            .map(|s| s.dropdown_open)
            .unwrap_or(false)
    }

    fn render(&mut self) -> anyhow::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        self.buf.add_changes(vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
        ]);

        let title = &self.args.title;
        self.buf.add_changes(vec![
            Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
            Change::Attribute(AttributeChange::Foreground(self.colors.active_label_fg)),
            Change::Text(title.clone()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
            Change::Attribute(AttributeChange::Foreground(self.colors.separator_fg)),
            Change::Text("─".repeat(cols)),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
        ]);

        let mut cursor_x = 0;
        let mut cursor_y = 0;
        let mut current_row = 3;

        // Pre-calculate dropdown info to know how to offset rows
        let mut dropdown_info: Option<(usize, usize, usize)> = None; // (field_idx, start_row, height)
        for (idx, _field) in self.args.fields.iter().enumerate() {
            if self.is_dropdown_open(idx) {
                if let Some(Some(selector_state)) = self.selector_states.get(idx) {
                    let start_row = 3 + idx + 1; // row after this field
                    let max_height = rows.saturating_sub(start_row + 3); // leave room for submit
                    let height = if selector_state.filtered_choices.is_empty() {
                        1
                    } else {
                        selector_state.filtered_choices.len().min(max_height).min(8)
                    };
                    // Add 1 for the bottom border of dropdown
                    dropdown_info = Some((idx, start_row, height + 1));
                }
                break;
            }
        }

        for (idx, field) in self.args.fields.iter().enumerate() {
            let is_active = idx == self.active_idx;
            let is_selector = !field.choices.is_empty();
            let dropdown_open = self.is_dropdown_open(idx);

            // If this field comes after a dropdown, offset it by dropdown height
            let is_after_dropdown = if let Some((dropdown_idx, _start, height)) = dropdown_info {
                if idx > dropdown_idx {
                    // Position this field after the dropdown
                    let field_row = 3 + idx + height;
                    self.buf.add_changes(vec![Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(field_row),
                    }]);
                    current_row = field_row;
                    true
                } else {
                    false
                }
            } else {
                false
            };

            // Add newline for fields that aren't explicitly positioned
            if !is_after_dropdown {
                self.buf.add_changes(vec![Change::Text("\r\n".to_string())]);
            }

            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(self.colors.required_fg)),
                Change::Text(if field.required {
                    "* ".to_string()
                } else {
                    "  ".to_string()
                }),
                Change::Attribute(AttributeChange::Foreground(if is_active {
                    self.colors.active_label_fg
                } else {
                    self.colors.label_fg
                })),
                Change::Text(field.label.clone()),
                Change::AllAttributes(CellAttributes::default()),
                Change::Text(": ".to_string()),
            ]);

            if is_selector {
                // For selector fields
                if let Some(selector_state) = &self.selector_states[idx] {
                    if dropdown_open {
                        // Show filter input when dropdown is open
                        let filter_display = format!("/{}", selector_state.filter_term);
                        self.buf.add_changes(vec![
                            Change::Attribute(AttributeChange::Foreground(self.colors.input_fg)),
                            Change::Text(filter_display.clone()),
                            Change::AllAttributes(CellAttributes::default()),
                            Change::Text(" ▾".to_string()),
                        ]);
                        if is_active {
                            cursor_y = current_row;
                            cursor_x = field.label.chars().count()
                                + 4
                                + 1
                                + selector_state.filter_term.chars().count();
                        }
                    } else {
                        // Show selected value when dropdown is closed
                        let value = &self.field_values[idx];
                        let display_label = field
                            .choices
                            .iter()
                            .find(|c| c.id.as_deref().unwrap_or(&c.label) == value)
                            .map(|c| c.label.clone())
                            .unwrap_or_else(|| value.clone());

                        let display_value = if display_label.is_empty() {
                            if let Some(placeholder) = &field.placeholder {
                                format!("({})", placeholder)
                            } else {
                                "(Select...)".to_string()
                            }
                        } else {
                            display_label
                        };

                        let input_color = if !self.field_values[idx].is_empty() {
                            self.colors.input_fg
                        } else {
                            self.colors.placeholder_fg
                        };

                        self.buf.add_changes(vec![
                            Change::Attribute(AttributeChange::Foreground(input_color)),
                            Change::Text(display_value.clone()),
                            Change::AllAttributes(CellAttributes::default()),
                            Change::Text(" ▾".to_string()),
                        ]);

                        if is_active {
                            cursor_y = current_row;
                            cursor_x = field.label.chars().count() + 4;
                        }
                    }
                }
            } else {
                // For regular text fields
                let value = &self.field_values[idx];
                let display_value = if field.is_password && !value.is_empty() {
                    "*".repeat(value.len())
                } else if value.is_empty() {
                    if let Some(placeholder) = &field.placeholder {
                        format!("({})", placeholder)
                    } else {
                        "".to_string()
                    }
                } else {
                    value.clone()
                };

                let input_color = if !value.is_empty() {
                    self.colors.input_fg
                } else {
                    self.colors.placeholder_fg
                };

                if is_active {
                    cursor_y = current_row;
                    cursor_x = field.label.chars().count() + 4 + self.field_cursors[idx];
                }

                self.buf.add_changes(vec![
                    Change::Attribute(AttributeChange::Foreground(input_color)),
                    Change::Text(display_value.clone()),
                    Change::AllAttributes(CellAttributes::default()),
                ]);
            }

            current_row += 1;
        }

        // Calculate where to put the submit row - after dropdown if open
        let submit_row = if let Some((_, start, height)) = dropdown_info {
            (start + height + 1).max(current_row + 1)
        } else {
            current_row + 1
        };

        let submit_label = self.args.submit_label.as_deref().unwrap_or("Submit");
        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(submit_row),
            },
            Change::Attribute(AttributeChange::Foreground(self.colors.border_fg)),
            Change::Text("[Ctrl+Enter] ".to_string()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!("{}  ", submit_label)),
            Change::Attribute(AttributeChange::Foreground(self.colors.border_fg)),
            Change::Text("[Esc] ".to_string()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("Cancel".to_string()),
        ]);

        // Render dropdown if open
        if let Some((field_idx, dropdown_start_row, total_height)) = dropdown_info {
            if let Some(Some(selector_state)) = self.selector_states.get(field_idx) {
                let field = &self.args.fields[field_idx];
                let label_offset = field.label.chars().count() + 4;
                let dropdown_width = 30.min(cols.saturating_sub(label_offset + 2));
                // total_height includes the border row, so actual choice rows = total_height - 1
                let choice_rows = total_height.saturating_sub(1);

                // Clear the dropdown area and draw a border for choice rows only
                for row in 0..choice_rows {
                    let display_row = dropdown_start_row + row;
                    self.buf.add_changes(vec![
                        Change::CursorPosition {
                            x: Position::Absolute(label_offset.saturating_sub(1)),
                            y: Position::Absolute(display_row),
                        },
                        Change::Attribute(AttributeChange::Foreground(self.colors.border_fg)),
                        Change::Text("│".to_string()),
                        Change::AllAttributes(CellAttributes::default()),
                        // Clear the rest of the line for the dropdown
                        Change::Text(" ".repeat(dropdown_width)),
                    ]);
                }

                // Draw bottom border after the choice rows
                self.buf.add_changes(vec![
                    Change::CursorPosition {
                        x: Position::Absolute(label_offset.saturating_sub(1)),
                        y: Position::Absolute(dropdown_start_row + choice_rows),
                    },
                    Change::Attribute(AttributeChange::Foreground(self.colors.border_fg)),
                    Change::Text("└".to_string()),
                    Change::Text("─".repeat(dropdown_width)),
                    Change::AllAttributes(CellAttributes::default()),
                ]);

                if selector_state.filtered_choices.is_empty() {
                    // Show "No matches" when filter returns no results
                    self.buf.add_changes(vec![
                        Change::CursorPosition {
                            x: Position::Absolute(label_offset),
                            y: Position::Absolute(dropdown_start_row),
                        },
                        Change::Attribute(AttributeChange::Foreground(self.colors.placeholder_fg)),
                        Change::Text("(No matches)".to_string()),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);
                } else {
                    for (row_num, (choice_idx, choice)) in selector_state
                        .filtered_choices
                        .iter()
                        .enumerate()
                        .skip(selector_state.top_row)
                        .enumerate()
                    {
                        if row_num >= choice_rows {
                            break;
                        }

                        let is_highlighted = choice_idx == selector_state.active_choice_idx;
                        let display_row = dropdown_start_row + row_num;

                        self.buf.add_changes(vec![Change::CursorPosition {
                            x: Position::Absolute(label_offset),
                            y: Position::Absolute(display_row),
                        }]);

                        if is_highlighted {
                            self.buf.add_changes(vec![Change::Attribute(
                                AttributeChange::Reverse(true),
                            )]);
                        }

                        let choice_label: String = choice
                            .label
                            .chars()
                            .take(dropdown_width.saturating_sub(2))
                            .collect();
                        let padded_label = format!(
                            "{:width$}",
                            choice_label,
                            width = dropdown_width.saturating_sub(1)
                        );
                        self.buf.add_changes(vec![Change::Text(padded_label)]);

                        if is_highlighted {
                            self.buf.add_changes(vec![Change::Attribute(
                                AttributeChange::Reverse(false),
                            )]);
                        }

                        self.buf
                            .add_changes(vec![Change::AllAttributes(CellAttributes::default())]);
                    }
                }
            }
        }

        // Hide cursor when a selector field is active but dropdown is closed
        // (no text input happening in that state)
        let cursor_visible = if self.is_selector_field(self.active_idx) {
            self.is_dropdown_open(self.active_idx)
        } else {
            true
        };

        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(cursor_x),
                y: Position::Absolute(cursor_y),
            },
            Change::CursorVisibility(if cursor_visible {
                CursorVisibility::Visible
            } else {
                CursorVisibility::Hidden
            }),
        ]);

        self.buf.flush()?;
        Ok(())
    }

    fn close_dropdown(&mut self) {
        if let Some(Some(selector_state)) = self.selector_states.get_mut(self.active_idx) {
            selector_state.dropdown_open = false;
            selector_state.filter_term.clear();
            let field = &self.args.fields[self.active_idx];
            selector_state.update_filter(&field.choices);
        }
    }

    fn open_dropdown(&mut self) {
        if let Some(Some(selector_state)) = self.selector_states.get_mut(self.active_idx) {
            selector_state.dropdown_open = true;
            selector_state.filter_term.clear();
            let field = &self.args.fields[self.active_idx];
            selector_state.update_filter(&field.choices);
        }
    }

    fn select_current_choice(&mut self) {
        let field_idx = self.active_idx;
        if let Some(Some(selector_state)) = self.selector_states.get_mut(field_idx) {
            if let Some(choice) = selector_state
                .filtered_choices
                .get(selector_state.active_choice_idx)
            {
                let value = choice.id.clone().unwrap_or_else(|| choice.label.clone());
                self.field_values[field_idx] = value;
            }
            selector_state.dropdown_open = false;
            selector_state.filter_term.clear();
            let field = &self.args.fields[field_idx];
            selector_state.update_filter(&field.choices);
        }
    }

    fn handle_selector_input(&mut self, event: &InputEvent) -> bool {
        let field_idx = self.active_idx;
        let dropdown_open = self.is_dropdown_open(field_idx);

        match event {
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }) if dropdown_open => {
                self.select_current_choice();
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                modifiers: Modifiers::NONE,
            }) if !dropdown_open => {
                // Enter opens dropdown when closed
                self.open_dropdown();
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) if dropdown_open => {
                self.close_dropdown();
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::UpArrow,
                ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('P'),
                modifiers: Modifiers::CTRL,
            }) if dropdown_open => {
                if let Some(Some(selector_state)) = self.selector_states.get_mut(field_idx) {
                    selector_state.active_choice_idx =
                        selector_state.active_choice_idx.saturating_sub(1);
                    if selector_state.active_choice_idx < selector_state.top_row {
                        selector_state.top_row = selector_state.active_choice_idx;
                    }
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::DownArrow,
                ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('N'),
                modifiers: Modifiers::CTRL,
            }) if dropdown_open => {
                if let Some(Some(selector_state)) = self.selector_states.get_mut(field_idx) {
                    let max_idx = selector_state.filtered_choices.len().saturating_sub(1);
                    selector_state.active_choice_idx =
                        (selector_state.active_choice_idx + 1).min(max_idx);
                    // Scroll down if needed (assuming max 8 visible items)
                    if selector_state.active_choice_idx > selector_state.top_row + 7 {
                        selector_state.top_row = selector_state.active_choice_idx.saturating_sub(7);
                    }
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers,
            }) if dropdown_open && (modifiers.is_empty() || *modifiers == Modifiers::SHIFT) => {
                if let Some(Some(selector_state)) = self.selector_states.get_mut(field_idx) {
                    selector_state.filter_term.push(*c);
                    let field = &self.args.fields[field_idx];
                    selector_state.update_filter(&field.choices);
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                ..
            }) if dropdown_open => {
                if let Some(Some(selector_state)) = self.selector_states.get_mut(field_idx) {
                    if selector_state.filter_term.pop().is_none() {
                        // Close dropdown if backspace on empty filter
                        selector_state.dropdown_open = false;
                    } else {
                        let field = &self.args.fields[field_idx];
                        selector_state.update_filter(&field.choices);
                    }
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('U'),
                modifiers: Modifiers::CTRL,
            }) if dropdown_open => {
                if let Some(Some(selector_state)) = self.selector_states.get_mut(field_idx) {
                    selector_state.filter_term.clear();
                    let field = &self.args.fields[field_idx];
                    selector_state.update_filter(&field.choices);
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers,
            }) if dropdown_open => {
                // Select current choice and move to next field
                self.select_current_choice();
                if *modifiers == Modifiers::SHIFT {
                    if self.active_idx > 0 {
                        self.active_idx -= 1;
                    } else {
                        self.active_idx = self.args.fields.len() - 1;
                    }
                } else {
                    if self.active_idx < self.args.fields.len() - 1 {
                        self.active_idx += 1;
                    } else {
                        self.active_idx = 0;
                    }
                }
                true
            }
            // Space or any typing when dropdown is closed opens it
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(' '),
                modifiers: Modifiers::NONE,
            }) if !dropdown_open => {
                self.open_dropdown();
                true
            }
            _ => false,
        }
    }

    /// Handle text field specific input. Returns true if handled, false otherwise.
    fn handle_text_field_input(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::Paste(text) => {
                // Filter out control characters and newlines for single-line text fields
                let filtered: String = text
                    .chars()
                    .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                    .collect();
                if !filtered.is_empty() {
                    let mut chars: Vec<char> = self.field_values[self.active_idx].chars().collect();
                    let pos = self.field_cursors[self.active_idx];
                    for (i, c) in filtered.chars().enumerate() {
                        chars.insert(pos + i, c);
                    }
                    self.field_values[self.active_idx] = chars.into_iter().collect();
                    self.field_cursors[self.active_idx] += filtered.chars().count();
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::LeftArrow,
                ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('B'),
                modifiers: Modifiers::CTRL,
            }) => {
                if self.field_cursors[self.active_idx] > 0 {
                    self.field_cursors[self.active_idx] -= 1;
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::RightArrow,
                ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('F'),
                modifiers: Modifiers::CTRL,
            }) => {
                if self.field_cursors[self.active_idx]
                    < self.field_values[self.active_idx].chars().count()
                {
                    self.field_cursors[self.active_idx] += 1;
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Home, ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('A'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.field_cursors[self.active_idx] = 0;
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::End, ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('E'),
                modifiers: Modifiers::CTRL,
            }) => {
                self.field_cursors[self.active_idx] =
                    self.field_values[self.active_idx].chars().count();
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Delete,
                ..
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('D'),
                modifiers: Modifiers::CTRL,
            }) => {
                let mut chars: Vec<char> = self.field_values[self.active_idx].chars().collect();
                let pos = self.field_cursors[self.active_idx];
                if pos < chars.len() {
                    chars.remove(pos);
                    self.field_values[self.active_idx] = chars.into_iter().collect();
                }
                true
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers,
            }) => {
                if modifiers.is_empty() || *modifiers == Modifiers::SHIFT {
                    let mut chars: Vec<char> = self.field_values[self.active_idx].chars().collect();
                    let pos = self.field_cursors[self.active_idx];
                    chars.insert(pos, *c);
                    self.field_values[self.active_idx] = chars.into_iter().collect();
                    self.field_cursors[self.active_idx] += 1;
                    true
                } else if *modifiers == Modifiers::CTRL {
                    match c {
                        'U' => {
                            self.field_values[self.active_idx].clear();
                            self.field_cursors[self.active_idx] = 0;
                        }
                        'K' => {
                            let chars: Vec<char> =
                                self.field_values[self.active_idx].chars().collect();
                            let pos = self.field_cursors[self.active_idx];
                            self.field_values[self.active_idx] = chars[0..pos].iter().collect();
                        }
                        'W' => {
                            let mut chars: Vec<char> =
                                self.field_values[self.active_idx].chars().collect();
                            let mut pos = self.field_cursors[self.active_idx];
                            let orig_pos = pos;
                            while pos > 0 && chars[pos - 1].is_whitespace() {
                                pos -= 1;
                            }
                            while pos > 0 && !chars[pos - 1].is_whitespace() {
                                pos -= 1;
                            }
                            chars.drain(pos..orig_pos);
                            self.field_values[self.active_idx] = chars.into_iter().collect();
                            self.field_cursors[self.active_idx] = pos;
                        }
                        _ => return false,
                    }
                    true
                } else {
                    false
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                ..
            }) => {
                let mut chars: Vec<char> = self.field_values[self.active_idx].chars().collect();
                let pos = self.field_cursors[self.active_idx];
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.field_values[self.active_idx] = chars.into_iter().collect();
                    self.field_cursors[self.active_idx] -= 1;
                }
                true
            }
            _ => false,
        }
    }

    /// Try to submit the form. Returns true if submitted, false if validation failed.
    fn try_submit(&mut self) -> bool {
        for (idx, field) in self.args.fields.iter().enumerate() {
            if field.required && self.field_values[idx].trim().is_empty() {
                self.active_idx = idx;
                return false;
            }
        }
        self.submit();
        true
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            let is_selector = self.is_selector_field(self.active_idx);

            // Handle field-specific input first
            let handled = if is_selector {
                self.handle_selector_input(&event)
            } else {
                self.handle_text_field_input(&event)
            };

            if handled {
                self.render()?;
                continue;
            }

            // Handle common events
            match event {
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    ..
                }) => break,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Tab,
                    modifiers: Modifiers::SHIFT,
                }) => {
                    self.close_dropdown();
                    if self.active_idx > 0 {
                        self.active_idx -= 1;
                    } else {
                        self.active_idx = self.args.fields.len() - 1;
                    }
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Tab, ..
                }) => {
                    self.close_dropdown();
                    if self.active_idx < self.args.fields.len() - 1 {
                        self.active_idx += 1;
                    } else {
                        self.active_idx = 0;
                    }
                }
                // Ctrl+P or Up Arrow moves to previous field
                InputEvent::Key(
                    KeyEvent {
                        key: KeyCode::Char('P'),
                        modifiers: Modifiers::CTRL,
                    }
                    | KeyEvent {
                        key: KeyCode::UpArrow,
                        ..
                    },
                ) => {
                    if self.active_idx > 0 {
                        self.active_idx -= 1;
                    }
                }
                // Ctrl+N or Down Arrow moves to next field
                InputEvent::Key(
                    KeyEvent {
                        key: KeyCode::Char('N'),
                        modifiers: Modifiers::CTRL,
                    }
                    | KeyEvent {
                        key: KeyCode::DownArrow,
                        ..
                    },
                ) => {
                    if self.active_idx < self.args.fields.len() - 1 {
                        self.active_idx += 1;
                    }
                }
                // Ctrl+Enter submits the form
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    modifiers: Modifiers::CTRL,
                }) => {
                    if self.try_submit() {
                        break;
                    }
                }
                InputEvent::Resized { cols, rows } => {
                    self.buf.resize(cols, rows);
                }
                _ => {}
            }
            self.render()?;
        }
        Ok(())
    }

    fn submit(&self) {
        let name = match *self.args.action {
            KeyAssignment::EmitEvent(ref id) => id,
            _ => {
                log::error!("InputForm requires action to be defined by wezterm.action_callback");
                return;
            }
        };

        let result = InputFormResult {
            fields: self
                .args
                .fields
                .iter()
                .zip(self.field_values.iter())
                .map(|(f, v)| FormFieldResult {
                    id: f.id.clone(),
                    value: v.clone(),
                })
                .collect(),
        };

        self.trigger_event(name, Some(result));
    }

    fn trigger_event(&self, name: &str, result: Option<InputFormResult>) {
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
struct FormFieldResult {
    id: String,
    value: String,
}

#[derive(FromDynamic, ToDynamic)]
struct InputFormResult {
    fields: Vec<FormFieldResult>,
}
impl_lua_conversion_dynamic!(InputFormResult);

fn trampoline(name: String, window: GuiWin, pane: MuxPane, result: Option<InputFormResult>) {
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
    result: Option<InputFormResult>,
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

pub fn show_input_form_overlay(
    term: TermWizTerminal,
    args: InputForm,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let mut state = FormState::new(&args, window, pane, &mut buf);

    state.render()?;
    state.run_loop()
}
