use crate::config::{ExitBehavior, ExitBehaviorMessaging};
use crate::default_true;
use crate::keys::KeyNoAction;
use crate::window::WindowLevel;
use luahelper::impl_lua_conversion_dynamic;
use ordered_float::NotNan;
use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use wezterm_dynamic::{FromDynamic, FromDynamicOptions, ToDynamic, Value};
use wezterm_input_types::{KeyCode, Modifiers};
use wezterm_term::input::MouseButton;
use wezterm_term::SemanticType;

#[derive(Default, Debug, Clone, FromDynamic, ToDynamic, PartialEq, Eq)]
pub struct LauncherActionArgs {
    pub flags: LauncherFlags,
    pub title: Option<String>,
    pub help_text: Option<String>,
    pub fuzzy_help_text: Option<String>,
    pub alphabet: Option<String>,
}

bitflags::bitflags! {
    #[derive(Default,  FromDynamic, ToDynamic)]
    #[dynamic(try_from="String", into="String")]
    pub struct LauncherFlags :u32 {
        const ZERO = 0;
        const FUZZY = 1;
        const TABS = 2;
        const LAUNCH_MENU_ITEMS = 4;
        const DOMAINS = 8;
        const KEY_ASSIGNMENTS = 16;
        const WORKSPACES = 32;
        const COMMANDS = 64;
    }
}

impl From<LauncherFlags> for String {
    fn from(val: LauncherFlags) -> Self {
        val.to_string()
    }
}

impl From<&LauncherFlags> for String {
    fn from(val: &LauncherFlags) -> Self {
        val.to_string()
    }
}

impl ToString for LauncherFlags {
    fn to_string(&self) -> String {
        let mut s = vec![];
        if self.contains(Self::FUZZY) {
            s.push("FUZZY");
        }
        if self.contains(Self::TABS) {
            s.push("TABS");
        }
        if self.contains(Self::LAUNCH_MENU_ITEMS) {
            s.push("LAUNCH_MENU_ITEMS");
        }
        if self.contains(Self::DOMAINS) {
            s.push("DOMAINS");
        }
        if self.contains(Self::KEY_ASSIGNMENTS) {
            s.push("KEY_ASSIGNMENTS");
        }
        if self.contains(Self::WORKSPACES) {
            s.push("WORKSPACES");
        }
        if self.contains(Self::COMMANDS) {
            s.push("COMMANDS");
        }
        s.join("|")
    }
}

impl TryFrom<String> for LauncherFlags {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        let mut flags = LauncherFlags::default();

        for ele in s.split('|') {
            let ele = ele.trim();
            match ele {
                "FUZZY" => flags |= Self::FUZZY,
                "TABS" => flags |= Self::TABS,
                "LAUNCH_MENU_ITEMS" => flags |= Self::LAUNCH_MENU_ITEMS,
                "DOMAINS" => flags |= Self::DOMAINS,
                "KEY_ASSIGNMENTS" => flags |= Self::KEY_ASSIGNMENTS,
                "WORKSPACES" => flags |= Self::WORKSPACES,
                "COMMANDS" => flags |= Self::COMMANDS,
                _ => {
                    return Err(format!("invalid LauncherFlags `{}` in `{}`", ele, s));
                }
            }
        }

        Ok(flags)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, FromDynamic, ToDynamic)]
pub enum SelectionMode {
    Cell,
    Word,
    Line,
    SemanticZone,
    Block,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum ActivateMatchPosition {
    First,
    AfterCursor,
    BeforeCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum InnerPattern {
    CaseSensitiveString(String),
    CaseInSensitiveString(String),
    Regex(String),
    CurrentSelectionOrEmptyString,
}

/// Pattern for search operations
#[derive(Debug, Clone, PartialEq, Eq, ToDynamic)]
pub struct Pattern {
    pub pattern: InnerPattern,
    #[dynamic(default)]
    pub activate_match: Option<ActivateMatchPosition>,
}

impl Pattern {
    fn inner_pattern_variants() -> &'static [&'static str] {
        &[
            "CaseSensitiveString",
            "CaseInSensitiveString",
            "Regex",
            "CurrentSelectionOrEmptyString",
        ]
    }
}

impl FromDynamic for Pattern {
    fn from_dynamic(
        value: &Value,
        options: FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        match value {
            Value::String(s) => {
                // Handle unit variant: CurrentSelectionOrEmptyString
                if s == "CurrentSelectionOrEmptyString" {
                    return Ok(Self {
                        pattern: InnerPattern::CurrentSelectionOrEmptyString,
                        activate_match: None,
                    });
                }
                Err(wezterm_dynamic::Error::InvalidVariantForType {
                    variant_name: s.clone(),
                    type_name: "Pattern",
                    possible: Self::inner_pattern_variants(),
                })
            }
            Value::Object(obj) => {
                // Check if this is the extended syntax with "pattern" and optionally "activate_match" keys
                // This allows: { pattern = { CaseSensitiveString = "foo" }, activate_match = "First" }
                if obj.get_by_str("pattern").is_some() {
                    let pattern = obj.get_by_str("pattern").ok_or_else(|| {
                        wezterm_dynamic::Error::Message("missing 'pattern' field".to_string())
                    })?;
                    let inner = InnerPattern::from_dynamic(pattern, options)?;
                    let activate_match = match obj.get_by_str("activate_match") {
                        Some(v) => Some(ActivateMatchPosition::from_dynamic(v, options)?),
                        None => None,
                    };
                    return Ok(Self {
                        pattern: inner,
                        activate_match,
                    });
                }

                // Simple syntax: expects single key like { CaseSensitiveString = "..." }
                if obj.len() == 1 {
                    let (name, inner_value): (&Value, &Value) = obj.iter().next().unwrap();
                    match name {
                        Value::String(name) => {
                            let inner = match name.as_str() {
                                "CaseSensitiveString" => InnerPattern::CaseSensitiveString(
                                    String::from_dynamic(inner_value, options)?,
                                ),
                                "CaseInSensitiveString" => InnerPattern::CaseInSensitiveString(
                                    String::from_dynamic(inner_value, options)?,
                                ),
                                "Regex" => {
                                    InnerPattern::Regex(String::from_dynamic(inner_value, options)?)
                                }
                                "CurrentSelectionOrEmptyString" => {
                                    InnerPattern::CurrentSelectionOrEmptyString
                                }
                                _ => {
                                    return Err(wezterm_dynamic::Error::InvalidVariantForType {
                                        variant_name: name.to_string(),
                                        type_name: "Pattern",
                                        possible: Self::inner_pattern_variants(),
                                    })
                                }
                            };
                            Ok(Self {
                                pattern: inner,
                                activate_match: None,
                            })
                        }
                        _ => Err(wezterm_dynamic::Error::InvalidVariantForType {
                            variant_name: name.variant_name().to_string(),
                            type_name: "Pattern",
                            possible: Self::inner_pattern_variants(),
                        }),
                    }
                } else {
                    Err(wezterm_dynamic::Error::IncorrectNumberOfEnumKeys {
                        type_name: "Pattern",
                        num_keys: obj.len(),
                    })
                }
            }
            other => Err(wezterm_dynamic::Error::NoConversion {
                source_type: other.variant_name().to_string(),
                dest_type: "Pattern",
            }),
        }
    }
}

impl Pattern {
    pub fn is_empty(&self) -> bool {
        match &self.pattern {
            InnerPattern::CaseSensitiveString(s)
            | InnerPattern::CaseInSensitiveString(s)
            | InnerPattern::Regex(s) => s.is_empty(),
            InnerPattern::CurrentSelectionOrEmptyString => true,
        }
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            pattern: InnerPattern::CurrentSelectionOrEmptyString,
            activate_match: None,
        }
    }
}

/// A mouse event that can trigger an action
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, FromDynamic, ToDynamic)]
pub enum MouseEventTrigger {
    /// Mouse button is pressed. streak is how many times in a row
    /// it was pressed.
    Down { streak: usize, button: MouseButton },
    /// Mouse button is held down while the cursor is moving. streak is how many times in a row
    /// it was pressed, with the last of those being held to form the drag.
    Drag { streak: usize, button: MouseButton },
    /// Mouse button is being released. streak is how many times
    /// in a row it was pressed and released.
    Up { streak: usize, button: MouseButton },
}

/// When spawning a tab, specify which domain should be used to
/// host/spawn that tab.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum SpawnTabDomain {
    /// Use the default domain
    DefaultDomain,
    /// Use the domain from the current tab in the associated window
    CurrentPaneDomain,
    /// Use a specific domain by name
    DomainName(String),
    /// Use a specific domain by id
    DomainId(usize),
}

impl Default for SpawnTabDomain {
    fn default() -> Self {
        Self::CurrentPaneDomain
    }
}

#[derive(Default, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct SpawnCommand {
    /// Optional descriptive label
    pub label: Option<String>,

    /// The command line to use.
    /// If omitted, the default command associated with the
    /// domain will be used instead, which is typically the
    /// shell for the user.
    pub args: Option<Vec<String>>,

    /// Specifies the current working directory for the command.
    /// If omitted, a default will be used; typically that will
    /// be the home directory of the user, but may also be the
    /// current working directory of the wezterm process when
    /// it was launched, or for some domains it may be some
    /// other location appropriate to the domain.
    pub cwd: Option<PathBuf>,

    /// Specifies a map of environment variables that should be set.
    /// Whether this is used depends on the domain.
    #[dynamic(default)]
    pub set_environment_variables: HashMap<String, String>,

    #[dynamic(default)]
    pub domain: SpawnTabDomain,

    pub position: Option<crate::GuiPosition>,

    /// Specifies the behavior when the spawned process exits.
    /// If omitted, the global `exit_behavior` configuration will be used.
    #[dynamic(default)]
    pub exit_behavior: Option<ExitBehavior>,

    /// Specifies how exit information is displayed when the process exits.
    /// If omitted, the global `exit_behavior_messaging` configuration will be used.
    #[dynamic(default)]
    pub exit_behavior_messaging: Option<ExitBehaviorMessaging>,
}
impl_lua_conversion_dynamic!(SpawnCommand);

impl std::fmt::Debug for SpawnCommand {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}", self)
    }
}

impl std::fmt::Display for SpawnCommand {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "SpawnCommand")?;
        if let Some(label) = &self.label {
            write!(fmt, " label='{}'", label)?;
        }
        write!(fmt, " domain={:?}", self.domain)?;
        if let Some(args) = &self.args {
            write!(fmt, " args={:?}", args)?;
        }
        if let Some(cwd) = &self.cwd {
            write!(fmt, " cwd={}", cwd.display())?;
        }
        for (k, v) in &self.set_environment_variables {
            write!(fmt, " {}={}", k, v)?;
        }
        if let Some(exit_behavior) = &self.exit_behavior {
            write!(fmt, " exit_behavior={:?}", exit_behavior)?;
        }
        if let Some(exit_behavior_messaging) = &self.exit_behavior_messaging {
            write!(
                fmt,
                " exit_behavior_messaging={:?}",
                exit_behavior_messaging
            )?;
        }
        Ok(())
    }
}

impl SpawnCommand {
    pub fn label_for_palette(&self) -> Option<String> {
        if let Some(label) = &self.label {
            Some(label.to_string())
        } else if let Some(args) = &self.args {
            Some(shlex::try_join(args.iter().map(|s| s.as_str())).ok()?)
        } else {
            None
        }
    }

    pub fn from_command_builder(cmd: &CommandBuilder) -> anyhow::Result<Self> {
        let mut args = vec![];
        let mut set_environment_variables = HashMap::new();
        for arg in cmd.get_argv() {
            args.push(
                arg.to_str()
                    .ok_or_else(|| anyhow::anyhow!("command argument is not utf8"))?
                    .to_string(),
            );
        }
        for (k, v) in cmd.iter_full_env_as_str() {
            set_environment_variables.insert(k.to_string(), v.to_string());
        }
        let cwd = match cmd.get_cwd() {
            Some(cwd) => Some(PathBuf::from(cwd)),
            None => None,
        };
        Ok(Self {
            label: None,
            domain: SpawnTabDomain::DefaultDomain,
            args: if args.is_empty() { None } else { Some(args) },
            set_environment_variables,
            cwd,
            position: None,
            exit_behavior: None,
            exit_behavior_messaging: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, FromDynamic, ToDynamic)]
pub enum PaneDirection {
    Up,
    Down,
    Left,
    Right,
    Next,
    Prev,
}

impl PaneDirection {
    pub fn direction_from_str(arg: &str) -> Result<PaneDirection, String> {
        for candidate in PaneDirection::variants() {
            if candidate.to_lowercase() == arg.to_lowercase() {
                if let Ok(direction) = PaneDirection::from_dynamic(
                    &Value::String(candidate.to_string()),
                    FromDynamicOptions::default(),
                ) {
                    return Ok(direction);
                }
            }
        }
        Err(format!(
            "invalid direction {arg}, possible values are {:?}",
            PaneDirection::variants()
        ))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, FromDynamic, ToDynamic, Serialize, Deserialize)]
pub enum ScrollbackEraseMode {
    ScrollbackOnly,
    ScrollbackAndViewport,
}

impl Default for ScrollbackEraseMode {
    fn default() -> Self {
        Self::ScrollbackOnly
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum ClipboardCopyDestination {
    Clipboard,
    PrimarySelection,
    ClipboardAndPrimarySelection,
}
impl_lua_conversion_dynamic!(ClipboardCopyDestination);

impl Default for ClipboardCopyDestination {
    fn default() -> Self {
        Self::ClipboardAndPrimarySelection
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum ClipboardPasteSource {
    Clipboard,
    PrimarySelection,
}

impl Default for ClipboardPasteSource {
    fn default() -> Self {
        Self::Clipboard
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum PaneSelectMode {
    Activate,
    SwapWithActive,
    SwapWithActiveKeepFocus,
    MoveToNewTab,
    MoveToNewWindow,
}

impl Default for PaneSelectMode {
    fn default() -> Self {
        Self::Activate
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, FromDynamic, ToDynamic)]
pub struct PaneSelectArguments {
    /// Overrides the main quick_select_alphabet config
    #[dynamic(default)]
    pub alphabet: String,

    #[dynamic(default)]
    pub mode: PaneSelectMode,

    #[dynamic(default)]
    pub show_pane_ids: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum CharSelectGroup {
    RecentlyUsed,
    SmileysAndEmotion,
    PeopleAndBody,
    AnimalsAndNature,
    FoodAndDrink,
    TravelAndPlaces,
    Activities,
    Objects,
    Symbols,
    Flags,
    NerdFonts,
    UnicodeNames,
    ShortCodes,
}

// next is default, previous is the reverse
macro_rules! char_select_group_impl_next_prev {
    ($($x:ident => $y:ident),+ $(,)?) => {
        impl CharSelectGroup {
            pub const fn next(self) -> Self {
                match self {
                    $(CharSelectGroup::$x => CharSelectGroup::$y),+
                }
            }

            pub const fn previous(self) -> Self {
                match self {
                    $(CharSelectGroup::$y => CharSelectGroup::$x),+
                }
            }
        }
    };
}

char_select_group_impl_next_prev! (
    RecentlyUsed => SmileysAndEmotion,
    SmileysAndEmotion => PeopleAndBody,
    PeopleAndBody => AnimalsAndNature,
    AnimalsAndNature => FoodAndDrink,
    FoodAndDrink => TravelAndPlaces,
    TravelAndPlaces => Activities,
    Activities => Objects,
    Objects => Symbols,
    Symbols => Flags,
    Flags => NerdFonts,
    NerdFonts => UnicodeNames,
    UnicodeNames => ShortCodes,
    ShortCodes => RecentlyUsed,
);

impl Default for CharSelectGroup {
    fn default() -> Self {
        Self::SmileysAndEmotion
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromDynamic, ToDynamic)]
pub struct CharSelectArguments {
    #[dynamic(default)]
    pub group: Option<CharSelectGroup>,
    #[dynamic(default = "default_true")]
    pub copy_on_select: bool,
    #[dynamic(default)]
    pub copy_to: ClipboardCopyDestination,
}

impl Default for CharSelectArguments {
    fn default() -> Self {
        Self {
            group: None,
            copy_on_select: true,
            copy_to: ClipboardCopyDestination::default(),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct QuickSelectArguments {
    /// Overrides the main quick_select_alphabet config
    #[dynamic(default)]
    pub alphabet: String,
    /// Overrides the main quick_select_patterns config
    #[dynamic(default)]
    pub patterns: Vec<String>,
    #[dynamic(default)]
    pub action: Option<Box<KeyAssignment>>,
    /// Skip triggering `action` after paste is performed (capital selection)
    #[dynamic(default)]
    pub skip_action_on_paste: bool,
    /// Label to use in place of "copy" when `action` is set
    #[dynamic(default)]
    pub label: String,
    /// How many lines before and how many lines after the viewport to
    /// search to produce the quickselect results
    pub scope_lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct PromptInputLine {
    pub action: Box<KeyAssignment>,
    /// Optional label to pre-fill the input line with
    #[dynamic(default)]
    pub initial_value: Option<String>,
    /// Descriptive text to show ahead of prompt
    #[dynamic(default)]
    pub description: String,
    /// Text to show for prompt
    #[dynamic(default = "default_prompt")]
    pub prompt: String,
}

fn default_prompt() -> String {
    "> ".to_string()
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct InputSelectorEntry {
    pub label: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct InputSelector {
    pub action: Box<KeyAssignment>,
    #[dynamic(default)]
    pub title: String,

    pub choices: Vec<InputSelectorEntry>,

    #[dynamic(default)]
    pub fuzzy: bool,

    #[dynamic(default = "default_num_alphabet")]
    pub alphabet: String,

    #[dynamic(default = "default_description")]
    pub description: String,

    #[dynamic(default = "default_fuzzy_description")]
    pub fuzzy_description: String,
}

fn default_num_alphabet() -> String {
    "1234567890abcdefghilmnopqrstuvwxyz".to_string()
}

fn default_description() -> String {
    "Select an item and press Enter = accept,  Esc = cancel,  / = filter".to_string()
}

fn default_fuzzy_description() -> String {
    "Fuzzy matching: ".to_string()
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct Confirmation {
    pub action: Box<KeyAssignment>,
    #[dynamic(default)]
    pub cancel: Option<Box<KeyAssignment>>,
    /// Text to show for confirmation
    #[dynamic(default = "default_message")]
    pub message: String,
}

fn default_message() -> String {
    "🛑 Really continue?".to_string()
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientSwitch {
    pub key: String,
    #[dynamic(default)]
    pub default: bool,
    pub description: String,
    pub flag: String,
    #[dynamic(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientCyclicSwitch {
    pub key: String,
    #[dynamic(default)]
    pub default: Option<String>,
    pub description: String,
    pub flag: String,
    pub choices: Vec<String>,
    #[dynamic(default = "crate::default_true")]
    pub allow_nil: bool,
    #[dynamic(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientOption {
    pub key: String,
    #[dynamic(default)]
    pub default: Option<String>,
    pub description: String,
    pub flag: String,
    #[dynamic(default = "crate::default_true")]
    pub allow_nil: bool,
    #[dynamic(default)]
    pub choices: Option<Vec<String>>,
    #[dynamic(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientArgument {
    pub key: String,
    pub description: String,
    pub action: Box<KeyAssignment>,
}

#[derive(Debug, Clone, PartialEq, ToDynamic)]
pub enum TransientEntry {
    TransientSwitch(TransientSwitch),
    TransientOption(TransientOption),
    TransientCyclicSwitch(TransientCyclicSwitch),
    TransientArgument(TransientArgument),
}

impl FromDynamic for TransientEntry {
    fn from_dynamic(
        value: &Value,
        options: FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        match value {
            Value::Object(obj) => {
                let type_value = obj.get_by_str("type").ok_or_else(|| {
                    wezterm_dynamic::Error::Message(
                        "TransientEntry requires a 'type' field".to_string(),
                    )
                })?;

                let type_name = match type_value {
                    Value::String(s) => s.as_str(),
                    _ => {
                        return Err(wezterm_dynamic::Error::Message(
                            "'type' field must be a string".to_string(),
                        ))
                    }
                };

                // Use flatten() to ignore the 'type' field when parsing inner structs
                let inner_options = options.flatten();

                match type_name {
                    "switch" => Ok(Self::TransientSwitch(TransientSwitch::from_dynamic(
                        value,
                        inner_options,
                    )?)),
                    "option" => Ok(Self::TransientOption(TransientOption::from_dynamic(
                        value,
                        inner_options,
                    )?)),
                    "cyclic" => Ok(Self::TransientCyclicSwitch(
                        TransientCyclicSwitch::from_dynamic(value, inner_options)?,
                    )),
                    "argument" => Ok(Self::TransientArgument(TransientArgument::from_dynamic(
                        value,
                        inner_options,
                    )?)),
                    _ => Err(wezterm_dynamic::Error::InvalidVariantForType {
                        variant_name: type_name.to_string(),
                        type_name: "TransientEntry",
                        possible: &["switch", "option", "cyclic", "argument"],
                    }),
                }
            }
            _ => Err(wezterm_dynamic::Error::Message(
                "TransientEntry must be an object".to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientSection {
    pub header: String,
    pub entries: Vec<TransientEntry>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientContextEntry {
    pub label: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientContext {
    pub header: String,
    pub entries: Vec<TransientContextEntry>,
}

#[derive(Debug, Clone, PartialEq, ToDynamic)]
pub struct TransientMenu {
    pub description: String,
    #[dynamic(default)]
    pub context: Option<TransientContext>,
    pub sections: Vec<TransientSection>,
    #[dynamic(default)]
    pub cancel: Option<Box<KeyAssignment>>,
}

impl FromDynamic for TransientMenu {
    fn from_dynamic(
        value: &Value,
        options: FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        let obj = match value {
            Value::Object(obj) => obj,
            _ => {
                return Err(wezterm_dynamic::Error::Message(
                    "TransientMenu must be an object".to_string(),
                ))
            }
        };

        let get_required = |field: &str| {
            obj.get_by_str(field).ok_or_else(|| {
                wezterm_dynamic::Error::Message(format!(
                    "TransientMenu requires a '{}' field",
                    field
                ))
            })
        };

        let description = String::from_dynamic(get_required("description")?, options)?;

        let context = obj
            .get_by_str("context")
            .map(|v| TransientContext::from_dynamic(v, options))
            .transpose()?;

        let cancel = obj
            .get_by_str("cancel")
            .map(|v| Box::<KeyAssignment>::from_dynamic(v, options))
            .transpose()?;

        let entries_val = obj.get_by_str("entries");
        let sections_val = obj.get_by_str("sections");
        let header_val = obj.get_by_str("header");

        let sections = match (entries_val, sections_val, header_val) {
            (Some(_), Some(_), _) => {
                return Err(wezterm_dynamic::Error::Message(
                    "TransientMenu cannot have both 'sections' and 'entries'".to_string(),
                ));
            }
            (None, Some(_), Some(_)) => {
                return Err(wezterm_dynamic::Error::Message(
                    "TransientMenu cannot have 'header' with 'sections'".to_string(),
                ));
            }
            (Some(entries_val), None, header_val) => {
                let entries = Vec::<TransientEntry>::from_dynamic(entries_val, options)?;
                let header = header_val
                    .map(|v| String::from_dynamic(v, options))
                    .transpose()?
                    .unwrap_or_default();
                vec![TransientSection { header, entries }]
            }
            (None, Some(sections_val), None) => {
                Vec::<TransientSection>::from_dynamic(sections_val, options)?
            }
            (None, None, _) => {
                return Err(wezterm_dynamic::Error::Message(
                    "TransientMenu requires 'sections' or 'entries'".to_string(),
                ));
            }
        };

        Ok(Self {
            description,
            context,
            sections,
            cancel,
        })
    }
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct ArgumentSection {
    #[dynamic(default)]
    pub header: Option<String>,
    pub arguments: Vec<TransientArgument>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct SelectorActions {
    pub description: String,
    #[dynamic(default)]
    pub context: Option<TransientContext>,
    pub choices: Vec<InputSelectorEntry>,
    pub section: ArgumentSection,
    #[dynamic(default)]
    pub multiple: bool,
    #[dynamic(default)]
    pub fuzzy_description: Option<String>,
    #[dynamic(default)]
    pub fuzzy: bool,
    #[dynamic(default)]
    pub cancel: Option<Box<KeyAssignment>>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct DisplayText {
    pub text: String,
}

/// A single command to run in the CommandRunner overlay.
/// Uses similar syntax to `wezterm.run_child_process` where `args` contains
/// the command as the first element followed by its arguments.
#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct CommandRunnerCommand {
    /// Display title for this command (defaults to args[0] if not specified)
    #[dynamic(default)]
    pub title: Option<String>,
    /// The command and its arguments (first element is the program)
    pub args: Vec<String>,
    #[dynamic(default)]
    pub cwd: Option<String>,
    #[dynamic(default)]
    pub set_environment_variables: HashMap<String, String>,
}

/// Configuration for the CommandRunner overlay
#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct CommandRunner {
    pub commands: Vec<CommandRunnerCommand>,
    #[dynamic(default)]
    pub auto_close_on_success: bool,
}

/// Built-in word lists for the typing test (matches toipe's wordlists)
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic, Default)]
pub enum TypingTestWordlist {
    /// Top 250 most common English words
    #[default]
    Top250,
    /// Top 500 most common English words
    Top500,
    /// Top 1000 most common English words
    Top1000,
    /// Top 2500 most common English words
    Top2500,
    /// Top 5000 most common English words
    Top5000,
    /// Top 10000 most common English words
    Top10000,
    /// Top 25000 most common English words
    Top25000,
    /// Commonly misspelled English words
    CommonlyMisspelled,
}

/// Configuration for the TypingTest overlay
#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TypingTest {
    /// Word list name (default: Top250)
    /// Matches toipe's -w/--wordlist argument
    #[dynamic(default)]
    pub wordlist: TypingTestWordlist,
    /// Path to custom word list file
    /// Matches toipe's -f/--file argument
    #[dynamic(default)]
    pub wordlist_file: Option<String>,
    /// Number of words to show on each test (default: 30)
    /// Matches toipe's -n/--num-words argument
    #[dynamic(default = "default_typing_test_num_words")]
    pub num_words: usize,
    /// Whether to include punctuation (default: false)
    /// Matches toipe's -p/--punctuation argument
    #[dynamic(default)]
    pub punctuation: bool,
    /// Optional callback action when test completes
    #[dynamic(default)]
    pub action: Option<Box<KeyAssignment>>,
}

fn default_typing_test_num_words() -> usize {
    30
}

/// An entry for the ImageSelector overlay
#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct ImageSelectorEntry {
    /// Display label for the image (defaults to filename if not specified)
    #[dynamic(default)]
    pub label: Option<String>,
    /// Path to the image file
    pub path: String,
}

/// Configuration for the ImageSelector overlay
#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct ImageSelector {
    /// Callback action when an image is selected
    pub action: Box<KeyAssignment>,
    /// Title for the overlay
    #[dynamic(default)]
    pub title: String,
    /// List of image choices
    pub choices: Vec<ImageSelectorEntry>,
    /// Whether to start in fuzzy finding mode
    #[dynamic(default)]
    pub fuzzy: bool,
    /// Description shown in the overlay (required)
    pub description: String,
    /// Description shown in fuzzy mode (falls back to description if not provided)
    #[dynamic(default)]
    pub fuzzy_description: Option<String>,
}

/// A choice option for selector fields in InputForm
#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct FormFieldChoice {
    /// Display label for the choice
    pub label: String,
    /// Value to submit (defaults to label if not specified)
    #[dynamic(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct FormField {
    pub label: String,
    pub id: String,
    #[dynamic(default)]
    pub placeholder: Option<String>,
    #[dynamic(default)]
    pub is_password: bool,
    #[dynamic(default)]
    pub initial_value: Option<String>,
    #[dynamic(default)]
    pub required: bool,
    /// If non-empty, this field becomes a selector with fuzzy search
    #[dynamic(default)]
    pub choices: Vec<FormFieldChoice>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct InputForm {
    pub title: String,
    pub fields: Vec<FormField>,
    pub action: Box<KeyAssignment>,
    #[dynamic(default)]
    pub submit_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct InputText {
    #[dynamic(default)]
    pub title: Option<String>,
    #[dynamic(default)]
    pub initial_value: Option<String>,
    pub action: Box<KeyAssignment>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub enum KeyAssignment {
    SpawnTab(SpawnTabDomain),
    SpawnWindow,
    ToggleFullScreen,
    ToggleAlwaysOnTop,
    ToggleAlwaysOnBottom,
    SetWindowLevel(WindowLevel),
    CopyTo(ClipboardCopyDestination),
    CopyTextTo {
        text: String,
        destination: ClipboardCopyDestination,
    },
    PasteFrom(ClipboardPasteSource),
    ActivateTabRelative(isize),
    ActivateTabRelativeNoWrap(isize),
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    ResetFontAndWindowSize,
    ActivateTab(isize),
    ActivateLastTab,
    SendString(String),
    SendKey(KeyNoAction),
    Nop,
    DisableDefaultAssignment,
    Hide,
    Show,
    CloseCurrentTab {
        confirm: bool,
    },
    ReloadConfiguration,
    MoveTabRelative(isize),
    MoveTab(usize),
    ScrollByPage(NotNan<f64>),
    ScrollByLine(isize),
    ScrollByCurrentEventWheelDelta,
    ScrollToPrompt(isize),
    ScrollToTop,
    ScrollToBottom,
    ShowTabNavigator,
    ShowDebugOverlay,
    HideApplication,
    QuitApplication,
    SpawnCommandInNewTab(SpawnCommand),
    SpawnCommandInNewWindow(SpawnCommand),
    SpawnCommandInFloatingPane(FloatingPaneSpawn),
    ToggleFloatingPane,
    SplitHorizontal(SpawnCommand),
    SplitVertical(SpawnCommand),
    ShowLauncher,
    ShowLauncherArgs(LauncherActionArgs),
    ClearScrollback(ScrollbackEraseMode),
    Search(Pattern),
    ActivateCopyMode,

    SelectTextAtMouseCursor(SelectionMode),
    ExtendSelectionToMouseCursor(SelectionMode),
    OpenLinkAtMouseCursor,
    ClearSelection,
    CompleteSelection(ClipboardCopyDestination),
    CompleteSelectionOrOpenLinkAtMouseCursor(ClipboardCopyDestination),
    StartWindowDrag,

    AdjustPaneSize(PaneDirection, usize),
    ActivatePaneDirection(PaneDirection),
    ActivatePaneByIndex(usize),
    TogglePaneZoomState,
    SetPaneZoomState(bool),
    CloseCurrentPane {
        confirm: bool,
    },
    EmitEvent(String),
    QuickSelect,
    QuickSelectArgs(QuickSelectArguments),

    Multiple(Vec<KeyAssignment>),

    SwitchToWorkspace {
        name: Option<String>,
        spawn: Option<SpawnCommand>,
    },
    SwitchWorkspaceRelative(isize),

    ActivateKeyTable {
        name: String,
        #[dynamic(default)]
        timeout_milliseconds: Option<u64>,
        #[dynamic(default)]
        replace_current: bool,
        #[dynamic(default = "crate::default_true")]
        one_shot: bool,
        #[dynamic(default)]
        until_unknown: bool,
        #[dynamic(default)]
        prevent_fallback: bool,
    },
    PopKeyTable,
    ClearKeyTableStack,
    DetachDomain(SpawnTabDomain),
    AttachDomain(String),

    CopyMode(CopyModeAssignment),
    RotatePanes(RotationDirection),
    SplitPane(SplitPane),
    PaneSelect(PaneSelectArguments),
    CharSelect(CharSelectArguments),

    ResetTerminal,
    OpenUri(String),
    ActivateCommandPalette,
    ActivateWindow(usize),
    ActivateWindowRelative(isize),
    ActivateWindowRelativeNoWrap(isize),
    PromptInputLine(PromptInputLine),
    InputSelector(InputSelector),
    ImageSelector(ImageSelector),
    InputForm(InputForm),
    InputText(InputText),
    Confirmation(Confirmation),
    TransientMenu(TransientMenu),
    SelectorActions(SelectorActions),
    DisplayText(DisplayText),
    TypingTest(TypingTest),
    ScrollbackSearchWithContext(ScrollbackSearchWithContextArgs),
    CommandRunner(CommandRunner),
}

#[derive(Debug, Clone, PartialEq, Eq, FromDynamic, ToDynamic, Default)]
pub struct ScrollbackSearchWithContextArgs {
    /// If true, automatically refresh search results when buffer content changes.
    /// Can be toggled with Ctrl+R while the overlay is open.
    #[dynamic(default)]
    pub auto_refresh: bool,
}
impl_lua_conversion_dynamic!(ScrollbackSearchWithContextArgs);
impl_lua_conversion_dynamic!(KeyAssignment);

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct SplitPane {
    pub direction: PaneDirection,
    #[dynamic(default)]
    pub size: SplitSize,
    #[dynamic(default)]
    pub command: SpawnCommand,
    #[dynamic(default)]
    pub top_level: bool,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct FloatingPaneSpawn {
    #[dynamic(flatten)]
    pub command: SpawnCommand,
    /// If true, replace the current floating pane (if any) with this new one.
    /// If false (default), do nothing if a floating pane already exists.
    #[dynamic(default)]
    pub replace_current: bool,
}
impl_lua_conversion_dynamic!(FloatingPaneSpawn);

#[derive(Debug, Clone, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum SplitSize {
    Cells(usize),
    Percent(u8),
}

impl Default for SplitSize {
    fn default() -> Self {
        Self::Percent(50)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum RotationDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug, Clone, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum CopyModeAssignment {
    MoveToViewportBottom,
    MoveToViewportTop,
    MoveToViewportMiddle,
    MoveToScrollbackTop,
    MoveToScrollbackBottom,
    SetSelectionMode(Option<SelectionMode>),
    ClearSelectionMode,
    MoveToStartOfLineContent,
    MoveToEndOfLineContent,
    MoveToStartOfLine,
    MoveToStartOfNextLine,
    MoveToSelectionOtherEnd,
    MoveToSelectionOtherEndHoriz,
    MoveBackwardWord,
    MoveBackwardLongWord,
    MoveForwardWord,
    MoveForwardLongWord,
    MoveForwardLongWordEnd,
    MoveForwardWordEnd,
    MoveRight,
    MoveLeft,
    MoveUp,
    MoveDown,
    MoveByPage(NotNan<f64>),
    PageUp,
    PageDown,
    Close,
    PriorMatch,
    NextMatch,
    PriorMatchPage,
    NextMatchPage,
    CycleMatchType,
    ClearPattern,
    EditPattern,
    AcceptPattern,
    MoveBackwardSemanticZone,
    MoveForwardSemanticZone,
    MoveBackwardZoneOfType(SemanticType),
    MoveForwardZoneOfType(SemanticType),
    JumpForward { prev_char: bool },
    JumpBackward { prev_char: bool },
    JumpAgain,
    JumpReverse,
    JumpToMatchingBracket,
}

pub type KeyTable = HashMap<(KeyCode, Modifiers), KeyTableEntry>;

#[derive(Debug, Clone, Default)]
pub struct KeyTables {
    pub default: KeyTable,
    pub by_name: HashMap<String, KeyTable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyTableEntry {
    pub action: KeyAssignment,
}
