//! Keyboard layouts: installed list, foreground layout, switching.
//!
//! Layout is per thread; another app's layout is changed by posting
//! `WM_INPUTLANGCHANGEREQUEST` and is applied asynchronously.

use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Globalization::LCIDToLocaleName;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutList, HKL};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, WM_INPUTLANGCHANGEREQUEST,
};

use retype_core::layout::LayoutId;

/// `LOCALE_NAME_MAX_LENGTH` (in UTF-16 units, including the NUL).
const LOCALE_NAME_MAX_LENGTH: usize = 85;

/// How often to check whether a requested switch has applied.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// A keyboard layout handle (`HKL`), stored as an integer so it is `Send`.
/// It is an identifier, not an owned resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hkl(isize);

impl Hkl {
    fn from_raw(hkl: HKL) -> Self {
        Self(hkl.0 as isize)
    }

    fn to_raw(self) -> HKL {
        HKL(self.0 as *mut _)
    }

    /// The language tag of the layout's language (low word of the HKL),
    /// e.g. `ru-ru`. Variants of one language (Dvorak, typewriter) share it.
    pub fn layout_id(self) -> Option<LayoutId> {
        let language = (self.0 as usize & 0xFFFF) as u32;
        let mut name = [0u16; LOCALE_NAME_MAX_LENGTH];
        // SAFETY: `name` is a writable buffer whose length the wrapper passes;
        // the call writes at most that many units.
        let written = unsafe { LCIDToLocaleName(language, Some(&mut name), 0) };
        let len = usize::try_from(written).ok()?.checked_sub(1)?; // minus NUL
        LayoutId::new(&String::from_utf16_lossy(&name[..len]))
    }
}

/// The foreground window and the layout of the thread that owns it.
#[derive(Debug, Clone, Copy)]
pub struct Foreground {
    hwnd: isize,
    pub hkl: Hkl,
}

impl Foreground {
    /// Reads the foreground window's layout. `None` without a foreground
    /// window (e.g. the desktop is locked).
    pub fn current() -> Option<Self> {
        // SAFETY: no preconditions; may return null.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return None;
        }
        Some(Self {
            hwnd: hwnd.0 as isize,
            hkl: layout_of(hwnd),
        })
    }

    /// True while this window is still in the foreground.
    pub fn is_current(&self) -> bool {
        // SAFETY: no preconditions; may return null.
        let hwnd = unsafe { GetForegroundWindow() };
        hwnd.0 as isize == self.hwnd
    }

    /// Asks the window to switch to `hkl` and waits up to `timeout` for the
    /// switch to apply. Returns false if it did not (consoles often report
    /// layouts unreliably, see `docs/winapi-notes.md`).
    pub fn switch_layout(&self, hkl: Hkl, timeout: Duration) -> bool {
        let hwnd = HWND(self.hwnd as *mut _);
        if layout_of(hwnd) == hkl {
            return true;
        }
        let lparam = LPARAM(hkl.to_raw().0 as isize);
        // SAFETY: posting to a window handle passes no pointers; a stale
        // handle only makes the call fail.
        let posted =
            unsafe { PostMessageW(Some(hwnd), WM_INPUTLANGCHANGEREQUEST, WPARAM(0), lparam) };
        if let Err(err) = posted {
            log::warn!("layout switch request failed: {err}");
            return false;
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            thread::sleep(POLL_INTERVAL);
            if layout_of(hwnd) == hkl {
                return true;
            }
        }
        false
    }
}

/// Installed layouts with their language tags, in the user's order.
pub fn installed_layouts() -> Vec<(LayoutId, Hkl)> {
    // SAFETY: a `None` buffer only asks for the count.
    let count = unsafe { GetKeyboardLayoutList(None) };
    let mut list = vec![HKL::default(); usize::try_from(count).unwrap_or(0)];
    // SAFETY: `list` is a writable buffer whose length the wrapper passes.
    let written = unsafe { GetKeyboardLayoutList(Some(&mut list)) };
    list.truncate(usize::try_from(written).unwrap_or(0));
    list.into_iter()
        .map(Hkl::from_raw)
        .filter_map(|hkl| Some((hkl.layout_id()?, hkl)))
        .collect()
}

fn layout_of(hwnd: HWND) -> Hkl {
    // SAFETY: a stale handle only yields thread id 0.
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    // SAFETY: any thread id is accepted. Thread id 0 (window gone) reads our
    // own thread's layout, which then simply fails to match.
    Hkl::from_raw(unsafe { GetKeyboardLayout(thread_id) })
}
