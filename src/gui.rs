//! The settings process (`--settings`): the WinUI 3 window. It exits when the window closes,
//! so WinUI's ~40 MB is only paid while settings are open.

use std::cell::RefCell;
use std::ptr::{null, null_mut};

use windows_core::{Interface, Result};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND};
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::Threading::{CreateMutexW, Sleep};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::bindings::Microsoft::UI::Xaml::Controls::XamlControlsResources;
use crate::bindings::Microsoft::UI::Xaml::{Application, ApplicationInitializationCallback, ResourceDictionary};
use crate::shortcut::Shortcut;
use crate::{runtime, tray, ui, wide, xaml_app};

const TITLE: &str = "Always on Top";
const WINUI_CLASS: &str = "WinUIDesktopWin32WindowClass";

pub struct Ctx {
    pub ui: ui::Ui,
}

impl Ctx {
    pub fn refresh(&mut self) {
        let _ = self.ui.refresh();
    }

    /// Asks the tray process to switch hotkeys; false if another app owns `s`.
    pub fn set_shortcut(&mut self, s: Shortcut) -> bool {
        let ok = tray::send(tray::WM_SET_HOTKEY, s.mods as usize, s.vk as isize) == 1;
        if ok {
            let _ = self.ui.set_shortcut(s, true);
        }
        ok
    }
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

/// Runs `f` on the window state; skipped if it is already in use (re-entrant callback).
pub fn with_ctx<R>(f: impl FnOnce(&mut Ctx) -> R) -> Option<R> {
    CTX.with(|c| c.try_borrow_mut().ok()?.as_mut().map(f))
}

fn running() -> HWND {
    unsafe { FindWindowW(wide(WINUI_CLASS).as_ptr(), wide(TITLE).as_ptr()) }
}

/// Closes an open settings window (used when the tray exits).
pub fn close_running() {
    let hwnd = running();
    if !hwnd.is_null() {
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    }
}

unsafe extern "system" fn tick(_: HWND, _: u32, _: usize, _: u32) {
    with_ctx(|c| c.refresh());
}

fn launched() -> Result<()> {
    // Fluent styles for every control.
    let fluent: ResourceDictionary = XamlControlsResources::new()?.cast()?;
    Application::Current()?.Resources()?.MergedDictionaries()?.Append(&fluent)?;

    let mut ctx = Ctx { ui: ui::Ui::build()? };
    ctx.ui.set_shortcut(Shortcut::load(), tray::send(tray::WM_HOTKEY_OK, 0, 0) == 1)?;
    ctx.refresh();
    ctx.ui.show()?;
    CTX.with(|c| *c.borrow_mut() = Some(ctx));
    // The pinned list lives in the tray process; poll it while the window is open.
    unsafe { SetTimer(null_mut(), 0, 1000, Some(tick)) };
    Ok(())
}

fn error_box(text: &str) {
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide(TITLE).as_ptr(), MB_ICONERROR) };
}

pub fn run() {
    unsafe {
        // One settings window: focus the open one instead.
        CreateMutexW(null(), 0, wide("Local\\AlwaysOnTopRsSettings").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let hwnd = running();
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
            return;
        }
        // Settings need the tray process for the hotkey and the pinned list.
        if tray::find().is_null() {
            tray::spawn("--background");
            for _ in 0..30 {
                if !tray::find().is_null() {
                    break;
                }
                Sleep(100);
            }
        }
        if let Err(e) = runtime::attach() {
            error_box(&format!(
                "The settings window needs the Windows App Runtime: {e}.\n\nInstall it from https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads"
            ));
            return;
        }
        CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
    }

    let started = Application::Start(&ApplicationInitializationCallback::new(|_| {
        std::mem::forget(xaml_app::create(launched)?);
        Ok(())
    }));
    if let Err(e) = started {
        error_box(&format!("WinUI failed to start: {e}"));
    }
}
