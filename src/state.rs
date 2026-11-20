//! Pinned windows and the user's settings.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONWARNING;

use crate::{pin, settings, wide};

pub struct Pins {
    pub pinned: Vec<HWND>,
    pub border: bool,
    pub sound: bool,
    accent: u32,
}

fn play(on: bool) {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let file = wide(&format!("{root}\\Media\\Speech {}.wav", if on { "On" } else { "Off" }));
    unsafe { PlaySoundW(file.as_ptr(), std::ptr::null_mut(), SND_FILENAME | SND_ASYNC | SND_NODEFAULT) };
}

impl Pins {
    pub fn load() -> Self {
        Self {
            pinned: Vec::new(),
            border: settings::get("Border", true),
            sound: settings::get("Sound", true),
            accent: pin::accent(),
        }
    }

    fn border_color(&self) -> Option<u32> {
        self.border.then(|| pin::cref(self.accent))
    }

    fn apply_borders(&self) {
        for &h in &self.pinned {
            pin::set_border(h, self.border_color());
        }
    }

    /// Re-reads the toggles the settings process just saved.
    pub fn reload(&mut self) {
        self.border = settings::get("Border", true);
        self.sound = settings::get("Sound", true);
        self.apply_borders();
    }

    /// Re-reads the accent colour after a theme change.
    pub fn accent_changed(&mut self) {
        self.accent = pin::accent();
        self.apply_borders();
    }

    pub fn unpin(&mut self, hwnd: HWND) {
        if pin::alive(hwnd) {
            pin::set_topmost(hwnd, false);
            pin::set_border(hwnd, None);
        }
        self.pinned.retain(|&h| h != hwnd);
    }

    pub fn unpin_all(&mut self) {
        for h in self.pinned.clone() {
            self.unpin(h);
        }
    }

    /// Same rule as PowerToys: a topmost window gets unpinned, anything else pinned.
    pub fn toggle(&mut self, hwnd: HWND) {
        if !pin::pinnable(hwnd) {
            return;
        }
        let pinning = !pin::is_topmost(hwnd);
        if pinning {
            if !pin::set_topmost(hwnd, true) {
                // Elevated windows refuse us unless we run elevated too.
                unsafe { MessageBeep(MB_ICONWARNING) };
                return;
            }
            pin::set_border(hwnd, self.border_color());
            self.pinned.push(hwnd);
        } else {
            self.unpin(hwnd);
        }
        if self.sound {
            play(pinning);
        }
    }

    /// Forgets windows that closed or were made non-topmost by someone else.
    pub fn prune(&mut self) {
        self.pinned.retain(|&h| pin::alive(h) && pin::is_topmost(h));
    }
}
