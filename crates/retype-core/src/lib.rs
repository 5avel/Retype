//! OS-independent logic of retype.
//!
//! Everything here is pure and deterministic: events come in as data,
//! conversion plans go out as data. The Windows crate supplies layout tables
//! and executes plans. See `docs/ARCHITECTURE.md`.

#![forbid(unsafe_code)]

pub mod buffer;
pub mod config;
pub mod event;
pub mod hotkey;
pub mod inject;
pub mod keystroke;
pub mod layout;
pub mod plan;
pub mod selection;
pub mod session;
#[cfg(test)]
mod testing;

pub use config::{Config, ConfigError};
pub use event::InputEvent;
pub use hotkey::{Hotkey, HotkeyAction, HotkeySet};
pub use inject::{KeyAction, KeyCode, Step, execution_steps, restore_modifiers};
pub use keystroke::{Keystroke, Modifiers};
pub use layout::{LayoutId, LayoutTable};
pub use plan::{ConversionPlan, PlanOp, Scope};
pub use selection::{SelectionConversion, convert_selection};
pub use session::{Command, Session};

/// Upper bound on recorded keystrokes (SPEC P2). The buffer never grows past it.
pub const MAX_BUFFER_KEYS: usize = 512;
