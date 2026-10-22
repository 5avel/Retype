//! Runs conversion plans: the core decides the steps, this module performs
//! them with `SendInput` and layout switches.

use std::thread;
use std::time::Duration;

use retype_core::inject::{Step, execution_steps, restore_modifiers};
use retype_core::layout::{LayoutId, LayoutTable, missing_layouts};
use retype_core::plan::{ConversionPlan, PlanOp};

use super::hook::{physical_modifiers, reconcile_modifiers};
use super::input::send_input_keys;
use super::layout::{Foreground, Hkl, installed_layouts};

/// How long to wait for a requested layout switch to apply.
pub(super) const SWITCH_TIMEOUT: Duration = Duration::from_millis(300);

/// Time for the target app to process injected keys before a layout switch
/// (see [`Step::Settle`]).
const SETTLE: Duration = Duration::from_millis(100);

/// Why a plan was not run, or stopped part way. Either way the screen may
/// no longer match the session's buffer.
#[derive(Debug)]
pub enum ExecError {
    NoForegroundWindow,
    /// The foreground layout has no language tag we understand.
    UnknownLayout,
    LayoutNotInstalled(LayoutId),
    /// Another window came to the foreground while the plan ran.
    FocusChanged,
    /// `SendInput` injected fewer keys than asked.
    Injection(windows::core::Error),
    /// The clipboard could not be opened or written.
    Clipboard(windows::core::Error),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoForegroundWindow => f.write_str("no foreground window"),
            Self::UnknownLayout => f.write_str("foreground layout unknown"),
            Self::LayoutNotInstalled(id) => write!(f, "layout {id} is not installed"),
            Self::FocusChanged => f.write_str("focus changed during conversion"),
            Self::Injection(err) => write!(f, "SendInput injected fewer keys than asked: {err}"),
            Self::Clipboard(err) => write!(f, "clipboard: {err}"),
        }
    }
}

/// Knows the installed layouts and executes plans in the foreground window.
pub struct Executor {
    layouts: Vec<(LayoutId, Hkl)>,
    /// Character tables of the installed layouts, one per language tag.
    pub(super) tables: Vec<(LayoutId, LayoutTable)>,
}

impl Executor {
    pub fn new(configured: &[LayoutId]) -> Self {
        let mut executor = Self {
            layouts: Vec::new(),
            tables: Vec::new(),
        };
        executor.refresh();
        log::info!("{} keyboard layouts installed", executor.layouts.len());
        executor.warn_missing(configured);
        executor
    }

    /// Logs configured layouts that are not installed (tags only).
    pub fn warn_missing(&self, configured: &[LayoutId]) {
        let installed: Vec<LayoutId> = self.tables.iter().map(|(id, _)| id.clone()).collect();
        for id in missing_layouts(configured, &installed) {
            log::warn!("configured layout {id} is not installed");
        }
    }

    /// The foreground window's layout as the core sees it.
    pub fn foreground_layout(&mut self) -> Option<LayoutId> {
        let hkl = Foreground::current()?.hkl;
        self.layout_id_of(hkl)
    }

    /// Like [`Executor::foreground_layout`] but never rebuilds the tables;
    /// cheap enough to poll for the tray indicator.
    pub fn foreground_layout_cached(&self) -> Option<LayoutId> {
        let hkl = Foreground::current()?.hkl;
        match self.layouts.iter().find(|(_, h)| *h == hkl) {
            Some((id, _)) => Some(id.clone()),
            None => hkl.layout_id(),
        }
    }

    pub(super) fn layout_id_of(&mut self, hkl: Hkl) -> Option<LayoutId> {
        if let Some((id, _)) = self.layouts.iter().find(|(_, h)| *h == hkl) {
            return Some(id.clone());
        }
        // Installed after startup: refresh, but work even if still unknown.
        self.refresh();
        hkl.layout_id()
    }

    /// Re-reads the installed layouts and builds their character tables.
    fn refresh(&mut self) {
        self.layouts = installed_layouts();
        self.tables.clear();
        for (id, hkl) in &self.layouts {
            if !self.tables.iter().any(|(known, _)| known == id) {
                self.tables.push((id.clone(), hkl.table()));
            }
        }
    }

    /// Runs `plan`. Fails before injecting anything if a target layout is
    /// missing; stops at the first failed key step (so a lost Backspace never
    /// leads to a doubled word) or when the foreground window changes.
    /// A switch that does not confirm is logged and the plan goes on: the
    /// replayed keys then retype the original text, so nothing is lost.
    pub fn run(&mut self, plan: &ConversionPlan) -> Result<(), ExecError> {
        let foreground = Foreground::current().ok_or(ExecError::NoForegroundWindow)?;
        for op in &plan.ops {
            if let PlanOp::SwitchLayout(id) = op {
                self.hkl_of(id)?;
            }
        }
        with_held_modifiers(|held| self.run_steps(plan, &foreground, held))
    }

    fn run_steps(
        &self,
        plan: &ConversionPlan,
        foreground: &Foreground,
        held: &[u16],
    ) -> Result<(), ExecError> {
        for step in execution_steps(plan, held) {
            if !foreground.is_current() {
                return Err(ExecError::FocusChanged);
            }
            match step {
                Step::Keys(actions) => {
                    send_input_keys(&actions).map_err(ExecError::Injection)?;
                }
                Step::SwitchLayout(id) => {
                    let hkl = self.hkl_of(&id)?;
                    if !foreground.switch_layout(hkl, SWITCH_TIMEOUT) {
                        log::warn!("switch to {id} not confirmed within {SWITCH_TIMEOUT:?}");
                    }
                }
                Step::Settle => thread::sleep(SETTLE),
            }
        }
        Ok(())
    }

    pub(super) fn hkl_of(&self, id: &LayoutId) -> Result<Hkl, ExecError> {
        self.layouts
            .iter()
            .find(|(installed, _)| installed == id)
            .map(|(_, hkl)| *hkl)
            .ok_or_else(|| ExecError::LayoutNotInstalled(id.clone()))
    }
}

/// Runs `f` with the modifiers the user physically holds, then presses again
/// those released by `f` that are still held — also after a failure, so they
/// never stay half-applied.
pub(super) fn with_held_modifiers<T>(f: impl FnOnce(&[u16]) -> T) -> T {
    reconcile_modifiers();
    let held_before = physical_modifiers();
    let result = f(&held_before);
    let restore = restore_modifiers(&held_before, &physical_modifiers());
    if !restore.is_empty()
        && let Err(err) = send_input_keys(&restore)
    {
        log::warn!("could not restore held modifiers: {err}");
    }
    result
}
