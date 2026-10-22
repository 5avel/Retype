//! Tray icon, its menu and the main thread's message loop (SPEC 5.4).

use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::SyncSender;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFY_ICON_MESSAGE,
    NOTIFYICONDATAW, Shell_NotifyIconW, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, IDI_APPLICATION, LoadIconW, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SW_SHOWNORMAL, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_DESTROY, WM_ENDSESSION,
    WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{HSTRING, PCWSTR, w};

use super::autostart;
use super::hook::PAUSED;
use super::worker::WorkerMsg;
use crate::paths::Paths;

/// The shell sends icon clicks to the window with this message.
const WM_APP_TRAY: u32 = WM_APP + 1;
/// The worker posts this when pause state changes; wparam = paused.
const WM_APP_PAUSED: u32 = WM_APP + 2;
const ICON_ID: u32 = 1;

const ID_PAUSE: usize = 1;
const ID_OPEN_CONFIG: usize = 2;
const ID_OPEN_LOGS: usize = 3;
const ID_EXIT: usize = 4;
const ID_RELOAD: usize = 5;
const ID_AUTOSTART: usize = 6;

/// Written when the user opens a config file that does not exist yet.
const EXAMPLE_CONFIG: &str = include_str!("../../../../config.example.toml");

struct Context {
    worker: SyncSender<WorkerMsg>,
    paths: Paths,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();
/// "TaskbarCreated": Explorer restarted, the icon must be added again.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Lets other threads notify the tray window. Holds the window handle as an
/// integer because `HWND` is not `Send`.
#[derive(Debug, Clone, Copy)]
pub struct TrayHandle(isize);

impl TrayHandle {
    pub fn notify_paused(self, paused: bool) {
        let hwnd = HWND(self.0 as *mut _);
        // SAFETY: PostMessageW is thread-safe; a stale handle just fails.
        let posted = unsafe {
            PostMessageW(
                Some(hwnd),
                WM_APP_PAUSED,
                WPARAM(usize::from(paused)),
                LPARAM(0),
            )
        };
        if let Err(err) = posted {
            log::warn!("tray update failed: {err}");
        }
    }
}

/// The hidden window that owns the tray icon. Lives on the main thread.
pub struct Tray {
    hwnd: HWND,
}

impl Tray {
    pub fn create(worker: SyncSender<WorkerMsg>, paths: Paths) -> Result<Self, String> {
        if CONTEXT.set(Context { worker, paths }).is_err() {
            return Err("tray already created".to_owned());
        }
        // SAFETY: static NUL-terminated string.
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        TASKBAR_CREATED.store(taskbar_created, Ordering::Relaxed);

        // SAFETY: NULL name returns this exe's module handle; no ownership.
        let module = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
        let class_name = w!("retype-tray");
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
        // SAFETY: the class was registered above; the window is never shown.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                w!("retype"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(module.into()),
                None,
            )
        }
        .map_err(|e| format!("CreateWindowExW: {e}"))?;
        if !set_icon(hwnd, NIM_ADD) {
            log::warn!("tray icon could not be added");
        }
        Ok(Self { hwnd })
    }

    pub fn handle(&self) -> TrayHandle {
        TrayHandle(self.hwnd.0 as isize)
    }

    /// Runs the main thread's message loop until Exit is chosen.
    pub fn run(&self) {
        let mut msg = MSG::default();
        loop {
            // SAFETY: `msg` is a valid out pointer for the duration of the call.
            let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if result.0 <= 0 {
                break;
            }
            // SAFETY: `msg` was filled by GetMessageW. The BOOL only says
            // whether a character message was generated; it is not an error.
            let _ = unsafe { TranslateMessage(&msg) };
            // SAFETY: `msg` was filled by GetMessageW.
            unsafe { DispatchMessageW(&msg) };
        }
    }
}

impl Drop for Tray {
    /// Also covers early exits where the message loop never ran.
    fn drop(&mut self) {
        remove_icon(self.hwnd);
    }
}

fn set_icon(hwnd: HWND, action: NOTIFY_ICON_MESSAGE) -> bool {
    let paused = PAUSED.load(Ordering::Relaxed);
    let tip = if paused {
        "retype — paused"
    } else {
        "retype"
    };
    let mut data = NOTIFYICONDATAW {
        cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(0),
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_APP_TRAY,
        ..Default::default()
    };
    // SAFETY: loads a shared system icon; no ownership is taken.
    if let Ok(icon) = unsafe { LoadIconW(None, IDI_APPLICATION) } {
        data.hIcon = icon;
    }
    for (dst, src) in data.szTip.iter_mut().zip(tip.encode_utf16()) {
        *dst = src; // szTip stays NUL-terminated: the tip is far below 128
    }
    // SAFETY: `data` is fully initialised and outlives the call.
    unsafe { Shell_NotifyIconW(action, &data) }.as_bool()
}

fn remove_icon(hwnd: HWND) {
    let data = NOTIFYICONDATAW {
        cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(0),
        hWnd: hwnd,
        uID: ICON_ID,
        ..Default::default()
    };
    // SAFETY: `data` identifies our icon and outlives the call.
    // FALSE is expected when the icon is already gone (Drop after WM_DESTROY).
    if !unsafe { Shell_NotifyIconW(NIM_DELETE, &data) }.as_bool() {
        log::debug!("tray icon already removed");
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_APP_TRAY => {
            let mouse = u32::try_from(lparam.0 & 0xFFFF).unwrap_or(0);
            if mouse == WM_RBUTTONUP || mouse == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_APP_PAUSED => {
            set_icon(hwnd, NIM_MODIFY);
            LRESULT(0)
        }
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                remove_icon(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            remove_icon(hwnd);
            // SAFETY: ends this (main) thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            set_icon(hwnd, NIM_ADD);
            LRESULT(0)
        }
        // SAFETY: default handling with the unchanged arguments.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn show_menu(hwnd: HWND) {
    // SAFETY: creates a menu owned by this function and destroyed below.
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let pause_label = if PAUSED.load(Ordering::Relaxed) {
        w!("Resume")
    } else {
        w!("Pause")
    };
    let autostart_flags = if autostart::is_enabled() {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    };
    let items = [
        (MF_STRING, ID_PAUSE, pause_label),
        (MF_STRING, ID_OPEN_CONFIG, w!("Open config")),
        (MF_STRING, ID_RELOAD, w!("Reload config")),
        (MF_STRING, ID_OPEN_LOGS, w!("Open log folder")),
        (autostart_flags, ID_AUTOSTART, w!("Start with Windows")),
        (MF_SEPARATOR, 0, PCWSTR::null()),
        (MF_STRING, ID_EXIT, w!("Exit")),
    ];
    for (flags, id, label) in items {
        // SAFETY: `menu` is valid; labels are static or null for separators.
        if let Err(err) = unsafe { AppendMenuW(menu, flags, id, label) } {
            log::warn!("tray menu item: {err}");
        }
    }
    let mut point = POINT::default();
    // SAFETY: `point` is a valid out pointer.
    if let Err(err) = unsafe { GetCursorPos(&mut point) } {
        log::warn!("cursor position: {err}");
    }
    // SAFETY: required so the menu closes when the user clicks elsewhere.
    if !unsafe { SetForegroundWindow(hwnd) }.as_bool() {
        log::debug!("SetForegroundWindow refused; menu may stay open");
    }
    // SAFETY: `menu` and `hwnd` are valid; TPM_RETURNCMD returns the item id.
    let chosen = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        )
    };
    // SAFETY: the menu was created above and is destroyed once.
    if let Err(err) = unsafe { DestroyMenu(menu) } {
        log::warn!("destroy tray menu: {err}");
    }
    // SAFETY: documented workaround so the next menu opens reliably.
    if let Err(err) = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) } {
        log::debug!("post WM_NULL: {err}");
    }

    match usize::try_from(chosen.0).unwrap_or(0) {
        ID_PAUSE => {
            if let Some(context) = CONTEXT.get()
                && context.worker.try_send(WorkerMsg::TogglePause).is_err()
            {
                log::warn!("pause request dropped: worker busy");
            }
        }
        ID_OPEN_CONFIG => open_config(),
        ID_RELOAD => {
            if let Some(context) = CONTEXT.get()
                && context.worker.try_send(WorkerMsg::ReloadConfig).is_err()
            {
                log::warn!("reload request dropped: worker busy");
            }
        }
        ID_AUTOSTART => {
            let enable = !autostart::is_enabled();
            match autostart::set_enabled(enable) {
                Ok(()) => log::info!("autostart {}", if enable { "on" } else { "off" }),
                Err(err) => log::error!("autostart change failed: {err}"),
            }
        }
        ID_OPEN_LOGS => {
            if let Some(context) = CONTEXT.get() {
                let _ = fs::create_dir_all(&context.paths.log_dir);
                open(&context.paths.log_dir);
            }
        }
        ID_EXIT => {
            // SAFETY: destroys our own window; WM_DESTROY ends the loop.
            if let Err(err) = unsafe { DestroyWindow(hwnd) } {
                log::error!("exit failed: {err}");
            }
        }
        _ => {}
    }
}

/// Opens the config in the default editor, creating it from the example first.
fn open_config() {
    let Some(context) = CONTEXT.get() else {
        return;
    };
    let path = &context.paths.config_file;
    if !path.exists() {
        let written = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(path, EXAMPLE_CONFIG));
        if let Err(err) = written {
            log::error!("cannot create config file: {err}");
            return;
        }
        log::info!("created config from example");
    }
    if !open(path) {
        // `.toml` often has no associated editor.
        shell_execute(Path::new("notepad.exe"), Some(path));
    }
}

fn open(path: &Path) -> bool {
    shell_execute(path, None)
}

fn shell_execute(file: &Path, argument: Option<&Path>) -> bool {
    let file = HSTRING::from(file);
    let argument = argument.map(HSTRING::from);
    let argument = argument
        .as_ref()
        .map_or(PCWSTR::null(), |a| PCWSTR(a.as_ptr()));
    // SAFETY: all strings are NUL-terminated and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &file,
            argument,
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    let ok = result.0 as isize > 32;
    if !ok {
        log::warn!("ShellExecuteW failed ({})", result.0 as isize);
    }
    ok
}
