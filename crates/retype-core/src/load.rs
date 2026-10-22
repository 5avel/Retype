//! When a config file is applied, at startup and on reload (SPEC 5.5).

use crate::config::Config;

/// What reading the config file gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFile {
    Text(String),
    Missing,
    /// Exists but could not be read; the message says why.
    Unreadable(String),
}

/// Why the config is being loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadTrigger {
    Startup,
    /// "Reload config" from the tray.
    TrayCommand,
    /// The file's modification time changed.
    FileChanged,
}

/// The config to apply (`None`: keep the running one) and an error to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadResult {
    pub maybe_config: Option<Config>,
    pub maybe_error: Option<String>,
}

/// Decides what to apply.
///
/// - At startup there is always a config: defaults when the file is missing
///   or broken (with an error to log).
/// - On reload a broken or unreadable file keeps the running config.
/// - A missing file means defaults only when the user asked for the reload:
///   editors that save by delete + rename make the file vanish briefly.
pub fn load_config(file: ConfigFile, trigger: LoadTrigger) -> LoadResult {
    let startup = trigger == LoadTrigger::Startup;
    let fallback = |error: String| LoadResult {
        maybe_config: startup.then(Config::default),
        maybe_error: Some(error),
    };
    match file {
        ConfigFile::Text(text) => match Config::from_toml(&text) {
            Ok(config) => LoadResult {
                maybe_config: Some(config),
                maybe_error: None,
            },
            Err(err) => fallback(err.to_string()),
        },
        ConfigFile::Unreadable(why) => fallback(format!("cannot read the config file: {why}")),
        ConfigFile::Missing => LoadResult {
            maybe_config: (trigger != LoadTrigger::FileChanged).then(Config::default),
            maybe_error: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SwitchMode;

    const VALID: &str = "mode = \"cycle\"";
    const BROKEN: &str = "mode = \"auto\"";

    #[test]
    fn valid_file_applies_everywhere() {
        for trigger in [
            LoadTrigger::Startup,
            LoadTrigger::TrayCommand,
            LoadTrigger::FileChanged,
        ] {
            let result = load_config(text(VALID), trigger);
            assert_eq!(result.maybe_config.map(|c| c.mode), Some(SwitchMode::Cycle));
            assert_eq!(result.maybe_error, None);
        }
    }

    #[test]
    fn broken_file_uses_defaults_at_startup_only() {
        let startup = load_config(text(BROKEN), LoadTrigger::Startup);
        assert_eq!(startup.maybe_config, Some(Config::default()));
        assert!(startup.maybe_error.is_some());

        for trigger in [LoadTrigger::TrayCommand, LoadTrigger::FileChanged] {
            let reload = load_config(text(BROKEN), trigger);
            assert_eq!(
                reload.maybe_config, None,
                "{trigger:?} keeps the running config"
            );
            assert!(reload.maybe_error.is_some());
        }
    }

    #[test]
    fn unreadable_file_keeps_running_config() {
        let file = ConfigFile::Unreadable("access denied".to_owned());
        let reload = load_config(file.clone(), LoadTrigger::FileChanged);
        assert_eq!(reload.maybe_config, None);
        assert!(reload.maybe_error.unwrap().contains("access denied"));
        let startup = load_config(file, LoadTrigger::Startup);
        assert_eq!(startup.maybe_config, Some(Config::default()));
    }

    #[test]
    fn missing_file_means_defaults_unless_it_just_vanished() {
        let at = |trigger| load_config(ConfigFile::Missing, trigger);
        assert_eq!(
            at(LoadTrigger::Startup).maybe_config,
            Some(Config::default())
        );
        assert_eq!(
            at(LoadTrigger::TrayCommand).maybe_config,
            Some(Config::default())
        );
        assert_eq!(
            at(LoadTrigger::FileChanged),
            LoadResult {
                maybe_config: None,
                maybe_error: None,
            }
        );
    }

    fn text(s: &str) -> ConfigFile {
        ConfigFile::Text(s.to_owned())
    }
}
