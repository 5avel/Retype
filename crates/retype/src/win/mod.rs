//! Windows glue: the only place where `unsafe` and WinAPI calls are allowed.
//!
//! Threads (see `docs/ARCHITECTURE.md`): the main thread owns the tray
//! window, the hook thread only enqueues input, the worker runs the session.
//! Rules: `.claude/rules/unsafe-winapi.md`.

#![allow(unsafe_code)] // Every block still needs `// SAFETY:` (clippy enforces it).

mod executor;
mod hook;
mod input;
mod instance;
mod layout;
mod tray;
mod worker;

use std::fs;
use std::io::ErrorKind;
use std::sync::mpsc;

use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::{HSTRING, w};

use retype_core::config::Config;
use retype_core::session::Session;

use crate::logger;
use crate::paths::Paths;

/// Hook → worker queue size. Typing never comes close; if the worker stalls,
/// input is dropped and the buffer is cleared rather than blocking the hook.
const CHANNEL_CAPACITY: usize = 256;

/// Entry point of the Windows app.
pub fn run() {
    let paths = Paths::detect();
    let (config, config_error) = load_config(&paths);
    // Without a log file retype still works; there is nowhere to report it.
    let _ = logger::init(&paths.log_file(), config.log_level);
    log::info!("retype {} starting", env!("CARGO_PKG_VERSION"));
    if let Some(error) = config_error {
        log::error!("{error}; using defaults");
    }

    let Some(_instance) = instance::acquire() else {
        log::warn!("another instance is running; exiting");
        message_box(
            "retype is already running. Look for its icon in the tray \
             (it may be hidden under the ^ arrow).",
            MB_ICONINFORMATION,
        );
        return;
    };

    let (tx, rx) = mpsc::sync_channel(CHANNEL_CAPACITY);
    let hotkeys = config.hotkeys;
    let tray = match tray::Tray::create(tx.clone(), paths) {
        Ok(tray) => tray,
        Err(err) => {
            fatal(&format!("cannot create the tray icon: {err}"));
            return;
        }
    };
    let worker = match worker::spawn(rx, Session::new(config), tray.handle()) {
        Ok(worker) => worker,
        Err(err) => {
            fatal(&format!("cannot start the worker thread: {err}"));
            return;
        }
    };
    let hook = match hook::start(tx.clone(), hotkeys) {
        Ok(hook) => Some(hook),
        Err(err) => {
            fatal(&format!("cannot install the keyboard hook: {err}"));
            None
        }
    };

    if hook.is_some() {
        tray.run();
    }

    // Clean shutdown: hooks first so no new input arrives, then the worker.
    if let Some(hook) = hook {
        hook.stop();
    }
    let _ = tx.send(worker::WorkerMsg::Shutdown);
    if worker.join().is_err() {
        log::error!("worker panicked");
    }
    log::info!("exit");
    log::logger().flush();
}

/// Logs a startup failure and tells the user: release builds have no console.
fn fatal(message: &str) {
    log::error!("{message}");
    message_box(message, MB_ICONERROR);
}

fn message_box(text: &str, icon: MESSAGEBOX_STYLE) {
    let text = HSTRING::from(text);
    // SAFETY: both strings are NUL-terminated and outlive the modal call.
    unsafe { MessageBoxW(None, &text, w!("retype"), MB_OK | icon) };
}

/// Reads the config file. A missing file means defaults; a broken one means
/// defaults plus an error to log.
fn load_config(paths: &Paths) -> (Config, Option<String>) {
    match fs::read_to_string(&paths.config_file) {
        Ok(text) => match Config::from_toml(&text) {
            Ok(config) => (config, None),
            Err(err) => (Config::default(), Some(err.to_string())),
        },
        Err(err) if err.kind() == ErrorKind::NotFound => (Config::default(), None),
        Err(err) => (
            Config::default(),
            Some(format!(
                "cannot read {}: {err}",
                paths.config_file.display()
            )),
        ),
    }
}
