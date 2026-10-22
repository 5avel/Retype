//! Low-level keyboard/mouse hooks and focus tracking on a dedicated thread.
//!
//! Callbacks only translate and enqueue (ADR-004): they must return within
//! `LowLevelHooksTimeout` or Windows silently removes the hook.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND, GetMessageW,
    HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_XBUTTONDOWN,
};

use retype_core::event::Millis;
use retype_core::hotkey::HotkeySet;
use retype_core::inject::{KeyAction, KeyCode, SIDED_MODIFIERS};
use retype_core::keystroke::vk;
use retype_core::keystroke::{Keystroke, Modifiers};

use super::worker::{HookInput, WorkerMsg, now_millis};

/// Shared with the callbacks, which receive no user data pointer.
struct Shared {
    tx: SyncSender<WorkerMsg>,
}

static SHARED: OnceLock<Shared> = OnceLock::new();

/// The hotkeys, replaced on config reload. The callback only `try_lock`s,
/// so it never blocks; during a replace it simply swallows nothing.
static HOTKEYS: Mutex<Option<HotkeySet>> = Mutex::new(None);

/// Mirrors the session's pause state so the hook knows which keys to swallow.
pub static PAUSED: AtomicBool = AtomicBool::new(false);

/// True while the foreground app is in `exclusions.processes`: its keys,
/// hotkeys included, go to the app untouched (except pause/resume).
pub static EXCLUDED: AtomicBool = AtomicBool::new(false);

/// Set by the worker while autoswitch may act (on, not paused, not
/// excluded): then every Space starts holding the following keys.
pub static HOLD_ARMED: AtomicBool = AtomicBool::new(false);

/// Keys typed right after a Space are held back while the worker decides
/// about the word, so a correction (Backspace, switch, retype) cannot
/// interleave with them. The worker replays them in order and stops
/// holding ([`release_held`]).
static HOLDING: AtomicBool = AtomicBool::new(false);
static HOLD_SINCE: AtomicU64 = AtomicU64::new(0);
static HELD: Mutex<Vec<KeyAction>> = Mutex::new(Vec::new());
/// Beyond this many events the hook stops holding and lets keys through.
const MAX_HELD: usize = 64;

/// Stops holding and returns the held key events, oldest first.
pub fn release_held() -> Vec<KeyAction> {
    HOLDING.store(false, Ordering::Relaxed);
    match HELD.lock() {
        Ok(mut held) => std::mem::take(&mut *held),
        Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
    }
}

/// How long keys have been held, if they are.
pub fn hold_age() -> Option<Millis> {
    HOLDING
        .load(Ordering::Relaxed)
        .then(|| now_millis().saturating_sub(HOLD_SINCE.load(Ordering::Relaxed)))
}

/// Holds one key event if holding; false lets it through.
fn hold(vk: u16, scan: u16, extended: bool, down: bool) -> bool {
    if !HOLDING.load(Ordering::Relaxed) {
        return false;
    }
    let Ok(mut held) = HELD.try_lock() else {
        return false; // the worker is releasing right now
    };
    if held.len() >= MAX_HELD {
        HOLDING.store(false, Ordering::Relaxed);
        return false;
    }
    let code = if scan == 0 {
        KeyCode::Vk(vk)
    } else {
        KeyCode::Scan { scan, extended }
    };
    held.push(KeyAction { code, down });
    true
}

/// Replaces the hotkeys the callback matches (config reload).
pub fn set_hotkeys(hotkeys: HotkeySet) {
    match HOTKEYS.lock() {
        Ok(mut guard) => *guard = Some(hotkeys),
        Err(poisoned) => *poisoned.into_inner() = Some(hotkeys),
    }
}

/// Set when the channel was full and input was dropped; the worker then
/// clears the buffer because it no longer matches the screen.
pub static OVERFLOW: AtomicBool = AtomicBool::new(false);

/// Physical state of [`SIDED_MODIFIERS`] (bit i = entry i), from
/// non-injected events only. `GetAsyncKeyState` cannot be used for this: our
/// own injected releases and presses change it too.
static PHYSICAL_MODIFIERS: AtomicU8 = AtomicU8::new(0);

/// The sided modifiers the user is physically holding.
pub fn physical_modifiers() -> Vec<u16> {
    held_modifiers().collect()
}

/// Clears modifiers the tracking thinks are down but the system says are
/// up: the hook misses key-ups on the secure desktop (Ctrl+Alt+Del, UAC,
/// Win+L) or after a timeout. Call only while retype is not injecting:
/// during injection the system state shows our own releases.
pub fn reconcile_modifiers() {
    for (i, vk) in SIDED_MODIFIERS.into_iter().enumerate() {
        let bit = 1u8 << i;
        if PHYSICAL_MODIFIERS.load(Ordering::Relaxed) & bit == 0 {
            continue;
        }
        // SAFETY: no preconditions; reads async key state.
        let state = unsafe { GetAsyncKeyState(i32::from(vk)) };
        if state >= 0 {
            // High bit clear: the key is up.
            PHYSICAL_MODIFIERS.fetch_and(!bit, Ordering::Relaxed);
        }
    }
}

/// Allocation-free view of the tracked state, for the hook callback.
fn held_modifiers() -> impl Iterator<Item = u16> {
    let bits = PHYSICAL_MODIFIERS.load(Ordering::Relaxed);
    SIDED_MODIFIERS
        .into_iter()
        .enumerate()
        .filter(move |(i, _)| bits & (1 << i) != 0)
        .map(|(_, vk)| vk)
}

/// The running hook thread.
pub struct HookThread {
    thread_id: u32,
    handle: JoinHandle<()>,
}

impl HookThread {
    /// Ends the message loop, removes the hooks and waits for the thread.
    pub fn stop(self) {
        // SAFETY: posting WM_QUIT to a thread id we own; no pointers passed.
        let posted = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Err(err) = posted {
            log::error!("could not stop hook thread: {err}");
            return;
        }
        if self.handle.join().is_err() {
            log::error!("hook thread panicked");
        }
    }
}

/// Starts the hook thread. Fails if the hooks cannot be installed.
pub fn start(tx: SyncSender<WorkerMsg>, hotkeys: HotkeySet) -> Result<HookThread, String> {
    set_hotkeys(hotkeys);
    if SHARED.set(Shared { tx }).is_err() {
        return Err("hook already started".to_owned());
    }
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("hook".to_owned())
        .spawn(move || run(&ready_tx))
        .map_err(|e| format!("cannot spawn hook thread: {e}"))?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(HookThread { thread_id, handle }),
        Ok(Err(message)) => {
            let _ = handle.join();
            Err(message)
        }
        Err(_) => Err("hook thread exited during startup".to_owned()),
    }
}

fn run(ready: &mpsc::Sender<Result<u32, String>>) {
    let hooks = match Hooks::install() {
        Ok(hooks) => hooks,
        Err(message) => {
            let _ = ready.send(Err(message));
            return;
        }
    };
    // SAFETY: no preconditions.
    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = ready.send(Ok(thread_id));
    log::info!("hooks installed");

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid out pointer for the duration of the call.
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if result.0 <= 0 {
            break; // WM_QUIT (0) or error (-1)
        }
        // SAFETY: `msg` was filled by GetMessageW. The BOOL only says whether
        // a character message was generated; it is not an error.
        let _ = unsafe { TranslateMessage(&msg) };
        // SAFETY: `msg` was filled by GetMessageW.
        unsafe { DispatchMessageW(&msg) };
    }
    drop(hooks);
    log::info!("hooks removed");
}

/// Installed hooks; removed on drop, on the thread that installed them.
struct Hooks {
    keyboard: HHOOK,
    mouse: HHOOK,
    focus: [HWINEVENTHOOK; 2],
}

impl Hooks {
    fn install() -> Result<Self, String> {
        // SAFETY: NULL name returns this exe's module handle; no ownership.
        let module = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
        // SAFETY: the callback is a valid `extern "system"` fn for the
        // whole process lifetime; LL hooks run in our own process (P7).
        let keyboard = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0)
        }
        .map_err(|e| format!("keyboard hook: {e}"))?;
        // SAFETY: as above.
        let mouse =
            unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0) };
        let mouse = match mouse {
            Ok(mouse) => mouse,
            Err(err) => {
                // SAFETY: `keyboard` was just installed and is removed once.
                if let Err(unhook_err) = unsafe { UnhookWindowsHookEx(keyboard) } {
                    log::warn!("unhook keyboard: {unhook_err}");
                }
                return Err(format!("mouse hook: {err}"));
            }
        };
        let focus = [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_FOCUS].map(|event| {
            // SAFETY: out-of-context hook (no DLL injection, P7); events are
            // delivered to this thread's message loop.
            unsafe {
                SetWinEventHook(
                    event,
                    event,
                    None::<HMODULE>,
                    Some(win_event_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            }
        });
        if focus.iter().any(HWINEVENTHOOK::is_invalid) {
            log::warn!("focus tracking unavailable; buffer resets on clicks and keys only");
        }
        Ok(Self {
            keyboard,
            mouse,
            focus,
        })
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        // SAFETY: each hook was installed by `install` and is removed once.
        if let Err(err) = unsafe { UnhookWindowsHookEx(self.keyboard) } {
            log::warn!("unhook keyboard: {err}");
        }
        // SAFETY: as above.
        if let Err(err) = unsafe { UnhookWindowsHookEx(self.mouse) } {
            log::warn!("unhook mouse: {err}");
        }
        for hook in self.focus {
            if !hook.is_invalid() {
                // SAFETY: as above.
                if !unsafe { UnhookWinEvent(hook) }.as_bool() {
                    log::warn!("unhook focus events failed");
                }
            }
        }
    }
}

/// Enqueues input for the worker; false if it could not be delivered.
fn send(input: HookInput) -> bool {
    let Some(shared) = SHARED.get() else {
        return false;
    };
    match shared.tx.try_send(WorkerMsg::Input(input)) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            OVERFLOW.store(true, Ordering::Relaxed);
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

/// Physical modifier state: our injected releases and restores must not
/// change how the user's keys are read (e.g. a hotkey's auto-repeat).
fn modifiers() -> Modifiers {
    Modifiers::from_sided(held_modifiers())
}

/// Enqueues a key press; returns true if it is our hotkey and must be swallowed.
fn on_key(message: u32, info: &KBDLLHOOKSTRUCT) -> bool {
    if (info.flags.0 & LLKHF_INJECTED.0) != 0 {
        return false; // our own SendInput replay (and other injectors)
    }
    let is_up = message == WM_KEYUP || message == WM_SYSKEYUP;
    track_modifier(info.vkCode, is_up);
    let vk_code = u16::try_from(info.vkCode).unwrap_or(0);
    let scan = u16::try_from(info.scanCode).unwrap_or(0);
    let extended = (info.flags.0 & LLKHF_EXTENDED.0) != 0;
    if is_up {
        // Lets the session tell a hotkey's auto-repeat from a new press.
        send(HookInput::KeyUp {
            vk: vk_code,
            time: now_millis(),
        });
        return hold(vk_code, scan, extended, false);
    }
    if message != WM_KEYDOWN && message != WM_SYSKEYDOWN {
        return false;
    }
    let mods = modifiers();
    let key = Keystroke {
        vk: vk_code,
        scan,
        extended,
        shift: mods.shift,
    };
    let sent = send(HookInput::Key {
        key,
        mods,
        time: now_millis(),
    });
    if swallow_hotkey(key.vk, mods) {
        return true;
    }
    if hold(vk_code, scan, extended, true) {
        return true;
    }
    // The Space itself goes through; what follows waits for the worker's
    // autoswitch decision. Only if the worker will see this Space.
    if key.vk == vk::SPACE && !mods.is_command() && sent && HOLD_ARMED.load(Ordering::Relaxed) {
        HOLD_SINCE.store(now_millis(), Ordering::Relaxed);
        HOLDING.store(true, Ordering::Relaxed);
    }
    false
}

/// True if this key press is our hotkey and must not reach the app.
fn swallow_hotkey(vk_code: u16, mods: Modifiers) -> bool {
    let guard = match HOTKEYS.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => return false,
    };
    let maybe_action = guard.and_then(|hotkeys| hotkeys.action_for(vk_code, mods));
    drop(guard);
    maybe_action.is_some_and(|action| {
        action.swallowed(
            PAUSED.load(Ordering::Relaxed),
            EXCLUDED.load(Ordering::Relaxed),
        )
    })
}

fn track_modifier(vk_code: u32, is_up: bool) {
    let Some(i) = SIDED_MODIFIERS
        .iter()
        .position(|&m| u32::from(m) == vk_code)
    else {
        return;
    };
    let bit = 1u8 << i;
    if is_up {
        PHYSICAL_MODIFIERS.fetch_and(!bit, Ordering::Relaxed);
    } else {
        PHYSICAL_MODIFIERS.fetch_or(bit, Ordering::Relaxed);
    }
}

extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT that is
        // valid for the duration of this call.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = u32::try_from(wparam.0).unwrap_or(0);
        if on_key(message, info) {
            return LRESULT(1);
        }
    }
    // SAFETY: forwards the unchanged arguments to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a MSLLHOOKSTRUCT that is
        // valid for the duration of this call.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let message = u32::try_from(wparam.0).unwrap_or(0);
        let is_click = matches!(
            message,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        );
        if is_click && (info.flags & LLMHF_INJECTED) == 0 {
            send(HookInput::MouseClick);
        }
    }
    // SAFETY: forwards the unchanged arguments to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    send(HookInput::FocusChanged);
}
