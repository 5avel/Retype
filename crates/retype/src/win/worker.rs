//! Worker thread: feeds hook input into the core session and acts on commands.

use std::fs;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use retype_core::event::{InputEvent, Millis};
use retype_core::keystroke::{Keystroke, Modifiers};
use retype_core::load::{LoadTrigger, load_config};
use retype_core::plan::{ConversionPlan, PlanOp, Scope};
use retype_core::selection::convert_selection;
use retype_core::session::{Command, Session};

use super::executor::Executor;
use super::hook::{self, EXCLUDED, OVERFLOW, PAUSED, reconcile_modifiers};
use super::owner::OwnerWindow;
use super::process::foreground_exe_path;
use super::read_config_file;
use super::selection::SelectionOutcome;
use super::tray::TrayHandle;
use crate::logger;
use crate::paths::Paths;

/// How often the session gets a `Tick` for the idle reset.
const TICK: Duration = Duration::from_secs(1);

/// Longest wait between message pumps for the clipboard owner window:
/// other apps emptying the clipboard wait for it (see `owner`).
const PUMP_INTERVAL: Duration = Duration::from_millis(50);

/// How often the config file's modification time is checked (SPEC 5.5).
const CONFIG_POLL: Duration = Duration::from_secs(2);

/// Input as captured by the hook callbacks.
#[derive(Debug, Clone, Copy)]
pub enum HookInput {
    Key { key: Keystroke, mods: Modifiers },
    KeyUp { vk: u16 },
    MouseClick,
    FocusChanged,
}

/// Messages for the worker thread.
#[derive(Debug, Clone, Copy)]
pub enum WorkerMsg {
    Input(HookInput),
    /// Pause/resume chosen from the tray menu.
    TogglePause,
    /// "Reload config" chosen from the tray menu.
    ReloadConfig,
    Shutdown,
}

pub fn spawn(
    rx: Receiver<WorkerMsg>,
    session: Session,
    tray: TrayHandle,
    paths: Paths,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("worker".to_owned())
        .spawn(move || Worker::new(session, tray, paths).run(&rx))
}

struct Worker {
    session: Session,
    executor: Executor,
    tray: TrayHandle,
    paths: Paths,
    /// Modification time of the config file when it was last loaded.
    maybe_config_mtime: Option<SystemTime>,
    started: Instant,
}

impl Worker {
    fn new(session: Session, tray: TrayHandle, paths: Paths) -> Self {
        Self {
            executor: Executor::new(&session.config().layouts),
            maybe_config_mtime: modified(&paths),
            session,
            tray,
            paths,
            started: Instant::now(),
        }
    }

    fn now(&self) -> Millis {
        Millis::try_from(self.started.elapsed().as_millis()).unwrap_or(Millis::MAX)
    }

    fn run(mut self, rx: &Receiver<WorkerMsg>) {
        let owner = match OwnerWindow::create() {
            Ok(owner) => Some(owner),
            Err(err) => {
                log::error!("clipboard owner window: {err}; selection conversion disabled");
                None
            }
        };
        log::info!("worker started");
        self.update_exclusion();
        let mut last_tick = Instant::now();
        let mut last_config_check = Instant::now();
        loop {
            let message = rx.recv_timeout(PUMP_INTERVAL);
            if let Some(owner) = &owner {
                owner.pump();
            }
            if OVERFLOW.swap(false, Ordering::Relaxed) {
                log::warn!("input queue overflowed; buffer cleared");
                // A dropped focus event may have changed the foreground app.
                self.update_exclusion();
                self.session.handle(InputEvent::FocusChanged);
            }
            let event = match message {
                Ok(WorkerMsg::Input(input)) => self.translate(input),
                Ok(WorkerMsg::TogglePause) => {
                    let command = self.session.toggle_pause();
                    self.execute(command, owner.as_ref());
                    continue;
                }
                Ok(WorkerMsg::ReloadConfig) => {
                    self.reload_config(LoadTrigger::TrayCommand);
                    continue;
                }
                Ok(WorkerMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) if last_tick.elapsed() >= TICK => {
                    last_tick = Instant::now();
                    reconcile_modifiers();
                    if last_config_check.elapsed() >= CONFIG_POLL {
                        last_config_check = Instant::now();
                        self.reload_config_if_changed();
                    }
                    InputEvent::Tick { time: self.now() }
                }
                Err(RecvTimeoutError::Timeout) => continue,
            };
            if let Some(command) = self.session.handle(event) {
                self.execute(command, owner.as_ref());
            }
        }
        drop(owner);
        log::info!("worker stopped");
    }

    fn translate(&mut self, input: HookInput) -> InputEvent {
        match input {
            HookInput::Key { key, mods } => InputEvent::KeyDown {
                key,
                mods,
                time: self.now(),
            },
            HookInput::KeyUp { vk } => InputEvent::KeyUp { vk },
            HookInput::MouseClick => InputEvent::MouseClick,
            HookInput::FocusChanged => {
                self.update_exclusion();
                InputEvent::FocusChanged
            }
        }
    }

    fn execute(&mut self, command: Command, owner: Option<&OwnerWindow>) {
        match command {
            Command::PauseChanged { paused } => {
                PAUSED.store(paused, Ordering::Relaxed);
                self.tray.notify_paused(paused);
                log::info!("{}", if paused { "paused" } else { "resumed" });
            }
            Command::Convert(scope) => self.convert(scope),
            Command::ConvertSelection => match owner {
                Some(owner) => self.convert_selection(owner),
                None => log::warn!("convert selection: no clipboard owner window"),
            },
        }
    }

    /// Converts the last word or line. Logs counts only, never content (P4).
    fn convert(&mut self, scope: Scope) {
        let Some(current) = self.executor.foreground_layout() else {
            log::warn!("convert {scope:?}: foreground layout unknown");
            return;
        };
        let Some(plan) = self.session.plan(scope, &current) else {
            log::info!("convert {scope:?}: nothing to convert");
            return;
        };
        match self.executor.run(&plan) {
            Ok(()) => log::info!("converted {scope:?}, {} keys", replayed_keys(&plan)),
            Err(err) => {
                log::warn!("convert {scope:?} not run: {err}");
                self.session.clear();
            }
        }
    }

    /// Converts the selected text. Logs counts only, never content (P4).
    fn convert_selection(&mut self, owner: &OwnerWindow) {
        let config = self.session.config();
        let result = self.executor.convert_selection(
            owner.hwnd(),
            config.switch_layout_after_convert,
            |text, current, tables| convert_selection(text, current, config, tables),
        );
        match result {
            Ok(SelectionOutcome::Converted { chars }) => {
                log::info!("converted selection, {chars} chars");
            }
            Ok(outcome) => log::info!("convert selection: {outcome:?}"),
            Err(err) => log::warn!("convert selection failed: {err}"),
        }
    }

    /// Checks the foreground app against `exclusions.processes`. Process
    /// names are not logged: which apps the user runs is private too.
    fn update_exclusion(&mut self) {
        let excluded =
            foreground_exe_path().is_some_and(|path| self.session.config().is_excluded_path(&path));
        if excluded != self.session.is_excluded() {
            log::debug!("foreground app excluded: {excluded}");
        }
        self.session.set_excluded(excluded);
        EXCLUDED.store(excluded, Ordering::Relaxed);
    }

    fn reload_config_if_changed(&mut self) {
        if modified(&self.paths) != self.maybe_config_mtime {
            self.reload_config(LoadTrigger::FileChanged);
        }
    }

    /// Applies the config file as `retype_core::load` decides: a broken file
    /// keeps the running config.
    fn reload_config(&mut self, trigger: LoadTrigger) {
        self.maybe_config_mtime = modified(&self.paths);
        let result = load_config(read_config_file(&self.paths), trigger);
        if let Some(error) = result.maybe_error {
            log::error!("config not reloaded: {error}");
        }
        let Some(config) = result.maybe_config else {
            return;
        };
        log::set_max_level(logger::level_filter(config.log_level));
        hook::set_hotkeys(config.hotkeys);
        self.executor.warn_missing(&config.layouts);
        self.session.set_config(config);
        self.update_exclusion();
        log::info!("config reloaded");
    }
}

fn modified(paths: &Paths) -> Option<SystemTime> {
    fs::metadata(&paths.config_file)
        .and_then(|meta| meta.modified())
        .ok()
}

fn replayed_keys(plan: &ConversionPlan) -> usize {
    plan.ops
        .iter()
        .map(|op| match op {
            PlanOp::Replay(keys) => keys.len(),
            _ => 0,
        })
        .sum()
}
