//! Opt-in "Run as administrator", as in PowerToys: an elevated tray can pin elevated windows,
//! which UIPI blocks for a normal process.

use std::ops::RangeInclusive;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{ChangeWindowMessageFilterEx, MSGFLT_ALLOW, SW_SHOWNORMAL};

use crate::{settings, tray, wide};

const SETTING: &str = "RunElevated";
/// Arguments for a tray that replaces the running one.
const TAKEOVER: &str = "--background --takeover";

/// Whether the user turned on "Run as administrator".
pub fn wanted() -> bool {
    settings::get(SETTING, false)
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut info = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = 0;
        let ok = GetTokenInformation(token, TokenElevation, &mut info as *mut _ as _, size_of::<TOKEN_ELEVATION>() as u32, &mut size);
        CloseHandle(token);
        ok != 0 && info.TokenIsElevated != 0
    }
}

/// Starts this exe elevated with `args`, through the UAC prompt. False if the user declined.
pub fn run_elevated(args: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let (verb, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(args));
    unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), file.as_ptr(), params.as_ptr(), null(), SW_SHOWNORMAL) as isize > 32 }
}

/// Lets a normal settings process send `msgs` to the (possibly elevated) host window.
pub fn allow(hwnd: HWND, msgs: RangeInclusive<u32>) {
    for msg in msgs {
        unsafe { ChangeWindowMessageFilterEx(hwnd, msg, MSGFLT_ALLOW, null_mut()) };
    }
}

/// Turns the setting on or off and replaces the tray with one at that level.
/// False (setting unchanged) if the user declined the UAC prompt.
pub fn switch(on: bool) -> bool {
    settings::set(SETTING, on);
    let restarted = if on {
        run_elevated(TAKEOVER)
    } else if !is_elevated() {
        tray::spawn(TAKEOVER);
        true
    } else {
        // An elevated process can't start a normal one directly. Explorer starts it at its own
        // level, but drops arguments, so stop the tray here instead of via --takeover.
        tray::quit(false);
        std::env::current_exe().is_ok_and(|exe| std::process::Command::new("explorer.exe").arg(exe).spawn().is_ok())
    };
    if !restarted {
        settings::set(SETTING, !on);
    }
    restarted
}
