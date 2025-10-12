use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{
    KeyAssignment, TransientArgument as KTransientArgument, TransientContext as KTransientContext,
    TransientContextEntry as KTransientContextEntry,
    TransientCyclicSwitch as KTransientCyclicSwitch, TransientEntry as KTransientEntry,
    TransientMenu as KTransientMenu, TransientOption as KTransientOption,
    TransientSection as KTransientSection, TransientSwitch as KTransientSwitch,
};
use config::{configuration, AnsiColor, ColorAttribute};
use luahelper::impl_lua_conversion_dynamic;
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use rayon::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use termwiz::input::{InputEvent, KeyCode, KeyEvent};
use termwiz::surface::{Change, CursorVisibility, Position, Surface};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use termwiz_funcs::truncate_right;
use wezterm_dynamic::{FromDynamic, ToDynamic, Value};
use wezterm_term::{AttributeChange, CellAttributes, Intensity};
use window::Modifiers;

const ROW_OVERHEAD: usize = 6;

struct SelectorState<'a> {
    active_idx: usize,
    max_items: usize,
    top_row: usize,
    filter_term: String,
    filtered_entries: Vec<&'a str>,
    choices: &'a Vec<String>,
    colors: &'a TransientColors,
    option: &'a TransientOption<'a>,
    row_entities: &'a Vec<Option<RenderableEntity<'a>>>,
    description: &'a str,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
}

impl SelectorState<'_> {
    fn clear_selector(&mut self) -> anyhow::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        let selector_size = self.choices.len().min(self.max_items);

        let mut line_and_selector_surface = Surface::new(cols, 2 + selector_size);
        line_and_selector_surface.add_change(Change::ClearScreen(ColorAttribute::Default));

        self.buf
            .draw_from_screen(&line_and_selector_surface, 0, rows - selector_size - 3);

        for renderable_entity in self.row_entities.iter().skip(rows - selector_size - 3) {
            if let Some(renderable_entity) = renderable_entity {
                renderable_entity.render(&self.colors, self.buf)?;
            }
        }

        self.buf
            .add_change(Change::CursorVisibility(CursorVisibility::Hidden));

        Ok(())
    }

    fn draw_separator_and_show_cursor(&mut self) {
        let (cols, rows) = self.buf.dimensions();
        let selector_size = self.choices.len().min(self.max_items);

        let mut line_surface = Surface::new(cols, 1);
        line_surface.add_changes(vec![
            Change::ClearToEndOfScreen(ColorAttribute::Default),
            Change::Text("─".repeat(cols)),
        ]);
        self.buf
            .draw_from_screen(&line_surface, 0, rows - selector_size - 3);

        self.buf
            .add_change(Change::CursorVisibility(CursorVisibility::Visible));
    }

    fn render(&mut self) -> anyhow::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        let max_width = cols.saturating_sub(6);
        let input_selector_size = self.choices.len().min(self.max_items);

        let mut selector_surface = Surface::new(cols, input_selector_size + 2);
        selector_surface.add_changes(vec![
            Change::ClearToEndOfScreen(ColorAttribute::Default),
            Change::Text(truncate_right(
                &format!("{}: {}", self.option.delegate.description, self.filter_term),
                max_width,
            )),
        ]);

        let max_items = self.max_items;

        for (row_num, (entry_idx, entry)) in self
            .filtered_entries
            .iter()
            .enumerate()
            .skip(self.top_row)
            .enumerate()
        {
            if row_num > max_items {
                break;
            }

            selector_surface.add_change(Change::Text("\r\n".to_string()));

            let mut attr = CellAttributes::blank();

            if entry_idx == self.active_idx {
                selector_surface.add_change(Change::Attribute(AttributeChange::Reverse(true)));
                attr.set_reverse(true);
            }

            selector_surface.add_change(Change::Text("    ".to_string()));
            let mut line = crate::tabbar::parse_status_text(entry, attr.clone());
            if line.len() > max_width {
                line.resize(max_width, termwiz::surface::SEQ_ZERO);
            }
            selector_surface.add_changes(line.changes(&attr));
            selector_surface.add_change(Change::Text(" ".to_string()));
            if entry_idx == self.active_idx {
                selector_surface.add_change(Change::Attribute(AttributeChange::Reverse(false)));
            }
            selector_surface.add_change(Change::AllAttributes(CellAttributes::default()));
        }

        self.buf
            .draw_from_screen(&selector_surface, 0, rows - input_selector_size - 2);

        // Adjust the cursor position because it is reset after the selector surface is drawn to
        // the buffered terminal
        self.buf.add_change(Change::CursorPosition {
            x: Position::Absolute(
                2 + self.option.delegate.description.len() + self.filter_term.len(),
            ),
            y: Position::Absolute(rows - input_selector_size - 2),
        });

        self.buf.flush()?;

        Ok(())
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            match event {
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('P' | 'K'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.move_up();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('N' | 'J'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.move_down();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    ..
                }) if self.filter_term.pop().is_some() => {
                    self.update_filter();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('G' | 'C'),
                    modifiers: Modifiers::CTRL,
                })
                | InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    ..
                }) => {
                    self.clear_selector()?;
                    break;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    ..
                }) => {
                    self.filter_term.push(c);
                    self.update_filter();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    ..
                }) => {
                    if let Some(entry) = self.filtered_entries.get(self.active_idx).cloned() {
                        self.option.value.replace(Some(entry.to_string()));
                        self.clear_selector()?;
                        break;
                    }
                }
                InputEvent::Resized { cols, rows } => {
                    self.max_items = rows.saturating_sub(ROW_OVERHEAD);

                    let description_len =
                        crate::tabbar::parse_status_text(self.description, CellAttributes::blank())
                            .len();

                    self.buf.resize(cols, rows);

                    self.buf.add_changes(vec![
                        Change::ClearScreen(ColorAttribute::Default),
                        Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(0),
                        },
                    ]);

                    let mut description_surface = Surface::new(cols, 2);
                    description_surface.add_changes(vec![
                        Change::Text(self.description.to_string()),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text("\r\n".to_string()),
                        Change::Text("─".repeat(description_len)),
                    ]);

                    self.buf.draw_from_screen(&description_surface, 0, 0);

                    for entity in self.row_entities.iter().skip(3) {
                        if let Some(entity) = entity {
                            entity.render(&self.colors, self.buf)?;
                        }
                    }

                    self.draw_separator_and_show_cursor();
                }
                _ => continue,
            }
            self.render()?;
        }

        Ok(())
    }

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
                let score = matcher_score(&pattern, &entry)?;
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
    line: String,
    colors: &'a TransientColors,
    option: &'a TransientOption<'a>,
    row_entities: &'a Vec<Option<RenderableEntity<'a>>>,
    description: &'a str,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
}

impl PromptState<'_> {
    fn render(&mut self) -> termwiz::Result<()> {
        let mut prompt_with_value = self.option.delegate.description.clone();
        if let Some(default) = self.option.delegate.default.clone() {
            prompt_with_value.push_str(&format!(" (default {})", default));
        }
        prompt_with_value.push_str(&format!(": {}", self.line));

        let (cols, rows) = self.buf.dimensions();

        let mut prompt_line_surface = Surface::new(cols, 1);
        prompt_line_surface.add_changes(vec![
            Change::ClearToEndOfLine(ColorAttribute::Default),
            Change::Text(prompt_with_value),
        ]);
        self.buf.draw_from_screen(&prompt_line_surface, 0, rows - 2);

        let (xpos, _) = prompt_line_surface.cursor_position();

        // Adjust the cursor position because it is reset after the selector surface is drawn to
        // the buffered terminal
        self.buf.add_change(Change::CursorPosition {
            x: Position::Absolute(xpos),
            y: Position::Absolute(rows - 2),
        });

        self.buf.flush()?;

        Ok(())
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            match event {
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('G' | 'C' | 'D' | '['),
                    modifiers: Modifiers::CTRL,
                })
                | InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    ..
                }) => {
                    break;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    ..
                }) => {
                    self.line.push(c);
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    ..
                }) => {
                    let new_val = if self.line.is_empty() {
                        Some(self.option.delegate.default.clone().unwrap_or_default())
                    } else {
                        Some(self.line.clone())
                    };
                    self.option.value.replace(new_val);
                    break;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    ..
                }) => {
                    if self.line.pop().is_none() {
                        continue;
                    }
                }
                InputEvent::Resized { cols, rows } => {
                    let description_len =
                        crate::tabbar::parse_status_text(self.description, CellAttributes::blank())
                            .len();

                    self.buf.resize(cols, rows);

                    self.buf.add_changes(vec![
                        Change::ClearScreen(ColorAttribute::Default),
                        Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(0),
                        },
                    ]);

                    let mut description_surface = Surface::new(cols, 2);
                    description_surface.add_changes(vec![
                        Change::Text(self.description.to_string()),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text("\r\n".to_string()),
                        Change::Text("─".repeat(description_len)),
                    ]);

                    self.buf.draw_from_screen(&description_surface, 0, 0);

                    for entity in self.row_entities.iter().skip(3) {
                        if let Some(entity) = entity {
                            entity.render(self.colors, self.buf)?;
                        }
                    }

                    self.draw_separator_and_show_cursor();
                }
                _ => {}
            }
            self.render()?;
        }

        let (cols, rows) = self.buf.dimensions();
        let mut line_and_prompt_surface = Surface::new(cols, 2);
        line_and_prompt_surface.add_change(Change::ClearScreen(ColorAttribute::Default));
        self.buf
            .draw_from_screen(&line_and_prompt_surface, 0, rows - 3);

        self.buf
            .add_change(Change::CursorVisibility(CursorVisibility::Hidden));

        Ok(())
    }

    fn draw_separator_and_show_cursor(&mut self) {
        let (cols, rows) = self.buf.dimensions();
        let mut line_surface = Surface::new(cols, 1);
        line_surface.add_changes(vec![
            Change::ClearToEndOfScreen(ColorAttribute::Default),
            Change::Text("─".repeat(cols)),
        ]);
        self.buf.draw_from_screen(&line_surface, 0, rows - 3);

        self.buf
            .add_change(Change::CursorVisibility(CursorVisibility::Visible));
    }
}

struct TrieNode<'a> {
    children: HashMap<char, Box<TrieNode<'a>>>,
    entry: Option<&'a RenderableEntity<'a>>,
}

impl<'a> TrieNode<'a> {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            entry: None,
        }
    }

    fn add_word(&mut self, word: &str, entry: &'a RenderableEntity<'_>) {
        let mut current = self;
        for ch in word.chars() {
            current = current
                .children
                .entry(ch)
                .or_insert_with(|| Box::new(TrieNode::new()));
        }
        current.entry = Some(entry);
    }

    fn find_char(&self, c: char) -> Option<&TrieNode<'_>> {
        self.children.get(&c).map(|child| child.as_ref())
    }
}

struct TransientColors {
    key_fg: ColorAttribute,
    active_flag_fg: ColorAttribute,
    inactive_flag_fg: ColorAttribute,
    active_value_fg: ColorAttribute,
}

impl TransientColors {
    fn new() -> Self {
        let config = configuration();
        let colors = &config.resolved_palette;

        Self {
            key_fg: colors
                .transient_entry_key_fg
                .unwrap_or(AnsiColor::Purple.into())
                .into(),
            active_flag_fg: colors
                .transient_entry_active_flag_fg
                .unwrap_or(AnsiColor::Red.into())
                .into(),
            inactive_flag_fg: colors
                .transient_entry_inactive_flag_fg
                .map_or_else(|| ColorAttribute::default(), |fg_color| fg_color.into()),
            active_value_fg: colors
                .transient_entry_active_value_fg
                .unwrap_or(AnsiColor::Green.into())
                .into(),
        }
    }
}

struct TransientSwitch<'a> {
    delegate: &'a KTransientSwitch,
    value: Cell<bool>,
    row: usize,
}

impl<'a> TransientSwitch<'a> {
    fn render(
        &self,
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
        render_now: bool,
    ) -> termwiz::Result<()> {
        let delegate = self.delegate;

        let (cols, _) = buf.dimensions();
        let mut switch_surface = Surface::new(cols, 1);

        switch_surface.add_changes(vec![
            Change::ClearToEndOfLine(ColorAttribute::Default),
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(format!("{}", delegate.key)),
            Change::Attribute(AttributeChange::Foreground(ColorAttribute::Default)),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if self.value.get() {
            switch_surface.add_changes(vec![
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
            ]);
        } else {
            switch_surface.add_change(Change::Attribute(AttributeChange::Foreground(
                colors.inactive_flag_fg,
            )));
        }

        switch_surface.add_changes(vec![
            Change::Text(delegate.flag.clone()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(")".to_string()),
        ]);

        buf.draw_from_screen(&switch_surface, 0, self.row);

        if render_now {
            buf.flush()?;
        }

        Ok(())
    }
}

struct TransientOption<'a> {
    delegate: &'a KTransientOption,
    value: RefCell<Option<String>>,
    row: usize,
}

impl<'a> TransientOption<'a> {
    fn render(
        &self,
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
        render_now: bool,
    ) -> termwiz::Result<()> {
        let delegate = self.delegate;

        let (cols, _) = buf.dimensions();
        let mut option_surface = Surface::new(cols, 1);

        option_surface.add_changes(vec![
            Change::ClearToEndOfLine(ColorAttribute::Default),
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(format!("{}", delegate.key)),
            Change::Attribute(AttributeChange::Foreground(ColorAttribute::Default)),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if let Some(val) = self.value.borrow().as_ref() {
            option_surface.add_changes(vec![
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Normal)),
                Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                Change::Text(val.to_string()),
            ]);
        } else {
            option_surface.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                Change::Text(format!("{}", delegate.flag)),
            ]);
        }

        option_surface.add_changes(vec![
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(")".to_string()),
        ]);

        buf.draw_from_screen(&option_surface, 0, self.row);

        if render_now {
            buf.flush()?;
        }

        Ok(())
    }
}

struct TransientCyclicSwitch<'a> {
    delegate: &'a KTransientCyclicSwitch,
    active_idx: Cell<Option<usize>>,
    row: usize,
}

impl<'a> TransientCyclicSwitch<'a> {
    fn render(
        &self,
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
        render_now: bool,
    ) -> termwiz::Result<()> {
        let delegate = self.delegate;

        let (cols, _) = buf.dimensions();
        let mut cyclic_switch_surface = Surface::new(cols, 1);

        cyclic_switch_surface.add_changes(vec![
            Change::ClearToEndOfLine(ColorAttribute::Default),
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(format!("{}", delegate.key)),
            Change::Attribute(AttributeChange::Foreground(ColorAttribute::Default)),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if let Some(idx) = self.active_idx.get() {
            cyclic_switch_surface.add_changes(vec![
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if !delegate.choices.is_empty() {
                cyclic_switch_surface.add_change(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_flag_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in delegate.choices.iter().enumerate() {
                    if cur_idx == idx {
                        cyclic_switch_surface.add_changes(vec![
                            Change::Text(prefix.to_string()),
                            Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                            Change::Text(choice.to_string()),
                            Change::AllAttributes(CellAttributes::default()),
                            Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                        ]);
                    } else {
                        cyclic_switch_surface
                            .add_change(Change::Text(format!("{}{}", prefix, choice)));
                    }
                    if cur_idx == 0 {
                        prefix = "|";
                    }
                }
                cyclic_switch_surface.add_changes(vec![
                    Change::Text("]".to_string()),
                    Change::Attribute(AttributeChange::Foreground(ColorAttribute::Default)),
                ]);
            }
        } else {
            cyclic_switch_surface.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if !delegate.choices.is_empty() {
                cyclic_switch_surface.add_change(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_flag_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in delegate.choices.iter().enumerate() {
                    cyclic_switch_surface.add_change(Change::Text(format!("{}{}", prefix, choice)));
                    if cur_idx == 0 {
                        prefix = "|";
                    }
                }
                cyclic_switch_surface.add_changes(vec![
                    Change::Text("]".to_string()),
                    Change::Attribute(AttributeChange::Foreground(ColorAttribute::Default)),
                ]);
            }
        }
        cyclic_switch_surface.add_change(Change::Text(")".to_string()));

        buf.draw_from_screen(&cyclic_switch_surface, 0, self.row);

        if render_now {
            buf.flush()?;
        }

        Ok(())
    }
}

struct TransientArgument<'a> {
    delegate: &'a KTransientArgument,
    row: usize,
}

impl<'a> TransientArgument<'a> {
    fn render(
        &self,
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> termwiz::Result<()> {
        let (cols, _) = buf.dimensions();
        let mut argument_surface = Surface::new(cols, 1);

        argument_surface.add_changes(vec![
            Change::ClearToEndOfLine(ColorAttribute::Default),
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(self.delegate.key.clone()),
            Change::Attribute(AttributeChange::Foreground(ColorAttribute::Default)),
            Change::Text(format!(" {}", self.delegate.description)),
        ]);

        buf.draw_from_screen(&argument_surface, 0, self.row);

        Ok(())
    }
}

struct TransientSection<'a> {
    delegate: &'a KTransientSection,
    row: usize,
}

impl<'a> TransientSection<'a> {
    fn render(&self, buf: &mut BufferedTerminal<TermWizTerminal>) -> termwiz::Result<()> {
        let (cols, _) = buf.dimensions();
        let mut section_surface = Surface::new(cols, 1);

        section_surface.add_changes(vec![
            Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
            Change::Attribute(AttributeChange::Foreground(AnsiColor::Navy.into())),
            Change::Text(self.delegate.header.clone()),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        buf.draw_from_screen(&section_surface, 0, self.row);

        Ok(())
    }
}

enum RenderableEntity<'a> {
    TransientContext(TransientContext<'a>),
    TransientContextEntry(TransientContextEntry<'a>),
    TransientSection(TransientSection<'a>),
    TransientOption(TransientOption<'a>),
    TransientSwitch(TransientSwitch<'a>),
    TransientArgument(TransientArgument<'a>),
    TransientCyclicSwitch(TransientCyclicSwitch<'a>),
}

impl RenderableEntity<'_> {
    fn render(
        &self,
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> termwiz::Result<()> {
        match self {
            Self::TransientOption(option) => option.render(colors, buf, false),
            Self::TransientSwitch(switch) => switch.render(colors, buf, false),
            Self::TransientCyclicSwitch(cyclic_switch) => cyclic_switch.render(colors, buf, false),
            Self::TransientArgument(positional_arg) => positional_arg.render(colors, buf),
            Self::TransientSection(section) => section.render(buf),
            Self::TransientContext(context) => context.render(buf),
            Self::TransientContextEntry(entry) => entry.render(buf),
        }
    }
}

#[derive(Clone)]
struct TransientContextEntry<'a> {
    delegate: &'a KTransientContextEntry,
    row: usize,
}

impl<'a> TransientContextEntry<'a> {
    fn render(&self, buf: &mut BufferedTerminal<TermWizTerminal>) -> termwiz::Result<()> {
        let (cols, _) = buf.dimensions();
        let mut context_entry_surface = Surface::new(cols, 1);

        context_entry_surface.add_changes(vec![
            Change::Attribute(AttributeChange::Foreground(AnsiColor::Olive.into())),
            Change::Text(self.delegate.label.clone()),
            Change::Attribute(AttributeChange::Foreground(ColorAttribute::Default)),
            Change::Text(": ".to_string()),
            Change::Text(self.delegate.id.clone()),
        ]);

        buf.draw_from_screen(&context_entry_surface, 0, self.row);

        Ok(())
    }
}

#[derive(Clone)]
struct TransientContext<'a> {
    delegate: &'a KTransientContext,
    row: usize,
}

impl<'a> TransientContext<'a> {
    fn render(&self, buf: &mut BufferedTerminal<TermWizTerminal>) -> termwiz::Result<()> {
        let (cols, _) = buf.dimensions();
        let mut context_surface = Surface::new(cols, 1);

        context_surface.add_changes(vec![
            Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
            Change::Attribute(AttributeChange::Foreground(AnsiColor::Navy.into())),
            Change::Text(self.delegate.header.clone()),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        buf.draw_from_screen(&context_surface, 0, self.row);

        Ok(())
    }
}

struct TransientState<'a> {
    window: GuiWin,
    pane: MuxPane,
    description: String,
    colors: TransientColors,
    traversed_nodes: Vec<&'a TrieNode<'a>>,
    row_entities: &'a Vec<Option<RenderableEntity<'a>>>,
    cancel: Option<Box<KeyAssignment>>,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
}

impl<'a> TransientState<'a> {
    fn new(
        args: &KTransientMenu,
        window: GuiWin,
        pane: MuxPane,
        row_entities: &'a Vec<Option<RenderableEntity<'_>>>,
        trie_node: &'a TrieNode<'_>,
        buf: &'a mut BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        buf.add_change(Change::CursorVisibility(CursorVisibility::Hidden));

        Self {
            window,
            pane,
            description: args.description.clone(),
            colors: TransientColors::new(),
            traversed_nodes: vec![trie_node],
            row_entities,
            cancel: args.cancel.clone(),
            buf,
        }
    }

    fn render(&mut self) -> termwiz::Result<()> {
        let description_len =
            crate::tabbar::parse_status_text(&self.description, CellAttributes::blank()).len();

        self.buf
            .add_change(Change::ClearScreen(ColorAttribute::Default));

        let (cols, _) = self.buf.dimensions();

        let mut description_surface = Surface::new(cols, 2);
        description_surface.add_changes(vec![
            Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
            Change::Attribute(AttributeChange::Foreground(AnsiColor::Teal.into())),
            Change::Text(self.description.clone()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("\r\n".to_string()),
            Change::Text("─".repeat(description_len)),
        ]);

        self.buf.draw_from_screen(&description_surface, 0, 0);

        for entity in self.row_entities.iter().skip(3) {
            if let Some(entity) = entity {
                entity.render(&self.colors, self.buf)?;
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

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            match event {
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
                    let cur_node = self
                        .traversed_nodes
                        .last()
                        .expect("Root node is always traversed");

                    let cur_node = match cur_node.find_char(c) {
                        Some(cur_node) => cur_node,
                        None => {
                            self.traversed_nodes.truncate(1);
                            continue;
                        }
                    };

                    let transient_entry = match cur_node.entry.as_ref() {
                        Some(entry) => entry,
                        None => {
                            self.traversed_nodes.push(cur_node);
                            continue;
                        }
                    };

                    match transient_entry {
                        RenderableEntity::TransientSwitch(switch) => {
                            switch.value.update(|val| !val);

                            switch.render(&self.colors, self.buf, true)?;
                        }
                        RenderableEntity::TransientOption(option) => {
                            if option.value.borrow().is_none() || !option.delegate.allow_nil {
                                if let Some(choices) = option.delegate.choices.as_ref() {
                                    let (_, rows) = self.buf.dimensions();
                                    let max_items = rows.saturating_sub(ROW_OVERHEAD);
                                    let filtered_entries =
                                        choices.iter().map(|choice| choice.as_str()).collect();

                                    let mut selector_state = SelectorState {
                                        active_idx: 0,
                                        max_items,
                                        top_row: 0,
                                        filter_term: String::new(),
                                        filtered_entries,
                                        choices,
                                        colors: &self.colors,
                                        option,
                                        row_entities: self.row_entities,
                                        description: &self.description,
                                        buf: self.buf,
                                    };

                                    selector_state.draw_separator_and_show_cursor();
                                    selector_state.render()?;
                                    selector_state.run_loop()?;
                                } else {
                                    let mut prompt_state = PromptState {
                                        line: String::new(),
                                        colors: &self.colors,
                                        option,
                                        row_entities: &self.row_entities,
                                        description: &self.description,
                                        buf: self.buf,
                                    };

                                    prompt_state.draw_separator_and_show_cursor();
                                    prompt_state.render()?;
                                    prompt_state.run_loop()?;
                                }
                            } else {
                                option.value.replace(None);
                            }
                            option.render(&self.colors, self.buf, true)?;
                        }
                        RenderableEntity::TransientCyclicSwitch(cyclic_switch) => {
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
                                cyclic_switch.render(&self.colors, self.buf, true)?;
                            }
                        }
                        RenderableEntity::TransientArgument(positional_arg) => {
                            let name = match *positional_arg.delegate.action {
                                KeyAssignment::EmitEvent(ref id) => id,
                                _ => anyhow::bail!("TransientMenu requires action to be defined by wezterm.action_callback")
                            };

                            let result = TransientResult::from(self.row_entities);
                            self.trigger_event(name, Some(result));
                            break;
                        }
                        _ => {}
                    }
                    self.traversed_nodes.truncate(1);
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    ..
                }) => {
                    if self.traversed_nodes.len() >= 2 {
                        self.traversed_nodes.pop();
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[derive(FromDynamic, ToDynamic)]
struct TransientResultEntry {
    flag: String,
    value: Value,
}

#[derive(FromDynamic, ToDynamic)]
struct TransientResult {
    entries: Vec<TransientResultEntry>,
}
impl_lua_conversion_dynamic!(TransientResult);

impl From<&Vec<Option<RenderableEntity<'_>>>> for TransientResult {
    fn from(value: &Vec<Option<RenderableEntity<'_>>>) -> Self {
        let mut entries: Vec<TransientResultEntry> = vec![];

        for entry in value.iter().skip(3).filter_map(|k| k.as_ref()) {
            match entry {
                RenderableEntity::TransientOption(option) => {
                    entries.push(TransientResultEntry {
                        flag: option.delegate.flag.clone(),
                        value: option.value.borrow().to_dynamic(),
                    });
                }
                RenderableEntity::TransientSwitch(switch) => {
                    entries.push(TransientResultEntry {
                        flag: switch.delegate.flag.clone(),
                        value: switch.value.get().to_dynamic(),
                    });
                }
                RenderableEntity::TransientCyclicSwitch(cyclic_switch) => {
                    entries.push(TransientResultEntry {
                        flag: cyclic_switch.delegate.flag.clone(),
                        value: cyclic_switch
                            .active_idx
                            .get()
                            .map(|idx| cyclic_switch.delegate.choices.get(idx).cloned())
                            .to_dynamic(),
                    });
                }
                _ => {}
            }
        }

        Self { entries }
    }
}

fn create_trie<'a>(
    row_entities: &'a Vec<Option<RenderableEntity<'_>>>,
    trie_node: &mut TrieNode<'a>,
) {
    for entity in row_entities.iter().filter_map(|k| k.as_ref()) {
        match entity {
            RenderableEntity::TransientSwitch(switch) => {
                trie_node.add_word(&switch.delegate.key, entity);
            }
            RenderableEntity::TransientOption(option) => {
                trie_node.add_word(&option.delegate.key, entity);
            }
            RenderableEntity::TransientCyclicSwitch(cyclic_switch) => {
                trie_node.add_word(&cyclic_switch.delegate.key, entity);
            }
            RenderableEntity::TransientArgument(positional_arg) => {
                trie_node.add_word(&positional_arg.delegate.key, entity);
            }
            _ => {}
        }
    }
}

fn create_row_entities<'a>(
    args: &'a KTransientMenu,
    row_entities: &mut Vec<Option<RenderableEntity<'a>>>,
) {
    let mut row = 2;
    if let Some(k_context) = args.context.as_ref() {
        row_entities.push(None);
        row += 1;

        let transient_context = TransientContext {
            delegate: k_context,
            row,
        };
        row_entities.push(Some(RenderableEntity::TransientContext(transient_context)));
        row += 1;

        for k_context_entry in &k_context.entries {
            let entry = TransientContextEntry {
                delegate: k_context_entry,
                row,
            };
            row_entities.push(Some(RenderableEntity::TransientContextEntry(entry)));
            row += 1;
        }
    }

    for k_section in &args.sections {
        row_entities.push(None);
        row += 1;

        let transient_section = TransientSection {
            delegate: k_section,
            row,
        };
        row_entities.push(Some(RenderableEntity::TransientSection(transient_section)));
        row += 1;

        for k_transient_entry in &k_section.entries {
            let transient_entry = match k_transient_entry {
                KTransientEntry::TransientSwitch(switch) => {
                    RenderableEntity::TransientSwitch(TransientSwitch {
                        delegate: &switch,
                        value: Cell::new(switch.default),
                        row,
                    })
                }
                KTransientEntry::TransientOption(option) => {
                    RenderableEntity::TransientOption(TransientOption {
                        delegate: &option,
                        value: RefCell::new(option.default.clone()),
                        row,
                    })
                }
                KTransientEntry::TransientCyclicSwitch(cyclic_switch) => {
                    let active_idx = cyclic_switch
                        .default
                        .as_ref()
                        .map(|default| {
                            cyclic_switch
                                .choices
                                .iter()
                                .position(|choice| choice == default)
                        })
                        .flatten();
                    RenderableEntity::TransientCyclicSwitch(TransientCyclicSwitch {
                        delegate: &cyclic_switch,
                        active_idx: Cell::new(active_idx),
                        row,
                    })
                }
                KTransientEntry::TransientArgument(positional_arg) => {
                    RenderableEntity::TransientArgument(TransientArgument {
                        delegate: &positional_arg,
                        row,
                    })
                }
            };
            row_entities.push(Some(transient_entry));
            row += 1;
        }
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
    term: TermWizTerminal,
    args: KTransientMenu,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let mut row_entities: Vec<Option<RenderableEntity>> = vec![None, None];
    create_row_entities(&args, &mut row_entities);

    let mut trie_node = TrieNode::new();
    create_trie(&row_entities, &mut trie_node);

    let mut state = TransientState::new(&args, window, pane, &row_entities, &trie_node, &mut buf);

    state.render()?;
    state.run_loop()
}
