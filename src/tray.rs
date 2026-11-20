//! The background process: global hotkey, tray icon and pinned windows. Pure Win32, a few MB.
//!
//! The WinUI settings window runs as a separate `--settings` process and talks to this one
//! through window messages on the hidden host window (see the `WM_*` constants).

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{OpenProcess, Sleep, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::shortcut::Shortcut;
use crate::state::Pins;
use crate::wide;

const CLASS: &str = "AlwaysOnTopRsHost";
const WM_TRAY: u32 = WM_APP + 1;
/// Settings changed in the registry: re-read border and sound.
pub const WM_RELOAD: u32 = WM_APP + 2;
/// Switch hotkey to (mods = wParam, vk = lParam). Returns 1 on success.
pub const WM_SET_HOTKEY: u32 = WM_APP + 3;
/// Returns 1 if the hotkey is registered.
pub const WM_HOTKEY_OK: u32 = WM_APP + 4;
/// Returns the pinned window at index wParam, or 0 past the end.
pub const WM_PINNED_AT: u32 = WM_APP + 5;
/// Unpins window wParam, or every window if wParam is 0.
pub const WM_UNPIN: u32 = WM_APP + 6;
/// Turns sign-in start on (wParam 1) or off. Done here so an elevated tray can make the task.
pub const WM_AUTOSTART: u32 = WM_APP + 7;
/// Exit, for a replacing tray or an update; wParam 1 also closes the settings window.
pub const WM_EXIT: u32 = WM_APP + 8;
const ID_OPEN: usize = 1;
const ID_EXIT: usize = 2;
const HOTKEY_ID: i32 = 1;

struct Tray {
    hwnd: HWND,
    pins: Pins,
    shortcut: Shortcut,
    hotkey_ok: bool,
    icon_shown: bool,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Tray) -> R) -> Option<R> {
    TRAY.with(|t| t.try_borrow_mut().ok()?.as_mut().map(f))
}

/// The app icon (resource 1, see `assets/app.rc`) at `size` pixels.
pub fn app_icon(size: i32) -> HICON {
    unsafe { LoadImageW(GetModuleHandleW(null()), 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) as HICON }
}

/// The running tray process's host window, if any.
pub fn find() -> HWND {
    unsafe { FindWindowW(wide(CLASS).as_ptr(), null()) }
}

/// Sends an IPC message to the tray process; 0 if it is gone or hung.
pub fn send(msg: u32, wp: usize, lp: isize) -> usize {
    let mut result = 0;
    unsafe { SendMessageTimeoutW(find(), msg, wp, lp, SMTO_ABORTIFHUNG, 1000, &mut result) };
    result
}

/// The pinned windows, as raw HWND values.
pub fn pinned() -> Vec<usize> {
    (0..).map_while(|i| Some(send(WM_PINNED_AT, i, 0)).filter(|&h| h != 0)).collect()
}

/// Starts this exe again with space-separated `args` (`--settings`, `--background`, ...).
pub fn spawn(args: &str) {
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).args(args.split_whitespace()).spawn();
    }
}

/// Stops the running tray process, if any, and waits for it to exit (killing it after 5 s,
/// e.g. an older version that ignores `WM_EXIT`).
pub fn quit(close_settings: bool) {
    let hwnd = find();
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        // Fails for an elevated tray seen from a normal process; then wait for the window instead.
        let process = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid);
        send(WM_EXIT, close_settings as usize, 0);
        if process.is_null() {
            for _ in 0..50 {
                if find().is_null() {
                    break;
                }
                Sleep(100);
            }
            return;
        }
        if WaitForSingleObject(process, 5000) != WAIT_OBJECT_0 {
            TerminateProcess(process, 1);
            WaitForSingleObject(process, 1000);
        }
        CloseHandle(process);
    }
}

impl Tray {
    fn register(&mut self, s: Shortcut) -> bool {
        unsafe {
            UnregisterHotKey(self.hwnd, HOTKEY_ID);
            RegisterHotKey(self.hwnd, HOTKEY_ID, s.mods | MOD_NOREPEAT, s.vk) != 0
        }
    }

    fn set_hotkey(&mut self, s: Shortcut) -> bool {
        if !self.register(s) {
            self.hotkey_ok = self.register(self.shortcut);
            return false;
        }
        (self.shortcut, self.hotkey_ok) = (s, true);
        s.save();
        icon(self.hwnd, NIM_MODIFY, s);
        true
    }

    /// Adds or removes the tray icon to match the `TrayIcon` setting. Without it, settings
    /// open by starting the exe again.
    fn sync_icon(&mut self) {
        let want = crate::settings::get("TrayIcon", true);
        if want != self.icon_shown {
            icon(self.hwnd, if want { NIM_ADD } else { NIM_DELETE }, self.shortcut);
            self.icon_shown = want;
        }
    }
}

fn icon(hwnd: HWND, msg: NOTIFY_ICON_MESSAGE, s: Shortcut) {
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = app_icon(GetSystemMetrics(SM_CXSMICON));
        let admin = if crate::elevation::is_elevated() { ", administrator" } else { "" };
        let tip = wide(&format!("Always on Top ({}{admin})", s.label()));
        let n = tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        Shell_NotifyIconW(msg, &nid);
    }
}

fn menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, ID_OPEN, wide("Settings").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_EXIT, wide("Exit").as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
        DestroyMenu(menu);
    }
}

fn exit(hwnd: HWND, close_settings: bool) {
    with(|t| {
        t.pins.unpin_all();
        icon(hwnd, NIM_DELETE, t.shortcut);
    });
    if close_settings {
        crate::gui::close_running();
    }
    unsafe { PostQuitMessage(0) };
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();
    let taskbar_created = *TASKBAR_CREATED.get_or_init(|| unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) });
    let done = |_| 0;
    let r = match msg {
        WM_HOTKEY => with(|t| t.pins.toggle(unsafe { GetForegroundWindow() })).map(done),
        WM_TIMER => with(|t| t.pins.prune()).map(done),
        WM_SETTINGCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED => with(|t| t.pins.accent_changed()).map(done),
        WM_RELOAD => with(|t| {
            t.pins.reload();
            t.sync_icon();
        })
        .map(done),
        WM_SET_HOTKEY => with(|t| t.set_hotkey(Shortcut { mods: wp as u32, vk: lp as u32 }) as isize),
        WM_HOTKEY_OK => with(|t| t.hotkey_ok as isize),
        WM_PINNED_AT => with(|t| t.pins.pinned.get(wp).map_or(0, |&h| h as isize)),
        WM_UNPIN => with(|t| match wp {
            0 => t.pins.unpin_all(),
            h => t.pins.unpin(h as HWND),
        })
        .map(done),
        WM_AUTOSTART => {
            // schtasks takes a moment; keep the hotkey responsive.
            std::thread::spawn(move || crate::autostart::set(wp != 0));
            Some(1)
        }
        WM_EXIT => {
            exit(hwnd, wp != 0);
            Some(1)
        }
        WM_TRAY => {
            match (lp & 0xFFFF) as u32 {
                WM_LBUTTONUP => spawn("--settings"),
                WM_RBUTTONUP => menu(hwnd),
                _ => {}
            }
            Some(0)
        }
        WM_COMMAND => {
            match wp & 0xFFFF {
                ID_OPEN => spawn("--settings"),
                ID_EXIT => exit(hwnd, true),
                _ => {}
            }
            Some(0)
        }
        m if m == taskbar_created => with(|t| {
            if t.icon_shown {
                icon(hwnd, NIM_ADD, t.shortcut);
            }
        })
        .map(done),
        _ => None,
    };
    r.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wp, lp) })
}

/// Runs the tray process until Exit. Opens settings too unless `background`.
pub fn run(background: bool) {
    let class = wide(CLASS);
    unsafe {
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hIcon: app_icon(GetSystemMetrics(SM_CXICON)),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        // A hidden top-level window (not message-only) so it receives TaskbarCreated and theme broadcasts.
        let hwnd = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0, null_mut(), null_mut(), instance, null());
        // When elevated, UIPI drops messages from a normal settings process unless allowed.
        crate::elevation::allow(hwnd, WM_RELOAD..=WM_EXIT);
        // Point the sign-in entry at this exe and admin mode (after an update or a switch).
        std::thread::spawn(crate::autostart::sync);
        let shortcut = Shortcut::load();
        let mut tray = Tray { hwnd, pins: Pins::load(), shortcut, hotkey_ok: false, icon_shown: false };
        tray.hotkey_ok = tray.register(shortcut);
        tray.sync_icon();
        SetTimer(hwnd, 1, 1000, None);
        // A taken shortcut needs the settings window to fix.
        if !background || !tray.hotkey_ok {
            spawn("--settings");
        }
        TRAY.with(|t| *t.borrow_mut() = Some(tray));

        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
