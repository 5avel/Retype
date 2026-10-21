//! OS-independent logic of retype.
//!
//! Everything here is pure and deterministic: events come in as data,
//! conversion plans go out as data. The Windows crate supplies layout tables
//! and executes plans. See `docs/ARCHITECTURE.md`.
//!
//! Modules are added by the roadmap (M1): `keystroke`, `event`, `buffer`,
//! `session`, `plan`, `layout`, `config`.

#![forbid(unsafe_code)]

/// Upper bound on recorded keystrokes (SPEC P2). The buffer never grows past it.
pub const MAX_BUFFER_KEYS: usize = 512;
