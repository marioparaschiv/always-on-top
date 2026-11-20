//! Start with Windows. Normally a `Run` registry value; in admin mode a scheduled task with
//! highest privileges, since a `Run` entry can't start elevated without a UAC prompt.

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::markup::escape;
use crate::{elevation, wide};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const NAME: &str = "AlwaysOnTopRs";

fn exe() -> Option<String> {
    Some(std::env::current_exe().ok()?.display().to_string())
}

fn run_value() -> bool {
    let (k, n) = (wide(RUN_KEY), wide(NAME));
    let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
    rc == 0
}

fn set_run_value(on: bool) {
    let (k, n) = (wide(RUN_KEY), wide(NAME));
    unsafe {
        if !on {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr());
            return;
        }
        let Some(exe) = exe() else { return };
        let data = wide(&format!("\"{exe}\" --background"));
        let mut hkey = std::ptr::null_mut();
        if RegCreateKeyW(HKEY_CURRENT_USER, k.as_ptr(), &mut hkey) == 0 {
            RegSetValueExW(hkey, n.as_ptr(), 0, REG_SZ, data.as_ptr() as _, (data.len() * 2) as u32);
            RegCloseKey(hkey);
        }
    }
}

/// Runs schtasks.exe without a console window; true if it succeeded.
fn schtasks(args: &[&str]) -> bool {
    Command::new("schtasks")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Creates the logon task. Needs an elevated caller. XML rather than `/SC ONLOGON` so the task
/// runs on battery, never times out (the default limit is 72 h) and only for this user.
fn create_task() -> bool {
    let Some(exe) = exe().filter(|_| elevation::is_elevated()) else { return false };
    let var = |name| std::env::var(name).unwrap_or_default();
    let user = escape(&format!("{}\\{}", var("USERDOMAIN"), var("USERNAME")));
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{}</Command><Arguments>--background</Arguments></Exec></Actions>
</Task>"#,
        escape(&exe)
    );
    // UTF-16 LE with a BOM, as schtasks expects.
    let bytes: Vec<u8> = std::iter::once(0xFEFF).chain(xml.encode_utf16()).flat_map(u16::to_le_bytes).collect();
    let path = std::env::temp_dir().join("AlwaysOnTopRs-task.xml");
    if std::fs::write(&path, bytes).is_err() {
        return false;
    }
    let ok = schtasks(&["/Create", "/TN", NAME, "/XML", &path.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&path);
    ok
}

/// Whether either sign-in mechanism is set up.
pub fn enabled() -> bool {
    run_value() || schtasks(&["/Query", "/TN", NAME])
}

/// Turns sign-in start on or off, using the mechanism that fits the admin setting and
/// removing the other one. Without elevation the task can't be made, so the `Run` value stays.
pub fn set(on: bool) {
    let task = on && elevation::wanted() && create_task();
    set_run_value(on && !task);
    if !task {
        schtasks(&["/Delete", "/TN", NAME, "/F"]);
    }
}

/// Re-points an enabled sign-in entry at this exe and the current admin setting.
pub fn sync() {
    if enabled() {
        set(true);
    }
}
