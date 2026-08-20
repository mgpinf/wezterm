use crate::overlay::common::{
    display_key, EntryRenderStyle, KeyLookup, KeyMap, LoopAction, OverlayColors,
};
use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{
    KeyAssignment, SelectorActionSection, SelectorActions, SelectorActionsEntry, TransientAction,
    TransientContext,
};
use config::ColorAttribute;
use luahelper::impl_lua_conversion_dynamic;
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use rayon::prelude::*;
use std::rc::Rc;
use termwiz::input::{InputEvent, KeyCode, KeyEvent};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use termwiz_funcs::truncate_right;
use wezterm_dynamic::{FromDynamic, ToDynamic, Value};
use wezterm_term::unicode_column_width;
use wezterm_term::{AttributeChange, CellAttributes, Intensity};
use window::{Clipboard, Modifiers, WindowOps};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ActionId(usize);

struct SelectorModel {
    choices: Vec<SelectorActionsEntry>,
    actions: Vec<TransientAction>,
    keymap: KeyMap<ActionId>,
    context: Option<TransientContext>,
    header: String,
    max_key_width: usize,
}

impl SelectorModel {
    fn new(
        choices: Vec<SelectorActionsEntry>,
        section: SelectorActionSection,
        context: Option<TransientContext>,
    ) -> Self {
        let actions = section.actions;
        let max_key_width = actions
            .iter()
            .map(|action| unicode_column_width(display_key(&action.key), None))
            .max()
            .unwrap_or(0);

        Self {
            keymap: create_keymap(&actions),
            choices,
            actions,
            context,
            header: section.header.unwrap_or_else(|| "Default".to_string()),
            max_key_width,
        }
    }
}

struct SelectorState {
    active_idx: usize,
    max_items: usize,
    top_row: usize,
    multiple_idx: Option<Vec<bool>>,
    filtered_entries: Vec<usize>,
    filtering: bool,
    filter_term: String,
    description: String,
    fuzzy_description: String,
    window: GuiWin,
    pane: MuxPane,
    typed: String,
    colors: OverlayColors,
    model: SelectorModel,
    cancel: Option<Box<KeyAssignment>>,
    repeat: [u8; 2],
    buf: BufferedTerminal<TermWizTerminal>,
    separator_line: String,
}

fn selected_ids(
    choices: &[SelectorActionsEntry],
    filtered_entries: &[usize],
    active_idx: usize,
    multiple_idx: Option<&[bool]>,
) -> Vec<String> {
    let mut ids = vec![];

    if let Some(multiple_idx) = multiple_idx {
        ids.extend(
            choices
                .iter()
                .zip(multiple_idx)
                .filter(|(_, selected)| **selected)
                .map(|(entry, _)| entry.id.clone()),
        );
    }

    if ids.is_empty() {
        if let Some(choice_idx) = filtered_entries.get(active_idx) {
            ids.push(choices[*choice_idx].id.clone());
        }
    }

    ids
}

impl SelectorState {
    fn new(
        args: SelectorActions,
        window: GuiWin,
        pane: MuxPane,
        buf: BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        let SelectorActions {
            description,
            context,
            choices,
            section,
            multiple,
            fuzzy_description,
            fuzzy,
            cancel,
            ..
        } = args;

        let model = SelectorModel::new(choices, section, context);
        let context_size = model.context.as_ref().map_or(0, |v| v.entries.len() + 2);
        let actions_size = model.actions.len() + 1;
        let overhead = context_size + actions_size + 3;

        let (_, rows) = buf.dimensions();
        let max_items = rows.saturating_sub(overhead);

        let multiple_idx = multiple.then(|| vec![false; model.choices.len()]);
        let filtered_entries = (0..model.choices.len()).collect();

        let fuzzy_description = fuzzy_description.unwrap_or_else(|| description.clone());

        let (cols, _) = buf.dimensions();
        let separator_line = "─".repeat(cols);

        SelectorState {
            active_idx: 0,
            max_items,
            top_row: 0,
            multiple_idx,
            filtered_entries,
            filtering: fuzzy,
            filter_term: String::new(),
            description,
            fuzzy_description,
            window,
            pane,
            typed: String::new(),
            colors: OverlayColors::new(),
            model,
            cancel,
            repeat: [1, 1],
            buf,
            separator_line,
        }
    }

    fn move_up(&mut self) {
        self.active_idx = self.active_idx.saturating_sub(self.repeat[0] as usize);
        if self.active_idx < self.top_row {
            self.top_row = self.active_idx;
        }
    }

    fn move_down(&mut self) {
        self.active_idx =
            (self.active_idx + self.repeat[0] as usize).min(self.filtered_entries.len() - 1);
        if self.active_idx > self.top_row + self.max_items {
            self.top_row = self.active_idx.saturating_sub(self.max_items);
        }
    }

    fn toggle_multiple_marker(&mut self, down: bool) {
        // start_idx and end_idx are guaranteed to be within bounds of filtered_entries if
        // filtered_entries is not empty
        if !self.filtered_entries.is_empty() {
            if let Some(multiple_idx) = self.multiple_idx.as_mut() {
                // self.repeat[0] is guaranteed to be at least 1, so we can subtract 1 from it
                let (start_idx, end_idx) = if down {
                    (
                        self.active_idx,
                        (self.active_idx + self.repeat[0] as usize - 1)
                            .min(self.filtered_entries.len() - 1),
                    )
                } else {
                    (
                        self.active_idx.saturating_sub(self.repeat[0] as usize - 1),
                        self.active_idx,
                    )
                };

                for choice_idx in &self.filtered_entries[start_idx..=end_idx] {
                    multiple_idx[*choice_idx] ^= true;
                }
            }
        }
    }

    fn set_filtered_entries_multiple_marker(&mut self, mark: bool) {
        if let Some(multiple_idx) = self.multiple_idx.as_mut() {
            for choice_idx in &self.filtered_entries {
                multiple_idx[*choice_idx] = mark;
            }
        }
    }

    fn toggle_filtered_entries_multiple_marker(&mut self) {
        if let Some(multiple_idx) = self.multiple_idx.as_mut() {
            for choice_idx in &self.filtered_entries {
                multiple_idx[*choice_idx] ^= true;
            }
        }
    }

    fn copy_active_choice_to_clipboard(&self) {
        if let Some(choice_idx) = self.filtered_entries.get(self.active_idx) {
            let text = self.model.choices[*choice_idx].id.as_str();
            let clipboard = [Clipboard::Clipboard, Clipboard::PrimarySelection];
            for &c in &clipboard {
                self.window.window.set_clipboard(c, text.to_string());
            }
        }
    }

    fn update_filter(&mut self) {
        if self.filter_term.is_empty() {
            self.filtered_entries = (0..self.model.choices.len()).collect();
            return;
        }

        self.filtered_entries.clear();

        struct MatchResult {
            row_idx: usize,
            score: u32,
        }

        let pattern = matcher_pattern(&self.filter_term);

        let mut scores: Vec<MatchResult> = self
            .model
            .choices
            .par_iter()
            .enumerate()
            .filter_map(|(row_idx, entry)| {
                let score = matcher_score(&pattern, &entry.label)?;
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

    fn render(&mut self) -> anyhow::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        let max_width = cols.saturating_sub(6);
        let selector_size = self.model.choices.len().min(self.max_items);
        let selector_start_row = rows.saturating_sub(selector_size + 3);
        let max_items = self.max_items;

        let mut changes = vec![];

        // Initial setup
        changes.push(Change::ClearScreen(ColorAttribute::Default));
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(0),
        });
        changes.push(Change::CursorVisibility(CursorVisibility::Hidden));

        // Context section
        if let Some(context) = self.model.context.as_ref() {
            changes.push(Change::Attribute(AttributeChange::Intensity(
                Intensity::Bold,
            )));
            changes.push(Change::Attribute(AttributeChange::Foreground(
                self.colors.context_header_fg,
            )));
            changes.push(Change::Text(context.header.clone()));
            changes.push(Change::AllAttributes(CellAttributes::default()));

            for entry in &context.entries {
                changes.push(Change::Text("\r\n".to_string()));
                changes.push(Change::Attribute(AttributeChange::Foreground(
                    self.colors.context_label_fg,
                )));
                changes.push(Change::Text(entry.label.clone()));
                changes.push(Change::AllAttributes(CellAttributes::default()));
                changes.push(Change::Text(format!(": {}", entry.value)));
                changes.push(Change::AllAttributes(CellAttributes::default()));
            }

            changes.push(Change::Text("\r\n\r\n".to_string()));
        }

        // Section header and actions
        changes.push(Change::Attribute(AttributeChange::Intensity(
            Intensity::Bold,
        )));
        changes.push(Change::Attribute(AttributeChange::Foreground(
            self.colors.section_header_fg,
        )));
        changes.push(Change::Text(self.model.header.clone()));
        changes.push(Change::AllAttributes(CellAttributes::default()));

        for action in &self.model.actions {
            let entry_start = changes.len();
            changes.push(Change::Text("\r\n  ".to_string()));
            let style = EntryRenderStyle::new(&action.key, &self.typed);
            style.append_key(
                &self.colors,
                &action.key,
                self.model.max_key_width,
                &mut changes,
            );
            changes.push(Change::AllAttributes(CellAttributes::default()));
            changes.push(Change::Text(format!(" {}", action.description)));

            style.apply(&self.colors, &mut changes[entry_start..]);
        }

        // Selector area: separator + description
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(selector_start_row),
        });
        changes.push(Change::Attribute(AttributeChange::Foreground(
            self.colors.separator_fg,
        )));
        changes.push(Change::Text(self.separator_line.clone()));
        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::Text("\r\n".to_string()));
        changes.push(Change::Attribute(AttributeChange::Intensity(
            Intensity::Bold,
        )));
        changes.push(Change::Attribute(AttributeChange::Foreground(
            self.colors.description_fg,
        )));
        changes.push(Change::Text(truncate_right(&self.description, max_width)));
        changes.push(Change::AllAttributes(CellAttributes::default()));
        if !self.filter_term.is_empty() {
            changes.push(Change::Text(format!(": {}", self.filter_term)));
        }
        changes.push(Change::Text("\r\n".to_string()));

        // Selector entries
        for (row_num, (entry_idx, choice_idx)) in self
            .filtered_entries
            .iter()
            .enumerate()
            .skip(self.top_row)
            .enumerate()
        {
            if row_num > max_items {
                break;
            }

            if row_num != 0 {
                changes.push(Change::Text("\r\n".to_string()));
            }

            let mut attr = CellAttributes::blank();

            if let Some(multiple_idx) = self.multiple_idx.as_deref() {
                if multiple_idx[*choice_idx] {
                    changes.push(Change::Attribute(AttributeChange::Background(
                        self.colors.multiple_marker_bg,
                    )));
                    changes.push(Change::Text(" ".to_string()));
                    changes.push(Change::Attribute(AttributeChange::Background(
                        ColorAttribute::Default,
                    )));
                } else {
                    changes.push(Change::Text(" ".to_string()));
                }
            }

            if entry_idx == self.active_idx {
                changes.push(Change::Attribute(AttributeChange::Reverse(true)));
                attr.set_reverse(true);
            }

            changes.push(Change::Text("    ".to_string()));
            let entry = &self.model.choices[*choice_idx];
            let mut line = crate::tabbar::parse_status_text(&entry.label, attr.clone());
            if line.len() > max_width {
                line.resize(max_width, termwiz::surface::SEQ_ZERO);
            }
            changes.extend(line.changes(&attr));
            changes.push(Change::Text(" ".to_string()));
            if entry_idx == self.active_idx {
                changes.push(Change::Attribute(AttributeChange::Reverse(false)));
            }
            changes.push(Change::AllAttributes(CellAttributes::default()));
        }

        // Filter input overlay
        if self.filtering {
            let filter_prefix = truncate_right(&self.fuzzy_description, max_width);
            let cursor_x = filter_prefix.len() + 2 + self.filter_term.len(); // +2 for ": "

            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(selector_start_row + 1),
            });
            changes.push(Change::ClearToEndOfLine(ColorAttribute::Default));
            changes.push(Change::Attribute(AttributeChange::Intensity(
                Intensity::Bold,
            )));
            changes.push(Change::Attribute(AttributeChange::Foreground(
                self.colors.description_fg,
            )));
            changes.push(Change::Text(filter_prefix));
            changes.push(Change::AllAttributes(CellAttributes::default()));
            changes.push(Change::Text(format!(": {}", self.filter_term)));
            changes.push(Change::CursorPosition {
                x: Position::Absolute(cursor_x),
                y: Position::Absolute(selector_start_row + 1),
            });
            changes.push(Change::CursorVisibility(CursorVisibility::Visible));
        }

        self.buf.add_changes(changes);
        self.buf.flush()?;

        Ok(())
    }

    /// Handles a keymap character input.
    fn handle_keymap_char(&mut self, c: char) -> anyhow::Result<LoopAction> {
        self.typed.push(c);

        match self.model.keymap.lookup(&self.typed) {
            KeyLookup::Found(action_id) => {
                let action = &self.model.actions[action_id.0];
                let name = match *action.action {
                    KeyAssignment::EmitEvent(ref id) => id,
                    _ => anyhow::bail!(
                        "SelectorActions requires action to be defined by wezterm.action_callback"
                    ),
                };

                let ids = selected_ids(
                    &self.model.choices,
                    &self.filtered_entries,
                    self.active_idx,
                    self.multiple_idx.as_deref(),
                );

                if ids.is_empty() {
                    self.typed.clear();
                    return Ok(LoopAction::SkipRender);
                }

                let result = SelectorActionsResult { ids };
                self.trigger_event(name, Some(result));
                if action.keep_overlay {
                    self.typed.clear();
                } else {
                    return Ok(LoopAction::Break);
                }
            }
            KeyLookup::Prefix => {}
            KeyLookup::NotFound => self.typed.clear(),
        }

        Ok(LoopAction::Render)
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            self.repeat[0] = self.repeat[1];
            if self.repeat[1] != 1 {
                self.repeat[1] = 1;
            }
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
                    key: KeyCode::Char('/'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.filtering ^= true;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    modifiers: _,
                }) if self.filtering => {
                    if self.filter_term.pop().is_some() {
                        self.update_filter();
                    } else {
                        continue;
                    }
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    modifiers: _,
                }) => {
                    self.typed.pop();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('G' | 'C'),
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
                    key: KeyCode::Char('A'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.set_filtered_entries_multiple_marker(true);
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('D'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.set_filtered_entries_multiple_marker(false);
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('T'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.toggle_filtered_entries_multiple_marker();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    modifiers: _,
                }) if self.filtering => {
                    self.filter_term.push(c);
                    self.update_filter();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('j'),
                    modifiers: Modifiers::NONE,
                }) if !self.model.keymap.has_continuation(&self.typed, 'j') => {
                    self.move_down();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('k'),
                    modifiers: Modifiers::NONE,
                }) if !self.model.keymap.has_continuation(&self.typed, 'k') => {
                    self.move_up();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('/'),
                    modifiers: Modifiers::NONE,
                }) if !self.model.keymap.has_continuation(&self.typed, '/') => {
                    self.filtering = true;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('y'),
                    modifiers: Modifiers::NONE,
                }) if !self.model.keymap.has_continuation(&self.typed, 'y') => {
                    self.copy_active_choice_to_clipboard();
                    continue;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('g'),
                    modifiers: Modifiers::NONE,
                }) if !self.model.keymap.has_continuation(&self.typed, 'g') => {
                    self.active_idx = 0;
                    self.top_row = 0;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('G'),
                    modifiers: Modifiers::NONE,
                })
                | InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('G'),
                    modifiers: Modifiers::SHIFT,
                }) if !self.model.keymap.has_continuation(&self.typed, 'G') => {
                    self.active_idx = self.filtered_entries.len().saturating_sub(1);
                    self.top_row = self.active_idx.saturating_sub(self.max_items);
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    modifiers: Modifiers::NONE,
                }) if c.is_ascii_digit() && !self.model.keymap.has_continuation(&self.typed, c) => {
                    if c >= '2' {
                        self.repeat[1] = c as u8 - b'0';
                    }
                    continue;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    ..
                }) => match self.handle_keymap_char(c)? {
                    LoopAction::Break => break,
                    LoopAction::SkipRender => continue,
                    LoopAction::Render => {}
                },
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Tab,
                    modifiers: Modifiers::NONE,
                }) => {
                    self.toggle_multiple_marker(true);
                    self.move_down();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Tab,
                    modifiers: Modifiers::SHIFT,
                }) => {
                    self.toggle_multiple_marker(false);
                    self.move_up();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    ..
                }) => {
                    if self.filtering {
                        self.filtering = false;
                    } else {
                        match self.handle_keymap_char('\n')? {
                            LoopAction::Break => break,
                            LoopAction::SkipRender => continue,
                            LoopAction::Render => {}
                        }
                    }
                }
                InputEvent::Resized { cols, rows } => {
                    let context_size = self
                        .model
                        .context
                        .as_ref()
                        .map_or(0, |v| v.entries.len() + 2);
                    let actions_size = self.model.actions.len() + 1;
                    let overhead = context_size + actions_size + 3;
                    self.max_items = rows.saturating_sub(overhead);
                    self.separator_line = "─".repeat(cols);

                    self.buf.resize(cols, rows);
                }
                _ => continue,
            }
            self.render()?;
        }

        Ok(())
    }

    fn trigger_event(&self, name: &str, result: Option<SelectorActionsResult>) {
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

struct SelectorActionsResult {
    ids: Vec<String>,
}

impl ToDynamic for SelectorActionsResult {
    fn to_dynamic(&self) -> Value {
        self.ids.to_dynamic()
    }
}

impl FromDynamic for SelectorActionsResult {
    fn from_dynamic(
        value: &Value,
        options: wezterm_dynamic::FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        Ok(Self {
            ids: Vec::from_dynamic(value, options)?,
        })
    }
}

impl_lua_conversion_dynamic!(SelectorActionsResult);

#[cfg(test)]
mod test {
    use super::*;

    fn action(key: &str) -> TransientAction {
        TransientAction {
            key: key.to_string(),
            description: key.to_string(),
            action: Box::new(KeyAssignment::EmitEvent(key.to_string())),
            keep_overlay: false,
        }
    }

    #[test]
    fn selector_model_indexes_actions_by_key() {
        let model = SelectorModel::new(
            vec![],
            SelectorActionSection {
                header: None,
                actions: vec![action("a"), action("bb")],
            },
            None,
        );

        assert!(matches!(
            model.keymap.lookup("bb"),
            KeyLookup::Found(ActionId(1))
        ));
        assert_eq!(model.header, "Default");
        assert_eq!(model.max_key_width, 2);
    }

    #[test]
    fn selected_ids_uses_the_active_filtered_entry() {
        let choices = vec![
            SelectorActionsEntry {
                label: "Alpha".to_string(),
                id: "alpha".to_string(),
            },
            SelectorActionsEntry {
                label: "Beta".to_string(),
                id: "beta".to_string(),
            },
            SelectorActionsEntry {
                label: "Gamma".to_string(),
                id: "gamma".to_string(),
            },
        ];
        let filtered = vec![1, 2];

        assert_eq!(
            selected_ids(&choices, &filtered, 1, None),
            vec!["gamma".to_string()]
        );
    }

    #[test]
    fn selected_ids_prefers_multiple_markers_over_the_active_entry() {
        let choices = vec![
            SelectorActionsEntry {
                label: "Alpha".to_string(),
                id: "alpha".to_string(),
            },
            SelectorActionsEntry {
                label: "Beta".to_string(),
                id: "beta".to_string(),
            },
            SelectorActionsEntry {
                label: "Gamma".to_string(),
                id: "gamma".to_string(),
            },
        ];
        let filtered = vec![1];

        assert_eq!(
            selected_ids(&choices, &filtered, 0, Some(&[true, false, true]),),
            vec!["alpha".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn selected_ids_is_empty_when_no_filtered_entry_is_available() {
        let choices = vec![SelectorActionsEntry {
            label: "Alpha".to_string(),
            id: "alpha".to_string(),
        }];

        assert!(selected_ids(&choices, &[], 0, Some(&[false])).is_empty());
    }

    #[test]
    fn selector_actions_result_converts_to_and_from_the_ids_array() {
        let ids = vec!["choice1".to_string(), "choice2".to_string()];
        let value = ids.to_dynamic();

        assert_eq!(
            SelectorActionsResult { ids: ids.clone() }.to_dynamic(),
            value
        );

        assert_eq!(
            SelectorActionsResult::from_dynamic(&value, Default::default())
                .unwrap()
                .ids,
            ids
        );
    }
}

fn create_keymap(actions: &[TransientAction]) -> KeyMap<ActionId> {
    let mut keymap = KeyMap::new();
    for (idx, action) in actions.iter().enumerate() {
        keymap.insert(&action.key, ActionId(idx));
    }
    keymap
}

fn trampoline(name: String, window: GuiWin, pane: MuxPane, result: Option<SelectorActionsResult>) {
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
    result: Option<SelectorActionsResult>,
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

pub fn show_selector_actions_overlay(
    mut term: TermWizTerminal,
    args: SelectorActions,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    term.render(&[Change::Title(args.title.clone())])?;
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let mut state = SelectorState::new(args, window, pane, buf);

    state.render()?;
    state.run_loop()?;
    Ok(())
}
