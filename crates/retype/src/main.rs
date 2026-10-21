//! retype — fix text typed in the wrong keyboard layout.
//!
//! The binary is Windows-only; on other systems it builds as a stub so the
//! workspace (and `retype-core` tests) work everywhere.

// No console window in release builds; debug builds keep it for convenience.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// unsafe is allowed only inside `win/` (it re-allows it at module level).
#![deny(unsafe_code)]

#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    win::run();

    #[cfg(not(windows))]
    {
        log::error!("retype runs only on Windows");
        std::process::exit(1);
    }
}
