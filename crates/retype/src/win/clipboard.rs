//! Clipboard access for selection conversion (SPEC 5.3, P6): snapshot and
//! restore, read and write text.
//!
//! Everything retype writes is marked to stay out of the clipboard history
//! (Win+V), clipboard monitors and cloud sync. The app's own copy (our
//! Ctrl+C) is outside our control; see ADR-011.

use std::hint::black_box;
use std::ptr;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::core::{PCWSTR, w};

/// `CF_UNICODETEXT` (defined here to avoid the large `Win32_System_Ole` feature).
const CF_UNICODETEXT: u32 = 13;

/// Snapshot limits. Delay-rendered formats make the source app render each
/// one while we hold the clipboard open (Excel offers dozens, some large).
const MAX_SNAPSHOT_FORMATS: usize = 64;
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;

/// `OpenClipboard` fails while another app holds the clipboard.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY: Duration = Duration::from_millis(20);

/// Registered formats that keep our writes private. Each gets a zero DWORD.
const PRIVACY_FORMATS: [PCWSTR; 3] = [
    w!("ExcludeClipboardContentFromMonitorProcessing"),
    w!("CanIncludeInClipboardHistory"),
    w!("CanUploadToCloudClipboard"),
];

/// The clipboard sequence number; it changes whenever the contents change.
pub fn sequence_number() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetClipboardSequenceNumber() }
}

/// Saved clipboard contents. Bytes are zeroed when dropped: the clipboard
/// may hold a password.
#[derive(Default)]
pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
    /// Formats not saved: GDI handles, or over the limits.
    skipped: usize,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        for (_, bytes) in &mut self.formats {
            bytes.fill(0);
            black_box(&bytes);
        }
    }
}

/// The open clipboard; closed on drop. Keep it open only briefly, and never
/// while injecting Ctrl+C/Ctrl+V: the target app needs it then.
pub struct Clipboard(());

impl Clipboard {
    /// Opens the clipboard, retrying while another app holds it. `owner`
    /// must be a window with a message loop: it becomes the clipboard owner
    /// and receives `WM_DESTROYCLIPBOARD` (a sent message) later.
    pub fn open(owner: HWND) -> windows::core::Result<Self> {
        let mut attempt = 1;
        loop {
            // SAFETY: `owner` is a window handle; a stale one only fails.
            match unsafe { OpenClipboard(Some(owner)) } {
                Ok(()) => return Ok(Self(())),
                Err(err) if attempt >= OPEN_ATTEMPTS => return Err(err),
                Err(_) => {
                    attempt += 1;
                    thread::sleep(OPEN_RETRY);
                }
            }
        }
    }

    /// Copies every format whose data is a plain memory block, within the
    /// snapshot limits. GDI-handle formats (bitmaps, metafiles) are skipped;
    /// Windows synthesises most of them from the DIB/text formats kept.
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::default();
        let mut total = 0;
        let mut format = 0;
        loop {
            // SAFETY: the clipboard is open (we hold `self`).
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            let room = MAX_SNAPSHOT_BYTES - total;
            let maybe_bytes = (is_memory_format(format)
                && snapshot.formats.len() < MAX_SNAPSHOT_FORMATS)
                .then(|| read_global(format, room))
                .flatten();
            match maybe_bytes {
                Some(bytes) => {
                    total += bytes.len();
                    snapshot.formats.push((format, bytes));
                }
                None => snapshot.skipped += 1,
            }
        }
        snapshot
    }

    /// The clipboard text, if any.
    pub fn read_text(&self) -> Option<String> {
        let mut bytes = read_global(CF_UNICODETEXT, usize::MAX)?;
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .take_while(|&unit| unit != 0)
            .collect();
        bytes.fill(0);
        black_box(&bytes);
        let text = String::from_utf16_lossy(&units);
        wipe_units(units);
        Some(text)
    }

    /// Replaces the clipboard with `text`, kept out of history and sync.
    pub fn write_text(&self, text: &str) -> windows::core::Result<()> {
        empty()?;
        let mut bytes: Vec<u8> = text
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        let result = set_bytes(CF_UNICODETEXT, &bytes);
        bytes.fill(0);
        black_box(&bytes);
        result?;
        mark_private();
        Ok(())
    }

    /// Puts the saved contents back, kept out of history and sync (the
    /// original entry is already there). Returns how many formats are lost:
    /// not saved, or failed to set.
    pub fn restore(&self, snapshot: &Snapshot) -> windows::core::Result<usize> {
        empty()?;
        let failed = snapshot
            .formats
            .iter()
            .filter(|(format, bytes)| set_bytes(*format, bytes).is_err())
            .count();
        if !snapshot.formats.is_empty() {
            mark_private();
        }
        Ok(failed + snapshot.skipped)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: we opened the clipboard in `open` and close it once.
        if let Err(err) = unsafe { CloseClipboard() } {
            log::warn!("CloseClipboard failed: {err}");
        }
    }
}

/// Formats whose data is an `HGLOBAL`. Excluded: bitmap (2), metafile
/// picture (3), palette (9), enhanced metafile (14), owner display and
/// display formats (0x80..), private (0x200..0x2FF) and GDI object
/// (0x300..0x3FF) handles.
fn is_memory_format(format: u32) -> bool {
    !matches!(
        format,
        2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x200..=0x3FF
    )
}

fn empty() -> windows::core::Result<()> {
    // SAFETY: the clipboard is open (callers hold a `Clipboard`).
    unsafe { EmptyClipboard() }
}

fn mark_private() {
    for name in PRIVACY_FORMATS {
        // SAFETY: `name` is a static NUL-terminated string.
        let format = unsafe { RegisterClipboardFormatW(name) };
        if format == 0 || set_bytes(format, &0u32.to_le_bytes()).is_err() {
            log::warn!("could not mark clipboard data as private");
        }
    }
}

/// Copies one format's data out of the open clipboard, if it is at most
/// `max_bytes` long.
fn read_global(format: u32, max_bytes: usize) -> Option<Vec<u8>> {
    // SAFETY: the clipboard is open; the handle stays owned by the system.
    let handle = unsafe { GetClipboardData(format) }.ok()?;
    let global = HGLOBAL(handle.0);
    // SAFETY: `global` is a valid memory handle from the clipboard.
    let size = unsafe { GlobalSize(global) };
    if size > max_bytes {
        return None;
    }
    // SAFETY: as above; a null result means it could not be locked.
    let data = unsafe { GlobalLock(global) };
    if data.is_null() {
        return None;
    }
    // SAFETY: a locked block is readable for `size` bytes until unlocked.
    let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) }.to_vec();
    unlock(global);
    Some(bytes)
}

/// Sets one format from `bytes` on the open, already emptied clipboard.
fn set_bytes(format: u32, bytes: &[u8]) -> windows::core::Result<()> {
    let global = OwnedGlobal::alloc(bytes.len().max(1))?;
    // SAFETY: `global` is a valid moveable block we own.
    let data = unsafe { GlobalLock(global.0) };
    if data.is_null() {
        return Err(windows::core::Error::from_thread());
    }
    // SAFETY: the locked block holds at least `bytes.len()` bytes and does
    // not overlap `bytes`.
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>(), bytes.len()) };
    unlock(global.0);
    // SAFETY: on success the system takes ownership of the block.
    unsafe { SetClipboardData(format, Some(HANDLE(global.0.0))) }?;
    global.release();
    Ok(())
}

fn unlock(global: HGLOBAL) {
    // SAFETY: `global` was locked by the caller.
    let unlocked = unsafe { GlobalUnlock(global) };
    // Reaching lock count zero reports "failure" with error code 0.
    if let Err(err) = unlocked
        && err.code().is_err()
    {
        log::warn!("GlobalUnlock failed: {err}");
    }
}

fn wipe_units(mut units: Vec<u16>) {
    units.fill(0);
    black_box(&units);
}

/// A memory block we own until the clipboard takes it.
struct OwnedGlobal(HGLOBAL);

impl OwnedGlobal {
    fn alloc(size: usize) -> windows::core::Result<Self> {
        // SAFETY: plain allocation; the handle is freed in `Drop`.
        unsafe { GlobalAlloc(GMEM_MOVEABLE, size) }.map(Self)
    }

    /// Hands the block over (the clipboard owns it now).
    fn release(self) {
        std::mem::forget(self);
    }
}

impl Drop for OwnedGlobal {
    fn drop(&mut self) {
        // SAFETY: we own the block and free it once.
        let freed = unsafe { GlobalFree(Some(self.0)) };
        // Inverted by the wrapper: success returns NULL (reported as `Err`),
        // failure returns the handle (reported as `Ok`).
        if freed.is_ok() {
            log::warn!("GlobalFree failed: {}", windows::core::Error::from_thread());
        }
    }
}
