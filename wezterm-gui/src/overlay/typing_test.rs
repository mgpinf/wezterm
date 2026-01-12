//! Typing test overlay - a terminal typing speed tester inspired by toipe.

use config::keyassignment::{KeyAssignment, TypingTest, TypingTestWordlist};
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use std::fs;
use std::rc::Rc;
use std::time::Instant;
use termwiz::cell::{AttributeChange, CellAttributes, Underline};
use termwiz::color::AnsiColor;
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::surface::Change;
use termwiz::terminal::buffered::BufferedTerminal;
use termwiz::terminal::Terminal;

use crate::scripting::guiwin::GuiWin;

const TOP_250: &str = include_str!("wordlists/top250.txt");
const TOP_500: &str = include_str!("wordlists/top500.txt");
const TOP_1000: &str = include_str!("wordlists/top1000.txt");
const TOP_2500: &str = include_str!("wordlists/top2500.txt");
const TOP_5000: &str = include_str!("wordlists/top5000.txt");
const TOP_10000: &str = include_str!("wordlists/top10000.txt");
const TOP_25000: &str = include_str!("wordlists/top25000.txt");
const COMMONLY_MISSPELLED: &str = include_str!("wordlists/commonly_misspelled.txt");

const PUNCTUATION_CAPITALIZING: &[char] = &['!', '?', '.'];
const PUNCTUATION_ENDING: &[char] = &[',', ':', ';'];
const PUNCTUATION_SURROUNDING: &[(char, char)] = &[
    ('\'', '\''),
    ('"', '"'),
    ('(', ')'),
    ('{', '}'),
    ('<', '>'),
    ('[', ']'),
];

const MAX_WORDS_PER_LINE: usize = 10;

#[derive(Clone, Debug)]
struct TypingTestResults {
    total_words: usize,
    total_chars_typed: usize,
    total_chars_in_text: usize,
    total_char_errors: usize,
    final_chars_typed_correctly: usize,
    final_uncorrected_errors: usize,
    started_at: Instant,
    ended_at: Instant,
}

impl TypingTestResults {
    fn duration_secs(&self) -> f64 {
        self.ended_at.duration_since(self.started_at).as_secs_f64()
    }

    fn accuracy(&self) -> f64 {
        if self.total_chars_typed == 0 {
            return 0.0;
        }
        ((self.total_chars_typed as isize - self.total_char_errors as isize) as f64
            / self.total_chars_typed as f64)
            .max(0.0)
    }

    fn wpm(&self) -> f64 {
        let duration_mins = self.duration_secs() / 60.0;
        if duration_mins <= 0.0 {
            return 0.0;
        }
        ((self.final_chars_typed_correctly as f64 / 5.0 - self.final_uncorrected_errors as f64)
            / duration_mins)
            .max(0.0)
    }
}

struct WordSelector {
    words: Vec<String>,
    punctuation: bool,
    next_is_capital: bool,
}

impl WordSelector {
    fn from_config(args: &TypingTest) -> anyhow::Result<Self> {
        let content = if let Some(ref file_path) = args.wordlist_file {
            fs::read_to_string(file_path).map_err(|e| {
                anyhow::anyhow!("Failed to read wordlist file '{}': {}", file_path, e)
            })?
        } else {
            match args.wordlist {
                TypingTestWordlist::Top250 => TOP_250,
                TypingTestWordlist::Top500 => TOP_500,
                TypingTestWordlist::Top1000 => TOP_1000,
                TypingTestWordlist::Top2500 => TOP_2500,
                TypingTestWordlist::Top5000 => TOP_5000,
                TypingTestWordlist::Top10000 => TOP_10000,
                TypingTestWordlist::Top25000 => TOP_25000,
                TypingTestWordlist::CommonlyMisspelled => COMMONLY_MISSPELLED,
            }
            .to_string()
        };

        Ok(Self {
            words: Self::parse_word_list(&content),
            punctuation: args.punctuation,
            next_is_capital: true,
        })
    }

    fn parse_word_list(content: &str) -> Vec<String> {
        content
            .lines()
            .filter(|line| {
                let len = line.len();
                len >= 2 && len <= 8 && line.chars().all(|c| c.is_ascii_alphabetic())
            })
            .map(|s| s.to_lowercase())
            .collect()
    }

    fn select_words(&mut self, n: usize) -> Vec<String> {
        if self.words.is_empty() {
            return Vec::new();
        }

        self.next_is_capital = true;
        (0..n)
            .map(|_| {
                let word = self.words[fastrand::usize(..self.words.len())].clone();
                if self.punctuation {
                    self.add_punctuation(word)
                } else {
                    word
                }
            })
            .collect()
    }

    fn capitalize(word: &str) -> String {
        let mut chars: Vec<char> = word.chars().collect();
        if let Some(first) = chars.first_mut() {
            *first = first.to_ascii_uppercase();
        }
        chars.into_iter().collect()
    }

    fn add_punctuation(&mut self, word: String) -> String {
        let will_punctuate = fastrand::f64() < 0.15;

        if !will_punctuate && !self.next_is_capital {
            return word;
        }

        let mut result = if self.next_is_capital {
            self.next_is_capital = false;
            Self::capitalize(&word)
        } else {
            word
        };

        if will_punctuate {
            match fastrand::usize(..3) {
                0 => {
                    result.push(
                        PUNCTUATION_CAPITALIZING[fastrand::usize(..PUNCTUATION_CAPITALIZING.len())],
                    );
                    self.next_is_capital = true;
                }
                1 => {
                    result.push(PUNCTUATION_ENDING[fastrand::usize(..PUNCTUATION_ENDING.len())]);
                }
                _ => {
                    let (open, close) =
                        PUNCTUATION_SURROUNDING[fastrand::usize(..PUNCTUATION_SURROUNDING.len())];
                    result = format!("{}{}{}", open, result, close);
                }
            }
        }

        result
    }
}

struct TypingTestState {
    target_chars: Vec<char>,
    input_chars: Vec<char>,
    total_errors: usize,
    total_chars_typed: usize,
    started_at: Option<Instant>,
    num_words: usize,
}

impl TypingTestState {
    fn new(words: &[String]) -> Self {
        Self {
            target_chars: words.join(" ").chars().collect(),
            input_chars: Vec::new(),
            total_errors: 0,
            total_chars_typed: 0,
            started_at: None,
            num_words: words.len(),
        }
    }

    fn is_complete(&self) -> bool {
        self.input_chars.len() >= self.target_chars.len()
    }

    fn get_results(&self) -> TypingTestResults {
        let ended_at = Instant::now();
        let started_at = self.started_at.unwrap_or(ended_at);

        let (final_correct, final_errors) = self
            .input_chars
            .iter()
            .zip(self.target_chars.iter())
            .fold((0, 0), |(c, e), (typed, target)| {
                if typed == target {
                    (c + 1, e)
                } else {
                    (c, e + 1)
                }
            });

        TypingTestResults {
            total_words: self.num_words,
            total_chars_typed: self.total_chars_typed,
            total_chars_in_text: self.target_chars.len(),
            total_char_errors: self.total_errors,
            final_chars_typed_correctly: final_correct,
            final_uncorrected_errors: final_errors,
            started_at,
            ended_at,
        }
    }

    fn delete_last_word(&mut self) {
        while !matches!(self.input_chars.last(), Some(' ') | None) {
            self.input_chars.pop();
        }
    }

    fn type_char(&mut self, c: char) {
        if self.started_at.is_none() {
            self.started_at = Some(Instant::now());
        }
        self.total_chars_typed += 1;

        let pos = self.input_chars.len();
        if pos < self.target_chars.len() && self.target_chars[pos] != c {
            self.total_errors += 1;
        }
        self.input_chars.push(c);
    }
}

enum TestAction {
    Restart,
    Quit,
}

pub fn show_typing_test_overlay(
    term: TermWizTerminal,
    args: TypingTest,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let mut buf = BufferedTerminal::new(term)?;
    buf.terminal().no_grab_mouse_in_raw_mode();

    let mut word_selector = match WordSelector::from_config(&args) {
        Ok(ws) => ws,
        Err(e) => {
            buf.add_changes(vec![
                Change::ClearScreen(Default::default()),
                Change::Text(format!("Error: {}\r\n\r\nPress any key to exit.", e)),
            ]);
            buf.flush()?;
            let _ = buf.terminal().poll_input(None);
            return Ok(());
        }
    };

    let num_words = args.num_words.clamp(5, 100);
    let wordlist_name = get_wordlist_name(&args);

    loop {
        let words = word_selector.select_words(num_words);
        match run_typing_test(&mut buf, words, &wordlist_name)? {
            TestAction::Restart => continue,
            TestAction::Quit => break,
        }
    }

    if let Some(action) = args.action {
        if let KeyAssignment::EmitEvent(name) = *action {
            promise::spawn::spawn_into_main_thread(async move {
                trampoline(name, window, pane);
                anyhow::Result::<()>::Ok(())
            })
            .detach();
        }
    }

    Ok(())
}

fn get_wordlist_name(args: &TypingTest) -> String {
    if let Some(ref file_path) = args.wordlist_file {
        format!("custom file `{}`", file_path)
    } else {
        match args.wordlist {
            TypingTestWordlist::Top250 => "top250",
            TypingTestWordlist::Top500 => "top500",
            TypingTestWordlist::Top1000 => "top1000",
            TypingTestWordlist::Top2500 => "top2500",
            TypingTestWordlist::Top5000 => "top5000",
            TypingTestWordlist::Top10000 => "top10000",
            TypingTestWordlist::Top25000 => "top25000",
            TypingTestWordlist::CommonlyMisspelled => "commonly-misspelled",
        }
        .to_string()
    }
}

fn trampoline(name: String, window: GuiWin, pane: MuxPane) {
    promise::spawn::spawn(async move {
        config::with_lua_config_on_main_thread(move |lua| do_event(lua, name, window, pane)).await
    })
    .detach();
}

async fn do_event(
    lua: Option<Rc<mlua::Lua>>,
    name: String,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    if let Some(lua) = lua {
        let args = lua.pack_multi((window, pane))?;
        if let Err(err) = config::lua::emit_event(&lua, (name.clone(), args)).await {
            log::error!("while processing {} event: {:#}", name, err);
        }
    }
    Ok(())
}

fn run_typing_test(
    buf: &mut BufferedTerminal<TermWizTerminal>,
    words: Vec<String>,
    wordlist_name: &str,
) -> anyhow::Result<TestAction> {
    let mut state = TypingTestState::new(&words);
    render_test_screen(buf, &state)?;

    loop {
        match buf.terminal().poll_input(None) {
            Ok(Some(InputEvent::Key(KeyEvent { key, modifiers }))) => {
                match (modifiers.contains(Modifiers::CTRL), key) {
                    (true, KeyCode::Char('C')) => return Ok(TestAction::Quit),
                    (true, KeyCode::Char('R')) => return Ok(TestAction::Restart),
                    (true, KeyCode::Char('W')) => {
                        state.delete_last_word();
                        render_test_screen(buf, &state)?;
                    }
                    (false, KeyCode::Char(c)) => {
                        state.type_char(c);
                        if state.is_complete() {
                            return show_results(buf, &state.get_results(), wordlist_name);
                        }
                        render_test_screen(buf, &state)?;
                    }
                    (false, KeyCode::Backspace) => {
                        if state.input_chars.pop().is_some() {
                            render_test_screen(buf, &state)?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Some(InputEvent::Resized { .. })) => {
                buf.check_for_resize()?;
                render_test_screen(buf, &state)?;
            }
            Ok(None) | Ok(Some(_)) => {}
            Err(_) => return Ok(TestAction::Quit),
        }
    }
}

fn center_x(width: usize, text_len: usize) -> usize {
    (width.saturating_sub(text_len)) / 2 - 1
}

fn push_bottom_instructions(
    buf: &mut BufferedTerminal<TermWizTerminal>,
    width: usize,
    height: usize,
) {
    let text = "ctrl-r to restart, ctrl-c to quit ";
    buf.add_changes(vec![
        Change::CursorPosition {
            x: termwiz::surface::Position::Absolute(center_x(width, text.len())),
            y: termwiz::surface::Position::Absolute(height - 3),
        },
        AttributeChange::Foreground(AnsiColor::Blue.into()).into(),
        Change::Text("ctrl-r".to_string()),
        Change::AllAttributes(CellAttributes::default()),
        AttributeChange::Intensity(termwiz::cell::Intensity::Half).into(),
        Change::Text(" to restart, ".to_string()),
        Change::AllAttributes(CellAttributes::default()),
        AttributeChange::Foreground(AnsiColor::Blue.into()).into(),
        Change::Text("ctrl-c".to_string()),
        Change::AllAttributes(CellAttributes::default()),
        AttributeChange::Intensity(termwiz::cell::Intensity::Half).into(),
        Change::Text(" to quit ".to_string()),
    ]);
}

fn render_test_screen(
    buf: &mut BufferedTerminal<TermWizTerminal>,
    state: &TypingTestState,
) -> anyhow::Result<()> {
    let (width, height) = buf.dimensions();

    buf.add_changes(vec![
        Change::ClearScreen(Default::default()),
        Change::Title("Typing Test".to_string()),
    ]);

    let target_text: String = state.target_chars.iter().collect();
    let max_line_width = (width * 2 / 5).max(50);
    let wrapped_lines = wrap_text(&target_text, max_line_width);
    let start_y = (height.saturating_sub(wrapped_lines.len())) / 2 - 1;

    let mut char_index = 0;
    for (line_idx, line) in wrapped_lines.iter().enumerate() {
        let line_x = center_x(width, line.chars().count());
        buf.add_change(Change::CursorPosition {
            x: termwiz::surface::Position::Absolute(line_x),
            y: termwiz::surface::Position::Absolute(start_y + line_idx),
        });

        for target_char in line.chars() {
            buf.add_change(Change::AllAttributes(CellAttributes::default()));
            if char_index < state.input_chars.len() {
                if state.input_chars[char_index] == target_char {
                    buf.add_change(AttributeChange::Foreground(AnsiColor::Lime.into()));
                } else {
                    buf.add_changes(vec![
                        AttributeChange::Foreground(AnsiColor::Red.into()).into(),
                        AttributeChange::Underline(Underline::Single).into(),
                    ]);
                }
            } else {
                buf.add_change(AttributeChange::Intensity(termwiz::cell::Intensity::Half));
            }
            buf.add_change(Change::Text(target_char.to_string()));
            char_index += 1;
        }
    }
    buf.add_change(Change::AllAttributes(CellAttributes::default()));

    push_bottom_instructions(buf, width, height);

    if let Some((line_idx, char_in_line)) =
        find_cursor_position(&wrapped_lines, state.input_chars.len())
    {
        let line_x = center_x(width, wrapped_lines[line_idx].chars().count());
        buf.add_change(Change::CursorPosition {
            x: termwiz::surface::Position::Absolute(line_x + char_in_line),
            y: termwiz::surface::Position::Absolute(start_y + line_idx),
        });
    }

    buf.add_changes(vec![
        Change::CursorShape(termwiz::surface::CursorShape::BlinkingBar),
        Change::CursorVisibility(termwiz::surface::CursorVisibility::Visible),
    ]);

    buf.flush()?;
    Ok(())
}

fn find_cursor_position(lines: &[String], cursor_pos: usize) -> Option<(usize, usize)> {
    let mut chars_counted = 0;
    for (line_idx, line) in lines.iter().enumerate() {
        let line_len = line.chars().count();
        if chars_counted + line_len > cursor_pos {
            return Some((line_idx, cursor_pos - chars_counted));
        }
        chars_counted += line_len;
    }
    None
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut words_in_line = 0;

    for word in text.split(' ') {
        if current_line.is_empty() {
            current_line = word.to_string();
            words_in_line = 1;
        } else if words_in_line < MAX_WORDS_PER_LINE
            && current_line.len() + 1 + word.len() <= max_width
        {
            current_line.push(' ');
            current_line.push_str(word);
            words_in_line += 1;
        } else {
            current_line.push(' ');
            lines.push(current_line);
            current_line = word.to_string();
            words_in_line = 1;
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}

fn show_results(
    buf: &mut BufferedTerminal<TermWizTerminal>,
    results: &TypingTestResults,
    wordlist_name: &str,
) -> anyhow::Result<TestAction> {
    let (width, height) = buf.dimensions();
    let center_y = height / 2 - 1;

    buf.add_changes(vec![
        Change::ClearScreen(Default::default()),
        Change::Title("Typing Test Results".to_string()),
    ]);

    let time_text = format!(
        "Took {}s for {} words of {}",
        results.duration_secs().round() as u64,
        results.total_words,
        wordlist_name
    );
    buf.add_changes(vec![
        Change::CursorPosition {
            x: termwiz::surface::Position::Absolute(center_x(width, time_text.len())),
            y: termwiz::surface::Position::Absolute(center_y - 2),
        },
        Change::AllAttributes(CellAttributes::default()),
        Change::Text(time_text),
    ]);

    let acc_text = format!("Accuracy: {:.1}%", results.accuracy() * 100.0);
    buf.add_changes(vec![
        Change::CursorPosition {
            x: termwiz::surface::Position::Absolute(center_x(width, acc_text.len())),
            y: termwiz::surface::Position::Absolute(center_y - 1),
        },
        AttributeChange::Foreground(AnsiColor::Blue.into()).into(),
        Change::Text(acc_text),
        Change::AllAttributes(CellAttributes::default()),
    ]);

    let mistakes_text = format!(
        "Mistakes: {} out of {} characters",
        results.total_char_errors, results.total_chars_in_text
    );
    buf.add_changes(vec![
        Change::CursorPosition {
            x: termwiz::surface::Position::Absolute(center_x(width, mistakes_text.len())),
            y: termwiz::surface::Position::Absolute(center_y),
        },
        Change::Text(mistakes_text),
    ]);

    let speed_prefix = "Speed: ";
    let speed_wpm = format!("{:.1} wpm", results.wpm());
    let speed_suffix = " (words per minute)";
    let speed_len = speed_prefix.len() + speed_wpm.len() + speed_suffix.len();
    buf.add_changes(vec![
        Change::CursorPosition {
            x: termwiz::surface::Position::Absolute(center_x(width, speed_len)),
            y: termwiz::surface::Position::Absolute(center_y + 1),
        },
        Change::Text(speed_prefix.to_string()),
        AttributeChange::Foreground(AnsiColor::Green.into()).into(),
        Change::Text(speed_wpm),
        Change::AllAttributes(CellAttributes::default()),
        Change::Text(speed_suffix.to_string()),
    ]);

    push_bottom_instructions(buf, width, height);
    buf.add_change(Change::CursorVisibility(
        termwiz::surface::CursorVisibility::Hidden,
    ));

    buf.flush()?;

    loop {
        match buf.terminal().poll_input(None) {
            Ok(Some(InputEvent::Key(KeyEvent { key, modifiers }))) => {
                if modifiers.contains(Modifiers::CTRL) {
                    match key {
                        KeyCode::Char('C') => return Ok(TestAction::Quit),
                        KeyCode::Char('R') => return Ok(TestAction::Restart),
                        _ => {}
                    }
                }
            }
            Ok(None) | Ok(Some(_)) => {}
            Err(_) => return Ok(TestAction::Quit),
        }
    }
}
