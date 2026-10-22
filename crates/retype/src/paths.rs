//! Where retype keeps its config and log (SPEC 5.5).
//!
//! Plain `std` code, compiled on every OS so it can be unit-tested.

#![cfg_attr(not(windows), allow(dead_code))]

use std::env;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "retype";
const CONFIG_FILE: &str = "config.toml";
const LOG_FILE: &str = "retype.log";
/// An empty file with this name next to the exe enables portable mode.
const PORTABLE_MARKER: &str = "portable";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_file: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn log_file(&self) -> PathBuf {
        self.log_dir.join(LOG_FILE)
    }

    /// Resolves paths from the running exe and the environment.
    pub fn detect() -> Self {
        let exe_dir = env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let portable = exe_dir
            .as_deref()
            .is_some_and(|dir| dir.join(PORTABLE_MARKER).is_file());
        Self::resolve(
            exe_dir.as_deref(),
            portable,
            env::var_os("APPDATA").map(PathBuf::from).as_deref(),
            env::var_os("LOCALAPPDATA").map(PathBuf::from).as_deref(),
        )
    }

    /// Portable: config next to the exe. Otherwise `%APPDATA%\retype\` for
    /// the config and `%LOCALAPPDATA%\retype\` for the log. Missing variables
    /// fall back to the exe directory, then to the current directory.
    pub fn resolve(
        exe_dir: Option<&Path>,
        portable: bool,
        appdata: Option<&Path>,
        local_appdata: Option<&Path>,
    ) -> Self {
        let fallback = exe_dir.map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let config_dir = match appdata {
            Some(dir) if !portable => dir.join(APP_DIR),
            _ => fallback.clone(),
        };
        let log_dir = match local_appdata {
            Some(dir) => dir.join(APP_DIR),
            None => fallback,
        };
        Self {
            config_file: config_dir.join(CONFIG_FILE),
            log_dir,
        }
    }
}

/// The HKCU `Run` value that starts `exe` at logon: the quoted path.
pub fn autostart_command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// The exe an existing `Run` value starts (quotes and spaces stripped).
pub fn autostart_target(current: &str) -> PathBuf {
    PathBuf::from(current.trim().trim_matches('"'))
}

/// True if an existing `Run` value should be pointed at `exe`: it names
/// another path (Windows paths ignore case) and that file is gone, i.e. the
/// exe was moved. Running another copy (a debug build, a download) leaves a
/// working autostart alone.
pub fn autostart_needs_update(current: &str, exe: &Path, target_exists: bool) -> bool {
    let target = autostart_target(current).to_string_lossy().to_lowercase();
    let exe = exe.to_string_lossy().to_lowercase();
    target != exe && !target_exists
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_layout() {
        let paths = Paths::resolve(
            Some(Path::new("/app")),
            false,
            Some(Path::new("/roaming")),
            Some(Path::new("/local")),
        );
        assert_eq!(paths.config_file, Path::new("/roaming/retype/config.toml"));
        assert_eq!(paths.log_file(), Path::new("/local/retype/retype.log"));
    }

    #[test]
    fn portable_keeps_config_next_to_exe() {
        let paths = Paths::resolve(
            Some(Path::new("/app")),
            true,
            Some(Path::new("/roaming")),
            Some(Path::new("/local")),
        );
        assert_eq!(paths.config_file, Path::new("/app/config.toml"));
        assert_eq!(paths.log_dir, Path::new("/local/retype"));
    }

    #[test]
    fn falls_back_without_environment() {
        let paths = Paths::resolve(Some(Path::new("/app")), false, None, None);
        assert_eq!(paths.config_file, Path::new("/app/config.toml"));
        assert_eq!(paths.log_dir, Path::new("/app"));
        let paths = Paths::resolve(None, false, None, None);
        assert_eq!(paths.config_file, Path::new("./config.toml"));
    }

    #[test]
    fn autostart_command_is_quoted() {
        let exe = Path::new(r"C:\Tools\re type\retype.exe");
        let command = autostart_command(exe);
        assert_eq!(command, r#""C:\Tools\re type\retype.exe""#);
        assert_eq!(autostart_target(&command), exe);
    }

    #[test]
    fn autostart_is_rewritten_only_for_a_moved_exe() {
        let exe = Path::new(r"C:\Пользователи\Ann\retype.exe");
        let same = r#" "c:\пользователи\ANN\retype.exe" "#;
        assert!(!autostart_needs_update(same, exe, false));
        let other = r#""D:\old\retype.exe""#;
        assert!(autostart_needs_update(other, exe, false), "old exe gone");
        assert!(
            !autostart_needs_update(other, exe, true),
            "other copy works"
        );
    }
}
