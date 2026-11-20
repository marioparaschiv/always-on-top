//! The WinUI 3 `Application` subclass, built by COM aggregation.
//!
//! `Application::Start` needs an object that derives from `Application`, overrides
//! `OnLaunched` and acts as the XAML metadata provider (so the Fluent control styles in
//! `XamlControlsResources` resolve). Rust has no class inheritance, so this hand-writes
//! the outer COM object: three vtables, a ref count, and the base ("inner") object that
//! answers every interface we don't.

use std::ffi::c_void;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_core::{IInspectable, IInspectable_Vtbl, IUnknown, IUnknown_Vtbl, Interface, Result, GUID, HRESULT, HSTRING};

use crate::bindings::Microsoft::UI::Xaml::Markup::{IXamlMetadataProvider, IXamlMetadataProvider_Vtbl, XmlnsDefinition};
use crate::bindings::Microsoft::UI::Xaml::XamlTypeInfo::XamlControlsXamlMetaDataProvider;
use crate::bindings::Microsoft::UI::Xaml::{Application, IApplicationFactory, IApplicationOverrides, IApplicationOverrides_Vtbl};
use crate::bindings::Windows::UI::Xaml::Interop::TypeName;

const S_OK: HRESULT = HRESULT(0);
const E_NOINTERFACE: HRESULT = HRESULT(0x8000_4002_u32 as i32);

#[repr(C)]
struct Outer {
    /// Interface pointers: identity (IInspectable), IApplicationOverrides, IXamlMetadataProvider.
    vtables: [*const c_void; 3],
    refs: AtomicU32,
    inner: Option<IInspectable>,
    provider: XamlControlsXamlMetaDataProvider,
    launched: fn() -> Result<()>,
}

/// Recovers the object from the interface pointer living in vtable slot `S`.
unsafe fn outer<const S: usize>(this: *mut c_void) -> *mut Outer {
    unsafe { (this as *mut *const c_void).sub(S) as *mut Outer }
}

unsafe extern "system" fn query<const S: usize>(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    unsafe {
        let o = outer::<S>(this);
        let iid = *iid;
        let slot = if iid == IUnknown::IID || iid == IInspectable::IID {
            Some(0)
        } else if iid == IApplicationOverrides::IID {
            Some(1)
        } else if iid == IXamlMetadataProvider::IID {
            Some(2)
        } else {
            None
        };
        match (slot, &(*o).inner) {
            (Some(s), _) => {
                *out = &mut (*o).vtables[s] as *mut _ as *mut c_void;
                (*o).refs.fetch_add(1, Ordering::Relaxed);
                S_OK
            }
            (None, Some(inner)) => inner.query(&iid, out),
            (None, None) => {
                *out = null_mut();
                E_NOINTERFACE
            }
        }
    }
}

unsafe extern "system" fn add_ref<const S: usize>(this: *mut c_void) -> u32 {
    unsafe { (*outer::<S>(this)).refs.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn release<const S: usize>(this: *mut c_void) -> u32 {
    unsafe {
        let o = outer::<S>(this);
        let left = (*o).refs.fetch_sub(1, Ordering::Release) - 1;
        if left == 0 {
            (*o).refs.store(1, Ordering::Relaxed); // stabilise while the inner object is released
            drop(Box::from_raw(o));
        }
        left
    }
}

unsafe extern "system" fn get_iids(_: *mut c_void, count: *mut u32, iids: *mut *mut GUID) -> HRESULT {
    unsafe {
        *count = 0;
        *iids = null_mut();
    }
    S_OK
}

unsafe extern "system" fn get_class_name(_: *mut c_void, name: *mut *mut c_void) -> HRESULT {
    unsafe { *name = null_mut() };
    S_OK
}

unsafe extern "system" fn get_trust_level(_: *mut c_void, level: *mut i32) -> HRESULT {
    unsafe { *level = 0 };
    S_OK
}

const fn inspectable<const S: usize>() -> IInspectable_Vtbl {
    IInspectable_Vtbl {
        base: IUnknown_Vtbl { QueryInterface: query::<S>, AddRef: add_ref::<S>, Release: release::<S> },
        GetIids: get_iids,
        GetRuntimeClassName: get_class_name,
        GetTrustLevel: get_trust_level,
    }
}

unsafe extern "system" fn on_launched(this: *mut c_void, _args: *mut c_void) -> HRESULT {
    match unsafe { ((*outer::<1>(this)).launched)() } {
        Ok(()) => S_OK,
        Err(e) => e.code(),
    }
}

/// Writes a WinRT result into an out pointer.
unsafe fn ret<T: Interface>(result: Result<T>, out: *mut *mut c_void) -> HRESULT {
    unsafe {
        match result {
            Ok(v) => {
                *out = v.into_raw();
                S_OK
            }
            Err(e) => {
                *out = null_mut();
                e.code()
            }
        }
    }
}

unsafe extern "system" fn get_xaml_type(this: *mut c_void, ty: MaybeUninit<TypeName>, out: *mut *mut c_void) -> HRESULT {
    unsafe {
        // The caller owns the HSTRING inside `ty`.
        let ty = ManuallyDrop::new(ty.assume_init());
        ret((*outer::<2>(this)).provider.GetXamlType(&ty), out)
    }
}

unsafe extern "system" fn get_xaml_type_by_name(this: *mut c_void, name: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    unsafe {
        let name = &*(&name as *const *mut c_void as *const HSTRING);
        ret((*outer::<2>(this)).provider.GetXamlTypeByFullName(name), out)
    }
}

unsafe extern "system" fn get_xmlns(this: *mut c_void, len: *mut u32, data: *mut *mut MaybeUninit<XmlnsDefinition>) -> HRESULT {
    unsafe {
        match (*outer::<2>(this)).provider.GetXmlnsDefinitions() {
            Ok(array) => {
                let (ptr, n) = array.into_abi();
                *data = ptr as _;
                *len = n;
                S_OK
            }
            Err(e) => e.code(),
        }
    }
}

static IDENTITY: IInspectable_Vtbl = inspectable::<0>();
static OVERRIDES: IApplicationOverrides_Vtbl = IApplicationOverrides_Vtbl { base__: inspectable::<1>(), OnLaunched: on_launched };
static METADATA: IXamlMetadataProvider_Vtbl = IXamlMetadataProvider_Vtbl {
    base__: inspectable::<2>(),
    GetXamlType: get_xaml_type,
    GetXamlTypeByFullName: get_xaml_type_by_name,
    GetXmlnsDefinitions: get_xmlns,
};

/// Creates the app object; `launched` runs from `OnLaunched`. Call inside `Application::Start`.
pub fn create(launched: fn() -> Result<()>) -> Result<Application> {
    XamlControlsXamlMetaDataProvider::Initialize()?;
    let outer = Box::into_raw(Box::new(Outer {
        vtables: [
            &IDENTITY as *const _ as _,
            &OVERRIDES as *const _ as _,
            &METADATA as *const _ as _,
        ],
        refs: AtomicU32::new(1),
        inner: None,
        provider: XamlControlsXamlMetaDataProvider::new()?,
        launched,
    }));
    let factory = windows_core::factory::<Application, IApplicationFactory>()?;
    let (mut inner, mut app) = (null_mut(), null_mut());
    unsafe {
        (factory.vtable().CreateInstance)(factory.as_raw(), outer as *mut c_void, &mut inner, &mut app).ok()?;
        (*outer).inner = Some(IInspectable::from_raw(inner));
        // Our initial reference on `outer` is intentionally kept: the app lives until exit.
        Ok(Application::from_raw(app))
    }
}
