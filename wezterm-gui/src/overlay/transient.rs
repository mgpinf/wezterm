use crate::overlay::common::{
    display_key, EntryRenderStyle, KeyLookup, KeyMap, LoopAction, OverlayColors,
};
use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{
    KeyAssignment, TransientArgument as KTransientArgument, TransientContext as KTransientContext,
    TransientCyclicSwitch as KTransientCyclicSwitch, TransientEntry as KTransientEntry,
    TransientMenu as KTransientMenu, TransientOption as KTransientOption,
    TransientSection as KTransientSection, TransientSwitch as KTransientSwitch,
};
use config::ColorAttribute;
use luahelper::impl_lua_conversion_dynamic;
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use rayon::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use termwiz::input::{InputEvent, KeyCode, KeyEvent};
use termwiz::lineedit::{LineEditBuffer, Movement};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use wezterm_dynamic::{FromDynamic, ToDynamic, Value};
use wezterm_term::{unicode_column_width, AttributeChange, CellAttributes, Intensity};
use window::Modifiers;

/// Concatenate a prefix and value without format! overhead
#[inline]
fn concat_str(prefix: &str, value: &str) -> String {
    let mut s = String::with_capacity(prefix.len() + value.len());
    s.push_str(prefix);
    s.push_str(value);
    s
}

/// Concatenate prefix, value, and suffix without format! overhead
#[inline]
fn concat_str3(prefix: &str, value: &str, suffix: &str) -> String {
    let mut s = String::with_capacity(prefix.len() + value.len() + suffix.len());
    s.push_str(prefix);
    s.push_str(value);
    s.push_str(suffix);
    s
}

const ROW_OVERHEAD: usize = 6;

struct SelectorState<'a> {
    active_idx: usize,
    max_items: usize,
    top_row: usize,
    filter_term: String,
    filtered_entries: Vec<&'a str>,
    choices: &'a [String],
    option: &'a TransientOption<'a>,
}

impl SelectorState<'_> {
    fn update_filter(&mut self) {
        if self.filter_term.is_empty() {
            self.filtered_entries = self.choices.iter().map(|choice| choice.as_str()).collect();
            return;
        }

        self.filtered_entries.clear();

        struct MatchResult {
            row_idx: usize,
            score: u32,
        }

        let pattern = matcher_pattern(&self.filter_term);

        let mut scores: Vec<MatchResult> = self
            .choices
            .par_iter()
            .enumerate()
            .filter_map(|(row_idx, entry)| {
                let score = matcher_score(&pattern, entry)?;
                Some(MatchResult { row_idx, score })
            })
            .collect();

        scores.sort_by(|a, b| a.score.cmp(&b.score).reverse());

        for result in scores {
            self.filtered_entries.push(&self.choices[result.row_idx]);
        }

        self.active_idx = 0;
        self.top_row = 0;
    }

    fn move_up(&mut self) {
        self.active_idx = self.active_idx.saturating_sub(1);
        if self.active_idx < self.top_row {
            self.top_row = self.active_idx;
        }
    }

    fn move_down(&mut self) {
        self.active_idx = (self.active_idx + 1).min(self.filtered_entries.len() - 1);
        if self.active_idx > self.top_row + self.max_items {
            self.top_row = self.active_idx.saturating_sub(self.max_items);
        }
    }
}

struct PromptState<'a> {
    line: LineEditBuffer,
    option: &'a TransientOption<'a>,
}

struct TransientSwitch<'a> {
    delegate: &'a KTransientSwitch,
    value: Cell<bool>,
}

impl<'a> TransientSwitch<'a> {
    fn render(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let delegate = self.delegate;

        let mut changes = Vec::with_capacity(12);
        changes.push(Change::Text("  ".to_string()));
        style.append_key(
            colors,
            &delegate.key,
            max_key_width,
            &mut changes,
        );
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(concat_str3(" ", &delegate.description, " (")),
        ]);

        if self.value.get() {
            changes.push(Change::Attribute(AttributeChange::Intensity(
                Intensity::Bold,
            )));
            changes.push(Change::Attribute(AttributeChange::Foreground(
                colors.active_flag_fg,
            )));
        } else {
            changes.push(Change::Attribute(AttributeChange::Foreground(
                colors.inactive_flag_fg,
            )));
        }

        changes.push(Change::Text(delegate.flag.clone()));
        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::Text(")".to_string()));

        style.apply(colors, &mut changes);
        buf.add_changes(changes);

        Ok(())
    }
}

struct TransientOption<'a> {
    delegate: &'a KTransientOption,
    value: RefCell<Option<String>>,
}

impl<'a> TransientOption<'a> {
    fn render(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let delegate = self.delegate;

        let mut changes = Vec::with_capacity(15);
        changes.push(Change::Text("  ".to_string()));
        style.append_key(
            colors,
            &delegate.key,
            max_key_width,
            &mut changes,
        );
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(concat_str3(" ", &delegate.description, " (")),
        ]);

        if let Some(val) = self.value.borrow().as_deref() {
            changes.extend([
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Normal)),
                Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                Change::Text(val.to_string()),
            ]);
        } else {
            changes.extend([
                Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                Change::Text(delegate.flag.to_string()),
            ]);
        }

        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::Text(")".to_string()));

        style.apply(colors, &mut changes);
        buf.add_changes(changes);

        Ok(())
    }
}

struct TransientCyclicSwitch<'a> {
    delegate: &'a KTransientCyclicSwitch,
    active_idx: Cell<Option<usize>>,
}

impl<'a> TransientCyclicSwitch<'a> {
    fn render(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let delegate = self.delegate;

        // Base: 12 elements + up to 5 per choice (when active choice is highlighted)
        let mut changes = Vec::with_capacity(14 + delegate.choices.len() * 5);
        changes.push(Change::Text("  ".to_string()));
        style.append_key(
            colors,
            &delegate.key,
            max_key_width,
            &mut changes,
        );
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(concat_str3(" ", &delegate.description, " (")),
        ]);

        if let Some(idx) = self.active_idx.get() {
            changes.extend([
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if !delegate.choices.is_empty() {
                changes.push(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_flag_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in delegate.choices.iter().enumerate() {
                    if cur_idx == idx {
                        changes.extend([
                            Change::Text(prefix.to_string()),
                            Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                            Change::Text(choice.to_string()),
                            Change::AllAttributes(CellAttributes::default()),
                            Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                        ]);
                    } else {
                        changes.push(Change::Text(concat_str(prefix, choice)));
                    }
                    if cur_idx == 0 {
                        prefix = "|";
                    }
                }
                changes.push(Change::Text("]".to_string()));
                changes.push(Change::AllAttributes(CellAttributes::default()));
            }
        } else {
            changes.extend([
                Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if !delegate.choices.is_empty() {
                changes.push(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_flag_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in delegate.choices.iter().enumerate() {
                    changes.push(Change::Text(concat_str(prefix, choice)));
                    if cur_idx == 0 {
                        prefix = "|";
                    }
                }
                changes.push(Change::Text("]".to_string()));
                changes.push(Change::AllAttributes(CellAttributes::default()));
            }
        }
        changes.push(Change::Text(")".to_string()));

        style.apply(colors, &mut changes);
        buf.add_changes(changes);

        Ok(())
    }
}

struct TransientArgument<'a> {
    delegate: &'a KTransientArgument,
}

impl<'a> TransientArgument<'a> {
    fn render(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let mut changes = Vec::with_capacity(7);
        changes.push(Change::Text("  ".to_string()));
        style.append_key(
            colors,
            &self.delegate.key,
            max_key_width,
            &mut changes,
        );
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(concat_str(" ", &self.delegate.description)),
        ]);

        style.apply(colors, &mut changes);
        buf.add_changes(changes);

        Ok(())
    }
}

struct TransientSection<'a> {
    delegate: &'a KTransientSection,
    entries: Vec<RenderableEntity<'a>>,
    max_key_width: usize,
}

enum RenderableEntity<'a> {
    Opt(TransientOption<'a>),
    Switch(TransientSwitch<'a>),
    Argument(TransientArgument<'a>),
    CyclicSwitch(TransientCyclicSwitch<'a>),
}

impl RenderableEntity<'_> {
    fn key(&self) -> &str {
        match self {
            Self::Opt(option) => &option.delegate.key,
            Self::Switch(switch) => &switch.delegate.key,
            Self::CyclicSwitch(cyclic_switch) => &cyclic_switch.delegate.key,
            Self::Argument(positional_arg) => &positional_arg.delegate.key,
        }
    }

    fn render(
        &self,
        colors: &OverlayColors,
        prefix: &str,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let style = EntryRenderStyle::new(self.key(), prefix);

        match self {
            Self::Opt(option) => option.render(colors, style, max_key_width, buf),
            Self::Switch(switch) => switch.render(colors, style, max_key_width, buf),
            Self::CyclicSwitch(cyclic_switch) => {
                cyclic_switch.render(colors, style, max_key_width, buf)
            }
            Self::Argument(positional_arg) => {
                positional_arg.render(colors, style, max_key_width, buf)
            }
        }
    }
}

enum InputMode<'a> {
    Prompt(PromptState<'a>),
    Selector(SelectorState<'a>),
}

struct TransientState<'a> {
    window: GuiWin,
    pane: MuxPane,
    description: String,
    colors: OverlayColors,
    keymap: &'a KeyMap<'a, RenderableEntity<'a>>,
    typed: String,
    sections: &'a [TransientSection<'a>],
    cancel: Option<Box<KeyAssignment>>,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
    context: Option<&'a KTransientContext>,
    mode: Option<InputMode<'a>>,
    description_separator: String,
    cols_separator: String,
}

impl<'a> TransientState<'a> {
    fn new(
        args: &'a KTransientMenu,
        window: GuiWin,
        pane: MuxPane,
        sections: &'a [TransientSection<'_>],
        keymap: &'a KeyMap<'a, RenderableEntity<'a>>,
        buf: &'a mut BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        let context = args.context.as_ref();

        let description_len =
            crate::tabbar::parse_status_text(&args.description, CellAttributes::blank()).len();
        let description_separator = "─".repeat(description_len);

        let (cols, _) = buf.dimensions();
        let cols_separator = "─".repeat(cols);

        Self {
            window,
            pane,
            description: args.description.clone(),
            colors: OverlayColors::new(),
            keymap,
            typed: String::new(),
            sections,
            cancel: args.cancel.clone(),
            buf,
            context,
            mode: None,
            description_separator,
            cols_separator,
        }
    }

    fn render(&mut self) -> anyhow::Result<()> {
        self.buf.add_changes(vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
            Change::Attribute(AttributeChange::Foreground(self.colors.description_fg)),
            Change::Text(self.description.clone()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
            Change::Attribute(AttributeChange::Foreground(self.colors.separator_fg)),
            Change::Text(self.description_separator.clone()),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        if let Some(context) = self.context {
            // 5 base elements + 5 per entry
            let mut changes = Vec::with_capacity(5 + context.entries.len() * 5);
            changes.extend([
                Change::Text("\r\n\r\n".to_string()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(self.colors.context_header_fg)),
                Change::Text(context.header.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);

            for entry in &context.entries {
                changes.extend([
                    Change::Text("\r\n".to_string()),
                    Change::Attribute(AttributeChange::Foreground(self.colors.context_label_fg)),
                    Change::Text(entry.label.clone()),
                    Change::AllAttributes(CellAttributes::default()),
                    Change::Text(concat_str(": ", &entry.id)),
                ]);
            }

            self.buf.add_changes(changes);
        }

        for section in self.sections {
            self.buf.add_changes(vec![
                Change::Text("\r\n\r\n".to_string()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(self.colors.section_header_fg)),
                Change::Text(section.delegate.header.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            for entity in &section.entries {
                self.buf.add_change(Change::Text("\r\n".to_string()));
                entity.render(&self.colors, &self.typed, section.max_key_width, self.buf)?;
            }
        }

        if let Some(input_mode) = self.mode.as_ref() {
            match input_mode {
                InputMode::Prompt(prompt_state) => {
                    let (_, rows) = self.buf.dimensions();

                    self.buf.add_changes(vec![
                        Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(rows - 3),
                        },
                        Change::ClearToEndOfScreen(ColorAttribute::Default),
                        Change::Attribute(AttributeChange::Foreground(self.colors.separator_fg)),
                        Change::Text(self.cols_separator.clone()),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text("\r\n".to_string()),
                        Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                        Change::Attribute(AttributeChange::Foreground(self.colors.prompt_label_fg)),
                        Change::Text(prompt_state.option.delegate.description.clone()),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);

                    let mut cursor_x = prompt_state.option.delegate.description.len()
                        + 2
                        + prompt_state.line.get_cursor();

                    if let Some(default) = prompt_state.option.delegate.default.as_deref() {
                        cursor_x += 10 + default.len() + 1;
                        self.buf.add_changes(vec![
                            Change::Text(" (default ".to_string()),
                            Change::Attribute(AttributeChange::Foreground(
                                self.colors.default_value_fg,
                            )),
                            Change::Text(default.to_string()),
                            Change::AllAttributes(CellAttributes::default()),
                            Change::Text(")".to_string()),
                        ]);
                    }

                    self.buf.add_changes(vec![
                        Change::Text(concat_str(": ", prompt_state.line.get_line())),
                        Change::CursorVisibility(CursorVisibility::Visible),
                        Change::CursorPosition {
                            x: Position::Absolute(cursor_x),
                            y: Position::Absolute(rows - 2),
                        },
                    ]);
                }
                InputMode::Selector(selector_state) => {
                    let (cols, rows) = self.buf.dimensions();
                    let max_width = cols.saturating_sub(6);

                    let selector_size = selector_state.choices.len().min(selector_state.max_items);
                    let visible_rows = selector_size.min(selector_state.max_items + 1);

                    // 8 base elements + ~10 per visible row (varies due to line.changes)
                    let mut changes = Vec::with_capacity(8 + visible_rows * 10);
                    changes.extend([
                        Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(rows.saturating_sub(selector_size + 3)),
                        },
                        Change::ClearToEndOfScreen(ColorAttribute::Default),
                        Change::Attribute(AttributeChange::Foreground(self.colors.separator_fg)),
                        Change::Text(self.cols_separator.clone()),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text("\r\n".to_string()),
                        Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                        Change::Attribute(AttributeChange::Foreground(
                            self.colors.selector_label_fg,
                        )),
                        Change::Text(selector_state.option.delegate.description.clone()),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text(concat_str(": ", &selector_state.filter_term)),
                    ]);

                    for (row_num, (entry_idx, entry)) in selector_state
                        .filtered_entries
                        .iter()
                        .enumerate()
                        .skip(selector_state.top_row)
                        .enumerate()
                    {
                        if row_num > selector_state.max_items {
                            break;
                        }

                        changes.push(Change::Text("\r\n".to_string()));

                        let mut attr = CellAttributes::blank();

                        if entry_idx == selector_state.active_idx {
                            changes.push(Change::Attribute(AttributeChange::Reverse(true)));
                            attr.set_reverse(true);
                        }

                        changes.push(Change::Text("    ".to_string()));
                        let mut line = crate::tabbar::parse_status_text(entry, attr.clone());
                        if line.len() > max_width {
                            line.resize(max_width, termwiz::surface::SEQ_ZERO);
                        }
                        changes.extend(line.changes(&attr));
                        changes.push(Change::Text(" ".to_string()));
                        if entry_idx == selector_state.active_idx {
                            changes.push(Change::Attribute(AttributeChange::Reverse(false)));
                        }
                        changes.push(Change::AllAttributes(CellAttributes::default()));
                    }

                    changes.extend([
                        Change::CursorVisibility(CursorVisibility::Visible),
                        Change::CursorPosition {
                            x: Position::Absolute(
                                2 + selector_state.option.delegate.description.len()
                                    + selector_state.filter_term.len(),
                            ),
                            y: Position::Absolute(rows.saturating_sub(selector_size + 2)),
                        },
                    ]);

                    self.buf.add_changes(changes);
                }
            }
        }

        self.buf.flush()?;

        Ok(())
    }

    fn trigger_event(&self, name: &str, result: Option<TransientResult>) {
        let name = name.to_string();
        let window = self.window.clone();
        let pane = self.pane;

        promise::spawn::spawn_into_main_thread(async move {
            trampoline(name, window, pane, result);
            anyhow::Result::<()>::Ok(())
        })
        .detach();
    }

    /// Handles a keymap character input.
    fn handle_keymap_char(&mut self, c: char) -> anyhow::Result<LoopAction> {
        self.typed.push(c);

        match self.keymap.lookup(&self.typed) {
            KeyLookup::Found(transient_entry) => {
                match transient_entry {
                    RenderableEntity::Switch(switch) => {
                        switch.value.update(|val| !val);
                    }
                    RenderableEntity::Opt(option) => {
                        if option.value.borrow().is_none() || !option.delegate.allow_nil {
                            self.mode = if let Some(choices) = option.delegate.choices.as_deref() {
                                let (_, rows) = self.buf.dimensions();
                                let max_items = rows.saturating_sub(ROW_OVERHEAD);
                                let filtered_entries =
                                    choices.iter().map(|choice| choice.as_str()).collect();

                                Some(InputMode::Selector(SelectorState {
                                    active_idx: 0,
                                    max_items,
                                    top_row: 0,
                                    filter_term: String::new(),
                                    filtered_entries,
                                    choices,
                                    option,
                                }))
                            } else {
                                Some(InputMode::Prompt(PromptState {
                                    line: LineEditBuffer::default(),
                                    option,
                                }))
                            }
                        } else {
                            option.value.replace(None);
                        }
                    }
                    RenderableEntity::CyclicSwitch(cyclic_switch) => {
                        if !cyclic_switch.delegate.choices.is_empty() {
                            cyclic_switch.active_idx.update(|idx| {
                                if let Some(idx) = idx {
                                    if idx == cyclic_switch.delegate.choices.len() - 1 {
                                        if cyclic_switch.delegate.allow_nil {
                                            None
                                        } else {
                                            Some(0)
                                        }
                                    } else {
                                        Some(idx + 1)
                                    }
                                } else {
                                    Some(0)
                                }
                            });
                        }
                    }
                    RenderableEntity::Argument(positional_arg) => {
                        let name = match *positional_arg.delegate.action {
                            KeyAssignment::EmitEvent(ref id) => id,
                            _ => anyhow::bail!("TransientMenu requires action to be defined by wezterm.action_callback")
                        };

                        let result = TransientResult::from(self.sections);
                        self.trigger_event(name, Some(result));
                        if !positional_arg.delegate.keep_overlay {
                            return Ok(LoopAction::Break);
                        }
                    }
                }
                self.typed.clear();
            }
            KeyLookup::Prefix => {}
            KeyLookup::NotFound => self.typed.clear(),
        }

        Ok(LoopAction::Render)
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            match &mut self.mode {
                None => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('G' | 'C' | 'D' | '['),
                        modifiers: Modifiers::CTRL,
                    })
                    | InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        if let Some(key_assignment) = self.cancel.as_ref() {
                            if let KeyAssignment::EmitEvent(ref id) = **key_assignment {
                                self.trigger_event(id, None);
                            }
                        }
                        break;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        ..
                    }) => {
                        if let LoopAction::Break = self.handle_keymap_char(c)? {
                            break;
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        if let LoopAction::Break = self.handle_keymap_char('\n')? {
                            break;
                        }
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) => {
                        self.typed.pop();
                    }
                    InputEvent::Resized { cols, rows } => {
                        self.cols_separator = "─".repeat(cols);
                        self.buf.resize(cols, rows);
                    }
                    _ => {}
                },
                Some(InputMode::Prompt(prompt_state)) => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('G' | 'C' | 'D' | '['),
                        modifiers: Modifiers::CTRL,
                    })
                    | InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        self.mode = None;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('B'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        prompt_state.line.exec_movement(Movement::BackwardChar(1));
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('F'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        prompt_state.line.exec_movement(Movement::ForwardChar(1));
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('A'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        prompt_state.line.exec_movement(Movement::StartOfLine);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('E'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        prompt_state.line.exec_movement(Movement::EndOfLine);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('B'),
                        modifiers: Modifiers::ALT,
                    }) => {
                        prompt_state.line.exec_movement(Movement::BackwardWord(1));
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('F'),
                        modifiers: Modifiers::ALT,
                    }) => {
                        prompt_state.line.exec_movement(Movement::ForwardWord(1));
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('W'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        prompt_state
                            .line
                            .kill_text(Movement::BackwardWord(1), Movement::BackwardWord(1));
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('K'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        prompt_state
                            .line
                            .kill_text(Movement::EndOfLine, Movement::EndOfLine);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('U'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        prompt_state
                            .line
                            .kill_text(Movement::StartOfLine, Movement::StartOfLine);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        modifiers: Modifiers::NONE,
                    }) => {
                        prompt_state.line.insert_char(c);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        modifiers: Modifiers::SHIFT,
                    }) => {
                        prompt_state.line.insert_char(c);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) => {
                        prompt_state
                            .line
                            .kill_text(Movement::BackwardChar(1), Movement::BackwardChar(1));
                    }
                    InputEvent::Paste(text) => {
                        prompt_state.line.insert_text(&text);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        let line = prompt_state.line.get_line();
                        let new_val = if line.is_empty() {
                            Some(
                                prompt_state
                                    .option
                                    .delegate
                                    .default
                                    .clone()
                                    .unwrap_or_default(),
                            )
                        } else {
                            Some(line.to_string())
                        };
                        prompt_state.option.value.replace(new_val);
                        self.mode = None;
                    }
                    InputEvent::Resized { cols, rows } => {
                        self.cols_separator = "─".repeat(cols);
                        self.buf.resize(cols, rows);
                    }
                    _ => {}
                },
                Some(InputMode::Selector(selector_state)) => match event {
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('P' | 'K'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        selector_state.move_up();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('N' | 'J'),
                        modifiers: Modifiers::CTRL,
                    }) => {
                        selector_state.move_down();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) if selector_state.filter_term.pop().is_some() => {
                        selector_state.update_filter();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char('G' | 'C'),
                        modifiers: Modifiers::CTRL,
                    })
                    | InputEvent::Key(KeyEvent {
                        key: KeyCode::Escape,
                        ..
                    }) => {
                        self.mode = None;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c),
                        ..
                    }) => {
                        selector_state.filter_term.push(c);
                        selector_state.update_filter();
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        if let Some(entry) = selector_state
                            .filtered_entries
                            .get(selector_state.active_idx)
                            .cloned()
                        {
                            selector_state.option.value.replace(Some(entry.to_string()));
                            self.mode = None;
                        }
                    }
                    InputEvent::Resized { cols, rows } => {
                        selector_state.max_items = rows.saturating_sub(ROW_OVERHEAD);
                        self.cols_separator = "─".repeat(cols);
                        self.buf.resize(cols, rows);
                    }
                    _ => {}
                },
            }

            self.render()?;
        }

        Ok(())
    }
}

#[derive(FromDynamic, ToDynamic)]
struct TransientResultEntry {
    value: Value,
    metadata: Option<HashMap<String, Value>>,
}

#[derive(FromDynamic, ToDynamic)]
struct TransientResult {
    entries: HashMap<String, TransientResultEntry>,
}
impl_lua_conversion_dynamic!(TransientResult);

impl From<&[TransientSection<'_>]> for TransientResult {
    fn from(value: &[TransientSection<'_>]) -> Self {
        let mut entries = HashMap::new();

        for section in value {
            for entity in &section.entries {
                match entity {
                    RenderableEntity::Opt(option) => {
                        entries.insert(
                            option.delegate.flag.clone(),
                            TransientResultEntry {
                                value: option.value.borrow().to_dynamic(),
                                metadata: option.delegate.metadata.clone(),
                            },
                        );
                    }
                    RenderableEntity::Switch(switch) => {
                        entries.insert(
                            switch.delegate.flag.clone(),
                            TransientResultEntry {
                                value: switch.value.get().to_dynamic(),
                                metadata: switch.delegate.metadata.clone(),
                            },
                        );
                    }
                    RenderableEntity::CyclicSwitch(cyclic_switch) => {
                        entries.insert(
                            cyclic_switch.delegate.flag.clone(),
                            TransientResultEntry {
                                value: cyclic_switch
                                    .active_idx
                                    .get()
                                    .map(|idx| cyclic_switch.delegate.choices.get(idx).cloned())
                                    .to_dynamic(),
                                metadata: cyclic_switch.delegate.metadata.clone(),
                            },
                        );
                    }
                    _ => {}
                }
            }
        }

        Self { entries }
    }
}

fn create_keymap<'a>(
    sections: &'a [TransientSection<'a>],
    keymap: &mut KeyMap<'a, RenderableEntity<'a>>,
) {
    for section in sections {
        for entity in &section.entries {
            match entity {
                RenderableEntity::Switch(switch) => {
                    keymap.insert(&switch.delegate.key, entity);
                }
                RenderableEntity::Opt(option) => {
                    keymap.insert(&option.delegate.key, entity);
                }
                RenderableEntity::CyclicSwitch(cyclic_switch) => {
                    keymap.insert(&cyclic_switch.delegate.key, entity);
                }
                RenderableEntity::Argument(positional_arg) => {
                    keymap.insert(&positional_arg.delegate.key, entity);
                }
            }
        }
    }
}

fn create_sections<'a>(args: &'a KTransientMenu, sections: &mut Vec<TransientSection<'a>>) {
    for k_section in &args.sections {
        let mut entries = vec![];

        for k_transient_entry in &k_section.entries {
            let transient_entry = match k_transient_entry {
                KTransientEntry::TransientSwitch(switch) => {
                    RenderableEntity::Switch(TransientSwitch {
                        delegate: switch,
                        value: Cell::new(switch.default),
                    })
                }
                KTransientEntry::TransientOption(option) => {
                    RenderableEntity::Opt(TransientOption {
                        delegate: option,
                        value: RefCell::new(option.default.clone()),
                    })
                }
                KTransientEntry::TransientCyclicSwitch(cyclic_switch) => {
                    let active_idx = cyclic_switch.default.as_deref().and_then(|default| {
                        cyclic_switch
                            .choices
                            .iter()
                            .position(|choice| choice == default)
                    });
                    RenderableEntity::CyclicSwitch(TransientCyclicSwitch {
                        delegate: cyclic_switch,
                        active_idx: Cell::new(active_idx),
                    })
                }
                KTransientEntry::TransientArgument(positional_arg) => {
                    RenderableEntity::Argument(TransientArgument {
                        delegate: positional_arg,
                    })
                }
            };
            entries.push(transient_entry);
        }

        let max_key_width = entries
            .iter()
            .map(|entry| unicode_column_width(display_key(entry.key()), None))
            .max()
            .unwrap_or(0);

        sections.push(TransientSection {
            delegate: k_section,
            entries,
            max_key_width,
        });
    }
}

fn trampoline(name: String, window: GuiWin, pane: MuxPane, result: Option<TransientResult>) {
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
    result: Option<TransientResult>,
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

pub fn show_transient_menu_overlay(
    mut term: TermWizTerminal,
    args: KTransientMenu,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    term.render(&[Change::Title(args.title.clone())])?;
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let mut sections = vec![];
    create_sections(&args, &mut sections);

    let mut keymap = KeyMap::new();
    create_keymap(&sections, &mut keymap);

    let mut state = TransientState::new(&args, window, pane, &sections, &keymap, &mut buf);

    state.render()?;
    state.run_loop()
}
