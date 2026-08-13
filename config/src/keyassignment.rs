use crate::config::{ExitBehavior, ExitBehaviorMessaging};
use crate::default_true;
use crate::keys::KeyNoAction;
use crate::window::WindowLevel;
use crate::ColorSpec;
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

fn default_delimiter() -> String {
    ".".to_string()
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, PartialEq, Eq)]
pub struct LauncherActionArgs {
    pub flags: LauncherFlags,
    pub title: Option<String>,
    pub help_text: Option<String>,
    pub fuzzy_help_text: Option<String>,
    pub alphabet: Option<String>,
    #[dynamic(default = "default_delimiter")]
    pub delimiter: String,
    #[dynamic(default)]
    pub pane_count_in_suffix: bool,
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
}

impl Default for LauncherActionArgs {
    fn default() -> Self {
        Self {
            flags: LauncherFlags::default(),
            title: None,
            help_text: None,
            fuzzy_help_text: None,
            alphabet: None,
            delimiter: default_delimiter(),
            pane_count_in_suffix: false,
            dimensions: OverlayDimensions::default(),
            border: false,
            border_color: None,
        }
    }
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, PartialEq, Eq)]
pub struct ShowTabNavigatorArgs {
    #[dynamic(default)]
    pub title: Option<String>,
    #[dynamic(default)]
    pub help_text: Option<String>,
    #[dynamic(default)]
    pub fuzzy_help_text: Option<String>,
    #[dynamic(default)]
    pub fuzzy: bool,
    #[dynamic(default = "default_delimiter")]
    pub delimiter: String,
    #[dynamic(default)]
    pub pane_count_in_suffix: bool,
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
}

impl Default for ShowTabNavigatorArgs {
    fn default() -> Self {
        Self {
            title: None,
            help_text: None,
            fuzzy_help_text: None,
            fuzzy: false,
            delimiter: default_delimiter(),
            pane_count_in_suffix: false,
            dimensions: OverlayDimensions::default(),
            border: false,
            border_color: None,
        }
    }
}

#[derive(Default, Debug, Clone, FromDynamic, ToDynamic, PartialEq, Eq)]
pub struct ShowDebugOverlayArgs {
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
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
    /// A "command block": the contiguous run of Prompt + Input + Output
    /// semantic zones produced by a single shell command (as reported via
    /// OSC 133). Snaps to the bounds of the enclosing block.
    CommandBlock,
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
    CaseSmartString(String),
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
            "CaseSmartString",
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
                                "CaseSmartString" => InnerPattern::CaseSmartString(
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
            | InnerPattern::CaseSmartString(s)
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
            if candidate.eq_ignore_ascii_case(arg) {
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
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
    #[dynamic(default)]
    pub hide_description: bool,
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

    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
}

fn default_num_alphabet() -> String {
    "1234567890abcdefhilmnopstuvwxyz".to_string()
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
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
}

fn default_message() -> String {
    "🛑 Really continue?".to_string()
}

/// A single command to run in the CommandRunner overlay.
/// Uses similar syntax to `wezterm.run_child_process` where `args` contains
/// the command as the first element followed by its arguments.
#[derive(Debug, Clone, PartialEq, ToDynamic)]
pub struct CommandRunnerCommand {
    /// Display title for this command (defaults to args[0] if not specified)
    pub title: Option<String>,
    /// The command and its arguments (first element is the program)
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub set_environment_variables: HashMap<String, String>,
}

impl FromDynamic for CommandRunnerCommand {
    fn from_dynamic(
        value: &wezterm_dynamic::Value,
        options: wezterm_dynamic::FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        match value {
            // Simple syntax: {"ls", "-la"} - just an array of strings
            wezterm_dynamic::Value::Array(_) => {
                let args = Vec::<String>::from_dynamic(value, options)?;
                if args.is_empty() {
                    return Err(wezterm_dynamic::Error::Message(
                        "CommandRunnerCommand 'args' must not be empty".to_string(),
                    ));
                }
                Ok(Self {
                    title: None,
                    args,
                    cwd: None,
                    set_environment_variables: HashMap::new(),
                })
            }
            // Extended syntax: {args = {"ls", "-la"}, cwd = "/tmp", ...}
            wezterm_dynamic::Value::Object(obj) => {
                let title = obj
                    .get_by_str("title")
                    .map(|v| String::from_dynamic(v, options))
                    .transpose()?;

                let args: Vec<String> = obj
                    .get_by_str("args")
                    .map(|v| Vec::<String>::from_dynamic(v, options))
                    .transpose()?
                    .unwrap_or_default();

                if args.is_empty() {
                    return Err(wezterm_dynamic::Error::Message(
                        "CommandRunnerCommand 'args' must not be empty".to_string(),
                    ));
                }

                let cwd = obj
                    .get_by_str("cwd")
                    .map(|v| String::from_dynamic(v, options))
                    .transpose()?;

                let set_environment_variables = obj
                    .get_by_str("set_environment_variables")
                    .map(|v| HashMap::<String, String>::from_dynamic(v, options))
                    .transpose()?
                    .unwrap_or_default();

                Ok(Self {
                    title,
                    args,
                    cwd,
                    set_environment_variables,
                })
            }
            _ => Err(wezterm_dynamic::Error::Message(
                "CommandRunnerCommand must be an array of strings or an object with 'args' field"
                    .to_string(),
            )),
        }
    }
}

/// Configuration for the CommandRunner overlay
#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct CommandRunner {
    pub commands: Vec<CommandRunnerCommand>,
    #[dynamic(default)]
    pub auto_close_on_success: bool,
    #[dynamic(default = "default_num_alphabet")]
    pub alphabet: String,
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientSwitch {
    pub key: String,
    #[dynamic(default)]
    pub default: bool,
    pub description: String,
    pub flag: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransientOptionInput {
    Prompt,
    Select,
    Cycle,
}

impl FromDynamic for TransientOptionInput {
    fn from_dynamic(
        value: &Value,
        _options: FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        match value {
            Value::String(value) => match value.as_str() {
                "prompt" => Ok(Self::Prompt),
                "select" => Ok(Self::Select),
                "cycle" => Ok(Self::Cycle),
                _ => Err(wezterm_dynamic::Error::InvalidVariantForType {
                    variant_name: value.clone(),
                    type_name: "TransientOptionInput",
                    possible: &["prompt", "select", "cycle"],
                }),
            },
            _ => Err(wezterm_dynamic::Error::Message(
                "TransientOption 'input' field must be a string".to_string(),
            )),
        }
    }
}

impl ToDynamic for TransientOptionInput {
    fn to_dynamic(&self) -> Value {
        Value::String(
            match self {
                Self::Prompt => "prompt",
                Self::Select => "select",
                Self::Cycle => "cycle",
            }
            .to_string(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientOption {
    pub key: String,
    #[dynamic(default)]
    pub default: Option<String>,
    pub description: String,
    pub flag: String,
    #[dynamic(default = "crate::default_true")]
    pub allow_unset: bool,
    #[dynamic(default)]
    pub choices: Option<Vec<String>>,
    #[dynamic(default)]
    pub input: Option<TransientOptionInput>,
}

impl TransientOption {
    pub fn resolved_input(&self) -> TransientOptionInput {
        self.input.unwrap_or_else(|| {
            if self.choices.is_some() {
                TransientOptionInput::Select
            } else {
                TransientOptionInput::Prompt
            }
        })
    }

    fn validate(&self) -> Result<(), wezterm_dynamic::Error> {
        let input = self.resolved_input();
        match input {
            TransientOptionInput::Prompt if self.choices.is_some() => {
                Err(wezterm_dynamic::Error::Message(
                    "TransientOption with input='prompt' cannot define choices".to_string(),
                ))
            }
            TransientOptionInput::Select | TransientOptionInput::Cycle => {
                let input_name = match input {
                    TransientOptionInput::Select => "select",
                    TransientOptionInput::Cycle => "cycle",
                    TransientOptionInput::Prompt => unreachable!(),
                };
                let Some(choices) = self.choices.as_ref().filter(|choices| !choices.is_empty())
                else {
                    return Err(wezterm_dynamic::Error::Message(format!(
                        "TransientOption with input='{}' requires a non-empty choices list",
                        input_name
                    )));
                };

                if let Some(default) = self.default.as_ref() {
                    if !choices.contains(default) {
                        return Err(wezterm_dynamic::Error::Message(format!(
                            "TransientOption default '{}' is not present in choices",
                            default
                        )));
                    }
                }

                Ok(())
            }
            TransientOptionInput::Prompt => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct TransientAction {
    pub key: String,
    pub description: String,
    pub action: Box<KeyAssignment>,
    #[dynamic(default)]
    pub keep_overlay: bool,
}

#[derive(Debug, Clone, PartialEq, ToDynamic)]
pub enum TransientEntry {
    TransientSwitch(TransientSwitch),
    TransientOption(TransientOption),
    TransientAction(TransientAction),
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
                    "option" => {
                        let option = TransientOption::from_dynamic(value, inner_options)?;
                        option.validate()?;
                        Ok(Self::TransientOption(option))
                    }
                    "action" => Ok(Self::TransientAction(TransientAction::from_dynamic(
                        value,
                        inner_options,
                    )?)),
                    _ => Err(wezterm_dynamic::Error::InvalidVariantForType {
                        variant_name: type_name.to_string(),
                        type_name: "TransientEntry",
                        possible: &["switch", "option", "action"],
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
    pub title: String,
    #[dynamic(default)]
    pub context: Option<TransientContext>,
    pub sections: Vec<TransientSection>,
    #[dynamic(default)]
    pub cancel: Option<Box<KeyAssignment>>,
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
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

        let title = obj
            .get_by_str("title")
            .map(|v| String::from_dynamic(v, options))
            .transpose()?
            .unwrap_or_default();

        let context = obj
            .get_by_str("context")
            .map(|v| TransientContext::from_dynamic(v, options))
            .transpose()?;

        let cancel = obj
            .get_by_str("cancel")
            .map(|v| Box::<KeyAssignment>::from_dynamic(v, options))
            .transpose()?;

        let dimensions = obj
            .get_by_str("dimensions")
            .map(|v| OverlayDimensions::from_dynamic(v, options))
            .transpose()?
            .unwrap_or_default();

        let border = obj
            .get_by_str("border")
            .map(|v| bool::from_dynamic(v, options))
            .transpose()?
            .unwrap_or_default();

        let border_color = obj
            .get_by_str("border_color")
            .map(|v| ColorSpec::from_dynamic(v, options))
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
            title,
            context,
            sections,
            cancel,
            dimensions,
            border,
            border_color,
        })
    }
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct SelectorActionSection {
    #[dynamic(default)]
    pub header: Option<String>,
    pub actions: Vec<TransientAction>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct SelectorActionsEntry {
    pub label: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct SelectorActions {
    pub description: String,
    #[dynamic(default)]
    pub title: String,
    #[dynamic(default)]
    pub context: Option<TransientContext>,
    pub choices: Vec<SelectorActionsEntry>,
    pub section: SelectorActionSection,
    #[dynamic(default)]
    pub multiple: bool,
    #[dynamic(default)]
    pub fuzzy_description: Option<String>,
    #[dynamic(default)]
    pub fuzzy: bool,
    #[dynamic(default)]
    pub cancel: Option<Box<KeyAssignment>>,
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
}

#[derive(Debug, Clone, PartialEq, FromDynamic, ToDynamic)]
pub struct DisplayText {
    pub text: String,
    #[dynamic(default)]
    pub title: String,
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
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
    ShowTabNavigator(ShowTabNavigatorArgs),
    ShowDebugOverlay(ShowDebugOverlayArgs),
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
    Confirmation(Confirmation),
    CommandRunner(CommandRunner),
    TransientMenu(TransientMenu),
    SelectorActions(SelectorActions),
    DisplayText(DisplayText),
}
impl_lua_conversion_dynamic!(KeyAssignment);

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub struct OverlayDimensions {
    #[dynamic(default = "default_full_overlay_size")]
    pub width: SplitSize,
    #[dynamic(default = "default_full_overlay_size")]
    pub height: SplitSize,
}

impl Default for OverlayDimensions {
    fn default() -> Self {
        Self {
            width: default_full_overlay_size(),
            height: default_full_overlay_size(),
        }
    }
}

fn default_full_overlay_size() -> SplitSize {
    SplitSize::Percent(100)
}

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
    /// Optional callback invoked after the spawned floating pane closes.
    #[dynamic(default)]
    pub action: Option<Box<KeyAssignment>>,
    #[dynamic(default)]
    pub dimensions: OverlayDimensions,
    #[dynamic(default)]
    pub border: bool,
    #[dynamic(default)]
    pub border_color: Option<ColorSpec>,
}
impl_lua_conversion_dynamic!(FloatingPaneSpawn);

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
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
    MoveForwardWord,
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
    MoveBackwardCommandBlock,
    MoveForwardCommandBlock,
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::AnsiColor;

    fn transient_option(
        input: Option<TransientOptionInput>,
        choices: Option<Vec<&str>>,
        default: Option<&str>,
    ) -> TransientOption {
        TransientOption {
            key: "o".to_string(),
            default: default.map(str::to_string),
            description: "Order".to_string(),
            flag: "--order=".to_string(),
            allow_unset: true,
            choices: choices.map(|choices| choices.into_iter().map(str::to_string).collect()),
            input,
        }
    }

    #[test]
    fn transient_option_infers_input_from_choices() {
        assert_eq!(
            TransientOptionInput::from_dynamic(
                &Value::String("cycle".to_string()),
                Default::default(),
            )
            .unwrap(),
            TransientOptionInput::Cycle
        );
        assert_eq!(
            transient_option(None, None, None).resolved_input(),
            TransientOptionInput::Prompt
        );
        assert_eq!(
            transient_option(None, Some(vec!["date"]), None).resolved_input(),
            TransientOptionInput::Select
        );
    }

    #[test]
    fn transient_option_validates_input_and_choices() {
        assert!(transient_option(
            Some(TransientOptionInput::Cycle),
            Some(vec!["date", "author-date"]),
            Some("date"),
        )
        .validate()
        .is_ok());
        assert!(
            transient_option(Some(TransientOptionInput::Cycle), None, None)
                .validate()
                .is_err()
        );
        assert!(
            transient_option(Some(TransientOptionInput::Prompt), Some(vec!["date"]), None,)
                .validate()
                .is_err()
        );
        assert!(transient_option(
            Some(TransientOptionInput::Select),
            Some(vec!["date"]),
            Some("topological"),
        )
        .validate()
        .is_err());
    }

    #[test]
    fn overlay_border_options_round_trip_through_dynamic_config() {
        let mut value = LauncherActionArgs::default().to_dynamic();
        let Value::Object(ref mut object) = value else {
            panic!("launcher arguments did not convert to an object");
        };
        object.insert("flags".to_dynamic(), "FUZZY".to_dynamic());
        object.insert("border".to_dynamic(), true.to_dynamic());
        object.insert(
            "border_color".to_dynamic(),
            ColorSpec::AnsiColor(AnsiColor::Blue).to_dynamic(),
        );

        let parsed = LauncherActionArgs::from_dynamic(&value, Default::default()).unwrap();
        assert!(parsed.border);
        assert_eq!(
            parsed.border_color,
            Some(ColorSpec::AnsiColor(AnsiColor::Blue))
        );
    }
}
