//! Tiny HKCU-backed settings.

use windows_sys::Win32::System::Registry::*;

use crate::wide;

const KEY: &str = "Software\\AlwaysOnTopRs";

pub fn get_u32(name: &str) -> Option<u32> {
    let (k, n) = (wide(KEY), wide(name));
    let (mut data, mut size) = (0u32, 4u32);
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut u32 as _,
            &mut size,
        )
    };
    (rc == 0).then_some(data)
}

pub fn set_u32(name: &str, data: u32) {
    let (k, n) = (wide(KEY), wide(name));
    unsafe {
        let mut hkey = std::ptr::null_mut();
        if RegCreateKeyW(HKEY_CURRENT_USER, k.as_ptr(), &mut hkey) == 0 {
            RegSetValueExW(hkey, n.as_ptr(), 0, REG_DWORD, &data as *const u32 as _, 4);
            RegCloseKey(hkey);
        }
    }
}

pub fn get(name: &str, default: bool) -> bool {
    get_u32(name).map_or(default, |v| v != 0)
}

pub fn set(name: &str, value: bool) {
    set_u32(name, value as u32);
}
