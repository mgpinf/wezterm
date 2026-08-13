use config::{configuration, AnsiColor, ColorAttribute};
use std::collections::HashMap;
use termwiz::surface::Change;
use wezterm_term::{AttributeChange, Intensity};

pub struct KeyMap<'a, T> {
    entries: HashMap<&'a str, &'a T>,
}

pub enum KeyLookup<'a, T> {
    Found(&'a T),
    Prefix,
    NotFound,
}

/// Signal returned by keymap input handlers to control the event loop.
pub enum LoopAction {
    /// Proceed to render.
    Render,
    /// Break out of the event loop.
    Break,
    /// Skip rendering and continue to the next event.
    SkipRender,
}

impl<'a, T> KeyMap<'a, T> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: &'a str, value: &'a T) {
        self.entries.insert(key, value);
    }

    pub fn lookup(&self, typed: &str) -> KeyLookup<'_, T> {
        if let Some(entry) = self.entries.get(typed) {
            return KeyLookup::Found(entry);
        }

        if self.entries.keys().any(|k| k.starts_with(typed)) {
            return KeyLookup::Prefix;
        }

        KeyLookup::NotFound
    }

    pub fn has_continuation(&self, typed: &str, c: char) -> bool {
        let mut test = typed.to_string();
        test.push(c);
        self.entries.keys().any(|k| k.starts_with(&test))
    }
}

impl<'a, T> Default for KeyMap<'a, T> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn display_key(key: &str) -> &str {
    match key {
        " " => "<space>",
        "\n" => "<enter>",
        _ => key,
    }
}

#[derive(Clone, Copy)]
pub struct EntryRenderStyle {
    muted: bool,
    matching_prefix_len: Option<usize>,
}

impl EntryRenderStyle {
    pub fn new(key: &str, prefix: &str) -> Self {
        let has_prefix = !prefix.is_empty();
        let matches_prefix = key.starts_with(prefix);

        Self {
            muted: has_prefix && !matches_prefix,
            matching_prefix_len: (has_prefix && matches_prefix).then_some(prefix.len()),
        }
    }

    pub fn append_key(
        self,
        colors: &OverlayColors,
        key: &str,
        max_key_width: usize,
        changes: &mut Vec<Change>,
    ) {
        let displayed_key = display_key(key);
        let mut padded_key = format!("{:<width$}", displayed_key, width = max_key_width);

        if let Some(prefix_len) = self.displayed_prefix_len(key, displayed_key) {
            let remaining = padded_key.split_off(prefix_len);
            changes.extend([
                Change::Attribute(AttributeChange::Foreground(colors.non_matching_fg)),
                Change::Text(padded_key),
                Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
                Change::Text(remaining),
            ]);
            return;
        }

        changes.extend([
            Change::Attribute(AttributeChange::Foreground(colors.key_fg)),
            Change::Text(padded_key),
        ]);
    }

    fn displayed_prefix_len(self, key: &str, displayed_key: &str) -> Option<usize> {
        self.matching_prefix_len
            .filter(|prefix_len| displayed_key.starts_with(&key[..*prefix_len]))
    }

    pub fn apply(self, colors: &OverlayColors, changes: &mut [Change]) {
        if !self.muted {
            return;
        }

        for change in changes {
            match change {
                Change::Attribute(AttributeChange::Foreground(color)) => {
                    *color = colors.non_matching_fg;
                }
                Change::Attribute(AttributeChange::Intensity(intensity)) => {
                    *intensity = Intensity::Normal;
                }
                Change::AllAttributes(attrs) => {
                    attrs.set_foreground(colors.non_matching_fg);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod prefix_tests {
    use super::{display_key, EntryRenderStyle};

    #[test]
    fn only_mutes_entries_excluded_by_an_active_prefix() {
        assert!(!EntryRenderStyle::new("-f", "").muted);
        assert!(!EntryRenderStyle::new("-f", "-").muted);
        assert!(EntryRenderStyle::new("g", "-").muted);
    }

    #[test]
    fn tracks_the_consumed_prefix_for_active_matches() {
        let style = EntryRenderStyle::new("-f", "-");

        assert_eq!(style.displayed_prefix_len("-f", display_key("-f")), Some(1));

        let special_key_style = EntryRenderStyle::new(" ", " ");
        assert_eq!(
            special_key_style.displayed_prefix_len(" ", display_key(" ")),
            None
        );
    }
}

/// Common colors used by transient-style overlays
pub struct OverlayColors {
    pub key_fg: ColorAttribute,
    pub active_argument_fg: ColorAttribute,
    pub inactive_argument_fg: ColorAttribute,
    pub active_value_fg: ColorAttribute,
    pub non_matching_fg: ColorAttribute,
    pub description_fg: ColorAttribute,
    pub context_label_fg: ColorAttribute,
    pub context_header_fg: ColorAttribute,
    pub section_header_fg: ColorAttribute,
    pub default_value_fg: ColorAttribute,
    pub prompt_label_fg: ColorAttribute,
    pub selector_label_fg: ColorAttribute,
    pub separator_fg: ColorAttribute,
    pub multiple_marker_bg: ColorAttribute,
}

impl OverlayColors {
    pub fn new() -> Self {
        let config = configuration();
        let colors = &config.resolved_palette;

        Self {
            key_fg: colors
                .transient_entry_key_fg
                .unwrap_or(AnsiColor::Purple.into())
                .into(),
            active_argument_fg: colors
                .transient_entry_active_argument_fg
                .unwrap_or(AnsiColor::Red.into())
                .into(),
            inactive_argument_fg: colors
                .transient_entry_inactive_argument_fg
                .map_or(ColorAttribute::Default, |fg_color| fg_color.into()),
            active_value_fg: colors
                .transient_entry_active_value_fg
                .unwrap_or(AnsiColor::Green.into())
                .into(),
            non_matching_fg: colors
                .transient_entry_non_matching_fg
                .unwrap_or(AnsiColor::Grey.into())
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
            default_value_fg: colors
                .transient_default_value_fg
                .unwrap_or(AnsiColor::Green.into())
                .into(),
            prompt_label_fg: colors
                .transient_prompt_label_fg
                .unwrap_or(AnsiColor::Teal.into())
                .into(),
            selector_label_fg: colors
                .transient_selector_label_fg
                .unwrap_or(AnsiColor::Teal.into())
                .into(),
            separator_fg: colors
                .transient_separator_fg
                .map_or(ColorAttribute::Default, |fg_color| fg_color.into()),
            multiple_marker_bg: colors
                .selector_multiple_marker_bg
                .unwrap_or(AnsiColor::Purple.into())
                .into(),
        }
    }
}

impl Default for OverlayColors {
    fn default() -> Self {
        Self::new()
    }
}
