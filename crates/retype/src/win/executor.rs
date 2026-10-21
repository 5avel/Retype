//! Runs conversion plans: the core decides the steps, this module performs
//! them with `SendInput` and layout switches.

use std::thread;
use std::time::Duration;

use retype_core::inject::{Step, execution_steps, restore_modifiers};
use retype_core::layout::{LayoutId, missing_layouts};
use retype_core::plan::{ConversionPlan, PlanOp};

use super::hook::physical_modifiers;
use super::input::send_input_keys;
use super::layout::{Foreground, Hkl, installed_layouts};

/// How long to wait for a requested layout switch to apply.
const SWITCH_TIMEOUT: Duration = Duration::from_millis(300);

/// Time for the target app to process injected keys before a layout switch
/// (see [`Step::Settle`]).
const SETTLE: Duration = Duration::from_millis(100);

/// Why a plan was not run, or stopped part way. Either way the screen may
/// no longer match the session's buffer.
#[derive(Debug)]
pub enum ExecError {
    NoForegroundWindow,
    LayoutNotInstalled(LayoutId),
    /// Another window came to the foreground while the plan ran.
    FocusChanged,
    /// `SendInput` injected fewer keys than asked.
    Injection(windows::core::Error),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoForegroundWindow => f.write_str("no foreground window"),
            Self::LayoutNotInstalled(id) => write!(f, "layout {id} is not installed"),
            Self::FocusChanged => f.write_str("focus changed during conversion"),
            Self::Injection(err) => write!(f, "SendInput injected fewer keys than asked: {err}"),
        }
    }
}

/// Knows the installed layouts and executes plans in the foreground window.
pub struct Executor {
    layouts: Vec<(LayoutId, Hkl)>,
}

impl Executor {
    pub fn new(configured: &[LayoutId]) -> Self {
        let layouts = installed_layouts();
        log::info!("{} keyboard layouts installed", layouts.len());
        let installed: Vec<LayoutId> = layouts.iter().map(|(id, _)| id.clone()).collect();
        for id in missing_layouts(configured, &installed) {
            log::warn!("configured layout {id} is not installed");
        }
        Self { layouts }
    }

    /// The foreground window's layout as the core sees it.
    pub fn foreground_layout(&mut self) -> Option<LayoutId> {
        let hkl = Foreground::current()?.hkl;
        if let Some((id, _)) = self.layouts.iter().find(|(_, h)| *h == hkl) {
            return Some(id.clone());
        }
        // Installed after startup: refresh, but work even if still unknown.
        self.layouts = installed_layouts();
        hkl.layout_id()
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
        let held_before = physical_modifiers();
        let result = self.run_steps(plan, &foreground, &held_before);
        // Also after a failure: released modifiers must not stay half-applied.
        let restore = restore_modifiers(&held_before, &physical_modifiers());
        if !restore.is_empty()
            && let Err(err) = send_input_keys(&restore)
        {
            log::warn!("could not restore held modifiers: {err}");
        }
        result
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

    fn hkl_of(&self, id: &LayoutId) -> Result<Hkl, ExecError> {
        self.layouts
            .iter()
            .find(|(installed, _)| installed == id)
            .map(|(_, hkl)| *hkl)
            .ok_or_else(|| ExecError::LayoutNotInstalled(id.clone()))
    }
}
