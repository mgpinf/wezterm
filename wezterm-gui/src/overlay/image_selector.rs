use crate::scripting::guiwin::GuiWin;
use config::configuration;
use config::keyassignment::{ImageSelector, ImageSelectorEntry, KeyAssignment};
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
const DEBOUNCE_MS: u64 = 50;
const POLL_TIMEOUT_MS: u64 = 16;

fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Clone)]
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

struct ImageLoadResult {
    path: String,
    result: Result<(Arc<ImageData>, (u32, u32)), ImageLoadError>,
}

enum PreviewState {
    None,
    Pending,
    Loading,
    Loaded {
        image_data: Arc<ImageData>,
        dims: (u32, u32),
    },
    Error(ImageLoadError),
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

    fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
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
    metadata_fg: ColorAttribute,
    filename_fg: ColorAttribute,
    loading_fg: ColorAttribute,
    image_cache: Arc<Mutex<ImageCache>>,
    buf: &'a mut BufferedTerminal<TermWizTerminal>,
    preview_state: PreviewState,
    current_preview_path: Option<String>,
    last_selection_change: Instant,
    load_receiver: Receiver<ImageLoadResult>,
    load_sender: Sender<ImageLoadResult>,
    in_flight_loads: Arc<Mutex<HashSet<String>>>,
}

fn get_entry_label(entry: &ImageSelectorEntry) -> String {
    entry.label.clone().unwrap_or_else(|| {
        Path::new(&entry.path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| entry.path.clone())
    })
}

fn load_image_sync(path: &str) -> Result<(Arc<ImageData>, (u32, u32)), ImageLoadError> {
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

    Ok((image_data, dims))
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

    fn get_path_at(&self, idx: usize) -> Option<&str> {
        self.filtered_indices
            .get(idx)
            .and_then(|&i| self.args.choices.get(i))
            .map(|e| e.path.as_str())
    }

    fn is_cached(&self, path: &str) -> bool {
        self.image_cache
            .lock()
            .expect("image_cache mutex poisoned")
            .contains(path)
    }

    fn on_selection_changed(&mut self) {
        self.last_selection_change = Instant::now();
        let new_path = self.get_path_at(self.active_idx).map(String::from);

        if new_path != self.current_preview_path {
            self.current_preview_path = new_path;
            self.preview_state = PreviewState::Pending;
        }
    }

    fn check_and_start_loading(&mut self) -> bool {
        if !matches!(self.preview_state, PreviewState::Pending) {
            return false;
        }

        if self.last_selection_change.elapsed() < Duration::from_millis(DEBOUNCE_MS) {
            return false;
        }

        let Some(ref path) = self.current_preview_path else {
            self.preview_state = PreviewState::None;
            return false;
        };

        if let Some((data, dims)) = self
            .image_cache
            .lock()
            .expect("image_cache mutex poisoned")
            .get(path)
        {
            self.preview_state = PreviewState::Loaded {
                image_data: data,
                dims,
            };
            return true;
        }

        let started = self.start_background_load(path.clone());
        self.preview_state = PreviewState::Loading;
        started
    }

    fn start_background_load(&self, path: String) -> bool {
        {
            let mut in_flight = self
                .in_flight_loads
                .lock()
                .expect("in_flight_loads mutex poisoned");
            if !in_flight.insert(path.clone()) {
                return false;
            }
        }

        let sender = self.load_sender.clone();
        let cache = Arc::clone(&self.image_cache);
        let in_flight = Arc::clone(&self.in_flight_loads);

        std::thread::spawn(move || {
            let result = load_image_sync(&path);

            if let Ok((ref data, dims)) = result {
                cache.lock().expect("image_cache mutex poisoned").put(
                    path.clone(),
                    Arc::clone(data),
                    dims,
                );
            }

            in_flight
                .lock()
                .expect("in_flight_loads mutex poisoned")
                .remove(&path);

            let _ = sender.send(ImageLoadResult { path, result });
        });

        true
    }

    fn process_load_results(&mut self) -> bool {
        let mut state_changed = false;

        while let Ok(result) = self.load_receiver.try_recv() {
            if Some(&result.path) == self.current_preview_path.as_ref() {
                match result.result {
                    Ok((image_data, dims)) => {
                        self.preview_state = PreviewState::Loaded { image_data, dims };
                    }
                    Err(err) => {
                        self.preview_state = PreviewState::Error(err);
                    }
                }
                state_changed = true;
                break;
            }
        }

        while self.load_receiver.try_recv().is_ok() {}

        state_changed
    }

    fn prefetch_adjacent(&self) {
        for offset in [-1isize, 1] {
            if let Some(path) = self
                .active_idx
                .checked_add_signed(offset)
                .and_then(|idx| self.get_path_at(idx))
                .filter(|p| !self.is_cached(p))
            {
                self.start_background_load(path.to_string());
            }
        }
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

        let disp_w = ((img_w as f64 * scale) as usize).div_ceil(cell_w);
        let disp_h = ((img_h as f64 * scale) as usize).div_ceil(cell_h);

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

        if let Some(ref path) = self.current_preview_path {
            self.buf.add_change(Change::CursorPosition {
                x: Position::Absolute(preview_col),
                y: Position::Absolute(0),
            });

            let filepath = Path::new(path);
            let filename = filepath
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            self.buf.add_changes(vec![
                AttributeChange::Foreground(self.filename_fg).into(),
                Change::Text(truncate_right(&filename, preview_width)),
                Change::AllAttributes(CellAttributes::default()),
            ]);

            match &self.preview_state {
                PreviewState::None => {}
                PreviewState::Pending | PreviewState::Loading => {
                    self.buf.add_changes(vec![
                        Change::CursorPosition {
                            x: Position::Absolute(preview_col),
                            y: Position::Absolute(1),
                        },
                        AttributeChange::Foreground(self.loading_fg).into(),
                        Change::Text("Loading...".into()),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);
                }
                PreviewState::Loaded { image_data, dims } => {
                    let (img_w, img_h) = *dims;
                    let format = filepath
                        .extension()
                        .map(|e| e.to_string_lossy().to_uppercase())
                        .unwrap_or_else(|| "?".to_string());
                    let file_size = std::fs::metadata(path)
                        .map(|m| format_file_size(m.len()))
                        .unwrap_or_else(|_| "?".to_string());
                    let dimensions = format!("{}×{}", img_w, img_h);

                    self.buf.add_changes(vec![
                        Change::CursorPosition {
                            x: Position::Absolute(preview_col),
                            y: Position::Absolute(1),
                        },
                        AttributeChange::Foreground(self.metadata_fg).into(),
                        Change::Text(dimensions),
                        AttributeChange::Foreground(self.separator_fg).into(),
                        Change::Text(" · ".into()),
                        AttributeChange::Foreground(self.metadata_fg).into(),
                        Change::Text(file_size),
                        AttributeChange::Foreground(self.separator_fg).into(),
                        Change::Text(" · ".into()),
                        AttributeChange::Foreground(self.metadata_fg).into(),
                        Change::Text(format),
                        Change::AllAttributes(CellAttributes::default()),
                    ]);

                    let max_h = rows.saturating_sub(6);
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
                                y: Position::Absolute(3),
                            },
                            Change::Image(Image {
                                width: disp_w,
                                height: disp_h,
                                top_left: TextureCoordinate::new_f32(0.0, 0.0),
                                bottom_right: TextureCoordinate::new_f32(1.0, 1.0),
                                image: Arc::clone(image_data),
                            }),
                        ]);
                    }
                }
                PreviewState::Error(err) => {
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
        let pane = self.pane;
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
        let old_idx = self.active_idx;
        self.active_idx = self.active_idx.saturating_sub(1);
        if self.active_idx < self.top_row {
            self.top_row = self.active_idx;
        }
        if old_idx != self.active_idx {
            self.on_selection_changed();
        }
    }

    fn move_down(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let old_idx = self.active_idx;
        self.active_idx = (self.active_idx + 1).min(self.filtered_indices.len() - 1);
        if self.active_idx > self.top_row + self.max_items {
            self.top_row = self.active_idx.saturating_sub(self.max_items);
        }
        if old_idx != self.active_idx {
            self.on_selection_changed();
        }
    }

    fn move_to_first(&mut self) {
        let old_idx = self.active_idx;
        self.active_idx = 0;
        self.top_row = 0;
        if old_idx != self.active_idx {
            self.on_selection_changed();
        }
    }

    fn move_to_last(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let old_idx = self.active_idx;
        self.active_idx = self.filtered_indices.len() - 1;
        self.top_row = self.active_idx.saturating_sub(self.max_items);
        if old_idx != self.active_idx {
            self.on_selection_changed();
        }
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        let poll_timeout = Some(Duration::from_millis(POLL_TIMEOUT_MS));

        loop {
            let load_completed = self.process_load_results();

            if self.check_and_start_loading() {
                self.prefetch_adjacent();
            }

            match self.buf.terminal().poll_input(poll_timeout) {
                Ok(Some(event)) => {
                    let mut should_render = true;

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
                            key: KeyCode::Char('g'),
                            ..
                        }) if !self.filtering => self.move_to_first(),

                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Char('G'),
                            ..
                        }) if !self.filtering => self.move_to_last(),

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
                                self.on_selection_changed();
                            }
                        }

                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Char('q'),
                            ..
                        }) if !self.filtering => {
                            self.trigger_event(None);
                            break;
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
                            self.on_selection_changed();
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
                            should_render = false;
                        }

                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Enter,
                            ..
                        }) => {
                            if self.filtering {
                                self.filtering = false;
                            } else {
                                should_render = false;
                            }
                        }

                        InputEvent::Resized { cols, rows } => {
                            self.max_items = rows.saturating_sub(ROW_OVERHEAD);
                            self.buf.resize(cols, rows);
                        }

                        _ => should_render = false,
                    }

                    if should_render {
                        self.render()?;
                    }
                }
                Ok(None) => {
                    if load_completed {
                        self.render()?;
                    }
                }
                Err(_) => break,
            }
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

    let (load_sender, load_receiver) = mpsc::channel();

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
        metadata_fg: colors
            .image_selector_metadata_fg
            .map(Into::into)
            .unwrap_or(ColorAttribute::Default),
        filename_fg: colors
            .image_selector_filename_fg
            .map(Into::into)
            .unwrap_or(ColorAttribute::Default),
        loading_fg: colors
            .image_selector_metadata_fg
            .map(Into::into)
            .unwrap_or(ColorAttribute::Default),
        image_cache: Arc::new(Mutex::new(ImageCache::new(IMAGE_CACHE_CAPACITY))),
        buf: &mut buf,
        preview_state: PreviewState::Pending,
        current_preview_path: None,
        last_selection_change: Instant::now(),
        load_receiver,
        load_sender,
        in_flight_loads: Arc::new(Mutex::new(HashSet::new())),
    };

    state
        .buf
        .add_change(Change::Title(state.args.title.to_string()));
    state.buf.flush()?;
    state.update_filter();
    state.on_selection_changed();
    state.render()?;
    state.run_loop()
}
