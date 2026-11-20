//! Always on Top: Win + Ctrl + T pins the foreground window, like the PowerToys module.
//!
//! Two processes from one exe: the tray process (default, pure Win32: hotkey, tray, pinning)
//! and the settings process (`--settings`, WinUI 3), which only runs while its window is open.
#![windows_subsystem = "windows"]

#[rustfmt::skip]
#[allow(clippy::all, unused)]
mod bindings;
mod autostart;
mod elevation;
mod gui;
mod install;
mod markup;
mod pin;
mod recorder;
mod runtime;
mod settings;
mod shortcut;
mod state;
mod tray;
mod ui;
mod xaml_app;

use std::ptr::null;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::{CreateMutexW, Sleep};
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn main() {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let has = |flag: &str| std::env::args().any(|a| a == flag);
    if has("--settings") {
        return gui::run();
    }
    // --takeover replaces the running tray (admin mode switch).
    let takeover = has("--takeover");
    if !takeover && install::run() {
        return;
    }
    install::cleanup();
    if takeover {
        tray::quit(false);
    }
    let background = has("--background");
    // One tray process: a second launch just opens settings.
    let Some(mutex) = lock(takeover) else {
        if !background {
            tray::spawn("--settings");
        }
        return;
    };
    if elevation::wanted() && !elevation::is_elevated() {
        unsafe { CloseHandle(mutex) };
        if elevation::run_elevated(if background { "--background" } else { "" }) {
            return;
        }
        // UAC declined: run normally.
        if lock(false).is_none() {
            return;
        }
    }
    tray::run(background);
}

/// Takes the single-instance mutex; with `wait`, retries for 5 s while the old tray exits.
fn lock(wait: bool) -> Option<HANDLE> {
    for _ in 0..if wait { 50 } else { 1 } {
        let mutex = unsafe { CreateMutexW(null(), 0, wide("Local\\AlwaysOnTopRs").as_ptr()) };
        if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            return Some(mutex);
        }
        unsafe {
            CloseHandle(mutex);
            Sleep(100);
        }
    }
    None
}
