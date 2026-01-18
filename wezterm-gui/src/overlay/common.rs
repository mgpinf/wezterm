use config::{configuration, AnsiColor, ColorAttribute};
use std::collections::HashMap;

pub struct KeyMap<'a, T> {
    entries: HashMap<&'a str, &'a T>,
}

pub enum KeyLookup<'a, T> {
    Found(&'a T),
    Prefix,
    NotFound,
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

/// Common colors used by transient-style overlays
pub struct OverlayColors {
    pub key_fg: ColorAttribute,
    pub active_flag_fg: ColorAttribute,
    pub inactive_flag_fg: ColorAttribute,
    pub active_value_fg: ColorAttribute,
    pub description_fg: ColorAttribute,
    pub context_label_fg: ColorAttribute,
    pub context_header_fg: ColorAttribute,
    pub section_header_fg: ColorAttribute,
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
            active_flag_fg: colors
                .transient_entry_active_flag_fg
                .unwrap_or(AnsiColor::Red.into())
                .into(),
            inactive_flag_fg: colors
                .transient_entry_inactive_flag_fg
                .map_or(ColorAttribute::Default, |fg_color| fg_color.into()),
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
