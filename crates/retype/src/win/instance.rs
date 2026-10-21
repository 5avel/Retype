//! Single-instance guard: a named mutex in the user's session.

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

/// Held for the lifetime of the process; releases the mutex on drop.
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateMutexW and is closed exactly once.
        if let Err(err) = unsafe { CloseHandle(self.0) } {
            log::warn!("close single-instance mutex: {err}");
        }
    }
}

/// Returns `None` if another retype is already running in this session.
pub fn acquire() -> Option<InstanceGuard> {
    // SAFETY: no security attributes, a static NUL-terminated name.
    let handle = unsafe { CreateMutexW(None, false, w!("Local\\retype-single-instance")) };
    let handle = match handle {
        Ok(handle) => handle,
        Err(err) => {
            log::error!("single-instance mutex failed: {err}");
            return None;
        }
    };
    let guard = InstanceGuard(handle);
    // SAFETY: reads the calling thread's last error, set by CreateMutexW.
    let last_error = unsafe { GetLastError() };
    (last_error != ERROR_ALREADY_EXISTS).then_some(guard)
}
