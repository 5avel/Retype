//! Windows glue: the only place where `unsafe` and WinAPI calls are allowed.
//!
//! Planned modules (ROADMAP M2–M4): `hook`, `input`, `layout`, `clipboard`,
//! `tray`, `instance`. Rules: `.claude/rules/unsafe-winapi.md`.

#![allow(unsafe_code)] // Every block still needs `// SAFETY:` (clippy enforces it).

/// Entry point of the Windows app. Filled in by ROADMAP M2.
pub fn run() {
    log::info!("retype {} starting", env!("CARGO_PKG_VERSION"));
}
