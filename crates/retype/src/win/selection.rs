//! Selection conversion (SPEC 5.3): copy, convert in the core, paste,
//! restore the clipboard (P6).

use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;

use retype_core::inject::{copy_keys, paste_keys, release_modifiers, shortcut_release};
use retype_core::layout::{LayoutId, LayoutTable};
use retype_core::selection::{SelectionConversion, wipe_string};

use super::clipboard::{self, Clipboard, Snapshot};
use super::executor::{ExecError, Executor, SWITCH_TIMEOUT, with_held_modifiers};
use super::input::send_input_keys;
use super::layout::Foreground;

/// How long the app gets to put the selection on the clipboard. No change
/// within it means nothing was selected.
const COPY_TIMEOUT: Duration = Duration::from_millis(500);

/// Extra wait before deciding the clipboard was not touched: a copy landing
/// late still replaced the user's contents and must be undone.
const LATE_COPY_GRACE: Duration = Duration::from_millis(300);

/// The app reads the clipboard when it processes Shift+Insert, some time
/// after we inject it; restoring earlier would paste the original contents.
const PASTE_SETTLE: Duration = Duration::from_millis(400);

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Restoring the clipboard is retried this many times, this far apart.
const RESTORE_ATTEMPTS: u32 = 5;
const RESTORE_RETRY: Duration = Duration::from_millis(200);

/// What happened, for the log (counts only, P4).
#[derive(Debug)]
pub enum SelectionOutcome {
    NothingSelected,
    NothingToConvert,
    Converted { chars: usize },
}

/// The core's decision: copied text, foreground layout, layout tables.
pub trait Decide:
    FnOnce(&str, &LayoutId, &[(LayoutId, LayoutTable)]) -> Option<SelectionConversion>
{
}

impl<F> Decide for F where
    F: FnOnce(&str, &LayoutId, &[(LayoutId, LayoutTable)]) -> Option<SelectionConversion>
{
}

impl Executor {
    /// Converts the selection in the foreground window, using `owner` as the
    /// clipboard owner. With `switch_after`, the window then switches to the
    /// target layout.
    pub fn convert_selection(
        &mut self,
        owner: HWND,
        switch_after: bool,
        decide: impl Decide,
    ) -> Result<SelectionOutcome, ExecError> {
        let foreground = Foreground::current().ok_or(ExecError::NoForegroundWindow)?;
        let current = self
            .layout_id_of(foreground.hkl)
            .ok_or(ExecError::UnknownLayout)?;
        let (outcome, maybe_target) = with_held_modifiers(|held| {
            self.copy_convert_paste(owner, &foreground, &current, held, decide)
        })?;
        if switch_after
            && let Some(target) = maybe_target
            && let Ok(hkl) = self.hkl_of(&target)
            && !foreground.switch_layout(hkl, SWITCH_TIMEOUT)
        {
            log::warn!("switch to {target} not confirmed within {SWITCH_TIMEOUT:?}");
        }
        Ok(outcome)
    }

    fn copy_convert_paste(
        &self,
        owner: HWND,
        foreground: &Foreground,
        current: &LayoutId,
        held: &[u16],
        decide: impl Decide,
    ) -> Result<(SelectionOutcome, Option<LayoutId>), ExecError> {
        send_input_keys(&release_modifiers(held)).map_err(ExecError::Injection)?;
        let snapshot = Clipboard::open(owner)
            .map_err(ExecError::Clipboard)?
            .snapshot();
        let before = clipboard::sequence_number();
        let changed = || clipboard::sequence_number() != before;

        let result = match send_shortcut(&copy_keys()) {
            Ok(()) if wait_until(COPY_TIMEOUT, changed) => {
                self.paste_converted(owner, foreground, current, decide)
            }
            Ok(()) => Ok((SelectionOutcome::NothingSelected, None)),
            Err(err) => Err(err),
        };
        // Restore whenever the clipboard changed, however we got here: a
        // late or partial copy also replaced the user's contents.
        if changed() || {
            thread::sleep(LATE_COPY_GRACE);
            changed()
        } {
            restore(owner, &snapshot);
        }
        result
    }

    /// Reads the copied text, converts it and pastes the result. Returns the
    /// target layout when something was pasted.
    fn paste_converted(
        &self,
        owner: HWND,
        foreground: &Foreground,
        current: &LayoutId,
        decide: impl Decide,
    ) -> Result<(SelectionOutcome, Option<LayoutId>), ExecError> {
        let maybe_text = Clipboard::open(owner)
            .map_err(ExecError::Clipboard)?
            .read_text();
        let Some(text) = maybe_text else {
            return Ok((SelectionOutcome::NothingToConvert, None)); // e.g. an image
        };
        let maybe_conversion = decide(&text, current, &self.tables);
        wipe_string(text);
        let Some(conversion) = maybe_conversion else {
            return Ok((SelectionOutcome::NothingToConvert, None));
        };
        let chars = conversion.text.chars().count();
        let written = if foreground.is_current() {
            Clipboard::open(owner)
                .and_then(|clipboard| clipboard.write_text(&conversion.text))
                .map_err(ExecError::Clipboard)
        } else {
            Err(ExecError::FocusChanged)
        };
        wipe_string(conversion.text);
        written?;
        send_shortcut(&paste_keys())?;
        thread::sleep(PASTE_SETTLE);
        Ok((
            SelectionOutcome::Converted { chars },
            Some(conversion.target),
        ))
    }
}

/// Injects a Ctrl/Shift shortcut. If only part of it went through, the
/// modifier could stay logically down: release it.
fn send_shortcut(keys: &[retype_core::inject::KeyAction]) -> Result<(), ExecError> {
    send_input_keys(keys).map_err(|err| {
        if let Err(release_err) = send_input_keys(&shortcut_release()) {
            log::warn!("could not release shortcut modifiers: {release_err}");
        }
        ExecError::Injection(err)
    })
}

/// Puts the user's clipboard back (P6). Each attempt already retries
/// opening for ~200 ms; another app holding the clipboard longer gets a few
/// more chances before we give up.
fn restore(owner: HWND, snapshot: &Snapshot) {
    for attempt in 1..=RESTORE_ATTEMPTS {
        match Clipboard::open(owner).and_then(|clipboard| clipboard.restore(snapshot)) {
            Ok(0) => return,
            Ok(lost) => {
                log::warn!("clipboard restored without {lost} formats");
                return;
            }
            Err(err) if attempt == RESTORE_ATTEMPTS => {
                log::error!("clipboard not restored: {err}");
            }
            Err(_) => thread::sleep(RESTORE_RETRY),
        }
    }
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        thread::sleep(POLL_INTERVAL);
    }
    done()
}
