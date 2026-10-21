//! Hotkeys: parsing from config strings and matching against key presses.

use std::fmt;
use std::str::FromStr;

use crate::keystroke::{KeyClass, Modifiers, classify, vk};

/// A key plus the exact modifier set that must be held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub mods: Modifiers,
    pub vk: u16,
}

impl Hotkey {
    /// True if pressing `vk_code` with `mods` triggers this hotkey.
    pub fn matches(self, vk_code: u16, mods: Modifiers) -> bool {
        // Windows reports Ctrl+Pause as VK_CANCEL.
        let vk_code = if vk_code == vk::CANCEL && mods.ctrl {
            vk::PAUSE
        } else {
            vk_code
        };
        self.vk == vk_code && self.mods == mods
    }
}

/// What a hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ConvertLastWord,
    ConvertLastLine,
    ConvertSelection,
    TogglePause,
}

/// The configured hotkeys (SPEC 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeySet {
    pub convert_last_word: Hotkey,
    pub convert_last_line: Hotkey,
    pub convert_selection: Hotkey,
    pub toggle_pause: Hotkey,
}

impl HotkeySet {
    /// The action bound to this key press, if any. Cheap enough to call from
    /// the hook callback to decide whether to swallow the key.
    pub fn action_for(&self, vk_code: u16, mods: Modifiers) -> Option<HotkeyAction> {
        self.bindings()
            .into_iter()
            .find(|(hotkey, _)| hotkey.matches(vk_code, mods))
            .map(|(_, action)| action)
    }

    pub fn bindings(&self) -> [(Hotkey, HotkeyAction); 4] {
        [
            (self.convert_last_word, HotkeyAction::ConvertLastWord),
            (self.convert_last_line, HotkeyAction::ConvertLastLine),
            (self.convert_selection, HotkeyAction::ConvertSelection),
            (self.toggle_pause, HotkeyAction::TogglePause),
        ]
    }
}

impl Default for HotkeySet {
    fn default() -> Self {
        let key = |mods, vk| Hotkey { mods, vk };
        Self {
            convert_last_word: key(Modifiers::NONE, vk::PAUSE),
            convert_last_line: key(
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
                vk::PAUSE,
            ),
            convert_selection: key(Modifiers::NONE, vk::SCROLL),
            toggle_pause: key(
                Modifiers {
                    ctrl: true,
                    ..Modifiers::NONE
                },
                vk::PAUSE,
            ),
        }
    }
}

/// Why a hotkey string was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyError {
    Empty,
    UnknownKey(String),
    DuplicateModifier(String),
    /// A printable key without Ctrl/Alt/Win would swallow normal typing.
    WouldBlockTyping(String),
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "hotkey is empty"),
            Self::UnknownKey(k) => write!(
                f,
                "unknown key `{k}` (expected e.g. Pause, ScrollLock, Insert, F1..F24, A..Z, 0..9)"
            ),
            Self::DuplicateModifier(m) => write!(f, "modifier `{m}` is repeated"),
            Self::WouldBlockTyping(k) => write!(
                f,
                "`{k}` types text; combine it with Ctrl, Alt or Win to use it as a hotkey"
            ),
        }
    }
}

impl std::error::Error for HotkeyError {}

impl FromStr for Hotkey {
    type Err = HotkeyError;

    /// Parses strings such as `Pause`, `Shift+Pause`, `Ctrl+Alt+F12`.
    /// Names are case-insensitive.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let Some((key, modifiers)) = parts.split_last() else {
            return Err(HotkeyError::Empty);
        };
        if key.is_empty() {
            return Err(HotkeyError::Empty);
        }
        let mut mods = Modifiers::NONE;
        for &name in modifiers {
            let flag = match name.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut mods.ctrl,
                "shift" => &mut mods.shift,
                "alt" => &mut mods.alt,
                "win" => &mut mods.win,
                _ => return Err(HotkeyError::UnknownKey(name.to_owned())),
            };
            if *flag {
                return Err(HotkeyError::DuplicateModifier(name.to_owned()));
            }
            *flag = true;
        }
        let vk_code = key_code(key).ok_or_else(|| HotkeyError::UnknownKey((*key).to_owned()))?;
        let types_text = matches!(
            classify(vk_code),
            KeyClass::Printable | KeyClass::Space | KeyClass::Backspace
        );
        if types_text && !mods.is_command() {
            return Err(HotkeyError::WouldBlockTyping(s.trim().to_owned()));
        }
        Ok(Self { mods, vk: vk_code })
    }
}

fn key_code(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    let named = match lower.as_str() {
        "pause" | "break" => Some(vk::PAUSE),
        "scrolllock" | "scroll" => Some(vk::SCROLL),
        "capslock" => Some(vk::CAPITAL),
        "insert" | "ins" => Some(0x2D),
        "space" => Some(vk::SPACE),
        "backspace" => Some(vk::BACK),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        return (1..=24).contains(&n).then(|| vk::F1 + n - 1);
    }
    match name.as_bytes() {
        [c] if c.is_ascii_alphanumeric() => Some(u16::from(c.to_ascii_uppercase())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(shift: bool, ctrl: bool, alt: bool) -> Modifiers {
        Modifiers {
            shift,
            ctrl,
            alt,
            win: false,
        }
    }

    #[test]
    fn parses_defaults() {
        let pause: Hotkey = "Pause".parse().unwrap();
        assert_eq!(
            pause,
            Hotkey {
                mods: Modifiers::NONE,
                vk: vk::PAUSE
            }
        );
        let shift_pause: Hotkey = "Shift+Pause".parse().unwrap();
        assert_eq!(shift_pause.mods, mods(true, false, false));
        let scroll: Hotkey = "ScrollLock".parse().unwrap();
        assert_eq!(scroll.vk, vk::SCROLL);
        let ctrl_alt_f12: Hotkey = " ctrl + ALT + f12 ".parse().unwrap();
        assert_eq!(
            ctrl_alt_f12,
            Hotkey {
                mods: mods(false, true, true),
                vk: 0x7B
            }
        );
    }

    #[test]
    fn rejects_bad_strings() {
        assert_eq!("".parse::<Hotkey>(), Err(HotkeyError::Empty));
        assert_eq!("Ctrl+".parse::<Hotkey>(), Err(HotkeyError::Empty));
        assert!(matches!(
            "Hyper+Pause".parse::<Hotkey>(),
            Err(HotkeyError::UnknownKey(_))
        ));
        assert!(matches!(
            "F25".parse::<Hotkey>(),
            Err(HotkeyError::UnknownKey(_))
        ));
        assert!(matches!(
            "Ctrl+ctrl+Pause".parse::<Hotkey>(),
            Err(HotkeyError::DuplicateModifier(_))
        ));
        assert!(matches!(
            "Q".parse::<Hotkey>(),
            Err(HotkeyError::WouldBlockTyping(_))
        ));
        assert!(matches!(
            "Shift+Space".parse::<Hotkey>(),
            Err(HotkeyError::WouldBlockTyping(_))
        ));
        assert!("Ctrl+Alt+Q".parse::<Hotkey>().is_ok());
    }

    #[test]
    fn matches_exact_modifiers_only() {
        let pause: Hotkey = "Pause".parse().unwrap();
        assert!(pause.matches(vk::PAUSE, Modifiers::NONE));
        assert!(!pause.matches(vk::PAUSE, mods(true, false, false)));
    }

    #[test]
    fn default_set_matches_spec() {
        let set = HotkeySet::default();
        assert_eq!(set.convert_last_word, "Pause".parse().unwrap());
        assert_eq!(set.convert_last_line, "Shift+Pause".parse().unwrap());
        assert_eq!(set.convert_selection, "ScrollLock".parse().unwrap());
        assert_eq!(set.toggle_pause, "Ctrl+Pause".parse().unwrap());
        assert_eq!(
            set.action_for(vk::PAUSE, mods(true, false, false)),
            Some(HotkeyAction::ConvertLastLine)
        );
        assert_eq!(
            set.action_for(vk::CANCEL, mods(false, true, false)),
            Some(HotkeyAction::TogglePause)
        );
        assert_eq!(set.action_for(0x41, Modifiers::NONE), None);
    }

    #[test]
    fn ctrl_pause_arrives_as_cancel() {
        let ctrl_pause: Hotkey = "Ctrl+Pause".parse().unwrap();
        assert!(ctrl_pause.matches(vk::CANCEL, mods(false, true, false)));
        assert!(ctrl_pause.matches(vk::PAUSE, mods(false, true, false)));
        let pause: Hotkey = "Pause".parse().unwrap();
        assert!(!pause.matches(vk::CANCEL, Modifiers::NONE));
    }
}
