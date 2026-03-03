use crate::overlay::common::{KeyLookup, KeyMap, OverlayColors};
use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{
    KeyAssignment, SelectorActions, SelectorActionsEntry, TransientArgument, TransientContext,
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
use wezterm_dynamic::{FromDynamic, ToDynamic};
use wezterm_term::{AttributeChange, CellAttributes, Intensity};
use window::{Clipboard, Modifiers, WindowOps};

/// Concatenate a prefix and value without format! overhead
#[inline]
fn concat_str(prefix: &str, value: &str) -> String {
    let mut s = String::with_capacity(prefix.len() + value.len());
    s.push_str(prefix);
    s.push_str(value);
    s
}

#[derive(Clone)]
struct SelectorEntry<'a> {
    delegate: &'a SelectorActionsEntry,
    idx: usize,
}

struct ArgumentSection<'a> {
    header: String,
    arguments: Vec<&'a TransientArgument>,
}

struct SelectorState<'a> {
    active_idx: usize,
    max_items: usize,
    top_row: usize,
    choices: &'a Vec<SelectorEntry<'a>>,
    multiple_idx: Option<Vec<bool>>,
    filtered_entries: Vec<&'a SelectorEntry<'a>>,
    filtering: bool,
    filter_term: String,
    description: String,
    fuzzy_description: String,
    window: GuiWin,
    pane: MuxPane,
    keymap: &'a KeyMap<'a, TransientArgument>,
    typed: String,
    context: Option<&'a TransientContext>,
    colors: OverlayColors,
    section: ArgumentSection<'a>,
    cancel: Option<Box<KeyAssignment>>,
    repeat: [u8; 2],
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
    separator_line: String,
}

impl<'a> SelectorState<'a> {
    fn new(
        args: &'a SelectorActions,
        window: GuiWin,
        pane: MuxPane,
        keymap: &'a KeyMap<'a, TransientArgument>,
        choices: &'a Vec<SelectorEntry<'_>>,
        buf: &'a mut BufferedTerminal<TermWizTerminal>,
    ) -> Self {
        let context_size = args.context.as_ref().map_or(0, |v| v.entries.len() + 2);
        let positional_args_size = args.section.arguments.len() + 1;
        let overhead = context_size + positional_args_size + 3;

        let (_, rows) = buf.dimensions();
        let max_items = rows.saturating_sub(overhead);

        let multiple_idx = args.multiple.then(|| vec![false; choices.len()]);
        let filtered_entries = choices.iter().collect();

        let arguments: Vec<&TransientArgument> = args.section.arguments.iter().collect();
        let section = ArgumentSection {
            header: args
                .section
                .header
                .clone()
                .unwrap_or_else(|| "Default".to_string()),
            arguments,
        };

        let fuzzy_description = args
            .fuzzy_description
            .clone()
            .unwrap_or_else(|| args.description.clone());

        let (cols, _) = buf.dimensions();
        let separator_line = "─".repeat(cols);

        SelectorState {
            active_idx: 0,
            max_items,
            top_row: 0,
            choices,
            multiple_idx,
            filtered_entries,
            filtering: args.fuzzy,
            filter_term: String::new(),
            description: args.description.clone(),
            fuzzy_description,
            window,
            pane,
            keymap,
            typed: String::new(),
            context: args.context.as_ref(),
            colors: OverlayColors::new(),
            section,
            cancel: args.cancel.clone(),
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

                for entry in &self.filtered_entries[start_idx..=end_idx] {
                    multiple_idx[entry.idx] ^= true;
                }
            }
        }
    }

    fn set_filtered_entries_multiple_marker(&mut self, mark: bool) {
        if let Some(multiple_idx) = self.multiple_idx.as_mut() {
            for entry in &self.filtered_entries {
                multiple_idx[entry.idx] = mark;
            }
        }
    }

    fn toggle_filtered_entries_multiple_marker(&mut self) {
        if let Some(multiple_idx) = self.multiple_idx.as_mut() {
            for entry in &self.filtered_entries {
                multiple_idx[entry.idx] ^= true;
            }
        }
    }

    fn copy_active_choice_to_clipboard(&self) {
        if let Some(entry) = self.filtered_entries.get(self.active_idx) {
            let text = entry.delegate.id.as_ref().unwrap_or(&entry.delegate.label);
            let clipboard = [Clipboard::Clipboard, Clipboard::PrimarySelection];
            for &c in &clipboard {
                self.window.window.set_clipboard(c, text.to_string());
            }
        }
    }

    fn update_filter(&mut self) {
        if self.filter_term.is_empty() {
            self.filtered_entries = self.choices.iter().collect();
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
                let score = matcher_score(&pattern, &entry.delegate.label)?;
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

    fn render(&mut self) -> anyhow::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        let max_width = cols.saturating_sub(6);
        let selector_size = self.choices.len().min(self.max_items);
        let selector_start_row = rows - selector_size - 3;
        let max_items = self.max_items;

        // Estimate capacity: base changes + context + section + selector entries
        let context_entries = self.context.as_ref().map_or(0, |c| c.entries.len());
        let visible_entries = self.filtered_entries.len().min(max_items + 1);
        let capacity =
            20 + context_entries * 6 + self.section.arguments.len() * 5 + visible_entries * 10;
        let mut changes = Vec::with_capacity(capacity);

        // Initial setup
        changes.push(Change::ClearScreen(ColorAttribute::Default));
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(0),
        });
        changes.push(Change::CursorVisibility(CursorVisibility::Hidden));

        // Context section
        if let Some(context) = self.context.as_ref() {
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
                changes.push(Change::Text(concat_str(": ", &entry.id)));
                changes.push(Change::AllAttributes(CellAttributes::default()));
            }

            changes.push(Change::Text("\r\n\r\n".to_string()));
        }

        // Section header and arguments
        changes.push(Change::Attribute(AttributeChange::Intensity(
            Intensity::Bold,
        )));
        changes.push(Change::Attribute(AttributeChange::Foreground(
            self.colors.section_header_fg,
        )));
        changes.push(Change::Text(self.section.header.clone()));
        changes.push(Change::AllAttributes(CellAttributes::default()));

        for positional_arg in &self.section.arguments {
            changes.push(Change::Text("\r\n  ".to_string()));
            changes.push(Change::Attribute(AttributeChange::Foreground(
                self.colors.key_fg,
            )));
            changes.push(Change::Text(positional_arg.key.clone()));
            changes.push(Change::AllAttributes(CellAttributes::default()));
            changes.push(Change::Text(concat_str(" ", &positional_arg.description)));
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
        changes.push(Change::Text("\r\n".to_string()));

        // Selector entries
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

            if row_num != 0 {
                changes.push(Change::Text("\r\n".to_string()));
            }

            let mut attr = CellAttributes::blank();

            if let Some(multiple_idx) = self.multiple_idx.as_ref() {
                if multiple_idx[self.filtered_entries[entry_idx].idx] {
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
            let mut line = crate::tabbar::parse_status_text(&entry.delegate.label, attr.clone());
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
            changes.push(Change::Text(concat_str(": ", &self.filter_term)));
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
                    continue;
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
                }) if !self.keymap.has_continuation(&self.typed, 'j') => {
                    self.move_down();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('k'),
                    modifiers: Modifiers::NONE,
                }) if !self.keymap.has_continuation(&self.typed, 'k') => {
                    self.move_up();
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('/'),
                    modifiers: Modifiers::NONE,
                }) if !self.keymap.has_continuation(&self.typed, '/') => {
                    self.filtering = true;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('y'),
                    modifiers: Modifiers::NONE,
                }) if !self.keymap.has_continuation(&self.typed, 'y') => {
                    self.copy_active_choice_to_clipboard();
                    continue;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    modifiers: Modifiers::NONE,
                }) if c.is_ascii_digit() && !self.keymap.has_continuation(&self.typed, c) => {
                    if c >= '2' {
                        self.repeat[1] = c as u8 - b'0';
                    }
                    continue;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    ..
                }) => {
                    self.typed.push(c);

                    match self.keymap.lookup(&self.typed) {
                        KeyLookup::Found(positional_arg) => {
                            let name = match *positional_arg.action {
                                KeyAssignment::EmitEvent(ref id) => id,
                                _ => anyhow::bail!("SelectorActions requires action to be defined by wezterm.action_callback")
                            };

                            let mut choices: Vec<SelectorActionsEntry> = vec![];

                            if let Some(multiple_idx) = self.multiple_idx.as_ref() {
                                choices.extend(
                                    multiple_idx
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, val)| **val)
                                        .map(|(idx, _)| SelectorActionsEntry {
                                            label: self.choices[idx].delegate.label.clone(),
                                            id: self.choices[idx].delegate.id.clone(),
                                            metadata: self.choices[idx].delegate.metadata.clone(),
                                        }),
                                );
                            }

                            if choices.is_empty() && self.filtered_entries.is_empty() {
                                self.typed.clear();
                                continue;
                            }

                            if choices.is_empty() {
                                let entry = self.filtered_entries[self.active_idx];
                                choices.push(SelectorActionsEntry {
                                    label: entry.delegate.label.clone(),
                                    id: entry.delegate.id.clone(),
                                    metadata: entry.delegate.metadata.clone(),
                                });
                            }

                            let result = SelectorActionsResult { choices };
                            self.trigger_event(name, Some(result));
                            if positional_arg.keep_overlay {
                                self.typed.clear();
                            } else {
                                break;
                            }
                        }
                        KeyLookup::Prefix => {}
                        KeyLookup::NotFound => self.typed.clear(),
                    }
                }
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
                        continue;
                    }
                }
                InputEvent::Resized { cols, rows } => {
                    let context_size = self.context.as_ref().map_or(0, |v| v.entries.len() + 2);
                    let positional_args_size = self.section.arguments.len() + 1;
                    let overhead = context_size + positional_args_size + 3;
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

#[derive(FromDynamic, ToDynamic)]
struct SelectorActionsResult {
    choices: Vec<SelectorActionsEntry>,
}
impl_lua_conversion_dynamic!(SelectorActionsResult);

fn create_keymap<'a>(args: &'a SelectorActions, keymap: &mut KeyMap<'a, TransientArgument>) {
    for positional_arg in &args.section.arguments {
        keymap.insert(&positional_arg.key, positional_arg);
    }
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
    term: TermWizTerminal,
    args: SelectorActions,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let choices: Vec<SelectorEntry<'_>> = args
        .choices
        .iter()
        .enumerate()
        .map(|(idx, delegate)| SelectorEntry { delegate, idx })
        .collect();

    let mut keymap = KeyMap::new();
    create_keymap(&args, &mut keymap);

    let mut state = SelectorState::new(&args, window, pane, &keymap, &choices, &mut buf);

    state.render()?;
    state.run_loop()?;
    Ok(())
}
