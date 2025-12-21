use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{InputForm, KeyAssignment};
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
                .transient_entry_key_fg
                .unwrap_or(AnsiColor::Purple.into())
                .into(),
            active_label_fg: AnsiColor::Yellow.into(),
            placeholder_fg: AnsiColor::Silver.into(),
            input_fg: AnsiColor::White.into(),
            required_fg: AnsiColor::Red.into(),
            border_fg: colors
                .transient_separator_fg
                .map_or_else(|| AnsiColor::Grey.into(), |fg_color| fg_color.into()),
            separator_fg: colors
                .transient_separator_fg
                .map_or_else(|| ColorAttribute::Default, |fg_color| fg_color.into()),
        }
    }
}

struct FormState<'a> {
    args: &'a InputForm,
    window: GuiWin,
    pane: MuxPane,
    active_idx: usize,
    field_values: Vec<String>,
    field_cursors: Vec<usize>,
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
            .map(|f| f.initial_value.clone().unwrap_or_default())
            .collect();
        let field_cursors = field_values.iter().map(|v| v.chars().count()).collect();

        Self {
            args,
            window,
            pane,
            active_idx: 0,
            field_values,
            field_cursors,
            colors: FormColors::new(),
            buf,
        }
    }

    fn render(&mut self) -> termwiz::Result<()> {
        let (cols, _rows) = self.buf.dimensions();
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

        for (idx, field) in self.args.fields.iter().enumerate() {
            let is_active = idx == self.active_idx;

            self.buf.add_changes(vec![
                Change::Text("\r\n".to_string()),
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
                cursor_y = 3 + idx;
                cursor_x = field.label.chars().count() + 4 + self.field_cursors[idx];
            }

            self.buf.add_changes(vec![
                Change::Attribute(AttributeChange::Foreground(input_color)),
                Change::Text(display_value.clone()),
                Change::AllAttributes(CellAttributes::default()),
            ]);
        }

        let submit_label = self.args.submit_label.as_deref().unwrap_or("Submit");
        self.buf.add_changes(vec![
            Change::Text("\r\n\r\n".to_string()),
            Change::Attribute(AttributeChange::Foreground(self.colors.border_fg)),
            Change::Text("[Enter] ".to_string()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text(format!("{}  ", submit_label)),
            Change::Attribute(AttributeChange::Foreground(self.colors.border_fg)),
            Change::Text("[Esc] ".to_string()),
            Change::AllAttributes(CellAttributes::default()),
            Change::Text("Cancel".to_string()),
        ]);

        self.buf.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(cursor_x),
                y: Position::Absolute(cursor_y),
            },
            Change::CursorVisibility(CursorVisibility::Visible),
        ]);

        self.buf.flush()?;
        Ok(())
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            match event {
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    ..
                }) => break,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Tab,
                    modifiers: Modifiers::SHIFT,
                }) => {
                    if self.active_idx > 0 {
                        self.active_idx -= 1;
                    } else {
                        self.active_idx = self.args.fields.len() - 1;
                    }
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Tab, ..
                }) => {
                    if self.active_idx < self.args.fields.len() - 1 {
                        self.active_idx += 1;
                    } else {
                        self.active_idx = 0;
                    }
                }
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
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    ..
                }) => {
                    let mut valid = true;
                    for (idx, field) in self.args.fields.iter().enumerate() {
                        if field.required && self.field_values[idx].trim().is_empty() {
                            self.active_idx = idx;
                            valid = false;
                            break;
                        }
                    }

                    if valid {
                        self.submit();
                        break;
                    }
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
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::RightArrow,
                    ..
                })
                | InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('F'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    if self.field_cursors[self.active_idx] < self.field_values[self.active_idx].chars().count() {
                        self.field_cursors[self.active_idx] += 1;
                    }
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Home, ..
                })
                | InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('A'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.field_cursors[self.active_idx] = 0;
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::End, ..
                })
                | InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('E'),
                    modifiers: Modifiers::CTRL,
                }) => {
                    self.field_cursors[self.active_idx] = self.field_values[self.active_idx].chars().count();
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
                }
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    modifiers,
                }) => {
                    if modifiers.is_empty() || modifiers == Modifiers::SHIFT {
                        let mut chars: Vec<char> = self.field_values[self.active_idx].chars().collect();
                        let pos = self.field_cursors[self.active_idx];
                        chars.insert(pos, c);
                        self.field_values[self.active_idx] = chars.into_iter().collect();
                        self.field_cursors[self.active_idx] += 1;
                    } else if modifiers == Modifiers::CTRL {
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
                            _ => {}
                        }
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
