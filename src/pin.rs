//! Pinning foreign windows: topmost z-order plus a DWM accent border (Windows 11).

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetColorizationColor, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Shell surfaces that must never be pinned.
const SHELL_CLASSES: [&str; 5] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
];

fn read(f: impl FnOnce(*mut u16, i32) -> i32) -> String {
    let mut buf = [0u16; 256];
    let n = f(buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}

pub fn title(hwnd: HWND) -> String {
    let t = read(|p, n| unsafe { GetWindowTextW(hwnd, p, n) });
    if t.is_empty() {
        "(untitled window)".into()
    } else {
        t
    }
}

pub fn pinnable(hwnd: HWND) -> bool {
    if hwnd.is_null() || unsafe { IsWindowVisible(hwnd) } == 0 {
        return false;
    }
    let class = read(|p, n| unsafe { GetClassNameW(hwnd, p, n) });
    !SHELL_CLASSES.contains(&class.as_str())
}

pub fn alive(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd) != 0 }
}

pub fn is_topmost(hwnd: HWND) -> bool {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 }
}

/// Fails for elevated windows when we run unelevated (UIPI).
pub fn set_topmost(hwnd: HWND, on: bool) -> bool {
    let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
    unsafe {
        SetWindowPos(hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) != 0
    }
}

/// `None` restores the default border. No-op before Windows 11.
pub fn set_border(hwnd: HWND, color: Option<u32>) {
    let c = color.unwrap_or(DWMWA_COLOR_DEFAULT);
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &c as *const u32 as _, 4);
    }
}

/// 0xRRGGBB -> COLORREF (0x00BBGGRR).
pub fn cref(rgb: u32) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

/// System accent as 0xRRGGBB.
pub fn accent() -> u32 {
    let (mut c, mut opaque) = (0u32, 0);
    if unsafe { DwmGetColorizationColor(&mut c, &mut opaque) } < 0 {
        return 0x0067C0;
    }
    c & 0xFF_FFFF
}
