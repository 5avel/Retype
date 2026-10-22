//! Worker thread: feeds hook input into the core session and acts on commands.

use std::fs;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use retype_core::event::{InputEvent, Millis};
use retype_core::indicator::Indicator;
use retype_core::inject::wipe_actions;
use retype_core::keystroke::{Keystroke, Modifiers, vk};
use retype_core::layout::LayoutId;
use retype_core::load::{LoadTrigger, load_config};
use retype_core::plan::{ConversionPlan, PlanOp, Scope};
use retype_core::selection::convert_selection;
use retype_core::session::{Command, Session};

use super::executor::Executor;
use super::hook::{self, EXCLUDED, HOLD_ARMED, OVERFLOW, PAUSED, reconcile_modifiers};
use super::input::send_input_keys;
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

/// How often the foreground layout is read for the tray indicator: the
/// user's own switches (Alt+Shift, Win+Space) send no event we can hook.
const INDICATOR_POLL: Duration = Duration::from_millis(200);

/// Held keys are released after this long even if the Space that started
/// holding never reached the worker (queue overflow).
const MAX_HOLD: Millis = 500;

/// How often the config file's modification time is checked (SPEC 5.5).
const CONFIG_POLL: Duration = Duration::from_secs(2);

/// Input as captured by the hook callbacks.
#[derive(Debug, Clone, Copy)]
pub enum HookInput {
    /// `time` is taken in the hook ([`now_millis`]), so a worker busy with a
    /// conversion does not squeeze queued key timings together.
    Key {
        key: Keystroke,
        mods: Modifiers,
        time: Millis,
    },
    KeyUp {
        vk: u16,
        time: Millis,
    },
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

/// Milliseconds on retype's monotonic clock, shared by the hook (event
/// times) and the worker (ticks).
pub fn now_millis() -> Millis {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    Millis::try_from(start.elapsed().as_millis()).unwrap_or(Millis::MAX)
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
    /// What the tray shows now.
    maybe_indicator: Option<Indicator>,
    /// The last foreground layout we could read, for the indicator.
    maybe_last_layout: Option<LayoutId>,
}

impl Worker {
    fn new(session: Session, tray: TrayHandle, paths: Paths) -> Self {
        Self {
            executor: Executor::new(&session.config().layouts),
            maybe_config_mtime: modified(&paths),
            maybe_indicator: None,
            maybe_last_layout: None,
            session,
            tray,
            paths,
        }
    }

    fn now(&self) -> Millis {
        now_millis()
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
        self.update_indicator();
        let mut last_tick = Instant::now();
        let mut last_config_check = Instant::now();
        let mut last_indicator = Instant::now();
        loop {
            let message = rx.recv_timeout(PUMP_INTERVAL);
            if let Some(owner) = &owner {
                owner.pump();
            }
            if hook::hold_age().is_some_and(|age| age >= MAX_HOLD) {
                log::warn!("held keys released by timeout");
                release_held_keys();
            }
            if last_indicator.elapsed() >= INDICATOR_POLL {
                last_indicator = Instant::now();
                self.update_indicator();
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
                    // Also covers missing focus hooks (SetWinEventHook failed).
                    self.update_exclusion();
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
            // The hook held what was typed after this Space until the
            // autoswitch decision above; let it through now.
            if matches!(event, InputEvent::KeyDown { key, .. } if key.vk == vk::SPACE) {
                release_held_keys();
            }
        }
        release_held_keys();
        drop(owner);
        log::info!("worker stopped");
    }

    fn translate(&mut self, input: HookInput) -> InputEvent {
        match input {
            HookInput::Key { key, mods, time } => InputEvent::KeyDown { key, mods, time },
            HookInput::KeyUp { vk, time } => InputEvent::KeyUp { vk, time },
            HookInput::MouseClick => {
                self.update_exclusion();
                InputEvent::MouseClick
            }
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
                self.update_hold_armed();
                self.update_indicator();
                log::info!("{}", if paused { "paused" } else { "resumed" });
            }
            Command::Convert(scope) => self.convert(scope),
            Command::AutoSwitch => self.autoswitch(),
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

    /// Fixes the word just finished if it was typed in the wrong layout.
    /// Logs counts only, never content (P4).
    fn autoswitch(&mut self) {
        let Some(current) = self.executor.foreground_layout() else {
            return;
        };
        let Some(plan) = self
            .session
            .plan_autoswitch(&current, &self.executor.tables)
        else {
            return;
        };
        match self.executor.run(&plan) {
            Ok(()) => log::info!("autoswitched a word, {} keys", replayed_keys(&plan)),
            Err(err) => {
                log::warn!("autoswitch not run: {err}");
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

    /// Sends the tray a new indicator when the foreground layout or the pause
    /// state changed. An unreadable layout (no foreground window) keeps the
    /// last known one, so a pause change is still shown.
    fn update_indicator(&mut self) {
        if let Some(layout) = self.executor.foreground_layout_cached() {
            self.maybe_last_layout = Some(layout);
        }
        let config = self.session.config();
        let indicator = Indicator::new(
            self.maybe_last_layout.as_ref(),
            &config.layouts,
            self.session.is_paused(),
        );
        if self.maybe_indicator.as_ref() != Some(&indicator) {
            self.tray.notify_indicator(indicator.clone());
            self.maybe_indicator = Some(indicator);
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
        self.update_hold_armed();
    }

    /// Lets the hook hold keys after Space only while autoswitch may act.
    fn update_hold_armed(&self) {
        let session = &self.session;
        let armed = session.config().autoswitch && !session.is_paused() && !session.is_excluded();
        HOLD_ARMED.store(armed, Ordering::Relaxed);
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
        self.update_indicator();
        log::info!("config reloaded");
    }
}

/// Replays the keys the hook held after a Space, in order (they are
/// injected, so the hook does not record them twice).
fn release_held_keys() {
    let mut held = hook::release_held();
    if held.is_empty() {
        return;
    }
    if let Err(err) = send_input_keys(&held) {
        log::warn!("held keys not all replayed: {err}");
    }
    wipe_actions(&mut held);
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
