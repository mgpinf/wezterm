use crate::overlay::common::{
    display_key, EntryRenderStyle, KeyLookup, KeyMap, LoopAction, OverlayColors,
};
use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{
    KeyAssignment, TransientAction as KTransientAction, TransientContext as KTransientContext,
    TransientEntry as KTransientEntry, TransientMenu as KTransientMenu,
    TransientOption as KTransientOption, TransientOptionInput as KTransientOptionInput,
    TransientSection as KTransientSection, TransientSwitch as KTransientSwitch,
};
use config::ColorAttribute;
use luahelper::impl_lua_conversion_dynamic;
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use termwiz::input::{InputEvent, KeyCode, KeyEvent};
use termwiz::lineedit::{LineEditBuffer, Movement};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use wezterm_dynamic::{FromDynamic, ToDynamic, Value};
use wezterm_term::{unicode_column_width, AttributeChange, CellAttributes, Intensity};
use window::Modifiers;

const ROW_OVERHEAD: usize = 6;

fn next_cycle_value(
    current: Option<&str>,
    choices: &[String],
    allow_unset: bool,
) -> Option<String> {
    let first = choices.first()?;
    match current.and_then(|value| choices.iter().position(|choice| choice == value)) {
        Some(idx) if idx == choices.len() - 1 => {
            if allow_unset {
                None
            } else {
                Some(first.clone())
            }
        }
        Some(idx) => Some(choices[idx + 1].clone()),
        None => Some(first.clone()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EntryId(usize);

struct SelectorState {
    active_idx: usize,
    max_items: usize,
    top_row: usize,
    filter_term: String,
    filtered_entries: Vec<usize>,
    option: EntryId,
}

impl SelectorState {
    fn update_filter(&mut self, choices: &[String]) {
        if self.filter_term.is_empty() {
            self.filtered_entries = (0..choices.len()).collect();
            return;
        }

        self.filtered_entries.clear();

        struct MatchResult {
            row_idx: usize,
            score: u32,
        }

        let pattern = matcher_pattern(&self.filter_term);

        let mut scores: Vec<MatchResult> = choices
            .par_iter()
            .enumerate()
            .filter_map(|(row_idx, entry)| {
                let score = matcher_score(&pattern, entry)?;
                Some(MatchResult { row_idx, score })
            })
            .collect();

        scores.sort_by(|a, b| a.score.cmp(&b.score).reverse());

        for result in scores {
            self.filtered_entries.push(result.row_idx);
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

struct PromptState {
    line: LineEditBuffer,
    option: EntryId,
}

struct TransientSwitch {
    delegate: KTransientSwitch,
    value: bool,
}

impl TransientSwitch {
    fn render(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let delegate = &self.delegate;

        let mut changes = vec![];
        changes.push(Change::Text("  ".to_string()));
        style.append_key(colors, &delegate.key, max_key_width, &mut changes);
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if self.value {
            changes.push(Change::Attribute(AttributeChange::Intensity(
                Intensity::Bold,
            )));
            changes.push(Change::Attribute(AttributeChange::Foreground(
                colors.active_argument_fg,
            )));
        } else {
            changes.push(Change::Attribute(AttributeChange::Foreground(
                colors.inactive_argument_fg,
            )));
        }

        changes.push(Change::Text(delegate.argument.clone()));
        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::Text(")".to_string()));

        style.apply(colors, &mut changes);
        buf.add_changes(changes);

        Ok(())
    }
}

struct TransientOption {
    delegate: KTransientOption,
    value: Option<String>,
}

impl TransientOption {
    fn render(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        if self.delegate.resolved_input() == KTransientOptionInput::Cycle {
            return self.render_cycle(colors, style, max_key_width, buf);
        }

        let delegate = &self.delegate;

        let mut changes = vec![];
        changes.push(Change::Text("  ".to_string()));
        style.append_key(colors, &delegate.key, max_key_width, &mut changes);
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        if let Some(val) = self.value.as_deref() {
            changes.extend([
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_argument_fg)),
                Change::Text(delegate.argument.clone()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Normal)),
                Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                Change::Text(val.to_string()),
            ]);
        } else {
            changes.extend([
                Change::Attribute(AttributeChange::Foreground(colors.inactive_argument_fg)),
                Change::Text(delegate.argument.to_string()),
            ]);
        }

        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::Text(")".to_string()));

        style.apply(colors, &mut changes);
        buf.add_changes(changes);

        Ok(())
    }

    fn render_cycle(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let delegate = &self.delegate;

        let mut changes = vec![];
        changes.push(Change::Text("  ".to_string()));
        style.append_key(colors, &delegate.key, max_key_width, &mut changes);
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {} (", delegate.description)),
        ]);

        let value = &self.value;
        if value.is_some() {
            changes.extend([
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(colors.active_argument_fg)),
                Change::Text(delegate.argument.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if let Some(choices) = delegate.choices.as_deref() {
                changes.push(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_argument_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in choices.iter().enumerate() {
                    if value.as_deref() == Some(choice.as_str()) {
                        changes.extend([
                            Change::Text(prefix.to_string()),
                            Change::Attribute(AttributeChange::Foreground(colors.active_value_fg)),
                            Change::Text(choice.to_string()),
                            Change::AllAttributes(CellAttributes::default()),
                            Change::Attribute(AttributeChange::Foreground(
                                colors.inactive_argument_fg,
                            )),
                        ]);
                    } else {
                        changes.push(Change::Text(format!("{prefix}{choice}")));
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
                Change::Attribute(AttributeChange::Foreground(colors.inactive_argument_fg)),
                Change::Text(delegate.argument.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            if let Some(choices) = delegate.choices.as_deref() {
                changes.push(Change::Attribute(AttributeChange::Foreground(
                    colors.inactive_argument_fg,
                )));
                let mut prefix = "[";
                for (cur_idx, choice) in choices.iter().enumerate() {
                    changes.push(Change::Text(format!("{prefix}{choice}")));
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

struct TransientAction {
    delegate: KTransientAction,
}

impl TransientAction {
    fn render(
        &self,
        colors: &OverlayColors,
        style: EntryRenderStyle,
        max_key_width: usize,
        buf: &mut BufferedTerminal<TermWizTerminal>,
    ) -> anyhow::Result<()> {
        let mut changes = vec![];
        changes.push(Change::Text("  ".to_string()));
        style.append_key(colors, &self.delegate.key, max_key_width, &mut changes);
        changes.extend([
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!(" {}", self.delegate.description)),
        ]);

        style.apply(colors, &mut changes);
        buf.add_changes(changes);

        Ok(())
    }
}

struct TransientSection {
    header: String,
    entries: Range<usize>,
    max_key_width: usize,
}

enum RenderableEntity {
    Opt(TransientOption),
    Switch(TransientSwitch),
    Action(TransientAction),
}

impl RenderableEntity {
    fn key(&self) -> &str {
        match self {
            Self::Opt(option) => &option.delegate.key,
            Self::Switch(switch) => &switch.delegate.key,
            Self::Action(action) => &action.delegate.key,
        }
    }

    fn argument(&self) -> Option<&str> {
        match self {
            Self::Opt(option) => Some(&option.delegate.argument),
            Self::Switch(switch) => Some(&switch.delegate.argument),
            Self::Action(_) => None,
        }
    }

    fn is_active(&self) -> bool {
        match self {
            Self::Opt(option) => option.value.is_some(),
            Self::Switch(switch) => switch.value,
            Self::Action(_) => false,
        }
    }

    fn unset(&mut self) {
        match self {
            Self::Opt(option) => option.value = None,
            Self::Switch(switch) => switch.value = false,
            Self::Action(_) => {}
        }
    }

    fn state_value(&self) -> Option<Value> {
        match self {
            Self::Opt(option) => Some(option.value.to_dynamic()),
            Self::Switch(switch) => Some(switch.value.to_dynamic()),
            Self::Action(_) => None,
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
            Self::Action(action) => action.render(colors, style, max_key_width, buf),
        }
    }
}

struct MenuModel {
    entries: Vec<RenderableEntity>,
    sections: Vec<TransientSection>,
    keymap: KeyMap<EntryId>,
    argument_index: HashMap<String, EntryId>,
    incompatible_entries: Vec<HashSet<EntryId>>,
}

impl MenuModel {
    fn new(configured_sections: Vec<KTransientSection>, incompatible: Vec<Vec<String>>) -> Self {
        let mut entries = vec![];
        let mut sections = vec![];
        let mut keymap = KeyMap::new();
        let mut argument_index = HashMap::new();

        for section in configured_sections {
            let KTransientSection {
                header,
                entries: configured_entries,
            } = section;
            let start = entries.len();

            for configured_entry in configured_entries {
                let entry = match configured_entry {
                    KTransientEntry::TransientSwitch(switch) => {
                        let value = switch.default;
                        RenderableEntity::Switch(TransientSwitch {
                            delegate: switch,
                            value,
                        })
                    }
                    KTransientEntry::TransientOption(option) => {
                        let value = option.default.clone();
                        RenderableEntity::Opt(TransientOption {
                            delegate: option,
                            value,
                        })
                    }
                    KTransientEntry::TransientAction(action) => {
                        RenderableEntity::Action(TransientAction { delegate: action })
                    }
                };
                let id = EntryId(entries.len());
                keymap.insert(entry.key(), id);
                if let Some(argument) = entry.argument() {
                    let previous = argument_index.insert(argument.to_string(), id);
                    debug_assert!(previous.is_none(), "arguments are validated as unique");
                }
                entries.push(entry);
            }

            let end = entries.len();
            let max_key_width = entries[start..end]
                .iter()
                .map(|entry| unicode_column_width(display_key(entry.key()), None))
                .max()
                .unwrap_or(0);
            sections.push(TransientSection {
                header,
                entries: start..end,
                max_key_width,
            });
        }

        let mut incompatible_entries = vec![HashSet::new(); entries.len()];
        for group in incompatible {
            let ids = group
                .iter()
                .filter_map(|argument| argument_index.get(argument).copied())
                .collect::<Vec<_>>();
            for id in &ids {
                for other in &ids {
                    if id != other {
                        incompatible_entries[id.0].insert(*other);
                    }
                }
            }
        }

        Self {
            entries,
            sections,
            keymap,
            argument_index,
            incompatible_entries,
        }
    }

    fn entry(&self, id: EntryId) -> &RenderableEntity {
        &self.entries[id.0]
    }

    fn option(&self, id: EntryId) -> &TransientOption {
        let RenderableEntity::Opt(option) = self.entry(id) else {
            unreachable!("option input mode must reference an option entry");
        };
        option
    }

    fn toggle_switch(&mut self, id: EntryId) {
        let RenderableEntity::Switch(switch) = &mut self.entries[id.0] else {
            unreachable!("toggle_switch must reference a switch entry");
        };
        switch.value = !switch.value;
        if switch.value {
            self.unset_incompatible(id);
        }
    }

    fn set_option_value(&mut self, id: EntryId, value: Option<String>) {
        let is_active = value.is_some();
        let RenderableEntity::Opt(option) = &mut self.entries[id.0] else {
            unreachable!("set_option_value must reference an option entry");
        };
        option.value = value;
        if is_active {
            self.unset_incompatible(id);
        }
    }

    fn unset(&mut self, id: EntryId) {
        self.entries[id.0].unset();
    }

    fn result(&self) -> TransientResult {
        let mut result = HashMap::new();

        for (argument, id) in &self.argument_index {
            if let Some(state_value) = self.entry(*id).state_value() {
                result.insert(argument.clone(), state_value);
            }
        }

        TransientResult { entries: result }
    }

    fn unset_incompatible(&mut self, id: EntryId) {
        let entries = &mut self.entries;
        for incompatible in &self.incompatible_entries[id.0] {
            entries[incompatible.0].unset();
        }
    }
}

enum InputMode {
    Prompt(PromptState),
    Selector(SelectorState),
}

struct TransientState {
    window: GuiWin,
    pane: MuxPane,
    description: String,
    colors: OverlayColors,
    model: MenuModel,
    typed: String,
    cancel: Option<Box<KeyAssignment>>,
    buf: BufferedTerminal<TermWizTerminal>,
    context: Option<KTransientContext>,
    mode: Option<InputMode>,
    description_separator: String,
    cols_separator: String,
}

impl TransientState {
    fn new(
        args: KTransientMenu,
        window: GuiWin,
        pane: MuxPane,
        buf: BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        let KTransientMenu {
            description,
            context,
            sections,
            incompatible,
            cancel,
            ..
        } = args;

        let description_len =
            crate::tabbar::parse_status_text(&description, CellAttributes::blank()).len();
        let description_separator = "─".repeat(description_len);

        let (cols, _) = buf.dimensions();
        let cols_separator = "─".repeat(cols);
        let model = MenuModel::new(sections, incompatible);

        Self {
            window,
            pane,
            description,
            colors: OverlayColors::new(),
            model,
            typed: String::new(),
            cancel,
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

        if let Some(context) = self.context.as_ref() {
            let mut changes = vec![];
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
                    Change::Text(format!(": {}", entry.id)),
                ]);
            }

            self.buf.add_changes(changes);
        }

        for section in &self.model.sections {
            self.buf.add_changes(vec![
                Change::Text("\r\n\r\n".to_string()),
                Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                Change::Attribute(AttributeChange::Foreground(self.colors.section_header_fg)),
                Change::Text(section.header.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
            for entry_idx in section.entries.clone() {
                self.buf.add_change(Change::Text("\r\n".to_string()));
                let entity = &self.model.entries[entry_idx];
                entity.render(
                    &self.colors,
                    &self.typed,
                    section.max_key_width,
                    &mut self.buf,
                )?;
            }
        }

        if let Some(input_mode) = self.mode.as_ref() {
            match input_mode {
                InputMode::Prompt(prompt_state) => {
                    let (_, rows) = self.buf.dimensions();
                    let option = self.model.option(prompt_state.option);

                    self.buf.add_changes(vec![
                        Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(rows.saturating_sub(3)),
                        },
                        Change::ClearToEndOfScreen(ColorAttribute::Default),
                        Change::Attribute(AttributeChange::Foreground(self.colors.separator_fg)),
                        Change::Text(self.cols_separator.clone()),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text("\r\n".to_string()),
                        Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                        Change::Attribute(AttributeChange::Foreground(self.colors.prompt_label_fg)),
                        Change::Text(option.delegate.description.clone()),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);

                    let mut cursor_x =
                        option.delegate.description.len() + 2 + prompt_state.line.get_cursor();

                    if let Some(default) = option.delegate.default.as_deref() {
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
                        Change::Text(format!(": {}", prompt_state.line.get_line())),
                        Change::CursorVisibility(CursorVisibility::Visible),
                        Change::CursorPosition {
                            x: Position::Absolute(cursor_x),
                            y: Position::Absolute(rows.saturating_sub(2)),
                        },
                    ]);
                }
                InputMode::Selector(selector_state) => {
                    let (cols, rows) = self.buf.dimensions();
                    let max_width = cols.saturating_sub(6);
                    let option = self.model.option(selector_state.option);
                    let choices = option
                        .delegate
                        .choices
                        .as_deref()
                        .expect("selector input mode requires choices");

                    let selector_size = choices.len().min(selector_state.max_items);
                    let mut changes = vec![];
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
                        Change::Text(option.delegate.description.clone()),
                        Change::AllAttributes(CellAttributes::default()),
                        Change::Text(format!(": {}", selector_state.filter_term)),
                    ]);

                    for (row_num, (entry_idx, choice_idx)) in selector_state
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
                        let entry = &choices[*choice_idx];
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
                                2 + option.delegate.description.len()
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

        match self.model.keymap.lookup(&self.typed) {
            KeyLookup::Found(id) => {
                if matches!(self.model.entry(id), RenderableEntity::Switch(_)) {
                    self.model.toggle_switch(id);
                } else if matches!(self.model.entry(id), RenderableEntity::Opt(_)) {
                    let (input, allow_unset, is_active) = {
                        let option = self.model.option(id);
                        (
                            option.delegate.resolved_input(),
                            option.delegate.allow_unset,
                            option.value.is_some(),
                        )
                    };

                    match input {
                        KTransientOptionInput::Cycle => {
                            let next_value = {
                                let option = self.model.option(id);
                                let choices =
                                    option.delegate.choices.as_deref().ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "TransientOption with input='cycle' requires choices"
                                        )
                                    })?;
                                next_cycle_value(
                                    option.value.as_deref(),
                                    choices,
                                    option.delegate.allow_unset,
                                )
                            };
                            self.model.set_option_value(id, next_value);
                        }
                        input => {
                            if !is_active || !allow_unset {
                                self.mode = match input {
                                    KTransientOptionInput::Select => {
                                        let choice_count = self
                                            .model
                                            .option(id)
                                            .delegate
                                            .choices
                                            .as_ref()
                                            .ok_or_else(|| {
                                                anyhow::anyhow!(
                                                    "TransientOption with input='select' requires choices"
                                                )
                                            })?
                                            .len();
                                        let (_, rows) = self.buf.dimensions();
                                        let max_items = rows.saturating_sub(ROW_OVERHEAD);

                                        Some(InputMode::Selector(SelectorState {
                                            active_idx: 0,
                                            max_items,
                                            top_row: 0,
                                            filter_term: String::new(),
                                            filtered_entries: (0..choice_count).collect(),
                                            option: id,
                                        }))
                                    }
                                    KTransientOptionInput::Prompt => {
                                        Some(InputMode::Prompt(PromptState {
                                            line: LineEditBuffer::default(),
                                            option: id,
                                        }))
                                    }
                                    KTransientOptionInput::Cycle => unreachable!(),
                                };
                            } else {
                                self.model.unset(id);
                            }
                        }
                    }
                } else {
                    let (name, keep_overlay) = {
                        let RenderableEntity::Action(action) = self.model.entry(id) else {
                            unreachable!("keymap entry must reference a renderable entity");
                        };
                        let name = match *action.delegate.action {
                            KeyAssignment::EmitEvent(ref id) => id.clone(),
                            _ => anyhow::bail!(
                                "TransientMenu requires action to be defined by wezterm.action_callback"
                            ),
                        };
                        (name, action.delegate.keep_overlay)
                    };

                    let result = self.model.result();
                    self.trigger_event(&name, Some(result));
                    if !keep_overlay {
                        return Ok(LoopAction::Break);
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
                                self.model
                                    .option(prompt_state.option)
                                    .delegate
                                    .default
                                    .clone()
                                    .unwrap_or_default(),
                            )
                        } else {
                            Some(line.to_string())
                        };
                        self.model.set_option_value(prompt_state.option, new_val);
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
                        let choices = self
                            .model
                            .option(selector_state.option)
                            .delegate
                            .choices
                            .as_deref()
                            .expect("selector input mode requires choices");
                        selector_state.update_filter(choices);
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
                        let choices = self
                            .model
                            .option(selector_state.option)
                            .delegate
                            .choices
                            .as_deref()
                            .expect("selector input mode requires choices");
                        selector_state.update_filter(choices);
                    }
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Enter,
                        ..
                    }) => {
                        if let Some(choice_idx) = selector_state
                            .filtered_entries
                            .get(selector_state.active_idx)
                            .copied()
                        {
                            let value = self
                                .model
                                .option(selector_state.option)
                                .delegate
                                .choices
                                .as_ref()
                                .expect("selector input mode requires choices")[choice_idx]
                                .clone();
                            self.model
                                .set_option_value(selector_state.option, Some(value));
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

struct TransientResult {
    entries: HashMap<String, Value>,
}

impl ToDynamic for TransientResult {
    fn to_dynamic(&self) -> Value {
        self.entries.to_dynamic()
    }
}

impl FromDynamic for TransientResult {
    fn from_dynamic(
        value: &Value,
        options: wezterm_dynamic::FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        Ok(Self {
            entries: HashMap::from_dynamic(value, options)?,
        })
    }
}

impl_lua_conversion_dynamic!(TransientResult);

#[cfg(test)]
mod test {
    use super::*;

    fn switch(key: &str, argument: &str, default: bool) -> KTransientEntry {
        KTransientEntry::TransientSwitch(KTransientSwitch {
            key: key.to_string(),
            default,
            description: argument.to_string(),
            argument: argument.to_string(),
        })
    }

    fn option(key: &str, argument: &str, default: Option<&str>) -> KTransientEntry {
        KTransientEntry::TransientOption(KTransientOption {
            key: key.to_string(),
            default: default.map(str::to_string),
            description: argument.to_string(),
            argument: argument.to_string(),
            allow_unset: false,
            choices: None,
            input: Some(KTransientOptionInput::Prompt),
        })
    }

    fn menu_model(entries: Vec<KTransientEntry>, incompatible: Vec<Vec<String>>) -> MenuModel {
        MenuModel::new(
            vec![KTransientSection {
                header: "Arguments".to_string(),
                entries,
            }],
            incompatible,
        )
    }

    #[test]
    fn cycle_option_advances_wraps_and_unsets() {
        let choices = vec!["topological".to_string(), "date".to_string()];

        assert_eq!(
            next_cycle_value(None, &choices, true),
            Some("topological".to_string())
        );
        assert_eq!(
            next_cycle_value(Some("topological"), &choices, true),
            Some("date".to_string())
        );
        assert_eq!(next_cycle_value(Some("date"), &choices, true), None);
        assert_eq!(
            next_cycle_value(Some("date"), &choices, false),
            Some("topological".to_string())
        );
        assert_eq!(
            next_cycle_value(Some("unknown"), &choices, true),
            Some("topological".to_string())
        );
        assert_eq!(next_cycle_value(None, &[], true), None);
    }

    #[test]
    fn option_selector_filters_choices_and_resets_position() {
        let choices = vec![
            "topological".to_string(),
            "date".to_string(),
            "author-date".to_string(),
        ];
        let mut state = SelectorState {
            active_idx: 2,
            max_items: 1,
            top_row: 1,
            filter_term: "author".to_string(),
            filtered_entries: vec![],
            option: EntryId(0),
        };

        state.update_filter(&choices);

        assert_eq!(state.filtered_entries, vec![2]);
        assert_eq!(state.active_idx, 0);
        assert_eq!(state.top_row, 0);

        state.filter_term.clear();
        state.update_filter(&choices);
        assert_eq!(state.filtered_entries, vec![0, 1, 2]);
    }

    #[test]
    fn transient_result_converts_to_the_entries_map() {
        let entries = HashMap::from([("--follow".to_string(), true.to_dynamic())]);
        let expected = entries.to_dynamic();

        assert_eq!(TransientResult { entries }.to_dynamic(), expected);
    }

    #[test]
    fn transient_result_contains_switch_and_option_values() {
        let mut model = menu_model(
            vec![
                switch("f", "--follow", true),
                option("t", "--tail=", Some("100")),
            ],
            vec![],
        );

        assert_eq!(model.sections[0].entries, 0..2);
        assert!(matches!(
            model.keymap.lookup("f"),
            KeyLookup::Found(EntryId(0))
        ));
        assert_eq!(model.argument_index["--follow"], EntryId(0));
        assert_eq!(model.argument_index["--tail="], EntryId(1));
        assert!(model.entry(EntryId(0)).is_active());
        assert!(model.entry(EntryId(1)).is_active());

        let result = model.result();

        assert_eq!(result.entries.get("--follow"), Some(&Value::Bool(true)));
        assert_eq!(
            result.entries.get("--tail="),
            Some(&Value::String("100".to_string()))
        );

        model.unset(EntryId(0));
        model.unset(EntryId(1));

        assert!(!model.entry(EntryId(0)).is_active());
        assert!(!model.entry(EntryId(1)).is_active());

        let result = model.result();

        assert_eq!(result.entries.get("--follow"), Some(&Value::Bool(false)));
        assert_eq!(result.entries.get("--tail="), Some(&Value::Null));
    }

    #[test]
    fn incompatible_argument_groups_are_symmetric_and_merge() {
        let groups = vec![
            vec!["--all".to_string(), "--author=".to_string()],
            vec!["--author=".to_string(), "--committer=".to_string()],
            vec!["--all".to_string(), "--author=".to_string()],
        ];
        let model = menu_model(
            vec![
                switch("a", "--all", false),
                option("u", "--author=", None),
                option("c", "--committer=", None),
            ],
            groups,
        );

        assert_eq!(model.incompatible_entries[0], HashSet::from([EntryId(1)]));
        assert_eq!(
            model.incompatible_entries[1],
            HashSet::from([EntryId(0), EntryId(2)])
        );
        assert_eq!(model.incompatible_entries[2], HashSet::from([EntryId(1)]));
    }

    #[test]
    fn menu_model_applies_incompatibility_to_switches_and_options() {
        let groups = vec![vec!["--all".to_string(), "--author=".to_string()]];
        let mut model = menu_model(
            vec![
                switch("a", "--all", false),
                option("u", "--author=", Some("Ada")),
                switch("p", "--patch", true),
            ],
            groups,
        );

        model.toggle_switch(EntryId(0));

        assert!(model.entry(EntryId(0)).is_active());
        assert!(!model.entry(EntryId(1)).is_active());
        assert!(model.entry(EntryId(2)).is_active());

        model.toggle_switch(EntryId(0));

        assert!(!model.entry(EntryId(0)).is_active());
        assert!(!model.entry(EntryId(1)).is_active());

        model.set_option_value(EntryId(1), Some("Ada".to_string()));

        assert!(!model.entry(EntryId(0)).is_active());
        assert!(model.entry(EntryId(1)).is_active());

        model.set_option_value(EntryId(1), None);

        assert!(!model.entry(EntryId(0)).is_active());
        assert!(!model.entry(EntryId(1)).is_active());

        model.toggle_switch(EntryId(0));

        model.set_option_value(EntryId(1), Some("Grace".to_string()));

        assert!(!model.entry(EntryId(0)).is_active());
        assert!(model.entry(EntryId(1)).is_active());
        assert!(model.entry(EntryId(2)).is_active());
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

    let mut state = TransientState::new(args, window, pane, buf);

    state.render()?;
    state.run_loop()
}
