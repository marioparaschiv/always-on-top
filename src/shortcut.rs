//! The activation shortcut: modifiers plus one key, stored in the registry.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

use crate::settings;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Shortcut {
    /// `MOD_WIN | MOD_CONTROL | MOD_ALT | MOD_SHIFT` bits.
    pub mods: u32,
    /// Virtual-key code of the non-modifier key, 0 if none yet.
    pub vk: u32,
}

const MODS: [(u32, &str); 4] = [(MOD_WIN, "Win"), (MOD_CONTROL, "Ctrl"), (MOD_ALT, "Alt"), (MOD_SHIFT, "Shift")];

/// The modifier bit for a virtual key, if it is a modifier.
pub fn modifier(vk: u32) -> Option<u32> {
    match vk as u16 {
        VK_LWIN | VK_RWIN => Some(MOD_WIN),
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => Some(MOD_CONTROL),
        VK_MENU | VK_LMENU | VK_RMENU => Some(MOD_ALT),
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some(MOD_SHIFT),
        _ => None,
    }
}

fn key_name(vk: u32) -> String {
    match vk as u16 {
        k @ (0x30..=0x39 | 0x41..=0x5A) => char::from(k as u8).to_string(),
        k @ VK_F1..=VK_F24 => format!("F{}", k - VK_F1 + 1),
        k => {
            // Extended keys need bit 24 or they read as their numpad twins.
            let extended = matches!(
                k,
                VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN | VK_PRIOR | VK_NEXT | VK_END | VK_HOME | VK_INSERT | VK_DELETE | VK_DIVIDE | VK_NUMLOCK
            );
            let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) };
            let lparam = ((scan << 16) | (u32::from(extended) << 24)) as i32;
            let mut buf = [0u16; 64];
            let n = unsafe { GetKeyNameTextW(lparam, buf.as_mut_ptr(), buf.len() as i32) };
            if n > 0 {
                String::from_utf16_lossy(&buf[..n as usize])
            } else {
                format!("Key {vk}")
            }
        }
    }
}

impl Shortcut {
    pub const DEFAULT: Self = Self { mods: MOD_WIN | MOD_CONTROL, vk: b'T' as u32 };

    pub fn load() -> Self {
        match (settings::get_u32("HotkeyMods"), settings::get_u32("HotkeyKey")) {
            (Some(mods), Some(vk)) if (Self { mods, vk }).is_valid() => Self { mods, vk },
            _ => Self::DEFAULT,
        }
    }

    pub fn save(self) {
        settings::set_u32("HotkeyMods", self.mods);
        settings::set_u32("HotkeyKey", self.vk);
    }

    /// Like PowerToys: Win, Ctrl or Alt, plus one other key.
    pub fn is_valid(self) -> bool {
        self.mods & (MOD_WIN | MOD_CONTROL | MOD_ALT) != 0 && self.vk != 0
    }

    pub fn keys(self) -> Vec<String> {
        let mods = MODS.iter().filter(|(bit, _)| self.mods & bit != 0).map(|(_, name)| name.to_string());
        mods.chain((self.vk != 0).then(|| key_name(self.vk))).collect()
    }

    pub fn label(self) -> String {
        self.keys().join(" + ")
    }
}
