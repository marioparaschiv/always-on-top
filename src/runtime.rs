//! Attaches the Windows App SDK runtime (which hosts WinUI 3) to this unpackaged process.
//!
//! Uses the dynamic dependency API: built into kernelbase on Windows 11, otherwise taken
//! from `Microsoft.WindowsAppRuntime.dll` inside the installed framework package.
//! Approach adapted from naui-winui3 (MIT).

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::core::{HRESULT, PCWSTR, PWSTR};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, HMODULE};
use windows_sys::Win32::Storage::Packaging::Appx::*;
use windows_sys::Win32::System::LibraryLoader::*;

use crate::wide;

/// Newest first; `CBS` lines ship with Windows itself.
const VERSIONS: [&str; 11] = ["2", "CBS.2", "1.8", "CBS.1.8", "1.7", "1.6", "CBS.1.6", "1.5", "1.4", "1.3", "CBS"];

type TryCreate = unsafe extern "system" fn(*mut c_void, PCWSTR, u64, i32, i32, PCWSTR, i32, *mut PWSTR) -> HRESULT;
type Add = unsafe extern "system" fn(PCWSTR, i32, i32, *mut *mut c_void, *mut PWSTR) -> HRESULT;

fn family(version: &str) -> Vec<u16> {
    wide(&format!("Microsoft.WindowsAppRuntime.{version}_8wekyb3d8bbwe"))
}

fn lookup(module: HMODULE, prefix: &str) -> Option<(TryCreate, Add)> {
    let proc = |name: &str| unsafe { GetProcAddress(module, format!("{prefix}{name}\0").as_ptr()) };
    let try_create = proc("TryCreatePackageDependency")?;
    let add = proc("AddPackageDependency")?;
    // SAFETY: documented signatures of the dynamic dependency API.
    type Proc = unsafe extern "system" fn() -> isize;
    unsafe { Some((std::mem::transmute::<Proc, TryCreate>(try_create), std::mem::transmute::<Proc, Add>(add))) }
}

/// Install folders of the packages in a family.
fn package_paths(family: &[u16]) -> Vec<Vec<u16>> {
    let filters = PACKAGE_FILTER_HEAD | PACKAGE_FILTER_DIRECT;
    let (mut count, mut len) = (0u32, 0u32);
    unsafe {
        let rc = FindPackagesByPackageFamily(family.as_ptr(), filters, &mut count, null_mut(), &mut len, null_mut(), null_mut());
        if rc != ERROR_INSUFFICIENT_BUFFER || count == 0 {
            return Vec::new();
        }
        let mut names = vec![null_mut(); count as usize];
        let mut buf = vec![0u16; len as usize];
        if FindPackagesByPackageFamily(family.as_ptr(), filters, &mut count, names.as_mut_ptr(), &mut len, buf.as_mut_ptr(), null_mut()) != 0 {
            return Vec::new();
        }
        names
            .into_iter()
            .filter_map(|name| {
                let mut n = 0u32;
                GetPackagePathByFullName(name, &mut n, null_mut());
                let mut path = vec![0u16; n as usize];
                (GetPackagePathByFullName(name, &mut n, path.as_mut_ptr()) == 0).then(|| {
                    path.truncate(n.saturating_sub(1) as usize);
                    path
                })
            })
            .collect()
    }
}

fn api() -> Option<(TryCreate, Add)> {
    let kernelbase = unsafe { GetModuleHandleW(wide("kernelbase.dll").as_ptr()) };
    if let Some(api) = (!kernelbase.is_null()).then(|| lookup(kernelbase, "")).flatten() {
        return Some(api);
    }
    VERSIONS.iter().flat_map(|v| package_paths(&family(v))).find_map(|mut path| {
        path.extend("\\Microsoft.WindowsAppRuntime.dll\0".encode_utf16());
        let module = unsafe { LoadLibraryExW(path.as_ptr(), null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
        (!module.is_null()).then(|| lookup(module, "Mdd")).flatten()
    })
}

/// Attaches the newest installed runtime for the life of the process.
pub fn attach() -> Result<(), String> {
    let (try_create, add) = api().ok_or("the Windows App SDK runtime is not installed")?;
    for version in VERSIONS {
        let family = family(version);
        unsafe {
            let mut id = null_mut();
            if try_create(null_mut(), family.as_ptr(), 0, PackageDependencyProcessorArchitectures_None, PackageDependencyLifetimeKind_Process, null(), CreatePackageDependencyOptions_None, &mut id) < 0 {
                continue;
            }
            let (mut context, mut full_name) = (null_mut(), null_mut());
            if add(id, 0, AddPackageDependencyOptions_None, &mut context, &mut full_name) >= 0 {
                return Ok(());
            }
        }
    }
    Err("no Windows App SDK runtime (1.3 or newer) is installed".into())
}
