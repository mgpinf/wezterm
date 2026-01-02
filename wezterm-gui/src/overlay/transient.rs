use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{
    KeyAssignment, TransientArgument as KTransientArgument, TransientContext as KTransientContext,
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
    option: &'a TransientOption<'a>,
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
    description_fg: ColorAttribute,
    context_label_fg: ColorAttribute,
    context_header_fg: ColorAttribute,
    section_header_fg: ColorAttribute,
    separator_fg: ColorAttribute,
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
            description_fg: colors
                .transient_description_fg
                .unwrap_or(AnsiColor::Teal.into())
                .into(),
            context_label_fg: colors
                .transient_context_label_fg
                .unwrap_or(AnsiColor::Olive.into())
                .into(),
            context_header_fg: colors
                .transient_context_header_fg
                .unwrap_or(AnsiColor::Navy.into())
                .into(),
            section_header_fg: colors
                .transient_section_header_fg
                .unwrap_or(AnsiColor::Navy.into())
                .into(),
            separator_fg: colors
                .transient_separator_fg
                .map_or_else(|| ColorAttribute::Default, |fg_color| fg_color.into()),
        }
    }
}

struct TransientSwitch<'a> {
    delegate: &'a KTransientSwitch,
    value: Cell<bool>,
}

impl<'a> TransientSwitch<'a> {
    fn render(
        &self,
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> termwiz::Result<()> {
        let delegate = self.delegate;

        buf.add_changes(vec![
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(format!("{}", delegate.key)),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if self.value.get() {
            buf.add_changes(vec![
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
            ]);
        } else {
            buf.add_change(Change::Attribute(AttributeChange::Foreground(
                colors.inactive_flag_fg,
            )));
        }

        buf.add_changes(vec![
            Change::Text(delegate.flag.clone()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(")".to_string()),
        ]);

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
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> termwiz::Result<()> {
        let delegate = self.delegate;

        buf.add_changes(vec![
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(format!("{}", delegate.key)),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if let Some(val) = self.value.borrow().as_ref() {
            buf.add_changes(vec![
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Normal)),
                Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                Change::Text(val.to_string()),
            ]);
        } else {
            buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                Change::Text(format!("{}", delegate.flag)),
            ]);
        }

        buf.add_changes(vec![
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(")".to_string()),
        ]);

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
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> termwiz::Result<()> {
        let delegate = self.delegate;

        buf.add_changes(vec![
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(format!("{}", delegate.key)),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if let Some(idx) = self.active_idx.get() {
            buf.add_changes(vec![
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if !delegate.choices.is_empty() {
                buf.add_change(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_flag_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in delegate.choices.iter().enumerate() {
                    if cur_idx == idx {
                        buf.add_changes(vec![
                            Change::Text(prefix.to_string()),
                            Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                            Change::Text(choice.to_string()),
                            Change::AllAttributes(CellAttributes::default()),
                            Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                        ]);
                    } else {
                        buf.add_change(Change::Text(format!("{}{}", prefix, choice)));
                    }
                    if cur_idx == 0 {
                        prefix = "|";
                    }
                }
                buf.add_changes(vec![
                    Change::Text("]".to_string()),
                    Change::AllAttributes(CellAttributes::default()),
                ]);
            }
        } else {
            buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(colors.inactive_flag_fg)),
                Change::Text(delegate.flag.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if !delegate.choices.is_empty() {
                buf.add_change(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_flag_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in delegate.choices.iter().enumerate() {
                    buf.add_change(Change::Text(format!("{}{}", prefix, choice)));
                    if cur_idx == 0 {
                        prefix = "|";
                    }
                }
                buf.add_changes(vec![
                    Change::Text("]".to_string()),
                    Change::AllAttributes(CellAttributes::default()),
                ]);
            }
        }
        buf.add_change(Change::Text(")".to_string()));

        Ok(())
    }
}

struct TransientArgument<'a> {
    delegate: &'a KTransientArgument,
}

impl<'a> TransientArgument<'a> {
    fn render(
        &self,
        colors: &TransientColors,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> termwiz::Result<()> {
        buf.add_changes(vec![
            Change::Text("  ".to_string()),
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(self.delegate.key.clone()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {}", self.delegate.description)),
        ]);

        Ok(())
    }
}

struct TransientSection<'a> {
    delegate: &'a KTransientSection,
    entries: Vec<RenderableEntity<'a>>,
}

enum RenderableEntity<'a> {
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
            Self::TransientOption(option) => option.render(colors, buf),
            Self::TransientSwitch(switch) => switch.render(colors, buf),
            Self::TransientCyclicSwitch(cyclic_switch) => cyclic_switch.render(colors, buf),
            Self::TransientArgument(positional_arg) => positional_arg.render(colors, buf),
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
    colors: TransientColors,
    traversed_nodes: Vec<&'a TrieNode<'a>>,
    sections: &'a Vec<TransientSection<'a>>,
    cancel: Option<Box<KeyAssignment>>,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
    context: Option<&'a KTransientContext>,
    mode: Option<InputMode<'a>>,
}

impl<'a> TransientState<'a> {
    fn new(
        args: &'a KTransientMenu,
        window: GuiWin,
        pane: MuxPane,
        sections: &'a Vec<TransientSection<'_>>,
        trie_node: &'a TrieNode<'_>,
        buf: &'a mut BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        let context = args.context.as_ref();

        Self {
            window,
            pane,
            description: args.description.clone(),
            colors: TransientColors::new(),
            traversed_nodes: vec![trie_node],
            sections,
            cancel: args.cancel.clone(),
            buf,
            context,
            mode: None,
        }
    }

    fn render(&mut self) -> termwiz::Result<()> {
        let description_len =
            crate::tabbar::parse_status_text(&self.description, CellAttributes::blank()).len();

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
            Change::Text("─".repeat(description_len)),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        if let Some(context) = self.context {
            self.buf.add_changes(vec![
                Change::Text("\r\n\r\n".to_string()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(self.colors.context_header_fg)),
                Change::Text(context.header.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);

            for entry in &context.entries {
                self.buf.add_changes(vec![
                    Change::Text("\r\n".to_string()),
                    Change::Attribute(AttributeChange::Foreground(self.colors.context_label_fg)),
                    Change::Text(entry.label.clone()),
                    Change::AllAttributes(CellAttributes::default()),
                    Change::Text(format!(": {}", entry.id)),
                ]);
            }
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
                entity.render(&self.colors, self.buf)?;
            }
        }

        if let Some(input_mode) = self.mode.as_ref() {
            match input_mode {
                InputMode::Prompt(prompt_state) => {
                    let (cols, rows) = self.buf.dimensions();

                    let mut prompt_surface = Surface::new(cols, 3);
                    prompt_surface.add_changes(vec![
                        Change::ClearScreen(ColorAttribute::Default),
                        Change::Attribute(AttributeChange::Foreground(self.colors.separator_fg)),
                        Change::Text("─".repeat(cols)),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text("\r\n".to_string()),
                        Change::Text(prompt_state.option.delegate.description.clone()),
                    ]);

                    if let Some(default) = prompt_state.option.delegate.default.as_ref() {
                        prompt_surface.add_change(Change::Text(format!(" (default {})", default)));
                    }

                    prompt_surface.add_change(Change::Text(format!(": {}", prompt_state.line)));
                    self.buf.draw_from_screen(&prompt_surface, 0, rows - 3);

                    let (xpos, _) = prompt_surface.cursor_position();

                    // Adjust the cursor position because it is reset after the selector surface is drawn to
                    // the buffered terminal
                    self.buf.add_changes(vec![
                        Change::CursorVisibility(CursorVisibility::Visible),
                        Change::CursorPosition {
                            x: Position::Absolute(xpos),
                            y: Position::Absolute(rows - 2),
                        },
                    ]);
                }
                InputMode::Selector(selector_state) => {
                    let (cols, rows) = self.buf.dimensions();
                    let max_width = cols.saturating_sub(6);

                    let selector_size = selector_state.choices.len().min(selector_state.max_items);

                    let mut selector_surface = Surface::new(cols, selector_size + 3);
                    selector_surface.add_changes(vec![
                        Change::ClearToEndOfScreen(ColorAttribute::Default),
                        Change::Attribute(AttributeChange::Foreground(self.colors.separator_fg)),
                        Change::Text("─".repeat(cols)),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text(truncate_right(
                            &format!(
                                "\r\n{}: {}",
                                selector_state.option.delegate.description,
                                selector_state.filter_term
                            ),
                            max_width,
                        )),
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

                        selector_surface.add_change(Change::Text("\r\n".to_string()));

                        let mut attr = CellAttributes::blank();

                        if entry_idx == selector_state.active_idx {
                            selector_surface
                                .add_change(Change::Attribute(AttributeChange::Reverse(true)));
                            attr.set_reverse(true);
                        }

                        selector_surface.add_change(Change::Text("    ".to_string()));
                        let mut line = crate::tabbar::parse_status_text(entry, attr.clone());
                        if line.len() > max_width {
                            line.resize(max_width, termwiz::surface::SEQ_ZERO);
                        }
                        selector_surface.add_changes(line.changes(&attr));
                        selector_surface.add_change(Change::Text(" ".to_string()));
                        if entry_idx == selector_state.active_idx {
                            selector_surface
                                .add_change(Change::Attribute(AttributeChange::Reverse(false)));
                        }
                        selector_surface
                            .add_change(Change::AllAttributes(CellAttributes::default()));
                    }

                    self.buf
                        .draw_from_screen(&selector_surface, 0, rows - selector_size - 3);

                    // Adjust the cursor position because it is reset after the selector surface is drawn to
                    // the buffered terminal
                    self.buf.add_changes(vec![
                        Change::CursorVisibility(CursorVisibility::Visible),
                        Change::CursorPosition {
                            x: Position::Absolute(
                                2 + selector_state.option.delegate.description.len()
                                    + selector_state.filter_term.len(),
                            ),
                            y: Position::Absolute(rows - selector_size - 2),
                        },
                    ]);
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
                        let cur_node = self.traversed_nodes[self.traversed_nodes.len() - 1];

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
                            }
                            RenderableEntity::TransientOption(option) => {
                                if option.value.borrow().is_none() || !option.delegate.allow_nil {
                                    self.mode = if let Some(choices) =
                                        option.delegate.choices.as_ref()
                                    {
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
                                            line: String::new(),
                                            option,
                                        }))
                                    }
                                } else {
                                    option.value.replace(None);
                                }
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
                                }
                            }
                            RenderableEntity::TransientArgument(positional_arg) => {
                                let name = match *positional_arg.delegate.action {
                                KeyAssignment::EmitEvent(ref id) => id,
                                _ => anyhow::bail!("TransientMenu requires action to be defined by wezterm.action_callback")
                            };

                                let result = TransientResult::from(self.sections);
                                self.trigger_event(name, Some(result));
                                break;
                            }
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
                        key: KeyCode::Char(c),
                        ..
                    }) => {
                        prompt_state.line.push(c);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        let new_val = if prompt_state.line.is_empty() {
                            Some(
                                prompt_state
                                    .option
                                    .delegate
                                    .default
                                    .clone()
                                    .unwrap_or_default(),
                            )
                        } else {
                            Some(prompt_state.line.clone())
                        };
                        prompt_state.option.value.replace(new_val);
                        self.mode = None;
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Backspace,
                        ..
                    }) => {
                        if prompt_state.line.pop().is_none() {
                            continue;
                        }
                    }
                    InputEvent::Resized { cols, rows } => {
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
    flag: String,
    value: Value,
    #[dynamic(default)]
    tag: Option<String>,
}

#[derive(FromDynamic, ToDynamic)]
struct TransientResult {
    entries: Vec<TransientResultEntry>,
}
impl_lua_conversion_dynamic!(TransientResult);

impl From<&Vec<TransientSection<'_>>> for TransientResult {
    fn from(value: &Vec<TransientSection<'_>>) -> Self {
        let mut entries: Vec<TransientResultEntry> = vec![];

        for section in value {
            for entity in &section.entries {
                match entity {
                    RenderableEntity::TransientOption(option) => {
                        entries.push(TransientResultEntry {
                            flag: option.delegate.flag.clone(),
                            value: option.value.borrow().to_dynamic(),
                            tag: option.delegate.tag.clone(),
                        });
                    }
                    RenderableEntity::TransientSwitch(switch) => {
                        entries.push(TransientResultEntry {
                            flag: switch.delegate.flag.clone(),
                            value: switch.value.get().to_dynamic(),
                            tag: switch.delegate.tag.clone(),
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
                            tag: cyclic_switch.delegate.tag.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        Self { entries }
    }
}

fn create_trie<'a>(sections: &'a Vec<TransientSection<'_>>, trie_node: &mut TrieNode<'a>) {
    for section in sections {
        for entity in &section.entries {
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
                    RenderableEntity::TransientSwitch(TransientSwitch {
                        delegate: &switch,
                        value: Cell::new(switch.default),
                    })
                }
                KTransientEntry::TransientOption(option) => {
                    RenderableEntity::TransientOption(TransientOption {
                        delegate: &option,
                        value: RefCell::new(option.default.clone()),
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
                    })
                }
                KTransientEntry::TransientArgument(positional_arg) => {
                    RenderableEntity::TransientArgument(TransientArgument {
                        delegate: &positional_arg,
                    })
                }
            };
            entries.push(transient_entry);
        }

        sections.push(TransientSection {
            delegate: k_section,
            entries,
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
    term: TermWizTerminal,
    args: KTransientMenu,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let mut sections = vec![];
    create_sections(&args, &mut sections);

    let mut trie_node = TrieNode::new();
    create_trie(&sections, &mut trie_node);

    let mut state = TransientState::new(&args, window, pane, &sections, &trie_node, &mut buf);

    state.render()?;
    state.run_loop()
}
