use crate::scripting::guiwin::GuiWin;
use config::configuration;
use config::keyassignment::{ImageSelector, ImageSelectorEntry, KeyAssignment};
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use termwiz::cell::{AttributeChange, CellAttributes, Intensity};
use termwiz::color::ColorAttribute;
use termwiz::image::{ImageData, ImageDataType, TextureCoordinate};
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::surface::change::Image;
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;
use termwiz_funcs::truncate_right;

use super::selector::{matcher_pattern, matcher_score};

const ROW_OVERHEAD: usize = 3;
const SEPARATOR: &str = "│";
const IMAGE_CACHE_CAPACITY: usize = 10;

enum ImageLoadError {
    NotFound,
    PermissionDenied,
    InvalidFormat,
    IoError(String),
}

impl ImageLoadError {
    fn message(&self) -> String {
        match self {
            Self::NotFound => "File not found".to_string(),
            Self::PermissionDenied => "Permission denied".to_string(),
            Self::InvalidFormat => "Invalid or unsupported image format".to_string(),
            Self::IoError(msg) => format!("I/O error: {}", msg),
        }
    }
}

struct CachedImage {
    data: Arc<ImageData>,
    dims: (u32, u32),
}

struct ImageCache {
    entries: HashMap<String, CachedImage>,
    order: VecDeque<String>,
    capacity: usize,
}

impl ImageCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get(&mut self, path: &str) -> Option<(Arc<ImageData>, (u32, u32))> {
        if let Some(cached) = self.entries.get(path) {
            if let Some(pos) = self.order.iter().position(|p| p == path) {
                self.order.remove(pos);
                self.order.push_front(path.to_string());
            }
            Some((Arc::clone(&cached.data), cached.dims))
        } else {
            None
        }
    }

    fn put(&mut self, path: String, data: Arc<ImageData>, dims: (u32, u32)) {
        if self.entries.contains_key(&path) {
            if let Some(pos) = self.order.iter().position(|p| p == &path) {
                self.order.remove(pos);
            }
            self.order.push_front(path);
            return;
        }

        if self.entries.len() == self.capacity {
            if let Some(oldest) = self.order.pop_back() {
                self.entries.remove(&oldest);
            }
        }

        self.entries
            .insert(path.clone(), CachedImage { data, dims });
        self.order.push_front(path);
    }
}

struct ImageSelectorState<'a> {
    active_idx: usize,
    max_items: usize,
    top_row: usize,
    filter_term: String,
    filtered_indices: Vec<usize>,
    pane: MuxPane,
    window: GuiWin,
    filtering: bool,
    args: ImageSelector,
    event_name: String,
    fuzzy_description: String,
    description_fg: ColorAttribute,
    error_fg: ColorAttribute,
    separator_fg: ColorAttribute,
    image_cache: ImageCache,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
}

fn get_entry_label(entry: &ImageSelectorEntry) -> String {
    entry.label.clone().unwrap_or_else(|| {
        Path::new(&entry.path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| entry.path.clone())
    })
}

impl<'a> ImageSelectorState<'a> {
    fn update_filter(&mut self) {
        self.filtered_indices.clear();

        if self.filter_term.is_empty() {
            self.filtered_indices = (0..self.args.choices.len()).collect();
            return;
        }

        struct MatchResult {
            idx: usize,
            score: u32,
        }

        let pattern = matcher_pattern(&self.filter_term);

        let mut scores: Vec<MatchResult> = self
            .args
            .choices
            .par_iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                let label = get_entry_label(entry);
                let score = matcher_score(&pattern, &label)?;
                Some(MatchResult { idx, score })
            })
            .collect();

        scores.sort_by(|a, b| b.score.cmp(&a.score));
        self.filtered_indices = scores.into_iter().map(|r| r.idx).collect();

        self.active_idx = 0;
        self.top_row = 0;
    }

    fn load_image(&mut self, path: &str) -> Result<(Arc<ImageData>, (u32, u32)), ImageLoadError> {
        if let Some(result) = self.image_cache.get(path) {
            return Ok(result);
        }

        let data = std::fs::read(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ImageLoadError::NotFound,
            std::io::ErrorKind::PermissionDenied => ImageLoadError::PermissionDenied,
            _ => ImageLoadError::IoError(e.to_string()),
        })?;

        let image_data = Arc::new(ImageData::with_data(ImageDataType::EncodedFile(data)));
        let dims = image_data
            .data()
            .dimensions()
            .map_err(|_| ImageLoadError::InvalidFormat)?;

        self.image_cache
            .put(path.to_string(), Arc::clone(&image_data), dims);

        Ok((image_data, dims))
    }

    fn calculate_display_size(
        img_w: u32,
        img_h: u32,
        max_w: usize,
        max_h: usize,
        cell_w: usize,
        cell_h: usize,
    ) -> (usize, usize) {
        if cell_w == 0 || cell_h == 0 {
            return (max_w, max_h);
        }

        let max_px_w = max_w * cell_w;
        let max_px_h = max_h * cell_h;

        let scale = (max_px_w as f64 / img_w as f64)
            .min(max_px_h as f64 / img_h as f64)
            .min(1.0);

        let disp_w = ((img_w as f64 * scale) as usize + cell_w - 1) / cell_w;
        let disp_h = ((img_h as f64 * scale) as usize + cell_h - 1) / cell_h;

        (disp_w.max(1), disp_h.max(1))
    }

    fn render(&mut self) -> termwiz::Result<()> {
        let (cols, rows) = self.buf.dimensions();
        let size = self.buf.terminal().get_screen_size()?;

        let list_width = cols * 2 / 7;
        let preview_col = list_width + 1;
        let preview_width = cols.saturating_sub(preview_col).saturating_sub(1);
        let max_width = list_width.saturating_sub(5);
        let max_items = rows.saturating_sub(ROW_OVERHEAD);
        self.max_items = max_items;

        self.buf.add_changes(vec![
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
            AttributeChange::Intensity(Intensity::Bold).into(),
            AttributeChange::Foreground(self.description_fg).into(),
            Change::Text(format!(
                "{}\r\n",
                truncate_right(&self.args.description, max_width)
            )),
            Change::AllAttributes(CellAttributes::default()),
        ]);

        for idx in self.top_row
            ..self
                .filtered_indices
                .len()
                .min(self.top_row + max_items + 1)
        {
            let entry_idx = self.filtered_indices[idx];
            let entry = &self.args.choices[entry_idx];
            let label = get_entry_label(entry);

            let is_active = idx == self.active_idx;
            if is_active {
                self.buf.add_change(AttributeChange::Reverse(true));
            }

            self.buf.add_changes(vec![
                Change::Text("    ".into()),
                Change::Text(truncate_right(&label, max_width)),
                Change::Text(" ".into()),
            ]);

            if is_active {
                self.buf.add_change(AttributeChange::Reverse(false));
            }
            self.buf.add_changes(vec![
                Change::AllAttributes(CellAttributes::default()),
                Change::Text("\r\n".into()),
            ]);
        }

        let filter_cursor_x = if self.filtering || !self.filter_term.is_empty() {
            let suffix = format!(": {}", self.filter_term);
            let cursor_x = self.fuzzy_description.len() + suffix.len();
            self.buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(0),
                },
                Change::ClearToEndOfLine(ColorAttribute::Default),
                AttributeChange::Intensity(Intensity::Bold).into(),
                AttributeChange::Foreground(self.description_fg).into(),
                Change::Text(truncate_right(&self.fuzzy_description, max_width)),
                Change::AllAttributes(CellAttributes::default()),
                Change::Text(truncate_right(
                    &suffix,
                    max_width.saturating_sub(self.fuzzy_description.len()),
                )),
            ]);
            Some(cursor_x)
        } else {
            None
        };

        self.buf
            .add_change(AttributeChange::Foreground(self.separator_fg));
        for row in 0..rows {
            self.buf.add_changes(vec![
                Change::CursorPosition {
                    x: Position::Absolute(list_width),
                    y: Position::Absolute(row),
                },
                Change::Text(SEPARATOR.into()),
            ]);
        }
        self.buf
            .add_change(Change::AllAttributes(CellAttributes::default()));

        let preview_path = self
            .filtered_indices
            .get(self.active_idx)
            .and_then(|&i| self.args.choices.get(i))
            .map(|e| e.path.clone());

        if let Some(path) = preview_path {
            self.buf.add_change(Change::CursorPosition {
                x: Position::Absolute(preview_col),
                y: Position::Absolute(0),
            });

            let filename = Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            self.buf
                .add_change(Change::Text(truncate_right(&filename, preview_width)));

            match self.load_image(&path) {
                Ok((image_data, (img_w, img_h))) => {
                    let max_h = rows.saturating_sub(5);
                    let max_w = preview_width.saturating_sub(2);

                    let (disp_w, disp_h) = Self::calculate_display_size(
                        img_w,
                        img_h,
                        max_w,
                        max_h,
                        size.xpixel,
                        size.ypixel,
                    );

                    if disp_h > 0 && disp_w > 0 {
                        let x_offset = (max_w.saturating_sub(disp_w)) / 2;
                        self.buf.add_changes(vec![
                            Change::CursorPosition {
                                x: Position::Absolute(preview_col + x_offset),
                                y: Position::Absolute(2),
                            },
                            Change::Image(Image {
                                width: disp_w,
                                height: disp_h,
                                top_left: TextureCoordinate::new_f32(0.0, 0.0),
                                bottom_right: TextureCoordinate::new_f32(1.0, 1.0),
                                image: image_data,
                            }),
                        ]);
                    }
                }
                Err(err) => {
                    self.buf.add_changes(vec![
                        Change::CursorPosition {
                            x: Position::Absolute(preview_col),
                            y: Position::Absolute(2),
                        },
                        AttributeChange::Foreground(self.error_fg).into(),
                        Change::Text(err.message()),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);
                }
            }
        }

        if self.filtering {
            if let Some(cursor_x) = filter_cursor_x {
                self.buf.add_changes(vec![
                    Change::CursorVisibility(CursorVisibility::Visible),
                    Change::CursorPosition {
                        x: Position::Absolute(cursor_x),
                        y: Position::Absolute(0),
                    },
                ]);
            }
        }

        self.buf.flush()
    }

    fn trigger_event(&self, entry: Option<&ImageSelectorEntry>) {
        let name = self.event_name.clone();
        let window = self.window.clone();
        let pane = self.pane.clone();
        let entry = entry.cloned();

        promise::spawn::spawn_into_main_thread(async move {
            trampoline(name, window, pane, entry);
            anyhow::Result::<()>::Ok(())
        })
        .detach();
    }

    fn launch(&self) -> bool {
        let entry = self
            .filtered_indices
            .get(self.active_idx)
            .and_then(|&i| self.args.choices.get(i));

        if let Some(entry) = entry {
            self.trigger_event(Some(entry));
            true
        } else {
            false
        }
    }

    fn move_up(&mut self) {
        self.active_idx = self.active_idx.saturating_sub(1);
        if self.active_idx < self.top_row {
            self.top_row = self.active_idx;
        }
    }

    fn move_down(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        self.active_idx = (self.active_idx + 1).min(self.filtered_indices.len() - 1);
        if self.active_idx > self.top_row + self.max_items {
            self.top_row = self.active_idx.saturating_sub(self.max_items);
        }
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        while let Ok(Some(event)) = self.buf.terminal().poll_input(None) {
            match event {
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('j'),
                    ..
                }) if !self.filtering => self.move_down(),

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('k'),
                    ..
                }) if !self.filtering => self.move_up(),

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('P' | 'K'),
                    modifiers: Modifiers::CTRL,
                }) => self.move_up(),

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('N' | 'J'),
                    modifiers: Modifiers::CTRL,
                }) => self.move_down(),

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('/'),
                    modifiers: Modifiers::CTRL,
                }) => self.filtering ^= true,

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('/'),
                    ..
                }) if !self.filtering => self.filtering = true,

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    ..
                }) if self.filtering => {
                    if self.filter_term.pop().is_some() {
                        self.update_filter();
                    }
                }

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('G' | 'C'),
                    modifiers: Modifiers::CTRL,
                })
                | InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    ..
                }) => {
                    self.trigger_event(None);
                    break;
                }

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(c),
                    ..
                }) if self.filtering => {
                    self.filter_term.push(c);
                    self.update_filter();
                }

                InputEvent::Key(KeyEvent {
                    key: KeyCode::UpArrow,
                    ..
                }) => self.move_up(),

                InputEvent::Key(KeyEvent {
                    key: KeyCode::DownArrow,
                    ..
                }) => self.move_down(),

                InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    modifiers: Modifiers::CTRL,
                }) => {
                    if self.launch() {
                        break;
                    }
                    continue;
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
                    self.max_items = rows.saturating_sub(ROW_OVERHEAD);
                    self.buf.resize(cols, rows);
                }

                _ => continue,
            }
            self.render()?;
        }

        Ok(())
    }
}

fn trampoline(name: String, window: GuiWin, pane: MuxPane, entry: Option<ImageSelectorEntry>) {
    promise::spawn::spawn(async move {
        config::with_lua_config_on_main_thread(move |lua| do_event(lua, name, window, pane, entry))
            .await
    })
    .detach();
}

async fn do_event(
    lua: Option<Rc<mlua::Lua>>,
    name: String,
    window: GuiWin,
    pane: MuxPane,
    entry: Option<ImageSelectorEntry>,
) -> anyhow::Result<()> {
    if let Some(lua) = lua {
        let label = entry.as_ref().map(get_entry_label);
        let path = entry.as_ref().map(|e| e.path.clone());
        let args = lua.pack_multi((window, pane, label, path))?;

        if let Err(err) = config::lua::emit_event(&lua, (name.clone(), args)).await {
            log::error!("while processing {} event: {:#}", name, err);
        }
    }
    Ok(())
}

pub fn image_selector(
    term: TermWizTerminal,
    args: ImageSelector,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let event_name = match *args.action {
        KeyAssignment::EmitEvent(ref id) => id.to_string(),
        _ => {
            anyhow::bail!("ImageSelector requires action to be defined by wezterm.action_callback")
        }
    };

    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let fuzzy_description = args
        .fuzzy_description
        .clone()
        .unwrap_or_else(|| args.description.clone());

    let config = configuration();
    let colors = &config.resolved_palette;

    let mut state = ImageSelectorState {
        active_idx: 0,
        max_items: 0,
        pane,
        top_row: 0,
        filter_term: String::new(),
        filtered_indices: vec![],
        window,
        filtering: args.fuzzy,
        args,
        event_name,
        fuzzy_description,
        description_fg: colors
            .image_selector_description_fg
            .map(Into::into)
            .unwrap_or(ColorAttribute::Default),
        error_fg: colors
            .image_selector_error_fg
            .map(Into::into)
            .unwrap_or(ColorAttribute::Default),
        separator_fg: colors
            .image_selector_separator_fg
            .map(Into::into)
            .unwrap_or(ColorAttribute::Default),
        image_cache: ImageCache::new(IMAGE_CACHE_CAPACITY),
        buf: &mut buf,
    };

    state
        .buf
        .add_change(Change::Title(state.args.title.to_string()));
    state.buf.flush()?;
    state.update_filter();
    state.render()?;
    state.run_loop()
}
