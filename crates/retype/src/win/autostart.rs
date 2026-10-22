//! Opt-in autostart via the `HKCU\...\Run` key (ADR-007). The registry value
//! is the only state: the tray reads it to show the checkbox.

use std::env;

use windows::Win32::Foundation::{E_FAIL, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey,
    RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

use crate::paths::{autostart_command, autostart_needs_update, autostart_target};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: PCWSTR = w!("retype");

/// True if retype starts at logon.
pub fn is_enabled() -> bool {
    read_value().is_some()
}

/// Turns autostart on (with the current exe path) or off.
pub fn set_enabled(enabled: bool) -> windows::core::Result<()> {
    let key = RunKey::open()?;
    if enabled {
        let exe = env::current_exe().map_err(|_| windows::core::Error::from(E_FAIL))?;
        key.set(&autostart_command(&exe))
    } else {
        key.delete()
    }
}

/// If autostart is on but its exe no longer exists (it was moved), points it
/// at this exe.
pub fn refresh_path() {
    let (Some(current), Ok(exe)) = (read_value(), env::current_exe()) else {
        return;
    };
    let target_exists = autostart_target(&current).is_file();
    if autostart_needs_update(&current, &exe, target_exists) {
        match RunKey::open().and_then(|key| key.set(&autostart_command(&exe))) {
            Ok(()) => log::info!("autostart path updated"),
            Err(err) => log::warn!("autostart path update failed: {err}"),
        }
    }
}

fn read_value() -> Option<String> {
    let mut size = 0u32;
    // SAFETY: a size query: no data buffer, `size` is a valid out pointer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        if status != ERROR_FILE_NOT_FOUND {
            log::warn!("autostart read failed: {}", error(status));
        }
        return None;
    }
    let mut data = vec![0u16; usize::try_from(size).unwrap_or(0).div_ceil(2)];
    let mut size = u32::try_from(data.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: `data` is writable for `size` bytes; `size` receives the bytes
    // written, including the NUL that RRF_RT_REG_SZ guarantees.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let units = usize::try_from(size).unwrap_or(0) / 2;
    data.truncate(units);
    let end = data.iter().position(|&u| u == 0).unwrap_or(data.len());
    Some(String::from_utf16_lossy(&data[..end]))
}

/// The open `Run` key, closed on drop.
struct RunKey(HKEY);

impl RunKey {
    fn open() -> windows::core::Result<Self> {
        let mut key = HKEY::default();
        // SAFETY: `key` is a valid out pointer; the key is closed in `Drop`.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                None,
                KEY_READ | KEY_SET_VALUE,
                &mut key,
            )
        };
        check(status)?;
        Ok(Self(key))
    }

    fn set(&self, command: &str) -> windows::core::Result<()> {
        // REG_SZ data is UTF-16 including the terminating NUL.
        let bytes: Vec<u8> = command
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        // SAFETY: the key is open with KEY_SET_VALUE; `bytes` outlives the call.
        check(unsafe { RegSetValueExW(self.0, VALUE_NAME, None, REG_SZ, Some(&bytes)) })
    }

    fn delete(&self) -> windows::core::Result<()> {
        // SAFETY: the key is open with KEY_SET_VALUE.
        let status = unsafe { RegDeleteValueW(self.0, VALUE_NAME) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(()); // already off
        }
        check(status)
    }
}

impl Drop for RunKey {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `open` and is closed once.
        let status = unsafe { RegCloseKey(self.0) };
        if status != ERROR_SUCCESS {
            log::warn!("RegCloseKey: {}", error(status));
        }
    }
}

fn check(status: WIN32_ERROR) -> windows::core::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(error(status))
    }
}

fn error(status: WIN32_ERROR) -> windows::core::Error {
    windows::core::Error::from_hresult(status.to_hresult())
}
