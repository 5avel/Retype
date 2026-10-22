//! User configuration: TOML parsing, defaults and validation (SPEC 5.5).
//!
//! See `config.example.toml` for the documented format. Unknown keys are
//! errors, so typos are reported instead of silently ignored.

use std::fmt;

use serde::Deserialize;

use crate::hotkey::{Hotkey, HotkeySet};
use crate::layout::{LayoutId, SwitchMode};

/// Log verbosity. Key content is never logged at any level (SPEC P4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

/// Validated configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub layouts: Vec<LayoutId>,
    pub mode: SwitchMode,
    pub switch_layout_after_convert: bool,
    pub keep_trailing_spaces: bool,
    /// 0 disables the idle reset.
    pub idle_reset_seconds: u32,
    pub hotkeys: HotkeySet,
    /// Double tap of right Shift converts the last word.
    pub double_right_shift: bool,
    /// Convert words typed in the wrong layout automatically (ADR-015).
    pub autoswitch: bool,
    /// Lowercased executable names.
    pub excluded_processes: Vec<String>,
    pub log_level: LogLevel,
}

impl Default for Config {
    fn default() -> Self {
        let layouts = ["en-us", "ru-ru"]
            .into_iter()
            .filter_map(LayoutId::new)
            .collect();
        Self {
            layouts,
            mode: SwitchMode::Pair,
            switch_layout_after_convert: true,
            keep_trailing_spaces: true,
            idle_reset_seconds: 30,
            hotkeys: HotkeySet::default(),
            double_right_shift: true,
            autoswitch: false,
            excluded_processes: Vec::new(),
            log_level: LogLevel::Info,
        }
    }
}

impl Config {
    /// Parses and validates a config file's contents. Missing keys take
    /// their defaults.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig =
            toml::from_str(text).map_err(|e| ConfigError::Syntax(e.to_string()))?;
        raw.validate()
    }

    /// True if retype must ignore input for this executable name.
    pub fn is_excluded(&self, exe_name: &str) -> bool {
        let exe_name = exe_name.to_lowercase();
        self.excluded_processes.contains(&exe_name)
    }

    /// Like [`Config::is_excluded`], for a full executable path such as
    /// `C:\Program Files\KeePassXC\KeePassXC.exe`.
    pub fn is_excluded_path(&self, exe_path: &str) -> bool {
        let name = exe_path.rsplit(['\\', '/']).next().unwrap_or(exe_path);
        self.is_excluded(name)
    }
}

/// Why a config was rejected. Messages name the offending key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// Not valid TOML, a wrong type or an unknown key.
    Syntax(String),
    /// Valid TOML with a value that makes no sense.
    Invalid { key: String, message: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(message) => write!(f, "config syntax error: {message}"),
            Self::Invalid { key, message } => write!(f, "config error in `{key}`: {message}"),
        }
    }
}

impl std::error::Error for ConfigError {}

fn invalid(key: &str, message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        key: key.to_owned(),
        message: message.into(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    layouts: Option<Vec<String>>,
    mode: Option<SwitchMode>,
    switch_layout_after_convert: Option<bool>,
    keep_trailing_spaces: Option<bool>,
    idle_reset_seconds: Option<u32>,
    #[serde(default)]
    hotkeys: RawHotkeys,
    #[serde(default)]
    exclusions: RawExclusions,
    #[serde(default)]
    log: RawLog,
    #[serde(default)]
    autoswitch: RawAutoswitch,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHotkeys {
    convert_last_word: Option<String>,
    convert_last_line: Option<String>,
    convert_selection: Option<String>,
    toggle_pause: Option<String>,
    double_right_shift: Option<bool>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExclusions {
    #[serde(default)]
    processes: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAutoswitch {
    enabled: Option<bool>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLog {
    level: Option<LogLevel>,
}

impl RawConfig {
    fn validate(self) -> Result<Config, ConfigError> {
        let defaults = Config::default();

        let layouts = match self.layouts {
            None => defaults.layouts,
            Some(tags) => parse_layouts(&tags)?,
        };

        let double_right_shift = self
            .hotkeys
            .double_right_shift
            .unwrap_or(defaults.double_right_shift);
        let hotkeys = self.hotkeys.validate(defaults.hotkeys)?;

        let mut excluded_processes = Vec::new();
        for name in self.exclusions.processes {
            let name = name.trim().to_lowercase();
            if name.is_empty() {
                return Err(invalid("exclusions.processes", "empty process name"));
            }
            if !excluded_processes.contains(&name) {
                excluded_processes.push(name);
            }
        }

        Ok(Config {
            layouts,
            mode: self.mode.unwrap_or(defaults.mode),
            switch_layout_after_convert: self
                .switch_layout_after_convert
                .unwrap_or(defaults.switch_layout_after_convert),
            keep_trailing_spaces: self
                .keep_trailing_spaces
                .unwrap_or(defaults.keep_trailing_spaces),
            idle_reset_seconds: self
                .idle_reset_seconds
                .unwrap_or(defaults.idle_reset_seconds),
            hotkeys,
            double_right_shift,
            autoswitch: self.autoswitch.enabled.unwrap_or(defaults.autoswitch),
            excluded_processes,
            log_level: self.log.level.unwrap_or(defaults.log_level),
        })
    }
}

fn parse_layouts(tags: &[String]) -> Result<Vec<LayoutId>, ConfigError> {
    let mut layouts: Vec<LayoutId> = Vec::new();
    for tag in tags {
        let id = LayoutId::new(tag.trim()).ok_or_else(|| {
            invalid(
                "layouts",
                format!("`{tag}` is not a language tag like \"en-US\""),
            )
        })?;
        if layouts.contains(&id) {
            return Err(invalid("layouts", format!("`{tag}` is listed twice")));
        }
        layouts.push(id);
    }
    if layouts.len() < 2 {
        return Err(invalid(
            "layouts",
            "list at least two layouts to switch between",
        ));
    }
    Ok(layouts)
}

impl RawHotkeys {
    fn validate(self, defaults: HotkeySet) -> Result<HotkeySet, ConfigError> {
        let parse = |key: &str, value: Option<String>, default: Hotkey| match value {
            None => Ok(default),
            Some(text) => text
                .parse::<Hotkey>()
                .map_err(|e| invalid(&format!("hotkeys.{key}"), e.to_string())),
        };
        let set = HotkeySet {
            convert_last_word: parse(
                "convert_last_word",
                self.convert_last_word,
                defaults.convert_last_word,
            )?,
            convert_last_line: parse(
                "convert_last_line",
                self.convert_last_line,
                defaults.convert_last_line,
            )?,
            convert_selection: parse(
                "convert_selection",
                self.convert_selection,
                defaults.convert_selection,
            )?,
            toggle_pause: parse("toggle_pause", self.toggle_pause, defaults.toggle_pause)?,
        };
        let bindings = set.bindings();
        for (i, (hotkey, _)) in bindings.iter().enumerate() {
            if bindings[..i].iter().any(|(other, _)| other == hotkey) {
                return Err(invalid("hotkeys", "two actions use the same hotkey"));
            }
        }
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }

    fn invalid_key(text: &str) -> String {
        match Config::from_toml(text) {
            Err(ConfigError::Invalid { key, .. }) => key,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn example_config_parses() {
        let config = Config::from_toml(include_str!("../../../config.example.toml")).unwrap();
        assert_eq!(config.layouts, vec![id("en-US"), id("ru-RU")]);
        assert_eq!(config.mode, SwitchMode::Pair);
        assert_eq!(config.hotkeys, HotkeySet::default());
        assert!(config.is_excluded("keepassxc.EXE"));
        assert!(config.is_excluded("MSTSC.exe"));
        assert!(!config.is_excluded("notepad.exe"));
        assert!(config.is_excluded_path(r"C:\Program Files\KeePassXC\KeePassXC.exe"));
        assert!(config.is_excluded_path("mstsc.exe"));
        assert!(!config.is_excluded_path(r"C:\mstsc.exe\notepad.exe"));
        assert!(!config.is_excluded_path(""));
        assert_eq!(config.log_level, LogLevel::Info);
    }

    #[test]
    fn reads_all_fields() {
        let config = Config::from_toml(
            r#"
            layouts = ["en-US", "uk-UA", "ru-RU"]
            mode = "cycle"
            switch_layout_after_convert = false
            keep_trailing_spaces = false
            idle_reset_seconds = 0
            [hotkeys]
            convert_last_word = "F9"
            [log]
            level = "debug"
            "#,
        )
        .unwrap();
        assert_eq!(config.layouts.len(), 3);
        assert_eq!(config.mode, SwitchMode::Cycle);
        assert!(!config.switch_layout_after_convert);
        assert!(!config.keep_trailing_spaces);
        assert_eq!(config.idle_reset_seconds, 0);
        assert_eq!(config.hotkeys.convert_last_word, "F9".parse().unwrap());
        assert_eq!(
            config.hotkeys.toggle_pause,
            HotkeySet::default().toggle_pause
        );
        assert_eq!(config.log_level, LogLevel::Debug);
    }

    #[test]
    fn syntax_errors_and_unknown_keys() {
        assert!(matches!(
            Config::from_toml("layouts = ["),
            Err(ConfigError::Syntax(_))
        ));
        assert!(matches!(
            Config::from_toml("mode = \"auto\""),
            Err(ConfigError::Syntax(_))
        ));
        assert!(matches!(
            Config::from_toml("idle_reset_secnds = 5"),
            Err(ConfigError::Syntax(_))
        ));
        assert!(matches!(
            Config::from_toml("[hotkeys]\nconvert_word = \"F9\""),
            Err(ConfigError::Syntax(_))
        ));
    }

    #[test]
    fn invalid_values_name_their_key() {
        assert_eq!(invalid_key("layouts = [\"en-US\"]"), "layouts");
        assert_eq!(invalid_key("layouts = [\"en-US\", \"EN-us\"]"), "layouts");
        assert_eq!(invalid_key("layouts = [\"en US\", \"ru-RU\"]"), "layouts");
        assert_eq!(
            invalid_key("[hotkeys]\nconvert_selection = \"Hyper+X\""),
            "hotkeys.convert_selection"
        );
        assert_eq!(
            invalid_key("[hotkeys]\nconvert_last_word = \"ScrollLock\""),
            "hotkeys"
        );
        assert_eq!(
            invalid_key("[exclusions]\nprocesses = [\" \"]"),
            "exclusions.processes"
        );
    }

    #[test]
    fn error_messages_are_readable() {
        let err = Config::from_toml("[hotkeys]\ntoggle_pause = \"Q\"").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("hotkeys.toggle_pause"), "{text}");
        assert!(text.contains("Ctrl, Alt or Win"), "{text}");
    }
}
