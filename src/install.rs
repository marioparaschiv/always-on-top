//! Self-install: a copy run from anywhere else (e.g. Downloads) moves itself to
//! `%LOCALAPPDATA%\Programs\AlwaysOnTop`, adds Start Menu and desktop shortcuts, says so in a
//! dialog, then hands over to the installed copy.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellLinkW, SHGetKnownFolderPath, ShellLink, FOLDERID_Desktop, FOLDERID_Programs, KF_FLAG_DEFAULT};
use windows_core::{h, Interface, GUID, HSTRING};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION};

use crate::shortcut::Shortcut;
use crate::{settings, tray, wide};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const EXE: &str = "always-on-top.exe";
const LNK: &str = "Always On Top.lnk";
/// Registry flag: the desktop shortcut was made once, so a deleted one stays deleted.
const DESKTOP_DONE: &str = "DesktopShortcut";
/// Passed to the installed copy with the path of an original it couldn't move, to delete.
const CLEANUP: &str = "--cleanup";

fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Programs\\AlwaysOnTop"))
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let path = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as _));
        path
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().map(|p| p.to_string_lossy().to_lowercase());
    matches!((canon(a), canon(b)), (Ok(a), Ok(b)) if a == b)
}

fn shortcut(lnk: &Path, target: &Path) -> windows_core::Result<()> {
    // Delete first: saving over "Always on Top.lnk" would keep the old capitalization.
    let _ = std::fs::remove_file(lnk);
    let target = HSTRING::from(target.as_os_str());
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&target)?;
        link.SetIconLocation(&target, 0)?;
        link.SetDescription(h!("Pin any window above all others"))?;
        link.cast::<IPersistFile>()?.Save(&HSTRING::from(lnk.as_os_str()), true)
    }
}

/// Moves (or, across drives, copies) `exe` to `target`. True if it was moved.
fn put(exe: &Path, target: &Path) -> std::io::Result<bool> {
    // A running exe can be renamed, so this works for this process's own file.
    if std::fs::rename(exe, target).is_ok() {
        return Ok(true);
    }
    // The old installed copy may take a moment to exit and unlock.
    let mut copied = std::fs::copy(exe, target);
    for _ in 0..30 {
        if copied.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
        copied = std::fs::copy(exe, target);
    }
    copied.map(|_| false)
}

/// Installs `exe` into `dir`, replacing a running install. True if the original was moved.
fn install(exe: &Path, dir: &Path) -> std::io::Result<bool> {
    tray::quit(true);
    std::fs::create_dir_all(dir)?;
    let target = dir.join(EXE);
    let moved = put(exe, &target)?;
    std::fs::write(dir.join("version"), VERSION)?;
    unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
    if let Some(programs) = known_folder(&FOLDERID_Programs) {
        let _ = shortcut(&programs.join(LNK), &target);
    }
    if let Some(desktop) = known_folder(&FOLDERID_Desktop).filter(|_| !settings::get(DESKTOP_DONE, false)) {
        if shortcut(&desktop.join(LNK), &target).is_ok() {
            settings::set(DESKTOP_DONE, true);
        }
    }
    Ok(moved)
}

fn dialog(text: &str) {
    unsafe { MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("Always on Top").as_ptr(), MB_ICONINFORMATION) };
}

/// Installs or updates when run from outside the install dir, then starts the installed copy
/// with the same arguments. True if that copy took over and this process should exit.
/// The installed tray re-points the sign-in entry at itself (`autostart::sync`).
pub fn run() -> bool {
    let (Ok(exe), Some(dir)) = (std::env::current_exe(), dir()) else { return false };
    let target = dir.join(EXE);
    if same_file(&exe, &target) {
        return false;
    }
    let installed = std::fs::read_to_string(dir.join("version")).ok().filter(|_| target.exists());
    // Same version: this is a duplicate; leave the running install alone and delete it.
    let mut moved = false;
    if installed.as_deref() != Some(VERSION) {
        match install(&exe, &dir) {
            Ok(m) => moved = m,
            Err(_) => return false,
        }
        let keys = Shortcut::load().label();
        dialog(&match installed {
            None => format!(
                "Always on Top {VERSION} is installed in {}, with shortcuts on your desktop and in the Start menu.\n\nPress {keys} to pin the active window.",
                dir.display()
            ),
            Some(old) => format!("Always on Top was updated from {old} to {VERSION}."),
        });
    }
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if !moved {
        args.extend([CLEANUP.into(), exe.into_os_string()]);
    }
    std::process::Command::new(&target).args(args).spawn().is_ok()
}

/// In the installed copy: deletes the original named by `--cleanup`, once it has exited.
pub fn cleanup() {
    let mut args = std::env::args_os().skip_while(|a| a != CLEANUP).skip(1);
    let Some(original) = args.next() else { return };
    for _ in 0..20 {
        if std::fs::remove_file(&original).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
