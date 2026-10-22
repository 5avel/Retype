//! The executable of the foreground window's process, for
//! `exclusions.processes`.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::PWSTR;

/// Runs on every focus change, so the buffer stays small. A longer path
/// makes the query fail and the app is simply not excluded.
const MAX_PATH_UNITS: usize = 1024;

/// Full path of the foreground process's executable. `None` without a
/// foreground window or when the process cannot be queried (e.g. protected
/// system processes); such apps are then not excluded.
pub fn foreground_exe_path() -> Option<String> {
    // SAFETY: no preconditions; may return null.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer; a stale handle yields 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    // SAFETY: limited query rights only; the handle is closed by `Process`.
    let process =
        Process(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    let mut path = vec![0u16; MAX_PATH_UNITS];
    let mut len = u32::try_from(path.len()).unwrap_or(u32::MAX);
    // SAFETY: `path` is writable for `len` units; `len` receives the length
    // written, without the NUL.
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    path.truncate(usize::try_from(len).unwrap_or(0));
    Some(String::from_utf16_lossy(&path))
}

/// A process handle, closed on drop.
struct Process(HANDLE);

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed once.
        if let Err(err) = unsafe { CloseHandle(self.0) } {
            log::warn!("CloseHandle (process): {err}");
        }
    }
}
