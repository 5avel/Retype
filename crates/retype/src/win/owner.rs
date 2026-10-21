//! A hidden message-only window on the worker thread that owns the
//! clipboard while retype writes to it.
//!
//! Other apps *send* `WM_DESTROYCLIPBOARD` to the owner when they empty the
//! clipboard, so the owner thread must pump messages ([`OwnerWindow::pump`]),
//! and its lifetime must not depend on the main thread's shutdown.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, HWND_MESSAGE, MSG, PM_REMOVE,
    PeekMessageW, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::w;

/// The window; destroyed on drop, on the thread that created it.
pub struct OwnerWindow(HWND);

impl OwnerWindow {
    pub fn create() -> Result<Self, String> {
        // SAFETY: NULL name returns this exe's module handle; no ownership.
        let module = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
        let class_name = w!("retype-clipboard-owner");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        // SAFETY: `class` is fully initialised and its strings are static.
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err("RegisterClassW failed".to_owned());
        }
        // SAFETY: the class was registered above; HWND_MESSAGE makes it a
        // message-only window (never shown, no broadcasts).
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                w!("retype clipboard"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(module.into()),
                None,
            )
        }
        .map_err(|e| format!("CreateWindowExW: {e}"))?;
        Ok(Self(hwnd))
    }

    pub fn hwnd(&self) -> HWND {
        self.0
    }

    /// Handles all pending messages for this thread without blocking.
    pub fn pump(&self) {
        let mut msg = MSG::default();
        // SAFETY: `msg` is a valid out pointer for the duration of the call.
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: `msg` was filled by PeekMessageW. The BOOL only says
            // whether a character message was generated; it is not an error.
            let _ = unsafe { TranslateMessage(&msg) };
            // SAFETY: `msg` was filled by PeekMessageW.
            unsafe { DispatchMessageW(&msg) };
        }
    }
}

impl Drop for OwnerWindow {
    fn drop(&mut self) {
        // SAFETY: the window was created on this thread and is destroyed once.
        if let Err(err) = unsafe { DestroyWindow(self.0) } {
            log::warn!("DestroyWindow (clipboard owner): {err}");
        }
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: default handling of a message for our own window.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
