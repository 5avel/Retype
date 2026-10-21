//! Physical keystrokes and how the core classifies them.
//!
//! Keys are identified by Windows virtual-key codes (plain numbers, so the
//! core stays OS-independent) plus the layout-independent scan code that is
//! replayed (ADR-003).

/// Virtual-key codes the core needs to know about.
pub mod vk {
    pub const CANCEL: u16 = 0x03;
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12;
    pub const PAUSE: u16 = 0x13;
    pub const CAPITAL: u16 = 0x14;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const F1: u16 = 0x70;
    pub const NUMLOCK: u16 = 0x90;
    pub const SCROLL: u16 = 0x91;
    pub const LSHIFT: u16 = 0xA0;
    pub const RSHIFT: u16 = 0xA1;
    pub const LCONTROL: u16 = 0xA2;
    pub const RCONTROL: u16 = 0xA3;
    pub const LMENU: u16 = 0xA4;
    pub const RMENU: u16 = 0xA5;
}

/// Modifier state at the moment a key was pressed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub win: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
        win: false,
    };

    /// Ctrl, Alt or Win is held: the key is a command, not text (SPEC P3).
    pub fn is_command(self) -> bool {
        self.ctrl || self.alt || self.win
    }
}

/// One recorded key press. Holds no character, only the physical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keystroke {
    pub vk: u16,
    pub scan: u16,
    pub extended: bool,
    pub shift: bool,
}

impl Keystroke {
    /// Used to overwrite forgotten keys.
    pub const ZEROED: Self = Self {
        vk: 0,
        scan: 0,
        extended: false,
        shift: false,
    };
}

/// What a key press means for the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyClass {
    /// Produces a character whose value depends on the layout.
    Printable,
    /// Separates words; produces the same character in every layout.
    Space,
    /// Deletes the previously recorded key.
    Backspace,
    /// Shift, Ctrl, Alt, Win, lock keys: do not touch the buffer.
    Modifier,
    /// Enter, Tab, Escape, navigation, function keys: the recorded text no
    /// longer matches what is on screen before the cursor.
    Reset,
}

/// Classifies a virtual-key code.
pub fn classify(vk_code: u16) -> KeyClass {
    match vk_code {
        vk::SPACE => KeyClass::Space,
        vk::BACK => KeyClass::Backspace,
        vk::SHIFT
        | vk::CONTROL
        | vk::MENU
        | vk::LSHIFT
        | vk::RSHIFT
        | vk::LCONTROL
        | vk::RCONTROL
        | vk::LMENU
        | vk::RMENU
        | vk::LWIN
        | vk::RWIN
        | vk::CAPITAL
        | vk::NUMLOCK
        | vk::SCROLL => KeyClass::Modifier,
        // Digits, letters, numpad digits and operators.
        0x30..=0x39 | 0x41..=0x5A | 0x60..=0x6F => KeyClass::Printable,
        // OEM punctuation keys: they become letters in other layouts.
        0xBA..=0xC0 | 0xDB..=0xDF | 0xE2 => KeyClass::Printable,
        _ => KeyClass::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_keys() {
        assert_eq!(classify(0x41), KeyClass::Printable); // A
        assert_eq!(classify(0x31), KeyClass::Printable); // 1
        assert_eq!(classify(0xBA), KeyClass::Printable); // ; (ж)
        assert_eq!(classify(0xDE), KeyClass::Printable); // ' (э)
        assert_eq!(classify(vk::SPACE), KeyClass::Space);
        assert_eq!(classify(vk::BACK), KeyClass::Backspace);
        assert_eq!(classify(vk::LSHIFT), KeyClass::Modifier);
        assert_eq!(classify(vk::CAPITAL), KeyClass::Modifier);
        assert_eq!(classify(vk::RETURN), KeyClass::Reset);
        assert_eq!(classify(vk::TAB), KeyClass::Reset);
        assert_eq!(classify(0x25), KeyClass::Reset); // Left arrow
        assert_eq!(classify(vk::F1), KeyClass::Reset);
    }

    #[test]
    fn command_modifiers() {
        assert!(!Modifiers::NONE.is_command());
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        assert!(!shift.is_command());
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        assert!(ctrl.is_command());
    }
}
