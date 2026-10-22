//! Input events delivered by the app to the core.

use crate::keystroke::{Keystroke, Modifiers};

/// Milliseconds on a monotonic clock supplied by the app.
pub type Millis = u64;

/// Something that happened on the user's desktop.
///
/// The app filters out its own injected input before creating events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// A key was pressed (or auto-repeated). `mods` is the modifier state at
    /// that moment, including Shift.
    KeyDown {
        key: Keystroke,
        mods: Modifiers,
        time: Millis,
    },
    /// A key was released. Only the virtual key is passed: it tells a
    /// hotkey's auto-repeat from a second press, and ends a double tap.
    KeyUp { vk: u16, time: Millis },
    /// Any mouse button was pressed: the caret may have moved.
    MouseClick,
    /// The foreground window or its focused control changed.
    FocusChanged,
    /// Periodic timer, so an idle buffer is cleared even without typing.
    Tick { time: Millis },
}
