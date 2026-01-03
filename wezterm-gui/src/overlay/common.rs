use config::{configuration, AnsiColor, ColorAttribute};
use std::collections::HashMap;

/// Generic trie node for keyboard-driven menu navigation
pub struct TrieNode<'a, T> {
    pub children: HashMap<char, Box<TrieNode<'a, T>>>,
    pub entry: Option<&'a T>,
}

impl<'a, T> TrieNode<'a, T> {
    pub fn new() -> Self {
        Self {
            children: HashMap::new(),
            entry: None,
        }
    }

    pub fn add_word(&mut self, word: &str, entry: &'a T) {
        let mut current = self;
        for ch in word.chars() {
            current = current
                .children
                .entry(ch)
                .or_insert_with(|| Box::new(TrieNode::new()));
        }
        current.entry = Some(entry);
    }

    pub fn find_char(&self, c: char) -> Option<&TrieNode<'_, T>> {
        self.children.get(&c).map(|child| child.as_ref())
    }
}

impl<'a, T> Default for TrieNode<'a, T> {
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
