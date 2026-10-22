//! Keyboard input injection with `SendInput`.

use std::mem::size_of;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, SendInput, VIRTUAL_KEY,
};

use retype_core::inject::{KeyAction, KeyCode};

/// Injects `actions` in one call, so other input cannot interleave.
///
/// Input to windows of a higher integrity level is silently blocked by UIPI;
/// that is not reported as an error (SPEC §3).
pub fn send_input_keys(actions: &[KeyAction]) -> windows::core::Result<()> {
    let mut inputs: Vec<INPUT> = actions.iter().map(to_input).collect();
    let size = i32::try_from(size_of::<INPUT>()).unwrap_or(i32::MAX);
    // SAFETY: `inputs` is a valid slice of initialised keyboard INPUTs and
    // `size` is the size of one element, as the API requires.
    let sent = unsafe { SendInput(&inputs, size) };
    let count = inputs.len();
    // The array holds replayed scan codes: overwrite before freeing it.
    inputs.fill(INPUT::default());
    std::hint::black_box(&inputs);
    if usize::try_from(sent).unwrap_or(0) == count {
        Ok(())
    } else {
        Err(windows::core::Error::from_thread())
    }
}

fn to_input(action: &KeyAction) -> INPUT {
    let (vk, scan, mut flags) = match action.code {
        KeyCode::Scan { scan, extended } => {
            let mut flags = KEYEVENTF_SCANCODE;
            if extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            (0, scan, flags)
        }
        KeyCode::Vk(vk) => (vk, 0, KEYBD_EVENT_FLAGS(0)),
    };
    if !action.down {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
