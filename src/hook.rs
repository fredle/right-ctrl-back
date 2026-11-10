//! Windows glue: the low-level keyboard hook, key injection, the tray icon and
//! single-instance handling. All decisions live in `engine.rs`; this file
//! only translates between Windows and that state machine.
//!
//! One background thread owns the hook, a hidden window and the tray icon,
//! and runs the message loop the hook needs. The egui window talks to it
//! through [`Shared`].

use std::collections::VecDeque;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::engine::{Action, Engine};
use crate::keys::{self, Target};

/// Tags our own injected events so the hook can ignore them.
const MARKER: usize = 0x5243_4231;
const WM_TRAY: u32 = WM_APP + 1;
const WM_REFRESH: u32 = WM_APP + 2;
const WM_SHOW: u32 = WM_APP + 3;
const ID_TOGGLE: usize = 1;
const ID_SHOW: usize = 2;
const ID_QUIT: usize = 3;
const WINDOW_CLASS: &str = "RightCtrlBackTray";
const WINDOW_TITLE: &str = "Right Ctrl Back";
const MUTEX_NAME: &str = "Local\\RightCtrlBack.SingleInstance";
const LOG_LINES: usize = 14;

/// State shared between the hook thread and the UI.
pub struct Shared {
    pub paused: AtomicBool,
    /// Record every key the hook sees (only while the tester is open).
    pub tester_on: AtomicBool,
    /// The next key pressed is reported through `captured` and swallowed.
    pub capturing: AtomicBool,
    pub captured: Mutex<Option<u16>>,
    pub target: Mutex<Target>,
    pub log: Mutex<VecDeque<LogLine>>,
    /// Set once the hook is installed; false means it failed.
    pub hook_state: Mutex<HookState>,
    ctx: Mutex<Option<eframe::egui::Context>>,
    tray_hwnd: AtomicIsize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HookState {
    Starting,
    Running,
    Failed,
}

#[derive(Clone, Debug)]
pub struct LogLine {
    /// True for what this app emitted, false for what the keyboard sent.
    pub emitted: bool,
    pub text: String,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock must never take the hook down.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn new(target: Target) -> Self {
        Self {
            paused: AtomicBool::new(false),
            tester_on: AtomicBool::new(false),
            capturing: AtomicBool::new(false),
            captured: Mutex::new(None),
            target: Mutex::new(target),
            log: Mutex::new(VecDeque::new()),
            hook_state: Mutex::new(HookState::Starting),
            ctx: Mutex::new(None),
            tray_hwnd: AtomicIsize::new(0),
        }
    }

    pub fn set_target(&self, t: Target) {
        *lock(&self.target) = t;
    }

    pub fn state(&self) -> HookState {
        *lock(&self.hook_state)
    }

    pub fn take_captured(&self) -> Option<u16> {
        lock(&self.captured).take()
    }

    pub fn clear_log(&self) {
        lock(&self.log).clear();
    }

    pub fn log_snapshot(&self) -> Vec<LogLine> {
        lock(&self.log).iter().cloned().collect()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        let hwnd = self.tray_hwnd.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe { PostMessageW(hwnd as HWND, WM_REFRESH, 0, 0) };
        }
    }

    fn wake_ui(&self) {
        if let Some(ctx) = lock(&self.ctx).as_ref() {
            ctx.request_repaint();
        }
    }

    fn push_log(&self, emitted: bool, text: String) {
        {
            let mut log = lock(&self.log);
            if log.len() >= LOG_LINES {
                log.pop_front();
            }
            log.push_back(LogLine { emitted, text });
        }
        self.wake_ui();
    }
}

struct HookCtx {
    shared: Arc<Shared>,
    engine: Mutex<Engine>,
    /// Key whose release must be swallowed after it was captured.
    capture_swallow: Mutex<Option<u16>>,
    last_down: Mutex<Option<u16>>,
}

static CTX: OnceLock<HookCtx> = OnceLock::new();
static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

/// Returns true if another copy is already running (and asks it to show its
/// window). Call before starting anything else.
pub fn already_running() -> bool {
    unsafe {
        let name = wide(MUTEX_NAME);
        let handle = CreateMutexW(null(), 0, name.as_ptr());
        // Deliberately never closed: the mutex lives as long as this process.
        let _ = handle;
        const ERROR_ALREADY_EXISTS: u32 = 183;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let class = wide(WINDOW_CLASS);
            let hwnd = FindWindowW(class.as_ptr(), null());
            if !hwnd.is_null() {
                PostMessageW(hwnd, WM_SHOW, 0, 0);
            }
            return true;
        }
    }
    false
}

/// Start the hook thread. Returns the shared state for the UI.
pub fn start(ctx: eframe::egui::Context, target: Target) -> Arc<Shared> {
    let shared = Arc::new(Shared::new(target));
    *lock(&shared.ctx) = Some(ctx);
    let _ = SHARED.set(shared.clone());
    let _ = CTX.set(HookCtx {
        shared: shared.clone(),
        engine: Mutex::new(Engine::new()),
        capture_swallow: Mutex::new(None),
        last_down: Mutex::new(None),
    });
    let thread_shared = shared.clone();
    std::thread::Builder::new()
        .name("keyboard-hook".into())
        .spawn(move || hook_thread(thread_shared))
        .ok();
    shared
}

/// Release anything held and exit. Used by the Quit button and tray item.
pub fn quit() -> ! {
    if let Some(c) = CTX.get() {
        let actions = lock(&c.engine).release();
        inject(&actions);
    }
    unsafe { tray_icon(SHARED.get().map(|s| s.tray_hwnd.load(Ordering::SeqCst)).unwrap_or(0), NIM_DELETE, false) };
    std::process::exit(0);
}

fn hook_thread(shared: Arc<Shared>) {
    unsafe {
        let hmod = GetModuleHandleW(null());
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod, 0);
        if hook.is_null() {
            *lock(&shared.hook_state) = HookState::Failed;
            shared.wake_ui();
            return;
        }

        let class = wide(WINDOW_CLASS);
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hmod,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let title = wide(WINDOW_TITLE);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hmod,
            null(),
        );
        if !hwnd.is_null() {
            shared.tray_hwnd.store(hwnd as isize, Ordering::SeqCst);
            tray_icon(hwnd as isize, NIM_ADD, shared.paused.load(Ordering::SeqCst));
        }
        *lock(&shared.hook_state) = HookState::Running;
        shared.wake_ui();

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if kb.dwExtraInfo != MARKER && handle_key(kb.vkCode as u16, wparam as u32) {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

/// Returns true to swallow the event.
fn handle_key(vk: u16, message: u32) -> bool {
    let Some(c) = CTX.get() else { return false };
    let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let shared = &c.shared;

    // Picking a key for the "any single key" target.
    if shared.capturing.load(Ordering::SeqCst) {
        let candidate = !(keys::is_win(vk) || keys::is_shift(vk) || vk == keys::VK_F23);
        if candidate && down {
            *lock(&shared.captured) = Some(vk);
            shared.capturing.store(false, Ordering::SeqCst);
            *lock(&c.capture_swallow) = Some(vk);
            shared.wake_ui();
            return true;
        }
    }
    if !down {
        let mut cs = lock(&c.capture_swallow);
        if *cs == Some(vk) {
            *cs = None;
            return true;
        }
    }

    let tester = shared.tester_on.load(Ordering::SeqCst);
    if tester {
        let mut last = lock(&c.last_down);
        let repeat = down && *last == Some(vk);
        *last = if down { Some(vk) } else { None };
        if !repeat {
            shared.push_log(
                false,
                format!("{} {}", keys::vk_name(vk), if down { "down" } else { "up" }),
            );
        }
    }

    let target = lock(&shared.target).clone();
    let enabled = !shared.paused.load(Ordering::SeqCst);
    let outcome = lock(&c.engine).on_key(vk, down, enabled, &target);

    if tester && !outcome.actions.is_empty() {
        for a in &outcome.actions {
            let text = match a {
                Action::KeyDown(v) => format!("{} down", keys::vk_name(*v)),
                Action::KeyUp(v) => format!("{} up", keys::vk_name(*v)),
                Action::Launch(p) => format!("Launch {p}"),
            };
            // The mask tap is plumbing, not something to show.
            if matches!(a, Action::KeyDown(crate::engine::MASK_VK) | Action::KeyUp(crate::engine::MASK_VK)) {
                continue;
            }
            shared.push_log(true, text);
        }
    }
    inject(&outcome.actions);
    outcome.suppress
}

/// Send the key actions and start any launches off the hook's thread.
fn inject(actions: &[Action]) {
    let mut inputs: Vec<INPUT> = Vec::new();
    for a in actions {
        match a {
            Action::KeyDown(vk) => inputs.push(key_input(*vk, true)),
            Action::KeyUp(vk) => inputs.push(key_input(*vk, false)),
            Action::Launch(path) => launch(path.clone()),
        }
    }
    if !inputs.is_empty() {
        unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
        }
    }
}

fn key_input(vk: u16, down: bool) -> INPUT {
    let mut flags = 0;
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if keys::is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MARKER,
            },
        },
    }
}

fn launch(path: String) {
    std::thread::spawn(move || unsafe {
        CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32);
        let verb = wide("open");
        let file = wide(&path);
        ShellExecuteW(null_mut(), verb.as_ptr(), file.as_ptr(), null(), null(), SW_SHOWNORMAL);
    });
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn tray_icon(hwnd: isize, message: u32, paused: bool) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd as HWND;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        // The icon embedded by build.rs has resource id 1.
        nid.hIcon = LoadIconW(GetModuleHandleW(null()), 1 as *const u16);
        let tip = if paused { "Right Ctrl Back (paused)" } else { "Right Ctrl Back" };
        for (i, c) in tip.encode_utf16().take(100).enumerate() {
            nid.szTip[i] = c;
        }
        Shell_NotifyIconW(message, &nid);
    }
}

fn show_main_window() {
    unsafe {
        // The egui window carries the app's title; the tray window's class
        // name differs, so searching by title alone finds the right one.
        let title = wide(WINDOW_TITLE);
        let mut found: HWND = null_mut();
        let own = SHARED.get().map(|s| s.tray_hwnd.load(Ordering::SeqCst)).unwrap_or(0);
        let mut prev: HWND = null_mut();
        loop {
            let h = FindWindowExW(null_mut(), prev, null(), title.as_ptr());
            if h.is_null() {
                break;
            }
            if h as isize != own {
                found = h;
                break;
            }
            prev = h;
        }
        if !found.is_null() {
            ShowWindow(found, SW_SHOW);
            ShowWindow(found, SW_RESTORE);
            SetForegroundWindow(found);
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();
    let taskbar = *TASKBAR_CREATED.get_or_init(|| unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) });
    let paused = || SHARED.get().map(|s| s.paused.load(Ordering::SeqCst)).unwrap_or(false);

    match msg {
        WM_TRAY => {
            match (lparam as u32) & 0xFFFF {
                WM_LBUTTONUP => show_main_window(),
                WM_RBUTTONUP => unsafe { tray_menu(hwnd, paused()) },
                _ => {}
            }
            0
        }
        WM_REFRESH => {
            unsafe { tray_icon(hwnd as isize, NIM_MODIFY, paused()) };
            if let Some(s) = SHARED.get() {
                s.wake_ui();
            }
            0
        }
        WM_SHOW => {
            show_main_window();
            0
        }
        m if m == taskbar => {
            // Explorer restarted; put the icon back.
            unsafe { tray_icon(hwnd as isize, NIM_ADD, paused()) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn tray_menu(hwnd: HWND, paused: bool) {
    unsafe {
        let menu = CreatePopupMenu();
        let toggle = wide(if paused { "Resume" } else { "Pause" });
        let show = wide("Open Right Ctrl Back");
        let quit_label = wide("Quit");
        AppendMenuW(menu, MF_STRING, ID_TOGGLE, toggle.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_SHOW, show.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_QUIT, quit_label.as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Required so the menu closes when the user clicks elsewhere.
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        match cmd as usize {
            ID_TOGGLE => {
                if let Some(s) = SHARED.get() {
                    s.set_paused(!paused);
                }
            }
            ID_SHOW => show_main_window(),
            ID_QUIT => quit(),
            _ => {}
        }
    }
}

/// Make the egui window hide to the tray instead of quitting; called by the UI
/// so the logic for "is the hook alive" stays in one place.
pub fn tray_available(shared: &Shared) -> bool {
    shared.tray_hwnd.load(Ordering::SeqCst) != 0
}
